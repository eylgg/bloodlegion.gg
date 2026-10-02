//! The old site's questions: an officer asks the guild a yes-or-no question ("Can you make
//! Thursdays?"), and each member answers once, free to change their mind. Everyone sees the tally;
//! officers see who said what.

pub mod api;
mod db;

use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use crate::users::{User, UserId};
use crate::{Error, Problem, Result};

pub use api::router;

#[derive(Debug, serde::Serialize)]
pub struct Question {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub yes: i64,
    pub no: i64,
    /// The caller's own answer, if they gave one.
    pub answer: Option<bool>,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, serde::Deserialize)]
pub struct QuestionInput {
    pub title: String,
    #[serde(default)]
    pub body: String,
}

/// One member's answer, for officers.
#[derive(Debug, serde::Serialize)]
pub struct Answer {
    pub user_id: UserId,
    pub username: String,
    pub choice: bool,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, thiserror::Error, Problem)]
pub enum QuestionError {
    #[error("invalid question")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Question",
        detail = "A question needs a title of 1 to 120 characters and at most 4,000 more."
    )]
    Invalid,
    #[error("question not found")]
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such question.")]
    NotFound,
}

fn validate(input: &QuestionInput) -> Result<(&str, &str), QuestionError> {
    let title = input.title.trim();
    let body = input.body.trim();
    if (1..=120).contains(&title.chars().count()) && body.chars().count() <= 4000 {
        Ok((title, body))
    } else {
        Err(Error::External(QuestionError::Invalid))
    }
}

/// Every question, latest first, with the caller's answers.
pub async fn list(pool: &PgPool, user: &User) -> sqlx::Result<Vec<Question>> {
    db::list(pool, user.id).await
}

pub async fn find(pool: &PgPool, user: &User, id: i64) -> Result<Question, QuestionError> {
    db::find(pool, user.id, id)
        .await?
        .ok_or(Error::External(QuestionError::NotFound))
}

pub async fn create(
    pool: &PgPool,
    user: &User,
    input: &QuestionInput,
) -> Result<Question, QuestionError> {
    let (title, body) = validate(input)?;
    let id = db::insert(pool, title, body).await?;
    find(pool, user, id).await
}

pub async fn update(
    pool: &PgPool,
    user: &User,
    id: i64,
    input: &QuestionInput,
) -> Result<Question, QuestionError> {
    let (title, body) = validate(input)?;
    if !db::update(pool, id, title, body).await? {
        return Err(Error::External(QuestionError::NotFound));
    }
    find(pool, user, id).await
}

pub async fn delete(pool: &PgPool, id: i64) -> Result<(), QuestionError> {
    if db::delete(pool, id).await? {
        Ok(())
    } else {
        Err(Error::External(QuestionError::NotFound))
    }
}

/// Records (or changes) the caller's answer.
pub async fn answer(
    pool: &PgPool,
    user: &User,
    id: i64,
    choice: bool,
) -> Result<Question, QuestionError> {
    db::upsert_answer(pool, id, user.id, choice)
        .await
        .map_err(|error| {
            crate::error::classify_db_error(error, |constraint| {
                (constraint == "question_answers_question_id_fkey")
                    .then_some(QuestionError::NotFound)
            })
        })?;
    find(pool, user, id).await
}

/// Who answered, yes first, then by username.
pub async fn answers(pool: &PgPool, id: i64) -> sqlx::Result<Vec<Answer>> {
    db::answers(pool, id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guild::{Rank, tests::member};

    #[sqlx::test]
    async fn members_answer_and_change_their_minds(pool: PgPool) {
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let raider = member(&pool, "Raider", Rank::Raider).await;
        let question = create(
            &pool,
            &officer,
            &QuestionInput {
                title: " Thursdays? ".into(),
                body: "Can you raid on Thursdays at 8?".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(question.title, "Thursdays?");

        answer(&pool, &raider, question.id, true).await.unwrap();
        answer(&pool, &officer, question.id, true).await.unwrap();
        let changed = answer(&pool, &raider, question.id, false).await.unwrap();
        assert_eq!(
            (changed.yes, changed.no, changed.answer),
            (1, 1, Some(false))
        );
        let officers_view = find(&pool, &officer, question.id).await.unwrap();
        assert_eq!(officers_view.answer, Some(true));

        let who: Vec<_> = answers(&pool, question.id)
            .await
            .unwrap()
            .into_iter()
            .map(|a| (a.username, a.choice))
            .collect();
        assert_eq!(
            who,
            vec![("Officer".to_string(), true), ("Raider".to_string(), false)]
        );
        assert!(matches!(
            answer(&pool, &raider, 999_999, true).await,
            Err(Error::External(QuestionError::NotFound))
        ));
        delete(&pool, question.id).await.unwrap();
        assert!(list(&pool, &raider).await.unwrap().is_empty());
    }
}
