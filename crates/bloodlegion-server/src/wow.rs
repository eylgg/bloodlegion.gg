//! Battle.net's World of Warcraft profile API, called with a person's own access token (granted
//! through the `wow.profile` scope at sign-in).

use anyhow::Context;
use serde_json::Value;

/// One character on a person's WoW account, as the account profile summary lists it.
#[derive(Debug, PartialEq, Eq)]
pub struct Character {
    pub name: String,
    pub realm: String,
    pub realm_slug: String,
    pub level: u32,
    pub class: String,
    pub race: String,
    pub faction: String,
    pub id: u64,
}

/// The regional API host for `region` (`us`, `eu`, `kr`, `tw`). China is a separate service and
/// is not supported.
fn api_host(region: &str) -> String {
    format!("https://{region}.api.blizzard.com")
}

/// `GET /profile/user/wow`: every character on every WoW license of the account the token belongs
/// to. A 401 means the token expired or was revoked; a 403 means `wow.profile` was not granted.
pub async fn account_characters(
    http: &reqwest::Client,
    region: &str,
    locale: &str,
    access_token: &str,
) -> anyhow::Result<Vec<Character>> {
    let mut url = url::Url::parse(&format!("{}/profile/user/wow", api_host(region)))
        .context("building the WoW account profile URL")?;
    url.query_pairs_mut()
        .append_pair("namespace", &format!("profile-{region}"))
        .append_pair("locale", locale);
    let response = http
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .context("calling the WoW account profile API")?;
    let status = response.status();
    if !status.is_success() {
        let hint = match status.as_u16() {
            401 => " (the access token expired or was revoked: sign in again)",
            403 => " (the token lacks the wow.profile scope)",
            404 => " (no WoW license on this account in that region)",
            _ => "",
        };
        anyhow::bail!("the WoW account profile API answered {status}{hint}");
    }
    let body: Value = response
        .json()
        .await
        .context("reading the WoW account profile")?;
    Ok(parse_characters(&body))
}

/// The characters in an account profile summary, highest level first, then by name. Lenient:
/// a field missing from one entry shows as empty rather than failing the whole listing.
fn parse_characters(body: &Value) -> Vec<Character> {
    let text = |value: &Value, path: &[&str]| -> String {
        let mut node = value;
        for key in path {
            node = &node[key];
        }
        node.as_str().unwrap_or_default().to_string()
    };
    let mut characters: Vec<Character> = body["wow_accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|account| account["characters"].as_array().into_iter().flatten())
        .map(|character| Character {
            name: text(character, &["name"]),
            realm: text(character, &["realm", "name"]),
            realm_slug: text(character, &["realm", "slug"]),
            level: character["level"].as_u64().unwrap_or_default() as u32,
            class: text(character, &["playable_class", "name"]),
            race: text(character, &["playable_race", "name"]),
            faction: text(character, &["faction", "name"]),
            id: character["id"].as_u64().unwrap_or_default(),
        })
        .collect();
    characters.sort_by(|a, b| b.level.cmp(&a.level).then_with(|| a.name.cmp(&b.name)));
    characters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_account_profile_summary() {
        // The shape Blizzard documents for `/profile/user/wow` with a `locale` (names are plain
        // strings), trimmed to the fields read.
        let body = serde_json::json!({
            "id": 1,
            "wow_accounts": [
                {
                    "id": 11,
                    "characters": [
                        {
                            "name": "Alt", "id": 2, "level": 70,
                            "realm": {"name": "Area 52", "slug": "area-52", "id": 3676},
                            "playable_class": {"name": "Mage", "id": 8},
                            "playable_race": {"name": "Gnome", "id": 7},
                            "faction": {"type": "ALLIANCE", "name": "Alliance"}
                        },
                        {
                            "name": "Main", "id": 1, "level": 80,
                            "realm": {"name": "Area 52", "slug": "area-52", "id": 3676},
                            "playable_class": {"name": "Warrior", "id": 1},
                            "playable_race": {"name": "Orc", "id": 2},
                            "faction": {"type": "HORDE", "name": "Horde"}
                        }
                    ]
                },
                {"id": 12}
            ]
        });
        let characters = parse_characters(&body);
        assert_eq!(characters.len(), 2);
        assert_eq!(
            characters[0],
            Character {
                name: "Main".into(),
                realm: "Area 52".into(),
                realm_slug: "area-52".into(),
                level: 80,
                class: "Warrior".into(),
                race: "Orc".into(),
                faction: "Horde".into(),
                id: 1,
            }
        );
        assert_eq!(characters[1].name, "Alt");
        assert!(parse_characters(&serde_json::json!({})).is_empty());
    }
}
