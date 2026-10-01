mod db;

use axum::Json;
use axum::extract::Form;
use axum::http::{HeaderMap, header};
use axum::response::IntoResponse;
use axum_extra::headers::{Authorization, Header, authorization::Basic};
use serde_json::json;

use anyhow::Context;
use thiserror::Error;

use crate::auth::jwt;
use crate::crypto::{TokenHash, generate_token, hash_token};
use crate::users::UserId;
use crate::{Problem, Result, State};

#[derive(Debug, serde::Deserialize)]
pub struct TokenPayload {
    grant_type: String,
    client_id: Option<String>,
    client_secret: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    refresh_token: Option<String>,
    code_verifier: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: i64,
    refresh_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
}

#[derive(Debug, Error, Problem)]
pub enum TokenError {
    #[error("unsupported grant_type")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "The 'grant_type' parameter must be 'authorization_code'.",
        extensions = json!({ "error": "unsupported_grant_type" })
    )]
    UnsupportedGrantType,
    #[error("invalid grant")]
    #[problem(
        status = BAD_REQUEST,
        title = "Bad Request",
        detail = "The authorization code is invalid, expired, or already used.",
        extensions = json!({ "error": "invalid_grant" })
    )]
    InvalidGrant,
    #[error("invalid client")]
    #[problem(
        status = UNAUTHORIZED,
        title = "Unauthorized",
        detail = "Client authentication failed.",
        extensions = json!({ "error": "invalid_client" })
    )]
    InvalidClient,
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let values = headers.get_all(header::AUTHORIZATION);
    let auth = Authorization::<Basic>::decode(&mut values.iter()).ok()?;
    let client_id = urlencoding::decode(auth.username()).ok()?.into_owned();
    let client_secret = urlencoding::decode(auth.password()).ok()?.into_owned();
    Some((client_id, client_secret))
}

pub async fn handler(
    state: State,
    headers: HeaderMap,
    Form(payload): Form<TokenPayload>,
) -> Result<impl IntoResponse, TokenError> {
    // client_id is always required; client_secret is optional, a public client
    // sends none and authenticates by PKCE instead.
    let (client_id, client_secret) = match basic_credentials(&headers) {
        Some((id, secret)) => (id, Some(secret)),
        None => match payload.client_id.clone() {
            Some(client_id) => (client_id, payload.client_secret.clone()),
            None => return Err(crate::Error::External(TokenError::InvalidClient)),
        },
    };
    let client_secret = client_secret.as_deref();

    let response = match payload.grant_type.as_str() {
        "authorization_code" => {
            authorization_code_grant(&state, &payload, &client_id, client_secret).await?
        }
        "refresh_token" => refresh_token_grant(&state, &payload, &client_id, client_secret).await?,
        _ => return Err(crate::Error::External(TokenError::UnsupportedGrantType)),
    };

    let cache_headers = [
        (header::CACHE_CONTROL, "no-store"),
        (header::PRAGMA, "no-cache"),
    ];
    Ok((cache_headers, Json(response)))
}

fn verify_client_secret(provided: &str, secret: &TokenHash) -> Result<(), TokenError> {
    if crate::crypto::ct_eq(TokenHash::new(provided).as_ref(), secret.as_ref()) {
        Ok(())
    } else {
        Err(crate::Error::External(TokenError::InvalidClient))
    }
}

/// Authenticates the client at the token endpoint.
///
/// - **Confidential** (a stored `secret`): must present a matching secret.
/// - **Public** (no stored `secret`): performs no secret authentication, and must
///   not present one, it proves possession via PKCE instead.
fn authenticate_client(
    secret: Option<&TokenHash>,
    provided_secret: Option<&str>,
) -> Result<(), TokenError> {
    match (secret, provided_secret) {
        (Some(secret), Some(provided)) => verify_client_secret(provided, secret),
        (None, None) => Ok(()),
        // confidential client without a secret, or public client presenting one.
        _ => Err(crate::Error::External(TokenError::InvalidClient)),
    }
}

/// Verifies PKCE for the authorization-code grant. When a code was issued with a
/// challenge (S256), the request must carry the matching verifier: S256 means
/// the challenge equals BASE64URL(SHA-256(verifier)), which is exactly what
/// `hash_token` computes. When no challenge was bound (a confidential client
/// without PKCE), there is nothing to check, the `/authorize` endpoint already
/// enforced whether a challenge was required.
fn verify_pkce(
    code_challenge: Option<&str>,
    code_verifier: Option<&str>,
) -> Result<(), TokenError> {
    let Some(challenge) = code_challenge else {
        return Ok(());
    };
    let verifier = code_verifier.ok_or(crate::Error::External(TokenError::InvalidGrant))?;
    if crate::crypto::ct_eq(&hash_token(verifier), challenge) {
        Ok(())
    } else {
        Err(crate::Error::External(TokenError::InvalidGrant))
    }
}

async fn authorization_code_grant(
    state: &State,
    payload: &TokenPayload,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<TokenResponse, TokenError> {
    let code = payload
        .code
        .as_deref()
        .ok_or(crate::Error::External(TokenError::InvalidGrant))?;
    let redirect_uri = payload
        .redirect_uri
        .as_deref()
        .ok_or(crate::Error::External(TokenError::InvalidGrant))?;
    let pool = &state.pool;
    let code = TokenHash::new(code);

    let Some(record) = db::find_authorization_code(pool, &code, client_id, redirect_uri)
        .await
        .context("looking up the authorization code")?
    else {
        // No live grant matched. If this is a replay of a code we already
        // consumed, that signals the code leaked: revoke the token family it
        // minted (RFC 9700 section 4.1.2), then reject either way.
        if let Some(family_id) = db::find_replayed_code_family(pool, &code, client_id)
            .await
            .context("checking for an authorization code replay")?
        {
            db::revoke_refresh_family(pool, &family_id)
                .await
                .context("revoking the token family for a replayed code")?;
            tracing::warn!(
                client_id,
                "authorization code replay detected; revoked token family"
            );
        }
        return Err(crate::Error::External(TokenError::InvalidGrant));
    };
    authenticate_client(record.secret.as_ref(), client_secret)?;
    // A public client (no secret) must have used PKCE, otherwise the code would
    // carry no proof of possession at all.
    if record.secret.is_none() && record.code_challenge.is_none() {
        return Err(crate::Error::External(TokenError::InvalidGrant));
    }
    verify_pkce(
        record.code_challenge.as_deref(),
        payload.code_verifier.as_deref(),
    )?;

    // Generate the family up front so it can be recorded on the code as it is
    // consumed; a later replay then maps back to exactly these tokens.
    let family_id = generate_token::<16>();
    if db::complete_authorization_code(pool, &code, client_id, redirect_uri, &family_id)
        .await
        .context("consuming the authorization code")?
        == 0
    {
        return Err(crate::Error::External(TokenError::InvalidGrant));
    }

    issue_tokens(
        state,
        client_id,
        record.user_id,
        &record.scope,
        &family_id,
        record.nonce.as_deref(),
    )
    .await
}

async fn refresh_token_grant(
    state: &State,
    payload: &TokenPayload,
    client_id: &str,
    client_secret: Option<&str>,
) -> Result<TokenResponse, TokenError> {
    let refresh_token = payload
        .refresh_token
        .as_deref()
        .ok_or(crate::Error::External(TokenError::InvalidGrant))?;
    let pool = &state.pool;

    let auth = db::find_client_auth(pool, client_id)
        .await
        .context("looking up the client")?
        .ok_or(crate::Error::External(TokenError::InvalidClient))?;
    authenticate_client(auth.secret.as_ref(), client_secret)?;

    match db::consume_refresh_token(pool, &TokenHash::new(refresh_token), client_id)
        .await
        .context("consuming the refresh token")?
    {
        db::RefreshOutcome::Valid {
            user_id,
            scope,
            family_id,
        } => issue_tokens(state, client_id, user_id, &scope, &family_id, None).await,
        db::RefreshOutcome::Reused => {
            tracing::warn!(
                client_id,
                "refresh token reuse detected; revoked token family"
            );
            Err(crate::Error::External(TokenError::InvalidGrant))
        }
        db::RefreshOutcome::Invalid => Err(crate::Error::External(TokenError::InvalidGrant)),
    }
}

async fn issue_tokens(
    state: &State,
    client_id: &str,
    user_id: UserId,
    scope: &super::Scope,
    family_id: &str,
    nonce: Option<&str>,
) -> Result<TokenResponse, TokenError> {
    let signer = state.active_signer();
    let id_token = if scope.contains(super::OPENID) {
        Some(jwt::generate_id_token(
            &state.origin,
            &signer.kid,
            &signer.key,
            user_id,
            client_id,
            nonce,
        )?)
    } else {
        None
    };

    let jti = generate_token::<32>();
    let access_token = jwt::generate_access_token(
        &state.origin,
        &signer.kid,
        &signer.key,
        &jti,
        user_id,
        client_id,
        scope.as_ref(),
    )?;

    let refresh_token = generate_token::<32>();
    db::insert_refresh_token(
        &state.pool,
        &TokenHash::new(&refresh_token),
        family_id,
        client_id,
        user_id,
        scope,
    )
    .await
    .context("storing the refresh token")?;

    Ok(TokenResponse {
        access_token,
        token_type: "Bearer",
        expires_in: jwt::ACCESS_TOKEN_LIFETIME.whole_seconds(),
        refresh_token,
        id_token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticate_client_confidential_requires_a_matching_secret() {
        let hash = TokenHash::new("s3cret");
        assert!(authenticate_client(Some(&hash), Some("s3cret")).is_ok());
        assert!(matches!(
            authenticate_client(Some(&hash), Some("wrong")),
            Err(crate::Error::External(TokenError::InvalidClient))
        ));
        // a confidential client must present its secret
        assert!(matches!(
            authenticate_client(Some(&hash), None),
            Err(crate::Error::External(TokenError::InvalidClient))
        ));
    }

    #[test]
    fn authenticate_client_public_takes_no_secret() {
        assert!(authenticate_client(None, None).is_ok());
        // a public client must not present a secret
        assert!(matches!(
            authenticate_client(None, Some("unexpected")),
            Err(crate::Error::External(TokenError::InvalidClient))
        ));
    }

    #[test]
    fn verify_pkce_passes_when_no_challenge_was_bound() {
        assert!(verify_pkce(None, None).is_ok());
        assert!(verify_pkce(None, Some("ignored")).is_ok());
    }

    #[test]
    fn verify_pkce_matches_the_s256_verifier() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        // S256 challenge = BASE64URL(SHA-256(verifier)) = hash_token(verifier).
        let challenge = hash_token(verifier);
        assert!(verify_pkce(Some(&challenge), Some(verifier)).is_ok());
    }

    #[test]
    fn verify_pkce_rejects_a_wrong_or_missing_verifier() {
        let challenge = hash_token("the-real-verifier");
        assert!(matches!(
            verify_pkce(Some(&challenge), Some("the-wrong-verifier")),
            Err(crate::Error::External(TokenError::InvalidGrant))
        ));
        assert!(matches!(
            verify_pkce(Some(&challenge), None),
            Err(crate::Error::External(TokenError::InvalidGrant))
        ));
    }
}
