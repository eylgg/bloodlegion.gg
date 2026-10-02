use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};

use crate::extract::{Officer, SameOrigin};
use crate::raids::LootEntry;
use crate::users::User;
use crate::{Error, Result, State};

use super::{Character, CharacterError, CharacterInput, Note, NoteListing, SpecsInput};

/// `GET /api/characters`: every character, by name. Members only.
async fn list(state: State, _user: User) -> Result<Json<Vec<Character>>> {
    Ok(Json(super::list(&state.pool).await?))
}

/// `POST /api/characters`.
async fn create(
    state: State,
    _: SameOrigin,
    user: User,
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
    /// Whether the caller may edit the character (its player, or an officer).
    can_edit: bool,
    /// Whether the caller may write its notes (its player).
    can_write_note: bool,
}

/// `GET /api/characters/{id}`.
async fn detail(
    state: State,
    user: User,
    Path(id): Path<i64>,
) -> Result<Json<CharacterDetail>, CharacterError> {
    let Some(character) = super::find(&state.pool, id).await? else {
        return Err(Error::External(CharacterError::NotFound));
    };
    let note = if super::may_read_note(&user, &character) {
        super::note(&state.pool, id).await?
    } else {
        None
    };
    let is_player = character.user_id == Some(user.id);
    Ok(Json(CharacterDetail {
        raids_attended: crate::raids::raids_attended(&state.pool, id).await?,
        loot: crate::raids::loot_for_character(&state.pool, id).await?,
        note,
        can_edit: is_player || user.is_officer(),
        can_write_note: is_player,
        character,
    }))
}

/// `PUT /api/characters/{id}`.
async fn update(
    state: State,
    _: SameOrigin,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<CharacterInput>,
) -> Result<Json<Character>, CharacterError> {
    Ok(Json(super::update(&state.pool, &user, id, &input).await?))
}

/// `DELETE /api/characters/{id}`.
async fn delete(
    state: State,
    _: SameOrigin,
    user: User,
    Path(id): Path<i64>,
) -> Result<StatusCode, CharacterError> {
    super::delete(&state.pool, &user, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/characters/{id}/specs`: the character's two specs and their talents.
async fn set_specs(
    state: State,
    _: SameOrigin,
    user: User,
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
    user: User,
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

pub fn router() -> Router<State> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/notes", get(notes))
        .route("/{id}", get(detail).put(update).delete(delete))
        .route("/{id}/note", put(set_note))
        .route("/{id}/specs", put(set_specs))
}
