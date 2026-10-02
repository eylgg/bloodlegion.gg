use sqlx::PgPool;

use super::{Answer, Question};
use crate::users::UserId;

pub async fn list(pool: &PgPool, user_id: UserId) -> sqlx::Result<Vec<Question>> {
    sqlx::query_as!(
        Question,
        r#"
        SELECT q.id, q.title, q.body,
               (SELECT count(*) FROM question_answers a
                WHERE a.question_id = q.id AND a.choice) AS "yes!",
               (SELECT count(*) FROM question_answers a
                WHERE a.question_id = q.id AND NOT a.choice) AS "no!",
               (SELECT a.choice FROM question_answers a
                WHERE a.question_id = q.id AND a.user_id = $1) AS answer,
               q.created_at
        FROM questions q
        ORDER BY q.created_at DESC, q.id DESC
        "#,
        user_id.0,
    )
    .fetch_all(pool)
    .await
}

pub async fn find(pool: &PgPool, user_id: UserId, id: i64) -> sqlx::Result<Option<Question>> {
    sqlx::query_as!(
        Question,
        r#"
        SELECT q.id, q.title, q.body,
               (SELECT count(*) FROM question_answers a
                WHERE a.question_id = q.id AND a.choice) AS "yes!",
               (SELECT count(*) FROM question_answers a
                WHERE a.question_id = q.id AND NOT a.choice) AS "no!",
               (SELECT a.choice FROM question_answers a
                WHERE a.question_id = q.id AND a.user_id = $1) AS answer,
               q.created_at
        FROM questions q
        WHERE q.id = $2
        "#,
        user_id.0,
        id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn insert(pool: &PgPool, title: &str, body: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        "INSERT INTO questions (title, body) VALUES ($1, $2) RETURNING id",
        title,
        body,
    )
    .fetch_one(pool)
    .await
}

pub async fn update(pool: &PgPool, id: i64, title: &str, body: &str) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE questions SET title = $2, body = $3 WHERE id = $1",
        id,
        title,
        body,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete(pool: &PgPool, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query!("DELETE FROM questions WHERE id = $1", id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn upsert_answer(
    pool: &PgPool,
    question_id: i64,
    user_id: UserId,
    choice: bool,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO question_answers (question_id, user_id, choice)
        VALUES ($1, $2, $3)
        ON CONFLICT (question_id, user_id) DO UPDATE SET choice = EXCLUDED.choice
        "#,
        question_id,
        user_id.0,
        choice,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn answers(pool: &PgPool, question_id: i64) -> sqlx::Result<Vec<Answer>> {
    sqlx::query_as!(
        Answer,
        r#"
        SELECT a.user_id AS "user_id: UserId", u.username, a.choice, a.updated_at
        FROM question_answers a
        JOIN users u ON u.id = a.user_id
        WHERE a.question_id = $1
        ORDER BY a.choice DESC, u.username_normalized
        "#,
        question_id,
    )
    .fetch_all(pool)
    .await
}
