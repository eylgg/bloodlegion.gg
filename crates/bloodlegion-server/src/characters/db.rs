use sqlx::{PgConnection, PgPool};

use sqlx::types::Json;
use time::OffsetDateTime;

use super::{Character, CharacterSpec, Note, NoteListing, Sex, Valid, order_specs};
use crate::users::UserId;

pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<Character>> {
    sqlx::query_as!(
        Character,
        r#"
        SELECT c.id AS "id!", c.user_id AS "user_id: UserId", u.username AS "username?",
               c.first_name AS "first_name!", c.last_name AS "last_name!", c.class AS "class!",
               c.race, c.sex AS "sex!: Sex", c.level AS "level!", c.reached_60_at,
               c.is_main AS "is_main!",
               (SELECT COALESCE(
                    jsonb_agg(jsonb_build_object(
                        'spec', s.spec, 'talents', s.talents, 'is_main', s.is_main
                    )),
                    '[]'::jsonb
                ) FROM character_specs s WHERE s.character_id = c.id)
                   AS "specs!: Json<Vec<CharacterSpec>>",
               c.created_at AS "created_at!",
               c.updated_at AS "updated_at!"
        FROM characters c
        LEFT JOIN users u ON u.id = c.user_id
        ORDER BY c.name_normalized
        "#,
    )
    .fetch_all(pool)
    .await
    .map(|characters| characters.into_iter().map(ordered).collect())
}

/// The character with its specs in the order characters show them.
fn ordered(mut character: Character) -> Character {
    order_specs(&character.class, &mut character.specs.0);
    character
}

pub async fn find(pool: &PgPool, id: i64) -> sqlx::Result<Option<Character>> {
    sqlx::query_as!(
        Character,
        r#"
        SELECT c.id AS "id!", c.user_id AS "user_id: UserId", u.username AS "username?",
               c.first_name AS "first_name!", c.last_name AS "last_name!", c.class AS "class!",
               c.race, c.sex AS "sex!: Sex", c.level AS "level!", c.reached_60_at,
               c.is_main AS "is_main!",
               (SELECT COALESCE(
                    jsonb_agg(jsonb_build_object(
                        'spec', s.spec, 'talents', s.talents, 'is_main', s.is_main
                    )),
                    '[]'::jsonb
                ) FROM character_specs s WHERE s.character_id = c.id)
                   AS "specs!: Json<Vec<CharacterSpec>>",
               c.created_at AS "created_at!",
               c.updated_at AS "updated_at!"
        FROM characters c
        LEFT JOIN users u ON u.id = c.user_id
        WHERE c.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
    .map(|character| character.map(ordered))
}

pub async fn count_owned(conn: &mut PgConnection, user_id: UserId) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM characters WHERE user_id = $1"#,
        user_id.0,
    )
    .fetch_one(&mut *conn)
    .await
}

/// Demotes the member's main, other than `keep`.
pub async fn clear_main(
    conn: &mut PgConnection,
    user_id: UserId,
    keep: Option<i64>,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE characters SET is_main = false
        WHERE user_id = $1 AND is_main AND id IS DISTINCT FROM $2
        "#,
        user_id.0,
        keep,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Inserts `character`, the main when `is_main` (the caller decides: a member's first is).
pub(super) async fn insert(
    conn: &mut PgConnection,
    character: &Valid,
    is_main: bool,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO characters (
            user_id, first_name, last_name, class, race, sex, is_main, level, reached_60_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, COALESCE($8::smallint, 1),
            CASE WHEN $8::smallint = 60 THEN COALESCE($9, now()) END
        )
        RETURNING id
        "#,
        character.user_id.map(|id| id.0),
        character.first_name,
        character.last_name,
        character.class,
        character.race,
        character.sex as Sex,
        is_main,
        character.level,
        character.reached_60_at,
    )
    .fetch_one(&mut *conn)
    .await
}

/// The character's owner, locking its row for the change that follows: `None` when there is no
/// such character, `Some(None)` when it is unclaimed.
pub async fn lock_owner(conn: &mut PgConnection, id: i64) -> sqlx::Result<Option<Option<UserId>>> {
    sqlx::query_scalar!(
        r#"SELECT user_id AS "user_id: UserId" FROM characters WHERE id = $1 FOR UPDATE"#,
        id,
    )
    .fetch_optional(&mut *conn)
    .await
}

pub(super) async fn update(
    conn: &mut PgConnection,
    id: i64,
    character: &Valid,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE characters
        SET user_id = $2, first_name = $3, last_name = $4, class = $5, race = $6, sex = $7,
            is_main = $8, level = COALESCE($9::smallint, level),
            reached_60_at = CASE WHEN COALESCE($9::smallint, level) = 60
                                 THEN COALESCE($10, reached_60_at, now()) END
        WHERE id = $1
        "#,
        id,
        character.user_id.map(|id| id.0),
        character.first_name,
        character.last_name,
        character.class,
        character.race,
        character.sex as Sex,
        character.is_main,
        character.level,
        character.reached_60_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A character's level and time at 60, before and after a roster raised them.
pub struct LevelChange {
    pub character_id: i64,
    pub old_level: i16,
    pub old_reached_60_at: Option<OffsetDateTime>,
    pub new_level: i16,
    pub new_reached_60_at: Option<OffsetDateTime>,
}

/// Raises the level of the character named `name` (lowercase: `first last`, or the two run
/// together) to `level` if it is lower, and at 60 keeps the earlier of when it reached it and
/// `reached_60_at` (or now). Each character of that name, as it was and is now: none when there is
/// no such character, and the same before and after when nothing changed.
pub async fn raise_level(
    conn: &mut PgConnection,
    name: &str,
    level: i16,
    reached_60_at: Option<OffsetDateTime>,
) -> sqlx::Result<Vec<LevelChange>> {
    sqlx::query_as!(
        LevelChange,
        r#"
        WITH old AS (
            SELECT id, level, reached_60_at
            FROM characters
            WHERE name_normalized = $1 OR replace(name_normalized, ' ', '') = $1
            FOR UPDATE
        ), new AS (
            UPDATE characters c
            SET level = GREATEST(c.level, $2),
                reached_60_at = CASE WHEN GREATEST(c.level, $2) = 60
                                     THEN LEAST(c.reached_60_at, COALESCE($3, now())) END
            FROM old
            WHERE c.id = old.id
            RETURNING c.id, c.level, c.reached_60_at
        )
        SELECT old.id AS "character_id!", old.level AS "old_level!",
               old.reached_60_at AS old_reached_60_at,
               new.level AS "new_level!", new.reached_60_at AS new_reached_60_at
        FROM old
        JOIN new ON new.id = old.id
        "#,
        name,
        level,
        reached_60_at,
    )
    .fetch_all(&mut *conn)
    .await
}

/// Records an upload and the characters it changed; its id.
pub async fn record_upload(
    conn: &mut PgConnection,
    user_id: UserId,
    members: i32,
    matched: i32,
    unmatched: serde_json::Value,
    changes: &[LevelChange],
) -> sqlx::Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO roster_uploads (user_id, members, matched, unmatched)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        user_id as UserId,
        members,
        matched,
        unmatched,
    )
    .fetch_one(&mut *conn)
    .await?;
    for change in changes {
        sqlx::query!(
            r#"
            INSERT INTO roster_changes (upload_id, character_id, old_level, old_reached_60_at,
                                        new_level, new_reached_60_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
            id,
            change.character_id,
            change.old_level,
            change.old_reached_60_at,
            change.new_level,
            change.new_reached_60_at,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(id)
}

pub struct UploadRow {
    pub id: i64,
    pub username: Option<String>,
    pub created_at: OffsetDateTime,
    pub members: i32,
    pub matched: i32,
    pub unmatched: serde_json::Value,
    pub undone_at: Option<OffsetDateTime>,
    pub undone_by: Option<String>,
}

/// The latest `limit` uploads, latest first.
pub async fn uploads(pool: &PgPool, limit: i64) -> sqlx::Result<Vec<UploadRow>> {
    sqlx::query_as!(
        UploadRow,
        r#"
        SELECT r.id, u.username AS "username?", r.created_at, r.members, r.matched, r.unmatched,
               r.undone_at, b.username AS "undone_by?"
        FROM roster_uploads r
        LEFT JOIN users u ON u.id = r.user_id
        LEFT JOIN users b ON b.id = r.undone_by
        ORDER BY r.created_at DESC, r.id DESC
        LIMIT $1
        "#,
        limit,
    )
    .fetch_all(pool)
    .await
}

pub struct ChangeRow {
    pub upload_id: i64,
    pub character_id: i64,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    pub old_level: i16,
    pub old_reached_60_at: Option<OffsetDateTime>,
    pub new_level: i16,
    pub new_reached_60_at: Option<OffsetDateTime>,
    /// Whether the character is still as the upload left it, so undoing it would put it back.
    pub current: bool,
}

/// The changes of the uploads `ids`, by character name.
pub async fn changes(pool: &PgPool, ids: &[i64]) -> sqlx::Result<Vec<ChangeRow>> {
    sqlx::query_as!(
        ChangeRow,
        r#"
        SELECT rc.upload_id, rc.character_id, c.first_name, c.last_name, c.class,
               rc.old_level, rc.old_reached_60_at, rc.new_level, rc.new_reached_60_at,
               (c.level = rc.new_level
                AND c.reached_60_at IS NOT DISTINCT FROM rc.new_reached_60_at) AS "current!"
        FROM roster_changes rc
        JOIN characters c ON c.id = rc.character_id
        WHERE rc.upload_id = ANY($1)
        ORDER BY c.name_normalized
        "#,
        ids,
    )
    .fetch_all(pool)
    .await
}

/// Undoes upload `id` as `by`: puts back the old level and time at 60 of each character it changed
/// that is still as it left it. How many it put back and how many it left (changed since); `None`
/// when there is no such upload, or it was undone already.
pub async fn undo_upload(
    conn: &mut PgConnection,
    id: i64,
    by: UserId,
) -> sqlx::Result<Option<(u64, u64)>> {
    let open = sqlx::query_scalar!(
        r#"
        UPDATE roster_uploads SET undone_at = now(), undone_by = $2
        WHERE id = $1 AND undone_at IS NULL
        RETURNING (SELECT count(*) FROM roster_changes WHERE upload_id = $1) AS "changes!"
        "#,
        id,
        by as UserId,
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(changes) = open else {
        return Ok(None);
    };
    let reverted = sqlx::query!(
        r#"
        UPDATE characters c
        SET level = rc.old_level, reached_60_at = rc.old_reached_60_at
        FROM roster_changes rc
        WHERE rc.upload_id = $1 AND c.id = rc.character_id
          AND c.level = rc.new_level
          AND c.reached_60_at IS NOT DISTINCT FROM rc.new_reached_60_at
        "#,
        id,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(Some((reverted, changes as u64 - reverted)))
}

/// Replaces the character's specs with `specs`.
pub async fn replace_specs(
    conn: &mut PgConnection,
    id: i64,
    specs: &[CharacterSpec],
) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM character_specs WHERE character_id = $1", id)
        .execute(&mut *conn)
        .await?;
    for spec in specs {
        sqlx::query!(
            r#"
            INSERT INTO character_specs (character_id, spec, talents, is_main)
            VALUES ($1, $2, $3, $4)
            "#,
            id,
            spec.spec,
            &spec.talents,
            spec.is_main,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Drops the character's specs when it is about to become another class.
pub async fn clear_specs_unless_class(
    conn: &mut PgConnection,
    id: i64,
    class: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM character_specs
        WHERE character_id = $1
            AND (SELECT class FROM characters WHERE id = $1) <> $2
        "#,
        id,
        class,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Whether the character attended a raid or won loot.
pub async fn has_history(conn: &mut PgConnection, id: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"
        SELECT EXISTS (SELECT 1 FROM raid_attendees WHERE character_id = $1)
            OR EXISTS (SELECT 1 FROM loot WHERE character_id = $1) AS "exists!"
        "#,
        id,
    )
    .fetch_one(&mut *conn)
    .await
}

pub async fn delete(conn: &mut PgConnection, id: i64) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM characters WHERE id = $1", id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn find_note(pool: &PgPool, character_id: i64) -> sqlx::Result<Option<Note>> {
    sqlx::query_as!(
        Note,
        "SELECT character_id, body, updated_at FROM character_notes WHERE character_id = $1",
        character_id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn upsert_note(pool: &PgPool, character_id: i64, body: &str) -> sqlx::Result<Note> {
    sqlx::query_as!(
        Note,
        r#"
        INSERT INTO character_notes (character_id, body)
        VALUES ($1, $2)
        ON CONFLICT (character_id) DO UPDATE SET body = EXCLUDED.body
        RETURNING character_id, body, updated_at
        "#,
        character_id,
        body,
    )
    .fetch_one(pool)
    .await
}

pub async fn delete_note(pool: &PgPool, character_id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        "DELETE FROM character_notes WHERE character_id = $1",
        character_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_raiding_notes(pool: &PgPool) -> sqlx::Result<Vec<NoteListing>> {
    sqlx::query_as!(
        NoteListing,
        r#"
        SELECT n.character_id, c.first_name, c.last_name, c.class, u.username, n.body,
               n.updated_at
        FROM character_notes n
        JOIN characters c ON c.id = n.character_id
        JOIN users u ON u.id = c.user_id
        WHERE u.disabled_at IS NULL
            AND u.guild_rank IN ('leader', 'officer', 'raider', 'trial')
        ORDER BY n.updated_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Inserts one of `seed`'s characters, unless the name is taken; its id when it was made.
pub async fn insert_seed(
    conn: &mut PgConnection,
    first_name: &str,
    last_name: &str,
    class: &str,
) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO characters (first_name, last_name, class)
        VALUES ($1, $2, $3)
        ON CONFLICT (name_normalized) DO NOTHING
        RETURNING id
        "#,
        first_name,
        last_name,
        class,
    )
    .fetch_optional(&mut *conn)
    .await
}

/// The ids of the unlinked characters of those (lowercase, full) names, in id order.
pub async fn find_seeded(pool: &PgPool, names: &[String]) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar!(
        r#"
        SELECT id FROM characters
        WHERE user_id IS NULL AND name_normalized = ANY($1)
        ORDER BY id
        "#,
        names,
    )
    .fetch_all(pool)
    .await
}

/// Deletes the unlinked characters of those (lowercase, full) names that never raided or won
/// loot; returns how many went and how many stayed for their history.
pub async fn delete_seeded(pool: &PgPool, names: &[String]) -> sqlx::Result<(u64, u64)> {
    let mut tx = pool.begin().await?;
    let deleted = sqlx::query!(
        r#"
        DELETE FROM characters c
        WHERE c.user_id IS NULL
            AND c.name_normalized = ANY($1)
            AND NOT EXISTS (SELECT 1 FROM raid_attendees a WHERE a.character_id = c.id)
            AND NOT EXISTS (SELECT 1 FROM loot l WHERE l.character_id = c.id)
        "#,
        names,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let kept = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!" FROM characters
        WHERE user_id IS NULL AND name_normalized = ANY($1)
        "#,
        names,
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((deleted, kept as u64))
}
