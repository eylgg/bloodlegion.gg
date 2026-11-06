//! The armory: Battle.net's public profile of a WoW: Forever character, by its name
//! (`/profile/wow/character/eyl-gg`, read with the site's own client credentials token), which
//! says what the guild list does not: its race, sex, and ruleset.

use anyhow::Context;
use serde_json::Value;

use super::Ruleset;
use crate::State;
use crate::characters::Sex;

/// Battle.net's token endpoint, for the site's own (client credentials) token.
const TOKEN_ENDPOINT: &str = "https://oauth.battle.net/token";

/// A token for the profile API as the site itself (RFC 6749 section 4.4), from the Battle.net
/// client members sign in through.
pub async fn app_token(state: &State) -> anyhow::Result<String> {
    let slug = crate::Slug::try_from("battlenet").context("the battlenet slug")?;
    let (client_id, secret) = crate::auth::oauth2::providers::client_credentials(state, &slug)
        .await?
        .context("there is no battlenet sign-in provider to read the armory with")?;
    #[derive(serde::Deserialize)]
    struct Token {
        access_token: String,
    }
    let token: Token = state
        .http_client
        .post(TOKEN_ENDPOINT)
        .basic_auth(client_id, Some(secret))
        .form(&[("grant_type", "client_credentials")])
        .send()
        .await
        .context("requesting a Battle.net token")?
        .error_for_status()
        .context("requesting a Battle.net token")?
        .json()
        .await
        .context("reading the Battle.net token")?;
    Ok(token.access_token)
}

/// The character's profile as the API answered it, by its page name (`eyl-gg`); `None` when the
/// armory has no such character.
pub async fn profile(
    http: &reqwest::Client,
    token: &str,
    slug: &str,
) -> anyhow::Result<Option<Value>> {
    let mut url = url::Url::parse(&format!(
        "https://{}.api.blizzard.com/profile/wow/character/",
        super::battlenet::REGION
    ))
    .context("building the armory URL")?;
    url.path_segments_mut()
        .map_err(|()| anyhow::anyhow!("the armory URL takes no path"))?
        .pop_if_empty()
        .push(slug);
    url.query_pairs_mut()
        .append_pair(
            "namespace",
            &super::profile_namespace(super::battlenet::REGION),
        )
        .append_pair("locale", "en_US");
    let response = http
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .context("calling the armory")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status().context("calling the armory")?;
    response
        .json()
        .await
        .map(Some)
        .context("reading the armory's answer")
}

/// What the armory says of a character, in the site's terms; each part `None` when it says
/// nothing the site knows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Armory {
    pub level: Option<i16>,
    /// The class's slug.
    pub class: Option<&'static str>,
    /// The race's slug.
    pub race: Option<&'static str>,
    pub sex: Option<Sex>,
    pub ruleset: Option<Ruleset>,
    /// The guild it is in, by name.
    pub guild: Option<String>,
}

/// A ruleset by the armory's name or slug for it ("PvP", "RP-PvP", "roleplay-pvp").
fn ruleset(name: &str) -> Option<Ruleset> {
    let name = name.trim().to_lowercase().replace([' ', '_'], "-");
    Some(match name.as_str() {
        "normal" | "pve" => Ruleset::Normal,
        "pvp" => Ruleset::Pvp,
        "roleplay" | "rp" => Ruleset::Roleplay,
        "roleplay-pvp" | "rp-pvp" | "rppvp" => Ruleset::RoleplayPvp,
        "hardcore" => Ruleset::Hardcore,
        _ => return None,
    })
}

pub fn parse(profile: &Value) -> Armory {
    let text = |path: &[&str]| -> Option<String> {
        let mut node = profile;
        for key in path {
            node = node.get(key)?;
        }
        node.as_str().map(str::to_string)
    };
    let slug_of = |path: &[&str]| text(path).map(|name| name.to_lowercase().replace(' ', "-"));
    Armory {
        level: profile["level"]
            .as_i64()
            .and_then(|level| i16::try_from(level).ok()),
        class: slug_of(&["character_class", "name"])
            .and_then(|slug| super::catalog::find(&slug))
            .map(|class| class.slug),
        race: slug_of(&["race", "name"])
            .and_then(|slug| super::catalog::find_race(&slug))
            .map(|race| race.slug),
        sex: match text(&["gender", "type"]).as_deref() {
            Some("FEMALE") => Some(Sex::Female),
            Some("MALE") => Some(Sex::Male),
            _ => None,
        },
        ruleset: text(&["ruleset", "slug"])
            .or_else(|| text(&["ruleset", "name"]))
            .and_then(|name| ruleset(&name)),
        guild: text(&["guild", "name"]),
    }
}

/// How many armory lookups a check makes at once.
const AT_ONCE: usize = 8;

/// One character as a check found it.
#[derive(Debug, serde::Serialize)]
pub struct Checked {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    pub class: String,
    /// Whether the armory has a character of its name; the rest is empty when it does not.
    pub found: bool,
    pub armory: Option<Armory>,
    /// Where the site and the armory disagree, the site having something.
    pub differences: Vec<Difference>,
    /// What filling in took from the armory (race, sex, level).
    pub filled: Vec<&'static str>,
    /// The lookup failed (not "no such character"): why.
    pub error: Option<String>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Difference {
    pub field: &'static str,
    pub site: String,
    pub armory: String,
}

/// Looks up every character on the site (not the hidden ones) in the armory, and compares them.
/// With `fill`, takes from the armory what the site lacks or has lower: a race or sex not
/// recorded, and a level below the armory's (as a roster would raise it). Anything else that
/// differs (a class, a ruleset, a race or sex already recorded) is only reported, for an officer.
pub async fn check(state: &State, fill: bool) -> anyhow::Result<Vec<Checked>> {
    let token = app_token(state).await?;
    let characters = crate::characters::list(&state.pool)
        .await
        .context("listing the characters")?;
    let limit = std::sync::Arc::new(tokio::sync::Semaphore::new(AT_ONCE));
    let mut lookups = tokio::task::JoinSet::new();
    for (index, character) in characters.iter().enumerate() {
        let slug = format!("{}-{}", character.first_name, character.last_name).to_lowercase();
        let (http, token, limit) = (state.http_client.clone(), token.clone(), limit.clone());
        lookups.spawn(async move {
            let _permit = limit.acquire_owned().await;
            (index, profile(&http, &token, &slug).await)
        });
    }
    let mut found: Vec<_> = std::iter::repeat_with(|| None)
        .take(characters.len())
        .collect();
    while let Some(joined) = lookups.join_next().await {
        let (index, result) = joined.context("an armory lookup panicked")?;
        found[index] = Some(result.map(|profile| profile.as_ref().map(parse)));
    }
    let mut checked = Vec::with_capacity(characters.len());
    for (character, result) in characters.into_iter().zip(found) {
        let mut entry = Checked {
            id: character.id,
            first_name: character.first_name.clone(),
            last_name: character.last_name.clone(),
            class: character.class.clone(),
            found: false,
            armory: None,
            differences: Vec::new(),
            filled: Vec::new(),
            error: None,
        };
        let armory = match result.expect("every lookup answers") {
            Ok(Some(armory)) => armory,
            Ok(None) => {
                checked.push(entry);
                continue;
            }
            Err(error) => {
                entry.error = Some(format!("{error:#}"));
                checked.push(entry);
                continue;
            }
        };
        entry.found = true;
        entry.differences = differences(&character, &armory);
        if fill {
            entry.filled = fill_in(&state.pool, &character, &armory)
                .await
                .context("filling in from the armory")?;
        }
        entry.armory = Some(armory);
        checked.push(entry);
    }
    Ok(checked)
}

#[derive(serde::Deserialize)]
struct CheckInput {
    #[serde(default)]
    fill: bool,
}

/// `POST /api/admin/armory-check`: every character against the armory, filling in what the site
/// lacks when `fill`. Superusers only.
async fn check_all(
    state: State,
    _: crate::extract::SameOrigin,
    _admin: crate::extract::Admin,
    axum::Json(input): axum::Json<CheckInput>,
) -> crate::Result<axum::Json<Vec<Checked>>> {
    Ok(axum::Json(check(&state, input.fill).await?))
}

pub fn admin_router() -> axum::Router<State> {
    axum::Router::new().route("/armory-check", axum::routing::post(check_all))
}

/// Where the site has something the armory disagrees with.
fn differences(character: &crate::characters::Character, armory: &Armory) -> Vec<Difference> {
    let mut found = Vec::new();
    let mut differ = |field, site: Option<String>, armory: Option<String>| {
        if let (Some(site), Some(armory)) = (site, armory)
            && site != armory
        {
            found.push(Difference {
                field,
                site,
                armory,
            });
        }
    };
    differ(
        "class",
        Some(character.class.clone()),
        armory.class.map(str::to_string),
    );
    differ(
        "race",
        character.race.clone(),
        armory.race.map(str::to_string),
    );
    differ(
        "sex",
        character.sex.map(|sex| name(&sex)),
        armory.sex.map(|sex| name(&sex)),
    );
    differ(
        "ruleset",
        Some(name(&character.ruleset)),
        armory.ruleset.map(|ruleset| name(&ruleset)),
    );
    // Only a level the armory has lower: a higher one is the site behind, which filling fixes.
    if armory.level.is_some_and(|level| level < character.level) {
        differ(
            "level",
            Some(character.level.to_string()),
            armory.level.map(|level| level.to_string()),
        );
    }
    found
}

/// Serde's name for a value (`"pvp"`, `"female"`).
fn name<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Takes the race and sex when the site has none, and the level when the site's is lower.
async fn fill_in(
    pool: &sqlx::PgPool,
    character: &crate::characters::Character,
    armory: &Armory,
) -> sqlx::Result<Vec<&'static str>> {
    let race = armory.race.filter(|_| character.race.is_none());
    let sex = armory.sex.filter(|_| character.sex.is_none());
    let level = armory
        .level
        .filter(|level| (1..=60).contains(level) && *level > character.level);
    let mut filled = Vec::new();
    if race.is_some() {
        filled.push("race");
    }
    if sex.is_some() {
        filled.push("sex");
    }
    if level.is_some() {
        filled.push("level");
    }
    if filled.is_empty() {
        return Ok(filled);
    }
    sqlx::query!(
        r#"UPDATE characters
           SET race = COALESCE(race, $2), sex = COALESCE(sex, $3),
               level = GREATEST(level, COALESCE($4, level)),
               reached_60_at = CASE WHEN GREATEST(level, COALESCE($4, level)) = 60
                                    THEN COALESCE(reached_60_at, now()) END
           WHERE id = $1"#,
        character.id,
        race,
        sex as Option<Sex>,
        level,
    )
    .execute(pool)
    .await?;
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_forever_profile() {
        // As the API answered for eyl-gg, trimmed to the fields read.
        let profile = serde_json::json!({
            "name": "Eyl", "surname": "Gg", "level": 14,
            "character_class": {"id": 1, "name": "Warrior"},
            "race": {"id": 96, "name": "Windshaper Skyborne"},
            "gender": {"type": "FEMALE", "name": "Female"},
            "ruleset": {"id": 1, "name": "PvP"},
            "guild": {"name": "BL", "slug": "bl"}
        });
        assert_eq!(
            parse(&profile),
            Armory {
                level: Some(14),
                class: Some("warrior"),
                race: Some("windshaper-skyborne"),
                sex: Some(Sex::Female),
                ruleset: Some(Ruleset::Pvp),
                guild: Some("BL".into()),
            }
        );
        assert_eq!(ruleset("RP-PvP"), Some(Ruleset::RoleplayPvp));
        assert_eq!(ruleset("Normal"), Some(Ruleset::Normal));
        assert_eq!(ruleset("Mars"), None);
        assert_eq!(parse(&serde_json::json!({})).race, None);
    }
}
