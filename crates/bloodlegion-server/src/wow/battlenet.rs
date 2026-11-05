//! The WoW: Forever characters on each member's Battle.net accounts, read from the profile API at
//! every sign-in into the site's own characters. A listed character with a site character's name is
//! linked to it, which goes to the account's member when nobody plays it yet; any other is added
//! hidden, for superusers only, until an officer adds a character of that name (which brings it out:
//! `characters::create`). Hidden ones follow Battle.net (its level, class, race, player) and go when
//! it stops listing them; the site's own characters only gain the link.

use anyhow::Context;
use axum::routing::get;
use axum::{Json, Router};
use sqlx::{PgConnection, PgPool};
use time::{OffsetDateTime, serde::iso8601};

use super::{Character, Ruleset};
use crate::auth::providers::ProviderId;
use crate::characters::Sex;
use crate::extract::Admin;
use crate::users::UserId;
use crate::{Result, State};

/// Battle.net's region for the guild's accounts.
pub const REGION: &str = "us";

/// The scope that lets an access token read the WoW profile.
const SCOPE: &str = "wow.profile";

/// Whether a provider's tokens can read the WoW profile (it asks for `wow.profile`).
pub fn reads_profile(scope: &str) -> bool {
    scope.split_whitespace().any(|s| s == SCOPE)
}

/// Reads the account's characters and applies them, in the background: a sign-in never waits on
/// Battle.net or fails because of it, so a failure is only logged.
pub fn spawn_sync(state: State, provider_id: ProviderId, subject: String, access_token: String) {
    tokio::spawn(async move {
        match sync(&state, provider_id, &subject, &access_token).await {
            Ok(synced) => tracing::info!(
                listed = synced.listed,
                linked = synced.linked,
                hidden = synced.hidden,
                claimed = synced.claimed,
                removed = synced.removed,
                "synced the Battle.net characters"
            ),
            Err(error) => tracing::warn!(
                error = format!("{error:#}"),
                "syncing the Battle.net characters failed"
            ),
        }
    });
}

/// What a sync did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Synced {
    /// Characters Battle.net listed.
    pub listed: usize,
    /// Of those, the ones on the site (linked to its character of that name).
    pub linked: usize,
    /// Of those, the ones kept hidden.
    pub hidden: usize,
    /// Characters that went to the account's member.
    pub claimed: u64,
    /// Hidden characters removed, no longer listed.
    pub removed: u64,
}

/// Reads the account's characters from Battle.net, then [`apply`]s them.
pub async fn sync(
    state: &State,
    provider_id: ProviderId,
    subject: &str,
    access_token: &str,
) -> anyhow::Result<Synced> {
    let profile = super::account_profile(
        &state.http_client,
        REGION,
        &super::profile_namespace(REGION),
        "en_US",
        access_token,
    )
    .await?;
    let characters = profile
        .as_ref()
        .map(super::parse_characters)
        .unwrap_or_default();
    apply(&state.pool, provider_id, subject, &characters)
        .await
        .context("applying the Battle.net characters")
}

/// A listed character as the site keeps it.
struct Mapped<'a> {
    battlenet_id: i64,
    first_name: &'a str,
    last_name: &'a str,
    class: &'static str,
    race: Option<&'static str>,
    sex: Sex,
    level: i16,
}

/// `None` for a character without a name or class the site takes.
fn map(character: &Character) -> Option<Mapped<'_>> {
    use crate::characters::is_valid_name;
    if !is_valid_name(&character.name) || !is_valid_name(&character.surname) {
        return None;
    }
    let class = super::catalog::find(&character.class.to_lowercase())?.slug;
    let race = super::catalog::find_race(&character.race.to_lowercase().replace(' ', "-"))
        .map(|race| race.slug);
    Some(Mapped {
        battlenet_id: i64::try_from(character.id).ok()?,
        first_name: &character.name,
        last_name: &character.surname,
        class,
        race,
        sex: if character.gender.eq_ignore_ascii_case("male") {
            Sex::Male
        } else {
            Sex::Female
        },
        level: character.level.clamp(1, 60) as i16,
    })
}

/// Applies the account's listing: links each character to the site's of the same Battle.net id or
/// name, adds the rest hidden, removes hidden ones no longer listed (and unlinks site ones), then
/// gives the account's characters nobody plays to its member ([`claim`]).
pub async fn apply(
    pool: &PgPool,
    provider_id: ProviderId,
    subject: &str,
    characters: &[Character],
) -> sqlx::Result<Synced> {
    let mut tx = pool.begin().await?;
    let owner = owner(&mut tx, provider_id, subject).await?;
    let mut synced = Synced {
        listed: characters.len(),
        ..Synced::default()
    };
    let mut ids = Vec::new();
    for character in characters.iter().filter_map(map) {
        ids.push(character.battlenet_id);
        let full_name = format!("{} {}", character.first_name, character.last_name);
        // Its own row first (a hidden one, or one linked before), else the site's of its name.
        let found = sqlx::query!(
            r#"SELECT id, is_hidden FROM characters
               WHERE battlenet_id = $1 OR name_normalized = lower($2)
               ORDER BY battlenet_id = $1 DESC NULLS LAST
               LIMIT 1
               FOR UPDATE"#,
            character.battlenet_id,
            full_name,
        )
        .fetch_optional(&mut *tx)
        .await?;
        match found {
            Some(row) => {
                sqlx::query!(
                    "UPDATE characters
                     SET battlenet_id = $2, battlenet_provider_id = $3, battlenet_subject = $4,
                         battlenet_seen_at = now()
                     WHERE id = $1",
                    row.id,
                    character.battlenet_id,
                    provider_id.0,
                    subject,
                )
                .execute(&mut *tx)
                .await?;
                if row.is_hidden {
                    synced.hidden += 1;
                    // A hidden character is Battle.net's: it follows the listing, and its
                    // account's member. A rename onto a name the site has keeps the old name.
                    sqlx::query!(
                        "UPDATE characters c
                         SET class = $2, race = $3, sex = $4, level = $5::smallint,
                             reached_60_at = CASE WHEN $5::smallint = 60
                                                  THEN COALESCE(c.reached_60_at, now()) END,
                             user_id = $6,
                             first_name = CASE WHEN EXISTS (
                                     SELECT 1 FROM characters t
                                     WHERE t.name_normalized = lower($7 || ' ' || $8)
                                         AND t.id <> c.id
                                 ) THEN c.first_name ELSE $7 END,
                             last_name = CASE WHEN EXISTS (
                                     SELECT 1 FROM characters t
                                     WHERE t.name_normalized = lower($7 || ' ' || $8)
                                         AND t.id <> c.id
                                 ) THEN c.last_name ELSE $8 END
                         WHERE c.id = $1",
                        row.id,
                        character.class,
                        character.race,
                        character.sex as Sex,
                        character.level,
                        owner.map(|id| id.0),
                        character.first_name,
                        character.last_name,
                    )
                    .execute(&mut *tx)
                    .await?;
                } else {
                    synced.linked += 1;
                }
            }
            None => {
                synced.hidden += 1;
                sqlx::query!(
                    "INSERT INTO characters
                         (user_id, first_name, last_name, class, race, sex, level, reached_60_at,
                          ruleset, is_hidden, battlenet_id, battlenet_provider_id,
                          battlenet_subject, battlenet_seen_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7::smallint, CASE WHEN $7::smallint = 60 THEN now() END,
                             (SELECT ruleset FROM guild_settings), true, $8, $9, $10, now())",
                    owner.map(|id| id.0),
                    character.first_name,
                    character.last_name,
                    character.class,
                    character.race,
                    character.sex as Sex,
                    character.level,
                    character.battlenet_id,
                    provider_id.0,
                    subject,
                )
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    synced.removed = sqlx::query!(
        "DELETE FROM characters
         WHERE battlenet_provider_id = $1 AND battlenet_subject = $2 AND is_hidden
           AND NOT (battlenet_id = ANY($3))",
        provider_id.0,
        subject,
        &ids,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query!(
        "UPDATE characters
         SET battlenet_id = NULL, battlenet_provider_id = NULL, battlenet_subject = NULL,
             battlenet_seen_at = NULL
         WHERE battlenet_provider_id = $1 AND battlenet_subject = $2
           AND NOT (battlenet_id = ANY($3))",
        provider_id.0,
        subject,
        &ids,
    )
    .execute(&mut *tx)
    .await?;
    synced.claimed = claim_in(&mut tx, provider_id, subject, owner).await?;
    tx.commit().await?;
    Ok(synced)
}

/// The member the Battle.net account is linked to, if any (none yet for a first sign-in still
/// choosing a username).
async fn owner(
    conn: &mut PgConnection,
    provider_id: ProviderId,
    subject: &str,
) -> sqlx::Result<Option<UserId>> {
    sqlx::query_scalar!(
        r#"SELECT user_id AS "user_id: UserId" FROM auth_oauth2_provider_credentials
           WHERE provider_id = $1 AND subject = $2 AND disconnected_at IS NULL"#,
        provider_id.0,
        subject,
    )
    .fetch_optional(&mut *conn)
    .await
}

/// Gives the account's characters nobody plays to its member, now that it has one (after a first
/// sign-in's username). Returns how many it gave.
pub async fn claim(pool: &PgPool, provider_id: ProviderId, subject: &str) -> sqlx::Result<u64> {
    let mut tx = pool.begin().await?;
    let owner = owner(&mut tx, provider_id, subject).await?;
    let claimed = claim_in(&mut tx, provider_id, subject, owner).await?;
    tx.commit().await?;
    Ok(claimed)
}

/// [`claim`] in a transaction. A member left without a main gets the highest level of their
/// characters on the site as one.
async fn claim_in(
    conn: &mut PgConnection,
    provider_id: ProviderId,
    subject: &str,
    owner: Option<UserId>,
) -> sqlx::Result<u64> {
    let Some(owner) = owner else {
        return Ok(0);
    };
    let claimed = sqlx::query!(
        "UPDATE characters SET user_id = $3
         WHERE battlenet_provider_id = $1 AND battlenet_subject = $2 AND user_id IS NULL",
        provider_id.0,
        subject,
        owner.0,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if claimed > 0 {
        sqlx::query!(
            "UPDATE characters SET is_main = true
             WHERE id = (
                 SELECT id FROM characters WHERE user_id = $1 AND NOT is_hidden
                 ORDER BY level DESC, id LIMIT 1
             )
             AND NOT EXISTS (SELECT 1 FROM characters WHERE user_id = $1 AND is_main)",
            owner.0,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(claimed)
}

/// A character a Battle.net listing linked or added, for the superusers' page.
#[derive(Debug, serde::Serialize)]
pub struct Listed {
    pub id: i64,
    /// The Battle.net account's BattleTag, when it gave one.
    pub account: Option<String>,
    /// Who plays it on the site, if anyone.
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    pub race: Option<String>,
    pub sex: Option<Sex>,
    pub ruleset: Ruleset,
    pub level: i16,
    /// Seen by superusers only, until an officer adds a character of its name.
    pub is_hidden: bool,
    /// When Battle.net last listed it; `None` once it stopped.
    #[serde(with = "iso8601::option")]
    pub seen_at: Option<OffsetDateTime>,
}

/// Every character a Battle.net listing linked or added, by member, then highest level first.
pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<Listed>> {
    let rows = sqlx::query!(
        r#"SELECT c.id, c.first_name, c.last_name, c.class, c.race, c.sex AS "sex: Sex",
                  c.ruleset AS "ruleset: Ruleset", c.level, c.is_hidden, c.battlenet_seen_at,
                  cr.raw_userinfo AS "raw_userinfo?", u.username AS "username?"
           FROM characters c
           LEFT JOIN auth_oauth2_provider_credentials cr
               ON cr.provider_id = c.battlenet_provider_id AND cr.subject = c.battlenet_subject
           LEFT JOIN users u ON u.id = c.user_id
           WHERE c.battlenet_id IS NOT NULL OR c.is_hidden
           ORDER BY u.username_normalized NULLS LAST, c.battlenet_subject, c.level DESC,
                    c.name_normalized"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Listed {
            id: row.id,
            account: row
                .raw_userinfo
                .as_ref()
                .and_then(crate::auth::oauth2::providers::identity_label),
            username: row.username,
            first_name: row.first_name,
            last_name: row.last_name,
            class: row.class,
            race: row.race,
            sex: row.sex,
            ruleset: row.ruleset,
            level: row.level,
            is_hidden: row.is_hidden,
            seen_at: row.battlenet_seen_at,
        })
        .collect())
}

/// `GET /api/admin/battlenet-characters`: the characters Battle.net listings linked or added,
/// hidden ones too. Superusers only.
async fn list_all(state: State, _admin: Admin) -> Result<Json<Vec<Listed>>> {
    Ok(Json(list(&state.pool).await?))
}

pub fn admin_router() -> Router<State> {
    Router::new().route("/battlenet-characters", get(list_all))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::characters::tests::{add, input};
    use crate::guild::Rank;
    use crate::guild::tests::member;

    /// The Battle.net provider, `subject` linked to `user_id` (when given) with a token row.
    async fn account(pool: &PgPool, subject: &str, user_id: Option<UserId>) -> ProviderId {
        let holder = match user_id {
            Some(id) => id,
            None => member(pool, "Holder", Rank::Member).await.id,
        };
        crate::auth::oauth2::providers::link_for_test(pool, "battlenet", subject, holder).await;
        let provider_id = sqlx::query_scalar!(
            "SELECT p.provider_id FROM auth_oauth2_providers p
             JOIN auth_providers a ON a.id = p.provider_id WHERE a.slug = 'battlenet'"
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if user_id.is_none() {
            sqlx::query!(
                "DELETE FROM auth_oauth2_provider_credentials
                 WHERE provider_id = $1 AND subject = $2",
                provider_id,
                subject,
            )
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query!(
            "INSERT INTO auth_oauth2_provider_tokens
                 (provider_id, subject, access_token_ciphertext, scope)
             VALUES ($1, $2, 'token', 'openid wow.profile')",
            provider_id,
            subject,
        )
        .execute(pool)
        .await
        .unwrap();
        ProviderId(provider_id)
    }

    fn listed(id: u64, name: &str, surname: &str, level: u32) -> Character {
        Character {
            name: name.into(),
            surname: surname.into(),
            realm: String::new(),
            realm_slug: String::new(),
            level,
            class: "Warrior".into(),
            race: "Windshaper Skyborne".into(),
            faction: "Horde".into(),
            gender: "Male".into(),
            id,
            wow_account_id: 7,
        }
    }

    async fn unowned(pool: &PgPool, first: &str, last: &str) -> i64 {
        sqlx::query_scalar!(
            "INSERT INTO characters (first_name, last_name, class) VALUES ($1, $2, 'warrior')
             RETURNING id",
            first,
            last,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    struct Row {
        user_id: Option<i64>,
        is_main: bool,
        is_hidden: bool,
        level: i16,
    }

    async fn row(pool: &PgPool, id: i64) -> Row {
        sqlx::query_as!(
            Row,
            "SELECT user_id, is_main, is_hidden, level FROM characters WHERE id = $1",
            id
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn links_the_sites_characters_and_hides_the_rest(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let other = member(&pool, "Other", Rank::Raider).await;
        let provider = account(&pool, "ey#1", Some(ey.id)).await;
        let gg = unowned(&pool, "Eyl", "Gg").await;
        let taken = add(&pool, &other, input("Eyl", "Taken", "warrior", true))
            .await
            .unwrap();

        let characters = [
            listed(1, "Eyl", "Gg", 12),
            listed(2, "EYL", "taken", 5),
            listed(3, "Eyl", "Shm", 10),
            listed(4, "Bad1", "Name", 1),
        ];
        let synced = apply(&pool, provider, "ey#1", &characters).await.unwrap();
        assert_eq!(
            synced,
            Synced {
                listed: 4,
                linked: 2,
                hidden: 1,
                // Gg; Shm is added already theirs.
                claimed: 1,
                removed: 0
            }
        );

        // The unclaimed site character is now Ey's, and their main; it keeps its level.
        let gg_row = row(&pool, gg).await;
        assert_eq!(gg_row.user_id, Some(ey.id.0));
        assert!(gg_row.is_main && !gg_row.is_hidden);
        assert_eq!(gg_row.level, 1);
        // Another member's stays theirs.
        assert_eq!(row(&pool, taken.id).await.user_id, Some(other.id.0));

        // The new one is hidden: Ey's, but nowhere on the site.
        let all = list(&pool).await.unwrap();
        assert_eq!(all.len(), 3, "the bad name is skipped");
        let shm = all.iter().find(|c| c.last_name == "Shm").unwrap();
        assert!(shm.is_hidden);
        assert_eq!(shm.username.as_deref(), Some("Ey"));
        assert_eq!(shm.race.as_deref(), Some("windshaper-skyborne"));
        assert_eq!(shm.sex, Some(Sex::Male));
        assert_eq!(shm.ruleset, Ruleset::Pvp);
        assert!(!row(&pool, shm.id).await.is_main);
        let site = crate::characters::list(&pool).await.unwrap();
        assert!(site.iter().all(|c| c.last_name != "Shm"));
        assert!(
            crate::characters::find_by_slug(&pool, "eyl-shm")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn a_new_listing_updates_and_removes_hidden_ones(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let provider = account(&pool, "ey#1", Some(ey.id)).await;
        let gg = unowned(&pool, "Eyl", "Gg").await;
        apply(
            &pool,
            provider,
            "ey#1",
            &[listed(1, "Eyl", "Gg", 1), listed(2, "Eyl", "Old", 1)],
        )
        .await
        .unwrap();
        let synced = apply(&pool, provider, "ey#1", &[listed(2, "Eyl", "Old", 60)])
            .await
            .unwrap();
        assert_eq!(synced.removed, 0);
        let all = list(&pool).await.unwrap();
        assert_eq!(all.len(), 1, "the site character is unlinked, not removed");
        assert_eq!(all[0].level, 60);
        assert!(row(&pool, gg).await.user_id.is_some());

        let synced = apply(&pool, provider, "ey#1", &[]).await.unwrap();
        assert_eq!(synced.removed, 1);
        assert!(list(&pool).await.unwrap().is_empty());
    }

    #[sqlx::test]
    async fn an_officer_adding_the_name_brings_it_out(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let provider = account(&pool, "ey#1", Some(ey.id)).await;
        apply(&pool, provider, "ey#1", &[listed(1, "Eyl", "Shm", 10)])
            .await
            .unwrap();
        let id = list(&pool).await.unwrap()[0].id;

        let shown = add(&pool, &ey, input("eyl", "shm", "shaman", false))
            .await
            .unwrap();
        assert_eq!(shown.id, id);
        assert_eq!(shown.class, "shaman");
        assert_eq!(shown.level, 10);
        assert!(shown.is_main, "their first character on the site");
        assert!(!list(&pool).await.unwrap()[0].is_hidden);
    }

    #[sqlx::test]
    async fn a_first_sign_in_waits_for_its_member(pool: PgPool) {
        // Still choosing a username: no member to give characters to yet.
        let provider = account(&pool, "new#1", None).await;
        let synced = apply(&pool, provider, "new#1", &[listed(1, "Eyl", "Gg", 12)])
            .await
            .unwrap();
        assert_eq!(synced.claimed, 0);
        assert!(list(&pool).await.unwrap()[0].username.is_none());

        let ey = member(&pool, "Ey", Rank::Raider).await;
        crate::auth::oauth2::providers::link_for_test(&pool, "battlenet", "new#1", ey.id).await;
        assert_eq!(claim(&pool, provider, "new#1").await.unwrap(), 1);
        assert_eq!(
            list(&pool).await.unwrap()[0].username.as_deref(),
            Some("Ey")
        );
    }

    #[sqlx::test]
    async fn hidden_characters_never_raid(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let provider = account(&pool, "ey#1", Some(ey.id)).await;
        apply(&pool, provider, "ey#1", &[listed(1, "Eyl", "Shm", 10)])
            .await
            .unwrap();
        let id = list(&pool).await.unwrap()[0].id;
        let raid = sqlx::query_scalar!(
            "INSERT INTO raids (zone, starts_local, time_zone)
             VALUES ((SELECT slug FROM raid_zones LIMIT 1), now()::timestamp, 'America/New_York')
             RETURNING id"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let error = sqlx::query!(
            "INSERT INTO raid_attendees (raid_id, character_id, group_number, slot)
             VALUES ($1, $2, 1, 1)",
            raid,
            id,
        )
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().constraint(),
            Some("raid_attendees_character_id_fkey")
        );
    }

    #[test]
    fn only_profile_scopes_read_the_profile() {
        assert!(reads_profile("openid wow.profile"));
        assert!(!reads_profile("openid identify"));
        assert!(!reads_profile("wow.profile.extra"));
    }
}
