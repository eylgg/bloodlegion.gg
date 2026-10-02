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
