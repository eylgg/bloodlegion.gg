//! The guild roster as the addon saw it in game, sent by the Blood Legion client: each member's name and
//! level, and when the addon first saw them at 60. It raises the levels of the characters it names
//! (never lowers them: an older roster cannot undo a newer one) and keeps the earliest time each
//! reached 60, so several officers' rosters only sharpen it.

use sqlx::PgPool;
use time::{OffsetDateTime, serde::iso8601};

use super::{CharacterError, external};
use crate::Result;

/// The guild's name in game. The addon records only this guild's roster and the site takes only
/// this guild's: WoW gives addons a guild's name, not an id.
pub const GUILD_NAME: &str = "Blood Legion";

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
}

#[derive(Debug, serde::Serialize)]
pub struct RosterResult {
    /// How many characters it matched.
    pub matched: usize,
    /// The members no character on the site has the name of, for an officer to add.
    pub unmatched: Vec<UnmatchedMember>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct UnmatchedMember {
    pub name: String,
    pub level: i16,
    pub class: Option<String>,
}

/// How a roster name is matched against `characters.name_normalized` (lowercase `first last`):
/// trimmed and lowercased, with any `-Realm` suffix dropped. A name without a space also matches a
/// character whose two names run together, since Forever's roster format is not known yet.
fn normalize(name: &str) -> String {
    let name = name.trim();
    let name = name.split_once('-').map_or(name, |(name, _realm)| name);
    name.to_lowercase()
}

/// Applies the roster: raises levels and records reaching 60, by name.
pub async fn apply(pool: &PgPool, input: &RosterInput) -> Result<RosterResult, CharacterError> {
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
    for member in &input.members {
        if super::db::raise_level(
            &mut tx,
            &normalize(&member.name),
            member.level,
            member.reached_60_at,
        )
        .await?
        {
            matched += 1;
        } else {
            unmatched.push(UnmatchedMember {
                name: member.name.trim().to_string(),
                level: member.level,
                class: member.class.clone(),
            });
        }
    }
    tx.commit().await?;
    Ok(RosterResult { matched, unmatched })
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
        }
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

        // Matched by full name or with the names run together; a stranger is listed.
        let result = apply(
            &pool,
            &roster(vec![
                seen("garr bonecrush", 40, None),
                seen("LiraMoonwell-Forever", 12, None),
                seen("Random Pug", 30, None),
            ]),
        )
        .await
        .unwrap();
        assert_eq!(result.matched, 2);
        assert_eq!(
            result.unmatched,
            [UnmatchedMember {
                name: "Random Pug".into(),
                level: 30,
                class: Some("Warrior".into())
            }]
        );
        let pool = &pool;
        let level = |id| async move { crate::characters::find(pool, id).await.unwrap().unwrap() };
        assert_eq!(level(garr.id).await.level, 40);
        assert_eq!(level(lira.id).await.level, 12);

        // An older roster never lowers a level.
        apply(pool, &roster(vec![seen("Garr Bonecrush", 35, None)]))
            .await
            .unwrap();
        assert_eq!(level(garr.id).await.level, 40);

        // At 60: the time the addon saw it, and an earlier sighting wins over a later one.
        let later = datetime!(2026-11-20 05:00 UTC);
        let earlier = datetime!(2026-11-20 03:15 UTC);
        apply(pool, &roster(vec![seen("Garr Bonecrush", 60, Some(later))]))
            .await
            .unwrap();
        apply(
            pool,
            &roster(vec![seen("Garr Bonecrush", 60, Some(earlier))]),
        )
        .await
        .unwrap();
        apply(pool, &roster(vec![seen("Garr Bonecrush", 60, Some(later))]))
            .await
            .unwrap();
        let capped = level(garr.id).await;
        assert_eq!((capped.level, capped.reached_60_at), (60, Some(earlier)));

        // At 60 without a time: now.
        apply(pool, &roster(vec![seen("Lira Moonwell", 60, None)]))
            .await
            .unwrap();
        assert!(level(lira.id).await.reached_60_at.is_some());
    }

    #[sqlx::test]
    async fn only_the_guilds_roster(pool: PgPool) {
        let mut other = roster(vec![seen("Garr Bonecrush", 40, None)]);
        other.guild = "Some Other Guild".into();
        assert!(matches!(
            apply(&pool, &other).await,
            Err(Error::External(CharacterError::WrongGuild))
        ));
        assert!(matches!(
            apply(&pool, &roster(vec![seen("Garr Bonecrush", 61, None)])).await,
            Err(Error::External(CharacterError::InvalidLevel))
        ));
    }
}
