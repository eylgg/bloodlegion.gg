pub mod api;
mod db;
mod email;
mod name;
mod user_id;
mod username;

use sqlx::{PgConnection, PgPool};
use time::{OffsetDateTime, serde::iso8601};

use crate::guild::Rank;
use crate::{Error, Problem, Result};

pub use api::router;
/// Re-exported so the types are reachable as `crate::users::{Email, Name, UserId, Username}`.
pub use email::Email;
pub use name::Name;
pub use user_id::UserId;
pub use username::Username;

/// Usernames no account may claim: they collide with API routes (`self`),
/// invite phishing (`admin`, `support`), or read as system senders
/// (`noreply`). [`create_user`] rejects them, which covers every path that
/// makes up a username (the admin form, SAML and OAuth2 registration), the
/// Canvas roster sync bypasses the check on purpose, since its usernames are
/// institutional SIS IDs.
const RESERVED_USERNAMES: &[&str] = &[
    "admin",
    "administrator",
    "root",
    "sys",
    "system",
    "moderator",
    "mod",
    "owner",
    "support",
    "help",
    "contact",
    "info",
    "helpdesk",
    "noreply",
    "no-reply",
    "self",
    "current",
    "account",
    "user",
    "users",
    "guest",
    "anonymous",
    "team",
    "staff",
    "developer",
    "dev",
    "test",
    "testing",
    "demo",
    "api",
    "docs",
    "status",
    "bot",
    "webmaster",
];

#[derive(Debug, serde::Serialize)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub is_superuser: bool,
    pub disabled: bool,
    /// The primary email, or `None`: an account may have no email at all, since a Battle.net
    /// login asserts none and local login never needs one.
    pub email: Option<String>,
    pub is_email_verified: bool,
    pub is_email_federated: bool,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    /// Their rank in the guild; with `is_superuser`, what decides who runs raids.
    pub guild_rank: Rank,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
}

impl User {
    /// Whether they may run the guild's records: schedule raids, record loot and attendance, and
    /// manage bosses, items, questions, and ranks.
    pub fn is_officer(&self) -> bool {
        self.is_superuser || self.guild_rank.is_officer()
    }
}

/// One of a user's email addresses, for the admin listing. `Deserialize` so it can be read back
/// from the `jsonb_agg` in [`db::list_user_listings`].
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UserEmail {
    pub email: String,
    pub is_verified: bool,
    pub is_primary: bool,
    pub is_federated: bool,
}

/// A user as the admin list shows them: the account fields plus every email on it (primary first),
/// so secondary addresses (e.g. a Canvas email recorded by the sync) are visible.
#[derive(Debug, serde::Serialize)]
pub struct UserListing {
    pub id: UserId,
    pub username: String,
    pub is_superuser: bool,
    pub disabled: bool,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
    pub emails: Vec<UserEmail>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateUserPayload {
    pub username: String,
    /// Optional: an account may have no email.
    #[serde(default)]
    pub email: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    #[serde(default)]
    pub is_superuser: bool,
}

#[derive(Debug, thiserror::Error, Problem)]
#[error("{}", self.detail())]
pub enum UsersError {
    #[problem(
        status = CONFLICT,
        title = "Email Taken",
        detail = "That email address is already in use."
    )]
    EmailTaken,
    #[problem(
        status = CONFLICT,
        title = "Username Taken",
        detail = "That username is already in use."
    )]
    UsernameTaken,
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Username Reserved",
        detail = "That username is reserved."
    )]
    UsernameReserved,
    /// The username failed the [`Username`] shape check. Transparent, so the
    /// `Username` error renders its own "Invalid Username" 422.
    #[problem(transparent)]
    InvalidUsername(username::Error),
    /// The email failed the [`Email`] shape check. Transparent, so the `Email`
    /// error renders its own "Invalid Email" 422.
    #[problem(transparent)]
    InvalidEmail(email::Error),
    /// A first/last name failed the [`Name`] length check. Transparent, so the
    /// `Name` error renders its own "Invalid Name" 422.
    #[problem(transparent)]
    InvalidName(name::Error),
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such user.")]
    NotFound,
    #[problem(
        status = CONFLICT,
        title = "Cannot Disable Self",
        detail = "You cannot disable your own account."
    )]
    CannotDisableSelf,
}

pub async fn create_user(
    conn: &mut PgConnection,
    payload: &CreateUserPayload,
    is_email_verified: bool,
) -> Result<User, UsersError> {
    // Validate the shape Rust-side via the `Username` type (the single mirror of
    // the `users_username_check` constraint), so every self-service path, the
    // admin form, SAML, and OIDC registration, rejects a malformed username with
    // a clean `InvalidUsername` instead of relying on the database round-trip.
    if let Err(error) = Username::try_from(payload.username.as_str()) {
        return Err(Error::External(UsersError::InvalidUsername(error)));
    }
    if let Some(email) = payload.email.as_deref()
        && let Err(error) = Email::try_from(email)
    {
        return Err(Error::External(UsersError::InvalidEmail(error)));
    }
    // Names are optional, but a present one must fit (1-64 chars). Reject an over-long name loudly
    // here rather than letting it become a 500 at the DB CHECK. This is the gate every self-service
    // path (admin, SAML, OIDC) shares.
    for part in [&payload.first_name, &payload.last_name] {
        if let Some(part) = part
            && let Err(error) = Name::try_from(part.as_str())
        {
            return Err(Error::External(UsersError::InvalidName(error)));
        }
    }
    if RESERVED_USERNAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(&payload.username))
    {
        return Err(Error::External(UsersError::UsernameReserved));
    }
    db::insert_user(conn, payload, is_email_verified)
        .await
        .map_err(classify)
}

/// Marks `email` as federated for `user_id`: an IdP-asserted email a future
/// email-management UI must not let the user remove (they can still demote it
/// from primary). Called from SAML/OIDC provisioning after it establishes the
/// user's IdP email.
pub async fn mark_email_federated(
    conn: &mut PgConnection,
    user_id: UserId,
    email: &str,
) -> sqlx::Result<()> {
    db::mark_email_federated(conn, user_id, email).await
}

/// Maps an insert failure to a domain error: the unique/check constraint
/// violations become client-facing conflicts; anything else is internal.
fn classify(error: sqlx::Error) -> Error<UsersError> {
    crate::error::classify_db_error(error, |constraint| match constraint {
        "user_emails_email_normalized_idx" => Some(UsersError::EmailTaken),
        "users_username_normalized_key" => Some(UsersError::UsernameTaken),
        // No users_username_check / user_emails_email_check arms: create_user
        // and the Canvas sync validate via `Username`/`Email` first, so a
        // DB-level violation of either means the Rust and database rules have
        // diverged, an internal bug, not a client 422, and falls through to
        // `Error::Internal`.
        _ => None,
    })
}

/// Inserts a user without the reserved-username check. Test fixtures only; every real path goes
/// through [`create_user`].
#[cfg(test)]
pub async fn insert_user(
    conn: &mut PgConnection,
    payload: &CreateUserPayload,
    is_email_verified: bool,
) -> sqlx::Result<User> {
    db::insert_user(conn, payload, is_email_verified).await
}

pub async fn find_user_by_id(pool: &PgPool, user_id: UserId) -> sqlx::Result<Option<User>> {
    db::find_user_by_id(pool, user_id).await
}

/// Every account with its emails, as the admin list and the CLI show them.
pub async fn list_user_listings(pool: &PgPool) -> sqlx::Result<Vec<UserListing>> {
    db::list_user_listings(pool).await
}

/// Renames an account. The new name goes through the same gates as a new account's (shape,
/// reserved names, uniqueness regardless of case), so a rename can never produce a name signup
/// would refuse. Changing only the capitalization of the current name is allowed.
pub async fn rename_user(
    pool: &PgPool,
    user_id: UserId,
    new_username: &str,
) -> Result<(), UsersError> {
    if let Err(error) = Username::try_from(new_username) {
        return Err(Error::External(UsersError::InvalidUsername(error)));
    }
    if RESERVED_USERNAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(new_username))
    {
        return Err(Error::External(UsersError::UsernameReserved));
    }
    match db::set_username(pool, user_id, new_username).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(Error::External(UsersError::NotFound)),
        Err(error) => Err(classify(error)),
    }
}

/// Grants or revokes the superuser flag (the admin role). Returns whether a row matched.
pub async fn set_user_superuser(
    pool: &PgPool,
    user_id: UserId,
    is_superuser: bool,
) -> sqlx::Result<bool> {
    db::set_user_superuser(pool, user_id, is_superuser).await
}

/// Suspends or restores an account. Returns whether a row matched.
pub async fn set_user_disabled(
    pool: &PgPool,
    user_id: UserId,
    disabled: bool,
) -> sqlx::Result<bool> {
    db::set_user_disabled(pool, user_id, disabled).await
}

/// Resolves a username in any capitalization to its account, via the normalized form.
pub async fn find_user_id_by_username(
    pool: &PgPool,
    username: &str,
) -> sqlx::Result<Option<UserId>> {
    db::find_user_id_by_username(pool, username).await
}

/// Records `email` as a verified, non-primary address for `user_id`, unless it already belongs to a
/// *different* account, in which case it is left alone and that account is returned, so the caller
/// can surface the conflict (it is authoritative that the email is `user_id`'s, so the other
/// account is likely a duplicate). Returns `Ok(None)` when the email was added, is already this
/// user's, or is malformed (skipped). The Canvas sync uses this to record the token holder's Canvas
/// email.
pub async fn add_verified_email(
    pool: &PgPool,
    user_id: UserId,
    email: &str,
) -> anyhow::Result<Option<UserId>> {
    use anyhow::Context;
    let email = email.trim();
    if Email::try_from(email).is_err() {
        return Ok(None);
    }
    match db::find_email_owner(pool, email)
        .await
        .context("checking who owns the email")?
    {
        // Already registered to a different account: do not move it, report it.
        Some(owner) if owner != user_id => return Ok(Some(owner)),
        // Already this user's: nothing to do.
        Some(_) => return Ok(None),
        None => {}
    }
    db::insert_additional_email(pool, user_id, email)
        .await
        .context("recording a verified email")?;
    Ok(None)
}

/// A user matching a search, for the admin username autocomplete (the same shape the roster's
/// candidate search returns).
#[derive(Debug, serde::Serialize)]
pub struct UserMatch {
    pub username: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

/// Up to 10 users matching `query` by username or name (admin autocomplete). An empty query returns
/// nothing rather than the whole directory.
pub async fn search_users(pool: &PgPool, query: &str) -> anyhow::Result<Vec<UserMatch>> {
    use anyhow::Context;
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    db::search_users(pool, query, 10)
        .await
        .context("searching users")
}

/// Used by other modules' tests; the production list handler is in `api`.
#[cfg(test)]
pub async fn list_users(pool: &PgPool) -> sqlx::Result<Vec<User>> {
    db::list_users(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    fn payload(username: &str, email: &str) -> CreateUserPayload {
        CreateUserPayload {
            username: username.to_string(),
            email: Some(email.to_string()),
            first_name: None,
            last_name: None,
            is_superuser: false,
        }
    }

    #[sqlx::test]
    async fn search_matches_username_and_ignores_empty(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        create_user(&mut tx, &payload("alice", "alice@example.com"), false)
            .await
            .unwrap();
        create_user(&mut tx, &payload("bob", "bob@example.com"), false)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        // An empty query returns nothing, never the whole directory.
        assert!(search_users(&pool, "  ").await.unwrap().is_empty());

        let hits = search_users(&pool, "ali").await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].username, "alice");
    }

    #[sqlx::test]
    async fn test_create_user_success(pool: PgPool) {
        let mut payload = payload("testuser123", "test@example.com");
        payload.first_name = Some("Ada".to_string());
        payload.last_name = Some("Lovelace".to_string());

        let mut tx = pool.begin().await.unwrap();
        let new_user = create_user(&mut tx, &payload, false)
            .await
            .expect("Failed to insert user into the database");
        tx.commit().await.unwrap();

        assert_eq!(new_user.username, payload.username);
        assert_eq!(new_user.email, payload.email);
        assert_eq!(new_user.email.as_deref(), Some("test@example.com"));
        assert_eq!(new_user.first_name, payload.first_name);
        assert_eq!(new_user.last_name, payload.last_name);
        assert!(!new_user.is_email_verified);
        assert!(new_user.id.0 > 0, "ID should be populated by Postgres");

        let retrieved = db::find_user_by_id(&pool, new_user.id)
            .await
            .expect("Failed to retrieve user from the database")
            .expect("User should exist after insertion");
        assert_eq!(retrieved.username, new_user.username);
        assert_eq!(retrieved.email, new_user.email);
        assert_eq!(retrieved.created_at, new_user.created_at);
        assert_eq!(retrieved.updated_at, new_user.updated_at);
    }

    #[sqlx::test]
    async fn test_create_user_duplicate_username_fails(pool: PgPool) {
        let payload = payload("duplicatehero", "duplicate@example.com");

        let mut tx = pool.begin().await.unwrap();
        create_user(&mut tx, &payload, false)
            .await
            .expect("First insert should have succeeded");
        tx.commit().await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        let duplicate = create_user(&mut tx, &payload, false).await;
        assert!(duplicate.is_err(), "duplicate username should fail");
    }

    #[sqlx::test]
    async fn usernames_keep_their_case_but_are_unique_case_insensitively(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        let user = create_user(&mut tx, &payload("Thrall", "thrall@example.com"), false)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(user.username, "Thrall", "the chosen capitalization is kept");

        // Any capitalization resolves to the same account.
        assert_eq!(
            find_user_id_by_username(&pool, "THRALL").await.unwrap(),
            Some(user.id)
        );
        // ...so a case-variant cannot be registered as a second account.
        let mut tx = pool.begin().await.unwrap();
        let result = create_user(&mut tx, &payload("thrall", "other@example.com"), false).await;
        assert!(matches!(
            result,
            Err(Error::External(UsersError::UsernameTaken))
        ));
    }

    #[sqlx::test]
    async fn renaming_applies_the_signup_rules(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        let thrall = create_user(&mut tx, &payload("Thrall", "t@example.com"), false)
            .await
            .unwrap();
        create_user(&mut tx, &payload("Jaina", "j@example.com"), false)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        rename_user(&pool, thrall.id, "Ey").await.unwrap();
        assert_eq!(
            find_user_id_by_username(&pool, "ey").await.unwrap(),
            Some(thrall.id)
        );
        assert!(
            find_user_id_by_username(&pool, "thrall")
                .await
                .unwrap()
                .is_none()
        );

        // A change of capitalization alone is fine.
        rename_user(&pool, thrall.id, "EY").await.unwrap();
        let user = db::find_user_by_id(&pool, thrall.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.username, "EY");

        // Taken in any case, reserved, malformed, or no such account: refused.
        assert!(matches!(
            rename_user(&pool, thrall.id, "JAINA").await,
            Err(Error::External(UsersError::UsernameTaken))
        ));
        assert!(matches!(
            rename_user(&pool, thrall.id, "Admin").await,
            Err(Error::External(UsersError::UsernameReserved))
        ));
        assert!(matches!(
            rename_user(&pool, thrall.id, "no spaces").await,
            Err(Error::External(UsersError::InvalidUsername(_)))
        ));
        assert!(matches!(
            rename_user(&pool, UserId(999_999), "Nobody").await,
            Err(Error::External(UsersError::NotFound))
        ));
    }

    #[sqlx::test]
    async fn test_create_user_email_taken(pool: PgPool) {
        // Uniqueness is enforced only between *verified* emails.
        let mut tx = pool.begin().await.unwrap();
        create_user(&mut tx, &payload("alice", "shared@example.com"), true)
            .await
            .expect("First insert should have succeeded");
        tx.commit().await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        let result = create_user(&mut tx, &payload("bob", "shared@example.com"), true).await;
        assert!(matches!(
            result,
            Err(Error::External(UsersError::EmailTaken))
        ));
    }

    #[sqlx::test]
    async fn test_email_kept_raw_but_unique_case_insensitively(pool: PgPool) {
        // Stored verbatim for display, normalized for uniqueness/lookups.
        let mut tx = pool.begin().await.unwrap();
        let user = create_user(&mut tx, &payload("casey", "Casey@Example.COM"), true)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            user.email.as_deref(),
            Some("Casey@Example.COM"),
            "email kept exactly as typed"
        );

        // A case-variant verified address collides on email_normalized.
        let mut tx = pool.begin().await.unwrap();
        let result = create_user(&mut tx, &payload("casey2", "casey@example.com"), true).await;
        assert!(
            matches!(result, Err(Error::External(UsersError::EmailTaken))),
            "a case-variant of a verified email must be rejected"
        );
    }

    #[sqlx::test]
    async fn test_create_user_allows_duplicate_unverified_emails(pool: PgPool) {
        // An unverified address is unproven, so two accounts may hold it; only a
        // verified collision is rejected.
        let mut tx = pool.begin().await.unwrap();
        create_user(&mut tx, &payload("alice", "shared@example.com"), false)
            .await
            .expect("first unverified insert should succeed");
        tx.commit().await.unwrap();

        let mut tx = pool.begin().await.unwrap();
        create_user(&mut tx, &payload("bob", "shared@example.com"), false)
            .await
            .expect("second unverified insert should also succeed");
        tx.commit().await.unwrap();
    }

    #[sqlx::test]
    async fn test_create_user_rejects_reserved_username(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        let result = create_user(&mut tx, &payload("admin", "admin@example.com"), false).await;
        assert!(matches!(
            result,
            Err(Error::External(UsersError::UsernameReserved))
        ));

        let result = create_user(&mut tx, &payload("self", "self@example.com"), false).await;
        assert!(matches!(
            result,
            Err(Error::External(UsersError::UsernameReserved))
        ));
    }

    #[sqlx::test]
    async fn test_create_user_rejects_an_invalid_username(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        // A space fails the `Username` shape check in create_user, surfacing as
        // the InvalidUsername domain error before the database is even touched.
        let result = create_user(&mut tx, &payload("Bad Name", "bad@example.com"), false).await;
        assert!(matches!(
            result,
            Err(Error::External(UsersError::InvalidUsername(_)))
        ));
    }

    #[sqlx::test]
    async fn test_create_user_rejects_a_non_ascii_email(pool: PgPool) {
        let mut tx = pool.begin().await.unwrap();
        // A Unicode local-part fails the `Email` ASCII-only check in create_user,
        // surfacing as the InvalidEmail domain error before the database is touched.
        let result = create_user(&mut tx, &payload("joe", "jöe@example.com"), false).await;
        assert!(matches!(
            result,
            Err(Error::External(UsersError::InvalidEmail(_)))
        ));
    }

    #[sqlx::test]
    async fn a_user_may_have_no_email(pool: PgPool) {
        let mut payload = payload("battletag1234", "unused@example.com");
        payload.email = None;
        let mut tx = pool.begin().await.unwrap();
        let user = create_user(&mut tx, &payload, false).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(user.email, None);
        assert!(!user.is_email_verified);

        let found = db::find_user_by_id(&pool, user.id).await.unwrap().unwrap();
        assert_eq!(found.email, None);
        assert_eq!(db::list_emails(&pool, user.id).await.unwrap().len(), 0);
        let listing = db::list_user_listings(&pool).await.unwrap();
        assert!(
            listing
                .iter()
                .any(|u| u.id == user.id && u.emails.is_empty())
        );
    }

    #[sqlx::test]
    async fn test_create_user_minimal_payload(pool: PgPool) {
        let payload = payload("min", "min@example.com");

        let mut tx = pool.begin().await.unwrap();
        let new_user = create_user(&mut tx, &payload, false)
            .await
            .expect("Failed to create minimal user");
        tx.commit().await.unwrap();

        assert_eq!(new_user.username, "min");
        assert_eq!(new_user.email.as_deref(), Some("min@example.com"));
        assert!(!new_user.is_email_verified);
        assert!(new_user.first_name.is_none());
        assert!(new_user.last_name.is_none());
    }
}
