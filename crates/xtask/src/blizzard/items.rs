//! The item snapshot the server embeds (`crates/bloodlegion-server/data/game-items`), from
//! Battle.net's Game Data API. Until the API serves WoW: Forever it reads Classic Era (see
//! [`api::NAMESPACE`]).
//!
//! It lists every item of the [`QUALITIES`], fetches each one's page (its tooltip) and icon name,
//! and downloads the icons not already in the snapshot. `items.json` is one item a line, by id,
//! with the API's `key` links dropped from the tooltips (they name the game build, so every patch
//! would otherwise change every line), so a refresh's diff shows only what changed. An item that
//! fails keeps its last snapshot entry; one that fails with none fails the sync, and nothing is
//! written.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::api;

/// The qualities the snapshot holds, as the API names them: what a raid drops.
const QUALITIES: &[&str] = &["EPIC", "LEGENDARY"];

/// Item requests at once: well under the API's 100 requests a second.
const CONCURRENCY: usize = 8;

/// One item of the snapshot. The field order is the file's; the server loads each by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Item {
    id: i32,
    name: String,
    quality: String,
    item_level: i32,
    required_level: i32,
    inventory_type: String,
    slot: String,
    item_class: String,
    item_subclass: String,
    is_equippable: bool,
    icon: Option<String>,
    preview: Value,
}

fn snapshot_dir(workspace: &Path) -> PathBuf {
    workspace.join("crates/bloodlegion-server/data/game-items")
}

pub async fn sync(workspace: &Path, client: &api::Client) -> Result<()> {
    let dir = snapshot_dir(workspace);
    let icons_dir = dir.join("icons");
    std::fs::create_dir_all(&icons_dir)
        .with_context(|| format!("creating {}", icons_dir.display()))?;
    let previous: HashMap<i32, Fetched> = read(&dir.join("items.json"))?
        .into_iter()
        .map(|fetched| (fetched.0.id, fetched))
        .collect();

    let mut ids = Vec::new();
    for quality in QUALITIES {
        let listed = list(client, quality).await?;
        eprintln!("items: listed {} {}", listed.len(), quality.to_lowercase());
        ids.extend(listed);
    }

    let fetched = fetch(client, ids).await?;
    let mut items = BTreeMap::new();
    let (mut kept, mut unknown, mut failed) = (0, 0, Vec::new());
    for (id, result) in fetched {
        match result {
            Ok(Some(fetched)) => {
                items.insert(id, fetched);
            }
            Ok(None) => unknown += 1,
            Err(error) => match previous.get(&id) {
                Some(item) => {
                    eprintln!("item {id}: {error:#}; keeping its last snapshot");
                    items.insert(id, item.clone());
                    kept += 1;
                }
                None => {
                    eprintln!("item {id}: {error:#}");
                    failed.push(id);
                }
            },
        }
    }
    if !failed.is_empty() {
        bail!(
            "{} new items failed and nothing was written; run the sync again",
            failed.len()
        );
    }

    // Many items share an icon; each image is downloaded once, and kept until no item uses it.
    let mut downloaded = 0;
    let mut wanted = HashSet::new();
    for (item, url) in items.values() {
        let Some(name) = &item.icon else { continue };
        let file = icons_dir.join(format!("{name}.{}", extension(url.as_deref())));
        if wanted.insert(file.clone()) && !file.exists() {
            let Some(url) = url else {
                bail!("item {}: icon {name} is missing and has no URL", item.id);
            };
            let image = client.download(url).await?;
            std::fs::write(&file, image).with_context(|| format!("writing {}", file.display()))?;
            downloaded += 1;
        }
    }
    let mut removed = 0;
    for entry in std::fs::read_dir(&icons_dir)? {
        let path = entry?.path();
        if !wanted.contains(&path) {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
            removed += 1;
        }
    }

    let items: Vec<Item> = items.into_values().map(|(item, _)| item).collect();
    write(&dir.join("items.json"), &items)?;
    eprintln!(
        "items: wrote {} ({} unknown to the API, {} kept from the last snapshot); \
         downloaded {downloaded} icons, removed {removed}",
        items.len(),
        unknown,
        kept,
    );
    Ok(())
}

/// Every item id of `quality`, page by page through ids.
async fn list(client: &api::Client, quality: &str) -> Result<Vec<i32>> {
    let mut from_id = 1;
    let mut ids = Vec::new();
    loop {
        let page = client.search(quality, from_id).await?;
        ids.extend(page.iter().map(|summary| summary.id));
        match page.last() {
            Some(last) if page.len() >= api::SEARCH_PAGE_SIZE => from_id = last.id + 1,
            _ => return Ok(ids),
        }
    }
}

/// An item and its icon's URL (`None` for an item kept from the last snapshot).
type Fetched = (Item, Option<String>);

/// Fetches each item's page and icon, [`CONCURRENCY`] at a time. `None` is an id the API does
/// not know.
async fn fetch(client: &api::Client, ids: Vec<i32>) -> Result<Vec<(i32, Result<Option<Fetched>>)>> {
    let total = ids.len();
    let permits = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    for id in ids {
        let permit = permits.clone().acquire_owned().await?;
        let client = client.clone();
        tasks.spawn(async move {
            let result = fetch_one(&client, id).await;
            drop(permit);
            (id, result)
        });
    }
    let mut results = Vec::with_capacity(total);
    while let Some(joined) = tasks.join_next().await {
        results.push(joined.context("an item task panicked")?);
        if results.len() % 500 == 0 {
            eprintln!("items: fetched {}/{total}", results.len());
        }
    }
    Ok(results)
}

async fn fetch_one(client: &api::Client, id: i32) -> Result<Option<Fetched>> {
    let Some(detail) = client.item(id).await? else {
        return Ok(None);
    };
    let icon = client.icon(id).await?;
    let summary = detail.summary;
    let item = Item {
        id: summary.id,
        name: summary.name,
        quality: summary.quality,
        item_level: summary.item_level,
        required_level: summary.required_level,
        inventory_type: summary.inventory_type,
        slot: summary.slot,
        item_class: summary.item_class,
        item_subclass: summary.item_subclass,
        is_equippable: summary.is_equippable,
        icon: icon.as_ref().map(|icon| icon.name.clone()),
        preview: canonical(detail.preview),
    };
    Ok(Some((item, icon.map(|icon| icon.url))))
}

/// The icon file's extension, from its URL (`jpg` when there is none to go by).
fn extension(url: Option<&str>) -> &str {
    url.and_then(|url| url.rsplit_once('.'))
        .map(|(_, ext)| ext)
        .filter(|ext| matches!(*ext, "jpg" | "png"))
        .unwrap_or("jpg")
}

/// A tooltip without the API's `key` links (`{"href": ...}`), its objects' keys sorted, so the
/// same tooltip is always the same line.
pub(super) fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map
                .into_iter()
                .filter(|(key, value)| !(key == "key" && is_link(value)))
                .collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
        value => value,
    }
}

fn is_link(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|map| map.len() == 1 && map.contains_key("href"))
}

/// The snapshot's items, or none when there is no snapshot yet.
fn read(path: &Path) -> Result<Vec<Fetched>> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let items: Vec<Item> = serde_json::from_str(&text)
                .with_context(|| format!("reading {}", path.display()))?;
            Ok(items.into_iter().map(|item| (item, None)).collect())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

/// `items` as a JSON array, one item a line.
fn format(items: &[Item]) -> Result<String> {
    let mut out = String::from("[\n");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        out.push_str(&serde_json::to_string(item)?);
    }
    out.push_str("\n]\n");
    Ok(out)
}

fn write(path: &Path, items: &[Item]) -> Result<()> {
    std::fs::write(path, format(items)?).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltips_drop_links_and_sort_keys() {
        let preview = serde_json::json!({
            "name": "Arcanist Boots",
            "item": {"id": 16800, "key": {"href": "https://us.api.blizzard.com/data/wow/item/16800"}},
            "armor": {"value": 70},
            "key": "not a link, kept",
        });
        assert_eq!(
            serde_json::to_string(&canonical(preview)).unwrap(),
            r#"{"armor":{"value":70},"item":{"id":16800},"key":"not a link, kept","name":"Arcanist Boots"}"#
        );
    }

    #[test]
    fn the_snapshot_is_in_the_form_a_sync_writes() {
        let path = snapshot_dir(&crate::workspace_dir()).join("items.json");
        let text = std::fs::read_to_string(&path).unwrap();
        let items: Vec<Item> = read(&path)
            .unwrap()
            .into_iter()
            .map(|(item, _)| item)
            .collect();
        assert!(items.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert!(
            items
                .iter()
                .all(|item| canonical(item.preview.clone()) == item.preview)
        );
        assert_eq!(format(&items).unwrap(), text);
    }
}
