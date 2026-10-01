use sqlx::{PgPool, Result};

use crate::auth::oauth2::Scope;
use crate::crypto::TokenHash;
use crate::users::UserId;

pub struct AuthorizationCode {
    /// NULL for a public client (no secret); present for a confidential one.
    pub secret: Option<TokenHash>,
    pub user_id: UserId,
    pub scope: Scope,
    pub nonce: Option<String>,
    pub code_challenge: Option<String>,
}

pub async fn find_authorization_code(
    pool: &PgPool,
    code: &TokenHash,
    client_id: &str,
    redirect_uri: &str,
) -> Result<Option<AuthorizationCode>> {
    sqlx::query_as!(
        AuthorizationCode,
        r#"
        SELECT
            c.secret_hash AS "secret: TokenHash",
            ac.user_id AS "user_id: UserId",
            ac.scope AS "scope: Scope",
            ac.nonce,
            ac.code_challenge
        FROM auth_oauth2_client_authorization_codes AS ac
        INNER JOIN auth_oauth2_clients AS c ON ac.client_id = c.id
        WHERE ac.code_hash = $1
            AND ac.client_id = $2
            AND ac.redirect_uri = $3
            AND ac.expires_at > now()
            AND ac.completed_at IS NULL
            AND c.disabled_at IS NULL
        "#,
        code.as_ref(),
        client_id,
        redirect_uri,
    )
    .fetch_optional(pool)
    .await
}

/// Atomically consumes an authorization code, recording the refresh-token
/// `family_id` it mints so a later replay can be tied back to those tokens.
/// Returns the number of rows updated: `1` for the winner, `0` if the code was
/// already consumed, expired, or never matched, the single-use gate.
pub async fn complete_authorization_code(
    pool: &PgPool,
    code: &TokenHash,
    client_id: &str,
    redirect_uri: &str,
    family_id: &str,
) -> Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE auth_oauth2_client_authorization_codes
        SET completed_at = now(), created_family_id = $4
        WHERE code_hash = $1
            AND client_id = $2
            AND redirect_uri = $3
            AND expires_at > now()
            AND completed_at IS NULL
        "#,
        code.as_ref(),
        client_id,
        redirect_uri,
        family_id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// The refresh-token family minted by an already-consumed code, if this code was
/// seen before. A non-`None` result means a code is being replayed, the token
/// endpoint revokes that family in response (RFC 9700 section 4.1.2).
pub async fn find_replayed_code_family(
    pool: &PgPool,
    code: &TokenHash,
    client_id: &str,
) -> Result<Option<String>> {
    sqlx::query_scalar!(
        r#"
        SELECT created_family_id
        FROM auth_oauth2_client_authorization_codes
        WHERE code_hash = $1 AND client_id = $2 AND completed_at IS NOT NULL
        "#,
        code.as_ref(),
        client_id,
    )
    .fetch_optional(pool)
    .await
    .map(Option::flatten)
}

/// Soft-revokes every still-live token in a refresh-token family. Idempotent.
/// Shared by refresh-token reuse detection and authorization-code replay.
pub async fn revoke_refresh_family(pool: &PgPool, family_id: &str) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE auth_oauth2_client_refresh_tokens
        SET revoked_at = now()
        WHERE family_id = $1 AND revoked_at IS NULL
        "#,
        family_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub enum RefreshOutcome {
    Valid {
        user_id: UserId,
        scope: Scope,
        family_id: String,
    },
    Reused,
    Invalid,
}

/// A client's authentication material. `secret_hash` is `None` for a public
/// client (which authenticates by PKCE, not a secret).
pub struct ClientAuth {
    pub secret: Option<TokenHash>,
}

/// Looks up a client's auth material. Returns `None` when no such client exists,
/// `Some(ClientAuth { secret: None })` for a public client.
pub async fn find_client_auth(pool: &PgPool, client_id: &str) -> Result<Option<ClientAuth>> {
    sqlx::query_as!(
        ClientAuth,
        r#"SELECT secret_hash AS "secret: TokenHash" FROM auth_oauth2_clients WHERE id = $1 AND disabled_at IS NULL"#,
        client_id
    )
    .fetch_optional(pool)
    .await
}

pub async fn consume_refresh_token(
    pool: &PgPool,
    token: &TokenHash,
    client_id: &str,
) -> Result<RefreshOutcome> {
    let consumed = sqlx::query!(
        r#"
        UPDATE auth_oauth2_client_refresh_tokens
        SET consumed_at = now()
        WHERE token_hash = $1
            AND client_id = $2
            AND consumed_at IS NULL
            AND revoked_at IS NULL
            AND expires_at > now()
        RETURNING user_id AS "user_id: UserId", scope AS "scope: Scope", family_id
        "#,
        token.as_ref(),
        client_id,
    )
    .fetch_optional(pool)
    .await?;

    if let Some(row) = consumed {
        return Ok(RefreshOutcome::Valid {
            user_id: row.user_id,
            scope: row.scope,
            family_id: row.family_id,
        });
    }

    let replayed_family = sqlx::query_scalar!(
        r#"
        SELECT family_id
        FROM auth_oauth2_client_refresh_tokens
        WHERE token_hash = $1 AND client_id = $2 AND consumed_at IS NOT NULL
        "#,
        token.as_ref(),
        client_id,
    )
    .fetch_optional(pool)
    .await?;

    let Some(family_id) = replayed_family else {
        return Ok(RefreshOutcome::Invalid);
    };

    // Reuse detected: soft-revoke the whole family (keeps the trail) rather than
    // deleting it. Any still-live token in the family becomes unusable.
    revoke_refresh_family(pool, &family_id).await?;

    Ok(RefreshOutcome::Reused)
}

pub async fn insert_refresh_token(
    pool: &PgPool,
    token: &TokenHash,
    family_id: &str,
    client_id: &str,
    user_id: UserId,
    scope: &Scope,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO auth_oauth2_client_refresh_tokens
            (token_hash, family_id, client_id, user_id, scope)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        token.as_ref(),
        family_id,
        client_id,
        user_id.0,
        scope.as_ref(),
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::oauth2::clients;
    use crate::crypto::{generate_token, hash_token};
    use crate::users::{self as users_db, CreateUserPayload};

    async fn user(pool: &PgPool, username: &str) -> UserId {
        let mut tx = pool.begin().await.unwrap();
        let user = users_db::insert_user(
            &mut tx,
            &CreateUserPayload {
                username: username.to_string(),
                email: Some(format!("{username}@example.com")),
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
            "Test Client",
            &["https://app.test/cb".to_string()],
        )
        .await
        .unwrap();
        id
    }

    async fn insert_code(
        pool: &PgPool,
        code: &TokenHash,
        client_id: &str,
        user_id: UserId,
        redirect_uri: &str,
    ) {
        // The default test client has is_pkce_required = true, so every code must
        // carry a challenge (enforced by the PKCE-required trigger).
        let code_challenge = hash_token(&generate_token::<32>());
        sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_client_authorization_codes
                (code_hash, client_id, user_id, redirect_uri, scope, nonce,
                 code_challenge, code_challenge_method)
            VALUES ($1, $2, $3, $4, 'openid', NULL, $5, 'S256')
            "#,
            code.as_ref(),
            client_id,
            user_id.0,
            redirect_uri,
            code_challenge,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    async fn refresh_token(
        pool: &PgPool,
        client_id: &str,
        user_id: UserId,
        family_id: &str,
    ) -> TokenHash {
        let token = TokenHash::new(&generate_token::<32>());
        let scope = Scope::try_from("openid").unwrap();
        insert_refresh_token(pool, &token, family_id, client_id, user_id, &scope)
            .await
            .unwrap();
        token
    }

    #[sqlx::test]
    async fn a_completed_code_is_set_once(pool: PgPool) {
        let user_id = user(&pool, "souser").await;
        let client_id = client(&pool).await;
        let code = TokenHash::new(&generate_token::<16>());
        let redirect = "https://app.test/cb";
        insert_code(&pool, &code, &client_id, user_id, redirect).await;
        let family = generate_token::<16>();
        complete_authorization_code(&pool, &code, &client_id, redirect, &family)
            .await
            .unwrap();

        // Once completed, completed_at can't be moved to a different instant...
        assert!(
            sqlx::query!(
                "UPDATE auth_oauth2_client_authorization_codes
                 SET completed_at = now() + interval '1 hour' WHERE code_hash = $1",
                code.as_ref(),
            )
            .execute(&pool)
            .await
            .is_err(),
            "rewriting completed_at must be rejected"
        );
        // ...and created_family_id (the replay -> family-revocation link) is fixed.
        assert!(
            sqlx::query!(
                "UPDATE auth_oauth2_client_authorization_codes
                 SET created_family_id = 'somethingelse' WHERE code_hash = $1",
                code.as_ref(),
            )
            .execute(&pool)
            .await
            .is_err(),
            "rewriting created_family_id must be rejected"
        );
    }

    #[sqlx::test]
    async fn authorization_code_is_single_use(pool: PgPool) {
        let user_id = user(&pool, "codeuser").await;
        let client_id = client(&pool).await;
        let code = TokenHash::new(&generate_token::<16>());
        let redirect = "https://app.test/cb";
        insert_code(&pool, &code, &client_id, user_id, redirect).await;

        // The first lookup resolves and carries the join'd client + claims.
        let found = find_authorization_code(&pool, &code, &client_id, redirect)
            .await
            .unwrap()
            .expect("code should be found");
        assert_eq!(found.user_id, user_id);
        assert_eq!(found.scope.as_ref(), "openid");

        // Completing it consumes it exactly once and records the created family.
        let family = generate_token::<16>();
        let first = complete_authorization_code(&pool, &code, &client_id, redirect, &family)
            .await
            .unwrap();
        assert_eq!(first, 1);
        let second = complete_authorization_code(&pool, &code, &client_id, redirect, &family)
            .await
            .unwrap();
        assert_eq!(second, 0, "a consumed code must not complete again");
        assert!(
            find_authorization_code(&pool, &code, &client_id, redirect)
                .await
                .unwrap()
                .is_none(),
            "a consumed code must not be findable"
        );
        // The consumed code now maps back to the family it minted, so a replay
        // can be tied to those tokens.
        assert_eq!(
            find_replayed_code_family(&pool, &code, &client_id)
                .await
                .unwrap()
                .as_deref(),
            Some(family.as_str()),
        );
    }

    #[sqlx::test]
    async fn replaying_a_consumed_code_revokes_its_token_family(pool: PgPool) {
        let user_id = user(&pool, "replayuser").await;
        let client_id = client(&pool).await;
        let code = TokenHash::new(&generate_token::<16>());
        let redirect = "https://app.test/cb";
        insert_code(&pool, &code, &client_id, user_id, redirect).await;

        // Redeem the code, minting a refresh-token family.
        let family = generate_token::<16>();
        assert_eq!(
            complete_authorization_code(&pool, &code, &client_id, redirect, &family)
                .await
                .unwrap(),
            1
        );
        let token = refresh_token(&pool, &client_id, user_id, &family).await;

        // A replay resolves to the recorded family; revoking it kills the tokens.
        let replayed = find_replayed_code_family(&pool, &code, &client_id)
            .await
            .unwrap()
            .expect("a consumed code reports its family");
        assert_eq!(replayed, family);
        revoke_refresh_family(&pool, &replayed).await.unwrap();
        assert!(matches!(
            consume_refresh_token(&pool, &token, &client_id)
                .await
                .unwrap(),
            RefreshOutcome::Invalid
        ));

        // An un-consumed code reports no family.
        let fresh = TokenHash::new(&generate_token::<16>());
        insert_code(&pool, &fresh, &client_id, user_id, redirect).await;
        assert!(
            find_replayed_code_family(&pool, &fresh, &client_id)
                .await
                .unwrap()
                .is_none()
        );
    }

    /// Inserts a confidential client with is_pkce_required = false plus one redirect
    /// uri (the >=1-uri invariant is deferred, so both go in one transaction).
    async fn client_without_pkce(pool: &PgPool) -> String {
        let id = generate_token::<24>();
        let secret_hash = hash_token(&generate_token::<32>());
        let mut tx = pool.begin().await.unwrap();
        sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_clients
                (id, secret_hash, name, client_type, is_pkce_required)
            VALUES ($1, $2, 'No PKCE', 'confidential', false)
            "#,
            id,
            secret_hash,
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_client_redirect_uris (client_id, uri)
            VALUES ($1, 'https://app.test/cb')
            "#,
            id,
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        id
    }

    #[sqlx::test]
    async fn pkce_required_trigger_enforces_challenge(pool: PgPool) {
        let user_id = user(&pool, "pkceuser").await;
        let redirect = "https://app.test/cb";

        // is_pkce_required client + a code with no challenge -> rejected by the trigger.
        let pkce_client = client(&pool).await;
        let no_challenge = sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_client_authorization_codes
                (code_hash, client_id, user_id, redirect_uri, scope)
            VALUES ($1, $2, $3, $4, 'openid')
            "#,
            hash_token(&generate_token::<16>()),
            pkce_client,
            user_id.0,
            redirect,
        )
        .execute(&pool)
        .await;
        assert!(
            no_challenge.is_err(),
            "a is_pkce_required client must reject a code with no challenge"
        );

        // Same client, code carrying a challenge -> accepted (insert_code adds one).
        insert_code(
            &pool,
            &TokenHash::new(&generate_token::<16>()),
            &pkce_client,
            user_id,
            redirect,
        )
        .await;

        // Non-PKCE client + a code with no challenge -> allowed.
        let open_client = client_without_pkce(&pool).await;
        sqlx::query!(
            r#"
            INSERT INTO auth_oauth2_client_authorization_codes
                (code_hash, client_id, user_id, redirect_uri, scope)
            VALUES ($1, $2, $3, $4, 'openid')
            "#,
            hash_token(&generate_token::<16>()),
            open_client,
            user_id.0,
            redirect,
        )
        .execute(&pool)
        .await
        .expect("a non-PKCE client may issue a code with no challenge");
    }

    #[sqlx::test]
    async fn authorization_code_requires_matching_redirect_uri(pool: PgPool) {
        let user_id = user(&pool, "rdruser").await;
        let client_id = client(&pool).await;
        let code = TokenHash::new(&generate_token::<16>());
        insert_code(&pool, &code, &client_id, user_id, "https://app.test/cb").await;

        assert!(
            find_authorization_code(&pool, &code, &client_id, "https://evil.test/cb")
                .await
                .unwrap()
                .is_none(),
            "a mismatched redirect_uri must not resolve the code"
        );
    }

    #[sqlx::test]
    async fn refresh_token_rotates_once(pool: PgPool) {
        let user_id = user(&pool, "refreshuser1").await;
        let client_id = client(&pool).await;
        let family = generate_token::<16>();
        let token = refresh_token(&pool, &client_id, user_id, &family).await;

        match consume_refresh_token(&pool, &token, &client_id)
            .await
            .unwrap()
        {
            RefreshOutcome::Valid {
                user_id: got_user,
                scope,
                family_id,
            } => {
                assert_eq!(got_user, user_id);
                assert_eq!(scope.as_ref(), "openid");
                assert_eq!(family_id, family);
            }
            _ => panic!("first use of a fresh token should be Valid"),
        }

        // Consuming the same token again is a replay, not a fresh rotation.
        assert!(matches!(
            consume_refresh_token(&pool, &token, &client_id)
                .await
                .unwrap(),
            RefreshOutcome::Reused
        ));
    }

    #[sqlx::test]
    async fn refresh_token_reuse_revokes_whole_family(pool: PgPool) {
        let user_id = user(&pool, "refreshuser2").await;
        let client_id = client(&pool).await;
        let family = generate_token::<16>();
        // Two tokens in one family, as if `sibling` was rotated from `first`.
        let first = refresh_token(&pool, &client_id, user_id, &family).await;
        let sibling = refresh_token(&pool, &client_id, user_id, &family).await;

        // Legitimately consume the first.
        assert!(matches!(
            consume_refresh_token(&pool, &first, &client_id)
                .await
                .unwrap(),
            RefreshOutcome::Valid { .. }
        ));
        // Replaying the consumed token is detected as reuse...
        assert!(matches!(
            consume_refresh_token(&pool, &first, &client_id)
                .await
                .unwrap(),
            RefreshOutcome::Reused
        ));
        // ...which revokes the entire family, so the unused sibling is gone too.
        assert!(matches!(
            consume_refresh_token(&pool, &sibling, &client_id)
                .await
                .unwrap(),
            RefreshOutcome::Invalid
        ));
    }

    #[sqlx::test]
    async fn refresh_token_unknown_is_invalid(pool: PgPool) {
        let client_id = client(&pool).await;
        let unknown = TokenHash::new(&generate_token::<32>());
        assert!(matches!(
            consume_refresh_token(&pool, &unknown, &client_id)
                .await
                .unwrap(),
            RefreshOutcome::Invalid
        ));
    }

    #[sqlx::test]
    async fn refresh_token_wrong_client_is_invalid(pool: PgPool) {
        let user_id = user(&pool, "refreshuser3").await;
        let client_id = client(&pool).await;
        let other_client = client(&pool).await;
        let token = refresh_token(&pool, &client_id, user_id, &generate_token::<16>()).await;

        // A token presented under a different client must not be consumable.
        assert!(matches!(
            consume_refresh_token(&pool, &token, &other_client)
                .await
                .unwrap(),
            RefreshOutcome::Invalid
        ));
    }
}
