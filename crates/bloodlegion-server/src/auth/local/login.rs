use anyhow::Context;
use axum::Json;
use axum::extract::Query;
use axum_extra::extract::cookie::CookieJar;
use sqlx::types::ipnet::IpNet;
use thiserror::Error;

use crate::auth::{Params, RedirectTarget, session::start_session};
use crate::extract::{ClientInfo, SameOrigin};
use crate::{Problem, Result, State};

#[derive(Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Credential {
    Cookie,
    /// Reserved for issuing a bearer JWT (for the OAuth2 userinfo flow) instead
    /// of a session cookie; not yet honored by the login handler.
    Jwt,
}

fn default_credentials() -> Vec<Credential> {
    vec![Credential::Cookie]
}

#[derive(serde::Deserialize)]
pub struct Payload {
    username: String,
    password: String,
    #[serde(default = "default_credentials")]
    pub credentials: Vec<Credential>,
}

#[derive(Debug, Error, Problem)]
pub enum LoginError {
    #[error("local login is disabled")]
    #[problem(status = FORBIDDEN, title = "Forbidden", detail = "Local login is disabled.")]
    LocalLoginDisabled,
    #[error("username not found")]
    #[problem(
        status = UNAUTHORIZED,
        title = "Unauthorized",
        detail = "Invalid username or password."
    )]
    UsernameNotFound,
    #[error("no password set")]
    #[problem(
        status = UNAUTHORIZED,
        title = "Unauthorized",
        detail = "Invalid username or password."
    )]
    NoPasswordSet,
    #[error("invalid password")]
    #[problem(
        status = UNAUTHORIZED,
        title = "Unauthorized",
        detail = "Invalid username or password."
    )]
    InvalidPassword,
    #[error("too many attempts")]
    #[problem(
        status = TOO_MANY_REQUESTS,
        title = "Too Many Attempts",
        detail = "Too many failed login attempts. Try again later."
    )]
    TooManyAttempts,
}

/// Rolling-window brute-force policy. Two buckets, both keyed on the source IP:
/// a strict per-(username, ip) bucket for targeted guessing of one account, and a
/// lenient IP-wide bucket for credential stuffing across accounts. The IP-wide
/// threshold is high on purpose, a shared NAT (e.g. a campus) is one IP for many
/// legitimate users, so only a much higher failure rate signals an attack. There
/// is no IP-independent per-username bucket, so no one can lock a victim out
/// globally just by guessing their password.
const THROTTLE_WINDOW_SECS: i32 = 15 * 60;
const ACCOUNT_MAX_FAILURES: i32 = 5;
const IP_MAX_FAILURES: i32 = 50;
const THROTTLE_LOCKOUT_SECS: i32 = 15 * 60;

/// Locked out if either this account-from-this-IP or the source IP as a whole is
/// locked.
async fn throttle_locked(
    pool: &sqlx::PgPool,
    ip: IpNet,
    username: &str,
) -> Result<bool, LoginError> {
    Ok(super::db::is_locked(pool, ip, Some(username))
        .await
        .context("checking the per-account lockout")?
        || super::db::is_locked(pool, ip, None)
            .await
            .context("checking the IP lockout")?)
}

/// Records a failed attempt against the IP-wide bucket only, used when there is no
/// valid account to key a per-username bucket on (an unparseable username).
/// Best-effort: a throttle-table hiccup must not change the login outcome.
async fn record_ip_failure(pool: &sqlx::PgPool, ip: IpNet) {
    let _ = super::db::record_failure(
        pool,
        ip,
        None,
        THROTTLE_WINDOW_SECS,
        IP_MAX_FAILURES,
        THROTTLE_LOCKOUT_SECS,
    )
    .await;
}

/// Records a failed attempt against both the (username, ip) and IP-wide buckets.
/// Best-effort: a throttle-table hiccup must not change the login outcome.
async fn record_throttle_failure(pool: &sqlx::PgPool, ip: IpNet, username: &str) {
    let _ = super::db::record_failure(
        pool,
        ip,
        Some(username),
        THROTTLE_WINDOW_SECS,
        ACCOUNT_MAX_FAILURES,
        THROTTLE_LOCKOUT_SECS,
    )
    .await;
    record_ip_failure(pool, ip).await;
}

/// Clears the (username, ip) bucket after a successful login: the user proved
/// control of that account from this source. The IP-wide bucket is left to decay
/// via the rolling window, so one legitimate login on a shared NAT can't reset an
/// attacker's stuffing count.
async fn clear_throttle(pool: &sqlx::PgPool, ip: IpNet, username: &str) {
    let _ = super::db::clear_throttle(pool, ip, Some(username)).await;
}

pub async fn handler(
    State { pool, .. }: State,
    _: SameOrigin,
    client: ClientInfo,
    mut jar: CookieJar,
    Query(params): Query<Params>,
    Json(payload): Json<Payload>,
) -> Result<(CookieJar, Json<RedirectTarget>), LoginError> {
    if !super::is_enabled(&pool)
        .await
        .context("checking whether local login is enabled")?
    {
        return Err(crate::Error::External(LoginError::LocalLoginDisabled));
    }

    let ip = IpNet::from(client.ip_address);

    // The username must be a well-formed `Username` (valid charset and length) to name a
    // real account, the same gate accounts are created through. A value that isn't (odd
    // characters, wrong length) can't exist, so treat it exactly like a wrong password:
    // decoy-hash for constant time and record only the IP-wide failure, never a per-username
    // bucket. The lookup and the throttle then use the lowercase form, so case rotation
    // ("Jon" vs "jon") can neither split the throttle nor miss the account.
    let Ok(username) = crate::users::Username::try_from(payload.username.as_str()) else {
        if super::db::is_locked(&pool, ip, None)
            .await
            .context("checking the IP lockout")?
        {
            return Err(crate::Error::External(LoginError::TooManyAttempts));
        }
        crate::crypto::verify_decoy_password(&payload.password).await;
        record_ip_failure(&pool, ip).await;
        return Err(crate::Error::External(LoginError::UsernameNotFound));
    };
    let username = username.normalized();
    let username = username.as_str();

    // Reject before doing any work (incl. the expensive hash) if this account or
    // the source IP is locked out.
    if throttle_locked(&pool, ip, username).await? {
        return Err(crate::Error::External(LoginError::TooManyAttempts));
    }

    let Some(record) = super::db::find_record(&pool, username)
        .await
        .context("looking up the login record")?
    else {
        // Run a decoy verification so an unknown username takes the same time as
        // a real one, otherwise response timing leaks which usernames exist.
        crate::crypto::verify_decoy_password(&payload.password).await;
        record_throttle_failure(&pool, ip, username).await;
        return Err(crate::Error::External(LoginError::UsernameNotFound));
    };
    let Some(password) = record.password else {
        crate::crypto::verify_decoy_password(&payload.password).await;
        record_throttle_failure(&pool, ip, username).await;
        return Err(crate::Error::External(LoginError::NoPasswordSet));
    };
    // Verification runs on the blocking pool; a plain mismatch is the
    // constant-time `LoginError::InvalidPassword`, while an unusable stored hash
    // or hasher failure propagates as an internal error.
    match password
        .verify_detailed(&payload.password)
        .await
        .context("verifying the password")?
    {
        crate::crypto::PasswordVerification::Matches => {}
        crate::crypto::PasswordVerification::Mismatch => {
            record_throttle_failure(&pool, ip, username).await;
            return Err(crate::Error::External(LoginError::InvalidPassword));
        }
    }

    // Success clears this account's failure counter for this source.
    clear_throttle(&pool, ip, username).await;
    if payload.credentials.contains(&Credential::Cookie) {
        jar = start_session(
            &pool,
            jar,
            record.user_id,
            client.ip_address,
            client.user_agent.as_deref(),
        )
        .await
        .context("adding the session cookie")?;
    }
    Ok((
        jar,
        Json(RedirectTarget {
            url: params.next.to_string(),
        }),
    ))
}
