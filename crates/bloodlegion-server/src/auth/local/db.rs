use sqlx::types::ipnet::IpNet;
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;

use crate::crypto::PasswordHash;
use crate::users::UserId;

pub struct LocalCredentialRow {
    pub user_id: UserId,
    pub username: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Lists the users that have a local password set (one `auth_local_credentials`
/// row each), with when it was set and last changed. Users with no local password, meaning
/// SSO-only accounts, are intentionally excluded via the inner join.
pub async fn list_credentials(pool: &PgPool) -> sqlx::Result<Vec<LocalCredentialRow>> {
    sqlx::query_as!(
        LocalCredentialRow,
        r#"
        SELECT
            users.id AS "user_id: UserId",
            users.username,
            auth_local_credentials.created_at,
            auth_local_credentials.updated_at
        FROM users
        JOIN auth_local_credentials ON users.id = auth_local_credentials.user_id
        ORDER BY users.username
        "#
    )
    .fetch_all(pool)
    .await
}

/// Whether the given user has a local password set.
pub async fn has_password(pool: &PgPool, user_id: UserId) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM auth_local_credentials WHERE user_id = $1) AS "exists!""#,
        user_id.0,
    )
    .fetch_one(pool)
    .await
}

pub struct Record {
    pub user_id: UserId,
    pub password: Option<PasswordHash>,
}

/// Looks up the login record by the *normalized* (lowercase) username.
pub async fn find_record(pool: &PgPool, username: &str) -> sqlx::Result<Option<Record>> {
    sqlx::query_as!(
        Record,
        r#"
        SELECT
            users.id AS "user_id: UserId",
            auth_local_credentials.password_hash AS "password?: PasswordHash"
        FROM users
        LEFT JOIN auth_local_credentials
        ON users.id = auth_local_credentials.user_id
        WHERE users.username_normalized = $1
        AND users.disabled_at IS NULL
        "#,
        username
    )
    .fetch_optional(pool)
    .await
}

pub async fn find_password(
    conn: &mut PgConnection,
    user_id: UserId,
) -> sqlx::Result<Option<PasswordHash>> {
    struct Row {
        password: PasswordHash,
    }
    Ok(sqlx::query_as!(
        Row,
        r#"SELECT password_hash AS "password: PasswordHash" FROM auth_local_credentials WHERE user_id = $1"#,
        user_id.0,
    )
    .fetch_optional(&mut *conn)
    .await?
    .map(|row| row.password))
}

/// Whether a throttle bucket is currently locked out. `username` is `None` for
/// the IP-wide bucket and `Some` for the per-(username, ip) bucket; `IS NOT
/// DISTINCT FROM` matches the `NULL` username row that `= $2` never would.
pub async fn is_locked(pool: &PgPool, ip: IpNet, username: Option<&str>) -> sqlx::Result<bool> {
    let locked = sqlx::query_scalar!(
        r#"
        SELECT locked_until > now() AS "locked!"
        FROM auth_local_login_throttles
        WHERE ip_address = $1
            AND username IS NOT DISTINCT FROM $2
            AND locked_until IS NOT NULL
        "#,
        ip,
        username,
    )
    .fetch_optional(pool)
    .await?;
    Ok(locked.unwrap_or(false))
}

/// Records a failed attempt for a throttle bucket. Counts within a rolling window
/// (`window_secs`); once `threshold` is reached, locks the bucket for
/// `lockout_secs`. A failure after the window lapses starts a fresh count.
/// `username` is `None` for the IP-wide bucket, `Some` for the (username, ip) one.
pub async fn record_failure(
    pool: &PgPool,
    ip: IpNet,
    username: Option<&str>,
    window_secs: i32,
    threshold: i32,
    lockout_secs: i32,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO auth_local_login_throttles (ip_address, username, failed_count, first_failed_at, last_failed_at)
        VALUES ($1, $2, 1, now(), now())
        ON CONFLICT (ip_address, username) DO UPDATE SET
            failed_count = CASE
                WHEN auth_local_login_throttles.last_failed_at < now() - $3::double precision * interval '1 second' THEN 1
                ELSE auth_local_login_throttles.failed_count + 1
            END,
            first_failed_at = CASE
                WHEN auth_local_login_throttles.last_failed_at < now() - $3::double precision * interval '1 second' THEN now()
                ELSE auth_local_login_throttles.first_failed_at
            END,
            last_failed_at = now(),
            locked_until = CASE
                WHEN (CASE
                        WHEN auth_local_login_throttles.last_failed_at < now() - $3::double precision * interval '1 second' THEN 1
                        ELSE auth_local_login_throttles.failed_count + 1
                      END) >= $4
                THEN now() + $5::double precision * interval '1 second'
                ELSE NULL
            END
        "#,
        ip,
        username,
        window_secs as f64,
        threshold,
        lockout_secs as f64,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Clears a throttle bucket after a successful login.
pub async fn clear_throttle(pool: &PgPool, ip: IpNet, username: Option<&str>) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM auth_local_login_throttles
        WHERE ip_address = $1 AND username IS NOT DISTINCT FROM $2
        "#,
        ip,
        username,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Whether local username/password login is enabled: the built-in local provider
/// row (seeded, un-deletable) is not disabled.
pub async fn is_enabled(pool: &PgPool) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT disabled_at IS NULL AS "enabled!" FROM auth_providers WHERE kind = 'local'"#
    )
    .fetch_one(pool)
    .await
}

/// Toggles local login by flipping the local provider's `disabled_at`, enforcing the
/// lockout invariant in the same statement: disabling it (`$1 = false`) only succeeds
/// while at least one other provider is enabled. Now that local is itself a provider
/// row, that guard is the uniform "some non-local provider is still enabled" rather
/// than a per-kind count, and folding it into the `WHERE` keeps the check and the
/// write atomic. Returns the new enabled state, or `None` when the guard blocked it.
pub async fn set_enabled(pool: &PgPool, enabled: bool) -> sqlx::Result<Option<bool>> {
    sqlx::query_scalar!(
        r#"
        UPDATE auth_providers
        SET disabled_at = CASE WHEN $1 THEN NULL ELSE now() END
        WHERE kind = 'local'
            AND (
                $1
                OR EXISTS (
                    SELECT 1 FROM auth_providers
                    WHERE kind <> 'local' AND disabled_at IS NULL
                )
            )
        RETURNING disabled_at IS NULL AS "enabled!"
        "#,
        enabled,
    )
    .fetch_optional(pool)
    .await
}

pub async fn upsert_credentials(
    conn: &mut PgConnection,
    user_id: UserId,
    password: &PasswordHash,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO auth_local_credentials (user_id, password_hash)
        VALUES ($1, $2)
        ON CONFLICT (user_id)
        DO UPDATE SET password_hash = EXCLUDED.password_hash
        "#,
        user_id.0,
        password.as_ref(),
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::{CreateUserPayload, insert_user};

    async fn user(pool: &PgPool, username: &str) -> UserId {
        let mut tx = pool.begin().await.unwrap();
        let user = insert_user(
            &mut tx,
            &CreateUserPayload {
                username: username.to_string(),
                email: Some(format!("{username}@example.com")),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        user.id
    }

    #[sqlx::test]
    async fn find_record_skips_a_disabled_user(pool: PgPool) {
        let user_id = user(&pool, "alice").await;
        assert!(
            find_record(&pool, "alice").await.unwrap().is_some(),
            "an active user is found"
        );

        sqlx::query!(
            "UPDATE users SET disabled_at = now() WHERE id = $1",
            user_id.0
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            find_record(&pool, "alice").await.unwrap().is_none(),
            "a disabled user is not found, so cannot log in"
        );
    }

    #[sqlx::test]
    async fn disabling_local_login_is_blocked_without_another_enabled_provider(pool: PgPool) {
        // Fresh DB: only the built-in local provider exists, so disabling is refused...
        assert!(is_enabled(&pool).await.unwrap());
        assert!(set_enabled(&pool, false).await.unwrap().is_none());
        assert!(
            is_enabled(&pool).await.unwrap(),
            "still enabled after the block"
        );
        // ...but enabling always succeeds.
        assert_eq!(set_enabled(&pool, true).await.unwrap(), Some(true));
    }

    #[sqlx::test]
    async fn disabling_local_login_is_allowed_with_another_enabled_provider(pool: PgPool) {
        // An enabled OAuth2 provider satisfies the "some other provider is enabled" guard.
        let mut tx = pool.begin().await.unwrap();
        let provider_id = sqlx::query_scalar!(
            "INSERT INTO auth_providers (kind, slug, name) VALUES ('oauth2', 'example', 'Example') RETURNING id"
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query!(
            r#"INSERT INTO auth_oauth2_providers
                   (provider_id, issuer, client_id, client_secret_ciphertext, claims, scope, is_oidc)
               VALUES ($1, 'https://example.test', 'client', 'secret', '[]'::jsonb, 'openid', true)"#,
            provider_id,
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(set_enabled(&pool, false).await.unwrap(), Some(false));
        assert!(!is_enabled(&pool).await.unwrap());

        // A disabled other provider no longer counts, so re-disabling after re-enabling
        // would be blocked; but enabling is always fine.
        assert_eq!(set_enabled(&pool, true).await.unwrap(), Some(true));
    }

    #[sqlx::test]
    async fn throttle_locks_after_threshold_then_clears(pool: PgPool) {
        let ip: IpNet = "1.2.3.4".parse::<std::net::IpAddr>().unwrap().into();
        let user = Some("alice");
        assert!(!is_locked(&pool, ip, user).await.unwrap());

        // Four failures within the window do not lock (threshold is 5).
        for _ in 0..4 {
            record_failure(&pool, ip, user, 900, 5, 900).await.unwrap();
        }
        assert!(!is_locked(&pool, ip, user).await.unwrap());

        // The fifth crosses the threshold and locks.
        record_failure(&pool, ip, user, 900, 5, 900).await.unwrap();
        assert!(is_locked(&pool, ip, user).await.unwrap());

        // A successful login clears the counter.
        clear_throttle(&pool, ip, user).await.unwrap();
        assert!(!is_locked(&pool, ip, user).await.unwrap());
    }

    #[sqlx::test]
    async fn throttle_buckets_are_independent_per_username_and_ip_wide(pool: PgPool) {
        let ip: IpNet = "1.2.3.4".parse::<std::net::IpAddr>().unwrap().into();

        // Lock the (alice, ip) bucket; the IP-wide bucket and another user are
        // unaffected, a single account's lock can't lock the whole source.
        for _ in 0..5 {
            record_failure(&pool, ip, Some("alice"), 900, 5, 900)
                .await
                .unwrap();
        }
        assert!(is_locked(&pool, ip, Some("alice")).await.unwrap());
        assert!(!is_locked(&pool, ip, Some("bob")).await.unwrap());
        assert!(!is_locked(&pool, ip, None).await.unwrap());

        // The IP-wide bucket is its own row and locks on its own threshold.
        for _ in 0..5 {
            record_failure(&pool, ip, None, 900, 5, 900).await.unwrap();
        }
        assert!(is_locked(&pool, ip, None).await.unwrap());
    }
}
