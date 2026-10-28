//! Linking more accounts at a provider to the signed-in member: most people with more than one
//! Battle.net account keep characters on each, so a Blood Legion account may hold several.
//!
//! Linking is a sign-in at the provider started from the profile page
//! (`GET /api/auth/oauth2/providers/{slug}/link`): the in-flight request carries the member's id,
//! and the callback attaches the identity it verifies to that member instead of signing anyone in.
//! An identity belongs to one account at most. Unlinking deletes the credential, which frees the
//! identity to be linked again; the member's last way to sign in cannot be unlinked.

use anyhow::Context;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use crate::extract::SameOrigin;
use crate::users::{User, UserId};
use crate::{Error, Problem, Result, Slug, State};

use super::provision::{build_profile, identity_label};
use super::{ProviderError, db};

/// What linking an identity did.
#[derive(Debug, PartialEq, Eq)]
pub enum Linked {
    /// Newly linked to the member.
    New,
    /// It was already theirs (the provider signed them straight back into a linked account).
    Existing,
}

impl Linked {
    /// The `?linked=` the profile page reads to say which happened.
    pub fn code(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Existing => "existing",
        }
    }
}

/// Attaches the verified identity (`subject` at `provider`) to `user_id`. Refused when it already
/// belongs to someone else, including when another link of the same identity wins a race or it
/// is one they unlinked (an unlinked account stays theirs; see [`unlink`]), and at a
/// provider that does not allow multiple connections (Discord) when the member already has an
/// account there. Linking one they unlinked themselves makes it live again.
pub(super) async fn link(
    pool: &PgPool,
    provider: &db::Provider,
    user_id: UserId,
    subject: &str,
    userinfo: &serde_json::Value,
) -> Result<Linked, ProviderError> {
    let owner = db::find_credential_owner(pool, provider.id, subject)
        .await
        .context("looking up the provider credential")?;
    if owner.is_some_and(|owner| owner.user_id != user_id) {
        return Err(Error::External(ProviderError::AlreadyLinked(
            provider.name.clone(),
        )));
    }
    let mut tx = pool
        .begin()
        .await
        .context("beginning a transaction to link")?;
    // Theirs already and live: the provider signed them straight back into it.
    if owner.is_some_and(|owner| owner.is_live) {
        return Ok(Linked::Existing);
    }
    if !provider.is_multiple_connections_allowed
        && db::lock_has_credential_at(&mut tx, user_id, provider.id)
            .await
            .context("checking for an account already linked at the provider")?
    {
        return Err(Error::External(
            ProviderError::MultipleConnectionsNotAllowed(provider.name.clone()),
        ));
    }
    if owner.is_some() {
        db::reconnect_credential(&mut tx, provider.id, subject, user_id)
            .await
            .context("reconnecting the provider credential")?;
        tx.commit().await.context("committing the link")?;
        return Ok(Linked::New);
    }
    let profile = build_profile(provider, userinfo)?;
    db::insert_credential(&mut tx, provider.id, subject, user_id, userinfo, &profile)
        .await
        .map_err(|error| {
            crate::error::classify_db_error(error, |constraint| {
                (constraint == "auth_oauth2_provider_credentials_provider_id_subject_key")
                    .then(|| ProviderError::AlreadyLinked(provider.name.clone()))
            })
        })?;
    tx.commit().await.context("committing the link")?;
    Ok(Linked::New)
}

#[derive(Debug, thiserror::Error, Problem)]
pub enum LinkError {
    #[error("linked account not found")]
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such linked account.")]
    NotFound,
    #[error("disconnection not allowed")]
    #[problem(
        status = FORBIDDEN,
        title = "Unlinking Disabled",
        detail = "Accounts at this provider cannot be unlinked."
    )]
    DisconnectionNotAllowed,
    #[error("last login method")]
    #[problem(
        status = CONFLICT,
        title = "Last Way to Sign In",
        detail = "This is the only way you can sign in. Link another account before unlinking it."
    )]
    LastLoginMethod,
}

/// One account at a provider linked to the member, for the profile page.
#[derive(Debug, serde::Serialize)]
pub struct LinkedAccount {
    pub id: i64,
    pub provider_slug: Slug,
    pub provider_name: String,
    /// How the provider names the account (a BattleTag), when it said.
    pub identity: Option<String>,
    /// Whether unlinking it is allowed now: the provider permits it and, unless it is
    /// connection-only (never a way in), the member has another way to sign in.
    pub can_unlink: bool,
    #[serde(with = "iso8601")]
    pub linked_at: OffsetDateTime,
}

pub async fn list(pool: &PgPool, user_id: UserId) -> anyhow::Result<Vec<LinkedAccount>> {
    let credentials = db::list_linked_credentials(pool, user_id)
        .await
        .context("listing the linked accounts")?;
    // A password only counts as a way in while password login is on.
    let password_usable = crate::auth::local::is_enabled(pool)
        .await
        .context("checking whether local login is enabled")?
        && crate::auth::local::has_password(pool, user_id)
            .await
            .context("checking the local password")?;
    // Connection-only accounts (Discord) are not ways in, so they never make another one
    // unlinkable.
    let ways_in = credentials.iter().filter(|c| !c.is_connection_only).count();
    let has_another = ways_in > 1 || password_usable;
    Ok(credentials
        .into_iter()
        .map(|credential| LinkedAccount {
            id: credential.id,
            provider_slug: credential.provider_slug,
            provider_name: credential.provider_name,
            identity: identity_label(&credential.raw_userinfo),
            can_unlink: credential.is_disconnection_allowed
                && (credential.is_connection_only || has_another),
            linked_at: credential.created_at,
        })
        .collect())
}

/// Unlinks the member's linked account `id`, unless the provider forbids it or it is their last way
/// to sign in. A connection-only account (Discord) is never a way in, so it never counts as one:
/// with only Battle.net to sign in through, a member always keeps one Battle.net account. An
/// unlinked account stays theirs (see [`link`]): it is no way in, and no one else can link it.
pub async fn unlink(pool: &PgPool, user_id: UserId, id: i64) -> Result<(), LinkError> {
    let local_enabled = crate::auth::local::is_enabled(pool)
        .await
        .context("checking whether local login is enabled")?;
    let mut tx = pool.begin().await?;
    let others = db::lock_other_login_methods(&mut tx, user_id, id).await?;
    let policy = match db::credential_policy(&mut tx, user_id, id).await? {
        None => return Err(Error::External(LinkError::NotFound)),
        Some(policy) if !policy.is_disconnection_allowed => {
            return Err(Error::External(LinkError::DisconnectionNotAllowed));
        }
        Some(policy) => policy,
    };
    if !policy.is_connection_only
        && others.credentials == 0
        && !(others.has_password && local_enabled)
    {
        return Err(Error::External(LinkError::LastLoginMethod));
    }
    // The account stays theirs once unlinked, so it cannot move to another member; they may link
    // it again.
    db::disconnect_credential(&mut tx, user_id, id).await?;
    tx.commit().await?;
    Ok(())
}

/// `GET /api/auth/linked-accounts`: the signed-in member's linked accounts, oldest first.
async fn list_handler(state: State, user: User) -> Result<Json<Vec<LinkedAccount>>> {
    Ok(Json(list(&state.pool, user.id).await?))
}

/// `DELETE /api/auth/linked-accounts/{id}`.
async fn unlink_handler(
    state: State,
    _: SameOrigin,
    user: User,
    Path(id): Path<i64>,
) -> Result<StatusCode, LinkError> {
    unlink(&state.pool, user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<State> {
    Router::new()
        .route("/", get(list_handler))
        .route("/{id}", delete(unlink_handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::CreateUserPayload;

    async fn battlenet(pool: &PgPool, is_disconnection_allowed: bool) -> db::Provider {
        let slug = Slug::try_from("battlenet").unwrap();
        let mut tx = pool.begin().await.unwrap();
        db::insert_provider(
            &mut tx,
            &slug,
            "Battle.net",
            Some("https://oauth.battle.test/oauth"),
            "client",
            &crate::crypto::Ciphertext::for_test("secret"),
            crate::auth::ProviderFlags {
                is_registration_allowed: true,
                is_auto_connection_allowed: false,
                is_email_verified: false,
                is_disconnection_allowed,
                is_unclaimed_username_connection_allowed: false,
            },
            &serde_json::json!([]),
            "openid wow.profile",
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        // As deployed: Battle.net takes several accounts per member.
        db::set_multiple_connections_allowed(pool, &slug, true)
            .await
            .unwrap();
        db::find_provider_by_slug(pool, &slug)
            .await
            .unwrap()
            .unwrap()
    }

    /// Discord as deployed: connection-only, one account per member, unlinkable.
    async fn discord(pool: &PgPool) -> db::Provider {
        let slug = Slug::try_from("discord").unwrap();
        let mut tx = pool.begin().await.unwrap();
        db::insert_provider(
            &mut tx,
            &slug,
            "Discord",
            Some("https://discord.test"),
            "discord-client",
            &crate::crypto::Ciphertext::for_test("secret"),
            crate::auth::ProviderFlags {
                is_registration_allowed: false,
                is_auto_connection_allowed: false,
                is_email_verified: false,
                is_disconnection_allowed: true,
                is_unclaimed_username_connection_allowed: false,
            },
            &serde_json::json!([]),
            "openid identify",
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        db::set_connection_only(pool, &slug, true).await.unwrap();
        db::find_provider_by_slug(pool, &slug)
            .await
            .unwrap()
            .unwrap()
    }

    async fn member(pool: &PgPool, username: &str) -> UserId {
        let mut tx = pool.begin().await.unwrap();
        let user = crate::users::insert_user(
            &mut tx,
            &CreateUserPayload {
                username: username.into(),
                email: None,
                first_name: None,
                last_name: None,
                is_superuser: false,
            },
            false,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        user.id
    }

    fn userinfo(tag: &str) -> serde_json::Value {
        serde_json::json!({ "sub": tag, "battle_tag": tag })
    }

    #[sqlx::test]
    async fn links_several_accounts_and_refuses_anothers(pool: PgPool) {
        let provider = battlenet(&pool, true).await;
        let ey = member(&pool, "Ey").await;
        let other = member(&pool, "Other").await;

        for tag in ["Ey#1111", "EyAlt#2222"] {
            let linked = link(&pool, &provider, ey, tag, &userinfo(tag))
                .await
                .unwrap();
            assert_eq!(linked, Linked::New);
        }
        // The provider signing them straight back into a linked account changes nothing.
        let again = link(&pool, &provider, ey, "Ey#1111", &userinfo("Ey#1111"))
            .await
            .unwrap();
        assert_eq!(again, Linked::Existing);
        // Someone else's account cannot take it.
        assert!(matches!(
            link(&pool, &provider, other, "Ey#1111", &userinfo("Ey#1111")).await,
            Err(Error::External(ProviderError::AlreadyLinked(_)))
        ));

        let accounts = list(&pool, ey).await.unwrap();
        let tags: Vec<_> = accounts.iter().map(|a| a.identity.as_deref()).collect();
        assert_eq!(tags, vec![Some("Ey#1111"), Some("EyAlt#2222")]);
        assert!(accounts.iter().all(|a| a.can_unlink));
        // Signing in with the second account reaches the same member.
        assert_eq!(
            db::find_credential_owner(&pool, provider.id, "EyAlt#2222")
                .await
                .unwrap()
                .map(|owner| owner.user_id),
            Some(ey)
        );
    }

    #[sqlx::test]
    async fn without_multiple_connections_a_member_links_one_account(pool: PgPool) {
        let slug = Slug::try_from("battlenet").unwrap();
        battlenet(&pool, true).await;
        assert!(
            db::set_multiple_connections_allowed(&pool, &slug, false)
                .await
                .unwrap()
        );
        let provider = db::find_provider_by_slug(&pool, &slug)
            .await
            .unwrap()
            .unwrap();
        assert!(!provider.is_multiple_connections_allowed);
        let ey = member(&pool, "Ey").await;
        let other = member(&pool, "Other").await;

        let linked = link(&pool, &provider, ey, "Ey#1111", &userinfo("Ey#1111"))
            .await
            .unwrap();
        assert_eq!(linked, Linked::New);
        // Signing straight back into the linked one is still fine...
        let again = link(&pool, &provider, ey, "Ey#1111", &userinfo("Ey#1111"))
            .await
            .unwrap();
        assert_eq!(again, Linked::Existing);
        // ...but a second account is refused.
        assert!(matches!(
            link(&pool, &provider, ey, "EyAlt#2222", &userinfo("EyAlt#2222")).await,
            Err(Error::External(
                ProviderError::MultipleConnectionsNotAllowed(_)
            ))
        ));
        // The limit is per member: someone else links theirs.
        let theirs = link(
            &pool,
            &provider,
            other,
            "EyAlt#2222",
            &userinfo("EyAlt#2222"),
        )
        .await
        .unwrap();
        assert_eq!(theirs, Linked::New);
    }

    #[sqlx::test]
    async fn a_connection_only_account_never_counts_as_a_way_in(pool: PgPool) {
        let battlenet = battlenet(&pool, true).await;
        let discord = discord(&pool).await;
        let ey = member(&pool, "Ey").await;
        link(&pool, &battlenet, ey, "Ey#1111", &userinfo("Ey#1111"))
            .await
            .unwrap();
        link(
            &pool,
            &discord,
            ey,
            "1234",
            &serde_json::json!({ "sub": "1234" }),
        )
        .await
        .unwrap();

        // Discord is unlinkable; the only Battle.net account is not, Discord notwithstanding.
        let accounts = list(&pool, ey).await.unwrap();
        let at = |slug: &str| {
            accounts
                .iter()
                .find(|a| a.provider_slug.as_ref() == slug)
                .unwrap()
        };
        assert!(!at("battlenet").can_unlink);
        assert!(at("discord").can_unlink);
        assert!(matches!(
            unlink(&pool, ey, at("battlenet").id).await,
            Err(Error::External(LinkError::LastLoginMethod))
        ));
        unlink(&pool, ey, at("discord").id).await.unwrap();
        assert_eq!(list(&pool, ey).await.unwrap().len(), 1);
    }

    #[sqlx::test]
    async fn an_unlinked_discord_account_stays_its_members(pool: PgPool) {
        discord(&pool).await;
        let slug = Slug::try_from("discord").unwrap();
        db::set_multiple_connections_allowed(&pool, &slug, false)
            .await
            .unwrap();
        let provider = db::find_provider_by_slug(&pool, &slug)
            .await
            .unwrap()
            .unwrap();
        let one = member(&pool, "One").await;
        let two = member(&pool, "Two").await;
        let discord_user = |id: &str| serde_json::json!({ "sub": id });

        // Member 1 links Discord account 1, unlinks it, and links Discord account 2.
        link(&pool, &provider, one, "1", &discord_user("1"))
            .await
            .unwrap();
        let first = list(&pool, one).await.unwrap()[0].id;
        unlink(&pool, one, first).await.unwrap();
        assert!(list(&pool, one).await.unwrap().is_empty());
        assert_eq!(
            link(&pool, &provider, one, "2", &discord_user("2"))
                .await
                .unwrap(),
            Linked::New
        );

        // Discord account 1 cannot move to member 2...
        let refused = link(&pool, &provider, two, "1", &discord_user("1")).await;
        assert!(matches!(
            &refused,
            Err(Error::External(ProviderError::AlreadyLinked(name))) if name == "Discord"
        ));

        // ...but member 1 can have it back, once account 2 is unlinked (one Discord account each).
        assert!(matches!(
            link(&pool, &provider, one, "1", &discord_user("1")).await,
            Err(Error::External(
                ProviderError::MultipleConnectionsNotAllowed(_)
            ))
        ));
        let second = list(&pool, one).await.unwrap()[0].id;
        unlink(&pool, one, second).await.unwrap();
        assert_eq!(
            link(&pool, &provider, one, "1", &discord_user("1"))
                .await
                .unwrap(),
            Linked::New
        );
        let accounts = list(&pool, one).await.unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, first);
        // Linked again, signing straight back in changes nothing.
        assert_eq!(
            link(&pool, &provider, one, "1", &discord_user("1"))
                .await
                .unwrap(),
            Linked::Existing
        );
    }

    #[sqlx::test]
    async fn unlinks_all_but_the_last_way_in(pool: PgPool) {
        let provider = battlenet(&pool, true).await;
        let ey = member(&pool, "Ey").await;
        let other = member(&pool, "Other").await;
        link(&pool, &provider, ey, "Ey#1111", &userinfo("Ey#1111"))
            .await
            .unwrap();
        link(&pool, &provider, ey, "EyAlt#2222", &userinfo("EyAlt#2222"))
            .await
            .unwrap();
        let accounts = list(&pool, ey).await.unwrap();

        // Another member cannot unlink it.
        assert!(matches!(
            unlink(&pool, other, accounts[1].id).await,
            Err(Error::External(LinkError::NotFound))
        ));
        unlink(&pool, ey, accounts[1].id).await.unwrap();

        // The one left is the only way in.
        let left = list(&pool, ey).await.unwrap();
        assert_eq!(left.len(), 1);
        assert!(!left[0].can_unlink);
        assert!(matches!(
            unlink(&pool, ey, left[0].id).await,
            Err(Error::External(LinkError::LastLoginMethod))
        ));

        // The unlinked account stays theirs: it signs no one in...
        assert!(matches!(
            super::super::provision::provision_user(
                &pool,
                &provider,
                "EyAlt#2222",
                &userinfo("EyAlt#2222")
            )
            .await,
            Err(Error::External(ProviderError::Unlinked(name))) if name == "Battle.net"
        ));
        // ...no one else can link it...
        assert!(matches!(
            link(
                &pool,
                &provider,
                other,
                "EyAlt#2222",
                &userinfo("EyAlt#2222")
            )
            .await,
            Err(Error::External(ProviderError::AlreadyLinked(_)))
        ));
        // ...and they can link it again, when it signs them back in.
        assert_eq!(
            link(&pool, &provider, ey, "EyAlt#2222", &userinfo("EyAlt#2222"))
                .await
                .unwrap(),
            Linked::New
        );
        assert_eq!(list(&pool, ey).await.unwrap().len(), 2);
        assert!(matches!(
            super::super::provision::provision_user(
                &pool,
                &provider,
                "EyAlt#2222",
                &userinfo("EyAlt#2222")
            )
            .await,
            Ok(super::super::provision::Provisioned::User(user)) if user == ey
        ));
    }

    #[sqlx::test]
    async fn respects_a_provider_that_forbids_unlinking(pool: PgPool) {
        let provider = battlenet(&pool, false).await;
        let ey = member(&pool, "Ey").await;
        for tag in ["Ey#1111", "EyAlt#2222"] {
            link(&pool, &provider, ey, tag, &userinfo(tag))
                .await
                .unwrap();
        }
        let accounts = list(&pool, ey).await.unwrap();
        assert!(accounts.iter().all(|a| !a.can_unlink));
        assert!(matches!(
            unlink(&pool, ey, accounts[0].id).await,
            Err(Error::External(LinkError::DisconnectionNotAllowed))
        ));
    }
}
