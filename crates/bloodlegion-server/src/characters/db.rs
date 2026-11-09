use sqlx::{PgConnection, PgPool};

use sqlx::types::Json;
use time::OffsetDateTime;

use super::{Character, CharacterSpec, Note, NoteListing, Sex, Valid, order_specs};
use crate::users::UserId;
use crate::wow::Ruleset;

pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<Character>> {
    sqlx::query_as!(
        Character,
        r#"
        SELECT c.id AS "id!", c.user_id AS "user_id: UserId", u.username AS "username?",
               c.first_name AS "first_name!", c.last_name AS "last_name!", c.class AS "class!",
               c.race, c.sex AS "sex: Sex", c.ruleset AS "ruleset!: Ruleset", c.level AS "level!", c.reached_60_at,
               c.is_main AS "is_main!", c.in_guild AS "in_guild!",
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
        WHERE NOT c.is_hidden
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

/// The id of the character named `name` (`first last`), whatever its capitalization.
pub async fn find_id_by_name(pool: &PgPool, name: &str) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar!(
        "SELECT id FROM characters WHERE name_normalized = lower($1) AND NOT is_hidden",
        name,
    )
    .fetch_optional(pool)
    .await
}

pub async fn find(pool: &PgPool, id: i64) -> sqlx::Result<Option<Character>> {
    sqlx::query_as!(
        Character,
        r#"
        SELECT c.id AS "id!", c.user_id AS "user_id: UserId", u.username AS "username?",
               c.first_name AS "first_name!", c.last_name AS "last_name!", c.class AS "class!",
               c.race, c.sex AS "sex: Sex", c.ruleset AS "ruleset!: Ruleset", c.level AS "level!", c.reached_60_at,
               c.is_main AS "is_main!", c.in_guild AS "in_guild!",
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
        WHERE c.id = $1 AND NOT c.is_hidden
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
    .map(|character| character.map(ordered))
}

pub async fn count_owned(conn: &mut PgConnection, user_id: UserId) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM characters WHERE user_id = $1 AND NOT is_hidden"#,
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
            user_id, first_name, last_name, class, race, sex, is_main, level, reached_60_at,
            ruleset
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, COALESCE($8::smallint, 1),
            CASE WHEN $8::smallint = 60 THEN COALESCE($9, now()) END,
            COALESCE($10::text, (SELECT ruleset FROM guild_settings))
        )
        RETURNING id
        "#,
        character.user_id.map(|id| id.0),
        character.first_name,
        character.last_name,
        character.class,
        character.race,
        character.sex as Option<Sex>,
        is_main,
        character.level,
        character.reached_60_at,
        character.ruleset as Option<Ruleset>,
    )
    .fetch_one(&mut *conn)
    .await
}

/// The character's owner, locking its row for the change that follows: `None` when there is no
/// such character, `Some(None)` when it is unclaimed.
pub async fn lock_owner(conn: &mut PgConnection, id: i64) -> sqlx::Result<Option<Option<UserId>>> {
    sqlx::query_scalar!(
        r#"SELECT user_id AS "user_id: UserId" FROM characters
           WHERE id = $1 AND NOT is_hidden FOR UPDATE"#,
        id,
    )
    .fetch_optional(&mut *conn)
    .await
}

/// The hidden character named `name` (`first last`, any capitalization) and its owner, locked for
/// bringing it out.
pub async fn lock_hidden_by_name(
    conn: &mut PgConnection,
    name: &str,
) -> sqlx::Result<Option<(i64, Option<UserId>)>> {
    sqlx::query!(
        r#"SELECT id, user_id AS "user_id: UserId" FROM characters
           WHERE name_normalized = lower($1) AND is_hidden FOR UPDATE"#,
        name,
    )
    .fetch_optional(&mut *conn)
    .await
    .map(|row| row.map(|row| (row.id, row.user_id)))
}

/// Brings a hidden character onto the site.
pub async fn reveal(conn: &mut PgConnection, id: i64) -> sqlx::Result<()> {
    sqlx::query!("UPDATE characters SET is_hidden = false WHERE id = $1", id)
        .execute(&mut *conn)
        .await?;
    Ok(())
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
            ruleset = COALESCE($11::text, ruleset),
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
        character.sex as Option<Sex>,
        character.is_main,
        character.level,
        character.reached_60_at,
        character.ruleset as Option<Ruleset>,
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

/// Raises the level of the character Battle.net knows as `battlenet_id` (when the site has it
/// linked), else of the one named `name` (lowercase: `first last`, or the two run together), to
/// `level` if it is lower, and at 60 keeps the earlier of when it reached it and
/// `reached_60_at` (or now). Each character of that name, as it was and is now: none when there is
/// no such character, and the same before and after when nothing changed.
pub async fn raise_level(
    conn: &mut PgConnection,
    battlenet_id: Option<i64>,
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
            WHERE NOT is_hidden
                AND CASE
                    -- The character Battle.net says it is, when the site has it linked.
                    WHEN EXISTS (
                        SELECT 1 FROM characters WHERE battlenet_id = $4 AND NOT is_hidden
                    ) THEN battlenet_id = $4
                    ELSE name_normalized = $1 OR replace(name_normalized, ' ', '') = $1
                END
            FOR UPDATE
        ), new AS (
            UPDATE characters c
            SET in_guild = true, level = GREATEST(c.level, $2),
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
        battlenet_id,
    )
    .fetch_all(&mut *conn)
    .await
}

/// Records an upload, every character it named, and the ones it changed; its id.
#[allow(clippy::too_many_arguments)]
pub async fn record_upload(
    conn: &mut PgConnection,
    user_id: UserId,
    members: i32,
    matched: i32,
    unmatched: serde_json::Value,
    named: &[i64],
    changes: &[LevelChange],
    added: &[Added],
) -> sqlx::Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO roster_uploads (user_id, members, matched, unmatched, records_members)
        VALUES ($1, $2, $3, $4, true)
        RETURNING id
        "#,
        user_id as UserId,
        members,
        matched,
        unmatched,
    )
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query!(
        "INSERT INTO roster_members (upload_id, character_id)
         SELECT $1, unnest($2::bigint[]) ON CONFLICT DO NOTHING",
        id,
        named,
    )
    .execute(&mut *conn)
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
    for added in added {
        sqlx::query!(
            "INSERT INTO roster_added (upload_id, character_id, revealed) VALUES ($1, $2, $3)",
            id,
            added.character_id,
            added.revealed,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(id)
}

pub struct NotAddedRow {
    pub name: String,
    pub level: i16,
    pub class: Option<String>,
    pub seen_at: OffsetDateTime,
}

/// The members uploads (not undone) could not add and no character on the site matches by now,
/// once each by name, as the latest upload naming them saw them.
pub async fn not_added(pool: &PgPool) -> sqlx::Result<Vec<NotAddedRow>> {
    sqlx::query_as!(
        NotAddedRow,
        r#"
        SELECT DISTINCT ON (lower(m->>'name'))
               m->>'name' AS "name!", (m->>'level')::smallint AS "level!",
               m->>'class' AS class, r.created_at AS seen_at
        FROM roster_uploads r, jsonb_array_elements(r.unmatched) m
        WHERE r.undone_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM characters c
              WHERE NOT c.is_hidden
                AND (c.name_normalized = lower(m->>'name')
                     OR replace(c.name_normalized, ' ', '') = lower(m->>'name'))
          )
        ORDER BY lower(m->>'name'), r.created_at DESC, r.id DESC
        "#
    )
    .fetch_all(pool)
    .await
}

/// A character a roster upload added: made, or a hidden one brought out (`revealed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Added {
    pub character_id: i64,
    pub revealed: bool,
}

/// Adds a roster member no character on the site matched: brings out the hidden character
/// Battle.net listed under its id or name, raising its level as [`raise_level`] would, or else
/// makes one nobody plays yet, on the guild's ruleset.
pub async fn add_from_roster(
    conn: &mut PgConnection,
    battlenet_id: Option<i64>,
    first_name: &str,
    last_name: &str,
    class: &str,
    level: i16,
    reached_60_at: Option<OffsetDateTime>,
) -> sqlx::Result<Added> {
    let hidden = sqlx::query_scalar!(
        r#"SELECT id FROM characters
           WHERE is_hidden
             AND (battlenet_id = $1 OR name_normalized = lower($2 || ' ' || $3))
           ORDER BY battlenet_id = $1 DESC NULLS LAST
           LIMIT 1
           FOR UPDATE"#,
        battlenet_id,
        first_name,
        last_name,
    )
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(id) = hidden {
        sqlx::query!(
            r#"UPDATE characters
               SET is_hidden = false, in_guild = true, level = GREATEST(level, $2),
                   reached_60_at = CASE WHEN GREATEST(level, $2) = 60
                                        THEN LEAST(reached_60_at, COALESCE($3, now())) END
               WHERE id = $1"#,
            id,
            level,
            reached_60_at,
        )
        .execute(&mut *conn)
        .await?;
        return Ok(Added {
            character_id: id,
            revealed: true,
        });
    }
    let id = sqlx::query_scalar!(
        r#"INSERT INTO characters
               (first_name, last_name, class, level, reached_60_at, ruleset, battlenet_id, in_guild)
           VALUES ($1, $2, $3, $4::smallint,
                   CASE WHEN $4::smallint = 60 THEN COALESCE($5, now()) END,
                   (SELECT ruleset FROM guild_settings), $6, true)
           RETURNING id"#,
        first_name,
        last_name,
        class,
        level,
        reached_60_at,
        battlenet_id,
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(Added {
        character_id: id,
        revealed: false,
    })
}

pub struct AddedRow {
    pub upload_id: i64,
    pub character_id: i64,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    pub revealed: bool,
}

/// The characters the uploads `ids` added, by name.
pub async fn added(pool: &PgPool, ids: &[i64]) -> sqlx::Result<Vec<AddedRow>> {
    sqlx::query_as!(
        AddedRow,
        r#"
        SELECT a.upload_id, a.character_id, c.first_name, c.last_name, c.class, a.revealed
        FROM roster_added a
        JOIN characters c ON c.id = a.character_id
        WHERE a.upload_id = ANY($1)
        ORDER BY c.name_normalized
        "#,
        ids,
    )
    .fetch_all(pool)
    .await
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

/// Sets `in_guild` from the latest upload that recorded its members and is not undone: the
/// characters it named are in the guild, and no others. Leaves it alone while there is none.
/// How many characters changed.
pub async fn sync_in_guild(conn: &mut PgConnection) -> sqlx::Result<u64> {
    Ok(sqlx::query!(
        r#"
        WITH latest AS (
            SELECT id FROM roster_uploads
            WHERE records_members AND undone_at IS NULL
            ORDER BY created_at DESC, id DESC
            LIMIT 1
        ), named AS (
            SELECT c.id,
                   EXISTS (
                       SELECT 1 FROM roster_members m
                       WHERE m.upload_id = (SELECT id FROM latest) AND m.character_id = c.id
                   ) AS in_guild
            FROM characters c
            WHERE EXISTS (SELECT 1 FROM latest)
        )
        UPDATE characters c SET in_guild = named.in_guild
        FROM named
        WHERE c.id = named.id AND c.in_guild <> named.in_guild
        "#
    )
    .execute(&mut *conn)
    .await?
    .rows_affected())
}

/// The latest upload that decides `in_guild` ([`sync_in_guild`]), if any.
pub struct LatestUpload {
    pub id: i64,
    pub created_at: OffsetDateTime,
    pub username: Option<String>,
    pub members: i32,
    pub matched: i32,
}

pub async fn latest_upload(pool: &sqlx::PgPool) -> sqlx::Result<Option<LatestUpload>> {
    sqlx::query_as!(
        LatestUpload,
        r#"
        SELECT r.id, r.created_at, u.username AS "username?", r.members, r.matched
        FROM roster_uploads r
        LEFT JOIN users u ON u.id = r.user_id
        WHERE r.records_members AND r.undone_at IS NULL
        ORDER BY r.created_at DESC, r.id DESC
        LIMIT 1
        "#
    )
    .fetch_optional(pool)
    .await
}

/// Undoes upload `id` as `by`: puts back the old level and time at 60 of each character it changed
/// that is still as it left it, and removes the characters it made that are still nobody's and in
/// no raid or loot. How many it put back, how many it left (changed since), and how many it
/// removed; `None` when there is no such upload, or it was undone already.
pub async fn undo_upload(
    conn: &mut PgConnection,
    id: i64,
    by: UserId,
) -> sqlx::Result<Option<(u64, u64, u64)>> {
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
    // The characters it made, while still nobody's and in no raid, loot, or loot priority.
    let removed = sqlx::query!(
        r#"
        DELETE FROM characters c
        USING roster_added a
        WHERE a.upload_id = $1 AND a.character_id = c.id AND NOT a.revealed
          AND c.user_id IS NULL
          AND NOT EXISTS (SELECT 1 FROM raid_attendees r WHERE r.character_id = c.id)
          AND NOT EXISTS (SELECT 1 FROM loot l WHERE l.character_id = c.id)
          AND NOT EXISTS (SELECT 1 FROM loot_priority_characters p WHERE p.character_id = c.id)
        "#,
        id,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    // The guild is as the upload before saw it.
    sync_in_guild(conn).await?;
    Ok(Some((reverted, changes as u64 - reverted, removed)))
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
