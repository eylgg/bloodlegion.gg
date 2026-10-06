//! Members' WoW: Forever characters. A character has a first and a last name and no realm (Forever
//! has rulesets instead, and the guild plays on one), so its full name is unique across the site.
//! Members add their own; officers may add anyone's, including a character no account claims (a
//! pug who won loot). Each member has at most one main.

pub mod api;
mod db;

use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use crate::users::{User, UserId};
use crate::{Error, Problem, Result};

pub use api::router;

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
    pub is_main: bool,
    /// The specs it plays, its main (if it has one) first, then in the catalog's order.
    pub specs: sqlx::types::Json<Vec<CharacterSpec>>,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
}

/// A character as submitted, before validation.
#[derive(Debug, serde::Deserialize)]
pub struct CharacterInput {
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    #[serde(default)]
    pub is_main: bool,
    /// Who plays it: absent for the caller themselves; an officer may name any member, or send
    /// `null` for an unclaimed character.
    #[serde(default, deserialize_with = "crate::api::double_option")]
    pub user_id: Option<Option<UserId>>,
}

/// A spec a character plays: any of its class's, with the notable talents it takes there (see
/// `launch::catalog` for both), and whether it is the character's main.
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
        crate::launch::catalog::find(class)
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
        detail = "Only the character's player or an officer can change it."
    )]
    Forbidden,
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
    user_id: Option<UserId>,
    is_main: bool,
}

/// Validates `input` from `actor`; `current_owner` is the owner of the character being edited
/// (`None` when creating), which an absent `user_id` keeps.
fn validate(
    actor: &User,
    input: &CharacterInput,
    current_owner: Option<Option<UserId>>,
) -> Result<Valid, CharacterError> {
    let Some(class) = crate::launch::catalog::find(input.class.trim()) else {
        return external(CharacterError::UnknownClass);
    };
    let user_id = match (input.user_id, current_owner) {
        (Some(owner), _) => owner,
        (None, Some(current)) => current,
        (None, None) => Some(actor.id),
    };
    // Only officers give a character to someone else (or to nobody).
    if user_id != Some(actor.id) && !actor.is_officer() {
        return external(CharacterError::Forbidden);
    }
    Ok(Valid {
        first_name: name(&input.first_name)?,
        last_name: name(&input.last_name)?,
        class: class.slug,
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

pub async fn find(pool: &PgPool, id: i64) -> sqlx::Result<Option<Character>> {
    db::find(pool, id).await
}

/// Adds a character. A member's first character becomes their main whether or not they asked;
/// declaring another main moves the title.
pub async fn create(
    pool: &PgPool,
    actor: &User,
    input: &CharacterInput,
) -> Result<Character, CharacterError> {
    let valid = validate(actor, input, None)?;
    let mut tx = pool.begin().await?;
    let mut is_main = valid.is_main;
    if let Some(owner) = valid.user_id {
        is_main |= db::count_owned(&mut tx, owner).await? == 0;
        if is_main {
            db::clear_main(&mut tx, owner, None).await?;
        }
    }
    let id = db::insert(
        &mut tx,
        valid.user_id,
        &valid.first_name,
        &valid.last_name,
        valid.class,
        is_main,
    )
    .await
    .map_err(classify)?;
    tx.commit().await?;
    Ok(db::find(pool, id)
        .await?
        .expect("the character was just created"))
}

/// Replaces a character's details: by its player, or by an officer (who may also hand it to
/// another member).
pub async fn update(
    pool: &PgPool,
    actor: &User,
    id: i64,
    input: &CharacterInput,
) -> Result<Character, CharacterError> {
    let mut tx = pool.begin().await?;
    let Some(current_owner) = db::lock_owner(&mut tx, id).await? else {
        return external(CharacterError::NotFound);
    };
    if current_owner != Some(actor.id) && !actor.is_officer() {
        return external(CharacterError::Forbidden);
    }
    let valid = validate(actor, input, Some(current_owner))?;
    if let (true, Some(owner)) = (valid.is_main, valid.user_id) {
        db::clear_main(&mut tx, owner, Some(id)).await?;
    }
    // A new class has other specs and talents.
    db::clear_specs_unless_class(&mut tx, id, valid.class).await?;
    db::update(
        &mut tx,
        id,
        valid.user_id,
        &valid.first_name,
        &valid.last_name,
        valid.class,
        valid.is_main,
    )
    .await
    .map_err(classify)?;
    tx.commit().await?;
    Ok(db::find(pool, id)
        .await?
        .expect("the character was just updated"))
}

/// Validates `input` against the class: each spec of the class at most once, talents of the class
/// (in the catalog's order), and a main among the specs.
fn valid_specs(
    class: &'static crate::launch::catalog::Class,
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
/// by its player, or by an officer. A raid it was set to play a removed spec in falls back to its
/// main.
pub async fn set_specs(
    pool: &PgPool,
    actor: &User,
    id: i64,
    input: &SpecsInput,
) -> Result<Character, CharacterError> {
    let Some(character) = db::find(pool, id).await? else {
        return external(CharacterError::NotFound);
    };
    if character.user_id != Some(actor.id) && !actor.is_officer() {
        return external(CharacterError::Forbidden);
    }
    let Some(class) = crate::launch::catalog::find(&character.class) else {
        return external(CharacterError::UnknownClass);
    };
    let specs = valid_specs(class, input)?;
    let mut tx = pool.begin().await?;
    db::replace_specs(&mut tx, id, &specs).await?;
    tx.commit().await?;
    Ok(db::find(pool, id).await?.expect("the character exists"))
}

/// Removes a character that has never raided or won loot.
pub async fn delete(pool: &PgPool, actor: &User, id: i64) -> Result<(), CharacterError> {
    let mut tx = pool.begin().await?;
    let Some(owner) = db::lock_owner(&mut tx, id).await? else {
        return external(CharacterError::NotFound);
    };
    if owner != Some(actor.id) && !actor.is_officer() {
        return external(CharacterError::Forbidden);
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
    for class in crate::launch::catalog::CLASSES {
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
    let names: Vec<String> = crate::launch::catalog::CLASSES
        .iter()
        .flat_map(|class| {
            SEED_NAMES
                .iter()
                .map(move |last| format!("{} {last}", class.name))
        })
        .map(|name| name.to_lowercase())
        .collect();
    db::delete_seeded(pool, &names).await
}

/// Every note on a character of the raiding roster (leaders through trials), latest first.
pub async fn raiding_notes(pool: &PgPool) -> sqlx::Result<Vec<NoteListing>> {
    db::list_raiding_notes(pool).await
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::guild::Rank;
    use crate::guild::tests::member;

    pub(crate) fn input(first: &str, last: &str, class: &str, is_main: bool) -> CharacterInput {
        CharacterInput {
            first_name: first.into(),
            last_name: last.into(),
            class: class.into(),
            is_main,
            user_id: None,
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

        let first = create(&pool, &ey, &input("Arthas", "Menethil", "paladin", false))
            .await
            .unwrap();
        assert!(first.is_main, "the first character is the main");
        assert_eq!(first.username.as_deref(), Some("Ey"));
        let second = create(&pool, &ey, &input("Jaina", "Proudmoore", "mage", true))
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
            create(
                &pool,
                &other,
                &input("arthas", "MENETHIL", "warrior", false)
            )
            .await,
            Err(Error::External(CharacterError::NameTaken))
        ));
        // A different last name is a different character.
        create(&pool, &other, &input("Arthas", "Other", "warrior", false))
            .await
            .unwrap();

        // Someone else's character is out of reach; a member cannot give theirs away.
        assert!(matches!(
            update(
                &pool,
                &other,
                first.id,
                &input("Arthas", "Menethil", "paladin", true)
            )
            .await,
            Err(Error::External(CharacterError::Forbidden))
        ));
        let mut give = input("Arthas", "Menethil", "paladin", false);
        give.user_id = Some(Some(other.id));
        assert!(matches!(
            update(&pool, &ey, first.id, &give).await,
            Err(Error::External(CharacterError::Forbidden))
        ));
        delete(&pool, &ey, first.id).await.unwrap();
    }

    #[sqlx::test]
    async fn officers_add_unclaimed_characters_and_reassign(pool: PgPool) {
        let officer = member(&pool, "Officer", Rank::Officer).await;
        let raider = member(&pool, "Raider", Rank::Raider).await;

        let mut pug = input("Random", "Pug", "rogue", true);
        pug.user_id = Some(None);
        let pug = create(&pool, &officer, &pug).await.unwrap();
        assert_eq!(pug.user_id, None);
        assert!(!pug.is_main, "an unclaimed character is nobody's main");

        // Handing it to a member who has no characters yet, as their main.
        let mut give = input("Random", "Pug", "rogue", true);
        give.user_id = Some(Some(raider.id));
        let given = update(&pool, &officer, pug.id, &give).await.unwrap();
        assert_eq!(given.user_id, Some(raider.id));
        assert!(given.is_main);

        // The member can now edit it; an absent owner keeps it theirs.
        let renamed = update(
            &pool,
            &raider,
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
            create(&pool, &officer, &nobody).await,
            Err(Error::External(CharacterError::UnknownMember))
        ));
    }

    #[sqlx::test]
    async fn characters_play_any_specs_with_an_optional_main(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let other = member(&pool, "Other", Rank::Raider).await;
        let paladin = create(&pool, &ey, &input("Arthas", "Menethil", "paladin", false))
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
        let saved = set_specs(&pool, &ey, paladin.id, &all_three).await.unwrap();
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
        let saved = set_specs(&pool, &ey, paladin.id, &unsure).await.unwrap();
        assert!(saved.specs.iter().all(|s| !s.is_main));

        let (db, actor, id) = (&pool, &ey, paladin.id);
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
            set_specs(&pool, &other, paladin.id, &all_three).await,
            Err(Error::External(CharacterError::Forbidden))
        ));

        // Renaming keeps the specs; a new class drops them.
        set_specs(&pool, &ey, paladin.id, &all_three).await.unwrap();
        let renamed = update(
            &pool,
            &ey,
            paladin.id,
            &input("Arthas", "Light", "paladin", true),
        )
        .await
        .unwrap();
        assert_eq!(renamed.specs.len(), 3);
        let warrior = update(
            &pool,
            &ey,
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
        let officer = member(&pool, "Officer", Rank::Officer).await;
        create(&pool, &officer, &input("Shaman", "Five", "shaman", false))
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
        let character = create(&pool, &ey, &input("Arthas", "Menethil", "paladin", false))
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
