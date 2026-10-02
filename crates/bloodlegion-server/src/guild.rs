//! The guild's members and their ranks, the old site's ranks carried over. Leaders and officers
//! (and any superuser) run the guild's records; raiders and trials make up the raiding roster.

pub mod api;
mod db;

use sqlx::PgPool;

use crate::characters::Character;
use crate::users::{User, UserId};
use crate::{Error, Problem, Result};

pub use api::router;

/// A guild rank, highest first. Stored as its snake_case name (`users.guild_rank`).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
)]
#[sqlx(type_name = "text", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Rank {
    Leader,
    Officer,
    Raider,
    Trial,
    Member,
    Friend,
    Retired,
}

impl Rank {
    pub const ALL: [Rank; 7] = [
        Rank::Leader,
        Rank::Officer,
        Rank::Raider,
        Rank::Trial,
        Rank::Member,
        Rank::Friend,
        Rank::Retired,
    ];

    pub fn is_officer(self) -> bool {
        matches!(self, Rank::Leader | Rank::Officer)
    }

    pub fn slug(self) -> &'static str {
        match self {
            Rank::Leader => "leader",
            Rank::Officer => "officer",
            Rank::Raider => "raider",
            Rank::Trial => "trial",
            Rank::Member => "member",
            Rank::Friend => "friend",
            Rank::Retired => "retired",
        }
    }

    pub fn parse(slug: &str) -> Option<Rank> {
        Rank::ALL.into_iter().find(|rank| rank.slug() == slug)
    }
}

#[derive(Debug, thiserror::Error, Problem)]
pub enum GuildError {
    #[error("member not found")]
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such member.")]
    NotFound,
    #[error("rank change refused")]
    #[problem(
        status = FORBIDDEN,
        title = "Forbidden",
        detail = "Only the guild leader can make or unmake officers, and nobody sets their own rank."
    )]
    RankChangeRefused,
}

/// A member of the guild (an account), with their characters, main first.
#[derive(Debug, serde::Serialize)]
pub struct Member {
    pub id: UserId,
    pub username: String,
    pub guild_rank: Rank,
    pub characters: Vec<Character>,
}

/// Every enabled account with its characters, by rank and then username.
pub async fn members(pool: &PgPool) -> sqlx::Result<Vec<Member>> {
    let mut members: Vec<Member> = db::list_members(pool)
        .await?
        .into_iter()
        .map(|row| Member {
            id: row.id,
            username: row.username,
            guild_rank: row.guild_rank,
            characters: Vec::new(),
        })
        .collect();
    let mut index: std::collections::HashMap<UserId, usize> = members
        .iter()
        .enumerate()
        .map(|(i, member)| (member.id, i))
        .collect();
    for character in crate::characters::list(pool).await? {
        if let Some(i) = character.user_id.and_then(|id| index.get_mut(&id)) {
            members[*i].characters.push(character);
        }
    }
    for member in &mut members {
        member.characters.sort_by_key(|c| !c.is_main);
    }
    members.sort_by(|a, b| {
        a.guild_rank
            .cmp(&b.guild_rank)
            .then_with(|| a.username.to_lowercase().cmp(&b.username.to_lowercase()))
    });
    Ok(members)
}

/// Whether `actor` may move `target` from `from` to `to`: officers set ranks, but only the leader
/// (or a superuser) makes or unmakes officers and leaders, and only a superuser sets their own.
fn may_set_rank(actor: &User, target: UserId, from: Rank, to: Rank) -> bool {
    if actor.is_superuser {
        return true;
    }
    if !actor.is_officer() || actor.id == target {
        return false;
    }
    actor.guild_rank == Rank::Leader || !(from.is_officer() || to.is_officer())
}

pub async fn set_rank(
    pool: &PgPool,
    actor: &User,
    target: UserId,
    rank: Rank,
) -> Result<(), GuildError> {
    let mut tx = pool.begin().await?;
    let Some(current) = db::lock_rank(&mut tx, target).await? else {
        return Err(Error::External(GuildError::NotFound));
    };
    if !may_set_rank(actor, target, current, rank) {
        return Err(Error::External(GuildError::RankChangeRefused));
    }
    db::set_rank(&mut tx, target, rank).await?;
    tx.commit().await?;
    Ok(())
}

/// Sets a rank without the permission rules, for the operator's CLI. Returns whether the account
/// exists.
pub async fn force_rank(pool: &PgPool, target: UserId, rank: Rank) -> sqlx::Result<bool> {
    let mut tx = pool.begin().await?;
    let exists = db::lock_rank(&mut tx, target).await?.is_some();
    if exists {
        db::set_rank(&mut tx, target, rank).await?;
    }
    tx.commit().await?;
    Ok(exists)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::users::CreateUserPayload;

    /// A plain account for tests, at `rank`.
    pub(crate) async fn member(pool: &PgPool, username: &str, rank: Rank) -> User {
        let mut tx = pool.begin().await.unwrap();
        let mut user = crate::users::insert_user(
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
        db::set_rank(&mut tx, user.id, rank).await.unwrap();
        tx.commit().await.unwrap();
        user.guild_rank = rank;
        user
    }

    #[test]
    fn ranks_round_trip_and_order_highest_first() {
        for rank in Rank::ALL {
            assert_eq!(Rank::parse(rank.slug()), Some(rank));
        }
        assert!(Rank::Leader < Rank::Officer && Rank::Trial < Rank::Retired);
        assert!(Rank::Officer.is_officer() && !Rank::Raider.is_officer());
    }

    #[sqlx::test]
    async fn officers_set_ranks_and_only_the_leader_makes_officers(pool: PgPool) {
        let leader = member(&pool, "Leader", Rank::Leader).await;
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let raider = member(&pool, "Raider", Rank::Raider).await;
        let refused = |result: Result<(), GuildError>| {
            matches!(result, Err(Error::External(GuildError::RankChangeRefused)))
        };

        set_rank(&pool, &officer, raider.id, Rank::Trial)
            .await
            .unwrap();
        assert!(refused(
            set_rank(&pool, &officer, raider.id, Rank::Officer).await
        ));
        assert!(refused(
            set_rank(&pool, &officer, leader.id, Rank::Member).await
        ));
        assert!(refused(
            set_rank(&pool, &officer, officer.id, Rank::Leader).await
        ));
        assert!(refused(
            set_rank(&pool, &raider, officer.id, Rank::Member).await
        ));
        set_rank(&pool, &leader, raider.id, Rank::Officer)
            .await
            .unwrap();

        let ranks: Vec<_> = members(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|m| (m.username, m.guild_rank))
            .collect();
        assert_eq!(
            ranks,
            vec![
                ("Leader".to_string(), Rank::Leader),
                ("Officer".to_string(), Rank::Officer),
                ("Raider".to_string(), Rank::Officer),
            ]
        );
    }
}
