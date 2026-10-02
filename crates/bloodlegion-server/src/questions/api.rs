use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};

use crate::extract::{Officer, SameOrigin};
use crate::users::User;
use crate::{Result, State};

use super::{Answer, Question, QuestionError, QuestionInput};

/// `GET /api/questions`: every question, latest first, with the tally and the caller's answer.
async fn list(state: State, user: User) -> Result<Json<Vec<Question>>> {
    Ok(Json(super::list(&state.pool, &user).await?))
}

async fn create(
    state: State,
    _: SameOrigin,
    officer: Officer,
    Json(input): Json<QuestionInput>,
) -> Result<(StatusCode, Json<Question>), QuestionError> {
    let question = super::create(&state.pool, &officer, &input).await?;
    Ok((StatusCode::CREATED, Json(question)))
}

#[derive(Debug, serde::Serialize)]
struct QuestionDetail {
    question: Question,
    /// Who said what: officers only, else `None`.
    answers: Option<Vec<Answer>>,
}

/// `GET /api/questions/{id}`.
async fn detail(
    state: State,
    user: User,
    Path(id): Path<i64>,
) -> Result<Json<QuestionDetail>, QuestionError> {
    let question = super::find(&state.pool, &user, id).await?;
    let answers = if user.is_officer() {
        Some(super::answers(&state.pool, id).await?)
    } else {
        None
    };
    Ok(Json(QuestionDetail { question, answers }))
}

async fn update(
    state: State,
    _: SameOrigin,
    officer: Officer,
    Path(id): Path<i64>,
    Json(input): Json<QuestionInput>,
) -> Result<Json<Question>, QuestionError> {
    Ok(Json(
        super::update(&state.pool, &officer, id, &input).await?,
    ))
}

async fn delete(
    state: State,
    _: SameOrigin,
    _officer: Officer,
    Path(id): Path<i64>,
) -> Result<StatusCode, QuestionError> {
    super::delete(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Deserialize)]
struct AnswerInput {
    choice: bool,
}

/// `PUT /api/questions/{id}/answer`: the caller's yes or no.
async fn answer(
    state: State,
    _: SameOrigin,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<AnswerInput>,
) -> Result<Json<Question>, QuestionError> {
    Ok(Json(
        super::answer(&state.pool, &user, id, input.choice).await?,
    ))
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(detail).put(update).delete(delete))
        .route("/{id}/answer", put(answer))
}
