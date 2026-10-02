//! WoW: Forever launch sign-ups: the classes each member plans to play on launch day, their main
//! and alts, with the specs they hope to play. Unnamed: Forever characters have a first and a last
//! name, and few members know theirs yet, so the member's username stands in. Self-reported ahead
//! of Blizzard opening an API for the game; real characters will live elsewhere once it does.

pub mod api;
pub mod catalog;
mod db;

use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use crate::users::UserId;
use crate::{Error, Problem, Result};

pub use api::router;

#[derive(Debug, serde::Serialize)]
pub struct Character {
    pub id: i64,
    pub class: String,
    pub specs: Vec<String>,
    pub is_main: bool,
    #[serde(with = "iso8601")]
    pub created_at: OffsetDateTime,
    #[serde(with = "iso8601")]
    pub updated_at: OffsetDateTime,
}

/// A character as submitted, before validation.
#[derive(Debug, serde::Deserialize)]
pub struct CharacterInput {
    pub class: String,
    #[serde(default)]
    pub specs: Vec<String>,
    #[serde(default)]
    pub is_main: bool,
}

#[derive(Debug, thiserror::Error, Problem)]
pub enum LaunchError {
    #[error("unknown class")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Unknown Class",
        detail = "That is not a WoW: Forever class."
    )]
    UnknownClass,
    #[error("invalid specs")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Specs",
        detail = "Pick specs from the character's own class, each at most once."
    )]
    InvalidSpecs,
    #[error("class taken")]
    #[problem(
        status = CONFLICT,
        title = "Class Taken",
        detail = "You have already signed up with that class."
    )]
    ClassTaken,
    #[error("character not found")]
    #[problem(status = NOT_FOUND, title = "Not Found", detail = "No such character.")]
    NotFound,
}

fn external<T>(error: LaunchError) -> Result<T, LaunchError> {
    Err(Error::External(error))
}

/// A validated character: class known, specs from that class in catalog order.
struct Valid {
    class: &'static str,
    specs: Vec<String>,
    is_main: bool,
}

fn validate(input: &CharacterInput) -> Result<Valid, LaunchError> {
    let Some(class) = catalog::find(input.class.trim()) else {
        return external(LaunchError::UnknownClass);
    };
    let mut seen = std::collections::HashSet::new();
    for spec in &input.specs {
        if !class.specs.iter().any(|s| s.slug == spec) || !seen.insert(spec.as_str()) {
            return external(LaunchError::InvalidSpecs);
        }
    }
    // Stored in the catalog's order, so a listing reads the same however they were clicked.
    let specs = class
        .specs
        .iter()
        .filter(|s| seen.contains(s.slug))
        .map(|s| s.slug.to_string())
        .collect();
    Ok(Valid {
        class: class.slug,
        specs,
        is_main: input.is_main,
    })
}

fn classify(error: sqlx::Error) -> Error<LaunchError> {
    crate::error::classify_db_error(error, |constraint| match constraint {
        "launch_characters_user_id_class_key" => Some(LaunchError::ClassTaken),
        _ => None,
    })
}

pub async fn list(pool: &PgPool, user_id: UserId) -> sqlx::Result<Vec<Character>> {
    db::list(pool, user_id).await
}

/// Signs up with a class. A member's first sign-up becomes their main whether or not they asked;
/// declaring another main moves the title. One per class, so at most one per class in the catalog.
pub async fn create(
    pool: &PgPool,
    user_id: UserId,
    input: &CharacterInput,
) -> Result<Character, LaunchError> {
    let valid = validate(input)?;
    let mut tx = pool.begin().await?;
    let is_main = valid.is_main || db::count(&mut tx, user_id).await? == 0;
    if is_main {
        db::clear_main(&mut tx, user_id).await?;
    }
    let character = db::insert(&mut tx, user_id, valid.class, &valid.specs, is_main)
        .await
        .map_err(classify)?;
    tx.commit().await?;
    Ok(character)
}

/// Replaces a character's details. Unsetting `is_main` on the main leaves the member without
/// one, which is allowed: they may not have decided.
pub async fn update(
    pool: &PgPool,
    user_id: UserId,
    id: i64,
    input: &CharacterInput,
) -> Result<Character, LaunchError> {
    let valid = validate(input)?;
    let mut tx = pool.begin().await?;
    if valid.is_main {
        db::clear_main_except(&mut tx, user_id, id).await?;
    }
    let character = db::update(
        &mut tx,
        user_id,
        id,
        valid.class,
        &valid.specs,
        valid.is_main,
    )
    .await
    .map_err(classify)?;
    let Some(character) = character else {
        return external(LaunchError::NotFound);
    };
    tx.commit().await?;
    Ok(character)
}

pub async fn delete(pool: &PgPool, user_id: UserId, id: i64) -> Result<(), LaunchError> {
    if db::delete(pool, user_id, id).await? {
        Ok(())
    } else {
        external(LaunchError::NotFound)
    }
}

/// Every sign-up with its member, by username, mains first, for the board and the CLI.
pub async fn list_all(pool: &PgPool) -> sqlx::Result<Vec<RosterEntry>> {
    db::list_all(pool).await
}

#[derive(Debug, serde::Serialize)]
pub struct RosterEntry {
    pub username: String,
    pub class: String,
    pub specs: Vec<String>,
    pub is_main: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(class: &str, specs: &[&str], is_main: bool) -> CharacterInput {
        CharacterInput {
            class: class.into(),
            specs: specs.iter().map(|s| s.to_string()).collect(),
            is_main,
        }
    }

    async fn user(pool: &PgPool, username: &str) -> UserId {
        let mut tx = pool.begin().await.unwrap();
        let user = crate::users::insert_user(
            &mut tx,
            &crate::users::CreateUserPayload {
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
        tx.commit().await.unwrap();
        user.id
    }

    #[test]
    fn validates_classes_and_specs() {
        let ok = validate(&input(" druid ", &["restoration", "feral"], false)).unwrap();
        assert_eq!(ok.class, "druid");
        // Catalog order, not click order.
        assert_eq!(ok.specs, vec!["feral", "restoration"]);
        assert!(validate(&input("monk", &[], false)).is_err());
        assert!(validate(&input("mage", &["holy"], false)).is_err());
        assert!(validate(&input("mage", &["fire", "fire"], false)).is_err());
    }

    #[sqlx::test]
    async fn mains_move_and_classes_are_unique_per_member(pool: PgPool) {
        let ey = user(&pool, "Ey").await;
        let other = user(&pool, "Other").await;

        // The first sign-up is the main without asking.
        let first = create(&pool, ey, &input("warrior", &["protection"], false))
            .await
            .unwrap();
        assert!(first.is_main);
        // A second main takes the title.
        let second = create(&pool, ey, &input("paladin", &["holy"], true))
            .await
            .unwrap();
        assert!(second.is_main);
        let all = list(&pool, ey).await.unwrap();
        assert_eq!(all.iter().filter(|c| c.is_main).count(), 1);
        assert_eq!(all[0].class, "paladin", "the main is listed first");

        // The same class twice: refused for this member, fine for another.
        assert!(matches!(
            create(&pool, ey, &input("warrior", &[], false)).await,
            Err(Error::External(LaunchError::ClassTaken))
        ));
        create(&pool, other, &input("warrior", &[], false))
            .await
            .unwrap();
        // Editing into a class already taken is refused too.
        assert!(matches!(
            update(&pool, ey, second.id, &input("warrior", &[], true)).await,
            Err(Error::External(LaunchError::ClassTaken))
        ));

        // Editing back to main moves it again; another member's sign-up is out of reach.
        let edited = update(&pool, ey, first.id, &input("warrior", &["arms"], true))
            .await
            .unwrap();
        assert!(edited.is_main);
        assert_eq!(edited.specs, vec!["arms"]);
        assert!(matches!(
            update(&pool, other, first.id, &input("warrior", &[], false)).await,
            Err(Error::External(LaunchError::NotFound))
        ));
        assert!(matches!(
            delete(&pool, other, first.id).await,
            Err(Error::External(LaunchError::NotFound))
        ));
        delete(&pool, ey, first.id).await.unwrap();
        assert_eq!(list(&pool, ey).await.unwrap().len(), 1);

        let roster = list_all(&pool).await.unwrap();
        assert_eq!(roster.len(), 2);
        assert_eq!(roster[0].username, "Ey");
    }
}
