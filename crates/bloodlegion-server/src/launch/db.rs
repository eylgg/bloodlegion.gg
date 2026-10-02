use sqlx::{PgConnection, PgPool};

use super::{Character, RosterEntry};
use crate::users::UserId;

pub async fn list(pool: &PgPool, user_id: UserId) -> sqlx::Result<Vec<Character>> {
    sqlx::query_as!(
        Character,
        r#"
        SELECT id, name, class, specs, is_main, created_at, updated_at
        FROM launch_characters
        WHERE user_id = $1
        ORDER BY is_main DESC, created_at
        "#,
        user_id.0,
    )
    .fetch_all(pool)
    .await
}

pub async fn count(conn: &mut PgConnection, user_id: UserId) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM launch_characters WHERE user_id = $1"#,
        user_id.0,
    )
    .fetch_one(&mut *conn)
    .await
}

pub async fn clear_main(conn: &mut PgConnection, user_id: UserId) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE launch_characters SET is_main = false WHERE user_id = $1 AND is_main",
        user_id.0,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn clear_main_except(
    conn: &mut PgConnection,
    user_id: UserId,
    keep: i64,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE launch_characters SET is_main = false WHERE user_id = $1 AND is_main AND id <> $2",
        user_id.0,
        keep,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn insert(
    conn: &mut PgConnection,
    user_id: UserId,
    name: &str,
    class: &str,
    specs: &[String],
    is_main: bool,
) -> sqlx::Result<Character> {
    sqlx::query_as!(
        Character,
        r#"
        INSERT INTO launch_characters (user_id, name, class, specs, is_main)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, class, specs, is_main, created_at, updated_at
        "#,
        user_id.0,
        name,
        class,
        specs,
        is_main,
    )
    .fetch_one(&mut *conn)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn update(
    conn: &mut PgConnection,
    user_id: UserId,
    id: i64,
    name: &str,
    class: &str,
    specs: &[String],
    is_main: bool,
) -> sqlx::Result<Option<Character>> {
    sqlx::query_as!(
        Character,
        r#"
        UPDATE launch_characters
        SET name = $3, class = $4, specs = $5, is_main = $6
        WHERE id = $2 AND user_id = $1
        RETURNING id, name, class, specs, is_main, created_at, updated_at
        "#,
        user_id.0,
        id,
        name,
        class,
        specs,
        is_main,
    )
    .fetch_optional(&mut *conn)
    .await
}

pub async fn delete(pool: &PgPool, user_id: UserId, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM launch_characters WHERE id = $2 AND user_id = $1",
        user_id.0,
        id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_all(pool: &PgPool) -> sqlx::Result<Vec<RosterEntry>> {
    sqlx::query_as!(
        RosterEntry,
        r#"
        SELECT u.username, c.name, c.class, c.specs, c.is_main, c.created_at
        FROM launch_characters c
        JOIN users u ON u.id = c.user_id
        WHERE u.disabled_at IS NULL
        ORDER BY u.username_normalized, c.is_main DESC, c.created_at
        "#,
    )
    .fetch_all(pool)
    .await
}
