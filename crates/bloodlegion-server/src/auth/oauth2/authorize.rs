mod db;

use axum::{Json, extract::Query, http::Uri};
use url::Url;

use anyhow::Context;
use thiserror::Error;

use super::ScopeDefinition;
use crate::crypto::{TokenHash, generate_token};
use crate::extract::SameOrigin;
use crate::{
    Problem, Result, State,
    users::{User, UserId},
};

const FRONTEND_AUTHORIZE_PATH: &str = "/oauth2/authorize";

/// Where sign-in happens on the frontend: the front page, whose provider button carries `next`.
const FRONTEND_LOGIN_PATH: &str = "/";

#[derive(Debug, serde::Deserialize)]
pub struct AuthorizeParams {
    response_type: String,
    client_id: String,
    redirect_uri: Option<String>,
    scope: Option<String>,
    state: Option<String>,
    nonce: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ConsentDecision {
    approved: bool,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AuthorizeResponse {
    Redirect {
        url: String,
    },
    Consent {
        client_name: String,
        scopes: Vec<ScopeDefinition>,
    },
}

#[derive(Debug, serde::Serialize)]
pub struct Callback {
    url: String,
}

#[derive(Debug, Error, Problem)]
pub enum AuthorizeError {
    #[error("unsupported response_type")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "The 'response_type' parameter must be 'code'."
    )]
    UnsupportedResponseType,
    #[error("invalid scope")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "The 'scope' parameter contains an unknown or unsupported scope."
    )]
    InvalidScope,
    #[error("unknown client")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "The 'client_id' does not identify a known client."
    )]
    UnknownClient,
    #[error("invalid redirect_uri")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "The 'redirect_uri' does not match a registered redirect URI."
    )]
    InvalidRedirectUri,
    #[error("PKCE required")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "This client requires PKCE: a 'code_challenge' is required."
    )]
    PkceRequired,
    #[error("invalid code_challenge")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "The 'code_challenge' must be a 43-character base64url string sent with 'code_challenge_method=S256'."
    )]
    InvalidCodeChallenge,
    #[error("identity provider connection required")]
    #[problem(
        status = FORBIDDEN,
        title = "Connection Required",
        detail = format!("This application requires a connected '{_0}' account. Sign in with that provider (or connect it from your profile) and try again.")
    )]
    IdentityProviderRequired(String),
}

struct Request {
    client_name: String,
    redirect_uri: String,
    scopes: Vec<&'static ScopeDefinition>,
    /// Scopes pre-authorized for this client: a request they fully cover skips
    /// the consent screen.
    auto_consent_scope: super::Scope,
    /// The id of the provider this client is federated to (gate + identity source),
    /// or `None` when unrestricted.
    identity_provider_id: Option<crate::auth::providers::ProviderId>,
    /// The validated S256 challenge to bind to the code, if PKCE is in use.
    code_challenge: Option<String>,
}

/// Enforces a federated client's provider gate: a signed-in user may authorize
/// only when they have a live credential at the client's `identity_provider_slug`
/// whose profile carries a username (the identity the client will see; an email
/// is optional, since a provider such as Battle.net asserts none).
/// Unrestricted clients (`None`) always pass.
async fn enforce_identity_provider(
    pool: &sqlx::PgPool,
    request: &Request,
    user_id: UserId,
) -> Result<(), AuthorizeError> {
    let Some(provider_id) = request.identity_provider_id else {
        return Ok(());
    };
    let identity = super::clients::find_federated_identity(pool, user_id, provider_id)
        .await
        .context("looking up the user's federated identity")?;
    let complete =
        identity.is_some_and(|identity| profile_str(&identity.profile, "username").is_some());
    if complete {
        return Ok(());
    }
    // Denied, name the provider in the message. This is the rare failure path, so
    // the extra id -> slug lookup is fine.
    let slug = db::provider_ref(pool, provider_id)
        .await
        .context("looking up the federated provider")?
        .map(|(slug, _)| slug.to_string())
        .unwrap_or_default();
    Err(crate::Error::External(
        AuthorizeError::IdentityProviderRequired(slug),
    ))
}

/// The URL to redirect an unauthenticated user to for a federated client: the
/// OAuth2 provider's own login-start endpoint (which then redirects to the
/// IdP), carrying `encoded_next` so the user returns to `/authorize` afterward.
/// Falls back to the login chooser if the slug names no provider (the gate then
/// denies after any login).
async fn provider_login_url(
    pool: &sqlx::PgPool,
    provider_id: crate::auth::providers::ProviderId,
    encoded_next: &str,
) -> Result<String, AuthorizeError> {
    let url = match db::provider_ref(pool, provider_id)
        .await
        .context("looking up the federated provider")?
    {
        Some((slug, kind)) if kind == "oauth2" => {
            format!("/api/auth/oauth2/providers/{slug}?next={encoded_next}")
        }
        _ => format!("{FRONTEND_LOGIN_PATH}?next={encoded_next}"),
    };
    Ok(url)
}

/// A non-empty string field from a credential `profile` jsonb.
fn profile_str<'a>(profile: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    profile
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
}

/// Whether every requested scope is already granted for this client, either
/// pre-authorized on the client (`auto_consent_scope`, a first-party client the
/// operator controls) or previously consented by the user. When true, `prompt`
/// issues the code straight away without showing the consent screen.
fn all_scopes_granted(
    requested: &[&ScopeDefinition],
    auto_consent: &super::Scope,
    user_consent: Option<&super::Scope>,
) -> bool {
    requested.iter().all(|scope| {
        auto_consent.contains(scope.name)
            || user_consent.is_some_and(|granted| granted.contains(scope.name))
    })
}

/// Validates the PKCE parameters and decides whether the flow may proceed.
/// Only S256 is accepted (a missing method defaults to the insecure `plain` per
/// RFC 7636, which we reject). When the client requires PKCE, a challenge must
/// be present.
fn resolve_pkce(
    code_challenge: Option<&str>,
    code_challenge_method: Option<&str>,
    is_pkce_required: bool,
) -> Result<Option<String>, AuthorizeError> {
    match code_challenge {
        Some(challenge) => {
            if code_challenge_method != Some("S256") {
                return Err(crate::Error::External(AuthorizeError::InvalidCodeChallenge));
            }
            // An S256 challenge is exactly 43 chars (base64url SHA-256, unpadded).
            let valid_len = challenge.len() == 43;
            let valid_charset = challenge
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
            if !valid_len || !valid_charset {
                return Err(crate::Error::External(AuthorizeError::InvalidCodeChallenge));
            }
            Ok(Some(challenge.to_string()))
        }
        None => {
            if code_challenge_method.is_some() {
                return Err(crate::Error::External(AuthorizeError::InvalidCodeChallenge));
            }
            if is_pkce_required {
                return Err(crate::Error::External(AuthorizeError::PkceRequired));
            }
            Ok(None)
        }
    }
}

fn parse_scopes(scope: &str) -> Result<Vec<&'static ScopeDefinition>, AuthorizeError> {
    let mut scopes: Vec<&'static ScopeDefinition> = Vec::new();
    for token in scope.split(' ').filter(|token| !token.is_empty()) {
        let scope = super::lookup_scope(token)
            .ok_or(crate::Error::External(AuthorizeError::InvalidScope))?;
        if !scopes.iter().any(|existing| existing.name == scope.name) {
            scopes.push(scope);
        }
    }
    Ok(scopes)
}

fn resolve_redirect_uri(
    requested: &Option<String>,
    registered: &[String],
) -> Result<String, AuthorizeError> {
    match requested {
        None => match registered {
            [only] => Ok(only.clone()),
            _ => Err(crate::Error::External(AuthorizeError::InvalidRedirectUri)),
        },
        Some(uri) => registered
            .iter()
            .find(|registered| *registered == uri)
            .cloned()
            .ok_or(crate::Error::External(AuthorizeError::InvalidRedirectUri)),
    }
}

async fn validate(
    pool: &sqlx::PgPool,
    params: &AuthorizeParams,
) -> Result<Request, AuthorizeError> {
    if params.response_type != "code" {
        return Err(crate::Error::External(
            AuthorizeError::UnsupportedResponseType,
        ));
    }
    let scopes = parse_scopes(params.scope.as_deref().unwrap_or_default())?;
    let client = super::clients::find_client(pool, &params.client_id)
        .await
        .context("looking up the OAuth2 client")?
        .ok_or(crate::Error::External(AuthorizeError::UnknownClient))?;
    let redirect_uri = resolve_redirect_uri(&params.redirect_uri, &client.redirect_uris)?;
    let code_challenge = resolve_pkce(
        params.code_challenge.as_deref(),
        params.code_challenge_method.as_deref(),
        client.is_pkce_required,
    )?;
    Ok(Request {
        client_name: client.name,
        redirect_uri,
        scopes,
        auto_consent_scope: client.auto_consent_scope,
        identity_provider_id: client.identity_provider_id,
        code_challenge,
    })
}

fn scope_string(scopes: &[&ScopeDefinition]) -> super::Scope {
    let joined = scopes
        .iter()
        .map(|scope| scope.name)
        .collect::<Vec<_>>()
        .join(" ");
    super::Scope::try_from(joined.as_str())
        .expect("scope names from the registry form a valid scope set")
}

fn build_callback(
    redirect_uri: &str,
    code: &str,
    state: Option<&str>,
) -> Result<String, AuthorizeError> {
    let mut url = Url::parse(redirect_uri).context("parsing the registered redirect URI")?;
    url.query_pairs_mut().append_pair("code", code);
    if let Some(state) = state {
        url.query_pairs_mut().append_pair("state", state);
    }
    Ok(url.into())
}

fn build_error_callback(
    redirect_uri: &str,
    error: &str,
    state: Option<&str>,
) -> Result<String, AuthorizeError> {
    let mut url = Url::parse(redirect_uri).context("parsing the registered redirect URI")?;
    url.query_pairs_mut().append_pair("error", error);
    if let Some(state) = state {
        url.query_pairs_mut().append_pair("state", state);
    }
    Ok(url.into())
}

async fn issue_code(
    pool: &sqlx::PgPool,
    params: &AuthorizeParams,
    user_id: UserId,
    redirect_uri: &str,
    scope: &super::Scope,
    code_challenge: Option<&str>,
) -> Result<String, AuthorizeError> {
    let code = generate_token::<16>();
    db::insert_authorization_code(
        pool,
        &TokenHash::new(&code),
        &params.client_id,
        user_id,
        redirect_uri,
        scope.as_ref(),
        params.nonce.as_deref(),
        code_challenge,
    )
    .await
    .context("persisting the authorization code")?;
    build_callback(redirect_uri, &code, params.state.as_deref())
}

pub async fn prompt(
    State { pool, .. }: State,
    user: Option<User>,
    uri: Uri,
    Query(params): Query<AuthorizeParams>,
) -> Result<Json<AuthorizeResponse>, AuthorizeError> {
    let request = validate(&pool, &params).await?;

    let Some(user) = user else {
        let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
        let next = urlencoding::encode(&format!("{FRONTEND_AUTHORIZE_PATH}{query}")).into_owned();
        // A federated client sends its users straight to its provider's login
        // (a server-side redirect, no chooser); everyone else to the login page.
        let url = match request.identity_provider_id {
            Some(provider_id) => provider_login_url(&pool, provider_id, &next).await?,
            None => format!("{FRONTEND_LOGIN_PATH}?next={next}"),
        };
        return Ok(Json(AuthorizeResponse::Redirect { url }));
    };

    // A federated client admits only users connected to its provider.
    enforce_identity_provider(&pool, &request, user.id).await?;

    let granted = db::find_consented_scope(&pool, user.id, &params.client_id)
        .await
        .context("looking up the consented scope")?;

    if all_scopes_granted(
        &request.scopes,
        &request.auto_consent_scope,
        granted.as_ref(),
    ) {
        let scope = scope_string(&request.scopes);
        let url = issue_code(
            &pool,
            &params,
            user.id,
            &request.redirect_uri,
            &scope,
            request.code_challenge.as_deref(),
        )
        .await?;
        return Ok(Json(AuthorizeResponse::Redirect { url }));
    }

    Ok(Json(AuthorizeResponse::Consent {
        client_name: request.client_name,
        scopes: request.scopes.into_iter().cloned().collect(),
    }))
}

pub async fn decide(
    State { pool, .. }: State,
    _: SameOrigin,
    user: User,
    Query(params): Query<AuthorizeParams>,
    Json(decision): Json<ConsentDecision>,
) -> Result<Json<Callback>, AuthorizeError> {
    let request = validate(&pool, &params).await?;
    enforce_identity_provider(&pool, &request, user.id).await?;

    if !decision.approved {
        let url = build_error_callback(
            &request.redirect_uri,
            "access_denied",
            params.state.as_deref(),
        )?;
        return Ok(Json(Callback { url }));
    }

    // Persist only what the user explicitly approved, so shrinking a client's
    // auto-consent later re-prompts rather than riding on a stale consent row.
    let approved: Vec<&ScopeDefinition> = request
        .scopes
        .iter()
        .copied()
        .filter(|scope| decision.scopes.iter().any(|name| name == scope.name))
        .collect();
    db::upsert_consent(
        &pool,
        user.id,
        &params.client_id,
        scope_string(&approved).as_ref(),
    )
    .await
    .context("recording the consent")?;

    // Issue for everything now granted: the user's approvals plus the client's
    // pre-authorized scopes, bounded to what was requested.
    let granted: Vec<&ScopeDefinition> = request
        .scopes
        .iter()
        .copied()
        .filter(|scope| {
            approved.iter().any(|entry| entry.name == scope.name)
                || request.auto_consent_scope.contains(scope.name)
        })
        .collect();
    let scope = scope_string(&granted);
    let url = issue_code(
        &pool,
        &params,
        user.id,
        &request.redirect_uri,
        &scope,
        request.code_challenge.as_deref(),
    )
    .await?;
    Ok(Json(Callback { url }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered(uris: &[&str]) -> Vec<String> {
        uris.iter().map(|uri| uri.to_string()).collect()
    }

    #[test]
    fn all_scopes_granted_covers_auto_consent_then_user_consent() {
        use crate::auth::oauth2::{Scope, lookup_scope};
        let requested = [
            lookup_scope("openid").unwrap(),
            lookup_scope("profile").unwrap(),
        ];
        let empty = Scope::try_from("").unwrap();

        // Nothing granted -> the consent screen is shown.
        assert!(!all_scopes_granted(&requested, &empty, None));
        // The client pre-authorizes both -> skip consent.
        let auto_both = Scope::try_from("openid profile").unwrap();
        assert!(all_scopes_granted(&requested, &auto_both, None));
        // Client auto-consents one and the user has consented the other, so skip.
        let auto_one = Scope::try_from("openid").unwrap();
        let user_other = Scope::try_from("profile").unwrap();
        assert!(all_scopes_granted(&requested, &auto_one, Some(&user_other)));
        // Only one covered, nothing for the rest -> still prompt.
        assert!(!all_scopes_granted(&requested, &auto_one, None));
    }

    #[test]
    fn parse_scopes_keeps_known_scopes_in_order_without_duplicates() {
        let scopes = parse_scopes("openid profile openid email").unwrap();
        let names: Vec<_> = scopes.iter().map(|scope| scope.name).collect();
        assert_eq!(names, vec!["openid", "profile", "email"]);
    }

    #[test]
    fn parse_scopes_rejects_an_unknown_scope() {
        assert!(matches!(
            parse_scopes("openid bogus"),
            Err(crate::Error::External(AuthorizeError::InvalidScope))
        ));
    }

    #[test]
    fn parse_scopes_treats_blank_input_as_no_scopes() {
        assert!(parse_scopes("").unwrap().is_empty());
        assert!(parse_scopes("   ").unwrap().is_empty());
    }

    // A syntactically valid S256 challenge (43-char base64url).
    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    #[test]
    fn resolve_pkce_accepts_a_valid_s256_challenge() {
        let resolved = resolve_pkce(Some(CHALLENGE), Some("S256"), true).unwrap();
        assert_eq!(resolved.as_deref(), Some(CHALLENGE));
    }

    #[test]
    fn resolve_pkce_requires_a_challenge_when_the_client_demands_it() {
        assert!(matches!(
            resolve_pkce(None, None, true),
            Err(crate::Error::External(AuthorizeError::PkceRequired))
        ));
        // ...but is optional when the client does not require it.
        assert!(resolve_pkce(None, None, false).unwrap().is_none());
    }

    #[test]
    fn resolve_pkce_rejects_plain_and_malformed_challenges() {
        // 'plain' (and a missing method, which defaults to plain) is refused.
        assert!(matches!(
            resolve_pkce(Some(CHALLENGE), Some("plain"), false),
            Err(crate::Error::External(AuthorizeError::InvalidCodeChallenge))
        ));
        assert!(matches!(
            resolve_pkce(Some(CHALLENGE), None, false),
            Err(crate::Error::External(AuthorizeError::InvalidCodeChallenge))
        ));
        // A method without a challenge is malformed.
        assert!(matches!(
            resolve_pkce(None, Some("S256"), false),
            Err(crate::Error::External(AuthorizeError::InvalidCodeChallenge))
        ));
        // Too short to be a real challenge.
        assert!(matches!(
            resolve_pkce(Some("too-short"), Some("S256"), false),
            Err(crate::Error::External(AuthorizeError::InvalidCodeChallenge))
        ));
    }

    #[test]
    fn resolve_redirect_uri_defaults_to_the_sole_registered_uri() {
        let registered = registered(&["https://app.test/cb"]);
        assert_eq!(
            resolve_redirect_uri(&None, &registered).unwrap(),
            "https://app.test/cb"
        );
    }

    #[test]
    fn resolve_redirect_uri_demands_a_choice_when_several_are_registered() {
        let registered = registered(&["https://a.test/cb", "https://b.test/cb"]);
        assert!(matches!(
            resolve_redirect_uri(&None, &registered),
            Err(crate::Error::External(AuthorizeError::InvalidRedirectUri))
        ));
    }

    #[test]
    fn resolve_redirect_uri_requires_an_exact_registered_match() {
        let registered = registered(&["https://app.test/cb", "https://app.test/other"]);
        assert_eq!(
            resolve_redirect_uri(&Some("https://app.test/other".to_string()), &registered).unwrap(),
            "https://app.test/other"
        );
        assert!(matches!(
            resolve_redirect_uri(&Some("https://evil.test/cb".to_string()), &registered),
            Err(crate::Error::External(AuthorizeError::InvalidRedirectUri))
        ));
    }
}
