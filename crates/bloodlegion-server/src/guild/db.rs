use sqlx::{PgConnection, PgPool};

use super::Rank;
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
