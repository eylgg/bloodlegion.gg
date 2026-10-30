use sqlx::{PgConnection, PgPool};

use super::{GameItem, GameItemSummary};

/// Takes the advisory lock `key` until the transaction ends.
pub async fn lock(conn: &mut PgConnection, key: i64) -> sqlx::Result<()> {
    sqlx::query_scalar!(
        r#"SELECT true AS "locked!" FROM (SELECT pg_advisory_xact_lock($1)) AS l"#,
        key
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(())
}

/// The hash of the snapshot last loaded.
pub async fn snapshot_sha256(conn: &mut PgConnection) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar!("SELECT sha256 FROM game_items_snapshot")
        .fetch_optional(&mut *conn)
        .await
}

pub async fn set_snapshot_sha256(conn: &mut PgConnection, sha256: &str) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO game_items_snapshot (sha256) VALUES ($1)
        ON CONFLICT (id) DO UPDATE SET sha256 = EXCLUDED.sha256, loaded_at = now()
        "#,
        sha256,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Stores the snapshot's icons, replacing any of the same name. Returns how many.
pub async fn upsert_icons(
    conn: &mut PgConnection,
    names: &[String],
    content_types: &[String],
    images: &[Vec<u8>],
) -> sqlx::Result<u64> {
    Ok(sqlx::query!(
        r#"
        INSERT INTO game_item_icons (name, content_type, image)
        SELECT * FROM UNNEST($1::text[], $2::text[], $3::bytea[])
        ON CONFLICT (name) DO UPDATE SET
            content_type = EXCLUDED.content_type,
            image = EXCLUDED.image
        "#,
        names,
        content_types,
        images,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected())
}

/// Stores the snapshot's items (its JSON array, as is), replacing any of the same id. Returns
/// their ids.
pub async fn upsert_items(conn: &mut PgConnection, items: &str) -> sqlx::Result<Vec<i32>> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO game_items
            (id, name, quality, item_level, required_level, inventory_type, slot, item_class,
             item_subclass, is_equippable, icon, preview)
        SELECT id, name, quality, item_level, required_level, inventory_type, slot, item_class,
               item_subclass, is_equippable, icon, preview
        FROM jsonb_to_recordset(($1::text)::jsonb) AS x(
            id integer, name text, quality text, item_level integer, required_level integer,
            inventory_type text, slot text, item_class text, item_subclass text,
            is_equippable boolean, icon text, preview jsonb
        )
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
            icon = EXCLUDED.icon,
            preview = EXCLUDED.preview
        RETURNING id
        "#,
        items,
    )
    .fetch_all(&mut *conn)
    .await
}

/// Drops the items not in `ids`, unless the guild has an item for them, then the icons neither in
/// `icons` (the snapshot's, kept for items to come) nor used by an item. Returns how many items
/// went.
pub async fn prune(conn: &mut PgConnection, ids: &[i32], icons: &[String]) -> sqlx::Result<u64> {
    let pruned = sqlx::query!(
        r#"
        DELETE FROM game_items g
        WHERE NOT (g.id = ANY($1))
            AND NOT EXISTS (SELECT 1 FROM items i WHERE i.game_item_id = g.id)
        "#,
        ids,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    sqlx::query!(
        r#"
        DELETE FROM game_item_icons i
        WHERE NOT (i.name = ANY($1))
            AND NOT EXISTS (SELECT 1 FROM game_items g WHERE g.icon = i.name)
        "#,
        icons,
    )
    .execute(&mut *conn)
    .await?;
    Ok(pruned)
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
               g.icon, g.preview, i.id AS "guild_item_id?",
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
    }))
}
