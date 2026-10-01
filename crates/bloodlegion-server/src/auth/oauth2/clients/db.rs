use sqlx::{PgPool, Result};
use time::OffsetDateTime;

use super::{Client, ClientSummary, FederatedIdentity};
use crate::Slug;
use crate::auth::oauth2::Scope;
use crate::auth::providers::ProviderId;
use crate::crypto::TokenHash;
use crate::users::UserId;

/// The id of the OAuth2 provider named by `slug`, or `None`: resolves a
/// client's requested identity provider (given by slug on the admin API) to the id
/// stored in `identity_provider_id`. The built-in local provider is excluded: a
/// client can't federate to local (it has no per-provider credential to source a
/// federated identity from), so `local` resolves to `None` here and the caller
/// rejects it like any unknown slug.
pub async fn find_provider_id_by_slug(pool: &PgPool, slug: &str) -> Result<Option<ProviderId>> {
    sqlx::query_scalar!(
        r#"SELECT id AS "id: ProviderId" FROM auth_providers WHERE slug = $1 AND kind <> 'local'"#,
        slug,
    )
    .fetch_optional(pool)
    .await
}

/// The id of the provider a client is federated to (`identity_provider_id`), or
/// `None` when unrestricted or the client is gone.
pub async fn find_identity_provider_id(
    pool: &PgPool,
    client_id: &str,
) -> Result<Option<ProviderId>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT identity_provider_id AS "id: ProviderId" FROM auth_oauth2_clients WHERE id = $1"#,
        client_id,
    )
    .fetch_optional(pool)
    .await?
    .flatten())
}

/// The user's live credential profile at `provider_id`, plus that
/// provider's email-verification trust flag. `None` when the user has no active
/// credential there, the caller treats that as "not connected" and fails closed.
pub async fn find_federated_identity(
    pool: &PgPool,
    user_id: UserId,
    provider_id: ProviderId,
) -> Result<Option<FederatedIdentity>> {
    sqlx::query_as!(
        FederatedIdentity,
        r#"
        SELECT c.profile AS "profile!", p.is_email_verified AS "provider_email_verified!"
        FROM auth_oauth2_provider_credentials c
        JOIN auth_providers p ON p.id = c.provider_id
        WHERE c.user_id = $1 AND c.provider_id = $2 AND c.disconnected_at IS NULL
        "#,
        user_id.0,
        provider_id.0,
    )
    .fetch_optional(pool)
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_client(
    pool: &PgPool,
    id: &str,
    secret: Option<&TokenHash>,
    name: &str,
    client_type: &str,
    is_pkce_required: bool,
    is_email_masked: bool,
    auto_consent_scope: &str,
    identity_provider_id: Option<i64>,
    redirect_uris: &[String],
) -> Result<OffsetDateTime> {
    let secret = secret.map(TokenHash::as_ref);
    let mut tx = pool.begin().await?;
    let created_at = sqlx::query_scalar!(
        r#"
        INSERT INTO auth_oauth2_clients
            (id, secret_hash, name, client_type, is_pkce_required, is_email_masked,
             auto_consent_scope, identity_provider_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING created_at
        "#,
        id,
        secret,
        name,
        client_type,
        is_pkce_required,
        is_email_masked,
        auto_consent_scope,
        identity_provider_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    for uri in redirect_uris {
        sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_client_redirect_uris (client_id, uri)
            VALUES ($1, $2)
            "#,
            id,
            uri,
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(created_at)
}

pub async fn find_client(pool: &PgPool, client_id: &str) -> Result<Option<ClientSummary>> {
    sqlx::query_as!(
        ClientSummary,
        r#"
        SELECT
            c.name,
            c.is_pkce_required,
            c.auto_consent_scope AS "auto_consent_scope: Scope",
            c.identity_provider_id AS "identity_provider_id: ProviderId",
            array_agg(r.uri ORDER BY r.id) AS "redirect_uris!"
        FROM auth_oauth2_clients AS c
        INNER JOIN auth_oauth2_client_redirect_uris AS r ON c.id = r.client_id
        WHERE c.id = $1 AND c.disabled_at IS NULL
        GROUP BY c.id
        "#,
        client_id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn delete_client(pool: &PgPool, id: &str) -> Result<()> {
    sqlx::query!("DELETE FROM auth_oauth2_clients WHERE id = $1", id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Whether the given client wants masked (origin-domain) emails in userinfo.
/// `None` when there is no such client.
pub async fn find_email_masked(pool: &PgPool, client_id: &str) -> Result<Option<bool>> {
    sqlx::query_scalar!(
        "SELECT is_email_masked FROM auth_oauth2_clients WHERE id = $1",
        client_id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn list_clients(pool: &PgPool) -> Result<Vec<Client>> {
    sqlx::query_as!(
        Client,
        r#"
        SELECT
            c.id,
            c.name,
            c.client_type,
            c.is_pkce_required,
            c.is_email_masked,
            c.auto_consent_scope AS "auto_consent_scope: Scope",
            p.slug AS "identity_provider_slug?: Slug",
            c.created_at,
            c.updated_at,
            array_agg(r.uri ORDER BY r.id) as "redirect_uris!"
        FROM auth_oauth2_clients c
        INNER JOIN auth_oauth2_client_redirect_uris r ON c.id = r.client_id
        LEFT JOIN auth_providers p ON p.id = c.identity_provider_id
        GROUP BY c.id, p.slug
        ORDER BY c.created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{generate_token, hash_token};

    #[sqlx::test]
    async fn insert_then_list_round_trips_name_and_redirect_uris(pool: PgPool) {
        let id = generate_token::<24>();
        let secret = TokenHash::new(&generate_token::<32>());
        insert_client(
            &pool,
            &id,
            Some(&secret),
            "Round Trip",
            "confidential",
            true,
            false,
            "openid profile",
            None,
            &[
                "https://a.test/cb".to_string(),
                "https://b.test/cb".to_string(),
            ],
        )
        .await
        .unwrap();

        let clients = list_clients(&pool).await.unwrap();
        let client = clients
            .iter()
            .find(|client| client.id == id)
            .expect("client should be listed");
        assert_eq!(client.name, "Round Trip");
        assert_eq!(&*client.auto_consent_scope, "openid profile");
        assert_eq!(
            client.redirect_uris,
            ["https://a.test/cb", "https://b.test/cb"]
        );
    }

    #[sqlx::test]
    async fn secret_shape_ties_the_secret_to_the_client_type(pool: PgPool) {
        // Everything but the (client_type, secret_hash) pairing is valid, so a
        // rejection can only be the secret-shape CHECK firing immediately on the
        // row (it is a plain CHECK, evaluated before the deferred URI trigger).

        // A confidential client must have a secret.
        let confidential_without_secret = sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_clients (id, name, client_type, secret_hash)
            VALUES ($1, 'Test Client', 'confidential', NULL)
            "#,
            generate_token::<24>(),
        )
        .execute(&pool)
        .await;
        assert!(
            confidential_without_secret.is_err(),
            "a confidential client with no secret must be rejected"
        );

        // A public client must not have one.
        let public_with_secret = sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_clients (id, name, client_type, secret_hash)
            VALUES ($1, 'Test Client', 'public', $2)
            "#,
            generate_token::<24>(),
            hash_token(&generate_token::<32>()),
        )
        .execute(&pool)
        .await;
        assert!(
            public_with_secret.is_err(),
            "a public client with a secret must be rejected"
        );
    }

    #[sqlx::test]
    async fn find_provider_id_by_slug_resolves_registered_providers(pool: PgPool) {
        assert!(
            find_provider_id_by_slug(&pool, "utoronto")
                .await
                .unwrap()
                .is_none()
        );
        // `local` is the built-in local-login provider, not a federatable identity
        // provider, so it never resolves as a federation target.
        assert!(
            find_provider_id_by_slug(&pool, "local")
                .await
                .unwrap()
                .is_none()
        );

        // The envelope + body must commit together (deferred "has a body" trigger).
        let mut tx = pool.begin().await.unwrap();
        let id = seed_oauth2_provider(&mut tx, "utoronto", "U of T", false).await;
        tx.commit().await.unwrap();

        assert_eq!(
            find_provider_id_by_slug(&pool, "utoronto").await.unwrap(),
            Some(ProviderId(id))
        );
    }

    /// Inserts an OAuth2 provider (envelope + body) and returns its id. The deferred
    /// "has a body" trigger means both rows must land in the caller's transaction.
    async fn seed_oauth2_provider(
        tx: &mut sqlx::PgConnection,
        slug: &str,
        name: &str,
        is_email_verified: bool,
    ) -> i64 {
        let id = sqlx::query_scalar!(
            r#"INSERT INTO auth_providers (kind, slug, name, is_email_verified)
               VALUES ('oauth2', $1, $2, $3) RETURNING id"#,
            slug,
            name,
            is_email_verified,
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query!(
            r#"INSERT INTO auth_oauth2_providers
                   (provider_id, issuer, client_id, client_secret_ciphertext, claims, scope, is_oidc)
               VALUES ($1, $2, 'client', 'secret', '[]'::jsonb, 'openid', true)"#,
            id,
            format!("https://{slug}.test"),
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        id
    }

    #[sqlx::test]
    async fn federated_identity_reflects_a_live_oauth2_credential(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        let user = crate::users::insert_user(
            &mut tx,
            &crate::users::CreateUserPayload {
                username: "someone".into(),
                email: Some("someone@example.com".into()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            true,
        )
        .await
        .unwrap();
        let provider_id = seed_oauth2_provider(&mut tx, "utoronto", "U of T", true).await;
        sqlx::query!(
            r#"INSERT INTO auth_oauth2_provider_credentials
                   (provider_id, user_id, subject, raw_userinfo, profile)
               VALUES ($1, $2, 'sub-1', '{}'::jsonb, $3)"#,
            provider_id,
            user.id.0,
            serde_json::json!({"username": "utor123", "email": "utor123@utoronto.ca"}),
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // Connected: the profile and the provider's verification flag come back.
        let found = find_federated_identity(&pool, user.id, ProviderId(provider_id))
            .await
            .unwrap()
            .expect("the user is connected");
        assert_eq!(
            found.profile.get("username").and_then(|v| v.as_str()),
            Some("utor123")
        );
        assert!(found.provider_email_verified);

        // A provider the user has no credential at -> not connected.
        assert!(
            find_federated_identity(&pool, user.id, ProviderId(provider_id + 1))
                .await
                .unwrap()
                .is_none()
        );

        // Disconnecting the credential drops the federated identity (fail-closed).
        sqlx::query!(
            "UPDATE auth_oauth2_provider_credentials SET disconnected_at = now() WHERE user_id = $1",
            user.id.0,
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            find_federated_identity(&pool, user.id, ProviderId(provider_id))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn a_provider_backing_a_client_cannot_be_deleted(pool: PgPool) {
        // Register a provider and a client federated to it.
        let mut tx = pool.begin().await.unwrap();
        let provider_id = seed_oauth2_provider(&mut tx, "utoronto", "U of T", false).await;
        tx.commit().await.unwrap();

        let client_id = generate_token::<24>();
        let secret = TokenHash::new(&generate_token::<32>());
        insert_client(
            &pool,
            &client_id,
            Some(&secret),
            "Fed",
            "confidential",
            true,
            false,
            "",
            Some(provider_id),
            &["https://app.test/cb".to_string()],
        )
        .await
        .unwrap();

        // ON DELETE RESTRICT: the provider can't be deleted while a client federates
        // to it (the dependency surfaces instead of silently opening the gate).
        let result = sqlx::query!("DELETE FROM auth_providers WHERE id = $1", provider_id)
            .execute(&pool)
            .await;
        assert!(
            result.is_err(),
            "a provider still backing a client must not be deletable"
        );
    }

    #[sqlx::test]
    async fn a_client_with_no_redirect_uri_cannot_commit(pool: PgPool) {
        let id = generate_token::<24>();
        let secret_hash = hash_token(&generate_token::<32>());

        let mut tx = pool.begin().await.unwrap();
        sqlx::query!(
            "INSERT INTO auth_oauth2_clients (id, secret_hash, name) VALUES ($1, $2, 'No URIs')",
            id,
            secret_hash,
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        // The deferred constraint trigger requires >= 1 redirect URI, checked
        // at commit, so the otherwise-valid INSERT must fail to commit.
        assert!(
            tx.commit().await.is_err(),
            "a client with no redirect URIs must fail the deferred constraint"
        );
    }
}
