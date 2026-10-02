mod admin;
pub mod api;
mod db;
pub mod http;
pub mod links;
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
    #[error("sign-in cancelled at the provider")]
    #[problem(
        status = FORBIDDEN,
        title = "Sign-In Cancelled",
        detail = "The sign-in was cancelled at the identity provider."
    )]
    Denied,
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
    #[error("scopes not granted: {0}")]
    #[problem(
        status = FORBIDDEN,
        title = "Permissions Required",
        detail = format!("Signing in needs every permission the site asks for. Please sign in again and allow: {_0}.")
    )]
    ScopesNotGranted(String),
    #[error("identity linked to another account")]
    #[problem(
        status = CONFLICT,
        title = "Already Linked",
        detail = "That account is already linked to another Blood Legion account."
    )]
    AlreadyLinked,
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

/// Changes a provider's registration and disconnection flags (each left alone when `None`),
/// returning whether a provider matched. The CLI's entry point.
pub(crate) async fn update_flags(
    pool: &PgPool,
    slug: &crate::Slug,
    is_registration_allowed: Option<bool>,
    is_disconnection_allowed: Option<bool>,
) -> sqlx::Result<bool> {
    db::update_provider_flags(
        pool,
        slug,
        is_registration_allowed,
        None,
        None,
        is_disconnection_allowed,
        None,
    )
    .await
}

impl ProviderError {
    /// A stable code for the front page to turn into a message, when a browser sign-in fails and
    /// is sent back there (`/?error=<code>`). Codes rather than text, so a crafted link cannot
    /// put arbitrary words on the site.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownProvider => "unknown_provider",
            Self::InvalidState | Self::NoPendingRegistration => "expired",
            Self::Denied => "cancelled",
            Self::InvalidIdToken | Self::SubjectMissing => "verification_failed",
            Self::ScopesNotGranted(_) => "permissions_required",
            Self::EmailInUse => "email_in_use",
            Self::AlreadyLinked => "already_linked",
            Self::RegistrationDisabled => "registration_closed",
            Self::CreateUser(_) => "account_failed",
            Self::Upstream(_) => "unavailable",
        }
    }
}

/// The requested scopes the provider did not grant, space-separated, or `None` when all were.
/// An absent `granted` means the provider granted exactly what was requested (RFC 6749 section
/// 5.1 lets it omit the field then). Scopes are compared as whole tokens.
fn missing_scopes(requested: &str, granted: Option<&str>) -> Option<String> {
    let granted: Vec<&str> = granted?.split_whitespace().collect();
    let missing: Vec<&str> = requested
        .split_whitespace()
        .filter(|scope| !granted.contains(scope))
        .collect();
    (!missing.is_empty()).then(|| missing.join(" "))
}

/// One of a user's linked identities' decrypted tokens, for calling the provider's APIs on their
/// behalf.
pub(crate) struct ProviderTokens {
    /// Which account at the provider these are for (a BattleTag), when it said.
    pub identity: Option<String>,
    pub access_token: String,
    pub has_refresh_token: bool,
    pub expires_at: Option<OffsetDateTime>,
    pub scope: String,
    pub updated_at: OffsetDateTime,
}

/// The tokens from the latest sign-in (or link) of each identity the user has linked at the
/// provider named by `slug`, oldest link first. Empty when they have none there, or linked before
/// tokens were kept.
pub(crate) async fn tokens_for_user(
    state: &State,
    user_id: UserId,
    slug: &crate::Slug,
) -> anyhow::Result<Vec<ProviderTokens>> {
    let stored = db::find_tokens_for_user(&state.pool, user_id, slug)
        .await
        .context("loading the stored provider tokens")?;
    stored
        .into_iter()
        .map(|stored| {
            Ok(ProviderTokens {
                identity: provision::identity_label(&stored.raw_userinfo),
                access_token: state
                    .decrypt(&stored.access_token)
                    .context("decrypting the access token")?,
                has_refresh_token: stored.refresh_token.is_some(),
                expires_at: stored.expires_at,
                scope: stored.scope,
                updated_at: stored.updated_at,
            })
        })
        .collect()
}

/// An access token for the provider's own APIs as this site, not as any person (the client
/// credentials grant), with the client registered for sign-in: Battle.net's Game Data API takes
/// one. `None` when no provider has the slug.
pub(crate) async fn app_access_token(
    state: &State,
    slug: &crate::Slug,
) -> anyhow::Result<Option<String>> {
    let Some(provider) = db::find_provider_by_slug(&state.pool, slug)
        .await
        .context("looking up the provider")?
    else {
        return Ok(None);
    };
    let metadata = resolve_metadata(state, &provider)
        .await
        .map_err(|error| match error {
            crate::Error::External(problem) => anyhow::anyhow!("{problem}"),
            crate::Error::Internal(error) => error,
        })
        .context("resolving the provider's endpoints")?;
    let client_secret = state
        .decrypt(&provider.client_secret)
        .context("decrypting the client secret")?;
    let tokens = http::client_credentials(
        &state.http_client,
        &metadata.token_endpoint,
        &provider.client_id,
        &client_secret,
    )
    .await
    .context("requesting a client credentials token")?;
    Ok(Some(tokens.access_token))
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

#[cfg(test)]
mod scope_tests {
    use super::missing_scopes;

    #[test]
    fn every_requested_scope_must_be_granted() {
        assert_eq!(missing_scopes("openid wow.profile", None), None);
        assert_eq!(
            missing_scopes("openid wow.profile", Some("wow.profile openid")),
            None
        );
        assert_eq!(
            missing_scopes("openid wow.profile", Some("openid")).as_deref(),
            Some("wow.profile")
        );
        assert_eq!(
            missing_scopes("openid wow.profile", Some("")).as_deref(),
            Some("openid wow.profile")
        );
        // Whole tokens only: `wow.profile.extra` does not satisfy `wow.profile`.
        assert_eq!(
            missing_scopes("wow.profile", Some("wow.profile.extra")).as_deref(),
            Some("wow.profile")
        );
    }
}
