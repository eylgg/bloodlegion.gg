use anyhow::Context;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use crate::auth::{Next, Params, session::start_session};
use crate::extract::{Admin, SameOrigin};
use crate::users::{User, UserId};
use crate::{Result, Slug, State};

use super::admin::{self, CreateError};
use super::provision::{
    Provisioned, extract_subject, merge_userinfo, provision_user, verify_id_token,
};
use super::{ProviderError, db, http, redirect_uri, resolve_metadata, upstream};
use super::{links, register};

/// Where a link flow ends, on success or failure: the profile page, which lists linked accounts.
const PROFILE_PATH: &str = "/profile";

/// The start-login query: the usual `next`, plus `prompt=consent` to force the provider's consent
/// screen. A provider remembers what a person granted and re-grants exactly that silently, so
/// someone who once withheld a scope would otherwise be refused forever; the front page adds this
/// on the retry after a permissions failure. Only `consent` is passed through.
#[derive(Deserialize)]
struct LoginQuery {
    #[serde(flatten)]
    params: Params,
    #[serde(default)]
    prompt: Option<String>,
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Where a browser sign-in lands when it fails: the front page, with a code it turns into a
/// message. Both endpoints here are full-page navigations, so a JSON problem would strand the
/// person on raw JSON. Internal failures are logged and reported as `unavailable`.
fn to_front_page<T: IntoResponse>(result: Result<T, ProviderError>) -> Response {
    on_error_to("/", result)
}

/// [`to_front_page`], landing on `path` instead (the profile page, for a link flow).
fn on_error_to<T: IntoResponse>(path: &str, result: Result<T, ProviderError>) -> Response {
    let code = match result {
        Ok(response) => return response.into_response(),
        Err(crate::Error::External(error)) => {
            if error.code() == "unavailable" {
                tracing::error!(%error, "oauth2 sign-in failed upstream");
            } else {
                tracing::info!(%error, "oauth2 sign-in refused");
            }
            error.code()
        }
        Err(crate::Error::Internal(error)) => {
            tracing::error!(error = ?error, "oauth2 sign-in failed");
            "unavailable"
        }
    };
    Redirect::to(&format!("{path}?error={code}")).into_response()
}

/// `GET /api/auth/oauth2/providers/{slug}`: starts a sign-in (see [`to_front_page`]).
async fn login(
    state: State,
    Path(slug): Path<Slug>,
    Query(LoginQuery { params, prompt }): Query<LoginQuery>,
) -> Response {
    let prompt = (prompt.as_deref() == Some("consent")).then_some("consent");
    to_front_page(start_login(state, slug, None, params.next, prompt).await)
}

/// `GET /api/auth/oauth2/providers/{slug}/link`: starts linking another account at the provider
/// to the signed-in member (see [`links`]); it ends on the profile page either way. Asks the
/// provider to prompt for a sign-in, so the person can pick an account other than the one the
/// browser is already signed in to there.
async fn link(state: State, user: Option<User>, Path(slug): Path<Slug>) -> Response {
    let Some(user) = user else {
        return Redirect::to(&format!("/?next={}", urlencoding::encode(PROFILE_PATH)))
            .into_response();
    };
    on_error_to(
        PROFILE_PATH,
        start_login(
            state,
            slug,
            Some(user.id),
            Next::fixed(PROFILE_PATH),
            Some("login"),
        )
        .await,
    )
}

/// Sends the browser to the provider's authorization endpoint. `user_id` makes it a link to that
/// member rather than a sign-in; `prompt` is passed to the provider as is.
async fn start_login(
    state: State,
    slug: Slug,
    user_id: Option<UserId>,
    next: Next,
    prompt: Option<&str>,
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

    db::insert_request(
        &state.pool,
        &auth_state,
        provider.id,
        user_id,
        nonce.as_deref(),
        &code_verifier,
        &next,
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
    if let Some(prompt) = prompt {
        url.query_pairs_mut().append_pair("prompt", prompt);
    }

    Ok(Redirect::to(url.as_str()))
}

/// `GET /auth/oauth2/callback`: the single callback every provider redirects to.
/// The `state` parameter (bound to the stored request) identifies which provider
/// this is, so the URL carries no slug.
/// `GET /auth/oauth2/callback`: finishes a sign-in or a link (see [`to_front_page`]). A failure
/// while signed in can only be a link (nobody signs in twice), so it lands on the profile page.
pub async fn callback(
    state: State,
    user: Option<User>,
    query: Query<CallbackQuery>,
    jar: CookieJar,
    client: crate::extract::ClientInfo,
) -> Response {
    let on_error = if user.is_some() { PROFILE_PATH } else { "/" };
    on_error_to(on_error, finish_login(state, query, jar, client).await)
}

async fn finish_login(
    state: State,
    Query(query): Query<CallbackQuery>,
    jar: CookieJar,
    client: crate::extract::ClientInfo,
) -> Result<(CookieJar, Redirect), ProviderError> {
    // The provider reports a refusal on the callback itself; `access_denied` is the person
    // pressing cancel on its consent screen.
    if let Some(error) = &query.error {
        return Err(crate::Error::External(if error == "access_denied" {
            ProviderError::Denied
        } else {
            ProviderError::InvalidIdToken
        }));
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

    // Every requested scope is required: a consent screen that lets the person untick one
    // (Battle.net's `wow.profile`) must not yield a sign-in without it. Checked before the
    // request is completed, so starting over works.
    if let Some(missing) = super::missing_scopes(&provider.scope, tokens.scope.as_deref()) {
        return Err(crate::Error::External(ProviderError::ScopesNotGranted(
            missing,
        )));
    }

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
                    // The full cause chain (which check failed), plus what the token claims and
                    // what we expected: enough to tell an issuer or audience mismatch from a bad
                    // signature without capturing a token. `iss`/`aud` are not secrets.
                    let (token_iss, token_aud) = super::provision::unverified_iss_aud(id_token);
                    tracing::warn!(
                        error = format!("{error:#}"),
                        token_iss,
                        token_aud,
                        expected_iss = issuer,
                        expected_aud = %provider.client_id,
                        "id_token verification failed"
                    );
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
    // Keep the tokens so the server can call the provider's APIs for this person later (the WoW
    // profile, for Battle.net). Stored before provisioning, so a first sign-in that still needs
    // a username keeps them too; the credential created then joins on (provider, subject).
    tracing::info!(
        provider = %provider.slug,
        has_refresh_token = tokens.refresh_token.is_some(),
        expires_in = tokens.expires_in,
        "oauth2 tokens received"
    );
    let access_token = state
        .encrypt(&tokens.access_token)
        .context("encrypting the access token")?;
    let refresh_token = tokens
        .refresh_token
        .as_deref()
        .map(|token| state.encrypt(token))
        .transpose()
        .context("encrypting the refresh token")?;
    let expires_at = tokens
        .expires_in
        .map(|seconds| time::OffsetDateTime::now_utc() + time::Duration::seconds(seconds));
    db::upsert_tokens(
        &state.pool,
        provider.id,
        &subject,
        &access_token,
        refresh_token.as_ref(),
        expires_at,
        tokens.scope.as_deref().unwrap_or(&provider.scope),
    )
    .await
    .context("storing the provider tokens")?;

    let completed = db::complete_request(&state.pool, request.id, &subject, &userinfo)
        .await
        .context("completing the in-flight OAuth2 request")?;
    if !completed {
        return Err(crate::Error::External(ProviderError::InvalidState));
    }

    // A link attaches the identity to the member who started it; their session stays as it is.
    if let Some(user_id) = request.user_id {
        let linked = links::link(&state.pool, &provider, user_id, &subject, &userinfo).await?;
        let target = format!("{}?linked={}", &*request.next, linked.code());
        return Ok((jar, Redirect::to(&target)));
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
        .route("/{slug}/link", get(link))
}
