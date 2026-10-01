pub mod api;
mod authorize;
// `pub(crate)` rather than private: `forgejo` resolves its configured OAuth2 client to the identity
// provider that names Forgejo accounts, so it needs `find_client` / `find_identity_provider_id`.
pub(crate) mod clients;
pub mod providers;
mod scope;
mod token;
mod userinfo;

use axum::{
    Router,
    routing::{get, post},
};

use crate::State;

/// The validated `scope` value type, reachable as `crate::auth::oauth2::Scope`.
pub use scope::Scope;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScopeDefinition {
    pub name: &'static str,
    pub description: &'static str,
}

pub const OPENID: &str = "openid";
pub const PROFILE: &str = "profile";
pub const EMAIL: &str = "email";

pub const SUPPORTED_SCOPES: &[ScopeDefinition] = &[
    ScopeDefinition {
        name: OPENID,
        description: "Verify your unique identity identifier securely.",
    },
    ScopeDefinition {
        name: PROFILE,
        description: "Access your basic identity details (first and last name).",
    },
    ScopeDefinition {
        name: EMAIL,
        description: "Access your primary email address and email verification status.",
    },
];

pub fn lookup_scope(name: &str) -> Option<&'static ScopeDefinition> {
    SUPPORTED_SCOPES.iter().find(|scope| scope.name == name)
}

/// Top-level OAuth2 protocol endpoints, mounted at `/auth/oauth2` and
/// reverse-proxied straight to the backend (not bounced through `/api`):
/// `token`/`userinfo` are the IdP endpoints external relying parties call (the
/// URLs advertised in openid-configuration), and `callback` is the single
/// redirect target an upstream provider returns to (the provider is resolved
/// from the `state` parameter, so the callback path carries no slug). The
/// SPA-driven `/api/auth/oauth2` endpoints live in [`api`].
pub fn router() -> Router<State> {
    Router::new()
        .route("/token", post(token::handler))
        .route("/userinfo", get(userinfo::handler))
        .route("/callback", get(providers::api::callback))
}
