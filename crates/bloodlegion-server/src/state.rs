use std::{
    env::var,
    sync::{Arc, RwLock},
    time::Duration,
};

use anyhow::Context;
use base64ct::{Base64, Encoding};
use rsa::{pkcs1v15::SigningKey, sha2::Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::Result;
use crate::auth::jwk;
use crate::well_known::openid_configuration::OpenidConfiguration;

/// The database connection pool's ceiling, per instance.
///
/// One connection is held permanently by the `jwk_rotated` listener (a `PgListener` keeps its
/// pooled connection for its whole life), the cleaner takes one per sweep, and the rest serve HTTP
/// requests, one per in-flight request (no handler holds two: transactions are threaded as
/// `&mut PgConnection` rather than re-acquiring). 16 is plenty for a guild site and lets several
/// instances share a stock Postgres (`max_connections = 100`).
const POOL_MAX_CONNECTIONS: u32 = 16;

/// Connections held open when idle: the listener plus a couple for requests, so the permanently
/// held one does not churn open and closed as the pool reaps idle connections.
const POOL_MIN_CONNECTIONS: u32 = 3;

/// How long to wait for a free connection before failing.
///
/// This budget also covers opening a *new* connection (TCP, TLS, auth), which the pool does at
/// startup and again after idle reaping. Against a database across a network that alone can take
/// most of a second, so a tighter bound fails requests for reasons that have nothing to do with
/// contention.
const POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) fn required_env_var(name: &str) -> Result<String> {
    Ok(var(name).with_context(|| format!("required environment variable {name} is not set"))?)
}

#[derive(Clone)]
pub struct State {
    pub origin: Arc<str>,
    encryption_key: [u8; 32],
    /// The active signing key, swapped atomically when a rotation is signalled.
    active_signer: Arc<RwLock<Arc<ActiveSigner>>>,
    pub oidc_config: Arc<OpenidConfiguration>,
    pub pool: PgPool,
    /// Shared outbound HTTP client for every upstream the server calls: the OIDC
    /// discovery/JWKS/token/userinfo fetches against a login provider such as Battle.net.
    pub http_client: reqwest::Client,
}

/// The active JWT signing key and its key id.
pub struct ActiveSigner {
    pub kid: String,
    pub key: SigningKey<Sha256>,
}

impl State {
    pub async fn new() -> Result<Self> {
        let origin: Arc<str> = Self::load_origin()?.into();
        let encryption_key = Self::load_encryption_key()?;
        let oidc_config = Arc::new(Self::load_oidc_config(&origin));
        let pool = Self::load_pool().await?;
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("building the HTTP client")?;

        // A signing key is generated on first run, then loaded into memory.
        jwk::bootstrap(&pool, &encryption_key).await?;
        let active_signer = Arc::new(RwLock::new(Arc::new(
            Self::load_active_signer(&pool, &encryption_key).await?,
        )));

        Ok(Self {
            origin,
            encryption_key,
            active_signer,
            oidc_config,
            pool,
            http_client,
        })
    }

    async fn load_active_signer(pool: &PgPool, encryption_key: &[u8; 32]) -> Result<ActiveSigner> {
        let (kid, private_key) = jwk::load_active_private_key(pool, encryption_key).await?;
        Ok(ActiveSigner {
            kid,
            key: SigningKey::<Sha256>::new(private_key),
        })
    }

    /// The current active signer (a cheap `Arc` clone).
    pub fn active_signer(&self) -> Arc<ActiveSigner> {
        self.active_signer
            .read()
            .expect("active signer lock poisoned")
            .clone()
    }

    /// Reloads the active signing key from the database and swaps it in, invoked when a
    /// `jwk_rotated` notification arrives, so rotation takes effect live.
    pub async fn reload_active_signer(&self) -> Result<()> {
        let signer = Arc::new(Self::load_active_signer(&self.pool, &self.encryption_key).await?);
        *self
            .active_signer
            .write()
            .expect("active signer lock poisoned") = signer;
        Ok(())
    }

    /// Encrypts a secret for storage at rest with AES-256-GCM.
    pub fn encrypt(&self, plaintext: &str) -> Result<crate::crypto::Ciphertext> {
        crate::crypto::encrypt(&self.encryption_key, plaintext)
    }

    /// Decrypts a secret stored at rest with [`State::encrypt`].
    pub fn decrypt(&self, value: &crate::crypto::Ciphertext) -> Result<String> {
        crate::crypto::decrypt(&self.encryption_key, value)
    }

    pub fn load_origin() -> Result<String> {
        required_env_var("ORIGIN")
    }

    pub fn load_encryption_key() -> Result<[u8; 32]> {
        let encoded = required_env_var("ENCRYPTION_KEY")?;
        let bytes = Base64::decode_vec(encoded.trim()).context("ENCRYPTION_KEY: invalid base64")?;
        Ok(bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("ENCRYPTION_KEY: must decode to 32 bytes"))?)
    }

    pub fn load_oidc_config(origin: &str) -> OpenidConfiguration {
        OpenidConfiguration {
            issuer: origin.to_string(),
            authorization_endpoint: format!("{}/oauth2/authorize", origin),
            token_endpoint: format!("{}/auth/oauth2/token", origin),
            userinfo_endpoint: format!("{}/auth/oauth2/userinfo", origin),
            jwks_uri: format!("{}/.well-known/jwks.json", origin),
            grant_types_supported: &["authorization_code", "refresh_token"],
            response_types_supported: &["code"],
            subject_types_supported: &["public"],
            id_token_signing_alg_values_supported: &["RS256"],
            // `none` covers public (PKCE-only) clients, which authenticate at the token endpoint
            // with no secret; see `token::authenticate_client`.
            token_endpoint_auth_methods_supported: &[
                "client_secret_basic",
                "client_secret_post",
                "none",
            ],
            code_challenge_methods_supported: &["S256"],
            scopes_supported: &["email", "openid", "profile"],
        }
    }

    pub async fn load_pool() -> Result<PgPool> {
        let database_url = required_env_var("DATABASE_URL")?;
        Ok(PgPoolOptions::new()
            .acquire_timeout(POOL_ACQUIRE_TIMEOUT)
            .min_connections(POOL_MIN_CONNECTIONS)
            .max_connections(POOL_MAX_CONNECTIONS)
            .connect(&database_url)
            .await
            .context("failed to connect to DATABASE_URL")?)
    }
}
