use axum::extract::{Path, Query};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

use crate::users::User;
use crate::{Result, State};

use super::{Browse, GameItem, Page};

/// `GET /api/game-items?q=&quality=&slot=&offset=&limit=`: a page of mirrored items, for the item
/// browser and the item picker. Every filter is optional.
async fn browse(state: State, _user: User, Query(browse): Query<Browse>) -> Result<Json<Page>> {
    Ok(Json(super::browse(&state.pool, &browse).await?))
}

/// `GET /api/game-items/slots`: the slots the mirror's items go in.
async fn slots(state: State, _user: User) -> Result<Json<Vec<String>>> {
    Ok(Json(super::slots(&state.pool).await?))
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
        .route("/", get(browse))
        .route("/slots", get(slots))
        .route("/{id}", get(find))
        .route("/icons/{name}", get(icon))
}
