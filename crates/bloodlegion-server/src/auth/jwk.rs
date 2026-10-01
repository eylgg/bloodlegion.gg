mod db;

use anyhow::Context;
use base64ct::{Base64UrlUnpadded, Encoding};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding},
    sha2::{Digest, Sha256},
    traits::PublicKeyParts,
};
use sqlx::{PgConnection, PgPool};

use crate::Result;
use crate::crypto::{decrypt, encrypt};

/// How long a retired key stays published so tokens it already signed can still
/// be verified. Comfortably exceeds the access-token lifetime.
const RETIRE_GRACE_SECS: i32 = 24 * 60 * 60;

#[derive(Debug, serde::Serialize)]
pub struct WebKeySet {
    #[serde(rename = "keys")]
    pub web_keys: Vec<WebKey>,
}

#[derive(Debug, serde::Serialize)]
pub struct WebKey {
    pub kty: &'static str,
    pub r#use: &'static str,
    pub alg: &'static str,
    pub kid: String,
    pub n: String,
    pub e: String,
}

/// Base64url-encodes an RSA component (modulus or exponent) from its big-endian
/// bytes, the wire form JWK `n`/`e` and the RFC 7638 thumbprint both use.
macro_rules! encode {
    ($num:expr) => {
        Base64UrlUnpadded::encode_string(&$num.to_be_bytes_trimmed_vartime())
    };
}

/// The RFC 7638 JWK thumbprint of the key's public half, used as its stable `kid`.
fn calculate_thumbprint(private_key: &RsaPrivateKey) -> String {
    let public_key = RsaPublicKey::from(private_key);
    let e = encode!(public_key.e());
    let n = encode!(public_key.n());
    let canonical_json = format!(r#"{{"e":"{}","kty":"RSA","n":"{}"}}"#, e, n);
    Base64UrlUnpadded::encode_string(&Sha256::digest(canonical_json.as_bytes()))
}

fn public_components(private_key: &RsaPrivateKey) -> (String, String) {
    let public_key = RsaPublicKey::from(private_key);
    (encode!(public_key.n()), encode!(public_key.e()))
}

/// A freshly generated signing key, ready to persist. Produced by the slow
/// RSA/encrypt step so it can be done *before* opening a transaction.
struct KeyMaterial {
    kid: String,
    n: String,
    e: String,
    encrypted: crate::crypto::Ciphertext,
}

/// Generates an RSA-2048 signing key and its encrypted PKCS#8 form. No I/O, the
/// caller persists it inside whatever transaction it needs.
fn generate_key_material(encryption_key: &[u8; 32]) -> Result<KeyMaterial> {
    let mut rng = rand::rng();
    let private_key =
        RsaPrivateKey::new(&mut rng, 2048).context("generating an RSA signing key")?;
    let kid = calculate_thumbprint(&private_key);
    let (n, e) = public_components(&private_key);
    let pkcs8 = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .context("encoding the signing key as PKCS#8")?;
    let encrypted = encrypt(encryption_key, &pkcs8)?;
    Ok(KeyMaterial {
        kid,
        n,
        e,
        encrypted,
    })
}

/// Persists a generated key (public + encrypted private halves) on `conn`.
async fn store_key(conn: &mut PgConnection, material: &KeyMaterial, active: bool) -> Result<()> {
    db::insert_public_key(&mut *conn, &material.kid, &material.n, &material.e)
        .await
        .context("persisting the public key")?;
    db::insert_private_key(
        &mut *conn,
        &material.kid,
        material.encrypted.as_ref(),
        active,
    )
    .await
    .context("persisting the private key")?;
    Ok(())
}

/// Generates a fresh RSA-2048 signing key and stores it: the public half in
/// `auth_jwk_public_keys`, the PKCS#8 private half encrypted in
/// `auth_jwk_private_keys`. When `active`, it becomes the signer. Returns the kid.
async fn generate_and_store(
    pool: &PgPool,
    encryption_key: &[u8; 32],
    active: bool,
) -> Result<String> {
    let material = generate_key_material(encryption_key)?;
    let mut tx = pool
        .begin()
        .await
        .context("beginning the key-store transaction")?;
    store_key(&mut tx, &material, active).await?;
    tx.commit()
        .await
        .context("committing the new signing key")?;
    Ok(material.kid)
}

/// Ensures an active signing key exists, generating one on first run.
pub async fn bootstrap(pool: &PgPool, encryption_key: &[u8; 32]) -> Result<()> {
    if db::find_active_private_key(pool)
        .await
        .context("looking up the active signing key")?
        .is_some()
    {
        return Ok(());
    }
    // Two instances cold-starting against a fresh database both see no active key
    // and both try to generate one; the `one active key` partial unique index lets
    // exactly one insert win. If our insert lost the race, an active key now exists,
    // so treat the loss as success rather than crashing this instance's startup.
    match generate_and_store(pool, encryption_key, true).await {
        Ok(_) => Ok(()),
        Err(error) => {
            if db::find_active_private_key(pool)
                .await
                .context("re-checking the active signing key after a failed bootstrap")?
                .is_some()
            {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

/// Loads and decrypts the active signing key as `(kid, private_key)`.
pub async fn load_active_private_key(
    pool: &PgPool,
    encryption_key: &[u8; 32],
) -> Result<(String, RsaPrivateKey)> {
    let active = db::find_active_private_key(pool)
        .await
        .context("loading the active signing key")?
        .context("no active signing key is present")?;
    let pkcs8 = decrypt(encryption_key, &active.private_key)?;
    let private_key =
        RsaPrivateKey::from_pkcs8_pem(&pkcs8).context("decoding the stored signing key")?;
    Ok((active.kid, private_key))
}

/// Rotates the signing key: retires the current active key (kept published for a
/// grace period) and generates a new active one. Returns the new kid.
pub async fn rotate(pool: &PgPool, encryption_key: &[u8; 32]) -> Result<String> {
    // Generate the (slow) key material before opening the transaction, then retire
    // the old key and install the new active one in ONE transaction: there must
    // never be a committed window with zero active signing keys (which would break
    // token issuance fleet-wide), and a crash mid-rotation must not strand the
    // system with the old key retired and no replacement.
    let material = generate_key_material(encryption_key)?;
    let mut tx = pool
        .begin()
        .await
        .context("beginning the rotation transaction")?;
    db::retire_active_key(&mut tx, RETIRE_GRACE_SECS)
        .await
        .context("retiring the current signing key")?;
    store_key(&mut tx, &material, true).await?;
    tx.commit().await.context("committing the key rotation")?;
    Ok(material.kid)
}

/// Signals running instances to hot-reload the active signing key after a
/// rotation, so the new key takes effect fleet-wide without a restart.
pub async fn notify_rotation(pool: &PgPool, kid: &str) -> Result<()> {
    db::notify_rotation(pool, kid)
        .await
        .context("notifying instances of the rotation")?;
    Ok(())
}

/// Builds a verifying key for the published key named by `kid`, if it is still
/// published (not revoked or expired).
pub async fn find_published_jwk(pool: &PgPool, kid: &str) -> Result<Option<(String, String)>> {
    Ok(db::find_published_public_key(pool, kid)
        .await
        .context("looking up the published JWK")?
        .map(|record| (record.n, record.e)))
}

pub async fn list_jwks(pool: &PgPool) -> Result<WebKeySet> {
    let records = db::list_published_public_keys(pool)
        .await
        .context("failed to retrieve JWK records")?;
    let web_keys = records
        .into_iter()
        .map(|record| WebKey {
            kty: "RSA",
            r#use: "sig",
            alg: "RS256",
            kid: record.kid,
            n: record.n,
            e: record.e,
        })
        .collect();
    Ok(WebKeySet { web_keys })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];

    #[sqlx::test]
    async fn bootstrap_is_idempotent_then_rotate_keeps_old_key_published(pool: PgPool) {
        bootstrap(&pool, &KEY).await.unwrap();
        let (kid1, _) = load_active_private_key(&pool, &KEY).await.unwrap();
        assert!(find_published_jwk(&pool, &kid1).await.unwrap().is_some());
        assert_eq!(list_jwks(&pool).await.unwrap().web_keys.len(), 1);

        // A second bootstrap is a no-op, the active key is unchanged.
        bootstrap(&pool, &KEY).await.unwrap();
        let (kid1_again, _) = load_active_private_key(&pool, &KEY).await.unwrap();
        assert_eq!(kid1, kid1_again);

        // Rotation installs a new active key; the old one stays published during
        // its grace window so its tokens still verify.
        let kid2 = rotate(&pool, &KEY).await.unwrap();
        assert_ne!(kid1, kid2);
        let (active_kid, _) = load_active_private_key(&pool, &KEY).await.unwrap();
        assert_eq!(active_kid, kid2);
        assert!(find_published_jwk(&pool, &kid1).await.unwrap().is_some());
        assert!(find_published_jwk(&pool, &kid2).await.unwrap().is_some());
        assert_eq!(list_jwks(&pool).await.unwrap().web_keys.len(), 2);
    }
}
