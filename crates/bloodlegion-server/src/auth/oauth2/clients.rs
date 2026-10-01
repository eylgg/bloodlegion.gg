mod db;

use axum::http::StatusCode;
use axum::{
    Json, Router,
    extract::Path,
    routing::{delete, get},
};
use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use anyhow::Context;
use thiserror::Error;

use crate::crypto::{TokenHash, generate_token};
use crate::extract::{Admin, SameOrigin};
use crate::{Problem, Result, State};

use super::Scope;

/// Looks up a client's name and redirect URIs (for the authorize flow).
pub async fn find_client(pool: &PgPool, client_id: &str) -> sqlx::Result<Option<ClientSummary>> {
    db::find_client(pool, client_id).await
}

/// Whether the given client wants masked (origin-domain) emails, `<username>@<host>`
/// instead of the user's account email. Used by the userinfo endpoint. A missing
/// client (e.g. deleted after token issue) is treated as not masked.
pub async fn client_masks_email(pool: &PgPool, client_id: &str) -> sqlx::Result<bool> {
    Ok(db::find_email_masked(pool, client_id)
        .await?
        .unwrap_or(false))
}

/// The id of the provider a client is federated to, or `None` when the client is
/// unrestricted. Used by `authorize` and `userinfo`.
pub async fn find_identity_provider_id(
    pool: &PgPool,
    client_id: &str,
) -> sqlx::Result<Option<crate::auth::providers::ProviderId>> {
    db::find_identity_provider_id(pool, client_id).await
}

/// The user's live credential profile at `provider_id` plus that provider's
/// email-verification trust flag, or `None` when the user has no active credential
/// there (fail-closed for the federated-client gate and userinfo).
pub async fn find_federated_identity(
    pool: &PgPool,
    user_id: crate::users::UserId,
    provider_id: crate::auth::providers::ProviderId,
) -> sqlx::Result<Option<FederatedIdentity>> {
    db::find_federated_identity(pool, user_id, provider_id).await
}

/// A user's identity at the provider a client is federated to: the credential
/// `profile` (`{ username, email, first_name, last_name }`) and the provider's
/// email-verification trust flag.
pub struct FederatedIdentity {
    pub profile: serde_json::Value,
    pub provider_email_verified: bool,
}

/// Inserts a client with its redirect URIs, a fixture for other oauth2
/// modules' tests; production creation goes through `create_client`.
#[cfg(test)]
pub async fn insert_client(
    pool: &PgPool,
    id: &str,
    secret: &TokenHash,
    name: &str,
    redirect_uris: &[String],
) -> sqlx::Result<OffsetDateTime> {
    db::insert_client(
        pool,
        id,
        Some(secret),
        name,
        "confidential",
        true,
        false,
        "",
        None,
        redirect_uris,
    )
    .await
}

#[derive(Debug, serde::Serialize)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub client_type: String,
    pub is_pkce_required: bool,
    pub is_email_masked: bool,
    /// Scopes this client is pre-authorized for (empty = always prompt).
    pub auto_consent_scope: Scope,
    /// The provider this client is federated to, or `null` when unrestricted.
    pub identity_provider_slug: Option<crate::Slug>,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
    pub redirect_uris: Vec<String>,
}

#[derive(Debug)]
pub struct ClientSummary {
    pub name: String,
    pub is_pkce_required: bool,
    /// Scopes pre-authorized for this client; a request covered by these skips
    /// the consent screen (see `authorize::prompt`).
    pub auto_consent_scope: Scope,
    /// The id of the provider this client is federated to (gate + identity source),
    /// or `None` when unrestricted.
    pub identity_provider_id: Option<crate::auth::providers::ProviderId>,
    pub redirect_uris: Vec<String>,
}

fn default_pkce_required() -> bool {
    true
}

fn default_client_type() -> String {
    "confidential".to_string()
}

#[derive(Debug, serde::Deserialize)]
pub struct CreatePayload {
    /// An optional admin-supplied client id, e.g. to recreate a deleted client
    /// under its original id. Blank or absent generates a random one; a supplied
    /// value must match the generated 32-char base64url shape (the DB check).
    #[serde(default)]
    id: Option<String>,
    name: String,
    redirect_uris: Vec<String>,
    /// "confidential" (has a secret) or "public" (secret-less, PKCE only).
    #[serde(default = "default_client_type")]
    client_type: String,
    /// Whether a confidential client must use PKCE. Defaults to true (strict
    /// OAuth 2.1); set false for a legacy OAuth 2.0 client. Ignored for public
    /// clients, which always require PKCE.
    #[serde(default = "default_pkce_required")]
    is_pkce_required: bool,
    /// When true, OIDC userinfo presents `<username>@<origin host>` (unverified) to
    /// this client instead of the user's account email.
    #[serde(default)]
    is_email_masked: bool,
    /// An optional admin-supplied secret for a confidential client. Blank or
    /// absent generates a random one (revealed once); rejected for public
    /// clients. Either way only its hash is stored.
    #[serde(default)]
    secret: Option<String>,
    /// Optional space-delimited scope set to pre-authorize: a request whose
    /// scopes are all covered here skips the consent screen (a first-party client
    /// the operator controls). Blank/absent = always prompt.
    #[serde(default)]
    auto_consent_scope: Option<String>,
    /// Optional provider slug to federate this client to: only users with a live
    /// credential there may authorize, and their identity is sourced from that
    /// provider. Blank/absent = unrestricted.
    #[serde(default)]
    identity_provider_slug: Option<String>,
}

/// Bounds for an admin-supplied confidential-client secret. The generated
/// default is 43 base64url characters (256 bits); a hand-picked one must still
/// carry real entropy, the OAuth 2.0 Security BCP (RFC 9700) and RFC 6819 call
/// for high entropy (>=128 bits ~ 22 random chars), and 32 leaves margin for
/// less-than-random input. The stored value is a fixed-length hash regardless.
const MIN_SECRET_LEN: usize = 32;
const MAX_SECRET_LEN: usize = 255;

#[derive(Debug, serde::Serialize)]
pub struct CreatedClient {
    id: String,
    /// The one-time-revealed secret, present only when the server generated it
    /// (a confidential client with no supplied secret). Absent for public
    /// clients and when the admin supplied their own.
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
    name: String,
    client_type: String,
    is_pkce_required: bool,
    is_email_masked: bool,
    auto_consent_scope: Scope,
    identity_provider_slug: Option<crate::Slug>,
    redirect_uris: Vec<String>,
    #[serde(with = "iso8601")]
    created_at: OffsetDateTime,
}

#[derive(Debug, Error, Problem)]
pub enum ClientError {
    #[error("client name out of bounds")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "The 'name' must be between 3 and 31 characters."
    )]
    InvalidName,
    #[error("no redirect URIs")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "At least one 'redirect_uri' is required."
    )]
    NoRedirectUris,
    #[error("invalid redirect URI")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "Each 'redirect_uri' must be a valid absolute URL under 512 characters."
    )]
    InvalidRedirectUri,
    #[error("invalid client_type")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "The 'client_type' must be 'confidential' or 'public'."
    )]
    InvalidClientType,
    #[error("weak secret")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "A supplied 'secret' must be between 32 and 255 characters."
    )]
    WeakSecret,
    #[error("secret on public client")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "A public client cannot have a secret; leave it blank."
    )]
    SecretForPublicClient,
    #[error("invalid client_id")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "A supplied 'client_id' must be 32 characters of letters, digits, '-', or '_'."
    )]
    InvalidClientId,
    #[error("client_id taken")]
    #[problem(
        status = CONFLICT,
        title = "Conflict",
        detail = "A client with that ID already exists."
    )]
    ClientIdTaken,
    #[error("invalid auto-consent scope")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "'auto_consent_scope' must be a space-delimited set of known scopes (openid, profile, email)."
    )]
    InvalidScope,
    #[error("unknown identity provider")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Unknown Identity Provider",
        detail = "'identity_provider_slug' must name an existing OAuth2 provider."
    )]
    UnknownIdentityProvider,
}

pub async fn list_clients(State { pool, .. }: State, _admin: Admin) -> Result<Json<Vec<Client>>> {
    Ok(Json(
        db::list_clients(&pool)
            .await
            .context("listing the OAuth2 clients")?,
    ))
}

pub async fn create_client(
    State { pool, .. }: State,
    _: SameOrigin,
    _admin: Admin,
    Json(payload): Json<CreatePayload>,
) -> Result<Json<CreatedClient>, ClientError> {
    let (name, redirect_uris) = validate(&payload.name, &payload.redirect_uris)?;

    let id = resolve_client_id(payload.id.as_deref())?;
    let (hash, secret, is_pkce_required) = resolve_secret(
        &payload.client_type,
        payload.secret.as_deref(),
        payload.is_pkce_required,
    )?;
    let auto_consent_scope = resolve_auto_consent_scope(payload.auto_consent_scope.as_deref())?;
    let identity_provider =
        resolve_identity_provider(&pool, payload.identity_provider_slug.as_deref()).await?;

    let created_at = db::insert_client(
        &pool,
        &id,
        hash.as_ref(),
        &name,
        &payload.client_type,
        is_pkce_required,
        payload.is_email_masked,
        auto_consent_scope.as_ref(),
        identity_provider.as_ref().map(|(id, _)| id.0),
        &redirect_uris,
    )
    .await
    .map_err(classify_insert)?;

    Ok(Json(CreatedClient {
        id,
        secret,
        name,
        client_type: payload.client_type,
        is_pkce_required,
        is_email_masked: payload.is_email_masked,
        auto_consent_scope,
        identity_provider_slug: identity_provider.map(|(_, slug)| slug),
        redirect_uris,
        created_at,
    }))
}

/// Validates the optional identity-provider slug the admin API sends and resolves
/// it to the provider's id (stored in `identity_provider_id`) plus its canonical
/// slug (echoed back). Blank/absent -> `None` (unrestricted); a value must name an
/// existing SAML or OAuth2 provider. The built-in `local` provider is not a valid
/// federation target (no per-provider credential to source an identity from), so it
/// resolves to nothing and is rejected like any unknown slug.
async fn resolve_identity_provider(
    pool: &PgPool,
    input: Option<&str>,
) -> Result<Option<(crate::auth::providers::ProviderId, crate::Slug)>, ClientError> {
    let Some(raw) = input.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let slug = crate::Slug::try_from(raw)
        .map_err(|_| crate::Error::External(ClientError::UnknownIdentityProvider))?;
    let id = db::find_provider_id_by_slug(pool, &slug)
        .await
        .context("resolving the identity provider slug")?
        .ok_or(crate::Error::External(ClientError::UnknownIdentityProvider))?;
    Ok(Some((id, slug)))
}

/// Validates the optional pre-authorized scope set: blank/absent yields the empty
/// set (always prompt). Beyond the stored `Scope` grammar, every token must name a
/// known scope, so a typo is a clean 422 rather than a silently dead auto-consent.
fn resolve_auto_consent_scope(input: Option<&str>) -> Result<Scope, ClientError> {
    let raw = input.map(str::trim).unwrap_or_default();
    let scope =
        Scope::try_from(raw).map_err(|_| crate::Error::External(ClientError::InvalidScope))?;
    if raw
        .split(' ')
        .filter(|token| !token.is_empty())
        .any(|token| super::lookup_scope(token).is_none())
    {
        return Err(crate::Error::External(ClientError::InvalidScope));
    }
    Ok(scope)
}

/// Validates a client-creation request, returning the trimmed name and the
/// trimmed, de-duplicated redirect URIs.
fn validate(name: &str, redirect_uris: &[String]) -> Result<(String, Vec<String>), ClientError> {
    let name = name.trim();
    let name_len = name.chars().count();
    if !(3..32).contains(&name_len) {
        return Err(crate::Error::External(ClientError::InvalidName));
    }

    if redirect_uris.is_empty() {
        return Err(crate::Error::External(ClientError::NoRedirectUris));
    }
    let mut validated: Vec<String> = Vec::new();
    for uri in redirect_uris {
        let uri = uri.trim();
        if crate::RedirectUri::try_from(uri).is_err() {
            return Err(crate::Error::External(ClientError::InvalidRedirectUri));
        }
        if !validated.iter().any(|existing| existing == uri) {
            validated.push(uri.to_string());
        }
    }
    Ok((name.to_string(), validated))
}

/// Resolves the client id: blank/absent generates a fresh 32-char id; a supplied
/// one must match the generator's shape (32 base64url chars), so it satisfies the
/// `auth_oauth2_clients_id_check` DB constraint and lets an admin recreate a
/// deleted client under its original id.
fn resolve_client_id(id: Option<&str>) -> Result<String, ClientError> {
    match id.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(generate_token::<24>()),
        Some(provided) => {
            let valid = provided.chars().count() == 32
                && provided
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !valid {
                return Err(crate::Error::External(ClientError::InvalidClientId));
            }
            Ok(provided.to_string())
        }
    }
}

/// Maps a duplicate-id primary-key violation to a 409; anything else is internal.
fn classify_insert(error: sqlx::Error) -> crate::Error<ClientError> {
    crate::error::classify_db_error(error, |constraint| match constraint {
        "auth_oauth2_clients_pkey" => Some(ClientError::ClientIdTaken),
        _ => None,
    })
}

fn check_secret_len(secret: &str) -> Result<(), ClientError> {
    if !(MIN_SECRET_LEN..=MAX_SECRET_LEN).contains(&secret.chars().count()) {
        return Err(crate::Error::External(ClientError::WeakSecret));
    }
    Ok(())
}

/// Resolves the secret for a new client into `(stored_hash, revealed_secret,
/// is_pkce_required)`. A confidential client uses the admin-supplied secret
/// (validated, and *not* echoed back since the admin already has it) or a
/// freshly generated one (revealed exactly once). A public client must have no
/// secret and always requires PKCE. A whitespace-only secret counts as absent.
fn resolve_secret(
    client_type: &str,
    secret: Option<&str>,
    is_pkce_required: bool,
) -> Result<(Option<TokenHash>, Option<String>, bool), ClientError> {
    let supplied = secret.map(str::trim).filter(|value| !value.is_empty());
    match client_type {
        "public" => {
            if supplied.is_some() {
                return Err(crate::Error::External(ClientError::SecretForPublicClient));
            }
            Ok((None, None, true))
        }
        "confidential" => match supplied {
            Some(provided) => {
                check_secret_len(provided)?;
                Ok((Some(TokenHash::new(provided)), None, is_pkce_required))
            }
            None => {
                let generated = generate_token::<32>();
                Ok((
                    Some(TokenHash::new(&generated)),
                    Some(generated),
                    is_pkce_required,
                ))
            }
        },
        _ => Err(crate::Error::External(ClientError::InvalidClientType)),
    }
}

pub async fn delete_client(
    State { pool, .. }: State,
    _: SameOrigin,
    _admin: Admin,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    db::delete_client(&pool, &id)
        .await
        .context("deleting the OAuth2 client")?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/", get(list_clients).post(create_client))
        .route("/{id}", delete(delete_client))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::hash_token;

    fn uris(list: &[&str]) -> Vec<String> {
        list.iter().map(|uri| uri.to_string()).collect()
    }

    #[test]
    fn validate_rejects_a_name_outside_the_length_bounds() {
        assert!(matches!(
            validate("ab", &uris(&["https://app.test/cb"])),
            Err(crate::Error::External(ClientError::InvalidName))
        ));
        let too_long = "x".repeat(32);
        assert!(matches!(
            validate(&too_long, &uris(&["https://app.test/cb"])),
            Err(crate::Error::External(ClientError::InvalidName))
        ));
    }

    #[test]
    fn validate_requires_at_least_one_redirect_uri() {
        assert!(matches!(
            validate("Valid Name", &[]),
            Err(crate::Error::External(ClientError::NoRedirectUris))
        ));
    }

    #[test]
    fn validate_rejects_a_non_absolute_redirect_uri() {
        assert!(matches!(
            validate("Valid Name", &uris(&["/relative/path"])),
            Err(crate::Error::External(ClientError::InvalidRedirectUri))
        ));
        assert!(matches!(
            validate("Valid Name", &uris(&["not a url"])),
            Err(crate::Error::External(ClientError::InvalidRedirectUri))
        ));
    }

    #[test]
    fn resolve_secret_generates_and_reveals_when_blank() {
        let (hash, revealed, pkce) = resolve_secret("confidential", None, true).unwrap();
        let revealed = revealed.expect("a generated secret is revealed once");
        assert_eq!(hash.unwrap().as_ref(), hash_token(&revealed));
        assert!(pkce);

        // Whitespace-only is treated as absent, so it still generates.
        let (hash, revealed, _) = resolve_secret("confidential", Some("   "), false).unwrap();
        assert!(hash.is_some() && revealed.is_some());
    }

    #[test]
    fn resolve_secret_stores_a_supplied_secret_without_revealing_it() {
        let supplied = "this-is-a-sufficiently-long-client-secret";
        let (hash, revealed, pkce) = resolve_secret("confidential", Some(supplied), false).unwrap();
        assert_eq!(hash.unwrap().as_ref(), hash_token(supplied));
        assert!(revealed.is_none(), "a supplied secret is never echoed back");
        assert!(!pkce);
    }

    #[test]
    fn resolve_secret_rejects_a_short_supplied_secret() {
        assert!(matches!(
            resolve_secret("confidential", Some("short"), true),
            Err(crate::Error::External(ClientError::WeakSecret))
        ));
    }

    #[test]
    fn resolve_secret_forbids_a_secret_on_a_public_client() {
        assert!(matches!(
            resolve_secret("public", Some("a-sufficiently-long-secret"), true),
            Err(crate::Error::External(ClientError::SecretForPublicClient))
        ));
        // A public client with no secret is fine and forces PKCE on.
        let (hash, revealed, pkce) = resolve_secret("public", None, false).unwrap();
        assert!(hash.is_none() && revealed.is_none() && pkce);
    }

    #[test]
    fn resolve_client_id_generates_when_blank_and_validates_supplied() {
        // Blank/whitespace generates a fresh 32-char base64url id.
        let generated = resolve_client_id(None).unwrap();
        assert_eq!(generated.chars().count(), 32);
        assert!(resolve_client_id(Some("   ")).is_ok());

        // A supplied id matching the generator's shape is accepted verbatim (so a
        // deleted client can be recreated under its original id).
        let original = "AbCd1234-_efGHij5678KLmnOPqrStUv";
        assert_eq!(original.chars().count(), 32);
        assert_eq!(resolve_client_id(Some(original)).unwrap(), original);

        // Wrong length, or right length with an illegal char ('+'), are rejected.
        for bad in ["short", &"x".repeat(33), "AbCd1234+_efGHij5678KLmnOPqrStUv"] {
            assert!(matches!(
                resolve_client_id(Some(bad)),
                Err(crate::Error::External(ClientError::InvalidClientId))
            ));
        }
    }

    #[test]
    fn validate_trims_the_name_and_deduplicates_redirect_uris() {
        let (name, redirect_uris) = validate(
            "  My Client  ",
            &uris(&[
                "https://app.test/cb",
                " https://app.test/cb ",
                "https://app.test/other",
            ]),
        )
        .unwrap();
        assert_eq!(name, "My Client");
        assert_eq!(
            redirect_uris,
            ["https://app.test/cb", "https://app.test/other"]
        );
    }
}
