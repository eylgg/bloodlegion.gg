use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};

use crate::extract::{GuildMember, Officer, RosterClient, SameOrigin};
use crate::raids::LootEntry;
use crate::{Error, Result, State};

use super::roster::{RosterInput, RosterResult};
use super::{Character, CharacterError, CharacterInput, Note, NoteListing, SpecsInput};

/// Which characters `GET /api/characters` lists.
#[derive(Debug, Default, serde::Deserialize)]
struct ListQuery {
    /// Include the unclaimed ones (pugs): for an officer picking attendees and loot. The admin
    /// always gets them; anyone else never does.
    #[serde(default)]
    unclaimed: bool,
}

/// `GET /api/characters?unclaimed=`: every character the caller may see, by name. Members only.
async fn list(
    state: State,
    user: GuildMember,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Character>>> {
    Ok(Json(
        super::visible(&state.pool, &user, query.unclaimed).await?,
    ))
}

/// `POST /api/characters`.
async fn create(
    state: State,
    _: SameOrigin,
    user: GuildMember,
    Json(input): Json<CharacterInput>,
) -> Result<(StatusCode, Json<Character>), CharacterError> {
    let character = super::create(&state.pool, &user, &input).await?;
    Ok((StatusCode::CREATED, Json(character)))
}

/// One character's page: who plays it, the raids it came to, the loot it won, and its notes when
/// the caller may read them.
#[derive(Debug, serde::Serialize)]
struct CharacterDetail {
    character: Character,
    raids_attended: i64,
    loot: Vec<LootEntry>,
    /// `None` when the caller may not read it, or there is none.
    note: Option<Note>,
    /// Whether the caller may edit the character (an officer).
    can_edit: bool,
    /// Whether the caller may write its notes (its player).
    can_write_note: bool,
}

/// `GET /api/characters/{id}`.
async fn detail(
    state: State,
    user: GuildMember,
    Path(id): Path<i64>,
) -> Result<Json<CharacterDetail>, CharacterError> {
    let character = super::find(&state.pool, id).await?;
    Ok(Json(detail_of(&state, &user, character).await?))
}

/// `GET /api/characters/named/{first-last}`: the same, by the name the site links a character's
/// page by (`/roster/garr-bonecrush`).
async fn detail_by_name(
    state: State,
    user: GuildMember,
    Path(slug): Path<String>,
) -> Result<Json<CharacterDetail>, CharacterError> {
    let character = super::find_by_slug(&state.pool, &slug).await?;
    Ok(Json(detail_of(&state, &user, character).await?))
}

/// A character's page, as `user` may see it.
async fn detail_of(
    state: &State,
    user: &GuildMember,
    character: Option<Character>,
) -> Result<CharacterDetail, CharacterError> {
    let Some(character) = character else {
        return Err(Error::External(CharacterError::NotFound));
    };
    let id = character.id;
    let note = if super::may_read_note(user, &character) {
        super::note(&state.pool, id).await?
    } else {
        None
    };
    let is_player = character.user_id == Some(user.id);
    Ok(CharacterDetail {
        raids_attended: crate::raids::raids_attended(&state.pool, id).await?,
        loot: crate::raids::loot_for_character(&state.pool, id).await?,
        note,
        can_edit: user.is_officer(),
        can_write_note: is_player,
        character,
    })
}

/// `PUT /api/characters/{id}`.
async fn update(
    state: State,
    _: SameOrigin,
    user: GuildMember,
    Path(id): Path<i64>,
    Json(input): Json<CharacterInput>,
) -> Result<Json<Character>, CharacterError> {
    Ok(Json(super::update(&state.pool, &user, id, &input).await?))
}

/// `DELETE /api/characters/{id}`.
async fn delete(
    state: State,
    _: SameOrigin,
    user: GuildMember,
    Path(id): Path<i64>,
) -> Result<StatusCode, CharacterError> {
    super::delete(&state.pool, &user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/characters/{id}/specs`: the character's two specs and their talents.
async fn set_specs(
    state: State,
    _: SameOrigin,
    user: GuildMember,
    Path(id): Path<i64>,
    Json(input): Json<SpecsInput>,
) -> Result<Json<Character>, CharacterError> {
    Ok(Json(
        super::set_specs(&state.pool, &user, id, &input).await?,
    ))
}

#[derive(Debug, serde::Deserialize)]
struct NoteInput {
    body: String,
}

/// `PUT /api/characters/{id}/note`: the player writes the character's notes; an empty body
/// clears them (and the response is `null`).
async fn set_note(
    state: State,
    _: SameOrigin,
    user: GuildMember,
    Path(id): Path<i64>,
    Json(input): Json<NoteInput>,
) -> Result<Json<Option<Note>>, CharacterError> {
    Ok(Json(
        super::set_note(&state.pool, &user, id, &input.body).await?,
    ))
}

/// `GET /api/characters/notes`: the raiding roster's notes, latest first. Officers only.
async fn notes(state: State, _officer: Officer) -> Result<Json<Vec<NoteListing>>> {
    Ok(Json(super::raiding_notes(&state.pool).await?))
}

/// `POST /api/characters/roster`: the Blood Legion client sends the guild roster the addon saw in game,
/// which raises the levels of the characters it names. Officers only, by access token.
async fn roster(
    state: State,
    client: RosterClient,
    Json(input): Json<RosterInput>,
) -> Result<Json<RosterResult>, CharacterError> {
    Ok(Json(
        super::roster::apply(&state.pool, client.id, &input).await?,
    ))
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/notes", get(notes))
        .route("/roster", post(roster))
        .route("/named/{name}", get(detail_by_name))
        .route("/{id}", get(detail).put(update).delete(delete))
        .route("/{id}/note", put(set_note))
        .route("/{id}/specs", put(set_specs))
}
