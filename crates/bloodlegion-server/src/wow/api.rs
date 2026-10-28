use axum::routing::get;
use axum::{Json, Router};

use crate::State;

use super::catalog;

/// `GET /api/wow/classes`: the WoW: Forever classes, with official colors, specs, and roles.
/// Public, so the frontend renders from the same catalog the server validates against.
async fn classes() -> Json<&'static [catalog::Class]> {
    Json(catalog::CLASSES)
}

/// `GET /api/wow/races`: the WoW: Forever races, with their factions. Public, like the classes.
async fn races() -> Json<&'static [catalog::Race]> {
    Json(catalog::RACES)
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/classes", get(classes))
        .route("/races", get(races))
}
