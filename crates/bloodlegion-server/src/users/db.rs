use sqlx::{PgConnection, PgPool};

use crate::guild::Rank;
use crate::users::{CreateUserPayload, User, UserId};

pub async fn find_user_by_id(pool: &PgPool, user_id: UserId) -> sqlx::Result<Option<User>> {
    sqlx::query_as!(
        User,
        r#"
        SELECT
            users.id AS "id: UserId",
            users.username,
            users.is_superuser,
            (users.disabled_at IS NOT NULL) AS "disabled!",
            user_emails.email AS "email?",
            COALESCE(user_emails.verified_at IS NOT NULL, false) AS "is_email_verified!",
            COALESCE(user_emails.is_federated, false) AS "is_email_federated!",
            users.first_name,
            users.last_name,
            users.guild_rank AS "guild_rank: Rank",
            users.created_at,
            users.updated_at
        FROM users
        LEFT JOIN user_emails ON user_emails.user_id = users.id AND user_emails.is_primary
        WHERE users.id = $1
        "#,
        user_id.0,
    )
    .fetch_optional(pool)
    .await
}

/// Sets a user's username. Returns whether a row matched. The unique constraint on
/// `username_normalized` rejects a name another account holds in any capitalization.
pub async fn set_username(pool: &PgPool, user_id: UserId, username: &str) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE users SET username = $2 WHERE id = $1",
        user_id.0,
        username,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Grants or revokes the superuser flag. Returns whether a row matched.
pub async fn set_user_superuser(
    pool: &PgPool,
    user_id: UserId,
    is_superuser: bool,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "UPDATE users SET is_superuser = $2 WHERE id = $1",
        user_id.0,
        is_superuser,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Sets or clears a user's disabled state. Returns whether a row matched.
pub async fn set_user_disabled(
    pool: &PgPool,
    user_id: UserId,
    disabled: bool,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        r#"
        UPDATE users
        SET disabled_at = CASE WHEN $2 THEN COALESCE(disabled_at, now()) ELSE NULL END
        WHERE id = $1
        "#,
        user_id.0,
        disabled,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Used only by other modules' tests; the admin list handler uses [`list_user_listings`].
#[cfg(test)]
pub async fn list_users(pool: &PgPool) -> sqlx::Result<Vec<User>> {
    sqlx::query_as!(
        User,
        r#"
        SELECT
            users.id AS "id: UserId",
            users.username,
            users.is_superuser,
            (users.disabled_at IS NOT NULL) AS "disabled!",
            user_emails.email AS "email?",
            COALESCE(user_emails.verified_at IS NOT NULL, false) AS "is_email_verified!",
            COALESCE(user_emails.is_federated, false) AS "is_email_federated!",
            users.first_name,
            users.last_name,
            users.guild_rank AS "guild_rank: Rank",
            users.created_at,
            users.updated_at
        FROM users
        LEFT JOIN user_emails ON user_emails.user_id = users.id AND user_emails.is_primary
        ORDER BY users.created_at
        LIMIT 1000
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Marks a user's email as federated (asserted by an IdP), keyed by the address.
/// IdP provisioning calls this so a future email-management UI can refuse to
/// remove it.
pub async fn mark_email_federated(
    conn: &mut PgConnection,
    user_id: UserId,
    email: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE user_emails SET is_federated = true WHERE user_id = $1 AND email_normalized = lower($2)",
        user_id.0,
        email,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn insert_user(
    conn: &mut PgConnection,
    payload: &CreateUserPayload,
    is_email_verified: bool,
) -> sqlx::Result<User> {
    // email is stored verbatim (for display); the user_emails.email_normalized
    // generated column carries the lowercased form for uniqueness and lookups.
    let row = sqlx::query!(
        r#"
        INSERT INTO users (username, first_name, last_name, is_superuser)
        VALUES ($1, $2, $3, $4)
        RETURNING id AS "id: UserId", guild_rank AS "guild_rank: Rank", created_at, updated_at
        "#,
        payload.username,
        payload.first_name,
        payload.last_name,
        payload.is_superuser,
    )
    .fetch_one(&mut *conn)
    .await?;

    if let Some(email) = &payload.email {
        sqlx::query!(
            r#"
            INSERT INTO user_emails (user_id, email, verified_at, is_primary)
            VALUES ($1, $2, CASE WHEN $3 THEN now() END, true)
            "#,
            row.id.0,
            email,
            is_email_verified,
        )
        .execute(&mut *conn)
        .await?;
    }

    Ok(User {
        id: row.id,
        username: payload.username.clone(),
        is_superuser: payload.is_superuser,
        disabled: false,
        email: payload.email.clone(),
        is_email_verified: is_email_verified && payload.email.is_some(),
        is_email_federated: false,
        first_name: payload.first_name.clone(),
        last_name: payload.last_name.clone(),
        guild_rank: row.guild_rank,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// The account that owns `email` (case-insensitively), or `None` if no account has it. Email is
/// globally unique, so at most one account owns it.
pub async fn find_email_owner(pool: &PgPool, email: &str) -> sqlx::Result<Option<UserId>> {
    sqlx::query_scalar!(
        r#"SELECT user_id AS "user_id: UserId" FROM user_emails WHERE email_normalized = lower($1)"#,
        email,
    )
    .fetch_optional(pool)
    .await
}

/// One user's email addresses (primary first, then alphabetical), for the profile page, the same
/// shape the admin list aggregates per user.
pub async fn list_emails(pool: &PgPool, user_id: UserId) -> sqlx::Result<Vec<super::UserEmail>> {
    sqlx::query_as!(
        super::UserEmail,
        r#"
        SELECT
            email,
            (verified_at IS NOT NULL) AS "is_verified!",
            is_primary,
            is_federated
        FROM user_emails
        WHERE user_id = $1
        ORDER BY is_primary DESC, email
        "#,
        user_id.0,
    )
    .fetch_all(pool)
    .await
}

/// Every user with all of their emails (primary first) aggregated into an array, for the admin
/// list.
pub async fn list_user_listings(pool: &PgPool) -> sqlx::Result<Vec<super::UserListing>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            users.id AS "id: UserId",
            users.username,
            users.is_superuser,
            (users.disabled_at IS NOT NULL) AS "disabled!",
            users.first_name,
            users.last_name,
            users.guild_rank AS "guild_rank: Rank",
            users.created_at,
            users.updated_at,
            COALESCE(
                jsonb_agg(
                    jsonb_build_object(
                        'email', ue.email,
                        'is_verified', ue.verified_at IS NOT NULL,
                        'is_primary', ue.is_primary,
                        'is_federated', ue.is_federated
                    ) ORDER BY ue.is_primary DESC, ue.email
                ) FILTER (WHERE ue.id IS NOT NULL),
                '[]'::jsonb
            ) AS "emails!: sqlx::types::Json<Vec<super::UserEmail>>"
        FROM users
        LEFT JOIN user_emails ue ON ue.user_id = users.id
        GROUP BY users.id
        ORDER BY users.created_at
        LIMIT 1000
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| super::UserListing {
            id: row.id,
            username: row.username,
            is_superuser: row.is_superuser,
            disabled: row.disabled,
            first_name: row.first_name,
            last_name: row.last_name,
            created_at: row.created_at,
            updated_at: row.updated_at,
            emails: row.emails.0,
        })
        .collect())
}

/// Adds `email` to `user_id` as a verified, non-primary, non-federated address. `ON CONFLICT DO
/// NOTHING` is a backstop only, the caller checks [`find_email_owner`] first, so a conflict here is
/// a lost race, not the expected path.
pub async fn insert_additional_email(
    pool: &PgPool,
    user_id: UserId,
    email: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO user_emails (user_id, email, verified_at, is_primary, is_federated)
        VALUES ($1, $2, now(), false, false)
        ON CONFLICT DO NOTHING
        "#,
        user_id.0,
        email,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_user_id_by_username(
    pool: &PgPool,
    username: &str,
) -> sqlx::Result<Option<UserId>> {
    sqlx::query_scalar!(
        r#"SELECT id AS "id: UserId" FROM users WHERE username_normalized = lower($1)"#,
        username
    )
    .fetch_optional(pool)
    .await
}

/// Users whose username or name matches `query` (a substring, case-insensitive), capped at `limit`.
/// The LIKE metacharacters in `query` are escaped so they match literally.
pub async fn search_users(
    pool: &PgPool,
    query: &str,
    limit: i64,
) -> sqlx::Result<Vec<super::UserMatch>> {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");
    sqlx::query_as!(
        super::UserMatch,
        r#"
        SELECT username, first_name, last_name
        FROM users
        WHERE username ILIKE $1 OR first_name ILIKE $1 OR last_name ILIKE $1
        ORDER BY username
        LIMIT $2
        "#,
        pattern,
        limit,
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert(pool: &PgPool, username: &str, email: &str, verified: bool) -> UserId {
        let mut tx = pool.begin().await.unwrap();
        let user = insert_user(
            &mut tx,
            &CreateUserPayload {
                username: username.to_string(),
                email: Some(email.to_string()),
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            verified,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        user.id
    }

    #[sqlx::test]
    async fn insert_user_links_a_verified_primary_email(pool: PgPool) {
        // The user row and its primary (is_primary) email are inserted together;
        // find_user_by_id joins back through the primary email.
        let id = insert(&pool, "linked", "linked@example.com", true).await;
        let user = find_user_by_id(&pool, id)
            .await
            .unwrap()
            .expect("user should exist");
        assert_eq!(user.email.as_deref(), Some("linked@example.com"));
        assert!(user.is_email_verified);
    }

    #[sqlx::test]
    async fn deleting_a_user_cascades_its_emails(pool: PgPool) {
        let id = insert(&pool, "goner", "goner@example.com", true).await;
        let mut tx = pool.begin().await.unwrap();
        sqlx::query!("DELETE FROM users WHERE id = $1", id.0)
            .execute(&mut *tx)
            .await
            .unwrap();
        // The orphan guard skips a user that is gone, so the cascade commits.
        tx.commit().await.unwrap();
        assert!(find_user_by_id(&pool, id).await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn user_emails_updated_at_only_bumps_on_a_real_change(pool: PgPool) {
        let id = insert(&pool, "tick", "tick@example.com", false).await;
        let before = sqlx::query_scalar!(
            "SELECT updated_at FROM user_emails WHERE user_id = $1",
            id.0
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        // A no-op update is gated out: the trigger does not fire.
        sqlx::query!(
            "UPDATE user_emails SET email = email WHERE user_id = $1",
            id.0
        )
        .execute(&pool)
        .await
        .unwrap();
        let after_noop = sqlx::query_scalar!(
            "SELECT updated_at FROM user_emails WHERE user_id = $1",
            id.0
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            after_noop, before,
            "a no-op update must not bump updated_at"
        );

        // A real change bumps it.
        sqlx::query!(
            "UPDATE user_emails SET verified_at = now() WHERE user_id = $1",
            id.0
        )
        .execute(&pool)
        .await
        .unwrap();
        let after_real = sqlx::query_scalar!(
            "SELECT updated_at FROM user_emails WHERE user_id = $1",
            id.0
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(after_real > before, "a real change must bump updated_at");
    }

    #[sqlx::test]
    async fn insert_user_records_an_unverified_email(pool: PgPool) {
        let id = insert(&pool, "pending", "pending@example.com", false).await;
        let user = find_user_by_id(&pool, id).await.unwrap().unwrap();
        assert!(!user.is_email_verified);
    }

    #[sqlx::test]
    async fn find_user_by_id_is_none_for_a_missing_id(pool: PgPool) {
        assert!(
            find_user_by_id(&pool, UserId(999_999))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn find_user_id_by_username_round_trips(pool: PgPool) {
        let id = insert(&pool, "lookup", "lookup@example.com", true).await;
        assert_eq!(
            find_user_id_by_username(&pool, "lookup").await.unwrap(),
            Some(id)
        );
        assert!(
            find_user_id_by_username(&pool, "absent")
                .await
                .unwrap()
                .is_none()
        );
    }
}
