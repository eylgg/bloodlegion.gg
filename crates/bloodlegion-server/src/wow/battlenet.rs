//! The WoW: Forever characters on each member's Battle.net accounts, read from the profile API at
//! every sign-in into the site's own characters. A listed character with a site character's
//! Battle.net id or name is linked to it, which goes to the account's member when nobody plays it
//! yet; any other is added as the member's. Either way it is on the roster only once a roster
//! upload names it (in the guild). Characters the account no longer lists are unlinked, and removed
//! when no roster upload ever named them and they have no raid or loot.

use anyhow::Context;
use axum::extract::Path;
use axum::routing::{get, post};
use axum::{Json, Router};
use sqlx::{PgConnection, PgPool};
use time::{OffsetDateTime, serde::iso8601};

use super::{Character, Ruleset};
use crate::auth::providers::ProviderId;
use crate::characters::Sex;
use crate::extract::{Admin, SameOrigin};
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
                added = synced.added,
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
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Synced {
    /// Characters Battle.net listed.
    pub listed: usize,
    /// Of those, the ones on the site (linked to its character of that name).
    pub linked: usize,
    /// Of those, the ones the site did not have, added as the member's.
    pub added: usize,
    /// Characters that went to the account's member.
    pub claimed: u64,
    /// Characters removed, no longer listed: never named by a roster upload, with no raid or loot.
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
/// name (raising its level, and filling in a race and sex nobody recorded), adds the rest, removes
/// the ones no longer listed that no roster upload ever named and that have no raid or loot (and
/// unlinks the rest
/// from the account, keeping the character's own id, which is the game's), then gives the
/// account's characters nobody plays to its member ([`claim`]).
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
        // Its own row first (one linked before), else the site's of its name.
        let found = sqlx::query_scalar!(
            r#"SELECT id FROM characters
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
            Some(id) => {
                synced.linked += 1;
                sqlx::query!(
                    "UPDATE characters
                     SET battlenet_id = $2, battlenet_provider_id = $3, battlenet_subject = $4,
                         battlenet_seen_at = now(),
                         race = COALESCE(race, $5), sex = COALESCE(sex, $6),
                         level = GREATEST(level, $7::smallint),
                         reached_60_at = CASE WHEN GREATEST(level, $7::smallint) = 60
                                              THEN COALESCE(reached_60_at, now()) END
                     WHERE id = $1",
                    id,
                    character.battlenet_id,
                    provider_id.0,
                    subject,
                    character.race,
                    character.sex as Sex,
                    character.level,
                )
                .execute(&mut *tx)
                .await?;
            }
            None => {
                synced.added += 1;
                sqlx::query!(
                    "INSERT INTO characters
                         (user_id, first_name, last_name, class, race, sex, level, reached_60_at,
                          ruleset, battlenet_id, battlenet_provider_id, battlenet_subject,
                          battlenet_seen_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7::smallint, CASE WHEN $7::smallint = 60 THEN now() END,
                             (SELECT ruleset FROM guild_settings), $8, $9, $10, now())",
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
    // Gone from the account (deleted or moved in game): never in the guild and with nothing to its
    // name, it goes; anything else only loses the link.
    synced.removed = sqlx::query!(
        "DELETE FROM characters c
         WHERE c.battlenet_provider_id = $1 AND c.battlenet_subject = $2
           AND NOT (c.battlenet_id = ANY($3))
           AND NOT in_guild(c.guild)
           AND NOT EXISTS (SELECT 1 FROM roster_members m WHERE m.character_id = c.id)
           AND NOT EXISTS (SELECT 1 FROM raid_attendees r WHERE r.character_id = c.id)
           AND NOT EXISTS (SELECT 1 FROM loot l WHERE l.character_id = c.id)
           AND NOT EXISTS (SELECT 1 FROM loot_priority_characters p WHERE p.character_id = c.id)",
        provider_id.0,
        subject,
        &ids,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query!(
        "UPDATE characters
         SET battlenet_provider_id = NULL, battlenet_subject = NULL, battlenet_seen_at = NULL
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

/// [`claim`] in a transaction. A member left without a main gets one: their highest level in the
/// guild, else their highest level.
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
                 SELECT id FROM characters WHERE user_id = $1
                 ORDER BY in_guild(guild) DESC, level DESC, id LIMIT 1
             )
             AND NOT EXISTS (SELECT 1 FROM characters WHERE user_id = $1 AND is_main)",
            owner.0,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(claimed)
}

/// A character a Battle.net sign-in linked to its account or added, for the superusers' page. Only a
/// sign-in says which account a character is on: a roster upload's id (from the in-game GUID) is the
/// character's alone, and an officer giving it to a member is the site's say, not Battle.net's.
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
    /// Named by the guild's latest roster upload.
    pub in_guild: bool,
    /// When Battle.net last listed it; `None` once it stopped.
    #[serde(with = "iso8601::option")]
    pub seen_at: Option<OffsetDateTime>,
}

/// Every character a Battle.net listing linked or added, by member, then highest level first.
pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<Listed>> {
    let rows = sqlx::query!(
        r#"SELECT c.id, c.first_name, c.last_name, c.class, c.race, c.sex AS "sex: Sex",
                  c.ruleset AS "ruleset: Ruleset", c.level, in_guild(c.guild) AS "in_guild!", c.battlenet_seen_at,
                  cr.raw_userinfo AS "raw_userinfo?", u.username AS "username?"
           FROM characters c
           LEFT JOIN auth_oauth2_provider_credentials cr
               ON cr.provider_id = c.battlenet_provider_id AND cr.subject = c.battlenet_subject
           LEFT JOIN users u ON u.id = c.user_id
           WHERE c.battlenet_subject IS NOT NULL
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
            in_guild: row.in_guild,
            seen_at: row.battlenet_seen_at,
        })
        .collect())
}

/// `GET /api/admin/battlenet-characters`: the characters Battle.net listings linked or added.
/// Superusers only.
async fn list_all(state: State, _admin: Admin) -> Result<Json<Vec<Listed>>> {
    Ok(Json(list(&state.pool).await?))
}

/// A member a Battle.net sign-in never read characters for, with their signed-in sessions.
#[derive(Debug, PartialEq, Eq)]
pub struct Unread {
    pub user_id: UserId,
    pub username: String,
    /// The connected Battle.net account's BattleTag, when it gave one.
    pub account: Option<String>,
    /// Their sessions still signed in.
    pub sessions: i64,
}

/// The members with a Battle.net account connected that no sign-in has read characters from
/// (they signed in before the read went live, or have none on WoW: Forever), with how many
/// sessions they still have signed in.
pub async fn unread(pool: &PgPool) -> sqlx::Result<Vec<Unread>> {
    let rows = sqlx::query!(
        r#"
        SELECT u.id AS "user_id: UserId", u.username, cr.raw_userinfo,
               (SELECT count(*) FROM auth_sessions s
                WHERE s.user_id = u.id AND s.revoked_at IS NULL AND s.expires_at > now())
                   AS "sessions!"
        FROM auth_oauth2_provider_credentials cr
        JOIN auth_providers p ON p.id = cr.provider_id
        JOIN users u ON u.id = cr.user_id
        WHERE p.slug = 'battlenet' AND cr.disconnected_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM characters c
              WHERE c.battlenet_provider_id = cr.provider_id
                AND c.battlenet_subject = cr.subject
          )
        ORDER BY u.username_normalized
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Unread {
            user_id: row.user_id,
            username: row.username,
            account: crate::auth::oauth2::providers::identity_label(&row.raw_userinfo),
            sessions: row.sessions,
        })
        .collect())
}

/// Signs out every session of the members [`unread`] lists, so their next visit is a fresh
/// Battle.net sign-in, which reads their characters. How many sessions it signed out.
pub async fn sign_out_unread(pool: &PgPool) -> sqlx::Result<u64> {
    let users: Vec<i64> = unread(pool)
        .await?
        .into_iter()
        .map(|u| u.user_id.0)
        .collect();
    Ok(sqlx::query!(
        "UPDATE auth_sessions SET revoked_at = now()
         WHERE user_id = ANY($1) AND revoked_at IS NULL",
        &users,
    )
    .execute(pool)
    .await?
    .rows_affected())
}

/// One Battle.net account's read, for a superuser's [`sync_user`].
#[derive(Debug, serde::Serialize)]
pub struct AccountRead {
    /// Its BattleTag, when it gave one.
    pub account: Option<String>,
    /// What the read did, when it worked.
    pub synced: Option<Synced>,
    /// Why it did not, when it failed (an expired token, the profile scope never granted).
    pub error: Option<String>,
}

/// Reads a member's characters from each Battle.net account they linked, now, with the tokens their
/// latest sign-in left (Battle.net's last a day). Empty when there are none: they linked before
/// tokens were kept, or have no Battle.net account linked.
pub async fn sync_user(state: &State, user_id: UserId) -> anyhow::Result<Vec<AccountRead>> {
    let slug = crate::Slug::try_from("battlenet").context("the battlenet slug")?;
    let accounts = crate::auth::oauth2::providers::tokens_for_user(state, user_id, &slug).await?;
    let now = OffsetDateTime::now_utc();
    let mut reads = Vec::new();
    for tokens in accounts {
        let result = match unusable(&tokens.scope, tokens.expires_at, now) {
            Some(reason) => Err(reason),
            None => sync(
                state,
                tokens.provider_id,
                &tokens.subject,
                &tokens.access_token,
            )
            .await
            .map_err(|error| format!("{error:#}")),
        };
        let (synced, error) = match result {
            Ok(synced) => (Some(synced), None),
            Err(error) => (None, Some(error)),
        };
        reads.push(AccountRead {
            account: tokens.identity,
            synced,
            error,
        });
    }
    Ok(reads)
}

/// Why a sign-in's token cannot read the profile, if it cannot: the scope was never granted, or it
/// expired.
fn unusable(
    scope: &str,
    expires_at: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> Option<String> {
    if !reads_profile(scope) {
        return Some(format!(
            "Their sign-in did not grant the WoW profile (scope: {scope})."
        ));
    }
    let at = expires_at.filter(|at| *at <= now)?;
    Some(format!(
        "The token from their last sign-in expired on {}. They need to sign in with Battle.net \
         again.",
        at.date()
    ))
}

/// `POST /api/admin/users/{id}/battlenet-sync`: [`sync_user`]. Superusers only.
async fn sync_one(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Path(id): Path<UserId>,
) -> Result<Json<Vec<AccountRead>>> {
    Ok(Json(sync_user(&state, id).await?))
}

pub fn admin_router() -> Router<State> {
    Router::new()
        .route("/battlenet-characters", get(list_all))
        .route("/users/{id}/battlenet-sync", post(sync_one))
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
        level: i16,
    }

    async fn row(pool: &PgPool, id: i64) -> Row {
        sqlx::query_as!(
            Row,
            "SELECT user_id, is_main, level FROM characters WHERE id = $1",
            id
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn links_the_sites_characters_and_adds_the_rest(pool: PgPool) {
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
                added: 1,
                // Gg; Shm is added already theirs.
                claimed: 1,
                removed: 0
            }
        );

        // The unclaimed site character is now Ey's, their main, at Battle.net's level.
        let gg_row = row(&pool, gg).await;
        assert_eq!(gg_row.user_id, Some(ey.id.0));
        assert!(gg_row.is_main);
        assert_eq!(gg_row.level, 12);
        // Another member's stays theirs.
        assert_eq!(row(&pool, taken.id).await.user_id, Some(other.id.0));

        // The new one is Ey's, on their page, and not in the guild until an upload names it.
        let all = list(&pool).await.unwrap();
        assert_eq!(all.len(), 3, "the bad name is skipped");
        let shm = all.iter().find(|c| c.last_name == "Shm").unwrap();
        assert!(!shm.in_guild);
        assert_eq!(shm.username.as_deref(), Some("Ey"));
        assert_eq!(shm.race.as_deref(), Some("windshaper-skyborne"));
        assert_eq!(shm.sex, Some(Sex::Male));
        assert_eq!(shm.ruleset, Ruleset::Pvp);
        assert!(!row(&pool, shm.id).await.is_main);
        let page = crate::characters::find_by_slug(&pool, "eyl-shm")
            .await
            .unwrap()
            .unwrap();
        assert!(!page.in_guild);
    }

    #[sqlx::test]
    async fn a_new_listing_unlinks_or_removes_the_ones_gone(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let provider = account(&pool, "ey#1", Some(ey.id)).await;
        let gg = unowned(&pool, "Eyl", "Gg").await;
        // Gg is in the guild: a roster upload named it.
        let upload = sqlx::query_scalar!(
            "INSERT INTO roster_uploads (user_id, guild, members, matched, unmatched,
                                         records_members, records_ranks)
             VALUES ($1, 'BL', 1, 1, '[]', true, true) RETURNING id",
            ey.id.0,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO roster_members (upload_id, character_id, rank) VALUES ($1, $2, 4)",
            upload,
            gg,
        )
        .execute(&pool)
        .await
        .unwrap();
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
        assert_eq!(
            all.len(),
            1,
            "the roster's character is unlinked, not removed"
        );
        let kept = sqlx::query_scalar!("SELECT battlenet_id FROM characters WHERE id = $1", gg)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(kept, Some(1), "it keeps the game's id for roster matching");
        assert_eq!(all[0].level, 60);
        assert!(row(&pool, gg).await.user_id.is_some());

        // Old, never in the guild and with nothing to its name, goes.
        let synced = apply(&pool, provider, "ey#1", &[]).await.unwrap();
        assert_eq!(synced.removed, 1);
        assert!(list(&pool).await.unwrap().is_empty());
        assert!(
            crate::characters::find_by_slug(&pool, "eyl-old")
                .await
                .unwrap()
                .is_none()
        );
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

    async fn session(pool: &PgPool, user_id: UserId, n: usize) {
        sqlx::query!(
            "INSERT INTO auth_sessions (token_hash, user_id, ip_address)
             VALUES ($1, $2, '127.0.0.1')",
            format!("{n:0>43}"),
            user_id.0,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test]
    async fn signs_out_members_no_sign_in_read(pool: PgPool) {
        let read = member(&pool, "Read", Rank::Raider).await;
        let unread_member = member(&pool, "Unread", Rank::Raider).await;
        let other = member(&pool, "Other", Rank::Raider).await;
        let provider = account(&pool, "read#1", Some(read.id)).await;
        account(&pool, "unread#1", Some(unread_member.id)).await;
        apply(&pool, provider, "read#1", &[listed(1, "Eyl", "Gg", 10)])
            .await
            .unwrap();
        for (n, user) in [&read, &unread_member, &unread_member, &other]
            .into_iter()
            .enumerate()
        {
            session(&pool, user.id, n).await;
        }

        let found = unread(&pool).await.unwrap();
        assert_eq!(
            found
                .iter()
                .map(|u| (u.username.as_str(), u.sessions))
                .collect::<Vec<_>>(),
            [("Unread", 2)],
            "a member without Battle.net is not listed"
        );
        assert_eq!(sign_out_unread(&pool).await.unwrap(), 2);
        assert_eq!(unread(&pool).await.unwrap()[0].sessions, 0);
        let active = |id: UserId| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar!(
                    r#"SELECT count(*) AS "n!" FROM auth_sessions
                       WHERE user_id = $1 AND revoked_at IS NULL"#,
                    id.0
                )
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };
        assert_eq!((active(read.id).await, active(other.id).await), (1, 1));
    }

    #[test]
    fn expired_or_unscoped_tokens_cannot_read() {
        let now = OffsetDateTime::now_utc();
        let day = time::Duration::days(1);
        assert_eq!(unusable("openid wow.profile", None, now), None);
        assert_eq!(unusable("openid wow.profile", Some(now + day), now), None);
        assert!(
            unusable("openid wow.profile", Some(now - day), now)
                .unwrap()
                .contains("sign in with Battle.net again")
        );
        assert!(
            unusable("openid", None, now)
                .unwrap()
                .contains("scope: openid")
        );
    }

    #[sqlx::test]
    async fn only_sign_ins_link_an_account(pool: PgPool) {
        let ey = member(&pool, "Ey", Rank::Raider).await;
        let provider = account(&pool, "ey#1", Some(ey.id)).await;
        apply(&pool, provider, "ey#1", &[listed(1, "Eyl", "Shm", 10)])
            .await
            .unwrap();
        // A roster upload's id, and an officer's say: neither is a Battle.net account.
        let zx = unowned(&pool, "Zx", "Forever").await;
        sqlx::query!(
            "UPDATE characters SET battlenet_id = 99, user_id = $2 WHERE id = $1",
            zx,
            ey.id.0
        )
        .execute(&pool)
        .await
        .unwrap();
        let all = list(&pool).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].last_name, "Shm");
        assert!(all[0].seen_at.is_some());
    }

    #[test]
    fn only_profile_scopes_read_the_profile() {
        assert!(reads_profile("openid wow.profile"));
        assert!(!reads_profile("openid identify"));
        assert!(!reads_profile("wow.profile.extra"));
    }
}
