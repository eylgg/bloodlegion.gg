use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};

use crate::extract::SameOrigin;
use crate::users::User;
use crate::{Result, State};

use super::{Character, CharacterInput, LaunchError, RosterEntry, catalog};

/// `GET /api/launch/classes`: the WoW: Forever classes, with official colors, specs, and roles.
/// Public, so the frontend renders from the same catalog the server validates against.
async fn classes() -> Json<&'static [catalog::Class]> {
    Json(catalog::CLASSES)
}

/// `GET /api/launch/characters`: the signed-in member's sign-ups, main first.
async fn list(state: State, user: User) -> Result<Json<Vec<Character>>> {
    Ok(Json(super::list(&state.pool, user.id).await?))
}

/// `POST /api/launch/characters`: reserve a character for launch.
async fn create(
    state: State,
    _: SameOrigin,
    user: User,
    Json(input): Json<CharacterInput>,
) -> Result<(StatusCode, Json<Character>), LaunchError> {
    let character = super::create(&state.pool, user.id, &input).await?;
    Ok((StatusCode::CREATED, Json(character)))
}

/// `PUT /api/launch/characters/{id}`: replace one of the member's own sign-ups.
async fn update(
    state: State,
    _: SameOrigin,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<CharacterInput>,
) -> Result<Json<Character>, LaunchError> {
    Ok(Json(super::update(&state.pool, user.id, id, &input).await?))
}

/// `DELETE /api/launch/characters/{id}`.
async fn delete(
    state: State,
    _: SameOrigin,
    user: User,
    Path(id): Path<i64>,
) -> Result<StatusCode, LaunchError> {
    super::delete(&state.pool, user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/launch/roster`: every member's sign-ups, for the whole guild to see who is coming.
/// Members only (signed in), not public.
async fn roster(state: State, _user: User) -> Result<Json<Vec<RosterEntry>>> {
    Ok(Json(super::list_all(&state.pool).await?))
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/classes", get(classes))
        .route("/roster", get(roster))
        .route("/characters", get(list).post(create))
        .route(
            "/characters/{id}",
            axum::routing::put(update).delete(delete),
        )
}
