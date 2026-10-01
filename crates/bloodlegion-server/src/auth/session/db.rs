use std::net::IpAddr;

use sqlx::PgPool;
use sqlx::types::ipnet::IpNet;
use time::OffsetDateTime;

use crate::crypto::TokenHash;
use crate::users::{User, UserId};

/// One active session row for the "your sessions" listing. `is_current` is
/// computed against the caller's own session token, so the row for the current
/// device can be badged without exposing any token hash to the client.
pub struct SessionRow {
    pub id: i64,
    pub created_at: OffsetDateTime,
    pub last_used_at: OffsetDateTime,
    pub ip_address: IpNet,
    pub user_agent: Option<String>,
    pub is_current: bool,
}

/// Lists a user's live (unexpired, unrevoked) sessions, most-recently-active
/// first, marking the one whose token is `current`.
pub async fn list_sessions(
    pool: &PgPool,
    user_id: UserId,
    current: &TokenHash,
) -> sqlx::Result<Vec<SessionRow>> {
    sqlx::query_as!(
        SessionRow,
        r#"
        SELECT
            id,
            created_at,
            last_used_at,
            ip_address AS "ip_address: IpNet",
            user_agent,
            (token_hash = $2) AS "is_current!"
        FROM auth_sessions
        WHERE user_id = $1
            AND expires_at > now()
            AND revoked_at IS NULL
        ORDER BY last_used_at DESC
        "#,
        user_id.0,
        current.as_ref(),
    )
    .fetch_all(pool)
    .await
}

pub async fn find_user_by_session_token(
    pool: &PgPool,
    token: &TokenHash,
) -> sqlx::Result<Option<User>> {
    sqlx::query_as!(
        User,
        r#"
        SELECT
            users.id AS "id: UserId",
            users.username,
            users.is_superuser,
            (users.disabled_at IS NOT NULL) AS "disabled!",
            user_emails.email AS "email?",
            COALESCE(user_emails.verified_at IS NOT NULL, false) AS "is_email_verified!",
            COALESCE(user_emails.is_federated, false) AS "is_email_federated!",
            users.first_name,
            users.last_name,
            users.created_at,
            users.updated_at
        FROM auth_sessions
        INNER JOIN users
        ON auth_sessions.user_id = users.id
        LEFT JOIN user_emails
        ON user_emails.user_id = users.id AND user_emails.is_primary
        WHERE auth_sessions.token_hash = $1
        AND auth_sessions.expires_at > now()
        AND auth_sessions.revoked_at IS NULL
        AND users.disabled_at IS NULL
        "#,
        token.as_ref()
    )
    .fetch_optional(pool)
    .await
}

/// Bumps the session's `last_used_at`, but only when it is stale by more than a
/// few minutes, so an active session is not a write on every single request.
pub async fn touch_last_used(pool: &PgPool, token: &TokenHash) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE auth_sessions
        SET last_used_at = now()
        WHERE token_hash = $1 AND last_used_at < now() - interval '5 minutes'
        "#,
        token.as_ref(),
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_session(
    pool: &PgPool,
    token: &TokenHash,
    user_id: UserId,
    ip_address: IpAddr,
    user_agent: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO auth_sessions (token_hash, user_id, ip_address, user_agent) VALUES ($1, $2, $3, $4)",
        token.as_ref(),
        user_id.0,
        // sqlx canonicalises `inet` to IpNet; a host IpAddr converts to a /32 or
        // /128 network.
        IpNet::from(ip_address),
        user_agent,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Soft-revokes a session (logout / "sign out this device"), keeping the row as
/// a trail. The cleaner deletes it once expired. The validator already excludes
/// revoked rows, so a revoked session stops resolving immediately.
pub async fn revoke_session(pool: &PgPool, token: &TokenHash) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE auth_sessions SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL",
        token.as_ref(),
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Soft-revokes every active session for `user_id` except the one identified by
/// `keep` (the caller's current session). Backs "sign out other devices" and the
/// post-password-change session reset. Returns the number of sessions revoked.
pub async fn revoke_other_sessions(
    pool: &PgPool,
    user_id: UserId,
    keep: &TokenHash,
) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE auth_sessions
        SET revoked_at = now()
        WHERE user_id = $1 AND token_hash <> $2 AND revoked_at IS NULL
        "#,
        user_id.0,
        keep.as_ref(),
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
