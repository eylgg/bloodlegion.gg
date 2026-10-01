use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit as _},
};
use anyhow::Context;
use argon2::{
    Argon2,
    password_hash::{
        PasswordHash as Argon2Hash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng,
    },
};
use base64ct::{Base64UrlUnpadded, Encoding};
use rand::Rng;
use rsa::{
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    sha2::{Digest, Sha256},
    signature::{RandomizedSigner, SignatureEncoding, Verifier},
};

use subtle::ConstantTimeEq;

use crate::Result;

pub fn generate_token<const N: usize>() -> String {
    let mut bytes = [0u8; N];
    rand::rng().fill_bytes(&mut bytes);
    Base64UrlUnpadded::encode_string(&bytes)
}

/// Hashes a high-entropy token (e.g., a session ID from [`generate_token`])
/// for storage and lookup, returning the base64url-encoded SHA-256 digest.
///
/// We deliberately chose SHA-256 (instead of Argon2) because the input
/// consists of CSPRNG output with no structure for an attacker to brute-force.
/// This approach requires no salt or key stretching, which keeps the hashing
/// fast on every request. Never use this function for low-entropy secrets
/// like passwords, hash those using Argon2 instead.
pub fn hash_token(data: &str) -> String {
    let input = Sha256::digest(data.as_bytes());
    Base64UrlUnpadded::encode_string(&input)
}

/// Constant-time equality for two token hashes (or any equal-length secret
/// strings), so a comparison can't leak how many leading bytes matched via its
/// timing. `subtle` short-circuits only on a length difference, which is public
/// here (both sides are fixed 43-char base64url SHA-256 digests).
pub fn ct_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// A SHA-256 token hash (43-char base64url). The only constructor hashes, so a
/// value of this type *proves* it was hashed, a raw token can never reach a
/// column typed `TokenHash`. No `Debug`/`Display` (a hash can't leak into a
/// log); reads come back through the transparent sqlx `Decode`.
#[derive(Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct TokenHash(String);

impl TokenHash {
    pub fn new(raw: &str) -> Self {
        Self(hash_token(raw))
    }
}

impl AsRef<str> for TokenHash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// An Argon2 password hash (PHC string). Built only by hashing a plaintext and
/// verified through its own methods, so the hash material has one clear path in
/// and one out. No `Debug`/`Display`.
#[derive(Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct PasswordHash(String);

impl PasswordHash {
    pub async fn new(plaintext: &str) -> Result<Self> {
        Ok(Self(hash_password(plaintext).await?))
    }

    pub async fn verify(&self, candidate: &str) -> bool {
        verify_password(candidate, &self.0).await
    }

    pub async fn verify_detailed(&self, candidate: &str) -> Result<PasswordVerification> {
        verify_password_detailed(candidate, &self.0).await
    }
}

impl AsRef<str> for PasswordHash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// AES-256-GCM ciphertext (base64url nonce||ct||tag). Built only by [`encrypt`]
/// (reached via `State::encrypt`) and read back through the transparent sqlx
/// `Decode`; there is no constructor that wraps an arbitrary string, so
/// plaintext can't be stored where ciphertext belongs. No `Debug`/`Display`.
#[derive(Clone, sqlx::Type)]
#[sqlx(transparent)]
pub struct Ciphertext(String);

impl AsRef<str> for Ciphertext {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
impl Ciphertext {
    /// Test-only: wrap a literal as stand-in ciphertext for fixtures that store
    /// a provider/secret but never decrypt it. Not available outside tests, so
    /// production code still has `State::encrypt` as the only constructor.
    pub fn for_test(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// Hashes a password with Argon2. The CPU-bound work runs on Tokio's blocking
/// thread pool so it never stalls the async runtime under concurrent load.
pub async fn hash_password(password: &str) -> Result<String> {
    let password = password.to_owned();
    let hash = tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
    })
    .await
    .context("joining the password-hashing task")?
    .context("hashing the password with Argon2")?;
    Ok(hash)
}

/// Whether `password` matches an Argon2 PHC `hash`, returning `false` for an
/// unusable hash. The CPU-bound verification runs on the blocking thread pool.
pub async fn verify_password(password: &str, hash: &str) -> bool {
    let password = password.to_owned();
    let hash = hash.to_owned();
    tokio::task::spawn_blocking(move || {
        let Ok(parsed) = Argon2Hash::new(&hash) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
    .await
    .unwrap_or(false)
}

/// Runs an Argon2 verification against a fixed decoy hash, discarding the
/// result. Used by the login handler on the "unknown username" and "no password
/// set" paths so they pay the same CPU cost as a real verification, otherwise
/// the response time reveals which usernames exist (defeating the uniform
/// "invalid username or password" message). The decoy hash is generated once
/// per process with a random salt; the plaintext it encodes is irrelevant.
pub async fn verify_decoy_password(password: &str) {
    static DECOY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(b"decoy-password-never-matches", &salt)
            .expect("hashing the decoy password")
            .to_string()
    });
    let _ = verify_password(password, &DECOY_HASH).await;
}

/// Outcome of verifying a password against a stored Argon2 hash.
pub enum PasswordVerification {
    /// The password matches the stored hash.
    Matches,
    /// The password does not match (Argon2's constant-time mismatch signal).
    Mismatch,
}

/// Verifies `password` against an Argon2 PHC `hash` on the blocking thread
/// pool, preserving Argon2's constant-time mismatch signal. Returns `Err`
/// only when the stored hash is unusable or the hasher fails unexpectedly, a
/// plain mismatch is `Ok(PasswordVerification::Mismatch)`, not an error.
pub async fn verify_password_detailed(password: &str, hash: &str) -> Result<PasswordVerification> {
    let password = password.to_owned();
    let hash = hash.to_owned();
    let verification = tokio::task::spawn_blocking(move || {
        let parsed = Argon2Hash::new(&hash)?;
        match Argon2::default().verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(PasswordVerification::Matches),
            Err(argon2::password_hash::Error::Password) => Ok(PasswordVerification::Mismatch),
            Err(e) => Err(e),
        }
    })
    .await
    .context("joining the password-verification task")?
    .context("verifying the password with Argon2")?;
    Ok(verification)
}

const NONCE_LEN: usize = 12;

/// Encrypts `plaintext` with AES-256-GCM under a 32-byte `key`, returning
/// base64url(nonce || ciphertext || tag).
pub fn encrypt(key: &[u8; 32], plaintext: &str) -> Result<Ciphertext> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| anyhow::anyhow!("invalid encryption key"))?;
    let mut nonce = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(Ciphertext(Base64UrlUnpadded::encode_string(&combined)))
}

/// Decrypts a value produced by [`encrypt`].
pub fn decrypt(key: &[u8; 32], value: &Ciphertext) -> Result<String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| anyhow::anyhow!("invalid encryption key"))?;
    let combined = Base64UrlUnpadded::decode_vec(&value.0)
        .map_err(|_| anyhow::anyhow!("invalid ciphertext"))?;
    if combined.len() < NONCE_LEN {
        return Err(anyhow::anyhow!("ciphertext too short").into());
    }
    let (nonce, ciphertext) = combined.split_at(NONCE_LEN);
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed"))?;
    Ok(String::from_utf8(plaintext).context("decrypted value is not valid UTF-8")?)
}

pub fn generate_signature(signing_key: &SigningKey<Sha256>, msg: &[u8]) -> String {
    let mut rng = rand::rng();
    let signature = signing_key.sign_with_rng(&mut rng, msg);
    Base64UrlUnpadded::encode_string(signature.to_bytes().as_ref())
}

pub fn verify_signature(verifying_key: &VerifyingKey<Sha256>, msg: &[u8], signature: &str) -> bool {
    let Ok(bytes) = Base64UrlUnpadded::decode_vec(signature) else {
        return false;
    };
    let Ok(signature) = Signature::try_from(bytes.as_slice()) else {
        return false;
    };
    verifying_key.verify(msg, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_round_trips() {
        let key = [7u8; 32];
        let ciphertext = encrypt(&key, "client-secret").unwrap();
        assert_eq!(decrypt(&key, &ciphertext).unwrap(), "client-secret");
    }

    #[test]
    fn encrypt_uses_a_fresh_nonce() {
        let key = [7u8; 32];
        assert_ne!(encrypt(&key, "x").unwrap().0, encrypt(&key, "x").unwrap().0);
    }

    #[test]
    fn decrypt_rejects_wrong_key_and_tampering() {
        let key = [7u8; 32];
        let ciphertext = encrypt(&key, "secret").unwrap();
        assert!(decrypt(&[8u8; 32], &ciphertext).is_err());
        assert!(decrypt(&key, &Ciphertext(format!("{}A", ciphertext.0))).is_err());
        assert!(decrypt(&key, &Ciphertext(String::new())).is_err());
    }
}
