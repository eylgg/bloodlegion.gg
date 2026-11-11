//! The guild's ranks in game, as roster uploads saw them, and the site rank each stands for, so a
//! member whose rank on the site differs from their characters' in game can be pointed out. A
//! rank is matched the first time an upload sees it ([`guess`]: rank 0, or exactly a site rank's
//! name); a superuser changes it, or leaves it out of the comparison (an alt rank).

use axum::Json;
use axum::extract::Query;
use sqlx::PgPool;

use super::{CharacterError, external};
use crate::extract::{Admin, SameOrigin};
use crate::guild::Rank;
use crate::users::UserId;
use crate::{Result, State};

/// The site rank a rank in game stands for until a superuser says otherwise: the guild master
/// (rank 0) is the leader, and a rank named exactly as a site rank ("Officer", in any case) is that
/// rank. Anything else ("Raider Alt", "Core") is left out of the comparison.
pub fn guess(name: &str, index: i16) -> Option<Rank> {
    if index == 0 {
        return Some(Rank::Leader);
    }
    Rank::ALL
        .into_iter()
        .find(|rank| rank.slug().eq_ignore_ascii_case(name.trim()))
}

/// A rank in game, as the guild's uploads saw it.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct GameRank {
    pub name: String,
    /// Its number when last seen: 0 is the guild master.
    pub rank_index: i16,
    /// The site rank it stands for; `None` to leave it out of the comparison.
    pub site_rank: Option<Rank>,
    /// How many characters on the site have it now.
    pub characters: i64,
}

/// The ranks in game of the guild in the settings, highest first.
pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<GameRank>> {
    sqlx::query_as!(
        GameRank,
        r#"
        SELECT r.name, r.rank_index, r.site_rank AS "site_rank: Rank",
               (SELECT count(*) FROM characters c
                WHERE c.guild = r.guild AND c.guild_rank = r.name AND NOT c.is_hidden)
                   AS "characters!"
        FROM guild_ranks r
        WHERE r.guild = (SELECT guild_name FROM guild_settings)
        ORDER BY r.rank_index, r.name
        "#
    )
    .fetch_all(pool)
    .await
}

/// Says which site rank the guild's rank `name` stands for, or `None` to leave it out.
pub async fn set(pool: &PgPool, name: &str, site_rank: Option<Rank>) -> Result<(), CharacterError> {
    let updated = sqlx::query!(
        r#"UPDATE guild_ranks SET site_rank = $2
           WHERE guild = (SELECT guild_name FROM guild_settings) AND name = $1"#,
        name,
        site_rank.map(Rank::slug),
    )
    .execute(pool)
    .await?
    .rows_affected();
    if updated == 0 {
        return external(CharacterError::NotFound);
    }
    Ok(())
}

/// A member whose rank on the site is not the one their characters have in game.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Mismatch {
    pub user_id: UserId,
    pub username: String,
    /// Their rank on the site.
    pub rank: Rank,
    /// The highest site rank their characters in the guild stand for; `None` when none of their
    /// characters is in the guild.
    pub expected: Option<Rank>,
    /// Their characters in the guild, with their ranks in game.
    pub characters: Vec<RankedCharacter>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct RankedCharacter {
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    pub guild_rank: Option<String>,
}

/// The enabled members whose site rank differs from their characters' in game: the highest site
/// rank among their characters in the guild (ranks left out not counting), or a rank above public
/// with no character in the guild at all. A member whose characters have only ranks left out is
/// not listed. None until an upload of the guild has said who is in it.
pub async fn mismatches(pool: &PgPool) -> sqlx::Result<Vec<Mismatch>> {
    if super::db::latest_upload(pool).await?.is_none() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query!(
        r#"
        SELECT u.id AS "user_id: UserId", u.username, u.guild_rank AS "rank: Rank",
               c.first_name AS "first_name?", c.last_name AS "last_name?", c.class AS "class?",
               c.guild_rank AS "guild_rank?", r.site_rank AS "site_rank?: Rank"
        FROM users u
        LEFT JOIN characters c ON c.user_id = u.id AND c.in_guild AND NOT c.is_hidden
        LEFT JOIN guild_ranks r ON r.guild = c.guild AND r.name = c.guild_rank
        WHERE u.disabled_at IS NULL
        ORDER BY u.username_normalized, c.guild_rank_index NULLS LAST, c.name_normalized
        "#
    )
    .fetch_all(pool)
    .await?;
    let mut found: Vec<Mismatch> = Vec::new();
    let mut mapped: Vec<Option<Rank>> = Vec::new();
    for row in rows {
        if found.last().is_none_or(|m| m.user_id != row.user_id) {
            found.push(Mismatch {
                user_id: row.user_id,
                username: row.username,
                rank: row.rank,
                expected: None,
                characters: Vec::new(),
            });
            mapped.push(None);
        }
        let (Some(member), Some(highest)) = (found.last_mut(), mapped.last_mut()) else {
            unreachable!("pushed above");
        };
        if let (Some(first_name), Some(last_name), Some(class)) =
            (row.first_name, row.last_name, row.class)
        {
            member.characters.push(RankedCharacter {
                first_name,
                last_name,
                class,
                guild_rank: row.guild_rank,
            });
            // Highest first: the least in the ranks' order.
            if let Some(rank) = row.site_rank {
                *highest = Some(highest.map_or(rank, |h| h.min(rank)));
            }
        }
    }
    Ok(found
        .into_iter()
        .zip(mapped)
        .filter_map(|(mut member, highest)| {
            let differs = if member.characters.is_empty() {
                member.rank != Rank::Public
            } else {
                highest.is_some_and(|rank| rank != member.rank)
            };
            member.expected = highest;
            differs.then_some(member)
        })
        .collect())
}

/// `GET /api/admin/rosters/ranks`: [`list`]. Superusers only.
pub(super) async fn list_ranks(state: State, _admin: Admin) -> Result<Json<Vec<GameRank>>> {
    Ok(Json(list(&state.pool).await?))
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct RankName {
    name: String,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct SiteRank {
    site_rank: Option<Rank>,
}

/// `PUT /api/admin/rosters/ranks?name=Officer`: `{ "site_rank": "officer" }`, or `null` to leave
/// it out. Superusers only.
pub(super) async fn set_rank(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Query(rank): Query<RankName>,
    Json(input): Json<SiteRank>,
) -> Result<Json<Vec<GameRank>>, CharacterError> {
    set(&state.pool, &rank.name, input.site_rank).await?;
    Ok(Json(list(&state.pool).await?))
}

/// `GET /api/admin/rosters/rank-mismatches`: [`mismatches`]. Superusers only.
pub(super) async fn list_mismatches(state: State, _admin: Admin) -> Result<Json<Vec<Mismatch>>> {
    Ok(Json(mismatches(&state.pool).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_ranks_by_name() {
        assert_eq!(guess("Warchief", 0), Some(Rank::Leader));
        assert_eq!(guess("Officer", 1), Some(Rank::Officer));
        assert_eq!(guess("raider", 3), Some(Rank::Raider));
        assert_eq!(guess("Trial", 4), Some(Rank::Trial));
        assert_eq!(guess("Member", 5), Some(Rank::Member));
        assert_eq!(guess("Leader", 2), Some(Rank::Leader));
        assert_eq!(guess("Officer Alt", 2), None);
        assert_eq!(guess("Core Raider", 3), None);
        assert_eq!(guess("Peon", 6), None);
    }
}
