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
use axum::routing::{get, post};
use axum::{Json, Router};
use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use super::db::LevelChange;
use super::{CharacterError, external};
use crate::extract::{Admin, SameOrigin};
use crate::users::UserId;
use crate::{Result, State};

/// The guild's name in game ("BL"). The addon records only this guild's roster and the site takes
/// only this guild's: WoW gives addons a guild's name, not an id.
pub const GUILD_NAME: &str = "BL";

/// The most members a roster may name (the game's guild cap is 1,000).
const MAX_MEMBERS: usize = 1_000;

#[derive(Debug, serde::Deserialize)]
pub struct RosterInput {
    /// The guild the addon saw it in, which must be [`GUILD_NAME`].
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
    /// A hidden character from a Battle.net listing brought out, rather than one made.
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
    if input.guild.trim() != GUILD_NAME {
        return external(CharacterError::WrongGuild);
    }
    if input.members.len() > MAX_MEMBERS {
        return external(CharacterError::RosterTooLong);
    }
    if input.members.iter().any(|m| !(1..=60).contains(&m.level)) {
        return external(CharacterError::InvalidLevel);
    }
    let mut tx = pool.begin().await?;
    let mut matched = 0;
    let mut unmatched = Vec::new();
    let mut added = Vec::new();
    let mut changes: Vec<LevelChange> = Vec::new();
    for member in &input.members {
        let battlenet_id = member.guid.as_deref().and_then(battlenet_id);
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
            added.push(AddedMember {
                character_id: made.character_id,
                first_name: first.to_string(),
                last_name: last.to_string(),
                class: class.to_string(),
                revealed: made.revealed,
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
        input.members.len() as i32,
        matched as i32,
        serde_json::to_value(&unmatched).context("listing the unmatched members")?,
        &changes,
        &added
            .iter()
            .map(|a| super::db::Added {
                character_id: a.character_id,
                revealed: a.revealed,
            })
            .collect::<Vec<_>>(),
    )
    .await?;
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
            guild: GUILD_NAME.into(),
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
    async fn adds_strangers_and_brings_out_hidden_ones(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Officer).await;
        let admin = member(&pool, "Admin", Rank::Leader).await;
        // A hidden character from Ey's Battle.net listing.
        let hidden = sqlx::query_scalar!(
            "INSERT INTO characters (user_id, first_name, last_name, class, level, is_hidden,
                                     battlenet_id)
             VALUES ($1, 'Eyl', 'Shm', 'shaman', 10, true, 5479164) RETURNING id",
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
        assert_eq!(result.matched, 0);
        assert_eq!(result.added.len(), 2);
        assert_eq!(result.unmatched.len(), 1, "a class the site does not know");

        let revealed = &result.added[0];
        assert_eq!((revealed.character_id, revealed.revealed), (hidden, true));
        let shown = crate::characters::find(&pool, hidden)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(shown.level, 12);
        assert_eq!(shown.user_id, Some(ey.id), "it keeps its player");

        let made = &result.added[1];
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

        // Undoing removes the one it made, and keeps the one it brought out.
        let undone = undo(&pool, result.upload_id, admin.id).await.unwrap();
        assert_eq!(undone.removed, 1);
        assert!(
            crate::characters::find(&pool, made.character_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            crate::characters::find(&pool, hidden)
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
