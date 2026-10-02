//! Raids and loot, ported from the old site's loot tracker. A raid is one night in a zone: the
//! characters who came (at most the zone's size) and the loot they won, boss by boss. Bosses and
//! items are entered by officers as the guild meets them, since Forever's raids are new and there
//! is no database of their encounters to load.
//!
//! Everyone signed in reads; officers write.

pub mod api;
pub mod catalog;
mod db;

use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use crate::users::UserId;
use crate::{Error, Problem, Result};

pub use api::router;

#[derive(Debug, thiserror::Error, Problem)]
pub enum RaidError {
    #[error("unknown zone")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Unknown Zone",
        detail = "That is not a WoW: Forever raid."
    )]
    UnknownZone,
    #[error("invalid title")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Title",
        detail = "A raid's title is at most 64 characters."
    )]
    InvalidTitle,
    #[error("raid not found")]
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such raid.")]
    RaidNotFound,
    #[error("raid full")]
    #[problem(
        status = CONFLICT,
        title = "Raid Full",
        detail = "The raid already has as many characters as its zone holds."
    )]
    RaidFull,
    #[error("zone too small")]
    #[problem(
        status = CONFLICT,
        title = "Zone Too Small",
        detail = "More characters came to this raid than that zone holds."
    )]
    ZoneTooSmall,
    #[error("zone has loot")]
    #[problem(
        status = CONFLICT,
        title = "Raid Has Loot",
        detail = "Loot from this raid's bosses is recorded, so its zone cannot change."
    )]
    ZoneHasLoot,
    #[error("unknown character")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Unknown Character",
        detail = "No such character."
    )]
    UnknownCharacter,
    #[error("invalid boss name")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Name",
        detail = "A boss's name is 1 to 64 characters."
    )]
    InvalidBossName,
    #[error("boss taken")]
    #[problem(
        status = CONFLICT,
        title = "Boss Exists",
        detail = "That zone already has a boss of that name."
    )]
    BossTaken,
    #[error("boss not found")]
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such boss.")]
    BossNotFound,
    #[error("boss has loot")]
    #[problem(
        status = CONFLICT,
        title = "Boss Has Loot",
        detail = "Loot from this boss is recorded; remove that first."
    )]
    BossHasLoot,
    #[error("boss not in zone")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Wrong Zone",
        detail = "That boss is not in this raid's zone."
    )]
    BossNotInZone,
    #[error("invalid item")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Item",
        detail = "An item needs a name of 1 to 128 characters, a quality, and, if given, a \
                  positive item id."
    )]
    InvalidItem,
    #[error("item name taken")]
    #[problem(
        status = CONFLICT,
        title = "Item Exists",
        detail = "An item of that name already exists."
    )]
    ItemNameTaken,
    #[error("item id taken")]
    #[problem(
        status = CONFLICT,
        title = "Item Id Taken",
        detail = "Another item already has that item id."
    )]
    ItemIdTaken,
    #[error("item not found")]
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such item.")]
    ItemNotFound,
    #[error("item has loot")]
    #[problem(
        status = CONFLICT,
        title = "Item Has Loot",
        detail = "This item has been won; remove that loot first."
    )]
    ItemHasLoot,
    #[error("loot not found")]
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such loot.")]
    LootNotFound,
}

fn external<T>(error: RaidError) -> Result<T, RaidError> {
    Err(Error::External(error))
}

fn classify(error: sqlx::Error) -> Error<RaidError> {
    crate::error::classify_db_error(error, |constraint| match constraint {
        "raid_attendees_within_size_check" => Some(RaidError::RaidFull),
        "raids_within_size_check" => Some(RaidError::ZoneTooSmall),
        "raids_zone_loot_consistent_check" => Some(RaidError::ZoneHasLoot),
        "raid_attendees_character_id_fkey" | "loot_character_id_fkey" => {
            Some(RaidError::UnknownCharacter)
        }
        "bosses_zone_name_key" => Some(RaidError::BossTaken),
        "loot_boss_id_fkey" => Some(RaidError::BossNotFound),
        "loot_boss_zone_consistent_check" => Some(RaidError::BossNotInZone),
        "items_name_normalized_key" => Some(RaidError::ItemNameTaken),
        "items_game_item_id_key" => Some(RaidError::ItemIdTaken),
        "loot_item_id_fkey" => Some(RaidError::ItemNotFound),
        _ => None,
    })
}

fn zone(slug: &str) -> Result<&'static catalog::Zone, RaidError> {
    catalog::find(slug.trim()).ok_or(Error::External(RaidError::UnknownZone))
}

/* --- bosses --- */

#[derive(Debug, serde::Serialize)]
pub struct Boss {
    pub id: i64,
    pub zone: String,
    pub name: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct BossInput {
    pub zone: String,
    pub name: String,
}

fn boss_name(name: &str) -> Result<&str, RaidError> {
    let name = name.trim();
    if (1..=64).contains(&name.chars().count()) {
        Ok(name)
    } else {
        external(RaidError::InvalidBossName)
    }
}

/// Every boss, in the order they were added.
pub async fn bosses(pool: &PgPool) -> sqlx::Result<Vec<Boss>> {
    db::list_bosses(pool).await
}

pub async fn create_boss(pool: &PgPool, input: &BossInput) -> Result<Boss, RaidError> {
    let zone = zone(&input.zone)?;
    db::insert_boss(pool, zone.slug, boss_name(&input.name)?)
        .await
        .map_err(classify)
}

/// Renames a boss. Its zone stays: its loot was won there.
pub async fn rename_boss(pool: &PgPool, id: i64, name: &str) -> Result<Boss, RaidError> {
    db::rename_boss(pool, id, boss_name(name)?)
        .await
        .map_err(classify)?
        .ok_or(Error::External(RaidError::BossNotFound))
}

pub async fn delete_boss(pool: &PgPool, id: i64) -> Result<(), RaidError> {
    match db::delete_boss_without_loot(pool, id).await? {
        db::Deleted::Yes => Ok(()),
        db::Deleted::InUse => external(RaidError::BossHasLoot),
        db::Deleted::Missing => external(RaidError::BossNotFound),
    }
}

/// How often a boss dropped an item: of the raids that recorded loot from it, how many had it.
#[derive(Debug, serde::Serialize)]
pub struct Drop {
    pub item_id: i64,
    pub item_name: String,
    pub item_quality: String,
    pub game_item_id: Option<i32>,
    pub count: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct BossDetail {
    pub boss: Boss,
    /// Raids that recorded loot from it: the kills the site knows of.
    pub kills: i64,
    /// Most dropped first.
    pub drops: Vec<Drop>,
    pub loot: Vec<LootEntry>,
}

pub async fn boss_detail(pool: &PgPool, id: i64) -> Result<BossDetail, RaidError> {
    let Some(boss) = db::find_boss(pool, id).await? else {
        return external(RaidError::BossNotFound);
    };
    Ok(BossDetail {
        kills: db::boss_kills(pool, id).await?,
        drops: db::boss_drops(pool, id).await?,
        loot: db::loot(
            pool,
            &LootFilter {
                boss_id: Some(id),
                ..LootFilter::default()
            },
        )
        .await?,
        boss,
    })
}

/* --- items --- */

pub const QUALITIES: &[&str] = &["poor", "common", "uncommon", "rare", "epic", "legendary"];

#[derive(Debug, serde::Serialize)]
pub struct Item {
    pub id: i64,
    pub name: String,
    pub quality: String,
    /// The game's own item id, once officers know it.
    pub game_item_id: Option<i32>,
    /// How many times it has been won.
    pub drops: i64,
}

#[derive(Debug, serde::Deserialize)]
pub struct ItemInput {
    pub name: String,
    pub quality: String,
    #[serde(default)]
    pub game_item_id: Option<i32>,
}

struct ValidItem<'a> {
    name: &'a str,
    quality: &'static str,
    game_item_id: Option<i32>,
}

fn validate_item(input: &ItemInput) -> Result<ValidItem<'_>, RaidError> {
    let name = input.name.trim();
    let quality = QUALITIES.iter().find(|q| **q == input.quality.trim());
    match quality {
        Some(quality)
            if (1..=128).contains(&name.chars().count())
                && input.game_item_id.is_none_or(|id| id > 0) =>
        {
            Ok(ValidItem {
                name,
                quality,
                game_item_id: input.game_item_id,
            })
        }
        _ => external(RaidError::InvalidItem),
    }
}

/// Every item, by name.
pub async fn items(pool: &PgPool) -> sqlx::Result<Vec<Item>> {
    db::list_items(pool).await
}

pub async fn create_item(pool: &PgPool, input: &ItemInput) -> Result<Item, RaidError> {
    let valid = validate_item(input)?;
    let mut conn = pool.acquire().await?;
    let id = db::insert_item(&mut conn, valid.name, valid.quality, valid.game_item_id)
        .await
        .map_err(classify)?;
    Ok(db::find_item(pool, id)
        .await?
        .expect("the item was just created"))
}

pub async fn update_item(pool: &PgPool, id: i64, input: &ItemInput) -> Result<Item, RaidError> {
    let valid = validate_item(input)?;
    if !db::update_item(pool, id, valid.name, valid.quality, valid.game_item_id)
        .await
        .map_err(classify)?
    {
        return external(RaidError::ItemNotFound);
    }
    Ok(db::find_item(pool, id)
        .await?
        .expect("the item was just updated"))
}

pub async fn delete_item(pool: &PgPool, id: i64) -> Result<(), RaidError> {
    match db::delete_item_without_loot(pool, id).await? {
        db::Deleted::Yes => Ok(()),
        db::Deleted::InUse => external(RaidError::ItemHasLoot),
        db::Deleted::Missing => external(RaidError::ItemNotFound),
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ItemDetail {
    pub item: Item,
    /// Every time it was won, latest first.
    pub loot: Vec<LootEntry>,
}

pub async fn item_detail(pool: &PgPool, id: i64) -> Result<ItemDetail, RaidError> {
    let Some(item) = db::find_item(pool, id).await? else {
        return external(RaidError::ItemNotFound);
    };
    let loot = db::loot(
        pool,
        &LootFilter {
            item_id: Some(id),
            ..LootFilter::default()
        },
    )
    .await?;
    Ok(ItemDetail { item, loot })
}

/* --- raids --- */

#[derive(Debug, serde::Serialize)]
pub struct Raid {
    pub id: i64,
    pub zone: String,
    pub title: Option<String>,
    #[serde(with = "iso8601")]
    pub starts_at: OffsetDateTime,
    pub attendee_count: i64,
    pub loot_count: i64,
}

#[derive(Debug, serde::Deserialize)]
pub struct RaidInput {
    pub zone: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(with = "iso8601")]
    pub starts_at: OffsetDateTime,
}

/// The title, trimmed, with an empty one meaning none.
fn title(input: &RaidInput) -> Result<Option<&str>, RaidError> {
    match input.title.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(title) if title.chars().count() <= 64 => Ok(Some(title)),
        Some(_) => external(RaidError::InvalidTitle),
    }
}

/// Every raid, latest first.
pub async fn raids(pool: &PgPool) -> sqlx::Result<Vec<Raid>> {
    db::list_raids(pool).await
}

pub async fn create_raid(pool: &PgPool, input: &RaidInput) -> Result<Raid, RaidError> {
    let zone = zone(&input.zone)?;
    let id = db::insert_raid(pool, zone.slug, title(input)?, input.starts_at).await?;
    Ok(db::find_raid(pool, id)
        .await?
        .expect("the raid was just created"))
}

/// Changes a raid's zone, title, or time. A new zone must hold everyone who came, and a raid with
/// boss loot keeps its zone.
pub async fn update_raid(pool: &PgPool, id: i64, input: &RaidInput) -> Result<Raid, RaidError> {
    let zone = zone(&input.zone)?;
    if !db::update_raid(pool, id, zone.slug, title(input)?, input.starts_at)
        .await
        .map_err(classify)?
    {
        return external(RaidError::RaidNotFound);
    }
    Ok(db::find_raid(pool, id)
        .await?
        .expect("the raid was just updated"))
}

/// Deletes a raid with its attendance and loot.
pub async fn delete_raid(pool: &PgPool, id: i64) -> Result<(), RaidError> {
    if db::delete_raid(pool, id).await? {
        Ok(())
    } else {
        external(RaidError::RaidNotFound)
    }
}

/// A character who came to a raid.
#[derive(Debug, serde::Serialize)]
pub struct Attendee {
    pub character_id: i64,
    pub user_id: Option<UserId>,
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    pub is_main: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct RaidDetail {
    pub raid: Raid,
    /// By class, then name.
    pub attendees: Vec<Attendee>,
    /// By boss (in the order they were added, trash last), then item.
    pub loot: Vec<LootEntry>,
}

pub async fn raid_detail(pool: &PgPool, id: i64) -> Result<RaidDetail, RaidError> {
    let Some(raid) = db::find_raid(pool, id).await? else {
        return external(RaidError::RaidNotFound);
    };
    let mut loot = db::loot(
        pool,
        &LootFilter {
            raid_id: Some(id),
            ..LootFilter::default()
        },
    )
    .await?;
    loot.sort_by(|a, b| {
        (a.boss_id.is_none(), a.boss_id, &a.item_name).cmp(&(
            b.boss_id.is_none(),
            b.boss_id,
            &b.item_name,
        ))
    });
    Ok(RaidDetail {
        attendees: db::attendees(pool, id).await?,
        loot,
        raid,
    })
}

/// Adds characters to a raid; ones already there are skipped. All or nothing: a raid that would
/// overflow its zone takes none of them.
pub async fn add_attendees(
    pool: &PgPool,
    raid_id: i64,
    character_ids: &[i64],
) -> Result<Vec<Attendee>, RaidError> {
    let mut tx = pool.begin().await?;
    if !db::lock_raid(&mut tx, raid_id).await? {
        return external(RaidError::RaidNotFound);
    }
    for character_id in character_ids {
        db::insert_attendee(&mut tx, raid_id, *character_id)
            .await
            .map_err(classify)?;
    }
    tx.commit().await?;
    Ok(db::attendees(pool, raid_id).await?)
}

/// Takes a character off a raid. Loot they won there stays theirs.
pub async fn remove_attendee(
    pool: &PgPool,
    raid_id: i64,
    character_id: i64,
) -> Result<(), RaidError> {
    if db::delete_attendee(pool, raid_id, character_id).await? {
        Ok(())
    } else {
        external(RaidError::UnknownCharacter)
    }
}

/// How many raids the character came to.
pub async fn raids_attended(pool: &PgPool, character_id: i64) -> sqlx::Result<i64> {
    db::raids_attended(pool, character_id).await
}

/* --- loot --- */

/// One item won: in which raid, from which boss (none for trash), by whom (none when nobody took
/// it: disenchanted, or banked).
#[derive(Debug, serde::Serialize)]
pub struct LootEntry {
    pub id: i64,
    pub raid_id: i64,
    pub zone: String,
    pub raid_title: Option<String>,
    #[serde(with = "iso8601")]
    pub raid_starts_at: OffsetDateTime,
    pub boss_id: Option<i64>,
    pub boss_name: Option<String>,
    pub item_id: i64,
    pub item_name: String,
    pub item_quality: String,
    pub game_item_id: Option<i32>,
    pub character_id: Option<i64>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub class: Option<String>,
}

/// Which loot to list; every field narrows it, and an absent one matches anything.
#[derive(Debug, Default, serde::Deserialize)]
pub struct LootFilter {
    pub raid_id: Option<i64>,
    pub boss_id: Option<i64>,
    pub item_id: Option<i64>,
    pub character_id: Option<i64>,
    pub zone: Option<String>,
    pub class: Option<String>,
    pub quality: Option<String>,
    /// At most this many, latest first (default and ceiling [`LOOT_LIMIT`]).
    pub limit: Option<i64>,
}

pub const LOOT_LIMIT: i64 = 500;

pub async fn loot(pool: &PgPool, filter: &LootFilter) -> sqlx::Result<Vec<LootEntry>> {
    db::loot(pool, filter).await
}

pub async fn loot_for_character(pool: &PgPool, character_id: i64) -> sqlx::Result<Vec<LootEntry>> {
    db::loot(
        pool,
        &LootFilter {
            character_id: Some(character_id),
            ..LootFilter::default()
        },
    )
    .await
}

/// Loot as an officer records it. The item is an existing one (`item_id`), or named: an item of
/// that name is reused, else created with the given quality (epic when omitted) and id.
#[derive(Debug, serde::Deserialize)]
pub struct LootInput {
    #[serde(default)]
    pub boss_id: Option<i64>,
    #[serde(default)]
    pub character_id: Option<i64>,
    #[serde(default)]
    pub item_id: Option<i64>,
    #[serde(default)]
    pub item_name: Option<String>,
    #[serde(default)]
    pub item_quality: Option<String>,
    #[serde(default)]
    pub game_item_id: Option<i32>,
}

pub async fn record_loot(
    pool: &PgPool,
    raid_id: i64,
    input: &LootInput,
) -> Result<LootEntry, RaidError> {
    let mut tx = pool.begin().await?;
    if !db::lock_raid(&mut tx, raid_id).await? {
        return external(RaidError::RaidNotFound);
    }
    let item_id = match (input.item_id, input.item_name.as_deref()) {
        (Some(id), _) => id,
        (None, Some(name)) => {
            let item = ItemInput {
                name: name.to_string(),
                quality: input.item_quality.clone().unwrap_or_else(|| "epic".into()),
                game_item_id: input.game_item_id,
            };
            let valid = validate_item(&item)?;
            match db::find_item_id_by_name(&mut tx, valid.name).await? {
                Some(id) => id,
                None => db::insert_item(&mut tx, valid.name, valid.quality, valid.game_item_id)
                    .await
                    .map_err(classify)?,
            }
        }
        (None, None) => return external(RaidError::InvalidItem),
    };
    let id = db::insert_loot(&mut tx, raid_id, input.boss_id, item_id, input.character_id)
        .await
        .map_err(classify)?;
    tx.commit().await?;
    Ok(db::find_loot(pool, id)
        .await?
        .expect("the loot was just recorded"))
}

/// What can change about recorded loot: who won it and which boss dropped it. The item is fixed;
/// recording the wrong one is a delete and a new entry.
#[derive(Debug, serde::Deserialize)]
pub struct LootUpdate {
    #[serde(default)]
    pub boss_id: Option<i64>,
    #[serde(default)]
    pub character_id: Option<i64>,
}

pub async fn update_loot(
    pool: &PgPool,
    id: i64,
    input: &LootUpdate,
) -> Result<LootEntry, RaidError> {
    if !db::update_loot(pool, id, input.boss_id, input.character_id)
        .await
        .map_err(classify)?
    {
        return external(RaidError::LootNotFound);
    }
    Ok(db::find_loot(pool, id)
        .await?
        .expect("the loot was just updated"))
}

pub async fn delete_loot(pool: &PgPool, id: i64) -> Result<(), RaidError> {
    if db::delete_loot(pool, id).await? {
        Ok(())
    } else {
        external(RaidError::LootNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characters::tests::input as character;
    use crate::guild::{Rank, tests::member};

    fn raid(zone: &str) -> RaidInput {
        RaidInput {
            zone: zone.into(),
            title: None,
            starts_at: OffsetDateTime::now_utc(),
        }
    }

    fn named(item: &str, boss_id: Option<i64>, character_id: Option<i64>) -> LootInput {
        LootInput {
            boss_id,
            character_id,
            item_id: None,
            item_name: Some(item.into()),
            item_quality: None,
            game_item_id: None,
        }
    }

    /// `count` characters of a fresh member, for filling raids.
    async fn characters(pool: &PgPool, count: usize) -> Vec<i64> {
        let officer = member(pool, "Officer", Rank::Officer).await;
        let mut ids = Vec::new();
        for i in 0..count {
            // Letters only: a two-letter suffix per character.
            let suffix: String = [b'a' + (i / 26) as u8, b'a' + (i % 26) as u8]
                .iter()
                .map(|b| *b as char)
                .collect();
            let mut input = character("Raider", &format!("Num{suffix}"), "warrior", false);
            input.user_id = Some(None);
            ids.push(
                crate::characters::create(pool, &officer, &input)
                    .await
                    .unwrap()
                    .id,
            );
        }
        ids
    }

    #[sqlx::test]
    async fn a_raid_holds_at_most_its_zones_size(pool: PgPool) {
        let ids = characters(&pool, 21).await;
        let deeps = create_raid(&pool, &raid("barrow-deeps")).await.unwrap();

        // Eleven into a ten-player raid: refused, and none of them taken.
        assert!(matches!(
            add_attendees(&pool, deeps.id, &ids[..11]).await,
            Err(Error::External(RaidError::RaidFull))
        ));
        assert!(
            raid_detail(&pool, deeps.id)
                .await
                .unwrap()
                .attendees
                .is_empty()
        );

        // Ten fit, and adding one of them again is a no-op.
        assert_eq!(
            add_attendees(&pool, deeps.id, &ids[..10])
                .await
                .unwrap()
                .len(),
            10
        );
        assert_eq!(
            add_attendees(&pool, deeps.id, &ids[..1])
                .await
                .unwrap()
                .len(),
            10
        );

        // Hyjal Summit holds twenty.
        let hyjal = create_raid(&pool, &raid("hyjal-summit")).await.unwrap();
        add_attendees(&pool, hyjal.id, &ids[..20]).await.unwrap();
        assert!(matches!(
            add_attendees(&pool, hyjal.id, &ids[20..]).await,
            Err(Error::External(RaidError::RaidFull))
        ));
        // ...so it cannot move to the Barrow Deeps; Onyxia's Lair is fine.
        assert!(matches!(
            update_raid(&pool, hyjal.id, &raid("barrow-deeps")).await,
            Err(Error::External(RaidError::ZoneTooSmall))
        ));
        let moved = update_raid(&pool, hyjal.id, &raid("onyxias-lair"))
            .await
            .unwrap();
        assert_eq!(
            (moved.zone.as_str(), moved.attendee_count),
            ("onyxias-lair", 20)
        );

        assert!(matches!(
            create_raid(&pool, &raid("molten-core")).await,
            Err(Error::External(RaidError::UnknownZone))
        ));
        assert_eq!(raids_attended(&pool, ids[0]).await.unwrap(), 2);
    }

    #[sqlx::test]
    async fn loot_is_recorded_by_boss_and_counted(pool: PgPool) {
        let ids = characters(&pool, 2).await;
        let onyxia = bosses(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|b| b.name == "Onyxia")
            .expect("Onyxia is seeded");
        let deeps_boss = create_boss(
            &pool,
            &BossInput {
                zone: "barrow-deeps".into(),
                name: " Barrow King ".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(deeps_boss.name, "Barrow King");

        let first = create_raid(&pool, &raid("onyxias-lair")).await.unwrap();
        let second = create_raid(&pool, &raid("onyxias-lair")).await.unwrap();

        // A named item is created on first sight and reused (whatever the capitals) after.
        let won = record_loot(
            &pool,
            first.id,
            &named("Head of Onyxia", Some(onyxia.id), Some(ids[0])),
        )
        .await
        .unwrap();
        assert_eq!(won.item_quality, "epic");
        assert_eq!(won.first_name.as_deref(), Some("Raider"));
        let again = record_loot(
            &pool,
            second.id,
            &named("head of onyxia", Some(onyxia.id), None),
        )
        .await
        .unwrap();
        assert_eq!(again.item_id, won.item_id);
        record_loot(
            &pool,
            second.id,
            &named("Onyxia Hide Backpack", None, Some(ids[1])),
        )
        .await
        .unwrap();

        // Another zone's boss cannot drop loot here.
        assert!(matches!(
            record_loot(
                &pool,
                first.id,
                &named("Barrow Crown", Some(deeps_boss.id), None)
            )
            .await,
            Err(Error::External(RaidError::BossNotInZone))
        ));
        // A raid with boss loot keeps its zone.
        assert!(matches!(
            update_raid(&pool, first.id, &raid("hyjal-summit")).await,
            Err(Error::External(RaidError::ZoneHasLoot))
        ));

        let detail = boss_detail(&pool, onyxia.id).await.unwrap();
        assert_eq!(detail.kills, 2);
        assert_eq!(detail.drops.len(), 1);
        assert_eq!(detail.drops[0].count, 2);

        // Trash loot sorts last on the raid.
        let raid_loot = raid_detail(&pool, second.id).await.unwrap().loot;
        assert_eq!(raid_loot.last().unwrap().boss_id, None);

        // Filters narrow the history.
        let filter = |class: &str| LootFilter {
            class: Some(class.into()),
            ..LootFilter::default()
        };
        assert_eq!(loot(&pool, &filter("warrior")).await.unwrap().len(), 2);
        assert_eq!(loot(&pool, &filter("mage")).await.unwrap().len(), 0);
        assert_eq!(loot_for_character(&pool, ids[0]).await.unwrap().len(), 1);

        // Reassigning, then deleting.
        let given = update_loot(
            &pool,
            again.id,
            &LootUpdate {
                boss_id: Some(onyxia.id),
                character_id: Some(ids[1]),
            },
        )
        .await
        .unwrap();
        assert_eq!(given.character_id, Some(ids[1]));
        assert!(matches!(
            delete_boss(&pool, onyxia.id).await,
            Err(Error::External(RaidError::BossHasLoot))
        ));
        assert!(matches!(
            delete_item(&pool, won.item_id).await,
            Err(Error::External(RaidError::ItemHasLoot))
        ));
        delete_loot(&pool, again.id).await.unwrap();
        delete_boss(&pool, deeps_boss.id).await.unwrap();

        let items = items(&pool).await.unwrap();
        assert_eq!(
            items
                .iter()
                .map(|i| (i.name.as_str(), i.drops))
                .collect::<Vec<_>>(),
            vec![("Head of Onyxia", 1), ("Onyxia Hide Backpack", 1)]
        );
        // The character that won loot keeps its history.
        let officer = member(&pool, "Leader", Rank::Leader).await;
        assert!(matches!(
            crate::characters::delete(&pool, &officer, ids[0]).await,
            Err(Error::External(
                crate::characters::CharacterError::HasHistory
            ))
        ));
    }

    #[test]
    fn validates_items() {
        let item = |name: &str, quality: &str, id: Option<i32>| ItemInput {
            name: name.into(),
            quality: quality.into(),
            game_item_id: id,
        };
        assert!(validate_item(&item("Thunderfury", "legendary", Some(19019))).is_ok());
        assert!(validate_item(&item("", "epic", None)).is_err());
        assert!(validate_item(&item("Thing", "artifact", None)).is_err());
        assert!(validate_item(&item("Thing", "epic", Some(0))).is_err());
    }
}
