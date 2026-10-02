use axum::Router;

use crate::State;

/// Distinguishes an absent field (`None`) from a present `null` (`Some(None)`) from a
/// present value (`Some(Some(_))`), `#[serde(default)]` alone collapses the first two.
/// For PATCH payloads whose nullable fields need three intents: keep, clear, set.
pub fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

pub fn router() -> Router<State> {
    Router::new()
        .nest("/users", crate::users::router())
        .nest("/launch", crate::launch::router())
        .nest("/guild", crate::guild::router())
        .nest("/characters", crate::characters::router())
        .nest("/questions", crate::questions::router())
        .merge(crate::raids::router())
        .nest("/auth", crate::auth::api::router())
}
