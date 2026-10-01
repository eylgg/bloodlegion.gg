//! Finishing a first sign-in through a provider that asserted no usable username: the callback
//! parks the verified identity and sends the browser to the registration page, which reads the
//! pending sign-in back through [`pending`] and completes it through [`choose`] once the person
//! has picked a username.
//!
//! The pending sign-in is the completed `auth_oauth2_provider_requests` row itself (it already
//! holds the provider, the subject, and the raw userinfo), referenced by its unguessable `state`
//! from a short-lived cookie. The row's own `expires_at` bounds the window: a person has the ten
//! minutes from starting the login to choose, after which they sign in again.

use anyhow::Context;
use axum::Json;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use time::Duration;

use crate::auth::{RedirectTarget, session::start_session};
use crate::extract::{ClientInfo, SameOrigin};
use crate::{Result, Slug, State};

use super::provision::{build_profile, identity_label, register, suggest_username};
use super::{ProviderError, db};

const NAME: &str = "__Host-registration";

/// The registration page's path on the frontend, where the callback sends a first sign-in that
/// still needs a username.
pub(super) const FRONTEND_REGISTER_PATH: &str = "/register";

fn build_cookie(state: Option<String>) -> Cookie<'static> {
    let is_addition = state.is_some();
    let mut builder = match state {
        Some(value) => Cookie::build((NAME, value)),
        None => Cookie::build(NAME),
    }
    .path("/")
    .secure(true);
    if is_addition {
        builder = builder
            .same_site(SameSite::Lax)
            .http_only(true)
            .max_age(Duration::minutes(10));
    }
    builder.build()
}

/// Binds the browser to the pending sign-in identified by `state`.
pub(super) fn start(jar: CookieJar, state: &str) -> CookieJar {
    jar.add(build_cookie(Some(state.to_string())))
}

fn clear(jar: CookieJar) -> CookieJar {
    jar.remove(build_cookie(None))
}

/// The pending sign-in behind the registration cookie, with its provider, or
/// [`ProviderError::NoPendingRegistration`] when there is none, it expired, or it was already
/// completed with an account.
async fn pending_request(
    state: &State,
    jar: &CookieJar,
) -> Result<(db::CompletedRequest, db::Provider), ProviderError> {
    let Some(cookie) = jar.get(NAME) else {
        return Err(crate::Error::External(ProviderError::NoPendingRegistration));
    };
    let request = db::find_completed_request(&state.pool, cookie.value_trimmed())
        .await
        .context("looking up the pending registration")?
        .ok_or(crate::Error::External(ProviderError::NoPendingRegistration))?;
    let provider = db::find_provider_by_id(&state.pool, request.provider_id)
        .await
        .context("looking up the pending registration's provider")?
        .ok_or(crate::Error::External(ProviderError::NoPendingRegistration))?;
    Ok((request, provider))
}

/// What the registration page shows: who the person is signing in with and as, plus a username
/// to prefill.
#[derive(Debug, Serialize)]
pub struct Registration {
    pub provider_slug: Slug,
    pub provider_name: String,
    /// How the provider identifies the person (a BattleTag, a nickname), if it said.
    pub identity: Option<String>,
    /// A username derived from the identity, when one can be; otherwise the field starts empty.
    pub suggestion: Option<String>,
}

/// `GET /api/auth/oauth2/registration`: the pending sign-in the registration page completes.
pub async fn pending(state: State, jar: CookieJar) -> Result<Json<Registration>, ProviderError> {
    let (request, provider) = pending_request(&state, &jar).await?;
    let profile = build_profile(&provider, &request.raw_userinfo)?;
    Ok(Json(Registration {
        provider_slug: provider.slug,
        provider_name: provider.name,
        identity: identity_label(&request.raw_userinfo),
        suggestion: suggest_username(&profile, &request.raw_userinfo),
    }))
}

#[derive(Debug, Deserialize)]
pub struct ChoosePayload {
    username: String,
}

/// `POST /api/auth/oauth2/registration`: creates the account under the chosen username, links
/// the provider credential, and signs the person in. A malformed, reserved, or taken name is
/// the underlying user-creation problem, so the page can say which. If the identity was linked
/// to an account meanwhile (a replay, or a second tab), that account is signed in instead.
pub async fn choose(
    state: State,
    _: SameOrigin,
    client: ClientInfo,
    jar: CookieJar,
    Json(payload): Json<ChoosePayload>,
) -> Result<(CookieJar, Json<RedirectTarget>), ProviderError> {
    let (request, provider) = pending_request(&state, &jar).await?;
    let user_id = match db::find_user_by_credential(&state.pool, provider.id, &request.subject)
        .await
        .context("looking up the provider credential")?
    {
        Some(user_id) => user_id,
        None => {
            if !provider.is_registration_allowed {
                return Err(crate::Error::External(ProviderError::RegistrationDisabled));
            }
            let profile = build_profile(&provider, &request.raw_userinfo)?;
            register(
                &state.pool,
                &provider,
                &request.subject,
                &request.raw_userinfo,
                &profile,
                payload.username.trim(),
            )
            .await?
        }
    };
    let jar = start_session(
        &state.pool,
        clear(jar),
        user_id,
        client.ip_address,
        client.user_agent.as_deref(),
    )
    .await
    .context("adding the session cookie")?;
    Ok((
        jar,
        Json(RedirectTarget {
            url: request.next.to_string(),
        }),
    ))
}
