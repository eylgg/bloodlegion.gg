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

/// Drops the items whose quality is not one of `qualities`, unless the guild has an item for
/// them, then the icons no item uses. Returns how many items went.
pub async fn prune(pool: &PgPool, qualities: &[String]) -> sqlx::Result<u64> {
    let mut tx = pool.begin().await?;
    let pruned = sqlx::query!(
        r#"
        DELETE FROM game_items g
        WHERE NOT (g.quality = ANY($1))
            AND NOT EXISTS (SELECT 1 FROM items i WHERE i.game_item_id = g.id)
        "#,
        qualities,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query!(
        r#"
        DELETE FROM game_item_icons i
        WHERE NOT EXISTS (SELECT 1 FROM game_items g WHERE g.icon = i.name)
        "#
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(pruned)
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

/// Items whose page is missing or was fetched before `before`, or whose icon is missing (a
/// failed download is tried again next sync, not a week later), the oldest first.
pub async fn stale_ids(pool: &PgPool, before: OffsetDateTime) -> sqlx::Result<Vec<i32>> {
    sqlx::query_scalar!(
        r#"
        SELECT g.id FROM game_items g
        WHERE g.detailed_at IS NULL
            OR g.detailed_at < $1
            OR NOT EXISTS (SELECT 1 FROM game_item_icons i WHERE i.name = g.icon)
        ORDER BY g.detailed_at NULLS FIRST, g.id
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

pub async fn browse(
    pool: &PgPool,
    browse: &super::Browse,
    limit: i64,
    offset: i64,
) -> sqlx::Result<super::Page> {
    let query = like_literal(browse.q.trim());
    let quality = browse.quality.as_deref().filter(|q| !q.is_empty());
    let slot = browse.slot.as_deref().filter(|s| !s.is_empty());
    let items = sqlx::query_as!(
        GameItemSummary,
        r#"
        SELECT g.id AS "id!", g.name AS "name!", g.quality AS "quality!",
               g.item_level AS "item_level!", g.required_level AS "required_level!",
               g.slot AS "slot!", g.item_subclass AS "item_subclass!", g.icon,
               i.id AS "guild_item_id?",
               (SELECT count(*) FROM loot l WHERE l.item_id = i.id) AS "drops!"
        FROM game_items g
        LEFT JOIN items i ON i.game_item_id = g.id
        WHERE g.name_normalized LIKE '%' || $1 || '%'
            AND ($2::text IS NULL OR g.quality = $2)
            AND ($3::text IS NULL OR g.slot = $3)
        ORDER BY g.name_normalized LIKE $1 || '%' DESC, g.item_level DESC, g.name_normalized, g.id
        LIMIT $4 OFFSET $5
        "#,
        query,
        quality,
        slot,
        limit,
        offset,
    )
    .fetch_all(pool)
    .await?;
    let total = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM game_items g
        WHERE g.name_normalized LIKE '%' || $1 || '%'
            AND ($2::text IS NULL OR g.quality = $2)
            AND ($3::text IS NULL OR g.slot = $3)
        "#,
        query,
        quality,
        slot,
    )
    .fetch_one(pool)
    .await?;
    Ok(super::Page { items, total })
}

pub async fn slots(pool: &PgPool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar!(
        r#"SELECT DISTINCT slot AS "slot!" FROM game_items WHERE slot <> '' ORDER BY 1"#
    )
    .fetch_all(pool)
    .await
}

pub async fn find(pool: &PgPool, id: i32) -> sqlx::Result<Option<GameItem>> {
    let row = sqlx::query!(
        r#"
        SELECT g.id, g.name, g.quality, g.item_level, g.required_level, g.slot, g.item_subclass,
               g.icon, g.preview, g.detailed_at, i.id AS "guild_item_id?",
               (SELECT count(*) FROM loot l WHERE l.item_id = i.id) AS "drops!"
        FROM game_items g
        LEFT JOIN items i ON i.game_item_id = g.id
        WHERE g.id = $1
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
            guild_item_id: row.guild_item_id,
            drops: row.drops,
        },
        preview: row.preview,
        detailed_at: row.detailed_at,
    }))
}
