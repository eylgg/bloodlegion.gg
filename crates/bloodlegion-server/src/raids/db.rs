use sqlx::{PgConnection, PgPool};
use time::PrimitiveDateTime;

use super::{
    Attendee, Boss, Drop, Item, LOOT_LIMIT, LootEntry, LootFilter, LootRow, Raid, RaidRow, calendar,
};
use crate::users::UserId;

/// The outcome of a delete that refuses while something still refers to the row.
pub enum Deleted {
    Yes,
    InUse,
    Missing,
}

/* --- bosses --- */

pub async fn list_bosses(pool: &PgPool) -> sqlx::Result<Vec<Boss>> {
    sqlx::query_as!(Boss, "SELECT id, zone, name FROM bosses ORDER BY id")
        .fetch_all(pool)
        .await
}

pub async fn find_boss(pool: &PgPool, id: i64) -> sqlx::Result<Option<Boss>> {
    sqlx::query_as!(Boss, "SELECT id, zone, name FROM bosses WHERE id = $1", id)
        .fetch_optional(pool)
        .await
}

pub async fn insert_boss(pool: &PgPool, zone: &str, name: &str) -> sqlx::Result<Boss> {
    sqlx::query_as!(
        Boss,
        "INSERT INTO bosses (zone, name) VALUES ($1, $2) RETURNING id, zone, name",
        zone,
        name,
    )
    .fetch_one(pool)
    .await
}

pub async fn rename_boss(pool: &PgPool, id: i64, name: &str) -> sqlx::Result<Option<Boss>> {
    sqlx::query_as!(
        Boss,
        "UPDATE bosses SET name = $2 WHERE id = $1 RETURNING id, zone, name",
        id,
        name,
    )
    .fetch_optional(pool)
    .await
}

pub async fn delete_boss_without_loot(pool: &PgPool, id: i64) -> sqlx::Result<Deleted> {
    let mut tx = pool.begin().await?;
    let exists = sqlx::query_scalar!("SELECT id FROM bosses WHERE id = $1 FOR UPDATE", id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
    if !exists {
        return Ok(Deleted::Missing);
    }
    let in_use = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM loot WHERE boss_id = $1) AS "exists!""#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if in_use {
        return Ok(Deleted::InUse);
    }
    sqlx::query!("DELETE FROM bosses WHERE id = $1", id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Deleted::Yes)
}

pub async fn boss_kills(pool: &PgPool, id: i64) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT count(DISTINCT raid_id) AS "count!" FROM loot WHERE boss_id = $1"#,
        id,
    )
    .fetch_one(pool)
    .await
}

pub async fn boss_drops(pool: &PgPool, id: i64) -> sqlx::Result<Vec<Drop>> {
    sqlx::query_as!(
        Drop,
        r#"
        SELECT i.id AS item_id, i.name AS item_name, i.quality AS item_quality, i.game_item_id,
               g.icon AS "item_icon?", count(*) AS "count!"
        FROM loot l
        JOIN items i ON i.id = l.item_id
        LEFT JOIN game_items g ON g.id = i.game_item_id
        WHERE l.boss_id = $1
        GROUP BY i.id, g.icon
        ORDER BY count(*) DESC, i.name_normalized
        "#,
        id,
    )
    .fetch_all(pool)
    .await
}

/* --- items --- */

pub async fn list_items(pool: &PgPool) -> sqlx::Result<Vec<Item>> {
    sqlx::query_as!(
        Item,
        r#"
        SELECT i.id, i.name, i.quality, i.game_item_id, g.icon AS "icon?",
               (SELECT count(*) FROM loot l WHERE l.item_id = i.id) AS "drops!"
        FROM items i
        LEFT JOIN game_items g ON g.id = i.game_item_id
        ORDER BY i.name_normalized
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn find_item(pool: &PgPool, id: i64) -> sqlx::Result<Option<Item>> {
    sqlx::query_as!(
        Item,
        r#"
        SELECT i.id, i.name, i.quality, i.game_item_id, g.icon AS "icon?",
               (SELECT count(*) FROM loot l WHERE l.item_id = i.id) AS "drops!"
        FROM items i
        LEFT JOIN game_items g ON g.id = i.game_item_id
        WHERE i.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// The guild's items of that name: several when the game reuses it.
pub async fn item_ids_by_name(conn: &mut PgConnection, name: &str) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar!(
        "SELECT id FROM items WHERE name_normalized = lower($1) ORDER BY id",
        name
    )
    .fetch_all(&mut *conn)
    .await
}

pub async fn item_id_by_game_item(
    conn: &mut PgConnection,
    game_item_id: i32,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar!("SELECT id FROM items WHERE game_item_id = $1", game_item_id)
        .fetch_optional(&mut *conn)
        .await
}

/// The item mirror's items of that name.
pub async fn game_item_ids_by_name(conn: &mut PgConnection, name: &str) -> sqlx::Result<Vec<i32>> {
    sqlx::query_scalar!(
        "SELECT id FROM game_items WHERE name_normalized = lower($1) ORDER BY id",
        name
    )
    .fetch_all(&mut *conn)
    .await
}

pub struct Mirrored {
    pub name: String,
    pub quality: String,
}

pub async fn mirrored_item(conn: &mut PgConnection, id: i32) -> sqlx::Result<Option<Mirrored>> {
    sqlx::query_as!(
        Mirrored,
        "SELECT name, quality FROM game_items WHERE id = $1",
        id
    )
    .fetch_optional(&mut *conn)
    .await
}

pub async fn insert_item(
    conn: &mut PgConnection,
    name: &str,
    quality: &str,
    game_item_id: Option<i32>,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        "INSERT INTO items (name, quality, game_item_id) VALUES ($1, $2, $3) RETURNING id",
        name,
        quality,
        game_item_id,
    )
    .fetch_one(&mut *conn)
    .await
}

pub async fn update_item(
    pool: &PgPool,
    id: i64,
    name: &str,
    quality: &str,
    game_item_id: Option<i32>,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE items SET name = $2, quality = $3, game_item_id = $4 WHERE id = $1",
        id,
        name,
        quality,
        game_item_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_item_without_loot(pool: &PgPool, id: i64) -> sqlx::Result<Deleted> {
    let mut tx = pool.begin().await?;
    let exists = sqlx::query_scalar!("SELECT id FROM items WHERE id = $1 FOR UPDATE", id)
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
    if !exists {
        return Ok(Deleted::Missing);
    }
    let in_use = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM loot WHERE item_id = $1) AS "exists!""#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if in_use {
        return Ok(Deleted::InUse);
    }
    sqlx::query!("DELETE FROM items WHERE id = $1", id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Deleted::Yes)
}

/* --- raids --- */

pub async fn list_raids(pool: &PgPool) -> sqlx::Result<Vec<Raid>> {
    sqlx::query_as!(
        RaidRow,
        r#"
        SELECT r.id, r.zone, r.title, r.starts_local, r.time_zone, r.starts_at,
               (SELECT count(*) FROM raid_attendees a WHERE a.raid_id = r.id) AS "attendee_count!",
               (SELECT count(*) FROM loot l WHERE l.raid_id = r.id) AS "loot_count!"
        FROM raids r
        ORDER BY r.starts_at DESC, r.id DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(Raid::from).collect())
}

pub async fn find_raid(pool: &PgPool, id: i64) -> sqlx::Result<Option<Raid>> {
    sqlx::query_as!(
        RaidRow,
        r#"
        SELECT r.id, r.zone, r.title, r.starts_local, r.time_zone, r.starts_at,
               (SELECT count(*) FROM raid_attendees a WHERE a.raid_id = r.id) AS "attendee_count!",
               (SELECT count(*) FROM loot l WHERE l.raid_id = r.id) AS "loot_count!"
        FROM raids r
        WHERE r.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
    .map(|row| row.map(Raid::from))
}

pub async fn insert_raid(
    pool: &PgPool,
    zone: &str,
    title: Option<&str>,
    starts_local: PrimitiveDateTime,
    time_zone: &str,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO raids (zone, title, starts_local, time_zone)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        zone,
        title,
        starts_local,
        time_zone,
    )
    .fetch_one(pool)
    .await
}

pub async fn update_raid(
    pool: &PgPool,
    id: i64,
    zone: &str,
    title: Option<&str>,
    starts_local: PrimitiveDateTime,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE raids SET zone = $2, title = $3, starts_local = $4 WHERE id = $1",
        id,
        zone,
        title,
        starts_local,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_raid(pool: &PgPool, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query!("DELETE FROM raids WHERE id = $1", id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Locks the raid's row for the changes that follow; `false` when there is no such raid.
pub async fn lock_raid(conn: &mut PgConnection, id: i64) -> sqlx::Result<bool> {
    Ok(
        sqlx::query_scalar!("SELECT id FROM raids WHERE id = $1 FOR UPDATE", id)
            .fetch_optional(&mut *conn)
            .await?
            .is_some(),
    )
}

pub async fn attendees(pool: &PgPool, raid_id: i64) -> sqlx::Result<Vec<Attendee>> {
    sqlx::query_as!(
        Attendee,
        r#"
        SELECT c.id AS "character_id!", c.user_id AS "user_id: UserId",
               u.username AS "username?", c.first_name AS "first_name!",
               c.last_name AS "last_name!", c.class AS "class!", c.is_main AS "is_main!",
               c.primary_spec, c.primary_talents AS "primary_talents!", c.secondary_spec,
               c.secondary_talents AS "secondary_talents!", a.group_number, a.slot,
               a.uses_secondary AS "uses_secondary!"
        FROM raid_attendees a
        JOIN characters c ON c.id = a.character_id
        LEFT JOIN users u ON u.id = c.user_id
        WHERE a.raid_id = $1
        ORDER BY c.class, c.name_normalized
        "#,
        raid_id,
    )
    .fetch_all(pool)
    .await
}

/// Benches everyone on the raid, before a new layout places them again (two people swapping slots
/// would otherwise collide on the unique slot).
pub async fn bench_all(conn: &mut PgConnection, raid_id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE raid_attendees SET group_number = NULL, slot = NULL WHERE raid_id = $1",
        raid_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Places one attendee; `false` when the character is not on the raid.
pub async fn place(
    conn: &mut PgConnection,
    raid_id: i64,
    character_id: i64,
    group_number: Option<i16>,
    slot: Option<i16>,
    uses_secondary: bool,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE raid_attendees SET group_number = $3, slot = $4, uses_secondary = $5
        WHERE raid_id = $1 AND character_id = $2
        "#,
        raid_id,
        character_id,
        group_number,
        slot,
        uses_secondary,
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Adds a character to the raid; a no-op when they are already on it.
pub async fn insert_attendee(
    conn: &mut PgConnection,
    raid_id: i64,
    character_id: i64,
) -> sqlx::Result<()> {
    // Checked first, rather than left to ON CONFLICT: the size trigger fires before the conflict
    // is detected, so re-adding someone to a full raid would be refused as overflowing it.
    let present = sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM raid_attendees WHERE raid_id = $1 AND character_id = $2
        ) AS "exists!"
        "#,
        raid_id,
        character_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    if !present {
        sqlx::query!(
            "INSERT INTO raid_attendees (raid_id, character_id) VALUES ($1, $2)",
            raid_id,
            character_id,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn delete_attendee(pool: &PgPool, raid_id: i64, character_id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM raid_attendees WHERE raid_id = $1 AND character_id = $2",
        raid_id,
        character_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn raids_attended(pool: &PgPool, character_id: i64) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM raid_attendees WHERE character_id = $1"#,
        character_id,
    )
    .fetch_one(pool)
    .await
}

/* --- loot --- */

/// Loot matching every set field of `filter`, latest raid first.
pub async fn loot(pool: &PgPool, filter: &LootFilter) -> sqlx::Result<Vec<LootEntry>> {
    let limit = filter.limit.unwrap_or(LOOT_LIMIT).clamp(1, LOOT_LIMIT);
    // A week narrows to its span; a week that does not exist (below 1) to an empty one.
    let span = filter.week.map(|number| {
        calendar::week(number).map_or((calendar::release_at(), calendar::release_at()), |week| {
            (week.starts_at, week.ends_at)
        })
    });
    sqlx::query_as!(
        LootRow,
        r#"
        SELECT l.id AS "id!", l.raid_id AS "raid_id!", r.zone AS "zone!", r.title AS raid_title,
               r.starts_local AS "raid_starts_local!", r.time_zone AS "raid_time_zone!",
               r.starts_at AS "raid_starts_at!", l.boss_id, b.name AS "boss_name?",
               l.item_id AS "item_id!", i.name AS "item_name!", i.quality AS "item_quality!",
               i.game_item_id, g.icon AS "item_icon?", l.character_id,
               c.first_name AS "first_name?", c.last_name AS "last_name?", c.class AS "class?"
        FROM loot l
        JOIN raids r ON r.id = l.raid_id
        JOIN items i ON i.id = l.item_id
        LEFT JOIN bosses b ON b.id = l.boss_id
        LEFT JOIN game_items g ON g.id = i.game_item_id
        LEFT JOIN characters c ON c.id = l.character_id
        WHERE ($1::bigint IS NULL OR l.raid_id = $1)
            AND ($2::bigint IS NULL OR l.boss_id = $2)
            AND ($3::bigint IS NULL OR l.item_id = $3)
            AND ($4::bigint IS NULL OR l.character_id = $4)
            AND ($5::text IS NULL OR r.zone = $5)
            AND ($6::text IS NULL OR c.class = $6)
            AND ($7::text IS NULL OR i.quality = $7)
            AND ($8::timestamptz IS NULL OR r.starts_at >= $8)
            AND ($9::timestamptz IS NULL OR r.starts_at < $9)
        ORDER BY r.starts_at DESC, l.id DESC
        LIMIT $10
        "#,
        filter.raid_id,
        filter.boss_id,
        filter.item_id,
        filter.character_id,
        filter.zone.as_deref(),
        filter.class.as_deref(),
        filter.quality.as_deref(),
        span.map(|(from, _)| from),
        span.map(|(_, to)| to),
        limit,
    )
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(LootEntry::from).collect())
}

pub async fn find_loot(pool: &PgPool, id: i64) -> sqlx::Result<Option<LootEntry>> {
    sqlx::query_as!(
        LootRow,
        r#"
        SELECT l.id AS "id!", l.raid_id AS "raid_id!", r.zone AS "zone!", r.title AS raid_title,
               r.starts_local AS "raid_starts_local!", r.time_zone AS "raid_time_zone!",
               r.starts_at AS "raid_starts_at!", l.boss_id, b.name AS "boss_name?",
               l.item_id AS "item_id!", i.name AS "item_name!", i.quality AS "item_quality!",
               i.game_item_id, g.icon AS "item_icon?", l.character_id,
               c.first_name AS "first_name?", c.last_name AS "last_name?", c.class AS "class?"
        FROM loot l
        JOIN raids r ON r.id = l.raid_id
        JOIN items i ON i.id = l.item_id
        LEFT JOIN bosses b ON b.id = l.boss_id
        LEFT JOIN game_items g ON g.id = i.game_item_id
        LEFT JOIN characters c ON c.id = l.character_id
        WHERE l.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
    .map(|row| row.map(LootEntry::from))
}

pub async fn insert_loot(
    conn: &mut PgConnection,
    raid_id: i64,
    boss_id: Option<i64>,
    item_id: i64,
    character_id: Option<i64>,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO loot (raid_id, boss_id, item_id, character_id)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        raid_id,
        boss_id,
        item_id,
        character_id,
    )
    .fetch_one(&mut *conn)
    .await
}

pub async fn update_loot(
    pool: &PgPool,
    id: i64,
    boss_id: Option<i64>,
    character_id: Option<i64>,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE loot SET boss_id = $2, character_id = $3 WHERE id = $1",
        id,
        boss_id,
        character_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_loot(pool: &PgPool, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query!("DELETE FROM loot WHERE id = $1", id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
