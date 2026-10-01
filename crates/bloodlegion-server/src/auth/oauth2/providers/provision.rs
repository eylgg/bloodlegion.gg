//! Callback-side identity resolution and user provisioning: verifies an OIDC
//! id_token (or reads the subject from a plain-OAuth2 userinfo), translates the
//! claims into a profile via the provider's mapping, and resolves or registers the
//! local account. The `api::callback` handler drives this after the token exchange;
//! when the provider asserts no usable username, registration finishes in
//! `register::choose`, after the person has picked one.

use anyhow::Context;
use sqlx::PgPool;

use crate::Result;
use crate::error::ErrorExt;
use crate::users::{CreateUserPayload, UserId, Username};

use super::{ProviderError, db, http};

/// Translates the raw claims into the user's profile (`{ target: value }`) via the
/// provider's operator-configured claim->target mapping, the counterpart to the raw
/// `userinfo`. Every target is single-valued, so a claim keeps its native scalar (so
/// `is_email_verified` stays a bool) and an array is refused as a mapping or IdP
/// mistake. `subject` is the protocol identity, stored in its own column, so it is
/// never a profile field. A `username` entry flagged `normalize` is coerced into the
/// username shape on the way in.
pub(super) fn build_profile(
    provider: &db::Provider,
    userinfo: &serde_json::Value,
) -> Result<serde_json::Value, ProviderError> {
    let configured = provider
        .claims
        .as_array()
        .with_context(|| format!("provider {} claims is not a JSON array", provider.id))?;
    let mut profile = serde_json::Map::new();
    for element in configured {
        let (Some(claim), Some(target_name)) = (
            element.get("claim").and_then(serde_json::Value::as_str),
            element.get("target").and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        let target: super::Oauth2Target =
            serde_json::from_value(serde_json::Value::String(target_name.to_owned()))
                .with_context(|| format!("provider {} has an invalid claim target", provider.id))?;
        // `subject` is the credential identity (its own column), not a profile field.
        if target == super::Oauth2Target::Extra(super::Oauth2Extra::Subject) {
            continue;
        }
        let normalize = element
            .get("normalize")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        // A single-valued target keeps the claim's native scalar type; an array is
        // a mapping or IdP mistake, so surface it rather than guessing.
        match userinfo.get(claim).filter(|value| !value.is_null()) {
            Some(serde_json::Value::Array(_)) => {
                return Err(crate::Error::Internal(anyhow::anyhow!(
                    "OIDC claim `{claim}` maps to a single-valued target but returned a list"
                )));
            }
            Some(serde_json::Value::String(value))
                if normalize
                    && target == super::Oauth2Target::Common(crate::auth::Target::Username) =>
            {
                profile.insert(
                    target_name.to_string(),
                    serde_json::Value::String(normalize_username(value)),
                );
            }
            Some(scalar) => {
                profile.insert(target_name.to_string(), scalar.clone());
            }
            None => {}
        }
    }
    Ok(serde_json::Value::Object(profile))
}

/// Coerces an upstream handle into the username shape (a letter, then letters and digits,
/// at most 32): everything else (a BattleTag's `#`, spaces, punctuation, accented letters)
/// is dropped and leading digits trimmed, so `Name#1234` becomes `Name1234`. Case is kept,
/// since the stored username is the displayed one and its lowercase form is derived. The
/// result still goes through `create_user`'s `Username` gate; a value this cannot rescue
/// (nothing left) is simply not a username, and the person picks one.
pub(super) fn normalize_username(raw: &str) -> String {
    let kept: String = raw.chars().filter(char::is_ascii_alphanumeric).collect();
    let start = kept
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(kept.len());
    kept[start..].chars().take(32).collect()
}

/// Claims an IdP might carry a human handle in, in order of preference. Heuristic only, for
/// the username suggestion and the "signing in as" label on the registration page: the
/// account identity is the subject, never one of these.
const HANDLE_CLAIMS: &[&str] = &[
    "preferred_username",
    "battletag",
    "battle_tag",
    "nickname",
    "login",
    "name",
];

/// How the provider identifies the person, as a label (a BattleTag, a nickname), if it
/// asserted anything of the kind.
pub(super) fn identity_label(userinfo: &serde_json::Value) -> Option<String> {
    HANDLE_CLAIMS.iter().find_map(|key| {
        userinfo
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

/// A username to prefill on the registration page: the mapped username when the provider
/// maps one, else the handle, either coerced into the username shape. `None` when nothing
/// usable comes out, and the field starts empty.
pub(super) fn suggest_username(
    profile: &serde_json::Value,
    userinfo: &serde_json::Value,
) -> Option<String> {
    let candidate = profile
        .get("username")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| identity_label(userinfo))?;
    let normalized = normalize_username(&candidate);
    Username::try_from(normalized.as_str())
        .ok()
        .map(|username| username.to_string())
}

/// Merges the userinfo document over the verified id_token claims, so a mapping may name a
/// claim that an IdP only exposes at its userinfo endpoint (Battle.net puts the BattleTag
/// there). Both come from the same token exchange, so a conflict would be the IdP disagreeing
/// with itself; userinfo wins so the mapping sees the fuller document.
pub(super) fn merge_userinfo(
    id_token_claims: serde_json::Value,
    userinfo: serde_json::Value,
) -> serde_json::Value {
    let mut merged = match id_token_claims {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    if let serde_json::Value::Object(extra) = userinfo {
        for (key, value) in extra {
            merged.insert(key, value);
        }
    }
    serde_json::Value::Object(merged)
}

/// Resolves the upstream subject from userinfo via the provider's claim mapping
/// (the entry whose target is `subject`), for non-OIDC providers that have no
/// id_token `sub`. Accepts a string or number field (e.g. Discord/GitHub `id`).
pub(super) fn extract_subject(
    provider: &db::Provider,
    userinfo: &serde_json::Value,
) -> Result<String, ProviderError> {
    let configured = provider
        .claims
        .as_array()
        .with_context(|| format!("provider {} claims is not a JSON array", provider.id))?;
    let claim = configured
        .iter()
        .find(|element| {
            element.get("target").and_then(serde_json::Value::as_str) == Some("subject")
        })
        .and_then(|element| element.get("claim").and_then(serde_json::Value::as_str))
        .context("non-OIDC provider has no subject claim mapping")?;
    match userinfo.get(claim) {
        Some(serde_json::Value::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(serde_json::Value::Number(value)) => Ok(value.to_string()),
        _ => Err(crate::Error::External(ProviderError::SubjectMissing)),
    }
}

/// The outcome of resolving a verified upstream identity.
pub(super) enum Provisioned {
    /// The identity resolved to, linked to, or registered this account.
    User(UserId),
    /// Registration is allowed but the provider asserted no usable username (none, or one that
    /// is malformed, reserved, or taken), so the person chooses one before the account exists.
    NeedsUsername,
}

#[cfg(test)]
impl Provisioned {
    /// The account, for tests that expect one.
    pub(super) fn user(self) -> UserId {
        match self {
            Self::User(id) => id,
            Self::NeedsUsername => panic!("expected an account, but a username is needed"),
        }
    }
}

/// Whether the asserted email counts as verified: the operator must trust this provider (its
/// `is_email_verified` flag) *and* the upstream must assert the address is verified (the
/// `email_verified` claim, mapped to the `is_email_verified` target). An unverified address is
/// still usable to register (stored unverified), it just cannot link to an existing account.
pub(super) fn email_is_verified(provider: &db::Provider, profile: &serde_json::Value) -> bool {
    provider.is_email_verified
        && profile
            .get("is_email_verified")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
}

pub(super) async fn provision_user(
    pool: &PgPool,
    provider: &db::Provider,
    subject: &str,
    userinfo: &serde_json::Value,
) -> Result<Provisioned, ProviderError> {
    if let Some(user_id) = db::find_user_by_credential(pool, provider.id, subject)
        .await
        .context("looking up the provider credential")?
    {
        return Ok(Provisioned::User(user_id));
    }

    // The credential stores the raw userinfo and the translated profile together;
    // the fields below are read from the profile, not the raw token.
    let profile = build_profile(provider, userinfo)?;

    // The email is optional: Battle.net asserts none, and an account may have none.
    let email = profile.get("email").and_then(serde_json::Value::as_str);

    // When this provider's asserted username is authoritative, a first login may claim an
    // *unclaimed* account (no linked identity) by matching that username to the account username,
    // instead of by email. This heals the case where the account's email differs from the IdP's.
    // Gated to unclaimed accounts, so it can never connect to an account someone has already
    // claimed.
    let username = profile
        .get("username")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if provider.is_unclaimed_username_connection_allowed && !username.is_empty() {
        let by_username = crate::users::find_user_id_by_username(pool, username)
            .await
            .context("looking up a user by the OIDC username")?;
        if let Some(user_id) = by_username
            && !db::has_credentials(pool, user_id)
                .await
                .context("checking whether the account is claimed")?
        {
            // Fill only valid names; an over-long IdP name is skipped rather than failing the login
            // (the claim writes directly, bypassing create_user's Name gate).
            let valid_name = |key: &str| {
                profile
                    .get(key)
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| {
                        !value.is_empty() && crate::users::Name::try_from(*value).is_ok()
                    })
                    .map(String::from)
            };
            let first_name = valid_name("first_name");
            let last_name = valid_name("last_name");
            let mut tx = pool
                .begin()
                .await
                .context("beginning a transaction to claim an account by username")?;
            db::insert_credential(&mut tx, provider.id, subject, user_id, userinfo, &profile)
                .await
                .context("inserting the OIDC credential for the claimed account")?;
            db::update_user_names_if_missing(
                &mut tx,
                user_id,
                first_name.as_deref(),
                last_name.as_deref(),
            )
            .await
            .context("filling in missing user names on claim")?;
            tx.commit()
                .await
                .context("committing the claimed OIDC credential")?;
            // Record the IdP-asserted email on the account, healing the mismatch that sent us down
            // the username path. Non-fatal.
            if let Some(email) = email {
                match crate::users::add_verified_email(pool, user_id, email).await {
                    Ok(Some(other)) => tracing::warn!(
                        user_id = user_id.0,
                        other_user_id = other.0,
                        oidc_email = email,
                        "the OIDC email belongs to a different account; left in place"
                    ),
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(user_id = user_id.0, %error, "could not record the OIDC email")
                    }
                }
            }
            return Ok(Provisioned::User(user_id));
        }
    }

    if email_is_verified(provider, &profile)
        && let Some(email) = email
        && let Some(user_id) = db::find_user_by_verified_email(pool, email)
            .await
            .context("looking up a user by verified email")?
    {
        if !provider.is_auto_connection_allowed {
            return Err(crate::Error::External(ProviderError::EmailInUse));
        }
        let mut conn = pool.acquire().await.context("acquiring a connection")?;
        db::insert_credential(&mut conn, provider.id, subject, user_id, userinfo, &profile)
            .await
            .context("linking the provider credential")?;
        crate::users::mark_email_federated(&mut conn, user_id, email)
            .await
            .context("marking the linked OIDC email federated")?;
        return Ok(Provisioned::User(user_id));
    }

    if !provider.is_registration_allowed {
        return Err(crate::Error::External(ProviderError::RegistrationDisabled));
    }

    // A mapped username that is valid and free registers the account straight away. One that
    // is missing, malformed (a raw BattleTag), reserved, or taken sends the person to choose
    // their own rather than failing the login.
    if !username.is_empty() {
        match register(pool, provider, subject, userinfo, &profile, username).await {
            Ok(user_id) => return Ok(Provisioned::User(user_id)),
            Err(crate::Error::External(ProviderError::CreateUser(error))) => {
                tracing::debug!(%error, "the mapped username is unusable; the person chooses one");
            }
            Err(error) => return Err(error),
        }
    }
    Ok(Provisioned::NeedsUsername)
}

/// Creates the account for a first login, the user row (with the asserted email, if any,
/// marked federated) and the provider credential, in one transaction. `username` is either the
/// mapped one or the one the person chose; it goes through `create_user`'s `Username` gate, so
/// a malformed, reserved, or taken name surfaces as [`ProviderError::CreateUser`].
pub(super) async fn register(
    pool: &PgPool,
    provider: &db::Provider,
    subject: &str,
    userinfo: &serde_json::Value,
    profile: &serde_json::Value,
    username: &str,
) -> Result<UserId, ProviderError> {
    let email = profile.get("email").and_then(serde_json::Value::as_str);
    let payload = CreateUserPayload {
        username: username.to_string(),
        email: email.map(str::to_string),
        first_name: provider_name(
            profile
                .get("first_name")
                .and_then(serde_json::Value::as_str),
        ),
        last_name: provider_name(profile.get("last_name").and_then(serde_json::Value::as_str)),
        is_superuser: false,
    };
    let mut tx = pool
        .begin()
        .await
        .context("beginning a transaction to provision a user")?;
    let user = crate::users::create_user(&mut tx, &payload, email_is_verified(provider, profile))
        .await
        .map_external(ProviderError::CreateUser)?;
    if let Some(email) = &payload.email {
        crate::users::mark_email_federated(&mut tx, user.id, email)
            .await
            .context("marking the OIDC email federated")?;
    }
    db::insert_credential(&mut tx, provider.id, subject, user.id, userinfo, profile)
        .await
        .context("inserting the provider credential")?;
    tx.commit()
        .await
        .context("committing the provisioned user")?;
    Ok(user.id)
}

/// Extracts a provider-supplied name, treating an empty value as absent. It does
/// not trim or truncate: an over-long name is rejected loudly by `create_user`'s
/// `Name` gate (surfaced transparently through [`ProviderError::CreateUser`]), not
/// silently clipped.
fn provider_name(name: Option<&str>) -> Option<String> {
    name.filter(|value| !value.is_empty()).map(String::from)
}

/// Verifies the id_token's RS256 signature against the provider's JWKS and
/// validates the issuer, audience, expiry, and nonce. Returns the typed claims
/// alongside the raw verified payload (every claim the IdP sent), so the caller
/// can persist the raw userinfo as well as the parsed identity fields. Lives here
/// rather than in `http` because it is pure JWT crypto, not a network call.
pub(super) fn verify_id_token(
    id_token: &str,
    jwks: &http::Jwks,
    issuer: &str,
    client_id: &str,
    expected_nonce: &str,
) -> anyhow::Result<(http::IdTokenClaims, serde_json::Value)> {
    use crate::auth::jwt;

    let kid = jwt::decode_header(id_token)
        .context("decoding id_token header")?
        .kid
        .context("id_token header has no kid")?;
    let jwk = jwks
        .keys
        .iter()
        .find(|key| key.kid == kid)
        .context("no jwk matches the id_token kid")?;
    let key = jwt::rsa_verifying_key(&jwk.n, &jwk.e).context("building verifying key from jwk")?;

    // Verify into the full payload first, then parse the typed subset from it.
    let raw: serde_json::Value =
        jwt::verify_rs256(id_token, &key, issuer, client_id).context("verifying id_token")?;
    let claims: http::IdTokenClaims =
        serde_json::from_value(raw.clone()).context("parsing id_token claims")?;

    if claims.nonce.as_deref() != Some(expected_nonce) {
        anyhow::bail!("id_token nonce mismatch");
    }
    Ok((claims, raw))
}

#[cfg(test)]
mod tests {
    use super::super::default_claim_mapping;
    use super::*;
    use crate::Slug;
    use crate::users::{self as users_db, CreateUserPayload, UsersError as CreateUserError};

    async fn provider(pool: &PgPool, is_registration_allowed: bool) -> db::Provider {
        // Trusts the provider's emails and permits auto-connection, so the
        // verified-email linking paths are exercised.
        let mut tx = pool.begin().await.unwrap();
        db::insert_provider(
            &mut tx,
            &Slug::try_from("google").unwrap(),
            "Google",
            Some("https://accounts.google.test"),
            "client-123",
            &crate::crypto::Ciphertext::for_test("encrypted-secret-placeholder"),
            crate::auth::ProviderFlags {
                is_registration_allowed,
                is_auto_connection_allowed: true,
                is_email_verified: true,
                is_disconnection_allowed: false,
                is_unclaimed_username_connection_allowed: false,
            },
            &default_claim_mapping(),
            "openid email profile",
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        db::find_provider_by_slug(pool, &Slug::try_from("google").unwrap())
            .await
            .unwrap()
            .expect("provider should exist")
    }

    fn claims(sub: &str, email: Option<&str>, email_verified: bool) -> http::IdTokenClaims {
        http::IdTokenClaims {
            sub: sub.to_string(),
            email: email.map(str::to_string),
            email_verified: Some(email_verified),
            given_name: Some("Ada".to_string()),
            family_name: Some("Lovelace".to_string()),
            nonce: None,
        }
    }

    /// Stand-in for the raw verified id_token payload the real flow passes. Injects
    /// a valid `preferred_username` (the subject doubles as a usable handle) so
    /// strict provisioning has a username to map; a test needing a specific one
    /// builds its own userinfo.
    fn userinfo(claims: &http::IdTokenClaims) -> serde_json::Value {
        let mut value = serde_json::to_value(claims).unwrap();
        value.as_object_mut().unwrap().insert(
            "preferred_username".into(),
            serde_json::json!(claims.sub.replace('-', "")),
        );
        value
    }

    async fn user_count(pool: &PgPool) -> usize {
        users_db::list_users(pool).await.unwrap().len()
    }

    /// A provider like [`provider`] but with the unclaimed-username-claim flag set to `allowed`.
    async fn claiming_provider(pool: &PgPool, allowed: bool) -> db::Provider {
        let mut tx = pool.begin().await.unwrap();
        db::insert_provider(
            &mut tx,
            &Slug::try_from("google").unwrap(),
            "Google",
            Some("https://accounts.google.test"),
            "client-123",
            &crate::crypto::Ciphertext::for_test("secret"),
            crate::auth::ProviderFlags {
                is_registration_allowed: true,
                is_auto_connection_allowed: true,
                is_email_verified: true,
                is_disconnection_allowed: false,
                is_unclaimed_username_connection_allowed: allowed,
            },
            &default_claim_mapping(),
            "openid email profile",
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        db::find_provider_by_slug(pool, &Slug::try_from("google").unwrap())
            .await
            .unwrap()
            .unwrap()
    }

    /// An unclaimed account (no credential) with the given username and email.
    async fn unclaimed_user(pool: &PgPool, username: &str, email: &str) -> UserId {
        let mut tx = pool.begin().await.unwrap();
        let user = users_db::insert_user(
            &mut tx,
            &CreateUserPayload {
                username: username.to_string(),
                email: Some(email.to_string()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        user.id
    }

    #[sqlx::test]
    async fn claims_an_unclaimed_account_by_username_when_allowed(pool: PgPool) {
        let provider = claiming_provider(&pool, true).await;
        // The IdP asserts username "zelda" but a different email than the account carries.
        let existing = unclaimed_user(&pool, "zelda", "roster@dept.example").await;
        let claims = claims("zelda", Some("idp@other.example"), true);

        let resolved = provision_user(&pool, &provider, &claims.sub, &userinfo(&claims))
            .await
            .unwrap()
            .user();

        assert_eq!(
            resolved, existing,
            "should claim the unclaimed account by username"
        );
        assert_eq!(user_count(&pool).await, 1, "must not create a second user");
        assert!(
            db::has_credentials(&pool, existing).await.unwrap(),
            "the account is now claimed"
        );
    }

    #[sqlx::test]
    async fn does_not_claim_when_the_flag_is_off(pool: PgPool) {
        let provider = claiming_provider(&pool, false).await;
        let existing = unclaimed_user(&pool, "zelda", "roster@dept.example").await;
        let claims = claims("zelda", Some("idp@other.example"), true);

        // Flag off: no claim by username. The IdP email matches no account, so the flow registers,
        // which collides with the existing "zelda" username and fails, proving it did not claim.
        let result = provision_user(&pool, &provider, &claims.sub, &userinfo(&claims)).await;
        // The asserted username belongs to an account this login may not claim, so the
        // person is asked to choose a different one; nothing is linked or created.
        assert!(
            matches!(result, Ok(Provisioned::NeedsUsername)),
            "flag off must not claim by username"
        );
        assert!(
            !db::has_credentials(&pool, existing).await.unwrap(),
            "the account stays unclaimed"
        );
    }

    #[sqlx::test]
    async fn registers_a_new_user_then_is_idempotent_by_subject(pool: PgPool) {
        let provider = provider(&pool, true).await;
        let claims = claims("sub-1", Some("ada@example.com"), true);

        let first = provision_user(&pool, &provider, &claims.sub, &userinfo(&claims))
            .await
            .unwrap()
            .user();
        // A second login with the same subject resolves to the same account
        // via the stored credential, no duplicate user.
        let second = provision_user(&pool, &provider, &claims.sub, &userinfo(&claims))
            .await
            .unwrap()
            .user();
        assert_eq!(first, second);
        assert_eq!(user_count(&pool).await, 1);
    }

    #[sqlx::test]
    async fn stores_raw_userinfo_and_parsed_claims(pool: PgPool) {
        let provider = provider(&pool, true).await;
        let claims = claims("sub-store", Some("grace@example.com"), true);
        let userinfo = userinfo(&claims);

        provision_user(&pool, &provider, &claims.sub, &userinfo)
            .await
            .unwrap()
            .user();

        let row = sqlx::query!(
            r#"
            SELECT raw_userinfo, profile
            FROM auth_oauth2_provider_credentials
            WHERE provider_id = $1 AND subject = $2
            "#,
            provider.id.0,
            "sub-store",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        // raw_userinfo is the untouched payload; profile is the mapping's
        // { target: value } (subject is the protocol `sub`, in its own column).
        assert_eq!(row.raw_userinfo, userinfo);
        assert_eq!(
            row.profile,
            serde_json::json!({
                "username": "substore",
                "email": "grace@example.com",
                "is_email_verified": true,
                "first_name": "Ada",
                "last_name": "Lovelace",
            }),
        );
    }

    #[sqlx::test]
    async fn applies_a_custom_claim_mapping(pool: PgPool) {
        // A provider whose IdP uses non-standard claim names.
        let mut tx = pool.begin().await.unwrap();
        db::insert_provider(
            &mut tx,
            &Slug::try_from("custom").unwrap(),
            "Custom",
            Some("https://idp.custom.test"),
            "client-c",
            &crate::crypto::Ciphertext::for_test("secret"),
            crate::auth::ProviderFlags {
                is_registration_allowed: true,
                is_auto_connection_allowed: true,
                is_email_verified: true,
                is_disconnection_allowed: false,
                is_unclaimed_username_connection_allowed: false,
            },
            &serde_json::json!([
                {"claim": "handle", "target": "username"},
                {"claim": "mail", "target": "email"},
                {"claim": "first", "target": "first_name"},
            ]),
            "openid email profile",
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let provider = db::find_provider_by_slug(&pool, &Slug::try_from("custom").unwrap())
            .await
            .unwrap()
            .unwrap();
        let claims = claims("sub-c", None, false);
        let userinfo = serde_json::json!({
            "sub": "sub-c",
            "handle": "zelda",
            "mail": "zelda@example.com",
            "first": "Zelda",
            "given_name": "ignored-by-mapping",
        });

        provision_user(&pool, &provider, &claims.sub, &userinfo)
            .await
            .unwrap()
            .user();

        let stored = sqlx::query_scalar!(
            r#"SELECT profile FROM auth_oauth2_provider_credentials WHERE subject = $1"#,
            "sub-c",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        // Only the mapped fields appear; `given_name` is not in this mapping.
        assert_eq!(
            stored,
            serde_json::json!({
                "username": "zelda",
                "email": "zelda@example.com",
                "first_name": "Zelda",
            }),
        );
    }

    #[sqlx::test]
    async fn links_to_an_existing_account_by_verified_email(pool: PgPool) {
        let provider = provider(&pool, true).await;
        let mut tx = pool.begin().await.unwrap();
        let existing = users_db::insert_user(
            &mut tx,
            &CreateUserPayload {
                username: "existing".to_string(),
                email: Some("shared@example.com".to_string()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let claims = claims("new-sub", Some("shared@example.com"), true);
        let resolved = provision_user(&pool, &provider, &claims.sub, &userinfo(&claims))
            .await
            .unwrap()
            .user();
        assert_eq!(resolved, existing.id, "should link to the existing account");
        assert_eq!(user_count(&pool).await, 1, "must not create a second user");
    }

    #[sqlx::test]
    async fn refuses_to_link_when_auto_connection_is_off(pool: PgPool) {
        {
            let mut tx = pool.begin().await.unwrap();
            db::insert_provider(
                &mut tx,
                &Slug::try_from("google").unwrap(),
                "Google",
                Some("https://accounts.google.test"),
                "client-123",
                &crate::crypto::Ciphertext::for_test("secret"),
                crate::auth::ProviderFlags {
                    is_registration_allowed: true,
                    is_auto_connection_allowed: false,
                    is_email_verified: true,
                    is_disconnection_allowed: false,
                    is_unclaimed_username_connection_allowed: false,
                },
                &default_claim_mapping(),
                "openid email profile",
                true,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        let provider = db::find_provider_by_slug(&pool, &Slug::try_from("google").unwrap())
            .await
            .unwrap()
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        users_db::insert_user(
            &mut tx,
            &CreateUserPayload {
                username: "owner".to_string(),
                email: Some("shared@example.com".to_string()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // A verified email that belongs to someone else, with auto-connect off.
        let claims = claims("new-sub", Some("shared@example.com"), true);
        assert!(matches!(
            provision_user(&pool, &provider, &claims.sub, &userinfo(&claims)).await,
            Err(crate::Error::External(ProviderError::EmailInUse))
        ));
    }

    #[sqlx::test]
    async fn registers_an_unverified_email_without_linking(pool: PgPool) {
        let provider = provider(&pool, true).await;
        // The id_token says the email is not verified: the account is still
        // created, but with an unverified email (and never linked).
        let claims = claims("sub-x", Some("ada@example.com"), false);
        let user_id = provision_user(&pool, &provider, &claims.sub, &userinfo(&claims))
            .await
            .unwrap()
            .user();
        let user = users_db::find_user_by_id(&pool, user_id)
            .await
            .unwrap()
            .unwrap();
        assert!(!user.is_email_verified);
    }

    #[sqlx::test]
    async fn registers_without_an_email(pool: PgPool) {
        // Battle.net asserts no email at all; the account is created with none.
        let provider = provider(&pool, true).await;
        let claims = claims("sub-y", None, true);
        let user_id = provision_user(&pool, &provider, &claims.sub, &userinfo(&claims))
            .await
            .unwrap()
            .user();
        let user = users_db::find_user_by_id(&pool, user_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.email, None);
        assert!(!user.is_email_verified);
        // The same subject resolves to the same account on the next login.
        assert_eq!(
            provision_user(&pool, &provider, &claims.sub, &userinfo(&claims))
                .await
                .unwrap()
                .user(),
            user_id
        );
    }

    #[sqlx::test]
    async fn a_battletag_mapping_normalizes_the_username(pool: PgPool) {
        // An OIDC provider with a Battle.net-style mapping: the username comes from the
        // userinfo `battletag`, normalized, and nothing else is mapped.
        let mut tx = pool.begin().await.unwrap();
        db::insert_provider(
            &mut tx,
            &Slug::try_from("battlenet").unwrap(),
            "Battle.net",
            Some("https://oauth.battle.net/oauth"),
            "client-123",
            &crate::crypto::Ciphertext::for_test("secret"),
            crate::auth::ProviderFlags {
                is_registration_allowed: true,
                is_auto_connection_allowed: false,
                is_email_verified: false,
                is_disconnection_allowed: false,
                is_unclaimed_username_connection_allowed: false,
            },
            &serde_json::json!([
                {"claim": "battletag", "target": "username", "normalize": true}
            ]),
            "openid",
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let provider = db::find_provider_by_slug(&pool, &Slug::try_from("battlenet").unwrap())
            .await
            .unwrap()
            .unwrap();

        let userinfo = merge_userinfo(
            serde_json::json!({"sub": "1234567890"}),
            serde_json::json!({"sub": "1234567890", "id": 1234567890, "battletag": "Jón#1234"}),
        );
        let user_id = provision_user(&pool, &provider, "1234567890", &userinfo)
            .await
            .unwrap()
            .user();
        let user = users_db::find_user_by_id(&pool, user_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.username, "Jn1234");
        assert_eq!(user.email, None);
    }

    #[test]
    fn normalize_username_folds_a_battletag_into_the_account_shape() {
        assert_eq!(normalize_username("Name#1234"), "Name1234");
        assert_eq!(normalize_username("  Spaced Out#99 "), "SpacedOut99");
        assert_eq!(normalize_username("123abc"), "abc");
        assert_eq!(normalize_username("Under_score#1"), "Underscore1");
        assert_eq!(normalize_username("###"), "");
        assert_eq!(normalize_username(&"a".repeat(40)).len(), 32);
        assert_eq!(normalize_username("ok-"), "ok");
    }

    #[test]
    fn merge_userinfo_lets_the_userinfo_document_fill_and_override() {
        let merged = merge_userinfo(
            serde_json::json!({"sub": "1", "name": "token"}),
            serde_json::json!({"name": "userinfo", "battletag": "X#1"}),
        );
        assert_eq!(merged["sub"], "1");
        assert_eq!(merged["name"], "userinfo");
        assert_eq!(merged["battletag"], "X#1");
        // A non-object on either side degrades to the other.
        assert_eq!(
            merge_userinfo(serde_json::json!(null), serde_json::json!({"a": 1}))["a"],
            1
        );
    }

    #[sqlx::test]
    async fn refuses_registration_when_disabled(pool: PgPool) {
        let provider = provider(&pool, false).await;
        let claims = claims("sub-z", Some("nobody@example.com"), true);
        assert!(matches!(
            provision_user(&pool, &provider, &claims.sub, &userinfo(&claims)).await,
            Err(crate::Error::External(ProviderError::RegistrationDisabled))
        ));
    }

    #[sqlx::test]
    async fn uses_a_valid_preferred_username(pool: PgPool) {
        let provider = provider(&pool, true).await;
        // A preferred_username that is already a valid username is used verbatim.
        let userinfo = serde_json::json!({
            "sub": "sub-clean",
            "email": "person@example.com",
            "email_verified": true,
            "preferred_username": "octocat",
        });
        let user_id = provision_user(&pool, &provider, "sub-clean", &userinfo)
            .await
            .unwrap()
            .user();
        let user = users_db::find_user_by_id(&pool, user_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.username, "octocat");
    }

    #[sqlx::test]
    async fn asks_for_a_username_when_the_asserted_one_is_unusable(pool: PgPool) {
        let provider = provider(&pool, true).await;
        // A free-form preferred_username (a space) is not a valid username, so the person
        // chooses one; the account is not created yet.
        let invalid = serde_json::json!({
            "sub": "sub-messy",
            "email": "grace@example.com",
            "email_verified": true,
            "preferred_username": "Grace Hopper",
        });
        assert!(matches!(
            provision_user(&pool, &provider, "sub-messy", &invalid).await,
            Ok(Provisioned::NeedsUsername)
        ));
        assert_eq!(user_count(&pool).await, 0);

        // No preferred_username at all: the same outcome.
        let missing = serde_json::json!({
            "sub": "sub-none",
            "email": "nobody@example.com",
            "email_verified": true,
        });
        assert!(matches!(
            provision_user(&pool, &provider, "sub-none", &missing).await,
            Ok(Provisioned::NeedsUsername)
        ));

        // A valid but reserved handle, too: it is not allocated around with a suffix.
        let reserved = serde_json::json!({
            "sub": "sub-admin",
            "email": "admin@example.com",
            "email_verified": true,
            "preferred_username": "admin",
        });
        assert!(matches!(
            provision_user(&pool, &provider, "sub-admin", &reserved).await,
            Ok(Provisioned::NeedsUsername)
        ));
        assert_eq!(user_count(&pool).await, 0);
    }

    #[sqlx::test]
    async fn registers_under_the_username_the_person_chose(pool: PgPool) {
        // A Battle.net-shaped sign-in: no mapping at all (the standard OIDC one applies and
        // finds nothing usable), only a BattleTag in the userinfo.
        let provider = provider(&pool, true).await;
        let userinfo = merge_userinfo(
            serde_json::json!({"sub": "1234567890"}),
            serde_json::json!({"sub": "1234567890", "id": 1234567890, "battletag": "Jón#1234"}),
        );
        assert!(matches!(
            provision_user(&pool, &provider, "1234567890", &userinfo).await,
            Ok(Provisioned::NeedsUsername)
        ));
        let profile = build_profile(&provider, &userinfo).unwrap();
        // The registration page says who this is and suggests a username from the tag.
        assert_eq!(identity_label(&userinfo).as_deref(), Some("Jón#1234"));
        assert_eq!(
            suggest_username(&profile, &userinfo).as_deref(),
            Some("Jn1234")
        );

        // The chosen name goes through the same gate as any other.
        assert!(matches!(
            register(
                &pool,
                &provider,
                "1234567890",
                &userinfo,
                &profile,
                "no spaces"
            )
            .await,
            Err(crate::Error::External(ProviderError::CreateUser(
                CreateUserError::InvalidUsername(_)
            )))
        ));
        let user_id = register(
            &pool,
            &provider,
            "1234567890",
            &userinfo,
            &profile,
            "Thrall",
        )
        .await
        .unwrap();
        let user = users_db::find_user_by_id(&pool, user_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.username, "Thrall");
        assert_eq!(user.email, None);

        // From now on the subject resolves straight to that account.
        assert_eq!(
            provision_user(&pool, &provider, "1234567890", &userinfo)
                .await
                .unwrap()
                .user(),
            user_id
        );
    }

    #[sqlx::test]
    async fn asks_for_a_username_when_the_asserted_one_is_taken(pool: PgPool) {
        let provider = provider(&pool, true).await;
        // An existing account already holds the username the IdP asserts; the
        // login's email differs, so it would register rather than link, and the
        // collision sends the person to choose a different name.
        let mut tx = pool.begin().await.unwrap();
        users_db::insert_user(
            &mut tx,
            &CreateUserPayload {
                username: "octocat".to_string(),
                email: Some("someone@example.com".to_string()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let userinfo = serde_json::json!({
            "sub": "sub-dup",
            "email": "newcomer@example.com",
            "email_verified": true,
            "preferred_username": "octocat",
        });
        assert!(matches!(
            provision_user(&pool, &provider, "sub-dup", &userinfo).await,
            Ok(Provisioned::NeedsUsername)
        ));
        assert_eq!(user_count(&pool).await, 1);
    }

    #[sqlx::test]
    async fn registers_a_new_account_when_the_email_belongs_to_an_unverified_account(pool: PgPool) {
        let provider = provider(&pool, true).await;
        // An existing account whose email has not been verified: not linkable,
        // and, uniqueness being verified-only, no longer a block.
        let mut tx = pool.begin().await.unwrap();
        users_db::insert_user(
            &mut tx,
            &CreateUserPayload {
                username: "owner".to_string(),
                email: Some("taken@example.com".to_string()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            false,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // The IdP asserts the same address as verified, so a fresh account is
        // provisioned alongside the unverified one.
        let claims = claims("sub-collide", Some("taken@example.com"), true);
        let user_id = provision_user(&pool, &provider, &claims.sub, &userinfo(&claims))
            .await
            .expect("a verified login should provision a new account")
            .user();
        let user = users_db::find_user_by_id(&pool, user_id)
            .await
            .unwrap()
            .unwrap();
        assert!(user.is_email_verified);
        assert_eq!(user_count(&pool).await, 2, "a second account is created");
    }

    /// A plain OAuth2 (non-OIDC) provider: no issuer, a subject claim mapping.
    async fn non_oidc_provider(pool: &PgPool) -> db::Provider {
        let mut tx = pool.begin().await.unwrap();
        db::insert_provider(
            &mut tx,
            &Slug::try_from("discord").unwrap(),
            "Discord",
            None,
            "client-d",
            &crate::crypto::Ciphertext::for_test("secret"),
            crate::auth::ProviderFlags {
                is_registration_allowed: true,
                is_auto_connection_allowed: true,
                is_email_verified: true,
                is_disconnection_allowed: false,
                is_unclaimed_username_connection_allowed: false,
            },
            &serde_json::json!([
                {"claim": "id", "target": "subject"},
                {"claim": "username", "target": "username"},
                {"claim": "email", "target": "email"},
                {"claim": "verified", "target": "is_email_verified"},
                {"claim": "global_name", "target": "first_name"},
            ]),
            "identify email",
            false,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        db::find_provider_by_slug(pool, &Slug::try_from("discord").unwrap())
            .await
            .unwrap()
            .unwrap()
    }

    #[sqlx::test]
    async fn non_oidc_subject_comes_from_the_claim_mapping(pool: PgPool) {
        let provider = non_oidc_provider(&pool).await;
        // A string id (Discord snowflake) is used verbatim.
        assert_eq!(
            extract_subject(&provider, &serde_json::json!({"id": "42"})).unwrap(),
            "42"
        );
        // A numeric id (e.g. GitHub) is stringified.
        assert_eq!(
            extract_subject(&provider, &serde_json::json!({"id": 99})).unwrap(),
            "99"
        );
        // A missing subject field is a clean SubjectMissing error.
        assert!(matches!(
            extract_subject(&provider, &serde_json::json!({"email": "a@b.test"})),
            Err(crate::Error::External(ProviderError::SubjectMissing))
        ));
    }

    #[sqlx::test]
    async fn provisions_a_non_oidc_user_from_userinfo(pool: PgPool) {
        let provider = non_oidc_provider(&pool).await;
        let userinfo = serde_json::json!({
            "id": "disc-1",
            "username": "neo",
            "email": "neo@example.com",
            "verified": true,
            "global_name": "Neo",
        });
        let subject = extract_subject(&provider, &userinfo).unwrap();
        let user_id = provision_user(&pool, &provider, &subject, &userinfo)
            .await
            .unwrap()
            .user();

        let row = sqlx::query!(
            r#"
            SELECT subject, profile
            FROM auth_oauth2_provider_credentials
            WHERE provider_id = $1
            "#,
            provider.id.0,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.subject, "disc-1");
        assert_eq!(
            row.profile.get("email").and_then(|v| v.as_str()),
            Some("neo@example.com")
        );
        assert_eq!(
            row.profile.get("first_name").and_then(|v| v.as_str()),
            Some("Neo")
        );
        // The subject is the credential identity, never a profile field.
        assert!(row.profile.get("subject").is_none());

        // Idempotent by subject, a second login resolves to the same account.
        let again = provision_user(&pool, &provider, &subject, &userinfo)
            .await
            .unwrap()
            .user();
        assert_eq!(user_id, again);
        assert_eq!(user_count(&pool).await, 1);
    }

    // --- claim mapping shape (build_profile) ---

    fn provider_with_claims(claims: serde_json::Value) -> db::Provider {
        db::Provider {
            id: crate::auth::providers::ProviderId(1),
            slug: Slug::try_from("plain").unwrap(),
            name: "Plain".into(),
            issuer: None,
            client_id: "client".into(),
            client_secret: crate::crypto::Ciphertext::for_test("secret"),
            is_registration_allowed: true,
            is_auto_connection_allowed: true,
            is_email_verified: true,
            is_disconnection_allowed: false,
            is_unclaimed_username_connection_allowed: false,
            is_oidc: false,
            claims,
            scope: "identify".into(),
        }
    }

    #[test]
    fn build_profile_keeps_native_scalars() {
        let provider = provider_with_claims(serde_json::json!([
            { "claim": "mail", "target": "email" },
            { "claim": "verified", "target": "is_email_verified" },
            { "claim": "extra", "target": null },
        ]));
        let userinfo = serde_json::json!({
            "mail": "ada@example.com",
            "verified": true,
            "extra": "ignored",
        });
        let profile = build_profile(&provider, &userinfo).unwrap();
        assert_eq!(profile["email"], serde_json::json!("ada@example.com"));
        // A bool claim keeps its native type rather than being stringified.
        assert_eq!(profile["is_email_verified"], serde_json::json!(true));
        // An unmapped claim is dropped.
        assert!(profile.get("extra").is_none());
    }

    #[test]
    fn build_profile_rejects_an_array_for_a_single_target() {
        let single =
            provider_with_claims(serde_json::json!([{ "claim": "mail", "target": "email" }]));
        assert!(matches!(
            build_profile(
                &single,
                &serde_json::json!({ "mail": ["a@x.test", "b@x.test"] })
            ),
            Err(crate::Error::Internal(_))
        ));
    }
}
