//! The guild roster as the addon saw it in game, sent by the Blood Legion client: each member's name and
//! level, and when the addon first saw them at 60. It raises the levels of the characters it names
//! (never lowers them: an older roster cannot undo a newer one) and keeps the earliest time each
//! reached 60, so several officers' rosters only sharpen it.
//!
//! Each upload is recorded with who sent it and every character it changed, before and after, so a
//! superuser can undo one with bad data (`undo`): the characters still as it left them get their
//! old level back, and those changed since keep theirs.

use anyhow::Context;
use axum::extract::Path;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use super::db::LevelChange;
use super::{CharacterError, external};
use crate::extract::{Admin, SameOrigin};
use crate::users::UserId;
use crate::{Result, State};

/// The most members a roster may name (the game's guild cap is 1,000).
const MAX_MEMBERS: usize = 1_000;

#[derive(Debug, serde::Deserialize)]
pub struct RosterInput {
    /// The guild the addon saw it in, which must be the one in the guild's settings
    /// ([`crate::guild::Settings::guild_name`]): WoW gives addons a guild's name, not an id.
    pub guild: String,
    pub members: Vec<RosterMember>,
}

#[derive(Debug, serde::Deserialize)]
pub struct RosterMember {
    /// As the roster shows it.
    pub name: String,
    pub level: i16,
    /// The class's English name as the roster shows it, for the officers adding an unmatched one.
    #[serde(default)]
    pub class: Option<String>,
    /// When the addon first saw it at 60.
    #[serde(default, with = "iso8601::option")]
    pub reached_60_at: Option<OffsetDateTime>,
    /// The game's GUID for it, `Player-6934-005096AC`: its last part is the character's Battle.net
    /// id in hex, which it is matched by before its name.
    #[serde(default)]
    pub guid: Option<String>,
    /// Its rank in the guild by name, as the roster shows it ("Officer").
    #[serde(default)]
    pub rank: Option<String>,
    /// That rank's number: 0 is the guild master, then down the ranks.
    #[serde(default)]
    pub rank_index: Option<i16>,
}

/// The Battle.net character id in a game GUID (`Player-<realm>-<id in hex>`).
fn battlenet_id(guid: &str) -> Option<i64> {
    let mut parts = guid.split('-');
    let (Some("Player"), Some(_realm), Some(id), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    i64::from_str_radix(id, 16).ok().filter(|id| *id > 0)
}

#[derive(Debug, serde::Serialize)]
pub struct RosterResult {
    /// The upload as recorded, for undoing it.
    pub upload_id: i64,
    /// How many characters it matched.
    pub matched: usize,
    /// How many of those it changed.
    pub changed: usize,
    /// The members no character on the site matched, added as characters.
    pub added: Vec<AddedMember>,
    /// The members it could not add (a name or class the site does not take), for an officer.
    pub unmatched: Vec<UnmatchedMember>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct AddedMember {
    pub character_id: i64,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    /// A hidden character from a Battle.net listing brought out, rather than one made: only uploads
    /// from before characters stopped being hidden did.
    pub revealed: bool,
}

/// A roster member as a new character: its two names (`First Last`, as Forever's roster shows
/// them) and its class's slug. `None` for a name or class the site does not take.
fn as_character(member: &RosterMember) -> Option<(&str, &str, &'static str)> {
    let name = member.name.trim();
    let name = name.split_once('-').map_or(name, |(name, _realm)| name);
    let (first, last) = name.split_once(' ')?;
    if !super::is_valid_name(first) || !super::is_valid_name(last) {
        return None;
    }
    let class = crate::wow::catalog::find(&member.class.as_deref()?.trim().to_lowercase())?;
    Some((first, last, class.slug))
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnmatchedMember {
    pub name: String,
    pub level: i16,
    pub class: Option<String>,
}

/// How a roster name is matched against `characters.name_normalized` (lowercase `first last`, as
/// Forever's roster shows names: "Eyl Gg"): trimmed and lowercased, with any `-Realm` suffix
/// dropped. A name without a space also matches a character whose two names run together.
fn normalize(name: &str) -> String {
    let name = name.trim();
    let name = name.split_once('-').map_or(name, |(name, _realm)| name);
    name.to_lowercase()
}

/// Applies the roster `by` sent: raises levels and records reaching 60, by Battle.net id (from the
/// GUID) or else by name, and records the upload.
pub async fn apply(
    pool: &PgPool,
    by: UserId,
    input: &RosterInput,
) -> Result<RosterResult, CharacterError> {
    let guild = crate::guild::settings(pool).await?.guild_name;
    if input.guild.trim() != guild {
        return external(CharacterError::WrongGuild);
    }
    if input.members.len() > MAX_MEMBERS {
        return external(CharacterError::RosterTooLong);
    }
    if input.members.iter().any(|m| !(1..=60).contains(&m.level)) {
        return external(CharacterError::InvalidLevel);
    }
    // Every member's rank (the game has ten, 0 the guild master): an older addon sends none.
    if input.members.iter().any(|m| {
        m.rank.as_deref().is_none_or(|rank| rank.trim().is_empty())
            || !m.rank_index.is_some_and(|index| (0..=9).contains(&index))
    }) {
        return external(CharacterError::RosterWithoutRanks);
    }
    let mut tx = pool.begin().await?;
    let mut matched = 0;
    let mut unmatched = Vec::new();
    let mut added = Vec::new();
    let mut changes: Vec<LevelChange> = Vec::new();
    // Every character it named, matched or added: the guild as the game has it now.
    let mut named: Vec<super::db::Named> = Vec::new();
    for member in &input.members {
        let battlenet_id = member.guid.as_deref().and_then(battlenet_id);
        // Checked above: every member has both.
        let rank = member.rank_index.unwrap_or_default();
        let rank_name = member
            .rank
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        let named_as = |character_id| super::db::Named {
            character_id,
            rank,
            rank_name: rank_name.clone(),
        };
        let found = super::db::raise_level(
            &mut tx,
            battlenet_id,
            &normalize(&member.name),
            member.level,
            member.reached_60_at,
        )
        .await?;
        if !found.is_empty() {
            matched += 1;
            named.extend(found.iter().map(|change| named_as(change.character_id)));
            // A character named twice in one roster: its first state is the one to go back to.
            for change in found {
                match changes
                    .iter_mut()
                    .find(|c| c.character_id == change.character_id)
                {
                    Some(earlier) => {
                        earlier.new_level = change.new_level;
                        earlier.new_reached_60_at = change.new_reached_60_at;
                    }
                    None => changes.push(change),
                }
            }
        } else if let Some((first, last, class)) = as_character(member) {
            let made = super::db::add_from_roster(
                &mut tx,
                battlenet_id,
                first,
                last,
                class,
                member.level,
                member.reached_60_at,
            )
            .await?;
            named.push(named_as(made.character_id));
            added.push(AddedMember {
                character_id: made.character_id,
                first_name: first.to_string(),
                last_name: last.to_string(),
                class: class.to_string(),
                revealed: false,
            });
        } else {
            unmatched.push(UnmatchedMember {
                name: member.name.trim().to_string(),
                level: member.level,
                class: member.class.clone(),
            });
        }
    }
    changes.retain(|c| (c.old_level, c.old_reached_60_at) != (c.new_level, c.new_reached_60_at));
    let upload_id = super::db::record_upload(
        &mut tx,
        by,
        &guild,
        input.members.len() as i32,
        matched as i32,
        serde_json::to_value(&unmatched).context("listing the unmatched members")?,
        &named,
        &changes,
        &added
            .iter()
            .map(|a| super::db::Added {
                character_id: a.character_id,
            })
            .collect::<Vec<_>>(),
    )
    .await?;
    super::db::sync_in_guild(&mut tx).await?;
    tx.commit().await?;
    Ok(RosterResult {
        upload_id,
        matched,
        changed: changes.len(),
        added,
        unmatched,
    })
}

/// How many uploads the admin page lists.
const LISTED: i64 = 50;

/// One upload, for the admin page.
#[derive(Debug, serde::Serialize)]
pub struct Upload {
    pub id: i64,
    /// Who sent it; `None` once their account is gone.
    pub username: Option<String>,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    /// How many members it named, and matched.
    pub members: i32,
    pub matched: i32,
    pub unmatched: Vec<UnmatchedMember>,
    pub changes: Vec<Change>,
    pub added: Vec<AddedMember>,
    #[serde(with = "iso8601::option")]
    pub undone_at: Option<OffsetDateTime>,
    pub undone_by: Option<String>,
}

/// A character an upload changed.
#[derive(Debug, serde::Serialize)]
pub struct Change {
    pub character_id: i64,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    pub old_level: i16,
    #[serde(with = "iso8601::option")]
    pub old_reached_60_at: Option<OffsetDateTime>,
    pub new_level: i16,
    #[serde(with = "iso8601::option")]
    pub new_reached_60_at: Option<OffsetDateTime>,
    /// Still as the upload left it: undoing the upload puts it back.
    pub current: bool,
}

/// The latest uploads, latest first, each with what it changed.
pub async fn uploads(pool: &PgPool) -> Result<Vec<Upload>> {
    let rows = super::db::uploads(pool, LISTED).await?;
    let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
    let mut changes = super::db::changes(pool, &ids).await?;
    let mut added = super::db::added(pool, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|row| Upload {
            id: row.id,
            username: row.username,
            created_at: row.created_at,
            members: row.members,
            matched: row.matched,
            unmatched: serde_json::from_value(row.unmatched).unwrap_or_default(),
            changes: changes
                .extract_if(.., |c| c.upload_id == row.id)
                .map(|c| Change {
                    character_id: c.character_id,
                    first_name: c.first_name,
                    last_name: c.last_name,
                    class: c.class,
                    old_level: c.old_level,
                    old_reached_60_at: c.old_reached_60_at,
                    new_level: c.new_level,
                    new_reached_60_at: c.new_reached_60_at,
                    current: c.current,
                })
                .collect(),
            added: added
                .extract_if(.., |a| a.upload_id == row.id)
                .map(|a| AddedMember {
                    character_id: a.character_id,
                    first_name: a.first_name,
                    last_name: a.last_name,
                    class: a.class,
                    revealed: a.revealed,
                })
                .collect(),
            undone_at: row.undone_at,
            undone_by: row.undone_by,
        })
        .collect())
}

/// A roster member an upload could not add, for the superusers to add by hand.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct NotAdded {
    pub name: String,
    pub level: i16,
    pub class: Option<String>,
    /// When the latest upload naming it was sent.
    #[serde(with = "iso8601")]
    pub seen_at: OffsetDateTime,
}

/// The members uploads could not add that are still not on the site, by name.
pub async fn not_added(pool: &PgPool) -> Result<Vec<NotAdded>> {
    Ok(super::db::not_added(pool)
        .await?
        .into_iter()
        .map(|row| NotAdded {
            name: row.name,
            level: row.level,
            class: row.class,
            seen_at: row.seen_at,
        })
        .collect())
}

/// What undoing an upload did.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Undone {
    /// Characters put back as they were before it.
    pub reverted: u64,
    /// Characters changed since (by a later upload or an officer), left as they are.
    pub kept: u64,
    /// Characters it made, removed (those still nobody's and in no raid or loot).
    pub removed: u64,
}

/// Undoes upload `id` as `by`.
pub async fn undo(pool: &PgPool, id: i64, by: UserId) -> Result<Undone, CharacterError> {
    let mut tx = pool.begin().await?;
    let Some((reverted, kept, removed)) = super::db::undo_upload(&mut tx, id, by).await? else {
        return external(CharacterError::UploadNotFound);
    };
    tx.commit().await?;
    Ok(Undone {
        reverted,
        kept,
        removed,
    })
}

/// The guild as the latest upload saw it, for the superusers.
#[derive(Debug, serde::Serialize)]
pub struct Standing {
    /// The latest upload that decides who is in the guild; `None` before any did.
    pub latest: Option<LatestUpload>,
    /// Characters on the site in the guild, and on the site at all.
    pub in_guild: i64,
    pub characters: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct LatestUpload {
    pub id: i64,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    pub username: Option<String>,
    /// How many members it named, and how many of those it matched.
    pub members: i32,
    pub matched: i32,
}

pub async fn standing(pool: &PgPool) -> Result<Standing> {
    let latest = super::db::latest_upload(pool)
        .await?
        .map(|row| LatestUpload {
            id: row.id,
            created_at: row.created_at,
            username: row.username,
            members: row.members,
            matched: row.matched,
        });
    let counts = sqlx::query!(
        r#"SELECT count(*) FILTER (WHERE in_guild(guild)) AS "in_guild!", count(*) AS "characters!"
           FROM characters"#
    )
    .fetch_one(pool)
    .await?;
    Ok(Standing {
        latest,
        in_guild: counts.in_guild,
        characters: counts.characters,
    })
}

/// `GET /api/admin/rosters/standing`: [`standing`]. Superusers only.
async fn get_standing(state: State, _admin: Admin) -> Result<Json<Standing>> {
    Ok(Json(standing(&state.pool).await?))
}

/// `GET /api/admin/rosters`: the latest roster uploads. Superusers only.
async fn list_uploads(state: State, _admin: Admin) -> Result<Json<Vec<Upload>>> {
    Ok(Json(uploads(&state.pool).await?))
}

/// `GET /api/admin/rosters/not-added`: the roster members still not on the site. Superusers only.
async fn list_not_added(state: State, _admin: Admin) -> Result<Json<Vec<NotAdded>>> {
    Ok(Json(not_added(&state.pool).await?))
}

/// `POST /api/admin/rosters/{id}/undo`: superusers only.
async fn undo_upload(
    state: State,
    _: SameOrigin,
    admin: Admin,
    Path(id): Path<i64>,
) -> Result<Json<Undone>, CharacterError> {
    Ok(Json(undo(&state.pool, id, admin.id).await?))
}

/// The superusers' routes, under `/api/admin`.
pub fn admin_router() -> Router<State> {
    Router::new()
        .route("/rosters", get(list_uploads))
        .route("/rosters/not-added", get(list_not_added))
        .route("/rosters/standing", get(get_standing))
        .route("/rosters/ranks", get(super::ranks::list_ranks))
        .route("/rosters/ranks/{rank}", put(super::ranks::set_rank))
        .route(
            "/rosters/rank-mismatches",
            get(super::ranks::list_mismatches),
        )
        .route("/rosters/{id}/undo", post(undo_upload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::characters::tests::{add, input};
    use crate::guild::Rank;
    use crate::guild::tests::member;
    use time::macros::datetime;

    fn roster(members: Vec<RosterMember>) -> RosterInput {
        RosterInput {
            guild: "BL".into(),
            members,
        }
    }

    fn seen(name: &str, level: i16, reached_60_at: Option<OffsetDateTime>) -> RosterMember {
        RosterMember {
            name: name.into(),
            level,
            class: Some("Warrior".into()),
            reached_60_at,
            guid: None,
            rank: Some("Member".into()),
            rank_index: Some(4),
        }
    }

    #[test]
    fn guids_hold_battlenet_ids() {
        assert_eq!(battlenet_id("Player-6934-005096AC"), Some(5_281_452));
        assert_eq!(battlenet_id("Player-6934"), None);
        assert_eq!(battlenet_id("Creature-0-1-2"), None);
        assert_eq!(battlenet_id("Player-6934-zz"), None);
    }

    #[sqlx::test]
    async fn the_latest_upload_says_who_is_in_the_guild(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let mine = add(&pool, &ey, input("Eyl", "Gg", "warrior", true))
            .await
            .unwrap();
        let in_guild = |name: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar!(
                    r#"SELECT in_guild(guild) AS "in_guild!" FROM characters WHERE name_normalized = $1"#,
                    name
                )
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };
        assert!(
            !mine.in_guild,
            "an officer adding it says nothing of the guild"
        );

        let first = apply(
            &pool,
            ey.id,
            &roster(vec![seen("Eyl Gg", 5, None), seen("Other Member", 3, None)]),
        )
        .await
        .unwrap();
        assert!(in_guild("eyl gg").await && in_guild("other member").await);
        let summary = standing(&pool).await.unwrap();
        assert_eq!((summary.in_guild, summary.characters), (2, 2));
        assert_eq!(summary.latest.unwrap().id, first.upload_id);

        // Other Member left the guild.
        let second = apply(&pool, ey.id, &roster(vec![seen("Eyl Gg", 6, None)]))
            .await
            .unwrap();
        assert!(in_guild("eyl gg").await && !in_guild("other member").await);

        // Undoing it goes back to the guild the one before saw.
        undo(&pool, second.upload_id, ey.id).await.unwrap();
        assert!(in_guild("other member").await);
        assert_eq!(
            standing(&pool).await.unwrap().latest.unwrap().id,
            first.upload_id
        );
    }

    fn ranked(name: &str, rank: &str, rank_index: i16) -> RosterMember {
        RosterMember {
            rank: Some(rank.into()),
            rank_index: Some(rank_index),
            ..seen(name, 10, None)
        }
    }

    #[sqlx::test]
    async fn every_member_needs_a_rank(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Officer).await;
        let unranked = RosterMember {
            rank: None,
            rank_index: None,
            ..seen("Eyl Gg", 10, None)
        };
        let too_low = ranked("Eyl Gg", "Peon", 10);
        for bad in [unranked, too_low] {
            assert!(matches!(
                apply(&pool, ey.id, &roster(vec![bad])).await,
                Err(crate::Error::External(CharacterError::RosterWithoutRanks))
            ));
        }
        // A guild without a rank cannot be stored.
        let gg = add(&pool, &ey, input("Eyl", "Gg", "warrior", true))
            .await
            .unwrap();
        assert!(
            sqlx::query!("UPDATE characters SET guild = 'BL' WHERE id = $1", gg.id)
                .execute(&pool)
                .await
                .is_err()
        );
    }

    #[sqlx::test]
    async fn ranks_in_game_point_out_site_ranks_that_differ(pool: PgPool) {
        use crate::characters::ranks;
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let zx = member(&pool, "Zx", Rank::Member).await;
        let gone = member(&pool, "Gone", Rank::Trial).await;
        add(&pool, &ey, input("Eyl", "Gg", "warrior", true))
            .await
            .unwrap();
        add(&pool, &ey, input("Eyl", "Alt", "mage", false))
            .await
            .unwrap();
        add(&pool, &zx, input("Zx", "Main", "rogue", true))
            .await
            .unwrap();
        add(&pool, &gone, input("Gone", "Away", "rogue", true))
            .await
            .unwrap();
        assert!(
            ranks::mismatches(&pool).await.unwrap().is_empty(),
            "no upload yet"
        );

        apply(
            &pool,
            ey.id,
            &roster(vec![
                ranked("Eyl Gg", "Officer", 1),
                ranked("Eyl Alt", "Officer Alt", 2),
                ranked("Zx Main", "Member", 4),
            ]),
        )
        .await
        .unwrap();
        let gg = super::super::find_by_slug(&pool, "eyl-gg")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                gg.guild.as_deref(),
                gg.guild_rank,
                gg.guild_rank_name.as_deref()
            ),
            (Some("BL"), Some(1), Some("Officer"))
        );
        let game = ranks::list(&pool).await.unwrap();
        assert_eq!(
            game.iter()
                .map(|r| (r.rank, r.name.as_deref(), r.site_rank, r.characters))
                .collect::<Vec<_>>(),
            [
                (1, Some("Officer"), Some(Rank::Officer), 1),
                (2, Some("Officer Alt"), None, 1),
                (4, Some("Member"), Some(Rank::Member), 1),
            ]
        );

        // Ey is an officer in game but a raider on the site, and Gone is in the guild no more.
        let found = ranks::mismatches(&pool).await.unwrap();
        let who: Vec<_> = found
            .iter()
            .map(|m| (m.username.as_str(), m.rank, m.expected))
            .collect();
        assert!(who.contains(&("Ey", Rank::Raider, Some(Rank::Officer))));
        assert!(who.contains(&("Gone", Rank::Trial, None)));
        assert!(!who.iter().any(|(name, ..)| *name == "Zx"), "{who:?}");

        // Officer counted as a raider, for a guild where officers raid: Ey matches.
        ranks::set(&pool, 1, Some(Rank::Raider)).await.unwrap();
        let found = ranks::mismatches(&pool).await.unwrap();
        assert!(!found.iter().any(|m| m.username == "Ey"));
        assert!(ranks::set(&pool, 7, None).await.is_err(), "no rank 7 seen");

        // Rank 1 renamed in game: the new name, and still a raider here.
        apply(
            &pool,
            ey.id,
            &roster(vec![
                ranked("Eyl Gg", "Lieutenant", 1),
                ranked("Eyl Alt", "Officer Alt", 2),
                ranked("Zx Main", "Member", 4),
            ]),
        )
        .await
        .unwrap();
        let first = &ranks::list(&pool).await.unwrap()[0];
        assert_eq!(
            (first.rank, first.name.as_deref(), first.site_rank),
            (1, Some("Lieutenant"), Some(Rank::Raider))
        );

        // Another guild's roster is turned down until the settings name it, and then decides.
        let mut other = roster(vec![ranked("Zx Main", "Peon", 3)]);
        other.guild = "Other".into();
        assert!(matches!(
            apply(&pool, ey.id, &other).await,
            Err(crate::Error::External(CharacterError::WrongGuild))
        ));
        let settings = crate::guild::settings(&pool).await.unwrap();
        crate::guild::set_settings(
            &pool,
            &crate::guild::Settings {
                guild_name: "Other".into(),
                ..settings
            },
        )
        .await
        .unwrap();
        let gg = super::super::find_by_slug(&pool, "eyl-gg")
            .await
            .unwrap()
            .unwrap();
        assert!(
            !gg.in_guild && gg.guild.as_deref() == Some("BL"),
            "no upload of Other yet: nobody is in it, and BL's stay as BL's"
        );
        apply(&pool, ey.id, &other).await.unwrap();
        let gg = super::super::find_by_slug(&pool, "eyl-gg")
            .await
            .unwrap()
            .unwrap();
        assert!(!gg.in_guild && gg.guild.is_none() && gg.guild_rank.is_none());
    }

    #[sqlx::test]
    async fn matches_by_battlenet_id_before_the_name(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let gg = add(&pool, &ey, input("Eyl", "Gg", "warrior", true))
            .await
            .unwrap();
        let other = add(&pool, &ey, input("Eyl", "Other", "warrior", false))
            .await
            .unwrap();
        sqlx::query!(
            "UPDATE characters SET battlenet_id = 5281452 WHERE id = $1",
            gg.id
        )
        .execute(&pool)
        .await
        .unwrap();
        // Renamed in game to a name the site has for another character: the id wins.
        let mut renamed = seen("Eyl Other", 13, None);
        renamed.guid = Some("Player-6934-005096AC".into());
        // An id the site does not know falls back to the name.
        let mut unknown = seen("Eyl Other", 5, None);
        unknown.guid = Some("Player-6934-00000001".into());
        let result = apply(&pool, ey.id, &roster(vec![renamed, unknown]))
            .await
            .unwrap();
        assert_eq!(result.matched, 2);
        let level = |id| {
            let pool = &pool;
            async move {
                crate::characters::find(pool, id)
                    .await
                    .unwrap()
                    .unwrap()
                    .level
            }
        };
        assert_eq!(level(gg.id).await, 13);
        assert_eq!(level(other.id).await, 5);
    }

    #[test]
    fn names_lose_their_realm_and_case() {
        assert_eq!(normalize(" Garr Bonecrush "), "garr bonecrush");
        assert_eq!(normalize("Garrbonecrush-Forever"), "garrbonecrush");
    }

    #[sqlx::test]
    async fn raises_levels_and_keeps_the_first_60(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let garr = add(&pool, &ey, input("Garr", "Bonecrush", "warrior", true))
            .await
            .unwrap();
        let lira = add(&pool, &ey, input("Lira", "Moonwell", "priest", false))
            .await
            .unwrap();

        // Matched by full name or with the names run together; a stranger is added, and one
        // without two names is listed.
        let result = apply(
            &pool,
            ey.id,
            &roster(vec![
                seen("garr bonecrush", 40, None),
                seen("LiraMoonwell-Forever", 12, None),
                seen("Random Pug", 30, None),
                seen("Mononym", 30, None),
            ]),
        )
        .await
        .unwrap();
        assert_eq!(result.matched, 2);
        assert_eq!(result.added.len(), 1);
        assert_eq!(
            (
                result.added[0].first_name.as_str(),
                result.added[0].last_name.as_str()
            ),
            ("Random", "Pug")
        );
        assert_eq!(
            result.unmatched,
            [UnmatchedMember {
                name: "Mononym".into(),
                level: 30,
                class: Some("Warrior".into())
            }]
        );
        let pool = &pool;
        let level = |id| async move { crate::characters::find(pool, id).await.unwrap().unwrap() };
        assert_eq!(level(garr.id).await.level, 40);
        assert_eq!(level(lira.id).await.level, 12);

        // An older roster never lowers a level.
        apply(pool, ey.id, &roster(vec![seen("Garr Bonecrush", 35, None)]))
            .await
            .unwrap();
        assert_eq!(level(garr.id).await.level, 40);

        // At 60: the time the addon saw it, and an earlier sighting wins over a later one.
        let later = datetime!(2026-11-20 05:00 UTC);
        let earlier = datetime!(2026-11-20 03:15 UTC);
        apply(
            pool,
            ey.id,
            &roster(vec![seen("Garr Bonecrush", 60, Some(later))]),
        )
        .await
        .unwrap();
        apply(
            pool,
            ey.id,
            &roster(vec![seen("Garr Bonecrush", 60, Some(earlier))]),
        )
        .await
        .unwrap();
        apply(
            pool,
            ey.id,
            &roster(vec![seen("Garr Bonecrush", 60, Some(later))]),
        )
        .await
        .unwrap();
        let capped = level(garr.id).await;
        assert_eq!((capped.level, capped.reached_60_at), (60, Some(earlier)));

        // At 60 without a time: now.
        apply(pool, ey.id, &roster(vec![seen("Lira Moonwell", 60, None)]))
            .await
            .unwrap();
        assert!(level(lira.id).await.reached_60_at.is_some());
    }

    #[sqlx::test]
    async fn adds_strangers_and_matches_battlenet_listed_ones(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Officer).await;
        let admin = member(&pool, "Admin", Rank::Leader).await;
        // A character from Ey's Battle.net listing, not in the guild yet.
        let listed = sqlx::query_scalar!(
            "INSERT INTO characters (user_id, first_name, last_name, class, level, battlenet_id)
             VALUES ($1, 'Eyl', 'Shm', 'shaman', 10, 5479164) RETURNING id",
            ey.id.0,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let mut shm = seen("Eyl Shm", 12, None);
        shm.class = Some("Shaman".into());
        shm.guid = Some("Player-6934-00539AFC".into());
        let mut capped = seen("Druthers Bank", 60, None);
        capped.guid = Some("Player-6800-006C36F7".into());
        let mut unknown = seen("Odd Class", 5, None);
        unknown.class = Some("Monk".into());
        let result = apply(&pool, ey.id, &roster(vec![shm, capped, unknown]))
            .await
            .unwrap();
        assert_eq!(result.matched, 1);
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.unmatched.len(), 1, "a class the site does not know");

        let shm = crate::characters::find(&pool, listed)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(shm.level, 12);
        assert!(shm.in_guild);
        assert_eq!(shm.user_id, Some(ey.id), "it keeps its player");

        let made = &result.added[0];
        assert!(!made.revealed);
        let bank = crate::characters::find(&pool, made.character_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (bank.level, bank.user_id, bank.class.as_str()),
            (60, None, "warrior")
        );
        assert!(bank.reached_60_at.is_some());
        // The guild list says neither.
        assert_eq!((bank.race.as_deref(), bank.sex), (None, None));
        assert!(bank.in_guild);

        // Everyone sees it, played or not; a pug an officer recorded stays the admin's.
        sqlx::query!(
            "INSERT INTO characters (first_name, last_name, class) VALUES ('Some', 'Pug', 'rogue')"
        )
        .execute(&pool)
        .await
        .unwrap();
        let raider = member(&pool, "Raider", Rank::Raider).await;
        let seen_by_raider = crate::characters::visible(&pool, &raider, false)
            .await
            .unwrap();
        assert!(seen_by_raider.iter().any(|c| c.id == bank.id));
        assert!(seen_by_raider.iter().all(|c| c.last_name != "Pug"));
        assert_eq!(bank.ruleset, crate::wow::Ruleset::Pvp);
        let battlenet_id = sqlx::query_scalar!(
            "SELECT battlenet_id FROM characters WHERE id = $1",
            made.character_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(battlenet_id, Some(0x006C36F7));

        // Undoing removes the one it made, and keeps the one it matched.
        let undone = undo(&pool, result.upload_id, admin.id).await.unwrap();
        assert_eq!(undone.removed, 1);
        assert!(
            crate::characters::find(&pool, made.character_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            crate::characters::find(&pool, listed)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[sqlx::test]
    async fn lists_the_members_still_not_added(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Officer).await;
        let admin = member(&pool, "Admin", Rank::Leader).await;
        apply(&pool, ey.id, &roster(vec![seen("Mononym", 5, None)]))
            .await
            .unwrap();
        let undone = apply(&pool, ey.id, &roster(vec![seen("Gone", 9, None)]))
            .await
            .unwrap();
        undo(&pool, undone.upload_id, admin.id).await.unwrap();
        apply(
            &pool,
            ey.id,
            &roster(vec![seen("mononym", 7, None), seen("Later", 3, None)]),
        )
        .await
        .unwrap();
        let listed = not_added(&pool).await.unwrap();
        let names: Vec<_> = listed.iter().map(|m| (m.name.as_str(), m.level)).collect();
        // Once by name, as last seen; an undone upload's are not listed.
        assert_eq!(names, [("Later", 3), ("mononym", 7)]);

        // Added by hand since: no longer listed.
        add(&pool, &ey, input("Lat", "Er", "mage", false))
            .await
            .unwrap();
        let listed = not_added(&pool).await.unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[sqlx::test]
    async fn only_the_guilds_roster(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Officer).await;
        let mut other = roster(vec![seen("Garr Bonecrush", 40, None)]);
        other.guild = "Some Other Guild".into();
        assert!(matches!(
            apply(&pool, ey.id, &other).await,
            Err(Error::External(CharacterError::WrongGuild))
        ));
        assert!(matches!(
            apply(
                &pool,
                ey.id,
                &roster(vec![seen("Garr Bonecrush", 61, None)])
            )
            .await,
            Err(Error::External(CharacterError::InvalidLevel))
        ));
    }

    #[sqlx::test]
    async fn records_each_upload_and_undoes_it(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Officer).await;
        let admin = member(&pool, "Admin", Rank::Leader).await;
        let garr = add(&pool, &ey, input("Garr", "Bonecrush", "warrior", true))
            .await
            .unwrap();
        let lira = add(&pool, &ey, input("Lira", "Moonwell", "priest", false))
            .await
            .unwrap();
        let pool = &pool;
        let level = |id| async move { crate::characters::find(pool, id).await.unwrap().unwrap() };

        // A good upload, then a bad one (Garr at 60 too early), then Lira up again on her own.
        let good = apply(
            pool,
            ey.id,
            &roster(vec![
                seen("Garr Bonecrush", 30, None),
                seen("Lira Moonwell", 20, None),
                seen("Random Pug", 10, None),
            ]),
        )
        .await
        .unwrap();
        assert_eq!((good.matched, good.changed), (2, 2));
        let bad = apply(
            pool,
            ey.id,
            &roster(vec![
                seen("Garr Bonecrush", 60, None),
                seen("Lira Moonwell", 25, None),
                // Unchanged: not recorded.
                seen("Garr Bonecrush", 12, None),
            ]),
        )
        .await
        .unwrap();
        assert_eq!(bad.changed, 2);
        apply(pool, ey.id, &roster(vec![seen("Lira Moonwell", 26, None)]))
            .await
            .unwrap();

        let listed = uploads(pool).await.unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[1].id, bad.upload_id);
        assert_eq!(listed[1].username.as_deref(), Some("Ey"));
        let garr_change = listed[1]
            .changes
            .iter()
            .find(|c| c.character_id == garr.id)
            .unwrap();
        assert_eq!((garr_change.old_level, garr_change.new_level), (30, 60));
        assert!(garr_change.current);
        // Lira was raised again since.
        assert!(
            !listed[1]
                .changes
                .iter()
                .find(|c| c.character_id == lira.id)
                .unwrap()
                .current
        );
        assert_eq!(listed[2].added[0].first_name, "Random");

        // Undoing the bad one puts Garr back, and leaves Lira, changed since.
        let undone = undo(pool, bad.upload_id, admin.id).await.unwrap();
        assert_eq!(
            undone,
            Undone {
                reverted: 1,
                kept: 1,
                removed: 0
            }
        );
        let garr_now = level(garr.id).await;
        assert_eq!((garr_now.level, garr_now.reached_60_at), (30, None));
        assert_eq!(level(lira.id).await.level, 26);
        let listed = uploads(pool).await.unwrap();
        assert_eq!(listed[1].undone_by.as_deref(), Some("Admin"));
        assert!(listed[1].undone_at.is_some());

        // Once only.
        assert!(matches!(
            undo(pool, bad.upload_id, admin.id).await,
            Err(Error::External(CharacterError::UploadNotFound))
        ));
    }
}
