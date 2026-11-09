//! The superusers' page of every account: who they are on the site, the sign-in accounts (Battle.net,
//! Discord) linked to them, and a rename.

use anyhow::Context;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use super::{UserId, UsersError};
use crate::extract::{Admin, SameOrigin};
use crate::guild::Rank;
use crate::{Result, State};

/// A sign-in account linked to a member, as a provider asserted it.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Connection {
    /// The provider's slug, such as `battlenet` or `discord`.
    pub provider: String,
    /// The provider's name, for display.
    pub provider_name: String,
    /// The provider's id for the account.
    pub subject: String,
    /// How the provider names the account (a BattleTag, a Discord username), when it said.
    pub label: Option<String>,
    /// When the member disconnected it; `None` while it is connected.
    #[serde(with = "iso8601::option")]
    pub disconnected_at: Option<OffsetDateTime>,
}

/// An account as the superusers' users page shows it.
#[derive(Debug, serde::Serialize)]
pub struct Account {
    pub id: UserId,
    pub username: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub guild_rank: Rank,
    pub is_superuser: bool,
    pub disabled: bool,
    /// Whether they can sign in with a password.
    pub has_password: bool,
    /// Their characters on the site, hidden ones left out.
    pub characters: i64,
    /// Every sign-in account ever linked, connected ones first.
    pub connections: Vec<Connection>,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
}

/// A connection row as the query gives it: the label still to read from the userinfo.
#[derive(Debug, serde::Deserialize)]
struct RawConnection {
    provider: String,
    provider_name: String,
    subject: String,
    userinfo: serde_json::Value,
    #[serde(with = "iso8601::option")]
    disconnected_at: Option<OffsetDateTime>,
}

/// How a provider names an account: the usual handle claims (a BattleTag), else Discord's
/// `global_name` or `username`.
fn label(userinfo: &serde_json::Value) -> Option<String> {
    ["username", "global_name"]
        .iter()
        .find_map(|key| {
            userinfo
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| crate::auth::oauth2::providers::identity_label(userinfo))
}

/// Every account, by username, with its sign-in accounts.
pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<Account>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            u.id AS "id: UserId",
            u.username,
            u.first_name,
            u.last_name,
            u.guild_rank AS "guild_rank: Rank",
            u.is_superuser,
            (u.disabled_at IS NOT NULL) AS "disabled!",
            EXISTS (
                SELECT 1 FROM auth_local_credentials l WHERE l.user_id = u.id
            ) AS "has_password!",
            (SELECT count(*) FROM characters ch WHERE ch.user_id = u.id AND NOT ch.is_hidden)
                AS "characters!",
            COALESCE(
                (SELECT jsonb_agg(
                     jsonb_build_object(
                         'provider', p.slug,
                         'provider_name', p.name,
                         'subject', c.subject,
                         'userinfo', c.raw_userinfo,
                         'disconnected_at', c.disconnected_at
                     ) ORDER BY c.disconnected_at NULLS FIRST, p.name
                 )
                 FROM auth_oauth2_provider_credentials c
                 JOIN auth_providers p ON p.id = c.provider_id
                 WHERE c.user_id = u.id),
                '[]'::jsonb
            ) AS "connections!: sqlx::types::Json<Vec<RawConnection>>",
            u.created_at
        FROM users u
        ORDER BY u.username_normalized
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Account {
            id: row.id,
            username: row.username,
            first_name: row.first_name,
            last_name: row.last_name,
            guild_rank: row.guild_rank,
            is_superuser: row.is_superuser,
            disabled: row.disabled,
            has_password: row.has_password,
            characters: row.characters,
            connections: row
                .connections
                .0
                .into_iter()
                .map(|raw| Connection {
                    label: label(&raw.userinfo),
                    provider: raw.provider,
                    provider_name: raw.provider_name,
                    subject: raw.subject,
                    disconnected_at: raw.disconnected_at,
                })
                .collect(),
            created_at: row.created_at,
        })
        .collect())
}

/// `GET /api/admin/users`. Superusers only.
async fn list_all(state: State, _admin: Admin) -> Result<Json<Vec<Account>>, UsersError> {
    Ok(Json(list(&state.pool).await.context("listing accounts")?))
}

#[derive(Debug, serde::Deserialize)]
struct Rename {
    username: String,
}

/// `PUT /api/admin/users/{id}/username`: renames an account, under signup's rules
/// ([`super::rename_user`]). Superusers only.
async fn rename(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Path(id): Path<UserId>,
    Json(input): Json<Rename>,
) -> Result<StatusCode, UsersError> {
    super::rename_user(&state.pool, id, input.username.trim()).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn admin_router() -> Router<State> {
    Router::new()
        .route("/users", get(list_all))
        .route("/users/{id}/username", put(rename))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guild::tests::member;

    #[sqlx::test]
    async fn lists_accounts_with_their_sign_ins(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        member(&pool, "Zx", Rank::Member).await;
        crate::auth::oauth2::providers::link_for_test(&pool, "battlenet", "ey#1", ey.id).await;
        crate::auth::oauth2::providers::link_for_test(&pool, "discord", "123", ey.id).await;

        let accounts = list(&pool).await.unwrap();
        assert_eq!(
            accounts
                .iter()
                .map(|a| a.username.as_str())
                .collect::<Vec<_>>(),
            ["Ey", "Zx"]
        );
        let providers: Vec<_> = accounts[0]
            .connections
            .iter()
            .map(|c| (c.provider.as_str(), c.subject.as_str()))
            .collect();
        assert_eq!(providers, [("battlenet", "ey#1"), ("discord", "123")]);
        assert!(accounts[1].connections.is_empty());
        assert!(!accounts[1].has_password);
    }

    #[sqlx::test]
    async fn renames_under_signups_rules(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        member(&pool, "Zx", Rank::Member).await;
        super::super::rename_user(&pool, ey.id, "Eyl")
            .await
            .unwrap();
        assert_eq!(list(&pool).await.unwrap()[0].username, "Eyl");
        assert!(matches!(
            super::super::rename_user(&pool, ey.id, "zx").await,
            Err(crate::Error::External(UsersError::UsernameTaken))
        ));
    }

    #[test]
    fn labels_battletags_and_discord_names() {
        let battlenet = serde_json::json!({ "sub": "1", "battle_tag": "Zoomkins#1878" });
        assert_eq!(label(&battlenet).as_deref(), Some("Zoomkins#1878"));
        let discord = serde_json::json!({ "id": "1", "username": "zoom", "global_name": "Zoom" });
        assert_eq!(label(&discord).as_deref(), Some("zoom"));
        assert_eq!(label(&serde_json::json!({ "sub": "1" })), None);
    }
}
