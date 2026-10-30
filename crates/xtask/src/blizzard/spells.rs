//! The keybind planner's spells (`apps/web/src/lib/wow/spells.json`) and their icons
//! (`apps/web/static/wow/spells`).
//!
//! Which spells each class has, the level each is trained at, its talent tree and ranks, and the
//! tooltip lines above the description (cost, range, cast time) are the catalog's own: Blizzard's
//! API serves no class spell lists, talents, or tooltip lines, so they are edited by hand. The
//! sync refreshes what the API does serve, for each spell the API knows: its name (by its first
//! rank), its description (by its highest rank, unless the API leaves a formula unworked, `$s1`),
//! and its icon. A spell the API does not know, or that fails, keeps what the catalog has. Then it
//! downloads the icons not already vendored and removes the ones no spell uses. Until the API
//! serves WoW: Forever's namespace ([`api::NAMESPACE`]) the sync skips it.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::api;

/// Spell requests at once: well under the API's 100 requests a second.
const CONCURRENCY: usize = 8;

/// One spell of the catalog, in the file's field order (see `spells.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Spell {
    id: i32,
    name: String,
    icon: String,
    level: i32,
    tree: Option<String>,
    talent: bool,
    ranks: i32,
    max: i32,
    rank: String,
    tooltip: Vec<Value>,
    description: Vec<String>,
}

/// Each class's spells, by class slug.
type Catalog = BTreeMap<String, Vec<Spell>>;

fn catalog_path(workspace: &Path) -> PathBuf {
    workspace.join("apps/web/src/lib/wow/spells.json")
}

fn icons_dir(workspace: &Path) -> PathBuf {
    workspace.join("apps/web/static/wow/spells")
}

/// What the API says of one spell; `None` where it says nothing.
#[derive(Debug, Default)]
struct Fetched {
    name: Option<String>,
    description: Option<Vec<String>>,
    icon: Option<String>,
    /// Whether the API knows the spell at all.
    known: bool,
}

pub async fn sync(workspace: &Path, client: &api::Client) -> Result<()> {
    let path = catalog_path(workspace);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut catalog: Catalog =
        serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))?;

    // Each spell once, though a general attack (Attack, Shoot) is in every class's list.
    let mut wanted: Vec<(i32, i32)> = catalog
        .values()
        .flatten()
        .map(|spell| (spell.id, spell.max))
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let fetched = fetch(client, wanted).await?;

    let (mut unknown, mut failed, mut changed) = (0, 0, 0);
    for (&(id, _), result) in &fetched {
        match result {
            Ok(fetched) if !fetched.known => unknown += 1,
            Ok(_) => {}
            Err(error) => {
                eprintln!("spell {id}: {error:#}; keeping the catalog's");
                failed += 1;
            }
        }
    }
    for spell in catalog.values_mut().flatten() {
        let Some(Ok(fetched)) = fetched.get(&(spell.id, spell.max)) else {
            continue;
        };
        let before = spell.clone();
        if let Some(name) = &fetched.name {
            spell.name.clone_from(name);
        }
        if let Some(description) = &fetched.description {
            spell.description.clone_from(description);
        }
        if let Some(icon) = &fetched.icon {
            spell.icon.clone_from(icon);
        }
        changed += usize::from(*spell != before);
    }

    let dir = icons_dir(workspace);
    let icons: HashSet<String> = catalog
        .values()
        .flatten()
        .map(|spell| spell.icon.clone())
        .collect();
    let mut downloaded = 0;
    for name in &icons {
        let file = dir.join(format!("{name}.jpg"));
        if file.exists() {
            continue;
        }
        let Some(image) = api::render_icon(&client.http, name).await? else {
            bail!("no icon {name} on the render service; nothing was written");
        };
        std::fs::write(&file, image).with_context(|| format!("writing {}", file.display()))?;
        downloaded += 1;
    }
    let mut removed = 0;
    for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let used = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| icons.contains(stem));
        if !used {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            removed += 1;
        }
    }

    write(workspace, &catalog)?;
    eprintln!(
        "spells: {changed} of {} changed ({unknown} unknown to the API, {failed} failed); \
         downloaded {downloaded} icons, removed {removed}",
        catalog.values().map(Vec::len).sum::<usize>(),
    );
    Ok(())
}

/// Fetches each `(first rank, highest rank)`, [`CONCURRENCY`] at a time.
async fn fetch(
    client: &api::Client,
    wanted: Vec<(i32, i32)>,
) -> Result<BTreeMap<(i32, i32), Result<Fetched>>> {
    let permits = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    for ids in wanted {
        let permit = permits.clone().acquire_owned().await?;
        let client = client.clone();
        tasks.spawn(async move {
            let result = fetch_one(&client, ids).await;
            drop(permit);
            (ids, result)
        });
    }
    let mut results = BTreeMap::new();
    while let Some(joined) = tasks.join_next().await {
        let (ids, result) = joined.context("a spell task panicked")?;
        results.insert(ids, result);
    }
    Ok(results)
}

async fn fetch_one(client: &api::Client, (id, max): (i32, i32)) -> Result<Fetched> {
    let Some(first) = client.spell(id).await? else {
        return Ok(Fetched::default());
    };
    let highest = if max == id {
        Some(first.description)
    } else {
        client.spell(max).await?.map(|spell| spell.description)
    };
    let icon = client.spell_icon(id).await?;
    Ok(Fetched {
        name: Some(first.name).filter(|name| !name.is_empty()),
        description: highest.flatten().as_deref().and_then(description),
        icon: icon.map(|icon| icon.name),
        known: true,
    })
}

/// The description's paragraphs, or `None` when it has a formula the API left unworked.
fn description(text: &str) -> Option<Vec<String>> {
    if text.contains('$') {
        return None;
    }
    Some(
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Writes the catalog, then has the web app's Prettier format it, as it checks it.
fn write(workspace: &Path, catalog: &Catalog) -> Result<()> {
    let path = catalog_path(workspace);
    let mut text = serde_json::to_string_pretty(catalog)?;
    text.push('\n');
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    let path = path.to_str().context("the catalog's path")?;
    crate::run(
        workspace,
        "pnpm",
        &["-C", "apps/web", "exec", "prettier", "--write", path],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_split_into_paragraphs_unless_unworked() {
        assert_eq!(
            description("Increases armor by 50.\n\nLasts 30 min.").unwrap(),
            ["Increases armor by 50.", "Lasts 30 min."]
        );
        assert_eq!(description("Increases armor by $s1."), None);
    }

    #[test]
    fn the_catalog_reads_and_has_its_icons() {
        let workspace = crate::workspace_dir();
        let text = std::fs::read_to_string(catalog_path(&workspace)).unwrap();
        let catalog: Catalog = serde_json::from_str(&text).unwrap();
        // Every field survives a read, so a sync rewrites only what it changes.
        let reread: Value = serde_json::to_value(&catalog).unwrap();
        assert_eq!(reread, serde_json::from_str::<Value>(&text).unwrap());
        for spell in catalog.values().flatten() {
            let icon = icons_dir(&workspace).join(format!("{}.jpg", spell.icon));
            assert!(icon.exists(), "{} has no icon {}", spell.name, spell.icon);
        }
    }
}
