pub mod api;
mod db;
mod login;

use anyhow::Context;
use sqlx::PgConnection;
use thiserror::Error;

use crate::users::UserId;
use crate::{Problem, Result};

const MIN_PASSWORD_LENGTH: usize = 8;

#[derive(Debug, Error, Problem)]
pub enum LocalError {
    #[error("password too short")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Password",
        detail = format!("The password must be at least {} characters.", MIN_PASSWORD_LENGTH)
    )]
    TooShort,
    #[error("incorrect password")]
    #[problem(
        status = FORBIDDEN,
        title = "Incorrect Password",
        detail = "The current password is incorrect."
    )]
    IncorrectPassword,
    #[error("local login cannot be disabled with no other provider enabled")]
    #[problem(
        status = CONFLICT,
        title = "No Other Login Providers",
        detail = "Local login cannot be disabled until another login provider is enabled."
    )]
    NoOtherLoginProviders,
    #[error("local login is disabled")]
    #[problem(
        status = CONFLICT,
        title = "Local Login Disabled",
        detail = "Local login is disabled, so a password cannot be set."
    )]
    LocalLoginDisabled,
}

/// Whether local username/password login is currently enabled (the built-in local
/// provider is not disabled). Drives login discovery and the login handler's gate.
pub async fn is_enabled(pool: &sqlx::PgPool) -> sqlx::Result<bool> {
    db::is_enabled(pool).await
}

/// Enables or disables local login, returning the new state. Disabling with no other
/// login provider enabled would lock everyone out, so the guard lives in the write
/// itself (atomic): a blocked disable maps to a 409.
pub async fn set_enabled(pool: &sqlx::PgPool, enabled: bool) -> Result<bool, LocalError> {
    db::set_enabled(pool, enabled)
        .await
        .context("toggling local login")?
        .ok_or(crate::Error::External(LocalError::NoOtherLoginProviders))
}

/// Whether the signed-in user has a local password set (drives the profile
/// page's "Set" vs "Change" password choice).
pub async fn has_password(pool: &sqlx::PgPool, user_id: UserId) -> sqlx::Result<bool> {
    db::has_password(pool, user_id).await
}

pub async fn set_password(
    conn: &mut PgConnection,
    user_id: UserId,
    current_password: Option<&str>,
    new_password: &str,
) -> Result<(), LocalError> {
    if let Some(current_hash) = db::find_password(&mut *conn, user_id)
        .await
        .context("looking up the existing password hash")?
    {
        let current =
            current_password.ok_or(crate::Error::External(LocalError::IncorrectPassword))?;
        if !current_hash.verify(current).await {
            return Err(crate::Error::External(LocalError::IncorrectPassword));
        }
    }
    if new_password.chars().count() < MIN_PASSWORD_LENGTH {
        return Err(crate::Error::External(LocalError::TooShort));
    }
    let password = crate::crypto::PasswordHash::new(new_password)
        .await
        .context("hashing the password")?;
    db::upsert_credentials(&mut *conn, user_id, &password)
        .await
        .context("storing the password")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::{self as users_db, CreateUserPayload};
    use sqlx::PgPool;

    async fn user(pool: &PgPool) -> UserId {
        let mut tx = pool.begin().await.unwrap();
        let user = users_db::insert_user(
            &mut tx,
            &CreateUserPayload {
                username: "pwuser".to_string(),
                email: Some("pw@example.com".to_string()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        user.id
    }

    #[sqlx::test]
    async fn sets_then_changes_password_with_the_correct_current(pool: PgPool) {
        let user_id = user(&pool).await;
        let mut conn = pool.acquire().await.unwrap();

        // The first set needs no current password.
        set_password(&mut conn, user_id, None, "initialpw1")
            .await
            .unwrap();
        let hash = db::find_password(&mut conn, user_id)
            .await
            .unwrap()
            .unwrap();
        assert!(hash.verify("initialpw1").await);

        // Changing it requires, and accepts, the correct current password.
        set_password(&mut conn, user_id, Some("initialpw1"), "changedpw2")
            .await
            .unwrap();
        let hash = db::find_password(&mut conn, user_id)
            .await
            .unwrap()
            .unwrap();
        assert!(hash.verify("changedpw2").await);
        assert!(!hash.verify("initialpw1").await);
    }

    #[sqlx::test]
    async fn rejects_a_short_password(pool: PgPool) {
        let user_id = user(&pool).await;
        let mut conn = pool.acquire().await.unwrap();
        assert!(matches!(
            set_password(&mut conn, user_id, None, "short").await,
            Err(crate::Error::External(LocalError::TooShort))
        ));
    }

    #[sqlx::test]
    async fn rejects_a_wrong_or_missing_current_password(pool: PgPool) {
        let user_id = user(&pool).await;
        let mut conn = pool.acquire().await.unwrap();
        set_password(&mut conn, user_id, None, "initialpw1")
            .await
            .unwrap();

        assert!(matches!(
            set_password(&mut conn, user_id, Some("wrongpw99"), "newpw12345").await,
            Err(crate::Error::External(LocalError::IncorrectPassword))
        ));
        // Omitting the current password when one is already set is rejected too.
        assert!(matches!(
            set_password(&mut conn, user_id, None, "newpw12345").await,
            Err(crate::Error::External(LocalError::IncorrectPassword))
        ));
        // The stored password is unchanged after the failed attempts.
        let hash = db::find_password(&mut conn, user_id)
            .await
            .unwrap()
            .unwrap();
        assert!(hash.verify("initialpw1").await);
    }
}
