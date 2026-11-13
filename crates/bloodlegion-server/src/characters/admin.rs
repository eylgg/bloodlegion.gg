//! The superusers' page of every character on the site, in the guild or not and played or not,
//! with who plays each and whether the guild's latest roster upload named it.

use axum::routing::get;
use axum::{Json, Router};
use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use crate::extract::Admin;
use crate::users::UserId;
use crate::{Result, State};

/// A character as the superusers' characters page shows it.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct AdminCharacter {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    pub race: Option<String>,
    pub level: i16,
    /// Who plays it; `None` for an unclaimed one.
    pub user_id: Option<UserId>,
    pub username: Option<String>,
    pub is_main: bool,
    /// Named by the latest roster upload of the guild.
    pub in_guild: bool,
    /// Its rank in the guild in game, as that upload saw it: the game's number (0 the guild
    /// master), and its name there when an upload has sent it.
    pub guild_rank: Option<i16>,
    pub guild_rank_name: Option<String>,
    /// When a Battle.net sign-in last listed it on its player's account; `None` if none has.
    #[serde(with = "iso8601::option")]
    pub battlenet_seen_at: Option<OffsetDateTime>,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
}

/// Every character, by name.
pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<AdminCharacter>> {
    sqlx::query_as!(
        AdminCharacter,
        r#"
        SELECT c.id, c.first_name, c.last_name, c.class, c.race, c.level,
               c.user_id AS "user_id: UserId", u.username AS "username?",
               c.is_main, in_guild(c.guild) AS "in_guild!", c.guild_rank,
               (SELECT r.name FROM guild_ranks r WHERE r.guild = c.guild AND r.rank = c.guild_rank)
                   AS guild_rank_name, c.battlenet_seen_at,
               c.created_at
        FROM characters c
        LEFT JOIN users u ON u.id = c.user_id
        ORDER BY c.name_normalized, c.id
        "#
    )
    .fetch_all(pool)
    .await
}

/// `GET /api/admin/characters`: [`list`]. Superusers only.
async fn list_all(state: State, _admin: Admin) -> Result<Json<Vec<AdminCharacter>>> {
    Ok(Json(list(&state.pool).await?))
}

/// The superusers' routes, under `/api/admin`.
pub fn admin_router() -> Router<State> {
    Router::new().route("/characters", get(list_all))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characters::tests::{add, input};
    use crate::guild::Rank;
    use crate::guild::tests::member;

    #[sqlx::test]
    async fn lists_every_character_guilded_or_not(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let gg = add(&pool, &ey, input("Eyl", "Gg", "warrior", true))
            .await
            .unwrap();
        add(&pool, &ey, input("Eyl", "Mg", "mage", false))
            .await
            .unwrap();
        sqlx::query!(
            "UPDATE characters SET guild = 'BL', guild_rank = 1 WHERE id = $1",
            gg.id
        )
        .execute(&pool)
        .await
        .unwrap();

        let all = list(&pool).await.unwrap();
        let names: Vec<_> = all
            .iter()
            .map(|c| {
                (
                    c.last_name.as_str(),
                    c.is_main,
                    c.in_guild,
                    c.guild_rank,
                    c.username.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            names,
            [
                ("Gg", true, true, Some(1), Some("Ey")),
                ("Mg", false, false, None, Some("Ey"))
            ]
        );
    }
}
