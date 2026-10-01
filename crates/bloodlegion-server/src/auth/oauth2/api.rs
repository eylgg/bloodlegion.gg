use axum::{Router, routing::get};

use super::providers::register;

use crate::State;

use super::{authorize, clients, providers};

/// Endpoints the SPA drives, mounted under `/api/auth/oauth2`: the consent
/// screen (`authorize`), the admin client CRUD, and the upstream provider
/// (relying-party) login/admin endpoints.
pub fn router() -> Router<State> {
    Router::new()
        .route("/authorize", get(authorize::prompt).post(authorize::decide))
        .route(
            "/registration",
            get(register::pending).post(register::choose),
        )
        .nest("/clients", clients::router())
        .nest("/providers", providers::api::router())
}
