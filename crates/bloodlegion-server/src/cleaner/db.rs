use sqlx::postgres::PgPool;

/// Advisory-lock key so only one instance cleans at a time (Canvas sync locks on
/// real course ids, which are never 0).
const CLEANER_LOCK_KEY: i64 = 0;

/// Deletes expired ephemeral rows (sessions, in-flight auth requests, authorization
/// codes, refresh tokens), stale login-throttle counters, and retired signing keys
/// in one pass, returning the number of rows removed. Guarded by a Postgres
/// advisory lock so only one instance sweeps per tick; returns `0` without touching
/// anything when another instance already holds the lock.
pub async fn sweep(pool: &PgPool) -> sqlx::Result<u64> {
    let mut conn = pool.acquire().await?;
    let got_lock = sqlx::query_scalar!(
        r#"SELECT pg_try_advisory_lock($1) AS "locked!""#,
        CLEANER_LOCK_KEY
    )
    .fetch_one(&mut *conn)
    .await?;
    if !got_lock {
        return Ok(0);
    }

    let mut total: u64 = 0;
    total += sqlx::query!(
        "DELETE FROM auth_sessions
         WHERE expires_at < now()
            OR (revoked_at IS NOT NULL AND revoked_at < now() - interval '7 days')"
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    // Tokens for an identity that never became an account (a first sign-in abandoned at the
    // username step); a linked identity keeps its row, which every sign-in rewrites.
    total += sqlx::query!(
        "DELETE FROM auth_oauth2_provider_tokens t
         WHERE t.updated_at < now() - interval '1 day'
           AND NOT EXISTS (
               SELECT 1 FROM auth_oauth2_provider_credentials c
               WHERE c.provider_id = t.provider_id AND c.subject = t.subject
           )"
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    total += sqlx::query!("DELETE FROM auth_oauth2_provider_requests WHERE expires_at < now()")
        .execute(&mut *conn)
        .await?
        .rows_affected();
    total +=
        sqlx::query!("DELETE FROM auth_oauth2_client_authorization_codes WHERE expires_at < now()")
            .execute(&mut *conn)
            .await?
            .rows_affected();
    total += sqlx::query!("DELETE FROM auth_oauth2_client_refresh_tokens WHERE expires_at < now()")
        .execute(&mut *conn)
        .await?
        .rows_affected();
    total += sqlx::query!(
        "DELETE FROM auth_local_login_throttles
         WHERE last_failed_at < now() - interval '1 hour'
           AND (locked_until IS NULL OR locked_until < now())"
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    // Retired/revoked signing keys no longer published (cascades the private half).
    total += sqlx::query!(
        "DELETE FROM auth_jwk_public_keys
         WHERE expires_at < now()
            OR (revoked_at IS NOT NULL AND revoked_at < now() - interval '1 day')"
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();

    let _ = sqlx::query!("SELECT pg_advisory_unlock($1)", CLEANER_LOCK_KEY)
        .fetch_optional(&mut *conn)
        .await;

    Ok(total)
}
