//! Members' WoW: Forever characters. A character has a first and a last name and no realm (Forever
//! has rulesets instead, and the guild plays on one), so its full name is unique across the site.
//! Officers add and edit them, including a character no account claims (a pug who won loot); a
//! player writes only their character's notes. Each member has at most one main.

pub mod admin;
pub mod api;
mod db;
pub mod ranks;
pub mod roster;

use crate::wow::Ruleset;
use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use crate::users::{User, UserId};
use crate::{Error, Problem, Result};

pub use api::router;
pub use db::sync_in_guild;

#[derive(Debug, serde::Serialize)]
pub struct Character {
    pub id: i64,
    /// The member who plays it, or `None` for an unclaimed character.
    pub user_id: Option<UserId>,
    /// That member's username.
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    /// Its race's slug (`wow::catalog::RACES`), or `None` while nobody has recorded it.
    pub race: Option<String>,
    /// `None` while nobody has recorded it (a character a roster upload added).
    pub sex: Option<Sex>,
    pub ruleset: Ruleset,
    /// 1 to 60, as an officer last recorded it.
    pub level: i16,
    /// When it reached 60 (`None` below it): of two at 60, the earlier got there first.
    #[serde(with = "iso8601::option")]
    pub reached_60_at: Option<OffsetDateTime>,
    pub is_main: bool,
    /// Named by the guild's roster (an upload from the addon): shown and racing like a member's
    /// even while nobody plays it on the site.
    pub in_guild: bool,
    /// The guild the latest roster upload of the guild saw it in (the guild's own name), and its
    /// rank there: the game's number for it, 0 the guild master, as the armory gives it. Both or
    /// neither: `None` while no upload named it.
    pub guild: Option<String>,
    pub guild_rank: Option<i16>,
    /// That rank's name in game ("Officer"), from `guild_ranks`, when an upload has sent it.
    pub guild_rank_name: Option<String>,
    /// The specs it plays, its main (if it has one) first, then in the catalog's order.
    pub specs: sqlx::types::Json<Vec<CharacterSpec>>,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
}

/// A character as submitted, before validation.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CharacterInput {
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    /// Its race's slug; absent, `null`, or empty for none recorded.
    #[serde(default)]
    pub race: Option<String>,
    /// Absent or `null` for none recorded, like the race.
    #[serde(default)]
    pub sex: Option<Sex>,
    /// Absent keeps the ruleset it has; a new character takes the guild's.
    #[serde(default)]
    pub ruleset: Option<Ruleset>,
    /// 1 to 60; absent keeps the level it has (a new character starts at 1).
    #[serde(default)]
    pub level: Option<i16>,
    /// When it reached 60, from the player's screenshot: absent or `null` keeps the time it has,
    /// or is now when it has just reached 60. Ignored below 60, where there is none.
    #[serde(default, with = "iso8601::option")]
    pub reached_60_at: Option<OffsetDateTime>,
    #[serde(default)]
    pub is_main: bool,
    /// Who plays it: absent for the caller themselves; an officer may name any member, or send
    /// `null` for an unclaimed character.
    #[serde(default, deserialize_with = "crate::api::double_option")]
    pub user_id: Option<Option<UserId>>,
}

/// A character's sex, which picks its race's portrait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Sex {
    Female,
    Male,
}

/// A spec a character plays: any of its class's, with the notable talents it takes there (see
/// `wow::catalog` for both), and whether it is the character's main.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CharacterSpec {
    pub spec: String,
    pub talents: Vec<String>,
    pub is_main: bool,
}

/// One spec as submitted: its slug and its talents.
#[derive(Debug, serde::Deserialize)]
pub struct SpecInput {
    pub spec: String,
    #[serde(default)]
    pub talents: Vec<String>,
}

/// Every spec the character plays (replacing what it had), and which is its main, if any.
#[derive(Debug, Default, serde::Deserialize)]
pub struct SpecsInput {
    #[serde(default)]
    pub specs: Vec<SpecInput>,
    #[serde(default)]
    pub main: Option<String>,
}

/// Orders specs as characters show them: the main first, then the class catalog's order.
pub(crate) fn order_specs(class: &str, specs: &mut [CharacterSpec]) {
    let position = |slug: &str| {
        crate::wow::catalog::find(class)
            .and_then(|c| c.specs.iter().position(|s| s.slug == slug))
            .unwrap_or(usize::MAX)
    };
    specs.sort_by_key(|s| (!s.is_main, position(&s.spec)));
}

#[derive(Debug, thiserror::Error, Problem)]
pub enum CharacterError {
    #[error("invalid name")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Name",
        detail = "A first or last name is 2 to 24 letters, with no spaces or digits."
    )]
    InvalidName,
    #[error("unknown class")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Unknown Class",
        detail = "That is not a WoW: Forever class."
    )]
    UnknownClass,
    #[error("unknown race")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Unknown Race",
        detail = "That is not a WoW: Forever race."
    )]
    UnknownRace,
    #[error("invalid level")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Level",
        detail = "A level is 1 to 60."
    )]
    InvalidLevel,
    #[error("name taken")]
    #[problem(
        status = CONFLICT,
        title = "Name Taken",
        detail = "A character with that name is already on the roster."
    )]
    NameTaken,
    #[error("unknown member")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Unknown Member",
        detail = "No such member to give the character to."
    )]
    UnknownMember,
    #[error("character not found")]
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such character.")]
    NotFound,
    #[error("not your character")]
    #[problem(
        status = FORBIDDEN,
        title = "Forbidden",
        detail = "Only the character's player can write its notes."
    )]
    Forbidden,
    #[error("only officers add, edit, and remove characters")]
    #[problem(
        status = FORBIDDEN,
        title = "Forbidden",
        detail = "Only officers can add, edit, or remove characters."
    )]
    OfficersOnly,
    #[error("character has history")]
    #[problem(
        status = CONFLICT,
        title = "Character Has History",
        detail = "This character has raided or won loot, so removing it would rewrite the guild's \
                  records. Give it to another member instead, or leave it be."
    )]
    HasHistory,
    #[error("note too long")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Note Too Long",
        detail = "Notes are at most 10,000 characters."
    )]
    NoteTooLong,
    #[error("wrong guild")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Wrong Guild",
        detail = "That roster is another guild's than the one in the guild's settings: log in on a character in the guild."
    )]
    WrongGuild,
    #[error("roster too long")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Roster Too Long",
        detail = "A roster names at most 1,000 members."
    )]
    RosterTooLong,
    #[error("roster without ranks")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Roster Without Ranks",
        detail = "The roster has no guild rank for every member: update the Blood Legion addon (0.5.0 or later) and the client, then /reload in game."
    )]
    RosterWithoutRanks,
    #[error("upload not found")]
    #[problem(
        status = NOT_FOUND,
        title = "Upload Not Found",
        detail = "No such roster upload, or it was undone already."
    )]
    UploadNotFound,
    #[error("invalid spec")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Spec",
        detail = "Pick specs and talents of the character's own class."
    )]
    InvalidSpec,
}

fn external<T>(error: CharacterError) -> Result<T, CharacterError> {
    Err(Error::External(error))
}

/// Whether `value` is a valid first or last name as it is (see [`name`]).
pub(crate) fn is_valid_name(value: &str) -> bool {
    name(value).is_ok_and(|trimmed| trimmed == value)
}

/// A first or last name: 2 to 24 letters (any script), surrounding space trimmed.
fn name(value: &str) -> Result<String, CharacterError> {
    let value = value.trim();
    let length = value.chars().count();
    if (2..=24).contains(&length) && value.chars().all(char::is_alphabetic) {
        Ok(value.to_string())
    } else {
        external(CharacterError::InvalidName)
    }
}

/// A validated character, its owner resolved against the caller's rights.
struct Valid {
    first_name: String,
    last_name: String,
    class: &'static str,
    race: Option<&'static str>,
    sex: Option<Sex>,
    /// `None` keeps the ruleset it has (or is the guild's, for a new character).
    ruleset: Option<Ruleset>,
    /// `None` keeps the level it has.
    level: Option<i16>,
    /// `None` keeps the time it has (or is now, newly at 60).
    reached_60_at: Option<OffsetDateTime>,
    user_id: Option<UserId>,
    is_main: bool,
}

/// Validates `input` from `actor` (an officer); `current_owner` is the owner of the character being
/// edited (`None` when creating), which an absent `user_id` keeps.
fn validate(
    actor: &User,
    input: &CharacterInput,
    current_owner: Option<Option<UserId>>,
) -> Result<Valid, CharacterError> {
    let Some(class) = crate::wow::catalog::find(input.class.trim()) else {
        return external(CharacterError::UnknownClass);
    };
    let race = match input.race.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(slug) => match crate::wow::catalog::find_race(slug) {
            Some(race) => Some(race.slug),
            None => return external(CharacterError::UnknownRace),
        },
    };
    if input.level.is_some_and(|level| !(1..=60).contains(&level)) {
        return external(CharacterError::InvalidLevel);
    }
    let user_id = match (input.user_id, current_owner) {
        (Some(owner), _) => owner,
        (None, Some(current)) => current,
        (None, None) => Some(actor.id),
    };
    Ok(Valid {
        first_name: name(&input.first_name)?,
        last_name: name(&input.last_name)?,
        class: class.slug,
        race,
        sex: input.sex,
        ruleset: input.ruleset,
        level: input.level,
        reached_60_at: input.reached_60_at,
        // An unclaimed character is nobody's main.
        is_main: input.is_main && user_id.is_some(),
        user_id,
    })
}

fn classify(error: sqlx::Error) -> Error<CharacterError> {
    crate::error::classify_db_error(error, |constraint| match constraint {
        "characters_name_normalized_key" => Some(CharacterError::NameTaken),
        "characters_user_id_fkey" => Some(CharacterError::UnknownMember),
        _ => None,
    })
}

/// Every character, by name.
pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<Character>> {
    db::list(pool).await
}

/// The characters `actor` may see, by name. Unclaimed ones (pugs) are the admin's, except that an
/// officer who records raids and loot may ask for them (`unclaimed`) where they pick attendees;
/// one the guild's roster named is everyone's, played or not.
pub async fn visible(pool: &PgPool, actor: &User, unclaimed: bool) -> sqlx::Result<Vec<Character>> {
    let pugs = actor.is_superuser || (unclaimed && actor.is_officer());
    let mut characters = db::list(pool).await?;
    characters.retain(|c| pugs || c.user_id.is_some() || c.in_guild);
    Ok(characters)
}

pub async fn find(pool: &PgPool, id: i64) -> sqlx::Result<Option<Character>> {
    db::find(pool, id).await
}

/// The character a page's name stands for: `first-last`, as the site links it (names are letters
/// only, so the hyphen is never part of one), whatever its capitalization.
pub async fn find_by_slug(pool: &PgPool, slug: &str) -> sqlx::Result<Option<Character>> {
    let Some((first, last)) = slug.split_once('-') else {
        return Ok(None);
    };
    match db::find_id_by_name(pool, &format!("{first} {last}")).await? {
        Some(id) => db::find(pool, id).await,
        None => Ok(None),
    }
}

/// Adds a character (officers only). A member's first character becomes their main whether or
/// not they asked; declaring another main moves the title.
pub async fn create(
    pool: &PgPool,
    actor: &User,
    input: &CharacterInput,
) -> Result<Character, CharacterError> {
    if !actor.is_officer() {
        return external(CharacterError::OfficersOnly);
    }
    let mut tx = pool.begin().await?;
    let valid = validate(actor, input, None)?;
    let mut is_main = valid.is_main;
    if let Some(owner) = valid.user_id {
        is_main |= db::count_owned(&mut tx, owner).await? == 0;
        if is_main {
            db::clear_main(&mut tx, owner, None).await?;
        }
    }
    let id = db::insert(&mut tx, &valid, is_main)
        .await
        .map_err(classify)?;
    tx.commit().await?;
    Ok(db::find(pool, id)
        .await?
        .expect("the character was just created"))
}

/// Replaces a character's details, including who plays it (officers only).
pub async fn update(
    pool: &PgPool,
    actor: &User,
    id: i64,
    input: &CharacterInput,
) -> Result<Character, CharacterError> {
    if !actor.is_officer() {
        return external(CharacterError::OfficersOnly);
    }
    let mut tx = pool.begin().await?;
    let Some(current_owner) = db::lock_owner(&mut tx, id).await? else {
        return external(CharacterError::NotFound);
    };
    let valid = validate(actor, input, Some(current_owner))?;
    if let (true, Some(owner)) = (valid.is_main, valid.user_id) {
        db::clear_main(&mut tx, owner, Some(id)).await?;
    }
    // A new class has other specs and talents.
    db::clear_specs_unless_class(&mut tx, id, valid.class).await?;
    db::update(&mut tx, id, &valid).await.map_err(classify)?;
    tx.commit().await?;
    Ok(db::find(pool, id)
        .await?
        .expect("the character was just updated"))
}

/// Validates `input` against the class: each spec of the class at most once, talents of the class
/// (in the catalog's order), and a main among the specs.
fn valid_specs(
    class: &'static crate::wow::catalog::Class,
    input: &SpecsInput,
) -> Result<Vec<CharacterSpec>, CharacterError> {
    let main = input.main.as_deref().map(str::trim);
    let mut specs: Vec<CharacterSpec> = Vec::new();
    for spec in &input.specs {
        let Some(known) = class.specs.iter().find(|s| s.slug == spec.spec.trim()) else {
            return external(CharacterError::InvalidSpec);
        };
        let talents_known = spec
            .talents
            .iter()
            .all(|t| class.talents.iter().any(|known| known.slug == t));
        if specs.iter().any(|s| s.spec == known.slug) || !talents_known {
            return external(CharacterError::InvalidSpec);
        }
        specs.push(CharacterSpec {
            spec: known.slug.to_string(),
            talents: class
                .talents
                .iter()
                .filter(|t| spec.talents.iter().any(|chosen| chosen == t.slug))
                .map(|t| t.slug.to_string())
                .collect(),
            is_main: main == Some(known.slug),
        });
    }
    if main.is_some() && !specs.iter().any(|s| s.is_main) {
        return external(CharacterError::InvalidSpec);
    }
    order_specs(class.slug, &mut specs);
    Ok(specs)
}

/// Replaces the specs a character plays, with their talents and the main among them (or none):
/// officers only. A raid it was set to play a removed spec in falls back to its
/// main.
pub async fn set_specs(
    pool: &PgPool,
    actor: &User,
    id: i64,
    input: &SpecsInput,
) -> Result<Character, CharacterError> {
    if !actor.is_officer() {
        return external(CharacterError::OfficersOnly);
    }
    let Some(character) = db::find(pool, id).await? else {
        return external(CharacterError::NotFound);
    };
    let Some(class) = crate::wow::catalog::find(&character.class) else {
        return external(CharacterError::UnknownClass);
    };
    let specs = valid_specs(class, input)?;
    let mut tx = pool.begin().await?;
    db::replace_specs(&mut tx, id, &specs).await?;
    tx.commit().await?;
    Ok(db::find(pool, id).await?.expect("the character exists"))
}

/// Removes a character that has never raided or won loot (officers only).
pub async fn delete(pool: &PgPool, actor: &User, id: i64) -> Result<(), CharacterError> {
    if !actor.is_officer() {
        return external(CharacterError::OfficersOnly);
    }
    let mut tx = pool.begin().await?;
    if db::lock_owner(&mut tx, id).await?.is_none() {
        return external(CharacterError::NotFound);
    }
    if db::has_history(&mut tx, id).await? {
        return external(CharacterError::HasHistory);
    }
    db::delete(&mut tx, id).await?;
    tx.commit().await?;
    Ok(())
}

/// A character's notes, as its player wrote them.
#[derive(Debug, serde::Serialize)]
pub struct Note {
    pub character_id: i64,
    pub body: String,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
}

/// Whether `actor` may read the character's notes: its player and officers.
pub fn may_read_note(actor: &User, character: &Character) -> bool {
    character.user_id == Some(actor.id) || actor.is_officer()
}

pub async fn note(pool: &PgPool, character_id: i64) -> sqlx::Result<Option<Note>> {
    db::find_note(pool, character_id).await
}

/// Writes the character's notes (its player only); an empty body clears them.
pub async fn set_note(
    pool: &PgPool,
    actor: &User,
    character_id: i64,
    body: &str,
) -> Result<Option<Note>, CharacterError> {
    let Some(character) = db::find(pool, character_id).await? else {
        return external(CharacterError::NotFound);
    };
    if character.user_id != Some(actor.id) {
        return external(CharacterError::Forbidden);
    }
    let body = body.trim();
    if body.is_empty() {
        db::delete_note(pool, character_id).await?;
        return Ok(None);
    }
    if body.chars().count() > 10_000 {
        return external(CharacterError::NoteTooLong);
    }
    Ok(Some(db::upsert_note(pool, character_id, body).await?))
}

/// A note with its character, for the officers' list.
#[derive(Debug, serde::Serialize)]
pub struct NoteListing {
    pub character_id: i64,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    pub username: String,
    pub body: String,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
}

/// The last names of the test characters [`seed`] makes, four per class.
const SEED_NAMES: [&str; 4] = ["One", "Two", "Three", "Four"];

/// Test characters, linked to no member: four of each class, named after it ("Shaman One"
/// through "Shaman Four"), each playing all three of the class's specs with the talents of each
/// spec's own tree. One, Two, and Three each have a different main; Four has none. Names already
/// taken are skipped. Returns how many were made.
pub async fn seed(pool: &PgPool) -> sqlx::Result<u64> {
    let mut created = 0;
    for class in crate::wow::catalog::CLASSES {
        for (i, last_name) in SEED_NAMES.iter().enumerate() {
            let main = class.specs.get(i).map(|s| s.slug);
            let specs: Vec<CharacterSpec> = class
                .specs
                .iter()
                .map(|spec| CharacterSpec {
                    spec: spec.slug.to_string(),
                    talents: class
                        .talents
                        .iter()
                        .filter(|t| t.tree == spec.slug)
                        .map(|t| t.slug.to_string())
                        .collect(),
                    is_main: main == Some(spec.slug),
                })
                .collect();
            let mut tx = pool.begin().await?;
            if let Some(id) = db::insert_seed(&mut tx, class.name, last_name, class.slug).await? {
                db::replace_specs(&mut tx, id, &specs).await?;
                created += 1;
            }
            tx.commit().await?;
        }
    }
    Ok(created)
}

/// Removes [`seed`]'s characters, except any that raided or won loot (they are history now).
/// Returns how many went and how many stayed.
pub async fn unseed(pool: &PgPool) -> sqlx::Result<(u64, u64)> {
    db::delete_seeded(pool, &seed_names()).await
}

/// The ids of [`seed`]'s characters that are still around.
pub async fn seeded_ids(pool: &PgPool) -> sqlx::Result<Vec<i64>> {
    db::find_seeded(pool, &seed_names()).await
}

/// [`seed`]'s characters' full names, lowercase.
fn seed_names() -> Vec<String> {
    crate::wow::catalog::CLASSES
        .iter()
        .flat_map(|class| {
            SEED_NAMES
                .iter()
                .map(move |last| format!("{} {last}", class.name))
        })
        .map(|name| name.to_lowercase())
        .collect()
}

/// Every note on a character of the raiding roster (leaders through trials), latest first.
pub async fn raiding_notes(pool: &PgPool) -> sqlx::Result<Vec<NoteListing>> {
    db::list_raiding_notes(pool).await
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::guild::Rank;
    use crate::guild::tests::{admin, member};

    /// The admin adds `owner`'s character, as only the admin can.
    pub(crate) async fn add(
        pool: &PgPool,
        owner: &User,
        mut input: CharacterInput,
    ) -> Result<Character, CharacterError> {
        static ADDERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = ADDERS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let admin = admin(pool, &format!("Adder{n}")).await;
        input.user_id = Some(Some(owner.id));
        create(pool, &admin, &input).await
    }

    pub(crate) fn input(first: &str, last: &str, class: &str, is_main: bool) -> CharacterInput {
        CharacterInput {
            first_name: first.into(),
            last_name: last.into(),
            class: class.into(),
            race: None,
            sex: Some(Sex::Female),
            ruleset: None,
            level: None,
            reached_60_at: None,
            is_main,
            user_id: None,
        }
    }

    #[sqlx::test]
    async fn finds_a_character_by_its_page_name(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let garr = add(&pool, &ey, input("Garr", "Bonecrush", "warrior", true))
            .await
            .unwrap();
        for slug in ["garr-bonecrush", "Garr-Bonecrush", "GARR-BONECRUSH"] {
            let found = find_by_slug(&pool, slug).await.unwrap();
            assert_eq!(found.map(|c| c.id), Some(garr.id), "{slug}");
        }
        for slug in ["garr", "garr-bone", "garrbonecrush", "garr-bonecrush-x", ""] {
            assert!(find_by_slug(&pool, slug).await.unwrap().is_none(), "{slug}");
        }
    }

    #[sqlx::test]
    async fn records_a_race(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let mut sky = input("Aerin", "Gale", "shaman", false);
        sky.race = Some(" windshaper-skyborne ".into());
        let made = add(&pool, &ey, sky.clone()).await.unwrap();
        sky.user_id = None;
        assert_eq!(made.race.as_deref(), Some("windshaper-skyborne"));

        // Empty is none recorded.
        sky.race = Some(String::new());
        let cleared = update(&pool, &officer, made.id, &sky).await.unwrap();
        assert_eq!(cleared.race, None);

        // Only Forever's races.
        sky.race = Some("pandaren".into());
        assert!(matches!(
            update(&pool, &officer, made.id, &sky).await,
            Err(Error::External(CharacterError::UnknownRace))
        ));
        sky.race = Some("high-order-skyborne".into());
        let again = update(&pool, &officer, made.id, &sky).await.unwrap();
        assert_eq!(again.race.as_deref(), Some("high-order-skyborne"));
    }

    #[sqlx::test]
    async fn records_a_sex(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let mut sky = input("Aerin", "Gale", "shaman", false);
        let made = add(&pool, &ey, sky.clone()).await.unwrap();
        assert_eq!(made.sex, Some(Sex::Female));

        sky.sex = Some(Sex::Male);
        let changed = update(&pool, &officer, made.id, &sky).await.unwrap();
        assert_eq!(changed.sex, Some(Sex::Male));

        // None recorded, like a race.
        sky.sex = None;
        let cleared = update(&pool, &officer, made.id, &sky).await.unwrap();
        assert_eq!(cleared.sex, None);

        // Optional, and only the two.
        let json = |sex: &str| {
            serde_json::from_str::<CharacterInput>(&format!(
                r#"{{"first_name":"A","last_name":"B","class":"mage"{sex}}}"#
            ))
        };
        assert_eq!(json("").unwrap().sex, None);
        assert_eq!(json(r#","sex":null"#).unwrap().sex, None);
        assert!(json(r#","sex":"other""#).is_err());
        assert_eq!(json(r#","sex":"male""#).unwrap().sex, Some(Sex::Male));
    }

    #[sqlx::test]
    async fn records_a_level(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let mut sky = input("Aerin", "Gale", "shaman", false);
        let made = add(&pool, &ey, sky.clone()).await.unwrap();
        assert_eq!(made.level, 1, "a new character starts at 1");

        // No time below 60, even if one is sent.
        sky.level = Some(42);
        sky.reached_60_at = Some(time::macros::datetime!(2026-11-20 03:15 UTC));
        let leveled = update(&pool, &officer, made.id, &sky).await.unwrap();
        assert_eq!(leveled.level, 42);
        assert_eq!(leveled.reached_60_at, None);

        // Reaching 60 without a time is now; absent keeps the level and the time.
        sky.level = Some(60);
        sky.reached_60_at = None;
        let capped = update(&pool, &officer, made.id, &sky).await.unwrap();
        let now = capped.reached_60_at.expect("reaching 60 records when");
        sky.level = None;
        let kept = update(&pool, &officer, made.id, &sky).await.unwrap();
        assert_eq!((kept.level, kept.reached_60_at), (60, Some(now)));

        // An officer corrects it from the screenshot.
        let ding = time::macros::datetime!(2026-11-20 03:15 UTC);
        sky.reached_60_at = Some(ding);
        let corrected = update(&pool, &officer, made.id, &sky).await.unwrap();
        assert_eq!(corrected.reached_60_at, Some(ding));

        // Dropping below 60 (a mistake undone) clears it.
        sky.level = Some(59);
        let undone = update(&pool, &officer, made.id, &sky).await.unwrap();
        assert_eq!(undone.reached_60_at, None);

        // Added at 60 with a time.
        let mut done = input("Brann", "Bronzebeard", "warrior", false);
        done.level = Some(60);
        done.reached_60_at = Some(ding);
        let made = add(&pool, &ey, done).await.unwrap();
        assert_eq!(made.reached_60_at, Some(ding));

        for bad in [0, 61] {
            sky.level = Some(bad);
            assert!(matches!(
                update(&pool, &officer, made.id, &sky).await,
                Err(Error::External(CharacterError::InvalidLevel))
            ));
        }
    }

    #[test]
    fn names_are_letters() {
        assert_eq!(name(" Thrall ").unwrap(), "Thrall");
        assert_eq!(name("Éowyn").unwrap(), "Éowyn");
        for bad in ["T", "Two Words", "R2d2", "", "Abcdefghijklmnopqrstuvwxy"] {
            assert!(name(bad).is_err(), "{bad:?}");
        }
    }

    #[sqlx::test]
    async fn mains_move_and_names_are_unique(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let other = member(&pool, "Other", Rank::Raider).await;

        let first = add(&pool, &ey, input("Arthas", "Menethil", "paladin", false))
            .await
            .unwrap();
        assert!(first.is_main, "the first character is the main");
        assert_eq!(first.username.as_deref(), Some("Ey"));
        let second = add(&pool, &ey, input("Jaina", "Proudmoore", "mage", true))
            .await
            .unwrap();
        assert!(second.is_main);
        let mine: Vec<_> = list(&pool)
            .await
            .unwrap()
            .into_iter()
            .filter(|c| c.is_main)
            .collect();
        assert_eq!(mine.len(), 1);

        // The full name is unique, whatever the capitals.
        assert!(matches!(
            add(&pool, &other, input("arthas", "MENETHIL", "warrior", false)).await,
            Err(Error::External(CharacterError::NameTaken))
        ));
        // A different last name is a different character.
        add(&pool, &other, input("Arthas", "Other", "warrior", false))
            .await
            .unwrap();

        // Members add, edit, and remove no characters, not even their own; officers do all three.
        let uther = input("Uther", "Lightbringer", "paladin", false);
        assert!(matches!(
            create(&pool, &ey, &uther).await,
            Err(Error::External(CharacterError::OfficersOnly))
        ));
        assert!(matches!(
            update(
                &pool,
                &ey,
                first.id,
                &input("Arthas", "Menethil", "paladin", true)
            )
            .await,
            Err(Error::External(CharacterError::OfficersOnly))
        ));
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let made = create(&pool, &officer, &uther).await.unwrap();
        assert_eq!(
            made.user_id,
            Some(officer.id),
            "an officer's own, unless they name another"
        );
        assert!(matches!(
            delete(&pool, &ey, first.id).await,
            Err(Error::External(CharacterError::OfficersOnly))
        ));
        delete(&pool, &officer, first.id).await.unwrap();
    }

    #[sqlx::test]
    async fn only_the_admin_sees_unclaimed_characters_unless_an_officer_asks(pool: PgPool) {
        let admin = admin(&pool, "Admin").await;
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let raider = member(&pool, "Raider", Rank::Raider).await;
        add(&pool, &raider, input("Arthas", "Menethil", "paladin", true))
            .await
            .unwrap();
        let mut pug = input("Random", "Pug", "rogue", false);
        pug.user_id = Some(None);
        create(&pool, &admin, &pug).await.unwrap();

        let names = |characters: Vec<Character>| -> Vec<String> {
            characters.into_iter().map(|c| c.first_name).collect()
        };
        let seen = |actor, unclaimed| visible(&pool, actor, unclaimed);
        assert_eq!(
            names(seen(&admin, false).await.unwrap()),
            ["Arthas", "Random"]
        );
        assert_eq!(names(seen(&officer, false).await.unwrap()), ["Arthas"]);
        assert_eq!(
            names(seen(&officer, true).await.unwrap()),
            ["Arthas", "Random"]
        );
        assert_eq!(names(seen(&raider, false).await.unwrap()), ["Arthas"]);
        assert_eq!(names(seen(&raider, true).await.unwrap()), ["Arthas"]);
    }

    #[sqlx::test]
    async fn the_admin_adds_unclaimed_characters_and_officers_reassign(pool: PgPool) {
        let admin = admin(&pool, "Admin").await;
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let raider = member(&pool, "Raider", Rank::Raider).await;

        let mut pug = input("Random", "Pug", "rogue", true);
        pug.user_id = Some(None);
        let pug = create(&pool, &admin, &pug).await.unwrap();
        assert_eq!(pug.user_id, None);
        assert!(!pug.is_main, "an unclaimed character is nobody's main");

        // Handing it to a member who has no characters yet, as their main.
        let mut give = input("Random", "Pug", "rogue", true);
        give.user_id = Some(Some(raider.id));
        let given = update(&pool, &officer, pug.id, &give).await.unwrap();
        assert_eq!(given.user_id, Some(raider.id));
        assert!(given.is_main);

        // An absent owner keeps it theirs.
        let renamed = update(
            &pool,
            &officer,
            pug.id,
            &input("Randal", "Pug", "rogue", true),
        )
        .await
        .unwrap();
        assert_eq!(renamed.user_id, Some(raider.id));
        assert_eq!(renamed.first_name, "Randal");

        let mut nobody = input("Ghost", "Owner", "rogue", false);
        nobody.user_id = Some(Some(UserId(999_999)));
        assert!(matches!(
            create(&pool, &admin, &nobody).await,
            Err(Error::External(CharacterError::UnknownMember))
        ));
    }

    #[sqlx::test]
    async fn characters_play_any_specs_with_an_optional_main(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let paladin = add(&pool, &ey, input("Arthas", "Menethil", "paladin", false))
            .await
            .unwrap();
        let spec = |spec: &str, talents: &[&str]| SpecInput {
            spec: spec.into(),
            talents: talents.iter().map(|t| t.to_string()).collect(),
        };
        let all_three = SpecsInput {
            specs: vec![
                spec("retribution", &["improved-blessing-of-might"]),
                // Kings is in the protection tree, but a holy paladin can take it.
                spec(
                    "holy",
                    &["improved-blessing-of-wisdom", "blessing-of-kings"],
                ),
                spec("protection", &[]),
            ],
            main: Some("holy".into()),
        };
        let saved = set_specs(&pool, &officer, paladin.id, &all_three)
            .await
            .unwrap();
        let order: Vec<_> = saved
            .specs
            .iter()
            .map(|s| (s.spec.as_str(), s.is_main))
            .collect();
        // The main first, then the catalog's order.
        assert_eq!(
            order,
            [
                ("holy", true),
                ("protection", false),
                ("retribution", false)
            ]
        );
        // Talents in the catalog's order, not click order.
        assert_eq!(
            saved.specs[0].talents,
            ["blessing-of-kings", "improved-blessing-of-wisdom"]
        );

        // No main is fine.
        let unsure = SpecsInput {
            specs: vec![spec("holy", &[]), spec("retribution", &[])],
            main: None,
        };
        let saved = set_specs(&pool, &officer, paladin.id, &unsure)
            .await
            .unwrap();
        assert!(saved.specs.iter().all(|s| !s.is_main));

        let (db, actor, id) = (&pool, &officer, paladin.id);
        let refused = |input: SpecsInput| async move {
            matches!(
                set_specs(db, actor, id, &input).await,
                Err(Error::External(CharacterError::InvalidSpec))
            )
        };
        // Another class's spec or talent, a spec twice, a main it does not play.
        assert!(
            refused(SpecsInput {
                specs: vec![spec("shadow", &[])],
                main: None
            })
            .await
        );
        assert!(
            refused(SpecsInput {
                specs: vec![spec("holy", &["shadow-weaving"])],
                main: None
            })
            .await
        );
        assert!(
            refused(SpecsInput {
                specs: vec![spec("holy", &[]), spec("holy", &[])],
                main: None
            })
            .await
        );
        assert!(
            refused(SpecsInput {
                specs: vec![spec("holy", &[])],
                main: Some("retribution".into())
            })
            .await
        );
        assert!(matches!(
            set_specs(&pool, &ey, paladin.id, &all_three).await,
            Err(Error::External(CharacterError::OfficersOnly))
        ));

        // Renaming keeps the specs; a new class drops them.
        set_specs(&pool, &officer, paladin.id, &all_three)
            .await
            .unwrap();
        let renamed = update(
            &pool,
            &officer,
            paladin.id,
            &input("Arthas", "Light", "paladin", true),
        )
        .await
        .unwrap();
        assert_eq!(renamed.specs.len(), 3);
        let warrior = update(
            &pool,
            &officer,
            paladin.id,
            &input("Arthas", "Light", "warrior", true),
        )
        .await
        .unwrap();
        assert!(warrior.specs.is_empty());
    }

    #[sqlx::test]
    async fn seeds_four_unlinked_characters_of_each_class(pool: PgPool) {
        assert_eq!(seed(&pool).await.unwrap(), 36);
        // Again: every name is taken, so nothing more.
        assert_eq!(seed(&pool).await.unwrap(), 0);

        let all = list(&pool).await.unwrap();
        assert!(all.iter().all(|c| c.user_id.is_none() && !c.is_main));
        let shamans: Vec<_> = all.iter().filter(|c| c.class == "shaman").collect();
        let names: Vec<_> = shamans.iter().map(|c| c.last_name.as_str()).collect();
        assert_eq!(names, ["Four", "One", "Three", "Two"]);
        // Every one plays all three specs, each with its tree's talents.
        assert!(shamans.iter().all(|c| c.specs.len() == 3));
        let main = |last: &str| {
            shamans
                .iter()
                .find(|c| c.last_name == last)
                .unwrap()
                .specs
                .iter()
                .find(|s| s.is_main)
                .map(|s| s.spec.clone())
        };
        assert_eq!(main("One").as_deref(), Some("elemental"));
        assert_eq!(main("Two").as_deref(), Some("enhancement"));
        assert_eq!(main("Three").as_deref(), Some("restoration"));
        assert_eq!(main("Four"), None, "Four has no main");
        let one = shamans.iter().find(|c| c.last_name == "One").unwrap();
        let enhancement = one.specs.iter().find(|s| s.spec == "enhancement").unwrap();
        assert_eq!(enhancement.talents, ["enhancing-totems", "stormstrike"]);

        // Unseeding removes them, except one with history; a member's own character stays.
        let admin = admin(&pool, "Admin").await;
        create(&pool, &admin, &input("Shaman", "Five", "shaman", false))
            .await
            .unwrap();
        let raid = crate::raids::create_raid(
            &pool,
            &crate::raids::RaidInput {
                zone: "onyxias-lair".into(),
                starts_local: time::macros::datetime!(2026-12-10 20:00),
            },
        )
        .await
        .unwrap();
        crate::raids::add_attendees(&pool, raid.id, &[one.id])
            .await
            .unwrap();
        assert_eq!(unseed(&pool).await.unwrap(), (35, 1));
        let left: Vec<_> = list(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|c| format!("{} {}", c.first_name, c.last_name))
            .collect();
        assert_eq!(left, ["Shaman Five", "Shaman One"]);
    }

    #[sqlx::test]
    async fn notes_are_written_by_the_player(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let character = add(&pool, &ey, input("Arthas", "Menethil", "paladin", false))
            .await
            .unwrap();

        assert!(may_read_note(&officer, &character));
        assert!(matches!(
            set_note(&pool, &officer, character.id, "hi").await,
            Err(Error::External(CharacterError::Forbidden))
        ));
        let note = set_note(&pool, &ey, character.id, " Saving for Ashkandi ")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(note.body, "Saving for Ashkandi");
        assert_eq!(raiding_notes(&pool).await.unwrap().len(), 1);
        assert!(
            set_note(&pool, &ey, character.id, "  ")
                .await
                .unwrap()
                .is_none()
        );
        assert!(super::note(&pool, character.id).await.unwrap().is_none());
    }
}
