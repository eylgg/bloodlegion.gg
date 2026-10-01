use axum::Json;
use axum::http::{HeaderMap, header};
use axum_extra::headers::{Authorization, Header, authorization::Bearer};
use serde_json::{Value, json};

use anyhow::Context;
use thiserror::Error;

use crate::auth::{jwk, jwt};
use crate::{Problem, Result, State};

#[derive(Debug, Error, Problem)]
pub enum UserinfoError {
    #[error("invalid or missing bearer token")]
    #[problem(
        status = UNAUTHORIZED,
        title = "Unauthorized",
        detail = "Valid authentication credentials are required to access this resource."
    )]
    Unauthorized,
}

pub async fn handler(state: State, headers: HeaderMap) -> Result<Json<Value>, UserinfoError> {
    let bearer = headers.get_all(header::AUTHORIZATION);
    let bearer = Authorization::<Bearer>::decode(&mut bearer.iter())
        .map_err(|_| crate::Error::External(UserinfoError::Unauthorized))?;

    // Verify against the published key named by the token's `kid`, so tokens
    // signed by a recently-retired key still validate during a rotation.
    let token = bearer.token();
    let kid = jwt::decode_header(token)
        .ok()
        .and_then(|header| header.kid)
        .ok_or(crate::Error::External(UserinfoError::Unauthorized))?;
    let (n, e) = jwk::find_published_jwk(&state.pool, &kid)
        .await
        .context("looking up the token's signing key")?
        .ok_or(crate::Error::External(UserinfoError::Unauthorized))?;
    let verifying_key = jwt::rsa_verifying_key(&n, &e)
        .map_err(|_| crate::Error::External(UserinfoError::Unauthorized))?;
    let claims = jwt::validate_access_token(&verifying_key, token, &state.origin)
        .map_err(|_| crate::Error::External(UserinfoError::Unauthorized))?;

    let scopes: Vec<&str> = claims.scope.split(' ').filter(|s| !s.is_empty()).collect();
    if !scopes.contains(&super::OPENID) {
        return Err(crate::Error::External(UserinfoError::Unauthorized));
    }

    let user_id = crate::users::UserId(
        claims
            .sub
            .parse()
            .map_err(|_| crate::Error::External(UserinfoError::Unauthorized))?,
    );
    let user = crate::users::find_user_by_id(&state.pool, user_id)
        .await
        .context("looking up the userinfo subject")?
        .ok_or(crate::Error::External(UserinfoError::Unauthorized))?;

    // A federated client presents the identity from its provider credential's
    // profile (e.g. the BattleTag-derived username), not the account. Fail closed
    // if the user is no longer connected there.
    let federated = match super::clients::find_identity_provider_id(&state.pool, &claims.client_id)
        .await
        .context("looking up the client's identity provider")?
    {
        Some(provider_id) => Some(
            super::clients::find_federated_identity(&state.pool, user.id, provider_id)
                .await
                .context("looking up the federated identity")?
                .ok_or(crate::Error::External(UserinfoError::Unauthorized))?,
        ),
        None => None,
    };

    let mut info = serde_json::Map::new();
    info.insert("sub".to_string(), json!(user.id.to_string()));

    if scopes.contains(&super::PROFILE) {
        let (username, given_name, family_name) = match &federated {
            Some(identity) => (
                profile_string(&identity.profile, "username"),
                profile_string(&identity.profile, "first_name"),
                profile_string(&identity.profile, "last_name"),
            ),
            None => (
                Some(user.username.clone()),
                user.first_name.clone(),
                user.last_name.clone(),
            ),
        };
        if let Some(username) = username {
            info.insert("preferred_username".to_string(), json!(username));
        }
        if let Some(given_name) = given_name {
            info.insert("given_name".to_string(), json!(given_name));
        }
        if let Some(family_name) = family_name {
            info.insert("family_name".to_string(), json!(family_name));
        }
    }

    if scopes.contains(&super::EMAIL) {
        // Federated: the provider's asserted email, trusted per that provider's
        // verification flag. Otherwise the account email, optionally masked as
        // `<username>@<origin host>` (a synthetic stand-in, so `email_verified` is
        // false unless the user's verified primary email equals it).
        let (email, email_verified) = if let Some(identity) = &federated {
            (
                profile_string(&identity.profile, "email"),
                identity.provider_email_verified,
            )
        } else if super::clients::client_masks_email(&state.pool, &claims.client_id)
            .await
            .context("checking the client's email-masking setting")?
        {
            let masked = format!("{}@{}", user.username, origin_host(&state.origin));
            let verified = user.is_email_verified
                && user
                    .email
                    .as_deref()
                    .is_some_and(|email| email.eq_ignore_ascii_case(&masked));
            (Some(masked), verified)
        } else {
            (user.email.clone(), user.is_email_verified)
        };
        if let Some(email) = email {
            info.insert("email".to_string(), json!(email));
            info.insert("email_verified".to_string(), json!(email_verified));
        }
    }

    Ok(Json(Value::Object(info)))
}

/// A non-empty string field from a credential `profile` jsonb, cloned.
fn profile_string(profile: &Value, key: &str) -> Option<String> {
    profile
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The host of the configured origin (`https://bloodlegion.gg` -> `bloodlegion.gg`),
/// used as the domain for a masked email. Falls back to the raw origin if it can't
/// be parsed as a URL (a configured origin always can).
fn origin_host(origin: &str) -> String {
    url::Url::parse(origin)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| origin.to_string())
}
