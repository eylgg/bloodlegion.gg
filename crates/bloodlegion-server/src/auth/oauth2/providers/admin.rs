//! Admin-side OAuth2/OIDC provider registration: validates the create payload
//! (slug, issuer/endpoints per OIDC-or-not, scopes, claim mapping), encrypts the
//! client secret, and writes the provider row (plus a non-OIDC provider's
//! admin-supplied endpoints) in one transaction.

use std::collections::HashSet;

use anyhow::Context;
use thiserror::Error;

use crate::{Problem, Result, Slug, State};

use super::api::{ClaimInput, CreateProviderPayload};
use super::{Oauth2Extra, Oauth2Target, db, default_claim_mapping};

/// The scopes requested when a provider is created without an explicit set.
const DEFAULT_SCOPES: &str = "openid email profile";

#[derive(Debug, Error, Problem)]
pub(crate) enum CreateError {
    #[error("invalid slug")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Slug",
        detail = "The slug must start with a lowercase letter and contain only lowercase letters, digits, and dashes (3-31 characters)."
    )]
    InvalidSlug,
    #[error("reserved slug")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Reserved Slug",
        detail = "That slug is reserved for a built-in login method and cannot be used for a provider."
    )]
    SlugReserved,
    #[error("invalid name")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Name",
        detail = "The name must be between 3 and 31 characters."
    )]
    InvalidName,
    #[error("invalid issuer")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Issuer",
        detail = "The issuer must be a valid absolute https URL under 512 characters."
    )]
    InvalidIssuer,
    #[error("issuer not allowed")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Issuer Not Allowed",
        detail = "A non-OIDC provider must not have an issuer (it has no discovery document)."
    )]
    IssuerNotAllowed,
    #[error("invalid endpoint")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Endpoint",
        detail = "Each endpoint must be a valid absolute https URL under 512 characters."
    )]
    InvalidEndpoint,
    #[error("endpoints not allowed")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Endpoints Not Allowed",
        detail = "An OIDC provider must not set endpoints; they come from its discovery document."
    )]
    EndpointsNotAllowed,
    #[error("missing field `{0}`")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Missing Field",
        detail = format!("The '{_0}' is required.")
    )]
    MissingField(&'static str),
    #[error("slug taken")]
    #[problem(
        status = CONFLICT,
        title = "Slug Taken",
        detail = "An OAuth2 provider with that slug already exists."
    )]
    SlugTaken,
    #[error("issuer taken")]
    #[problem(
        status = CONFLICT,
        title = "Issuer Taken",
        detail = "An OAuth2 provider for that issuer already exists."
    )]
    IssuerTaken,
    #[error("invalid claim mapping")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Claim Mapping",
        detail = "Each claim mapping entry needs a non-empty claim and target."
    )]
    InvalidClaim,
    #[error("duplicate claim `{0}`")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Duplicate Claim",
        detail = format!("The claim '{_0}' is mapped more than once.")
    )]
    DuplicateClaim(String),
    #[error("duplicate target `{0}`")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Duplicate Target",
        detail = format!("The target '{_0}' is mapped more than once.")
    )]
    DuplicateTarget(String),
    #[error("missing subject mapping")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Missing Subject Mapping",
        detail = "A non-OIDC provider must map a userinfo field to 'subject' (e.g. Discord's 'id')."
    )]
    MissingSubjectMapping,
    #[error("invalid scopes")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Scopes",
        detail = "Scopes must be a space-delimited list (no spaces, quotes, or \
                  backslashes within a token) of at most 512 characters."
    )]
    InvalidScopes,
    #[error("oidc scope required")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "OIDC Scope Required",
        detail = "An OIDC provider's scopes must include 'openid'."
    )]
    OidcScopeRequired,
    #[error("provider in use")]
    #[problem(
        status = CONFLICT,
        title = "Provider In Use",
        detail = "A client is federated to this provider; remove that federation before deleting it."
    )]
    ProviderInUse,
    #[error("provider not found")]
    #[problem(
        status = NOT_FOUND,
        title = "Provider Not Found",
        detail = "No OAuth2 provider has that slug."
    )]
    NotFound,
}

/// One RFC 6749 scope token: 1*( %x21 / %x23-5B / %x5D-7E ), printable ASCII
/// excluding space (the delimiter), `"`, and `\`.
fn is_scope_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|b| b == 0x21 || (0x23..=0x5B).contains(&b) || (0x5D..=0x7E).contains(&b))
}

/// Resolves the scopes string: blank falls back to the standard OIDC set;
/// otherwise it is trimmed, then format- and length-checked (mirroring the DB
/// CHECK, so a bad value is a clean 422 rather than a 500). An OIDC provider must
/// include openid; a plain OAuth2 upstream (e.g. Discord) requests its own scopes.
fn validate_scopes(scopes: Option<&str>, is_oidc: bool) -> Result<String, CreateError> {
    let Some(scopes) = scopes.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(DEFAULT_SCOPES.to_string());
    };
    let well_formed = scopes.split(' ').all(is_scope_token);
    if !well_formed || scopes.len() > 512 {
        return Err(crate::Error::External(CreateError::InvalidScopes));
    }
    if is_oidc && !scopes.split(' ').any(|token| token == "openid") {
        return Err(crate::Error::External(CreateError::OidcScopeRequired));
    }
    Ok(scopes.to_string())
}

/// The snake_case name of a target (its serde form), for error messages.
fn target_label(target: Oauth2Target) -> String {
    serde_json::to_value(target)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Validates the claim mapping and builds the `claims` jsonb. Enforces unique claims and
/// unique known targets. For an OIDC provider an empty mapping falls back to the standard OIDC
/// one and the subject is always the id_token `sub`; a custom mapping is how a provider that
/// asserts no `preferred_username` (Battle.net: only a BattleTag, from userinfo) names its users.
/// A plain OAuth2 provider has no id_token, so it must spell out the mapping, including a
/// `subject` (its stable account id). No mapping has to supply an email: an account may have none.
fn validate_claims(claims: &[ClaimInput], is_oidc: bool) -> Result<serde_json::Value, CreateError> {
    if claims.is_empty() {
        if is_oidc {
            return Ok(default_claim_mapping());
        }
        return Err(crate::Error::External(CreateError::MissingSubjectMapping));
    }
    // serde already parsed each `target` into an `Oauth2Target`, so an unknown
    // target is rejected at the request boundary; this only checks the semantic
    // rules. A claim with no target is carried but unmapped.
    let mut seen_claims: HashSet<&str> = HashSet::new();
    let mut seen_targets: HashSet<Oauth2Target> = HashSet::new();
    let mut elements = Vec::with_capacity(claims.len());
    for entry in claims {
        let claim = entry.claim.trim();
        if claim.is_empty() {
            return Err(crate::Error::External(CreateError::InvalidClaim));
        }
        if !seen_claims.insert(claim) {
            return Err(crate::Error::External(CreateError::DuplicateClaim(
                claim.to_string(),
            )));
        }
        if let Some(target) = entry.target
            && !seen_targets.insert(target)
        {
            return Err(crate::Error::External(CreateError::DuplicateTarget(
                target_label(target),
            )));
        }
        elements.push(serde_json::json!({
            "claim": claim,
            "target": entry.target,
            "normalize": entry.normalize,
        }));
    }
    if !is_oidc && !seen_targets.contains(&Oauth2Target::Extra(Oauth2Extra::Subject)) {
        return Err(crate::Error::External(CreateError::MissingSubjectMapping));
    }
    Ok(serde_json::Value::Array(elements))
}

fn validate(payload: &CreateProviderPayload) -> Result<Slug, CreateError> {
    let slug = Slug::try_from(payload.slug.trim())
        .map_err(|_| crate::Error::External(CreateError::InvalidSlug))?;
    if crate::auth::is_reserved_provider_slug(&slug) {
        return Err(crate::Error::External(CreateError::SlugReserved));
    }
    let name_len = payload.name.trim().chars().count();
    if !(3..32).contains(&name_len) {
        return Err(crate::Error::External(CreateError::InvalidName));
    }
    // issuer is required and validated for OIDC, and must be absent otherwise
    // (mirrors the DB issuer_oidc check, so a bad value is a 422 not a 500).
    let issuer = payload
        .issuer
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match (payload.is_oidc, issuer) {
        (true, Some(issuer)) => {
            if crate::HttpsUrl::try_from(issuer).is_err() {
                return Err(crate::Error::External(CreateError::InvalidIssuer));
            }
        }
        (true, None) => return Err(crate::Error::External(CreateError::InvalidIssuer)),
        (false, Some(_)) => return Err(crate::Error::External(CreateError::IssuerNotAllowed)),
        (false, None) => {}
    }
    if payload.client_id.trim().is_empty() {
        return Err(crate::Error::External(CreateError::MissingField(
            "client_id",
        )));
    }
    if payload.client_secret.is_empty() {
        return Err(crate::Error::External(CreateError::MissingField(
            "client_secret",
        )));
    }
    // Endpoints are required (and validated) for a non-OIDC provider and forbidden
    // for OIDC, which discovers them. Mirrors the metadata trigger, so a bad value
    // is a clean 422 rather than a 500.
    let endpoints = [
        (
            "authorization_endpoint",
            payload.authorization_endpoint.as_deref(),
        ),
        ("token_endpoint", payload.token_endpoint.as_deref()),
        ("userinfo_endpoint", payload.userinfo_endpoint.as_deref()),
    ];
    if payload.is_oidc {
        if endpoints
            .iter()
            .any(|(_, value)| value.map(str::trim).is_some_and(|value| !value.is_empty()))
        {
            return Err(crate::Error::External(CreateError::EndpointsNotAllowed));
        }
    } else {
        for (field, value) in endpoints {
            let value = value
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or(crate::Error::External(CreateError::MissingField(field)))?;
            if crate::HttpsUrl::try_from(value).is_err() {
                return Err(crate::Error::External(CreateError::InvalidEndpoint));
            }
        }
    }
    Ok(slug)
}

/// Maps a duplicate-slug unique violation to 409; anything else is internal.
fn classify_insert(error: sqlx::Error) -> crate::Error<CreateError> {
    crate::error::classify_db_error(error, |constraint| match constraint {
        "auth_providers_slug_key" => Some(CreateError::SlugTaken),
        "auth_oauth2_providers_issuer_key" => Some(CreateError::IssuerTaken),
        _ => None,
    })
}

/// Maps the identity-provider FK RESTRICT violation (a client still federated to
/// this provider) to a 409; anything else is internal.
pub(super) fn classify_delete(error: sqlx::Error) -> crate::Error<CreateError> {
    crate::error::classify_db_error(error, |constraint| match constraint {
        "auth_oauth2_clients_identity_provider_id_fkey" => Some(CreateError::ProviderInUse),
        _ => None,
    })
}

/// Registers an OAuth2 identity provider: validates the payload, encrypts the
/// client secret, and writes the provider row (plus, for a non-OIDC provider, its
/// admin-supplied endpoints) in one transaction.
pub(super) async fn create_provider(
    state: &State,
    payload: CreateProviderPayload,
) -> Result<(), CreateError> {
    let slug = validate(&payload)?;
    let claims = validate_claims(&payload.claims, payload.is_oidc)?;
    let scopes = validate_scopes(payload.scope.as_deref(), payload.is_oidc)?;
    let issuer = payload
        .issuer
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let client_secret = state
        .encrypt(&payload.client_secret)
        .context("encrypting the client secret")?;

    let mut tx = state
        .pool
        .begin()
        .await
        .context("beginning the provider transaction")?;
    let provider_id = db::insert_provider(
        &mut tx,
        &slug,
        payload.name.trim(),
        issuer,
        payload.client_id.trim(),
        &client_secret,
        crate::auth::ProviderFlags {
            is_registration_allowed: payload.is_registration_allowed,
            is_auto_connection_allowed: payload.is_auto_connection_allowed,
            is_email_verified: payload.is_email_verified,
            is_disconnection_allowed: payload.is_disconnection_allowed,
            is_unclaimed_username_connection_allowed: payload
                .is_unclaimed_username_connection_allowed,
        },
        &claims,
        &scopes,
        payload.is_oidc,
    )
    .await
    .map_err(classify_insert)?;

    if !payload.is_oidc {
        // A non-OIDC provider's endpoints are admin-provided (no discovery), so its
        // metadata row is written now, in the same transaction. validate() has
        // already ensured the required endpoints are present and well-formed.
        let authorization_endpoint = payload
            .authorization_endpoint
            .as_deref()
            .unwrap_or_default()
            .trim();
        let token_endpoint = payload.token_endpoint.as_deref().unwrap_or_default().trim();
        let userinfo_endpoint = payload.userinfo_endpoint.as_deref().map(str::trim);
        db::upsert_metadata(
            &mut *tx,
            provider_id,
            authorization_endpoint,
            token_endpoint,
            userinfo_endpoint,
            None,
            None,
            None,
        )
        .await
        .context("storing the provider endpoints")?;
    }

    tx.commit().await.context("committing the new provider")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Target;

    fn claim(claim: &str, target: Oauth2Target) -> ClaimInput {
        ClaimInput {
            claim: claim.into(),
            target: Some(target),
            normalize: false,
        }
    }

    /// A minimal valid OIDC create payload with the given slug.
    fn oidc_payload(slug: &str) -> CreateProviderPayload {
        CreateProviderPayload {
            slug: slug.into(),
            name: "Example".into(),
            issuer: Some("https://issuer.test".into()),
            client_id: "client".into(),
            client_secret: "secret".into(),
            is_registration_allowed: false,
            is_auto_connection_allowed: false,
            is_email_verified: false,
            is_disconnection_allowed: false,
            is_unclaimed_username_connection_allowed: false,
            is_oidc: true,
            authorization_endpoint: None,
            token_endpoint: None,
            userinfo_endpoint: None,
            claims: vec![],
            scope: None,
        }
    }

    #[test]
    fn validate_rejects_the_reserved_local_slug() {
        assert!(matches!(
            validate(&oidc_payload("local")),
            Err(crate::Error::External(CreateError::SlugReserved))
        ));
        // A non-reserved slug clears the slug checks.
        assert!(validate(&oidc_payload("google")).is_ok());
    }

    #[test]
    fn validate_claims_builds_the_mapping_jsonb() {
        let claims = vec![
            claim("id", Oauth2Target::Extra(Oauth2Extra::Subject)),
            claim("mail", Oauth2Target::Common(Target::Email)),
            claim("first", Oauth2Target::Common(Target::FirstName)),
        ];
        let value = validate_claims(&claims, false).unwrap();
        let elements = value.as_array().unwrap();
        // Each target is stored as its flat snake_case name, wrapper and all.
        assert!(
            elements
                .iter()
                .any(|element| element["target"] == "subject")
        );
        assert!(elements.iter().any(|element| element["target"] == "email"));
        assert!(
            elements
                .iter()
                .any(|element| element["target"] == "first_name")
        );
    }

    #[test]
    fn validate_claims_requires_a_subject_for_plain_oauth2_only() {
        // Missing the subject mapping a non-OIDC provider needs.
        let no_subject = vec![claim("mail", Oauth2Target::Common(Target::Email))];
        assert!(matches!(
            validate_claims(&no_subject, false),
            Err(crate::Error::External(CreateError::MissingSubjectMapping))
        ));
        // An email mapping is optional: an account may have no email.
        let no_email = vec![claim("id", Oauth2Target::Extra(Oauth2Extra::Subject))];
        assert!(validate_claims(&no_email, false).is_ok());
        // OIDC takes its subject from the id_token, so a custom mapping needs none, and an
        // empty one falls back to the standard OIDC mapping.
        let battlenet = vec![ClaimInput {
            claim: "battletag".into(),
            target: Some(Oauth2Target::Common(Target::Username)),
            normalize: true,
        }];
        let value = validate_claims(&battlenet, true).unwrap();
        assert_eq!(value[0]["normalize"], true);
        assert_eq!(validate_claims(&[], true).unwrap(), default_claim_mapping());
    }
}
