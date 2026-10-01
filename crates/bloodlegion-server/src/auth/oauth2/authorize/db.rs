use sqlx::{PgPool, Result};

use crate::Slug;
use crate::auth::oauth2::Scope;
use crate::auth::providers::ProviderId;
use crate::crypto::TokenHash;
use crate::users::UserId;

/// The `(slug, kind)` of the provider with `id`, or `None`. A federated client
/// stores its provider by id; this resolves it back to the slug (for the login
/// endpoint path and the denial message) and kind (`'saml'` / `'oauth2'`, to route
/// to the right login endpoint).
pub async fn provider_ref(pool: &PgPool, id: ProviderId) -> Result<Option<(Slug, String)>> {
    let row = sqlx::query!(
        r#"SELECT slug AS "slug: Slug", kind FROM auth_providers WHERE id = $1"#,
        id.0,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| (row.slug, row.kind)))
}

pub async fn find_consented_scope(
    pool: &PgPool,
    user_id: UserId,
    client_id: &str,
) -> Result<Option<Scope>> {
    sqlx::query_scalar!(
        r#"
        SELECT scope AS "scope: Scope"
        FROM auth_oauth2_client_consents
        WHERE user_id = $1 AND client_id = $2
        "#,
        user_id.0,
        client_id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn upsert_consent(
    pool: &PgPool,
    user_id: UserId,
    client_id: &str,
    scope: &str,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO auth_oauth2_client_consents (user_id, client_id, scope)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, client_id)
        DO UPDATE SET scope = EXCLUDED.scope
        "#,
        user_id.0,
        client_id,
        scope,
    )
    .execute(pool)
    .await?;
    Ok(())
}

// An authorization code carries the full grant; the api layer passes the
// columns positionally rather than threading a struct through the db boundary.
#[allow(clippy::too_many_arguments)]
pub async fn insert_authorization_code(
    pool: &PgPool,
    code: &TokenHash,
    client_id: &str,
    user_id: UserId,
    redirect_uri: &str,
    scope: &str,
    nonce: Option<&str>,
    code_challenge: Option<&str>,
) -> Result<()> {
    // Only S256 is ever stored; the method is present exactly when a challenge is.
    let code_challenge_method = code_challenge.map(|_| "S256");
    sqlx::query!(
        r#"
        INSERT INTO auth_oauth2_client_authorization_codes
            (code_hash, client_id, user_id, redirect_uri, scope, nonce,
             code_challenge, code_challenge_method)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        code.as_ref(),
        client_id,
        user_id.0,
        redirect_uri,
        scope,
        nonce,
        code_challenge,
        code_challenge_method,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::oauth2::clients;
    use crate::crypto::generate_token;
    use crate::users::{self as users_db, CreateUserPayload};

    async fn user(pool: &PgPool) -> UserId {
        let mut tx = pool.begin().await.unwrap();
        let user = users_db::insert_user(
            &mut tx,
            &CreateUserPayload {
                username: "consentuser".to_string(),
                email: Some("consent@example.com".to_string()),
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

    async fn client(pool: &PgPool) -> String {
        let id = generate_token::<24>();
        let secret = TokenHash::new(&generate_token::<32>());
        clients::insert_client(
            pool,
            &id,
            &secret,
            "Consent Client",
            &["https://app.test/cb".to_string()],
        )
        .await
        .unwrap();
        id
    }

    #[sqlx::test]
    async fn consent_starts_absent_then_upserts_idempotently(pool: PgPool) {
        let user_id = user(&pool).await;
        let client_id = client(&pool).await;

        assert!(
            find_consented_scope(&pool, user_id, &client_id)
                .await
                .unwrap()
                .is_none(),
            "no consent should exist initially"
        );

        upsert_consent(&pool, user_id, &client_id, "openid email")
            .await
            .unwrap();
        assert_eq!(
            find_consented_scope(&pool, user_id, &client_id)
                .await
                .unwrap()
                .as_deref(),
            Some("openid email")
        );

        // A second grant for the same (user, client) replaces the scope set
        // rather than creating a duplicate row (ON CONFLICT DO UPDATE).
        upsert_consent(&pool, user_id, &client_id, "openid")
            .await
            .unwrap();
        assert_eq!(
            find_consented_scope(&pool, user_id, &client_id)
                .await
                .unwrap()
                .as_deref(),
            Some("openid")
        );
    }
}
