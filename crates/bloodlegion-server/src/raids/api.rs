//! Mounted at `/api`: `/raids`, `/bosses`, `/items`, and `/loot`. Every read needs a session;
//! every write needs an officer.

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};

use crate::extract::{Officer, SameOrigin};
use crate::users::User;
use crate::{Result, State};

use super::{
    Attendee, Boss, BossDetail, BossInput, Item, ItemDetail, ItemInput, LootEntry, LootFilter,
    LootInput, LootUpdate, Raid, RaidDetail, RaidError, RaidInput, catalog,
};

/// `GET /api/raids/zones`: the raid zones and their sizes. Public, like the class catalog.
async fn zones() -> Json<&'static [catalog::Zone]> {
    Json(catalog::ZONES)
}

/* --- raids --- */

async fn list_raids(state: State, _user: User) -> Result<Json<Vec<Raid>>> {
    Ok(Json(super::raids(&state.pool).await?))
}

async fn create_raid(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Json(input): Json<RaidInput>,
) -> Result<(StatusCode, Json<Raid>), RaidError> {
    let raid = super::create_raid(&state.pool, &input).await?;
    Ok((StatusCode::CREATED, Json(raid)))
}

async fn raid_detail(
    state: State,
    _user: User,
    Path(id): Path<i64>,
) -> Result<Json<RaidDetail>, RaidError> {
    Ok(Json(super::raid_detail(&state.pool, id).await?))
}

async fn update_raid(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
    Json(input): Json<RaidInput>,
) -> Result<Json<Raid>, RaidError> {
    Ok(Json(super::update_raid(&state.pool, id, &input).await?))
}

async fn delete_raid(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
) -> Result<StatusCode, RaidError> {
    super::delete_raid(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Deserialize)]
struct AttendeesInput {
    character_ids: Vec<i64>,
}

/// `POST /api/raids/{id}/attendees`: adds characters (all or none); answers with everyone now on
/// the raid.
async fn add_attendees(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
    Json(input): Json<AttendeesInput>,
) -> Result<Json<Vec<Attendee>>, RaidError> {
    Ok(Json(
        super::add_attendees(&state.pool, id, &input.character_ids).await?,
    ))
}

async fn remove_attendee(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path((id, character_id)): Path<(i64, i64)>,
) -> Result<StatusCode, RaidError> {
    super::remove_attendee(&state.pool, id, character_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/raids/{id}/loot`.
async fn record_loot(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
    Json(input): Json<LootInput>,
) -> Result<(StatusCode, Json<LootEntry>), RaidError> {
    let entry = super::record_loot(&state.pool, id, &input).await?;
    Ok((StatusCode::CREATED, Json(entry)))
}

/* --- loot --- */

/// `GET /api/loot?zone=&boss_id=&item_id=&character_id=&class=&quality=&limit=`: the loot
/// history, latest first.
async fn list_loot(
    state: State,
    _user: User,
    Query(filter): Query<LootFilter>,
) -> Result<Json<Vec<LootEntry>>> {
    Ok(Json(super::loot(&state.pool, &filter).await?))
}

async fn update_loot(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
    Json(input): Json<LootUpdate>,
) -> Result<Json<LootEntry>, RaidError> {
    Ok(Json(super::update_loot(&state.pool, id, &input).await?))
}

async fn delete_loot(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
) -> Result<StatusCode, RaidError> {
    super::delete_loot(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/* --- bosses --- */

async fn list_bosses(state: State, _user: User) -> Result<Json<Vec<Boss>>> {
    Ok(Json(super::bosses(&state.pool).await?))
}

async fn create_boss(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Json(input): Json<BossInput>,
) -> Result<(StatusCode, Json<Boss>), RaidError> {
    let boss = super::create_boss(&state.pool, &input).await?;
    Ok((StatusCode::CREATED, Json(boss)))
}

async fn boss_detail(
    state: State,
    _user: User,
    Path(id): Path<i64>,
) -> Result<Json<BossDetail>, RaidError> {
    Ok(Json(super::boss_detail(&state.pool, id).await?))
}

#[derive(Debug, serde::Deserialize)]
struct RenameInput {
    name: String,
}

async fn rename_boss(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
    Json(input): Json<RenameInput>,
) -> Result<Json<Boss>, RaidError> {
    Ok(Json(
        super::rename_boss(&state.pool, id, &input.name).await?,
    ))
}

async fn delete_boss(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
) -> Result<StatusCode, RaidError> {
    super::delete_boss(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/* --- items --- */

async fn list_items(state: State, _user: User) -> Result<Json<Vec<Item>>> {
    Ok(Json(super::items(&state.pool).await?))
}

async fn create_item(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Json(input): Json<ItemInput>,
) -> Result<(StatusCode, Json<Item>), RaidError> {
    let item = super::create_item(&state.pool, &input).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn item_detail(
    state: State,
    _user: User,
    Path(id): Path<i64>,
) -> Result<Json<ItemDetail>, RaidError> {
    Ok(Json(super::item_detail(&state.pool, id).await?))
}

async fn update_item(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
    Json(input): Json<ItemInput>,
) -> Result<Json<Item>, RaidError> {
    Ok(Json(super::update_item(&state.pool, id, &input).await?))
}

async fn delete_item(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
) -> Result<StatusCode, RaidError> {
    super::delete_item(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/raids", get(list_raids).post(create_raid))
        .route("/raids/zones", get(zones))
        .route(
            "/raids/{id}",
            get(raid_detail).put(update_raid).delete(delete_raid),
        )
        .route("/raids/{id}/attendees", axum::routing::post(add_attendees))
        .route(
            "/raids/{id}/attendees/{character_id}",
            delete(remove_attendee),
        )
        .route("/raids/{id}/loot", axum::routing::post(record_loot))
        .route("/loot", get(list_loot))
        .route(
            "/loot/{id}",
            axum::routing::put(update_loot).delete(delete_loot),
        )
        .route("/bosses", get(list_bosses).post(create_boss))
        .route(
            "/bosses/{id}",
            get(boss_detail).put(rename_boss).delete(delete_boss),
        )
        .route("/items", get(list_items).post(create_item))
        .route(
            "/items/{id}",
            get(item_detail).put(update_item).delete(delete_item),
        )
}
