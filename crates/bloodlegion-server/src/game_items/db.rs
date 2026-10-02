use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;

use super::blizzard::{Detail, Summary};
use super::{GameItem, GameItemSummary};

pub async fn try_lock(conn: &mut PgConnection, key: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar!(r#"SELECT pg_try_advisory_lock($1) AS "locked!""#, key)
        .fetch_one(&mut *conn)
        .await
}

pub async fn unlock(conn: &mut PgConnection, key: i64) -> sqlx::Result<()> {
    sqlx::query_scalar!(r#"SELECT pg_advisory_unlock($1) AS "unlocked!""#, key)
        .fetch_one(&mut *conn)
        .await?;
    Ok(())
}

/// Records an item the listing returned, keeping any page already fetched.
pub async fn upsert_summary(pool: &PgPool, item: &Summary) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO game_items
            (id, name, quality, item_level, required_level, inventory_type, slot, item_class,
             item_subclass, is_equippable, listed_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
        ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name,
            quality = EXCLUDED.quality,
            item_level = EXCLUDED.item_level,
            required_level = EXCLUDED.required_level,
            inventory_type = EXCLUDED.inventory_type,
            slot = EXCLUDED.slot,
            item_class = EXCLUDED.item_class,
            item_subclass = EXCLUDED.item_subclass,
            is_equippable = EXCLUDED.is_equippable,
            listed_at = now()
        "#,
        item.id,
        item.name,
        item.quality,
        item.item_level,
        item.required_level,
        item.inventory_type,
        item.slot,
        item.item_class,
        item.item_subclass,
        item.is_equippable,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Records an item's page: its summary and tooltip, and when it was fetched.
pub async fn upsert_detail(pool: &PgPool, detail: &Detail) -> sqlx::Result<()> {
    let item = &detail.summary;
    sqlx::query!(
        r#"
        INSERT INTO game_items
            (id, name, quality, item_level, required_level, inventory_type, slot, item_class,
             item_subclass, is_equippable, preview, last_modified, detailed_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now())
        ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name,
            quality = EXCLUDED.quality,
            item_level = EXCLUDED.item_level,
            required_level = EXCLUDED.required_level,
            inventory_type = EXCLUDED.inventory_type,
            slot = EXCLUDED.slot,
            item_class = EXCLUDED.item_class,
            item_subclass = EXCLUDED.item_subclass,
            is_equippable = EXCLUDED.is_equippable,
            preview = EXCLUDED.preview,
            last_modified = EXCLUDED.last_modified,
            detailed_at = now()
        "#,
        item.id,
        item.name,
        item.quality,
        item.item_level,
        item.required_level,
        item.inventory_type,
        item.slot,
        item.item_class,
        item.item_subclass,
        item.is_equippable,
        detail.preview,
        detail.last_modified,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Marks an item's page as checked and unchanged.
pub async fn touch(pool: &PgPool, id: i32) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE game_items SET detailed_at = now() WHERE id = $1",
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Items whose page is missing or was fetched before `before`, the oldest first.
pub async fn stale_ids(pool: &PgPool, before: OffsetDateTime) -> sqlx::Result<Vec<i32>> {
    sqlx::query_scalar!(
        r#"
        SELECT id FROM game_items
        WHERE detailed_at IS NULL OR detailed_at < $1
        ORDER BY detailed_at NULLS FIRST, id
        "#,
        before,
    )
    .fetch_all(pool)
    .await
}

/// What a refresh needs to know of an item already in the mirror.
pub struct Cached {
    pub last_modified: Option<String>,
    pub icon_stored: bool,
}

pub async fn cached(pool: &PgPool, id: i32) -> sqlx::Result<Option<Cached>> {
    sqlx::query_as!(
        Cached,
        r#"
        SELECT g.last_modified,
               EXISTS (SELECT 1 FROM game_item_icons i WHERE i.name = g.icon) AS "icon_stored!"
        FROM game_items g
        WHERE g.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn icon_stored(pool: &PgPool, name: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM game_item_icons WHERE name = $1) AS "exists!""#,
        name,
    )
    .fetch_one(pool)
    .await
}

pub async fn store_icon(
    pool: &PgPool,
    name: &str,
    content_type: &str,
    image: &[u8],
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO game_item_icons (name, content_type, image)
        VALUES ($1, $2, $3)
        ON CONFLICT (name) DO NOTHING
        "#,
        name,
        content_type,
        image,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_icon(pool: &PgPool, id: i32, name: &str) -> sqlx::Result<()> {
    sqlx::query!("UPDATE game_items SET icon = $2 WHERE id = $1", id, name)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn icon(pool: &PgPool, name: &str) -> sqlx::Result<Option<(String, Vec<u8>)>> {
    let row = sqlx::query!(
        "SELECT content_type, image FROM game_item_icons WHERE name = $1",
        name
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| (row.content_type, row.image)))
}

/// `query` with LIKE's wildcards made literal.
fn like_literal(query: &str) -> String {
    query
        .to_lowercase()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

pub async fn search(pool: &PgPool, query: &str, limit: i64) -> sqlx::Result<Vec<GameItemSummary>> {
    sqlx::query_as!(
        GameItemSummary,
        r#"
        SELECT id, name, quality, item_level, required_level, slot, item_subclass, icon
        FROM game_items
        WHERE name_normalized LIKE '%' || $1 || '%'
        ORDER BY name_normalized LIKE $1 || '%' DESC, item_level DESC, name_normalized, id
        LIMIT $2
        "#,
        like_literal(query),
        limit,
    )
    .fetch_all(pool)
    .await
}

pub async fn find(pool: &PgPool, id: i32) -> sqlx::Result<Option<GameItem>> {
    let row = sqlx::query!(
        r#"
        SELECT id, name, quality, item_level, required_level, slot, item_subclass, icon, preview,
               detailed_at
        FROM game_items
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| GameItem {
        summary: GameItemSummary {
            id: row.id,
            name: row.name,
            quality: row.quality,
            item_level: row.item_level,
            required_level: row.required_level,
            slot: row.slot,
            item_subclass: row.item_subclass,
            icon: row.icon,
        },
        preview: row.preview,
        detailed_at: row.detailed_at,
    }))
}
