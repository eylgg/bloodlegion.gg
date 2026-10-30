//! A local mirror of the game's item database, so the site shows items, icons, and tooltips from
//! its own database instead of a third party's. Until Battle.net's API serves WoW: Forever it
//! has no items: Classic Era's are not Forever's, so the snapshot keeps only the game's icons until
//! the real items can be had.
//!
//! The mirror is a snapshot in the repository (`data/game-items`): `cargo xtask blizzard sync`
//! fetches it from the API, and the server embeds it. [`load`] puts it in the database after every migrate
//! (the server runs one on start), when its hash differs from the one last loaded, so the site
//! works offline and every deploy has the items of its own commit.

pub mod api;
mod db;

use anyhow::Context;
use rsa::sha2::{Digest, Sha256};
use sqlx::PgPool;

pub use api::router;

/// The snapshot's items: a JSON array, one item a line, by id.
const ITEMS: &str = include_str!("../data/game-items/items.json");

/// The snapshot's icons, `(name, extension, image)`, by name (from `build.rs`).
const ICONS: &[(&str, &str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/game_item_icons.rs"));

/// A mirrored item, as a picker or a link shows it.
#[derive(Debug, serde::Serialize)]
pub struct GameItemSummary {
    pub id: i32,
    pub name: String,
    pub quality: String,
    pub item_level: i32,
    pub required_level: i32,
    pub slot: String,
    pub item_subclass: String,
    pub icon: Option<String>,
    /// The guild's item for it, once it has been won (or added by an officer).
    pub guild_item_id: Option<i64>,
    /// How many times the guild has won it.
    pub drops: i64,
}

/// A mirrored item with its tooltip: the API's `preview_item`, display strings and all.
#[derive(Debug, serde::Serialize)]
pub struct GameItem {
    #[serde(flatten)]
    pub summary: GameItemSummary,
    pub preview: Option<serde_json::Value>,
}

/// The most items one page holds.
pub const PAGE_LIMIT: i64 = 100;

/// Which mirrored items to list, one page at a time; every filter is optional.
#[derive(Debug, Default, serde::Deserialize)]
pub struct Browse {
    /// Part of the name, case-insensitively; names that start with it come first.
    #[serde(default)]
    pub q: String,
    /// `epic`.
    #[serde(default)]
    pub quality: Option<String>,
    /// The slot as the game names it, `Feet`.
    #[serde(default)]
    pub slot: Option<String>,
    #[serde(default)]
    pub offset: i64,
    /// At most [`PAGE_LIMIT`]; 50 when omitted.
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct Page {
    pub items: Vec<GameItemSummary>,
    /// How many match, over every page.
    pub total: i64,
}

/// A page of mirrored items, highest item level first (after names starting with the query).
pub async fn browse(pool: &PgPool, browse: &Browse) -> sqlx::Result<Page> {
    let limit = browse.limit.unwrap_or(50).clamp(1, PAGE_LIMIT);
    let offset = browse.offset.max(0);
    db::browse(pool, browse, limit, offset).await
}

/// The slots the mirror's items go in, for the browser's filter.
pub async fn slots(pool: &PgPool) -> sqlx::Result<Vec<String>> {
    db::slots(pool).await
}

pub async fn find(pool: &PgPool, id: i32) -> sqlx::Result<Option<GameItem>> {
    db::find(pool, id).await
}

/// An icon's image and content type.
pub async fn icon(pool: &PgPool, name: &str) -> sqlx::Result<Option<(String, Vec<u8>)>> {
    db::icon(pool, name).await
}

/// What a load did.
#[derive(Debug, PartialEq, Eq)]
pub enum Loaded {
    /// The database already has this snapshot.
    Unchanged,
    Loaded {
        items: u64,
        /// Items no longer in the snapshot dropped from the mirror (none the guild has won).
        pruned: u64,
        icons: u64,
    },
}

/// Loads the embedded snapshot into the mirror, unless it is the one last loaded.
pub async fn load(pool: &PgPool) -> anyhow::Result<Loaded> {
    load_snapshot(pool, ITEMS, ICONS).await
}

async fn load_snapshot(
    pool: &PgPool,
    items: &str,
    icons: &[(&str, &str, &[u8])],
) -> anyhow::Result<Loaded> {
    let sha256 = hash(items, icons);
    let mut tx = pool.begin().await?;
    // Instances starting together load it once: the others wait, then find it loaded.
    db::lock(&mut tx, LOAD_LOCK_KEY).await?;
    if db::snapshot_sha256(&mut tx).await?.as_deref() == Some(sha256.as_str()) {
        return Ok(Loaded::Unchanged);
    }
    let (names, content_types, images) = icons
        .iter()
        .map(|&(name, ext, image)| {
            let content_type = match ext {
                "png" => "image/png",
                _ => "image/jpeg",
            };
            (name.to_string(), content_type.to_string(), image.to_vec())
        })
        .collect::<(Vec<_>, Vec<_>, Vec<_>)>();
    let icons = db::upsert_icons(&mut tx, &names, &content_types, &images).await?;
    let ids = db::upsert_items(&mut tx, items)
        .await
        .context("loading the snapshot's items")?;
    let pruned = db::prune(&mut tx, &ids, &names).await?;
    db::set_snapshot_sha256(&mut tx, &sha256).await?;
    tx.commit().await?;
    Ok(Loaded::Loaded {
        items: ids.len() as u64,
        pruned,
        icons,
    })
}

/// The advisory lock that keeps one load running across instances.
const LOAD_LOCK_KEY: i64 = 0x6761_6d65; // "game"

/// The snapshot's SHA-256, hex: its items, then each icon's name, extension, and image.
fn hash(items: &str, icons: &[(&str, &str, &[u8])]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(items.as_bytes());
    for (name, ext, image) in icons {
        for part in [name.as_bytes(), ext.as_bytes(), image] {
            hasher.update((part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i32, name: &str, quality: &str, icon: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id, "name": name, "quality": quality, "item_level": 66, "required_level": 60,
            "inventory_type": "feet", "slot": "Feet", "item_class": "Armor",
            "item_subclass": "Cloth", "is_equippable": true, "icon": icon,
            "preview": {"armor": {"value": 70}},
        })
    }

    fn snapshot(items: &[serde_json::Value]) -> String {
        serde_json::to_string(items).unwrap()
    }

    #[sqlx::test]
    async fn the_mirror_loads_its_snapshot(pool: PgPool) {
        let boots = item(16800, "Arcanist Boots", "epic", "inv_boots_07");
        let gloves = item(16805, "Felheart Gloves", "epic", "inv_gauntlets_19");
        let band = item(17063, "Band of Accuria", "epic", "inv_jewelry_ring_15");
        let icons: &[(&str, &str, &[u8])] = &[
            ("inv_boots_07", "jpg", &[0xff, 0xd8]),
            ("inv_gauntlets_19", "jpg", &[0xff, 0xd8]),
            ("inv_jewelry_ring_15", "png", &[0x89, 0x50]),
        ];
        let first = snapshot(&[boots.clone(), gloves.clone(), band]);
        assert_eq!(
            load_snapshot(&pool, &first, icons).await.unwrap(),
            Loaded::Loaded {
                items: 3,
                pruned: 0,
                icons: 3
            }
        );
        // The same snapshot again is a no-op.
        assert_eq!(
            load_snapshot(&pool, &first, icons).await.unwrap(),
            Loaded::Unchanged
        );
        let item = find(&pool, 16800).await.unwrap().unwrap();
        assert_eq!(item.preview.unwrap()["armor"]["value"], 70);
        assert_eq!(item.summary.icon.as_deref(), Some("inv_boots_07"));
        assert_eq!(
            icon(&pool, "inv_jewelry_ring_15").await.unwrap().unwrap().0,
            "image/png"
        );

        // A later snapshot drops what it no longer has, except an item the guild has won, and the
        // icons it no longer has that nothing uses.
        sqlx::query!(
            "INSERT INTO items (name, quality, game_item_id) VALUES ('Band of Accuria', 'epic', 17063)"
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut renamed = boots;
        renamed["name"] = "Arcanist Slippers".into();
        let second = snapshot(&[renamed]);
        let spare = ("inv_misc_gem_01", "jpg", &[0xff, 0xd8][..]);
        assert_eq!(
            load_snapshot(&pool, &second, &[icons[0], spare])
                .await
                .unwrap(),
            Loaded::Loaded {
                items: 1,
                pruned: 1,
                icons: 2
            }
        );
        assert_eq!(
            find(&pool, 16800).await.unwrap().unwrap().summary.name,
            "Arcanist Slippers"
        );
        assert!(find(&pool, 16805).await.unwrap().is_none());
        assert!(find(&pool, 17063).await.unwrap().is_some());
        assert!(icon(&pool, "inv_gauntlets_19").await.unwrap().is_none());
        // The won item's icon is still in use, so it stays; so does a snapshot icon no item uses.
        assert!(icon(&pool, "inv_jewelry_ring_15").await.unwrap().is_some());
        assert!(icon(&pool, "inv_misc_gem_01").await.unwrap().is_some());

        // Browsing finds by any part of the name; unfiltered, it pages through everything.
        load_snapshot(&pool, &first, icons).await.unwrap();
        let names = |page: Page| page.items.into_iter().map(|i| i.name).collect::<Vec<_>>();
        let by = |q: &str| Browse {
            q: q.into(),
            ..Browse::default()
        };
        assert_eq!(
            names(browse(&pool, &by("boots")).await.unwrap()),
            ["Arcanist Boots"]
        );
        assert_eq!(
            names(browse(&pool, &by("fel")).await.unwrap()),
            ["Felheart Gloves"]
        );
        // Wildcards in the query are literal.
        assert_eq!(browse(&pool, &by("%")).await.unwrap().total, 0);
        let page = |offset| Browse {
            limit: Some(1),
            offset,
            ..Browse::default()
        };
        let first = browse(&pool, &page(0)).await.unwrap();
        assert_eq!((first.items.len(), first.total), (1, 3));
        let second = browse(&pool, &page(1)).await.unwrap();
        assert_ne!(first.items[0].id, second.items[0].id);
        assert_eq!(slots(&pool).await.unwrap(), ["Feet"]);
    }

    #[test]
    fn the_embedded_snapshot_parses_and_has_its_icons() {
        // No items until the mirror has WoW: Forever's; the icons stay for them.
        let items: Vec<serde_json::Value> = serde_json::from_str(ITEMS).unwrap();
        let icons: std::collections::HashSet<_> = ICONS.iter().map(|(name, ..)| *name).collect();
        assert!(!icons.is_empty());
        for item in &items {
            if let Some(icon) = item["icon"].as_str() {
                assert!(icons.contains(icon), "{} has no icon {icon}", item["id"]);
            }
        }
    }
}
