use std::sync::Arc;

use axum::Json;

use crate::State;

#[derive(Debug, serde::Serialize)]
pub struct OpenidConfiguration {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub grant_types_supported: &'static [&'static str],
    pub response_types_supported: &'static [&'static str],
    pub subject_types_supported: &'static [&'static str],
    pub id_token_signing_alg_values_supported: &'static [&'static str],
    pub token_endpoint_auth_methods_supported: &'static [&'static str],
    pub code_challenge_methods_supported: &'static [&'static str],
    pub scopes_supported: &'static [&'static str],
}

pub async fn handler(state: State) -> Json<Arc<OpenidConfiguration>> {
    Json(state.oidc_config)
}
