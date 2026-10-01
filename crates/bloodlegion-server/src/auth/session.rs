mod db;

use std::net::IpAddr;

use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};

use crate::crypto::{TokenHash, generate_token};
use crate::users::{User, UserId};

const NAME: &str = "__Host-token";

/// A live session as shown on the profile page. Derived from a [`db::SessionRow`]
/// with the raw `inet` reduced to its host address.
pub struct Session {
    pub id: i64,
    pub created_at: OffsetDateTime,
    pub last_used_at: OffsetDateTime,
    pub ip_address: IpAddr,
    pub user_agent: Option<String>,
    pub is_current: bool,
}

fn build_cookie(id: Option<String>) -> Cookie<'static> {
    let is_addition = id.is_some();
    let mut builder = match id {
        Some(value) => Cookie::build((NAME, value)),
        None => Cookie::build(NAME),
    }
    .path("/")
    .secure(true);
    if is_addition {
        builder = builder
            .same_site(SameSite::Lax)
            .http_only(true)
            .max_age(Duration::days(90));
    }
    builder.build()
}

/// Starts a session for `user_id`: inserts an `auth_sessions` row keyed by the
/// token hash and sets the `__Host-token` cookie carrying the raw token.
pub async fn start_session(
    pool: &PgPool,
    jar: CookieJar,
    user_id: UserId,
    ip_address: IpAddr,
    user_agent: Option<&str>,
) -> sqlx::Result<CookieJar> {
    let id = generate_token::<32>();
    db::insert_session(pool, &TokenHash::new(&id), user_id, ip_address, user_agent).await?;
    Ok(jar.add(build_cookie(Some(id))))
}

/// Ends the current session (logout): soft-revokes the `auth_sessions` row and
/// clears the cookie. The cookie is cleared **unconditionally**, even if the
/// revoke write fails, so a transient database error can never leave the user
/// holding a cookie they believe is logged out. A failed revoke is logged; the
/// row's session token still resolves until it expires, but the client no longer
/// presents it.
pub async fn end_session(pool: &PgPool, jar: CookieJar) -> CookieJar {
    if let Some(cookie) = jar.get(NAME) {
        let id = cookie.value_trimmed();
        if let Err(error) = db::revoke_session(pool, &TokenHash::new(id)).await {
            tracing::warn!(%error, "failed to revoke session on logout; clearing the cookie anyway");
        }
    }
    jar.remove(build_cookie(None))
}

/// Revokes every session for `user_id` except the current one (the cookie in
/// `jar`), returning the number revoked. Backs the "sign out other devices"
/// action and the session reset after a password change. When the request
/// carries no session cookie, all of the user's sessions are revoked (the
/// sentinel hash matches nothing).
pub async fn revoke_other_sessions(
    pool: &PgPool,
    jar: &CookieJar,
    user_id: UserId,
) -> sqlx::Result<u64> {
    let keep = jar
        .get(NAME)
        .map(|cookie| TokenHash::new(cookie.value_trimmed()))
        .unwrap_or_else(|| TokenHash::new(""));
    db::revoke_other_sessions(pool, user_id, &keep).await
}

/// Lists the signed-in user's live sessions, most-recently-active first, with
/// the session behind the request's own cookie flagged `is_current`. When the
/// request carries no cookie, no row is current (the sentinel hash matches none).
pub async fn list_sessions(
    pool: &PgPool,
    jar: &CookieJar,
    user_id: UserId,
) -> sqlx::Result<Vec<Session>> {
    let current = jar
        .get(NAME)
        .map(|cookie| TokenHash::new(cookie.value_trimmed()))
        .unwrap_or_else(|| TokenHash::new(""));
    let rows = db::list_sessions(pool, user_id, &current).await?;
    Ok(rows
        .into_iter()
        .map(|row| Session {
            id: row.id,
            created_at: row.created_at,
            last_used_at: row.last_used_at,
            ip_address: row.ip_address.addr(),
            user_agent: row.user_agent,
            is_current: row.is_current,
        })
        .collect())
}

/// Resolves the session cookie to its user, or `None` when there is no valid,
/// unexpired, unrevoked session.
pub async fn authenticate(pool: &PgPool, jar: CookieJar) -> sqlx::Result<Option<User>> {
    let Some(cookie) = jar.get(NAME) else {
        return Ok(None);
    };
    let id = TokenHash::new(cookie.value_trimmed());
    let user = db::find_user_by_session_token(pool, &id).await?;
    if user.is_some() {
        // Best-effort "last active" tracking; a failure here must not fail the request.
        let _ = db::touch_last_used(pool, &id).await;
    }
    Ok(user)
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    #[sqlx::test]
    async fn test_session_token_is_hashed(pool: PgPool) -> sqlx::Result<()> {
        let id = "nzwqF7hdTmGQr3LD1RjgpHti-dHAhD5fqRFtLnxLgPU";
        let hashed_id = "9V1HyHjM58Voeqr3b_xbtYSoERNZG91DYIvRFn0caDQ";
        // `TokenHash::new` hashes the raw id, so storage only ever holds the hash.
        assert_eq!(crate::crypto::hash_token(id), hashed_id);
        let mut tx = pool.begin().await?;
        let user = crate::users::insert_user(
            &mut tx,
            &crate::users::CreateUserPayload {
                username: "user".to_string(),
                email: Some("user@example.com".to_string()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            false,
        )
        .await?;
        tx.commit().await?;
        db::insert_session(
            &pool,
            &TokenHash::new(id),
            user.id,
            "127.0.0.1".parse().unwrap(),
            Some("test-agent"),
        )
        .await?;
        let found_user = db::find_user_by_session_token(&pool, &TokenHash::new(id)).await?;
        assert_eq!(found_user.map(|u| u.id), Some(user.id));
        Ok(())
    }

    #[sqlx::test]
    async fn a_session_resolves_for_a_user_without_an_email(pool: PgPool) -> sqlx::Result<()> {
        // A Battle.net account has no email; its session must still resolve.
        let mut tx = pool.begin().await?;
        let user = crate::users::insert_user(
            &mut tx,
            &crate::users::CreateUserPayload {
                username: "Thrall".to_string(),
                email: None,
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            false,
        )
        .await?;
        tx.commit().await?;
        let id = "a-session-token-for-an-emailless-account";
        db::insert_session(
            &pool,
            &TokenHash::new(id),
            user.id,
            "127.0.0.1".parse().unwrap(),
            None,
        )
        .await?;
        let found = db::find_user_by_session_token(&pool, &TokenHash::new(id))
            .await?
            .expect("the session resolves");
        assert_eq!(found.id, user.id);
        assert_eq!(found.username, "Thrall");
        assert_eq!(found.email, None);
        assert!(!found.is_email_verified);
        Ok(())
    }

    #[sqlx::test]
    async fn revoking_a_session_makes_it_unresolvable(pool: PgPool) -> sqlx::Result<()> {
        let id = "9V1HyHjM58Voeqr3b_xbtYSoERNZG91DYIvRFn0caDQ";
        let mut tx = pool.begin().await?;
        let user = crate::users::insert_user(
            &mut tx,
            &crate::users::CreateUserPayload {
                username: "cookieuser".to_string(),
                email: Some("cookie@example.com".to_string()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            true,
        )
        .await?;
        tx.commit().await?;

        db::insert_session(
            &pool,
            &TokenHash::new(id),
            user.id,
            "127.0.0.1".parse().unwrap(),
            Some("test-agent"),
        )
        .await?;
        assert!(
            db::find_user_by_session_token(&pool, &TokenHash::new(id))
                .await?
                .is_some()
        );

        db::revoke_session(&pool, &TokenHash::new(id)).await?;
        assert!(
            db::find_user_by_session_token(&pool, &TokenHash::new(id))
                .await?
                .is_none()
        );
        Ok(())
    }
}
