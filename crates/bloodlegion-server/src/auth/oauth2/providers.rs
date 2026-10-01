mod admin;
pub mod api;
mod db;
pub mod http;
mod provision;
pub mod register;

pub(crate) use api::{ClaimInput, CreateProviderPayload};

use anyhow::Context;
use sqlx::PgPool;
use thiserror::Error;
use time::{Duration, OffsetDateTime};

use crate::users::{UserId, UsersError as CreateUserError};
use crate::{Problem, Result, State};

/// The OAuth2-only targets a claim may map onto, on top of the shared
/// [`crate::auth::Target`] fields: the protocol `subject` (the stable account id,
/// stored in its own credential column, never a profile field) and
/// `is_email_verified` (the upstream's assertion that the address is confirmed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Oauth2Extra {
    Subject,
    IsEmailVerified,
}

/// The internal target a claim maps onto: any shared [`crate::auth::Target`]
/// field, or an [`Oauth2Extra`] target specific to the OAuth2 side. Serde reads it
/// untagged, so both variants (de)serialize as the flat snake_case name ("email",
/// "subject", ...), an unknown name fails at the request boundary. Every target
/// is single-valued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Oauth2Target {
    Common(crate::auth::Target),
    Extra(Oauth2Extra),
}

#[derive(Debug, Error, Problem)]
pub enum ProviderError {
    #[error("unknown provider")]
    #[problem(status = NOT_FOUND, title = "Unknown Provider", detail = "No such login provider.")]
    UnknownProvider,
    #[error("invalid login request")]
    #[problem(
        status = BAD_REQUEST,
        title = "Invalid Login Request",
        detail = "The login request is invalid or has expired."
    )]
    InvalidState,
    #[error("could not verify the id token")]
    #[problem(
        status = UNAUTHORIZED,
        title = "Authentication Failed",
        detail = "Could not verify your identity."
    )]
    InvalidIdToken,
    #[error("subject missing")]
    #[problem(
        status = FORBIDDEN,
        title = "Subject Missing",
        detail = "The provider's userinfo did not supply the configured subject field."
    )]
    SubjectMissing,
    #[error("email in use")]
    #[problem(
        status = CONFLICT,
        title = "Email In Use",
        detail = "That email address belongs to another account."
    )]
    EmailInUse,
    #[error("no pending registration")]
    #[problem(
        status = NOT_FOUND,
        title = "No Pending Registration",
        detail = "There is no sign-in waiting to be completed. Start again from the sign-in page."
    )]
    NoPendingRegistration,
    #[error("registration disabled")]
    #[problem(
        status = FORBIDDEN,
        title = "Registration Disabled",
        detail = "Registration via this provider is disabled."
    )]
    RegistrationDisabled,
    /// A failure creating the local account (invalid/taken username, over-long
    /// name, ...), rendered as the underlying user-creation problem. Mirrors the
    /// SAML side's `CreateUser`.
    #[error(transparent)]
    #[problem(transparent)]
    CreateUser(CreateUserError),
    #[error(transparent)]
    #[problem(
        status = BAD_GATEWAY,
        title = "Identity Provider Error",
        detail = "The identity provider could not be reached or returned an unexpected response."
    )]
    Upstream(reqwest::Error),
}

/// The standard OIDC claim mapping, applied when a provider is created without an
/// explicit one. `preferred_username` is the OIDC username claim; provisioning
/// requires it to be a valid, available username and fails otherwise (it is not
/// coerced or derived from the email).
fn default_claim_mapping() -> serde_json::Value {
    serde_json::json!([
        {"claim": "preferred_username", "target": "username"},
        {"claim": "email", "target": "email"},
        {"claim": "email_verified", "target": "is_email_verified"},
        {"claim": "given_name", "target": "first_name"},
        {"claim": "family_name", "target": "last_name"},
    ])
}

/// Registers a provider, validating it exactly as the admin API does. The CLI's entry point.
pub(crate) async fn create(
    state: &State,
    payload: CreateProviderPayload,
) -> Result<(), admin::CreateError> {
    admin::create_provider(state, payload).await
}

/// The configured providers, as the admin listing shows them. The CLI's entry point.
pub(crate) async fn list(pool: &PgPool) -> sqlx::Result<Vec<db::ProviderSummary>> {
    db::list_providers(pool).await
}

/// Removes a provider by slug, returning whether one matched. The CLI's entry point.
pub(crate) async fn remove(pool: &PgPool, slug: &crate::Slug) -> sqlx::Result<bool> {
    db::delete_provider(pool, slug).await
}

/// OAuth2 providers offered on the login page.
pub async fn list_login_providers(
    pool: &PgPool,
) -> sqlx::Result<Vec<crate::auth::ProviderListing>> {
    db::list_login_providers(pool).await
}

/// OAuth2 providers with this user's connection status.
pub async fn list_connections(
    pool: &PgPool,
    user_id: UserId,
) -> sqlx::Result<Vec<crate::auth::ProviderConnectionListing>> {
    db::list_connections(pool, user_id).await
}

/// How long a cached OIDC discovery document (endpoints + JWKS) stays fresh before
/// it is refetched. Bounds staleness for IdP key rotation without paying a
/// discovery + JWKS round trip on every single login.
const OIDC_METADATA_TTL_SECONDS: i64 = 3600;

/// The single OAuth2 callback URL, shared by every provider. The upstream
/// redirects here with the `state` we issued, which resolves the provider, so
/// the path carries no slug (and there is one URL to register upstream).
fn redirect_uri(origin: &str) -> String {
    format!("{origin}/auth/oauth2/callback")
}

fn upstream(error: reqwest::Error) -> crate::Error<ProviderError> {
    crate::Error::External(ProviderError::Upstream(error))
}

/// Resolves a provider's endpoints (+ JWKS for OIDC) from the metadata table. For
/// a plain OAuth2 provider this is the admin-configured row written at create. For
/// OIDC it is the discovery document, cached and refetched only when missing or
/// expired, not on every login.
async fn resolve_metadata(
    state: &State,
    provider: &db::Provider,
) -> Result<db::OidcMetadata, ProviderError> {
    if !provider.is_oidc {
        return Ok(db::find_metadata(&state.pool, provider.id)
            .await
            .context("loading the provider endpoints")?
            .context("non-OIDC provider has no configured endpoints")?);
    }

    let now = OffsetDateTime::now_utc();
    if let Some(metadata) = db::find_metadata(&state.pool, provider.id)
        .await
        .context("loading the cached OIDC metadata")?
        && metadata.expires_at.is_none_or(|expires| expires > now)
    {
        return Ok(metadata);
    }

    let issuer = provider
        .issuer
        .as_deref()
        .context("OIDC provider is missing its issuer")?;
    let discovery = self::http::fetch_discovery(&state.http_client, issuer)
        .await
        .map_err(upstream)?;
    let jwks = self::http::fetch_jwks(&state.http_client, &discovery.jwks_uri)
        .await
        .map_err(upstream)?;
    let jwks = serde_json::to_value(&jwks).context("serializing the fetched JWKS")?;
    db::upsert_metadata(
        &state.pool,
        provider.id,
        &discovery.authorization_endpoint,
        &discovery.token_endpoint,
        discovery.userinfo_endpoint.as_deref(),
        Some(&discovery.jwks_uri),
        Some(&jwks),
        Some(now + Duration::seconds(OIDC_METADATA_TTL_SECONDS)),
    )
    .await
    .context("caching the OIDC discovery document")?;

    Ok(db::find_metadata(&state.pool, provider.id)
        .await
        .context("reloading the cached OIDC metadata")?
        .context("cached OIDC metadata vanished immediately after upsert")?)
}
