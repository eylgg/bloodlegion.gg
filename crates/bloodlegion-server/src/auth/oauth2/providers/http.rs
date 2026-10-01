#[derive(serde::Deserialize)]
pub struct Discovery {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    pub jwks_uri: String,
}

// Serialize so the fetched JWKS can be cached as jsonb in the metadata table.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct Jwks {
    pub keys: Vec<Jwk>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Jwk {
    pub kid: String,
    pub n: String,
    pub e: String,
}

/// The token endpoint's response. `id_token` is present only for OIDC; a plain
/// OAuth2 provider returns just the `access_token`, which is used to call userinfo.
#[derive(serde::Deserialize)]
pub struct Tokens {
    pub access_token: String,
    #[serde(default)]
    pub id_token: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct IdTokenClaims {
    pub sub: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: Option<bool>,
    #[serde(default)]
    pub given_name: Option<String>,
    #[serde(default)]
    pub family_name: Option<String>,
    #[serde(default)]
    pub nonce: Option<String>,
}

pub async fn fetch_discovery(http: &reqwest::Client, issuer: &str) -> reqwest::Result<Discovery> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    http.get(&url)
        .send()
        .await?
        .error_for_status()?
        .json::<Discovery>()
        .await
}

pub async fn fetch_jwks(http: &reqwest::Client, jwks_uri: &str) -> reqwest::Result<Jwks> {
    http.get(jwks_uri)
        .send()
        .await?
        .error_for_status()?
        .json::<Jwks>()
        .await
}

#[allow(clippy::too_many_arguments)]
pub async fn exchange_code(
    http: &reqwest::Client,
    token_endpoint: &str,
    code: &str,
    redirect_uri: &str,
    client_id: &str,
    client_secret: &str,
    code_verifier: &str,
) -> reqwest::Result<Tokens> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", code)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("client_id", client_id)
        .append_pair("client_secret", client_secret)
        .append_pair("code_verifier", code_verifier)
        .finish();
    http.post(token_endpoint)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

/// Fetches the userinfo document for a plain OAuth2 provider (no id_token), using
/// the access token as a bearer credential. Returns the raw JSON.
pub async fn fetch_userinfo(
    http: &reqwest::Client,
    userinfo_endpoint: &str,
    access_token: &str,
) -> reqwest::Result<serde_json::Value> {
    http.get(userinfo_endpoint)
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}
