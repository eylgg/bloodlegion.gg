use sqlx::{PgConnection, PgPool};

use super::{Rank, Settings};
use crate::users::UserId;

pub struct MemberRow {
    pub id: UserId,
    pub username: String,
    pub guild_rank: Rank,
}

pub async fn list_members(pool: &PgPool) -> sqlx::Result<Vec<MemberRow>> {
    sqlx::query_as!(
        MemberRow,
        r#"
        SELECT id AS "id: UserId", username, guild_rank AS "guild_rank: Rank"
        FROM users
        WHERE disabled_at IS NULL
        "#,
    )
    .fetch_all(pool)
    .await
}

/// The account's rank, locking its row for the change that follows; `None` when there is no such
/// account.
pub async fn lock_rank(conn: &mut PgConnection, user_id: UserId) -> sqlx::Result<Option<Rank>> {
    sqlx::query_scalar!(
        r#"SELECT guild_rank AS "guild_rank: Rank" FROM users WHERE id = $1 FOR UPDATE"#,
        user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await
}

pub async fn set_rank(conn: &mut PgConnection, user_id: UserId, rank: Rank) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE users SET guild_rank = $2 WHERE id = $1",
        user_id.0,
        rank.slug(),
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn settings(pool: &PgPool) -> sqlx::Result<Settings> {
    sqlx::query_as!(
        Settings,
        "SELECT time_zone, default_raid_time FROM guild_settings"
    )
    .fetch_one(pool)
    .await
}

pub async fn is_known_time_zone(pool: &PgPool, name: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1) AS "exists!""#,
        name,
    )
    .fetch_one(pool)
    .await
}

pub async fn set_settings(
    pool: &PgPool,
    time_zone: &str,
    default_raid_time: time::Time,
) -> sqlx::Result<Settings> {
    sqlx::query_as!(
        Settings,
        r#"
        UPDATE guild_settings SET time_zone = $1, default_raid_time = $2
        RETURNING time_zone, default_raid_time
        "#,
        time_zone,
        default_raid_time,
    )
    .fetch_one(pool)
    .await
}
