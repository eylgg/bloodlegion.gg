use sqlx::{PgConnection, PgPool};

use super::{Character, Note, NoteListing};
use crate::users::UserId;

pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<Character>> {
    sqlx::query_as!(
        Character,
        r#"
        SELECT c.id, c.user_id AS "user_id: UserId", u.username AS "username?", c.first_name,
               c.last_name, c.class, c.is_main, c.created_at, c.updated_at
        FROM characters c
        LEFT JOIN users u ON u.id = c.user_id
        ORDER BY c.name_normalized
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn find(pool: &PgPool, id: i64) -> sqlx::Result<Option<Character>> {
    sqlx::query_as!(
        Character,
        r#"
        SELECT c.id, c.user_id AS "user_id: UserId", u.username AS "username?", c.first_name,
               c.last_name, c.class, c.is_main, c.created_at, c.updated_at
        FROM characters c
        LEFT JOIN users u ON u.id = c.user_id
        WHERE c.id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
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

pub async fn insert(
    conn: &mut PgConnection,
    user_id: Option<UserId>,
    first_name: &str,
    last_name: &str,
    class: &str,
    is_main: bool,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO characters (user_id, first_name, last_name, class, is_main)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
        user_id.map(|id| id.0),
        first_name,
        last_name,
        class,
        is_main,
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

pub async fn update(
    conn: &mut PgConnection,
    id: i64,
    user_id: Option<UserId>,
    first_name: &str,
    last_name: &str,
    class: &str,
    is_main: bool,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE characters
        SET user_id = $2, first_name = $3, last_name = $4, class = $5, is_main = $6
        WHERE id = $1
        "#,
        id,
        user_id.map(|id| id.0),
        first_name,
        last_name,
        class,
        is_main,
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
