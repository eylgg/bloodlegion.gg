use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;

use crate::Slug;
use crate::auth::providers::ProviderId;
use crate::auth::{Next, ProviderConnectionListing, ProviderFlags, ProviderListing};
use crate::crypto::Ciphertext;
use crate::users::UserId;

pub struct Provider {
    pub id: ProviderId,
    pub slug: Slug,
    pub name: String,
    /// The OIDC issuer (discovery anchor); `None` for a plain OAuth2 provider,
    /// which has no discovery doc. Present iff the provider is `is_oidc`.
    pub issuer: Option<String>,
    pub client_id: String,
    pub client_secret: Ciphertext,
    pub is_registration_allowed: bool,
    pub is_auto_connection_allowed: bool,
    pub is_email_verified: bool,
    #[allow(dead_code)]
    pub is_disconnection_allowed: bool,
    /// Whether a first login may claim an unclaimed account (no linked identity) by the asserted
    /// username. Enable only when that username is authoritative (see the login-flow claim branch).
    pub is_unclaimed_username_connection_allowed: bool,
    /// OIDC (id_token + nonce + JWKS) vs plain OAuth2 (access token + userinfo).
    pub is_oidc: bool,
    /// Claim->target mapping: `[{ "claim": <id_token claim>, "target": <field> }]`.
    pub claims: serde_json::Value,
    /// Space-delimited scope requested at the authorize endpoint (openid not
    /// required; a plain OAuth2 upstream requests its own).
    pub scope: String,
}

pub struct Request {
    pub id: i64,
    pub provider_id: ProviderId,
    /// The account that started a connect/link flow; `None` for a plain login.
    /// Dormant until an OIDC connect endpoint consumes it (the column exists so
    /// the state is bound to a user, the way the SAML connect flow already is).
    #[allow(dead_code)]
    pub user_id: Option<UserId>,
    /// Present only for OIDC providers (it binds an id_token); `None` for plain
    /// OAuth2. The provider's `is_oidc` decides which, enforced by a DB trigger.
    pub nonce: Option<String>,
    pub code_verifier: String,
    pub next: Next,
}

/// Inserts a provider and returns its id. Takes a connection so both the
/// auth_providers envelope and the auth_oauth2_providers body can be written
/// in the same transaction as a non-OIDC provider's metadata row.
#[allow(clippy::too_many_arguments)]
pub async fn insert_provider(
    conn: &mut sqlx::PgConnection,
    slug: &Slug,
    name: &str,
    issuer: Option<&str>,
    client_id: &str,
    client_secret: &Ciphertext,
    flags: ProviderFlags,
    claims: &serde_json::Value,
    scope: &str,
    is_oidc: bool,
) -> sqlx::Result<ProviderId> {
    let provider_id = sqlx::query_scalar!(
        r#"
        INSERT INTO auth_providers
            (kind, slug, name, is_registration_allowed, is_auto_connection_allowed,
             is_email_verified, is_disconnection_allowed, is_unclaimed_username_connection_allowed)
        VALUES ('oauth2', $1, $2, $3, $4, $5, $6, $7)
        RETURNING id AS "id: ProviderId"
        "#,
        slug.as_ref(),
        name,
        flags.is_registration_allowed,
        flags.is_auto_connection_allowed,
        flags.is_email_verified,
        flags.is_disconnection_allowed,
        flags.is_unclaimed_username_connection_allowed,
    )
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO auth_oauth2_providers
            (provider_id, issuer, client_id, client_secret_ciphertext, claims, scope, is_oidc)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        provider_id.0,
        issuer,
        client_id,
        client_secret.as_ref(),
        claims,
        scope,
        is_oidc,
    )
    .execute(&mut *conn)
    .await?;
    Ok(provider_id)
}

/// OAuth2 providers offered on the login page, as `(slug, name)` ordered by
/// display name.
pub async fn list_login_providers(pool: &PgPool) -> sqlx::Result<Vec<ProviderListing>> {
    sqlx::query_as!(
        ProviderListing,
        r#"
        SELECT p.slug AS "slug: Slug", p.name
        FROM auth_oauth2_providers o
        JOIN auth_providers p ON p.id = o.provider_id
        ORDER BY p.name
        "#
    )
    .fetch_all(pool)
    .await
}

/// Every OAuth2/OIDC provider with whether `user_id` has an active
/// (non-disconnected) connection to it, ordered by display name.
pub async fn list_connections(
    pool: &PgPool,
    user_id: UserId,
) -> sqlx::Result<Vec<ProviderConnectionListing>> {
    sqlx::query_as!(
        ProviderConnectionListing,
        r#"
        SELECT
            p.slug AS "slug: Slug",
            p.name,
            EXISTS (
                SELECT 1 FROM auth_oauth2_provider_credentials c
                WHERE c.provider_id = p.id
                    AND c.user_id = $1
                    AND c.disconnected_at IS NULL
            ) AS "connected!"
        FROM auth_oauth2_providers o
        JOIN auth_providers p ON p.id = o.provider_id
        ORDER BY p.name
        "#,
        user_id.0,
    )
    .fetch_all(pool)
    .await
}

pub async fn find_provider_by_slug(pool: &PgPool, slug: &Slug) -> sqlx::Result<Option<Provider>> {
    sqlx::query_as!(
        Provider,
        r#"
        SELECT p.id AS "id: ProviderId", p.slug AS "slug: Slug", p.name, o.issuer, o.client_id,
               o.client_secret_ciphertext AS "client_secret: Ciphertext",
               p.is_registration_allowed, p.is_auto_connection_allowed, p.is_email_verified,
               p.is_disconnection_allowed, p.is_unclaimed_username_connection_allowed,
               o.is_oidc, o.claims, o.scope
        FROM auth_oauth2_providers o
        JOIN auth_providers p ON p.id = o.provider_id
        WHERE p.slug = $1
        "#,
        slug.as_ref(),
    )
    .fetch_optional(pool)
    .await
}

/// Looks up a provider by its id. The shared OAuth2 callback resolves the
/// provider from the `state`-bound request's `provider_id`, not a slug in the URL.
pub async fn find_provider_by_id(pool: &PgPool, id: ProviderId) -> sqlx::Result<Option<Provider>> {
    sqlx::query_as!(
        Provider,
        r#"
        SELECT p.id AS "id: ProviderId", p.slug AS "slug: Slug", p.name, o.issuer, o.client_id,
               o.client_secret_ciphertext AS "client_secret: Ciphertext",
               p.is_registration_allowed, p.is_auto_connection_allowed, p.is_email_verified,
               p.is_disconnection_allowed, p.is_unclaimed_username_connection_allowed,
               o.is_oidc, o.claims, o.scope
        FROM auth_oauth2_providers o
        JOIN auth_providers p ON p.id = o.provider_id
        WHERE p.id = $1
        "#,
        id.0,
    )
    .fetch_optional(pool)
    .await
}

/// Whether the account has any linked login identity: a local password, or a SAML/OIDC credential.
/// A `false` result means an unclaimed, provisioned account no one has authenticated as, so a
/// trusted first login may claim it by username. A disconnected credential still counts (claimed
/// once).
pub async fn has_credentials(pool: &PgPool, user_id: UserId) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM auth_local_credentials WHERE user_id = $1
            UNION ALL
            SELECT 1 FROM auth_oauth2_provider_credentials WHERE user_id = $1
        ) AS "exists!"
        "#,
        user_id.0,
    )
    .fetch_one(pool)
    .await
}

/// Fills the account's first and last name only where they are currently NULL, so a claim or link
/// can supply the names a roster-provisioned account lacks without overwriting anything already
/// set.
pub async fn update_user_names_if_missing(
    conn: &mut PgConnection,
    user_id: UserId,
    first_name: Option<&str>,
    last_name: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE users
        SET first_name = COALESCE(first_name, $2),
            last_name = COALESCE(last_name, $3)
        WHERE id = $1
        "#,
        user_id.0,
        first_name,
        last_name,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn insert_request(
    pool: &PgPool,
    state: &str,
    provider_id: ProviderId,
    user_id: Option<UserId>,
    nonce: Option<&str>,
    code_verifier: &str,
    next: &Next,
) -> sqlx::Result<()> {
    // The strong type guarantees the caller validated the target; the column
    // binds the underlying `&str`.
    let next: &str = next;
    sqlx::query!(
        r#"
        INSERT INTO auth_oauth2_provider_requests
            (state, provider_id, user_id, nonce, code_verifier, next)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        state,
        provider_id.0,
        user_id.map(|id| id.0),
        nonce,
        code_verifier,
        next,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Looks up a still-pending, unexpired request by `state` without consuming it: the callback needs
/// the `code_verifier` before it can exchange the code, so the single-use gate is
/// `complete_request`, run after the exchange succeeds. Returns `None` if the request is unknown,
/// already completed, or expired.
pub async fn find_pending_request(pool: &PgPool, state: &str) -> sqlx::Result<Option<Request>> {
    sqlx::query_as!(
        Request,
        r#"
        SELECT id, provider_id AS "provider_id: ProviderId", user_id AS "user_id: UserId", nonce, code_verifier, next as "next: Next"
        FROM auth_oauth2_provider_requests
        WHERE state = $1 AND completed_at IS NULL AND expires_at > now()
        "#,
        state,
    )
    .fetch_optional(pool)
    .await
}

/// Atomically completes the request: a single guarded UPDATE that flips
/// `completed_at` from NULL, the single-use gate, so a replayed callback cannot
/// complete twice, and records the resolved subject and raw userinfo for the
/// audit ledger. Returns whether it completed (`false` = already completed or
/// gone). Mirrors the SAML `complete_request` gate.
pub async fn complete_request(
    pool: &PgPool,
    id: i64,
    subject: &str,
    userinfo: &serde_json::Value,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE auth_oauth2_provider_requests
        SET completed_at = now(), subject = $2, raw_userinfo = $3
        WHERE id = $1 AND completed_at IS NULL
        "#,
        id,
        subject,
        userinfo,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// A completed, unexpired login request: the identity it verified, parked until the person
/// chooses a username.
pub struct CompletedRequest {
    pub provider_id: ProviderId,
    pub subject: String,
    pub raw_userinfo: serde_json::Value,
    pub next: Next,
}

/// Looks up a *completed* request by `state` while it is still within its window, for the
/// registration page. The counterpart of [`find_pending_request`], which the callback uses.
pub async fn find_completed_request(
    pool: &PgPool,
    state: &str,
) -> sqlx::Result<Option<CompletedRequest>> {
    sqlx::query_as!(
        CompletedRequest,
        r#"
        SELECT provider_id AS "provider_id: ProviderId", subject AS "subject!",
               raw_userinfo AS "raw_userinfo!", next AS "next: Next"
        FROM auth_oauth2_provider_requests
        WHERE state = $1 AND completed_at IS NOT NULL AND expires_at > now()
        "#,
        state,
    )
    .fetch_optional(pool)
    .await
}

pub async fn find_user_by_credential(
    pool: &PgPool,
    provider_id: ProviderId,
    subject: &str,
) -> sqlx::Result<Option<UserId>> {
    sqlx::query_scalar!(
        r#"
        SELECT user_id AS "user_id: UserId" FROM auth_oauth2_provider_credentials
        WHERE provider_id = $1 AND subject = $2
        "#,
        provider_id.0,
        subject,
    )
    .fetch_optional(pool)
    .await
}

/// Finds the user that owns a *verified* email, for safe account linking. Matches
/// on the normalized (lowercased) form via the generated email_normalized column.
pub async fn find_user_by_verified_email(
    pool: &PgPool,
    email: &str,
) -> sqlx::Result<Option<UserId>> {
    sqlx::query_scalar!(
        r#"
        SELECT user_id AS "user_id: UserId" FROM user_emails
        WHERE email_normalized = $1 AND verified_at IS NOT NULL
        "#,
        email.to_lowercase(),
    )
    .fetch_optional(pool)
    .await
}

pub async fn insert_credential(
    conn: &mut PgConnection,
    provider_id: ProviderId,
    subject: &str,
    user_id: UserId,
    userinfo: &serde_json::Value,
    profile: &serde_json::Value,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO auth_oauth2_provider_credentials
            (provider_id, subject, user_id, raw_userinfo, profile)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        provider_id.0,
        subject,
        user_id.0,
        userinfo,
        profile,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Admin-facing view of a configured OAuth2 provider. The client secret is
/// stored encrypted and never surfaced here.
#[derive(serde::Serialize)]
pub struct ProviderSummary {
    pub slug: Slug,
    pub name: String,
    pub issuer: Option<String>,
    pub client_id: String,
    pub is_registration_allowed: bool,
    pub is_auto_connection_allowed: bool,
    pub is_email_verified: bool,
    pub is_disconnection_allowed: bool,
    pub is_unclaimed_username_connection_allowed: bool,
    pub is_oidc: bool,
    pub scope: String,
    pub disabled_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::iso8601")]
    pub created_at: OffsetDateTime,
}

pub async fn list_providers(pool: &PgPool) -> sqlx::Result<Vec<ProviderSummary>> {
    sqlx::query_as!(
        ProviderSummary,
        r#"
        SELECT p.slug AS "slug: Slug", p.name, o.issuer, o.client_id, p.is_registration_allowed,
               p.is_auto_connection_allowed, p.is_email_verified, p.is_disconnection_allowed,
               p.is_unclaimed_username_connection_allowed,
               o.is_oidc, o.scope, p.disabled_at, p.created_at
        FROM auth_oauth2_providers o
        JOIN auth_providers p ON p.id = o.provider_id
        ORDER BY p.name
        "#
    )
    .fetch_all(pool)
    .await
}

/// Updates an OAuth2 provider's policy flags in place, keyed on its slug. Each `Some` overwrites
/// the stored value; each `None` leaves it unchanged (so a toggle sends just the one flag). Returns
/// whether an OAuth2 provider with that slug existed. Mirrors the SAML flag update.
#[allow(clippy::too_many_arguments)]
pub async fn update_provider_flags(
    pool: &PgPool,
    slug: &Slug,
    is_registration_allowed: Option<bool>,
    is_auto_connection_allowed: Option<bool>,
    is_email_verified: Option<bool>,
    is_disconnection_allowed: Option<bool>,
    is_unclaimed_username_connection_allowed: Option<bool>,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE auth_providers SET
            is_registration_allowed = COALESCE($2, is_registration_allowed),
            is_auto_connection_allowed = COALESCE($3, is_auto_connection_allowed),
            is_email_verified = COALESCE($4, is_email_verified),
            is_disconnection_allowed = COALESCE($5, is_disconnection_allowed),
            is_unclaimed_username_connection_allowed =
                COALESCE($6, is_unclaimed_username_connection_allowed)
        WHERE slug = $1 AND kind = 'oauth2'
        "#,
        slug.as_ref(),
        is_registration_allowed,
        is_auto_connection_allowed,
        is_email_verified,
        is_disconnection_allowed,
        is_unclaimed_username_connection_allowed,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Deletes the provider, returning whether a row matched. Cascades to the subtype body.
pub async fn delete_provider(pool: &PgPool, slug: &Slug) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"DELETE FROM auth_providers WHERE slug = $1 AND kind = 'oauth2'"#,
        slug.as_ref()
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// A provider's resolved endpoints (+ JWKS for OIDC), separate from the admin
/// config so its `fetched_at` means "last successful pull", not "admin last
/// edited". For OIDC it caches the discovery document; for plain OAuth2 it holds
/// the admin-configured endpoints (jwks_uri/jwks NULL).
pub struct OidcMetadata {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: Option<String>,
    /// jwks_uri/jwks are present only for OIDC providers (they verify id_token
    /// signatures); `None` for plain OAuth2. Enforced by a DB trigger. Only `jwks`
    /// is read at verify time; jwks_uri is retained for diagnostics/future refetch.
    #[allow(dead_code)]
    pub jwks_uri: Option<String>,
    pub jwks: Option<serde_json::Value>,
    /// Cache fill time; the freshness gate uses expires_at, so this is for audit.
    #[allow(dead_code)]
    pub fetched_at: OffsetDateTime,
    pub expires_at: Option<OffsetDateTime>,
}

/// Upserts the one metadata row for a provider, stamping `fetched_at` to now() on
/// every refresh (the row is wholly rewritten each time). jwks_uri/jwks are NULL
/// for a non-OIDC provider. Takes any executor so the create path can write it in
/// the provider's transaction.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_metadata<'e, E: sqlx::PgExecutor<'e>>(
    executor: E,
    provider_id: ProviderId,
    authorization_endpoint: &str,
    token_endpoint: &str,
    userinfo_endpoint: Option<&str>,
    jwks_uri: Option<&str>,
    jwks: Option<&serde_json::Value>,
    expires_at: Option<OffsetDateTime>,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO auth_oauth2_provider_metadata
            (provider_id, authorization_endpoint, token_endpoint, userinfo_endpoint,
             jwks_uri, jwks, fetched_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, now(), $7)
        ON CONFLICT (provider_id) DO UPDATE
        SET authorization_endpoint = EXCLUDED.authorization_endpoint,
            token_endpoint = EXCLUDED.token_endpoint,
            userinfo_endpoint = EXCLUDED.userinfo_endpoint,
            jwks_uri = EXCLUDED.jwks_uri,
            jwks = EXCLUDED.jwks,
            fetched_at = now(),
            expires_at = EXCLUDED.expires_at
        "#,
        provider_id.0,
        authorization_endpoint,
        token_endpoint,
        userinfo_endpoint,
        jwks_uri,
        jwks,
        expires_at,
    )
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn find_metadata(
    pool: &PgPool,
    provider_id: ProviderId,
) -> sqlx::Result<Option<OidcMetadata>> {
    sqlx::query_as!(
        OidcMetadata,
        r#"
        SELECT authorization_endpoint, token_endpoint, userinfo_endpoint,
               jwks_uri, jwks, fetched_at, expires_at
        FROM auth_oauth2_provider_metadata
        WHERE provider_id = $1
        "#,
        provider_id.0,
    )
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{generate_token, hash_token};

    async fn provider_id(pool: &PgPool) -> ProviderId {
        let google = Slug::try_from("google").unwrap();
        let mut tx = pool.begin().await.unwrap();
        insert_provider(
            &mut tx,
            &google,
            "Google",
            Some("https://issuer.test"),
            "client-id",
            &Ciphertext::for_test("secret"),
            crate::auth::ProviderFlags {
                is_registration_allowed: true,
                is_auto_connection_allowed: true,
                is_email_verified: true,
                is_disconnection_allowed: false,
                is_unclaimed_username_connection_allowed: false,
            },
            &serde_json::json!([{"claim": "email", "target": "email"}]),
            "openid email profile",
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        find_provider_by_slug(pool, &google)
            .await
            .unwrap()
            .unwrap()
            .id
    }

    /// A 43-char base64url token, matching the request column constraints.
    fn token() -> String {
        hash_token(&generate_token::<32>())
    }

    #[sqlx::test]
    async fn update_flags_toggles_only_the_supplied_flags(pool: PgPool) {
        // Inserts "google" with registration/auto/verified on and username-claim off.
        provider_id(&pool).await;
        let google = Slug::try_from("google").unwrap();

        // Turn on the username-claim flag; the others are left as they were.
        let updated = update_provider_flags(&pool, &google, None, None, None, None, Some(true))
            .await
            .unwrap();
        assert!(updated);
        let provider = &list_providers(&pool).await.unwrap()[0];
        assert!(provider.is_unclaimed_username_connection_allowed);
        assert!(provider.is_registration_allowed);

        // An unknown slug matches nothing.
        let absent = Slug::try_from("absent").unwrap();
        assert!(
            !update_provider_flags(&pool, &absent, None, None, None, None, Some(true))
                .await
                .unwrap()
        );
    }

    #[sqlx::test]
    async fn an_in_flight_request_completes_exactly_once(pool: PgPool) {
        let provider_id = provider_id(&pool).await;
        let state = token();
        let nonce = token();
        let code_verifier = token();
        insert_request(
            &pool,
            &state,
            provider_id,
            None,
            Some(&nonce),
            &code_verifier,
            &Next::for_test("/next"),
        )
        .await
        .unwrap();

        let request = find_pending_request(&pool, &state)
            .await
            .unwrap()
            .expect("request should be pending on first lookup");
        assert_eq!(request.provider_id, provider_id);
        assert_eq!(request.user_id, None);
        assert_eq!(request.nonce, Some(nonce));
        assert_eq!(request.code_verifier, code_verifier);
        assert_eq!(&*request.next, "/next");

        // The guarded completion is the single-use gate.
        let userinfo = serde_json::json!({"sub": "subject-1"});
        assert!(
            complete_request(&pool, request.id, "subject-1", &userinfo)
                .await
                .unwrap(),
            "first completion succeeds"
        );
        assert!(
            !complete_request(&pool, request.id, "subject-1", &userinfo)
                .await
                .unwrap(),
            "a replayed completion is rejected"
        );
        // Once completed, the request is no longer pending (replay finds nothing)...
        assert!(find_pending_request(&pool, &state).await.unwrap().is_none());
        // ...but the registration page can still read the verified identity back.
        let completed = find_completed_request(&pool, &state)
            .await
            .unwrap()
            .expect("a completed request is readable within its window");
        assert_eq!(completed.provider_id, provider_id);
        assert_eq!(completed.subject, "subject-1");
        assert_eq!(completed.raw_userinfo, userinfo);
        assert_eq!(&*completed.next, "/next");
        assert!(
            find_completed_request(&pool, &token())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn finding_an_unknown_state_returns_none(pool: PgPool) {
        assert!(
            find_pending_request(&pool, &token())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn metadata_cache_upserts_one_row_per_provider(pool: PgPool) {
        let provider_id = provider_id(&pool).await;
        let jwks = serde_json::json!({"keys": [{"kid": "k1"}]});
        upsert_metadata(
            &pool,
            provider_id,
            "https://idp.test/authorize",
            "https://idp.test/token",
            None,
            Some("https://idp.test/jwks"),
            Some(&jwks),
            None,
        )
        .await
        .unwrap();
        let first = find_metadata(&pool, provider_id)
            .await
            .unwrap()
            .expect("metadata should be cached");
        assert_eq!(first.token_endpoint, "https://idp.test/token");
        assert_eq!(first.userinfo_endpoint, None);
        assert_eq!(first.jwks, Some(jwks));

        // A refetch rewrites the same row in place and re-stamps fetched_at.
        let rotated = serde_json::json!({"keys": [{"kid": "k2"}]});
        upsert_metadata(
            &pool,
            provider_id,
            "https://idp.test/authorize",
            "https://idp.test/token",
            Some("https://idp.test/userinfo"),
            Some("https://idp.test/jwks"),
            Some(&rotated),
            None,
        )
        .await
        .unwrap();
        let second = find_metadata(&pool, provider_id).await.unwrap().unwrap();
        assert_eq!(
            second.userinfo_endpoint.as_deref(),
            Some("https://idp.test/userinfo")
        );
        assert_eq!(second.jwks, Some(rotated));
        assert!(second.fetched_at >= first.fetched_at);
    }

    /// Inserts a provider with an explicit `is_oidc`/`scope` and returns its id.
    async fn provider_with(pool: &PgPool, slug: &str, is_oidc: bool, scope: &str) -> ProviderId {
        let slug = Slug::try_from(slug).unwrap();
        // issuer is present iff OIDC (DB check), so derive it from is_oidc.
        let issuer = is_oidc.then(|| format!("https://{slug}.test"));
        let mut tx = pool.begin().await.unwrap();
        insert_provider(
            &mut tx,
            &slug,
            slug.as_ref(),
            issuer.as_deref(),
            "client-id",
            &Ciphertext::for_test("secret"),
            crate::auth::ProviderFlags {
                is_registration_allowed: true,
                is_auto_connection_allowed: true,
                is_email_verified: true,
                is_disconnection_allowed: false,
                is_unclaimed_username_connection_allowed: false,
            },
            &serde_json::json!([{"claim": "email", "target": "email"}]),
            scope,
            is_oidc,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        find_provider_by_slug(pool, &slug)
            .await
            .unwrap()
            .unwrap()
            .id
    }

    /// Raw request insert that can omit the nonce, to exercise the OIDC trigger.
    async fn try_request(
        pool: &PgPool,
        provider_id: ProviderId,
        nonce: Option<&str>,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_provider_requests (state, provider_id, nonce, code_verifier, next)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            token(),
            provider_id.0,
            nonce,
            token(),
            "/",
        )
        .execute(pool)
        .await
        .map(|_| ())
    }

    #[sqlx::test]
    async fn request_nonce_must_match_provider_is_oidc(pool: PgPool) {
        let oidc = provider_with(&pool, "oidc", true, "openid email").await;
        let plain = provider_with(&pool, "plain", false, "identify email").await;

        // OIDC: the nonce is required.
        assert!(try_request(&pool, oidc, None).await.is_err());
        assert!(try_request(&pool, oidc, Some(&token())).await.is_ok());

        // Plain OAuth2: the nonce is forbidden (no id_token to bind).
        assert!(try_request(&pool, plain, Some(&token())).await.is_err());
        assert!(try_request(&pool, plain, None).await.is_ok());
    }

    #[sqlx::test]
    async fn oidc_provider_requires_the_openid_scope(pool: PgPool) {
        // is_oidc with no openid scope -> rejected by the oidc_openid CHECK.
        {
            let mut tx = pool.begin().await.unwrap();
            let result = insert_provider(
                &mut tx,
                &Slug::try_from("bad").unwrap(),
                "Bad",
                Some("https://bad.test"),
                "client-id",
                &Ciphertext::for_test("secret"),
                crate::auth::ProviderFlags {
                    is_registration_allowed: true,
                    is_auto_connection_allowed: true,
                    is_email_verified: true,
                    is_disconnection_allowed: false,
                    is_unclaimed_username_connection_allowed: false,
                },
                &serde_json::json!([{"claim": "email", "target": "email"}]),
                "email profile",
                true,
            )
            .await;
            assert!(
                result.is_err(),
                "OIDC provider without openid must be rejected"
            );
        }

        // A plain OAuth2 provider has no such requirement.
        provider_with(&pool, "discord", false, "identify email").await;
    }

    #[sqlx::test]
    async fn metadata_jwks_must_match_provider_is_oidc(pool: PgPool) {
        let oidc = provider_with(&pool, "oidc", true, "openid email").await;
        let plain = provider_with(&pool, "plain", false, "identify").await;
        let jwks = serde_json::json!({"keys": []});

        // OIDC: jwks is required, so an upsert carrying it succeeds.
        assert!(
            upsert_metadata(
                &pool,
                oidc,
                "https://i/a",
                "https://i/t",
                None,
                Some("https://i/j"),
                Some(&jwks),
                None
            )
            .await
            .is_ok()
        );
        // Plain OAuth2: jwks is forbidden, so carrying it is rejected by the trigger.
        assert!(
            upsert_metadata(
                &pool,
                plain,
                "https://i/a",
                "https://i/t",
                None,
                Some("https://i/j"),
                Some(&jwks),
                None
            )
            .await
            .is_err()
        );
    }

    #[sqlx::test]
    async fn issuer_present_iff_oidc(pool: PgPool) {
        let claims = serde_json::json!([{"claim": "email", "target": "email"}]);
        // OIDC without an issuer -> rejected by the issuer_oidc check.
        {
            let mut tx = pool.begin().await.unwrap();
            assert!(
                insert_provider(
                    &mut tx,
                    &Slug::try_from("aaa").unwrap(),
                    "A",
                    None,
                    "c",
                    &Ciphertext::for_test("s"),
                    crate::auth::ProviderFlags {
                        is_registration_allowed: true,
                        is_auto_connection_allowed: true,
                        is_email_verified: true,
                        is_disconnection_allowed: false,
                        is_unclaimed_username_connection_allowed: false,
                    },
                    &claims,
                    "openid email",
                    true,
                )
                .await
                .is_err()
            );
        }
        // Non-OIDC carrying an issuer -> rejected.
        {
            let mut tx = pool.begin().await.unwrap();
            assert!(
                insert_provider(
                    &mut tx,
                    &Slug::try_from("bbb").unwrap(),
                    "B",
                    Some("https://b.test"),
                    "c",
                    &Ciphertext::for_test("s"),
                    crate::auth::ProviderFlags {
                        is_registration_allowed: true,
                        is_auto_connection_allowed: true,
                        is_email_verified: true,
                        is_disconnection_allowed: false,
                        is_unclaimed_username_connection_allowed: false,
                    },
                    &claims,
                    "identify",
                    false,
                )
                .await
                .is_err()
            );
        }
        // Matched pairs are accepted.
        provider_with(&pool, "oidc", true, "openid email").await;
        provider_with(&pool, "plain", false, "identify").await;
    }

    #[sqlx::test]
    async fn the_local_slug_is_reserved_at_the_database(pool: PgPool) {
        // `local` names the built-in local-login method, so the auth_providers slug
        // CHECK must reject it regardless of kind (the DB backstop for the
        // app-level reserved-slug guard).
        let result = sqlx::query!(
            "INSERT INTO auth_providers (kind, slug, name) VALUES ('oauth2', 'local', 'Local')"
        )
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "the reserved `local` slug must be rejected"
        );
    }

    /// Raw metadata insert exercising the OIDC consistency trigger directly.
    async fn try_metadata(
        pool: &PgPool,
        provider_id: ProviderId,
        userinfo: Option<&str>,
        jwks_uri: Option<&str>,
        jwks: Option<serde_json::Value>,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_provider_metadata
                (provider_id, authorization_endpoint, token_endpoint, userinfo_endpoint,
                 jwks_uri, jwks)
            VALUES ($1, 'https://i/a', 'https://i/t', $2, $3, $4)
            "#,
            provider_id.0,
            userinfo,
            jwks_uri,
            jwks,
        )
        .execute(pool)
        .await
        .map(|_| ())
    }

    #[sqlx::test]
    async fn metadata_endpoint_requirements_follow_is_oidc(pool: PgPool) {
        let plain = provider_with(&pool, "plain", false, "identify").await;
        // Non-OIDC needs a userinfo endpoint (no id_token to read the profile from).
        assert!(try_metadata(&pool, plain, None, None, None).await.is_err());
        assert!(
            try_metadata(&pool, plain, Some("https://i/u"), None, None)
                .await
                .is_ok()
        );

        let oidc = provider_with(&pool, "oidc", true, "openid email").await;
        let jwks = serde_json::json!({"keys": []});
        // OIDC needs jwks (to verify id_token signatures); userinfo is optional.
        assert!(try_metadata(&pool, oidc, None, None, None).await.is_err());
        assert!(
            try_metadata(&pool, oidc, None, Some("https://i/j"), Some(jwks))
                .await
                .is_ok()
        );
    }
}
