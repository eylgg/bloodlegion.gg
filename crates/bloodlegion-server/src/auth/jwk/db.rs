use sqlx::{PgConnection, PgPool, Result};

use crate::crypto::Ciphertext;

pub struct JwkRecord {
    pub kid: String,
    pub n: String,
    pub e: String,
}

/// The active signer's kid and its encrypted PKCS#8 private key.
pub struct ActivePrivateKey {
    pub kid: String,
    pub private_key: Ciphertext,
}

pub async fn insert_public_key(conn: &mut PgConnection, kid: &str, n: &str, e: &str) -> Result<()> {
    sqlx::query!(
        "INSERT INTO auth_jwk_public_keys (kid, n, e) VALUES ($1, $2, $3)",
        kid,
        n,
        e,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn insert_private_key(
    conn: &mut PgConnection,
    kid: &str,
    private_key: &str,
    active: bool,
) -> Result<()> {
    // active=true -> deactivated_at stays NULL (default); active=false -> set immediately.
    // The one-active partial unique index enforces at most one NULL deactivated_at row.
    let deactivated_at: Option<time::OffsetDateTime> = if active {
        None
    } else {
        Some(time::OffsetDateTime::now_utc())
    };
    sqlx::query!(
        "INSERT INTO auth_jwk_private_keys (kid, private_key_ciphertext, deactivated_at) VALUES ($1, $2, $3)",
        kid,
        private_key,
        deactivated_at,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The current active signer, if any.
pub async fn find_active_private_key(pool: &PgPool) -> Result<Option<ActivePrivateKey>> {
    sqlx::query_as!(
        ActivePrivateKey,
        r#"SELECT kid, private_key_ciphertext AS "private_key: Ciphertext"
           FROM auth_jwk_private_keys WHERE deactivated_at IS NULL"#,
    )
    .fetch_optional(pool)
    .await
}

/// Retires the active key: sets its `deactivated_at` and schedules its public
/// half to stop being published after `grace_secs`. Returns the retired kid, if
/// any. Runs inside the rotation transaction.
pub async fn retire_active_key(conn: &mut PgConnection, grace_secs: i32) -> Result<Option<String>> {
    let Some(row) = sqlx::query!(
        "UPDATE auth_jwk_private_keys SET deactivated_at = now() WHERE deactivated_at IS NULL RETURNING kid"
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    sqlx::query!(
        "UPDATE auth_jwk_public_keys
         SET expires_at = now() + $2::double precision * interval '1 second'
         WHERE kid = $1 AND expires_at IS NULL",
        row.kid,
        grace_secs as f64,
    )
    .execute(&mut *conn)
    .await?;
    Ok(Some(row.kid))
}

/// A published public key (not revoked, not expired) by kid, for verifying a
/// token whose header names that kid.
pub async fn find_published_public_key(pool: &PgPool, kid: &str) -> Result<Option<JwkRecord>> {
    sqlx::query_as!(
        JwkRecord,
        "SELECT kid, n, e FROM auth_jwk_public_keys
         WHERE kid = $1
           AND revoked_at IS NULL
           AND (expires_at IS NULL OR expires_at > now())",
        kid,
    )
    .fetch_optional(pool)
    .await
}

/// All currently published public keys, for the JWKS endpoint.
pub async fn list_published_public_keys(pool: &PgPool) -> Result<Vec<JwkRecord>> {
    sqlx::query_as!(
        JwkRecord,
        "SELECT kid, n, e FROM auth_jwk_public_keys
         WHERE revoked_at IS NULL
           AND (expires_at IS NULL OR expires_at > now())
         ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
}

/// `NOTIFY`s running instances (channel `jwk_rotated`, carrying the new `kid`) to
/// hot-reload the active signing key after a rotation.
pub async fn notify_rotation(pool: &PgPool, kid: &str) -> Result<()> {
    sqlx::query!("SELECT pg_notify('jwk_rotated', $1)", kid)
        .fetch_optional(pool)
        .await?;
    Ok(())
}
