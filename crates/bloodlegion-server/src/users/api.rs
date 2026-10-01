use anyhow::Context;
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::{Json, Router, routing::get};

use crate::extract::{Admin, SameOrigin};
use crate::{Error, Result, State};

use super::{
    CreateUserPayload, User, UserId, UserMatch, UsersError as CreateUserError, db, search_users,
};

pub async fn list_users(
    State { pool, .. }: State,
    _admin: Admin,
) -> Result<Json<Vec<super::UserListing>>, CreateUserError> {
    Ok(Json(
        db::list_user_listings(&pool)
            .await
            .context("listing users")?,
    ))
}

pub async fn current_user(user: User) -> Result<Json<User>, CreateUserError> {
    Ok(Json(user))
}

/// `GET /api/users/self/emails`: the signed-in user's own email addresses (primary first), so the
/// profile page can show secondary addresses (e.g. a Canvas email recorded when connecting).
pub async fn current_user_emails(
    State { pool, .. }: State,
    user: User,
) -> Result<Json<Vec<super::UserEmail>>, CreateUserError> {
    Ok(Json(
        db::list_emails(&pool, user.id)
            .await
            .context("listing the user's emails")?,
    ))
}

/// The `?q=` of the user search.
#[derive(Debug, serde::Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    q: String,
}

/// `GET /api/users/search?q=`: up to 10 users matching `q` by username or name (admin), for the
/// username autocomplete. An empty `q` returns an empty list.
pub async fn search(
    State { pool, .. }: State,
    _admin: Admin,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<UserMatch>>> {
    Ok(Json(search_users(&pool, &params.q).await?))
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateUserRequest {
    #[serde(flatten)]
    user: CreateUserPayload,
    /// Optional. When omitted or empty the account is created with no local credential, a plain,
    /// unclaimed account (which a federated first login can then claim).
    #[serde(default)]
    password: Option<String>,
}

/// `POST /api/users`: an admin creates a user, optionally with a local password. The admin vouches
/// for the email, so it counts as verified, matching the `create-superuser` CLI. Without a password
/// the account has no credential and no way to log in until one is set or a federated login claims
/// it.
pub async fn create_user(
    State { pool, .. }: State,
    _: SameOrigin,
    _admin: Admin,
    Json(payload): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<User>), CreateUserError> {
    let mut tx = pool
        .begin()
        .await
        .context("beginning a transaction to create a user")?;
    let user = super::create_user(&mut tx, &payload.user, true).await?;
    if let Some(password) = payload.password.as_deref().filter(|p| !p.is_empty()) {
        crate::auth::local::set_password(&mut tx, user.id, None, password)
            .await
            .map_err(|error| Error::Internal(error.into()))?;
    }
    tx.commit().await.context("committing the created user")?;
    Ok((StatusCode::CREATED, Json(user)))
}

#[derive(Debug, serde::Deserialize)]
pub struct SetDisabledRequest {
    disabled: bool,
}

/// `PATCH /api/users/{id}`: an admin disables or re-enables an account.
/// Disabling is a reversible suspension: existing sessions stop resolving and
/// local login is refused, but the account and its data are preserved. An admin
/// cannot disable themselves (a guard against locking the last admin out).
pub async fn set_disabled(
    State { pool, .. }: State,
    _: SameOrigin,
    admin: Admin,
    Path(user_id): Path<UserId>,
    Json(payload): Json<SetDisabledRequest>,
) -> Result<StatusCode, CreateUserError> {
    if user_id == admin.id {
        return Err(Error::External(CreateUserError::CannotDisableSelf));
    }
    let updated = db::set_user_disabled(&pool, user_id, payload.disabled)
        .await
        .context("updating the user's disabled state")?;
    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(Error::External(CreateUserError::NotFound))
    }
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/self", get(current_user))
        .route("/self/emails", get(current_user_emails))
        .route("/search", get(search))
        .route("/{id}", axum::routing::patch(set_disabled))
}
