use axum::extract::{Path, Query};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

use crate::users::User;
use crate::{Result, State};

use super::{GameItem, GameItemSummary};

#[derive(Debug, serde::Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    limit: Option<i64>,
}

/// `GET /api/game-items?q=&limit=`: mirrored items whose name contains `q`, for the item picker.
async fn search(
    state: State,
    _user: User,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<GameItemSummary>>> {
    Ok(Json(
        super::search(&state.pool, &query.q, query.limit.unwrap_or(20)).await?,
    ))
}

/// `GET /api/game-items/{id}`: one mirrored item with its tooltip; 404 when the mirror lacks it.
async fn find(state: State, _user: User, Path(id): Path<i32>) -> Result<Response> {
    Ok(match super::find(&state.pool, id).await? {
        Some(item) => Json::<GameItem>(item).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

/// `GET /api/game-items/icons/{name}`: an icon image, from the mirror. Public, like the class
/// icons, and cached for a week: an icon's name always means the same picture.
async fn icon(state: State, Path(name): Path<String>) -> Result<Response> {
    let name = name.strip_suffix(".jpg").unwrap_or(&name);
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    Ok(match super::icon(&state.pool, name).await? {
        Some((content_type, image)) => (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "public, max-age=604800".to_string()),
            ],
            image,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/", get(search))
        .route("/{id}", get(find))
        .route("/icons/{name}", get(icon))
}
