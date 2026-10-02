use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};

use crate::extract::{Admin, SameOrigin};
use crate::users::{User, UserId};
use crate::{Result, State};

use super::{GuildError, Member, Rank, Settings};

/// `GET /api/guild/members`: every member with their rank and characters. Members only.
async fn members(state: State, _user: User) -> Result<Json<Vec<Member>>> {
    Ok(Json(super::members(&state.pool).await?))
}

#[derive(Debug, serde::Deserialize)]
struct RankInput {
    rank: Rank,
}

/// `PUT /api/guild/members/{id}/rank`: an officer changes a member's rank (see `may_set_rank`).
async fn set_rank(
    state: State,
    _: SameOrigin,
    user: User,
    Path(id): Path<UserId>,
    Json(input): Json<RankInput>,
) -> Result<StatusCode, GuildError> {
    super::set_rank(&state.pool, &user, id, input.rank).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/guild/settings`: the raid time zone and default start. Members only.
async fn settings(state: State, _user: User) -> Result<Json<Settings>> {
    Ok(Json(super::settings(&state.pool).await?))
}

/// `PUT /api/guild/settings`: superusers only.
async fn set_settings(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Json(input): Json<Settings>,
) -> Result<Json<Settings>, GuildError> {
    Ok(Json(super::set_settings(&state.pool, &input).await?))
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/settings", get(settings).put(set_settings))
        .route("/members", get(members))
        .route("/members/{id}/rank", put(set_rank))
}
