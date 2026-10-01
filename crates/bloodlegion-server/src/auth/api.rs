use anyhow::Context;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use serde::Serialize;
use time::OffsetDateTime;
use time::serde::iso8601;

use crate::extract::SameOrigin;
use crate::users::User;
use crate::{Result, Slug, State};

use super::{local, logout, oauth2};

/// Public discovery payload returned by `GET /api/auth/providers`.
///
/// Lets the login page render itself without statically knowing which
/// providers are configured: register a provider and the next page load picks it up. `default_next`
/// lives here so frontend and backend agree on the post-auth destination
/// without duplicating the const on the SvelteKit side.
#[derive(Debug, Serialize)]
pub struct LoginOptions {
    pub providers: Vec<LoginProvider>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoginProvider {
    /// Username + password against `auth_local_credentials`. Present only
    /// while local login is enabled (the built-in local provider is not disabled).
    Local,
    /// One row from `auth_oauth2_providers`. The frontend navigates to
    /// `/api/auth/oauth2/providers/{slug}`.
    Oauth2 { slug: Slug, name: String },
}

/// `GET /api/auth/providers`: every way to sign in right now.
pub async fn providers(state: State) -> Result<Json<LoginOptions>> {
    let mut providers = Vec::new();
    if local::is_enabled(&state.pool)
        .await
        .context("checking whether local login is enabled")?
    {
        providers.push(LoginProvider::Local);
    }
    for super::ProviderListing { slug, name } in
        oauth2::providers::list_login_providers(&state.pool)
            .await
            .context("listing OAuth2 providers")?
    {
        providers.push(LoginProvider::Oauth2 { slug, name });
    }
    Ok(Json(LoginOptions { providers }))
}

/// One external identity provider with the signed-in user's connection state.
#[derive(Debug, Serialize)]
pub struct ProviderConnection {
    /// Always `oauth2` for now; kept so a second protocol can be added without a wire change.
    pub kind: &'static str,
    pub slug: Slug,
    pub name: String,
    /// Whether this user currently has an active connection to the provider.
    pub connected: bool,
}

/// The signed-in user's authentication methods, returned by
/// `GET /api/auth/methods` and rendered on the profile page.
#[derive(Debug, Serialize)]
pub struct AuthMethods {
    /// Whether local username/password login is enabled at all. When false, the
    /// password set/change UI is pointless (the credential can't be used to log
    /// in), so the profile page hides it.
    pub local_enabled: bool,
    /// Whether a local password is set (drives "Set" vs "Change password").
    pub password_set: bool,
    /// Every configured external provider with this user's connection status.
    pub providers: Vec<ProviderConnection>,
}

/// `GET /api/auth/methods`: the signed-in user's own authentication methods.
pub async fn methods(state: State, user: User) -> Result<Json<AuthMethods>> {
    let local_enabled = local::is_enabled(&state.pool)
        .await
        .context("checking whether local login is enabled")?;
    let password_set = local::has_password(&state.pool, user.id)
        .await
        .context("checking the local password")?;
    let mut providers = Vec::new();
    for super::ProviderConnectionListing {
        slug,
        name,
        connected,
    } in oauth2::providers::list_connections(&state.pool, user.id)
        .await
        .context("listing OAuth2 connections")?
    {
        providers.push(ProviderConnection {
            kind: "oauth2",
            slug,
            name,
            connected,
        });
    }
    Ok(Json(AuthMethods {
        local_enabled,
        password_set,
        providers,
    }))
}

/// One of the signed-in user's live sessions, returned by `GET /api/auth/sessions`
/// and rendered on the profile page. Carries no token hash, `is_current` is the
/// only link back to the caller's own session.
#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub id: i64,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    #[serde(with = "iso8601")]
    pub last_used_at: OffsetDateTime,
    pub ip_address: String,
    pub user_agent: Option<String>,
    pub is_current: bool,
}

/// `GET /api/auth/sessions`: the signed-in user's live sessions, current first.
pub async fn sessions(state: State, user: User, jar: CookieJar) -> Result<Json<Vec<SessionInfo>>> {
    let sessions = super::session::list_sessions(&state.pool, &jar, user.id)
        .await
        .context("listing the user's sessions")?;
    Ok(Json(
        sessions
            .into_iter()
            .map(|s| SessionInfo {
                id: s.id,
                created_at: s.created_at,
                last_used_at: s.last_used_at,
                ip_address: s.ip_address.to_string(),
                user_agent: s.user_agent,
                is_current: s.is_current,
            })
            .collect(),
    ))
}

/// `POST /api/auth/sign-out-others`: "sign out other devices": revokes every
/// session for the signed-in user except the one making this request. Reports
/// how many were revoked so the UI can confirm.
#[derive(Debug, Serialize)]
pub struct SignedOutOthers {
    pub revoked: u64,
}

pub async fn sign_out_others(
    state: State,
    _: SameOrigin,
    user: User,
    jar: CookieJar,
) -> Result<Json<SignedOutOthers>> {
    let revoked = super::session::revoke_other_sessions(&state.pool, &jar, user.id)
        .await
        .context("signing out other sessions")?;
    Ok(Json(SignedOutOthers { revoked }))
}

/// Endpoints the SPA drives, mounted under `/api/auth`.
pub fn router() -> Router<State> {
    Router::new()
        .route("/providers", get(providers))
        .route("/methods", get(methods))
        .route("/sessions", get(sessions))
        .route("/logout", post(logout::handler))
        .route("/sign-out-others", post(sign_out_others))
        .nest("/local", local::api::router())
        .nest("/oauth2", oauth2::api::router())
}
