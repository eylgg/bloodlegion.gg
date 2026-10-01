mod jwks_json;
pub mod openid_configuration;
mod webfinger;

use axum::{Router, routing::get};

use crate::State;

pub fn router() -> Router<State> {
    Router::new()
        .route("/jwks.json", get(jwks_json::handler))
        .route("/openid-configuration", get(openid_configuration::handler))
        .route("/webfinger", get(webfinger::handler))
}
