//! A local mirror of the game's item database, kept current from Battle.net's Game Data API, so
//! the site shows items, icons, and tooltips from its own database instead of a third party's.
//! Until the API serves WoW: Forever it mirrors Classic Era (see [`blizzard::NAMESPACE`]).
//!
//! A sync lists every item of the [`QUALITIES`] the API's search returns, then fetches each
//! item's own page (its tooltip) and icon, newest-missing first. A refresh asks for each page with
//! `If-Modified-Since`, so an unchanged item costs a 304. The server syncs daily
//! ([`spawn`]); `bloodlegion-server items sync` runs one now. The credentials are the `battlenet`
//! sign-in provider's: the API takes a client credentials token from the same client.

pub mod api;
pub mod blizzard;
mod db;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use sqlx::PgPool;
use time::OffsetDateTime;

use crate::State;

pub use api::router;

/// The sign-in provider whose client the API takes.
pub const PROVIDER: &str = "battlenet";

/// The qualities the listing mirrors: what a raid drops.
pub const QUALITIES: &[&str] = &["RARE", "EPIC", "LEGENDARY"];

/// How long an item's page is trusted before the sync asks again.
const REFRESH_AFTER: time::Duration = time::Duration::days(7);

/// How often the server syncs, and how long after it starts the first sync runs.
const SYNC_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const FIRST_SYNC_AFTER: Duration = Duration::from_secs(60);

/// Item pages fetched at once: well under the API's 100 requests a second.
const CONCURRENCY: usize = 8;

/// The advisory lock that keeps one sync running across instances.
const SYNC_LOCK_KEY: i64 = 0x6761_6d65; // "game"

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
}

/// A mirrored item with its tooltip: the API's `preview_item`, display strings and all.
#[derive(Debug, serde::Serialize)]
pub struct GameItem {
    #[serde(flatten)]
    pub summary: GameItemSummary,
    pub preview: Option<serde_json::Value>,
    #[serde(with = "time::serde::iso8601::option")]
    pub detailed_at: Option<OffsetDateTime>,
}

/// Mirrored items whose name contains `query` (case-insensitively), names that start with it
/// first, at most `limit`.
pub async fn search(pool: &PgPool, query: &str, limit: i64) -> sqlx::Result<Vec<GameItemSummary>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    db::search(pool, query, limit.clamp(1, 50)).await
}

pub async fn find(pool: &PgPool, id: i32) -> sqlx::Result<Option<GameItem>> {
    db::find(pool, id).await
}

/// An icon's image and content type.
pub async fn icon(pool: &PgPool, name: &str) -> sqlx::Result<Option<(String, Vec<u8>)>> {
    db::icon(pool, name).await
}

/// What a sync did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Items the listing returned.
    pub listed: usize,
    /// Item pages fetched new or changed.
    pub fetched: usize,
    /// Item pages unchanged since the last fetch (a 304).
    pub unchanged: usize,
    /// Ids the API does not know.
    pub missing: usize,
    /// Icons downloaded.
    pub icons: usize,
    /// Items that failed (logged); the next sync tries them again.
    pub failed: usize,
}

#[derive(Debug)]
pub enum Outcome {
    /// No `battlenet` provider is configured, so there are no credentials.
    NoProvider,
    /// Another instance is syncing.
    Busy,
    Done(Report),
}

/// Syncs the mirror: everything (list, then refresh what is missing or stale), or only the items
/// `ids` names (fetched whether or not the listing has them, for testing and one-offs).
pub async fn sync(state: &State, ids: Option<&[i32]>) -> anyhow::Result<Outcome> {
    let mut lock = state
        .pool
        .acquire()
        .await
        .context("acquiring a connection")?;
    if !db::try_lock(&mut lock, SYNC_LOCK_KEY).await? {
        return Ok(Outcome::Busy);
    }
    let result = run(state, ids).await;
    db::unlock(&mut lock, SYNC_LOCK_KEY).await?;
    result
}

async fn run(state: &State, ids: Option<&[i32]>) -> anyhow::Result<Outcome> {
    let slug = crate::Slug::try_from(PROVIDER).context("the provider slug")?;
    let Some(token) = crate::auth::oauth2::providers::app_access_token(state, &slug).await? else {
        return Ok(Outcome::NoProvider);
    };
    let client = blizzard::Client {
        http: state.http_client.clone(),
        token,
    };
    let mut report = Report::default();

    let queue = match ids {
        Some(ids) => ids.to_vec(),
        None => {
            for quality in QUALITIES {
                report.listed += list(&state.pool, &client, quality).await?;
            }
            db::stale_ids(&state.pool, OffsetDateTime::now_utc() - REFRESH_AFTER).await?
        }
    };

    let permits = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
    let mut tasks = tokio::task::JoinSet::new();
    for id in queue {
        let permit = permits
            .clone()
            .acquire_owned()
            .await
            .context("the semaphore")?;
        let (pool, client) = (state.pool.clone(), client.clone());
        tasks.spawn(async move {
            let result = refresh(&pool, &client, id).await;
            drop(permit);
            (id, result)
        });
    }
    while let Some(joined) = tasks.join_next().await {
        let (id, result) = joined.context("an item task panicked")?;
        match result {
            Ok(Refreshed { page, icon }) => {
                match page {
                    Page::Fetched => report.fetched += 1,
                    Page::Unchanged => report.unchanged += 1,
                    Page::Missing => report.missing += 1,
                }
                report.icons += usize::from(icon);
            }
            Err(error) => {
                tracing::warn!(
                    id,
                    error = format!("{error:#}"),
                    "game items: refresh failed"
                );
                report.failed += 1;
            }
        }
    }
    Ok(Outcome::Done(report))
}

/// Lists every item of `quality`, page by page through ids, into the mirror's summaries.
async fn list(pool: &PgPool, client: &blizzard::Client, quality: &str) -> anyhow::Result<usize> {
    let mut from_id = 1;
    let mut listed = 0;
    loop {
        let page = client.search(quality, from_id).await?;
        for summary in &page {
            db::upsert_summary(pool, summary).await?;
        }
        listed += page.len();
        match page.last() {
            Some(last) if page.len() >= blizzard::SEARCH_PAGE_SIZE => from_id = last.id + 1,
            _ => return Ok(listed),
        }
    }
}

enum Page {
    Fetched,
    Unchanged,
    Missing,
}

struct Refreshed {
    page: Page,
    /// Whether an icon was downloaded.
    icon: bool,
}

/// Fetches one item's page (unless unchanged) and, when it has none yet, its icon.
async fn refresh(pool: &PgPool, client: &blizzard::Client, id: i32) -> anyhow::Result<Refreshed> {
    let cached = db::cached(pool, id).await?;
    let since = cached.as_ref().and_then(|c| c.last_modified.as_deref());
    let page = match client.item(id, since).await? {
        Some(detail) => {
            db::upsert_detail(pool, &detail).await?;
            Page::Fetched
        }
        None if cached.is_some() => {
            db::touch(pool, id).await?;
            Page::Unchanged
        }
        None => {
            return Ok(Refreshed {
                page: Page::Missing,
                icon: false,
            });
        }
    };

    // Many items share an icon; each image is downloaded once.
    let has_icon = cached.is_some_and(|c| c.icon_stored);
    if has_icon {
        return Ok(Refreshed { page, icon: false });
    }
    let Some(icon) = client.icon(id).await? else {
        return Ok(Refreshed { page, icon: false });
    };
    let mut downloaded = false;
    if !db::icon_stored(pool, &icon.name).await? {
        let (image, content_type) = client.download(&icon.url).await?;
        db::store_icon(pool, &icon.name, &content_type, &image).await?;
        downloaded = true;
    }
    db::set_icon(pool, id, &icon.name).await?;
    Ok(Refreshed {
        page,
        icon: downloaded,
    })
}

/// Syncs the mirror daily, starting a minute after the server does. Without a `battlenet`
/// provider it does nothing; another instance syncing makes this one skip its turn.
pub fn spawn(state: State) {
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_SYNC_AFTER).await;
        let mut ticker = tokio::time::interval(SYNC_INTERVAL);
        loop {
            ticker.tick().await;
            match sync(&state, None).await {
                Ok(Outcome::Done(report)) => tracing::info!(?report, "game items: synced"),
                Ok(Outcome::NoProvider) => {
                    tracing::debug!("game items: no battlenet provider, nothing to sync with")
                }
                Ok(Outcome::Busy) => tracing::debug!("game items: another instance is syncing"),
                Err(error) => {
                    tracing::warn!(error = format!("{error:#}"), "game items: sync failed")
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: i32, name: &str) -> blizzard::Summary {
        blizzard::Summary {
            id,
            name: name.into(),
            quality: "epic".into(),
            item_level: 66,
            required_level: 60,
            inventory_type: "feet".into(),
            slot: "Feet".into(),
            item_class: "Armor".into(),
            item_subclass: "Cloth".into(),
            is_equippable: true,
        }
    }

    #[sqlx::test]
    async fn the_mirror_keeps_listings_pages_and_icons(pool: PgPool) {
        db::upsert_summary(&pool, &summary(16800, "Arcanist Boots"))
            .await
            .unwrap();
        db::upsert_summary(&pool, &summary(16805, "Felheart Gloves"))
            .await
            .unwrap();
        // Both are listed but have no page yet, so both are due.
        let due = db::stale_ids(&pool, OffsetDateTime::now_utc())
            .await
            .unwrap();
        assert_eq!(due, vec![16800, 16805]);

        db::upsert_detail(
            &pool,
            &blizzard::Detail {
                summary: summary(16800, "Arcanist Boots"),
                preview: serde_json::json!({"armor": {"value": 70}}),
                last_modified: Some("Tue, 29 Sep 2026 14:37:02 GMT".into()),
            },
        )
        .await
        .unwrap();
        db::store_icon(&pool, "inv_boots_07", "image/jpeg", &[0xff, 0xd8])
            .await
            .unwrap();
        db::set_icon(&pool, 16800, "inv_boots_07").await.unwrap();

        // A page fetched just now is not due until it goes stale.
        let due = db::stale_ids(&pool, OffsetDateTime::now_utc() - REFRESH_AFTER)
            .await
            .unwrap();
        assert_eq!(due, vec![16805]);
        let cached = db::cached(&pool, 16800).await.unwrap().unwrap();
        assert_eq!(
            cached.last_modified.as_deref(),
            Some("Tue, 29 Sep 2026 14:37:02 GMT")
        );
        assert!(cached.icon_stored);

        // A later listing does not wipe the page.
        db::upsert_summary(&pool, &summary(16800, "Arcanist Boots"))
            .await
            .unwrap();
        let item = find(&pool, 16800).await.unwrap().unwrap();
        assert_eq!(item.preview.unwrap()["armor"]["value"], 70);
        assert_eq!(item.summary.icon.as_deref(), Some("inv_boots_07"));

        // Search finds by any part of the name, starts first.
        let found: Vec<_> = search(&pool, "boots", 10)
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.name)
            .collect();
        assert_eq!(found, vec!["Arcanist Boots"]);
        let found: Vec<_> = search(&pool, "fel", 10)
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(found, vec![16805]);
        // Wildcards in the query are literal.
        assert!(search(&pool, "%", 10).await.unwrap().is_empty());
        assert_eq!(
            icon(&pool, "inv_boots_07").await.unwrap().unwrap().0,
            "image/jpeg"
        );
    }
}
