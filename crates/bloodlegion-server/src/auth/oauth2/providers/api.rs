use anyhow::Context;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use crate::auth::{Params, session::start_session};
use crate::extract::{Admin, SameOrigin};
use crate::{Result, Slug, State};

use super::admin::{self, CreateError};
use super::provision::{
    Provisioned, extract_subject, merge_userinfo, provision_user, verify_id_token,
};
use super::register;
use super::{ProviderError, db, http, redirect_uri, resolve_metadata, upstream};

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn login(
    state: State,
    Path(slug): Path<Slug>,
    Query(params): Query<Params>,
) -> Result<Redirect, ProviderError> {
    let provider = db::find_provider_by_slug(&state.pool, &slug)
        .await
        .context("looking up the provider")?
        .ok_or(crate::Error::External(ProviderError::UnknownProvider))?;
    let metadata = resolve_metadata(&state, &provider).await?;

    let auth_state = crate::crypto::generate_token::<32>();
    let code_verifier = crate::crypto::generate_token::<32>();
    let code_challenge = crate::crypto::hash_token(&code_verifier);
    // Only OIDC binds an id_token via a nonce; a plain OAuth2 provider has none.
    let nonce = provider.is_oidc.then(crate::crypto::generate_token::<32>);

    // A plain login is not bound to an account; a future connect flow would pass
    // the signed-in user's id here so the callback can link to it.
    db::insert_request(
        &state.pool,
        &auth_state,
        provider.id,
        None,
        nonce.as_deref(),
        &code_verifier,
        &params.next,
    )
    .await
    .context("persisting the in-flight OAuth2 request")?;

    let mut url = url::Url::parse(&metadata.authorization_endpoint)
        .context("parsing the authorization endpoint")?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &provider.client_id)
        .append_pair("redirect_uri", &redirect_uri(&state.origin))
        .append_pair("scope", &provider.scope)
        .append_pair("state", &auth_state)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256");
    if let Some(nonce) = &nonce {
        url.query_pairs_mut().append_pair("nonce", nonce);
    }

    Ok(Redirect::to(url.as_str()))
}

/// `GET /auth/oauth2/callback`: the single callback every provider redirects to.
/// The `state` parameter (bound to the stored request) identifies which provider
/// this is, so the URL carries no slug.
pub async fn callback(
    state: State,
    Query(query): Query<CallbackQuery>,
    jar: CookieJar,
    client: crate::extract::ClientInfo,
) -> Result<(CookieJar, Redirect), ProviderError> {
    if query.error.is_some() {
        return Err(crate::Error::External(ProviderError::InvalidIdToken));
    }
    let code = query
        .code
        .ok_or(crate::Error::External(ProviderError::InvalidState))?;
    let auth_state = query
        .state
        .ok_or(crate::Error::External(ProviderError::InvalidState))?;

    let request = db::find_pending_request(&state.pool, &auth_state)
        .await
        .context("looking up the in-flight OAuth2 request")?
        .ok_or(crate::Error::External(ProviderError::InvalidState))?;
    let provider = db::find_provider_by_id(&state.pool, request.provider_id)
        .await
        .context("looking up the provider")?
        .ok_or(crate::Error::External(ProviderError::UnknownProvider))?;

    let metadata = resolve_metadata(&state, &provider).await?;
    let client_secret = state
        .decrypt(&provider.client_secret)
        .context("decrypting the client secret")?;
    let tokens = http::exchange_code(
        &state.http_client,
        &metadata.token_endpoint,
        &code,
        &redirect_uri(&state.origin),
        &provider.client_id,
        &client_secret,
        &request.code_verifier,
    )
    .await
    .map_err(upstream)?;

    // OIDC: verify the id_token (subject = its `sub`). Plain OAuth2: there is no
    // id_token, so read the profile from the userinfo endpoint and take the subject
    // from the configured `subject` claim mapping.
    let (subject, userinfo) = if provider.is_oidc {
        let issuer = provider
            .issuer
            .as_deref()
            .context("OIDC provider is missing its issuer")?;
        let id_token = tokens
            .id_token
            .as_deref()
            .ok_or(crate::Error::External(ProviderError::InvalidIdToken))?;
        let jwks_value = metadata
            .jwks
            .as_ref()
            .context("OIDC metadata is missing its JWKS")?;
        let jwks: http::Jwks =
            serde_json::from_value(jwks_value.clone()).context("parsing the cached JWKS")?;
        // The DB trigger guarantees an OIDC request carries a nonce.
        let expected_nonce = request
            .nonce
            .as_deref()
            .ok_or(crate::Error::External(ProviderError::InvalidState))?;
        let (claims, id_token_claims) =
            verify_id_token(id_token, &jwks, issuer, &provider.client_id, expected_nonce).map_err(
                |error| {
                    tracing::warn!(%error, "id_token verification failed");
                    crate::Error::External(ProviderError::InvalidIdToken)
                },
            )?;
        // An IdP may keep profile claims off the id_token and serve them only from its userinfo
        // endpoint (Battle.net's BattleTag), so when discovery advertises one, fold it in.
        let userinfo = match metadata.userinfo_endpoint.as_deref() {
            Some(endpoint) => merge_userinfo(
                id_token_claims,
                http::fetch_userinfo(&state.http_client, endpoint, &tokens.access_token)
                    .await
                    .map_err(upstream)?,
            ),
            None => id_token_claims,
        };
        (claims.sub, userinfo)
    } else {
        let userinfo_endpoint = metadata
            .userinfo_endpoint
            .as_deref()
            .context("non-OIDC provider is missing its userinfo endpoint")?;
        let userinfo =
            http::fetch_userinfo(&state.http_client, userinfo_endpoint, &tokens.access_token)
                .await
                .map_err(upstream)?;
        let subject = extract_subject(&provider, &userinfo)?;
        (subject, userinfo)
    };

    // Single-use gate: atomically record completion (subject + raw userinfo)
    // before provisioning. A replayed callback loses this race and is rejected.
    let completed = db::complete_request(&state.pool, request.id, &subject, &userinfo)
        .await
        .context("completing the in-flight OAuth2 request")?;
    if !completed {
        return Err(crate::Error::External(ProviderError::InvalidState));
    }

    let user_id = match provision_user(&state.pool, &provider, &subject, &userinfo).await? {
        Provisioned::User(user_id) => user_id,
        // The identity is verified and recorded on the request; the person picks a username on
        // the registration page, which resolves the request again through the cookie.
        Provisioned::NeedsUsername => {
            let jar = register::start(jar, &auth_state);
            return Ok((jar, Redirect::to(register::FRONTEND_REGISTER_PATH)));
        }
    };
    let jar = start_session(
        &state.pool,
        jar,
        user_id,
        client.ip_address,
        client.user_agent.as_deref(),
    )
    .await
    .context("adding the session cookie")?;
    Ok((jar, Redirect::to(&request.next)))
}

/// One entry of the claim mapping: the id_token `claim` (e.g. `given_name`) and
/// the internal `target` field it fills (e.g. `first_name`; absent/`null` =
/// carried but not mapped).
#[derive(Deserialize)]
pub(crate) struct ClaimInput {
    pub(crate) claim: String,
    #[serde(default)]
    pub(crate) target: Option<super::Oauth2Target>,
    /// Coerce the value into the account-username shape (lowercase, `[a-z0-9_-]`) instead of
    /// requiring it to already be one. Meaningful only for the `username` target; it is how a
    /// BattleTag (`Name#1234`) becomes the username `name-1234`.
    #[serde(default)]
    pub(crate) normalize: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub(crate) struct CreateProviderPayload {
    pub(crate) slug: String,
    pub(crate) name: String,
    /// Required for an OIDC provider (the discovery anchor); omitted for a plain
    /// OAuth2 provider.
    #[serde(default)]
    pub(crate) issuer: Option<String>,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
    #[serde(default)]
    pub(crate) is_registration_allowed: bool,
    #[serde(default)]
    pub(crate) is_auto_connection_allowed: bool,
    #[serde(default)]
    pub(crate) is_email_verified: bool,
    #[serde(default)]
    pub(crate) is_disconnection_allowed: bool,
    /// Whether a first login may claim an unclaimed account (no linked identity) by the asserted
    /// username. Enable only when that username is authoritative (e.g. the UTORid).
    #[serde(default)]
    pub(crate) is_unclaimed_username_connection_allowed: bool,
    /// A full OIDC provider (id_token, nonce, JWKS) vs a plain OAuth2 provider
    /// (access token + userinfo only, e.g. Discord). Defaults to OIDC.
    #[serde(default = "default_true")]
    pub(crate) is_oidc: bool,
    /// Required for a non-OIDC provider (it has no discovery document); forbidden
    /// for OIDC, which fetches these from the issuer's discovery endpoint.
    #[serde(default)]
    pub(crate) authorization_endpoint: Option<String>,
    #[serde(default)]
    pub(crate) token_endpoint: Option<String>,
    #[serde(default)]
    pub(crate) userinfo_endpoint: Option<String>,
    /// Optional claim->target mapping; the standard OIDC mapping is used if empty.
    #[serde(default)]
    pub(crate) claims: Vec<ClaimInput>,
    /// Optional space-delimited scope set; defaults to the standard OIDC set.
    /// openid is not required (a plain OAuth2 upstream requests its own).
    #[serde(default)]
    pub(crate) scope: Option<String>,
}

/// `POST /api/auth/oauth2/providers`: registers an OAuth2 identity provider.
async fn create_provider(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Json(payload): Json<CreateProviderPayload>,
) -> Result<StatusCode, CreateError> {
    admin::create_provider(&state, payload).await?;
    Ok(StatusCode::CREATED)
}

/// `GET /api/auth/oauth2/providers`: lists configured OAuth2 providers (admin).
async fn list_providers(state: State, _admin: Admin) -> Result<Json<Vec<db::ProviderSummary>>> {
    Ok(Json(
        db::list_providers(&state.pool)
            .await
            .context("listing OAuth2 providers")?,
    ))
}

/// `DELETE /api/auth/oauth2/providers/{slug}`: removes an OAuth2 provider (idempotent).
async fn delete_provider(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Path(slug): Path<Slug>,
) -> Result<StatusCode, CreateError> {
    db::delete_provider(&state.pool, &slug)
        .await
        .map_err(admin::classify_delete)?;
    Ok(StatusCode::NO_CONTENT)
}

/// The editable policy flags of an OAuth2 provider. Every field is optional so a single toggle
/// sends just the one flag; an omitted flag is left unchanged.
#[derive(Deserialize)]
struct UpdateProviderPayload {
    #[serde(default)]
    is_registration_allowed: Option<bool>,
    #[serde(default)]
    is_auto_connection_allowed: Option<bool>,
    #[serde(default)]
    is_email_verified: Option<bool>,
    #[serde(default)]
    is_disconnection_allowed: Option<bool>,
    #[serde(default)]
    is_unclaimed_username_connection_allowed: Option<bool>,
}

/// `PATCH /api/auth/oauth2/providers/{slug}`: updates an OAuth2 provider's policy flags (admin).
async fn update_provider(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Path(slug): Path<Slug>,
    Json(payload): Json<UpdateProviderPayload>,
) -> Result<StatusCode, CreateError> {
    let updated = db::update_provider_flags(
        &state.pool,
        &slug,
        payload.is_registration_allowed,
        payload.is_auto_connection_allowed,
        payload.is_email_verified,
        payload.is_disconnection_allowed,
        payload.is_unclaimed_username_connection_allowed,
    )
    .await
    .context("updating the OAuth2 provider")?;
    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(crate::Error::External(CreateError::NotFound))
    }
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/", post(create_provider).get(list_providers))
        .route(
            "/{slug}",
            get(login).delete(delete_provider).patch(update_provider),
        )
}
