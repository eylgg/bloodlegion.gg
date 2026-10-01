use anyhow::Context;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, serde::iso8601};

use crate::extract::{Admin, SameOrigin};
use crate::users::{User, UserId};
use crate::{Result, State};

use super::LocalError;

/// Local username/password endpoints, mounted under `/api/auth/local`.
pub fn router() -> Router<State> {
    Router::new()
        .route("/", get(get_status).patch(set_enabled))
        .route("/login", post(super::login::handler))
        .route("/set-password", post(set_password))
        .route("/users", get(list_users))
}

#[derive(Serialize)]
struct LocalStatus {
    is_enabled: bool,
}

/// `GET /api/auth/local`: whether local login is currently enabled (admin).
async fn get_status(State { pool, .. }: State, _: Admin) -> Result<Json<LocalStatus>> {
    let is_enabled = super::is_enabled(&pool)
        .await
        .context("loading the local login status")?;
    Ok(Json(LocalStatus { is_enabled }))
}

#[derive(Deserialize)]
struct SetEnabledPayload {
    is_enabled: bool,
}

/// `PATCH /api/auth/local`: enables or disables local login (admin). Disabling is
/// refused with a 409 when no other login provider is enabled.
async fn set_enabled(
    State { pool, .. }: State,
    _: SameOrigin,
    _: Admin,
    Json(payload): Json<SetEnabledPayload>,
) -> Result<Json<LocalStatus>, LocalError> {
    let is_enabled = super::set_enabled(&pool, payload.is_enabled).await?;
    Ok(Json(LocalStatus { is_enabled }))
}

#[derive(Serialize)]
pub struct LocalUserStatus {
    pub user_id: UserId,
    pub username: String,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
}

/// Lists the users that have a local password set (SSO-only accounts are excluded).
async fn list_users(State { pool, .. }: State, _: Admin) -> Result<Json<Vec<LocalUserStatus>>> {
    let rows = super::db::list_credentials(&pool)
        .await
        .context("listing local credentials")?;
    Ok(Json(
        rows.into_iter()
            .map(|r| LocalUserStatus {
                user_id: r.user_id,
                username: r.username,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect(),
    ))
}

#[derive(serde::Deserialize)]
pub struct SetPasswordPayload {
    #[serde(default)]
    current_password: Option<String>,
    password: String,
}

pub async fn set_password(
    State { pool, .. }: State,
    _: SameOrigin,
    user: User,
    jar: CookieJar,
    Json(payload): Json<SetPasswordPayload>,
) -> Result<StatusCode, LocalError> {
    // Self-service password management only makes sense while local login is on;
    // a disabled credential can never be used to sign in.
    if !super::is_enabled(&pool)
        .await
        .context("checking whether local login is enabled")?
    {
        return Err(crate::Error::External(LocalError::LocalLoginDisabled));
    }
    let mut conn = pool
        .acquire()
        .await
        .context("acquiring a database connection")?;
    super::set_password(
        &mut conn,
        user.id,
        payload.current_password.as_deref(),
        &payload.password,
    )
    .await?;
    // A password change invalidates every other session for this user, so a
    // stolen or stale session elsewhere can't outlive the change. The current
    // device (this request's cookie) stays signed in.
    crate::auth::session::revoke_other_sessions(&pool, &jar, user.id)
        .await
        .context("revoking other sessions after the password change")?;
    Ok(StatusCode::NO_CONTENT)
}
