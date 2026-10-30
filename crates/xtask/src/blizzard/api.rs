//! The parts of Battle.net's Game Data API the sync reads: item search (the listing), an item's
//! own page (the tooltip), a spell's page, their media (the icon), and Blizzard's render service,
//! which serves the icon images.

use std::collections::HashMap;

use anyhow::Context;
use serde::Deserialize;

/// Where the sync reads from: the US API, in English.
pub const API_HOST: &str = "https://us.api.blizzard.com";
/// WoW: Forever's static data. The API does not serve it yet; until it does, the sync leaves the
/// items and spells as they are (see [`Client::serves_namespace`]).
pub const NAMESPACE: &str = "static-forever-us";
pub const LOCALE: &str = "en_US";

/// Where the render service keeps icons, by name: Classic Era's first, then the older Classic
/// path and the modern game's, which hold the icons Era's lacks (the same images; Era's answers
/// 403 for those).
const RENDER_ICONS: &[&str] = &[
    "https://render.worldofwarcraft.com/classic1x-us/icons/56",
    "https://render.worldofwarcraft.com/classic-us/icons/56",
    "https://render.worldofwarcraft.com/us/icons/56",
];

/// The most results a search page holds; the listing pages by id ranges past it.
pub const SEARCH_PAGE_SIZE: usize = 1000;

/// Where a client credentials token comes from.
pub const TOKEN_ENDPOINT: &str = "https://oauth.battle.net/token";

/// A client with the site's token. Cheap to clone: `reqwest::Client` is a handle.
#[derive(Clone)]
pub struct Client {
    pub http: reqwest::Client,
    pub token: String,
}

#[derive(Debug, Deserialize)]
struct Token {
    access_token: String,
}

/// The HTTP client every call goes through.
pub fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("building the HTTP client")
}

/// The icon `name`'s image from the render service (which needs no token), from the first of
/// [`RENDER_ICONS`] that has it; `None` when none does.
pub async fn render_icon(http: &reqwest::Client, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
    for base in RENDER_ICONS {
        let url = format!("{base}/{name}.jpg");
        let image = retry(|| async {
            let response = http.get(&url).send().await.context("downloading an icon")?;
            if matches!(
                response.status(),
                reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::NOT_FOUND
            ) {
                return Ok(None);
            }
            let bytes = response
                .error_for_status()
                .context("downloading an icon")?
                .bytes()
                .await
                .context("downloading an icon")?;
            Ok(Some(bytes.to_vec()))
        })
        .await?;
        if image.is_some() {
            return Ok(image);
        }
    }
    Ok(None)
}

/// A name in every locale, as search results carry them.
type Localized = HashMap<String, String>;

fn english(names: &Localized) -> String {
    names.get(LOCALE).cloned().unwrap_or_default()
}

#[derive(Debug, Deserialize)]
struct Typed<N> {
    #[serde(rename = "type")]
    kind: String,
    name: N,
}

#[derive(Debug, Deserialize)]
struct Named<N> {
    name: N,
}

/// The fields every item carries, from search (localized names) or its page (English names).
#[derive(Debug, Deserialize)]
struct ItemFields<N> {
    id: i32,
    name: N,
    quality: Typed<N>,
    level: i32,
    required_level: i32,
    inventory_type: Typed<N>,
    item_class: Named<N>,
    item_subclass: Named<N>,
    is_equippable: bool,
}

/// An item's summary, as the snapshot stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub id: i32,
    pub name: String,
    pub quality: String,
    pub item_level: i32,
    pub required_level: i32,
    pub inventory_type: String,
    pub slot: String,
    pub item_class: String,
    pub item_subclass: String,
    pub is_equippable: bool,
}

impl From<ItemFields<Localized>> for Summary {
    fn from(item: ItemFields<Localized>) -> Self {
        Summary {
            id: item.id,
            name: english(&item.name),
            quality: item.quality.kind.to_lowercase(),
            item_level: item.level,
            required_level: item.required_level,
            inventory_type: item.inventory_type.kind.to_lowercase(),
            slot: english(&item.inventory_type.name),
            item_class: english(&item.item_class.name),
            item_subclass: english(&item.item_subclass.name),
            is_equippable: item.is_equippable,
        }
    }
}

impl From<ItemFields<String>> for Summary {
    fn from(item: ItemFields<String>) -> Self {
        Summary {
            id: item.id,
            name: item.name,
            quality: item.quality.kind.to_lowercase(),
            item_level: item.level,
            required_level: item.required_level,
            inventory_type: item.inventory_type.kind.to_lowercase(),
            slot: item.inventory_type.name,
            item_class: item.item_class.name,
            item_subclass: item.item_subclass.name,
            is_equippable: item.is_equippable,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SearchPage {
    results: Vec<SearchResult>,
}

#[derive(Debug, Deserialize)]
struct SearchResult {
    data: ItemFields<Localized>,
}

/// An item's own page: its summary and the tooltip (`preview_item`).
#[derive(Debug)]
pub struct Detail {
    pub summary: Summary,
    pub preview: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ItemPage {
    #[serde(flatten)]
    fields: ItemFields<String>,
    preview_item: serde_json::Value,
}

/// A spell's page: its name and description, in English.
#[derive(Debug, Deserialize)]
pub struct Spell {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Media {
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    key: String,
    value: String,
}

/// An icon's file name and where to download it.
#[derive(Debug, PartialEq, Eq)]
pub struct Icon {
    pub name: String,
    pub url: String,
}

/// The icon's file name from its render URL (`.../icons/56/inv_boots_07.jpg` is `inv_boots_07`).
fn icon_name(url: &str) -> Option<String> {
    let file = url.rsplit('/').next()?;
    let name = file.rsplit_once('.').map_or(file, |(name, _)| name);
    (!name.is_empty()).then(|| name.to_string())
}

/// Tries per call: the first, then two retries.
const ATTEMPTS: u32 = 3;
/// The wait before the first retry, doubling for the next.
const BACKOFF: std::time::Duration = std::time::Duration::from_millis(750);

/// Runs `call` again when it fails in a way a moment's wait can fix: the API stalls (a timeout,
/// mid-body too), drops the connection, answers 5xx (it does, briefly, in bursts), or rate limits
/// (429). Anything else (a 404, a body that is not what was expected) fails at once.
pub async fn retry<T, F, Fut>(mut call: F) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut attempt = 1;
    loop {
        match call().await {
            Err(error) if attempt < ATTEMPTS && is_transient(&error) => {
                tokio::time::sleep(BACKOFF * 2u32.pow(attempt - 1)).await;
                attempt += 1;
            }
            result => return result,
        }
    }
}

fn is_transient(error: &anyhow::Error) -> bool {
    error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<reqwest::Error>())
        .any(|error| {
            error.is_timeout()
                || error.is_connect()
                || error.is_body()
                || error.status().is_some_and(|status| {
                    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                })
        })
}

impl Client {
    /// A client with a client credentials token for the API (RFC 6749 section 4.4), from the
    /// Battle.net client the site signs in with.
    pub async fn new(
        http: reqwest::Client,
        client_id: &str,
        client_secret: &str,
    ) -> anyhow::Result<Client> {
        let token: Token = retry(|| async {
            http.post(TOKEN_ENDPOINT)
                .basic_auth(client_id, Some(client_secret))
                .form(&[("grant_type", "client_credentials")])
                .send()
                .await
                .context("requesting a token")?
                .error_for_status()
                .context("requesting a token")?
                .json()
                .await
                .context("reading the token")
        })
        .await?;
        Ok(Client {
            http,
            token: token.access_token,
        })
    }

    fn url(&self, path: &str) -> anyhow::Result<url::Url> {
        let mut url = url::Url::parse(&format!("{API_HOST}{path}")).context("building a URL")?;
        url.query_pairs_mut().append_pair("namespace", NAMESPACE);
        Ok(url)
    }

    /// Whether the API serves [`NAMESPACE`], from its item class index: a 403 or 404 is no.
    pub async fn serves_namespace(&self) -> anyhow::Result<bool> {
        retry(|| async {
            let response = self
                .http
                .get(self.url("/data/wow/item-class/index")?)
                .bearer_auth(&self.token)
                .send()
                .await
                .context("checking the namespace")?;
            match response.status() {
                reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::NOT_FOUND => Ok(false),
                _ => {
                    response
                        .error_for_status()
                        .context("checking the namespace")?;
                    Ok(true)
                }
            }
        })
        .await
    }

    /// One page of items of `quality` (`EPIC`), by id from `from_id`.
    pub async fn search(&self, quality: &str, from_id: i32) -> anyhow::Result<Vec<Summary>> {
        retry(|| self.search_once(quality, from_id)).await
    }

    async fn search_once(&self, quality: &str, from_id: i32) -> anyhow::Result<Vec<Summary>> {
        let mut url = self.url("/data/wow/search/item")?;
        url.query_pairs_mut()
            .append_pair("quality.type", quality)
            .append_pair("orderby", "id")
            .append_pair("_pageSize", &SEARCH_PAGE_SIZE.to_string())
            .append_pair("id", &format!("[{from_id},]"));
        let page: SearchPage = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("searching items")?
            .error_for_status()
            .with_context(|| "searching items")?
            .json()
            .await
            .context("reading an item search page")?;
        Ok(page.results.into_iter().map(|r| r.data.into()).collect())
    }

    /// The item's page, or `None` when the API does not know the id (a 404).
    pub async fn item(&self, id: i32) -> anyhow::Result<Option<Detail>> {
        retry(|| self.item_once(id)).await
    }

    async fn item_once(&self, id: i32) -> anyhow::Result<Option<Detail>> {
        let mut url = self.url(&format!("/data/wow/item/{id}"))?;
        url.query_pairs_mut().append_pair("locale", LOCALE);
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("fetching an item")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let page: ItemPage = response
            .error_for_status()
            .context("fetching an item")?
            .json()
            .await
            .context("reading an item")?;
        Ok(Some(Detail {
            summary: page.fields.into(),
            preview: page.preview_item,
        }))
    }

    /// The spell's page, or `None` when the API does not know the id (a 404).
    pub async fn spell(&self, id: i32) -> anyhow::Result<Option<Spell>> {
        retry(|| self.spell_once(id)).await
    }

    async fn spell_once(&self, id: i32) -> anyhow::Result<Option<Spell>> {
        let mut url = self.url(&format!("/data/wow/spell/{id}"))?;
        url.query_pairs_mut().append_pair("locale", LOCALE);
        let response = self
            .http
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("fetching a spell")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(
            response
                .error_for_status()
                .context("fetching a spell")?
                .json()
                .await
                .context("reading a spell")?,
        ))
    }

    /// The item's icon, when it has one.
    pub async fn icon(&self, id: i32) -> anyhow::Result<Option<Icon>> {
        retry(|| self.media_icon("item", id)).await
    }

    /// The spell's icon, when the API has one.
    pub async fn spell_icon(&self, id: i32) -> anyhow::Result<Option<Icon>> {
        retry(|| self.media_icon("spell", id)).await
    }

    async fn media_icon(&self, kind: &str, id: i32) -> anyhow::Result<Option<Icon>> {
        let response = self
            .http
            .get(self.url(&format!("/data/wow/media/{kind}/{id}"))?)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("fetching media")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let media: Media = response
            .error_for_status()
            .context("fetching media")?
            .json()
            .await
            .context("reading media")?;
        Ok(media
            .assets
            .into_iter()
            .find(|asset| asset.key == "icon")
            .and_then(|asset| {
                Some(Icon {
                    name: icon_name(&asset.value)?,
                    url: asset.value,
                })
            }))
    }

    /// The icon image's bytes (from Blizzard's render service, which needs no token).
    pub async fn download(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        retry(|| self.download_once(url)).await
    }

    async fn download_once(&self, url: &str) -> anyhow::Result<Vec<u8>> {
        let bytes = self
            .http
            .get(url)
            .send()
            .await
            .context("downloading an icon")?
            .error_for_status()
            .context("downloading an icon")?
            .bytes()
            .await
            .context("downloading an icon")?;
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn retries_what_waiting_can_fix_and_nothing_else() {
        // A real reqwest error with a status, from a response built in place.
        async fn status_error(status: u16) -> anyhow::Error {
            let response =
                reqwest::Response::from(http::Response::builder().status(status).body("").unwrap());
            anyhow::Error::new(response.error_for_status().unwrap_err()).context("fetching")
        }
        assert!(is_transient(&status_error(500).await));
        assert!(is_transient(&status_error(429).await));
        assert!(!is_transient(&status_error(404).await));
        assert!(!is_transient(&anyhow::anyhow!("not a network error")));

        // Fails twice with a 503, then succeeds: three calls in all.
        let calls = std::sync::atomic::AtomicU32::new(0);
        let result = retry(|| async {
            match calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
                0 | 1 => Err(status_error(503).await),
                _ => Ok("fetched"),
            }
        })
        .await;
        assert_eq!(result.unwrap(), "fetched");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 3);

        // A 404 is not retried.
        let calls = std::sync::atomic::AtomicU32::new(0);
        let result: anyhow::Result<()> = retry(|| async {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err(status_error(404).await)
        })
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn reads_icon_names_from_render_urls() {
        assert_eq!(
            icon_name("https://render.worldofwarcraft.com/classic1x-us/icons/56/inv_boots_07.jpg")
                .as_deref(),
            Some("inv_boots_07")
        );
        assert_eq!(icon_name("https://example.com/"), None);
    }

    #[test]
    fn reads_search_results_and_item_pages() {
        // Trimmed from the API's real answers for Arcanist Boots.
        let search: SearchPage = serde_json::from_value(serde_json::json!({
            "results": [{"data": {
                "id": 16800,
                "name": {"en_US": "Arcanist Boots", "de_DE": "Stiefel des Arkanisten"},
                "quality": {"type": "EPIC", "name": {"en_US": "Epic"}},
                "level": 66, "required_level": 60, "is_equippable": true,
                "inventory_type": {"type": "FEET", "name": {"en_US": "Feet"}},
                "item_class": {"id": 4, "name": {"en_US": "Armor"}},
                "item_subclass": {"id": 1, "name": {"en_US": "Cloth"}}
            }}]
        }))
        .unwrap();
        let summary: Summary = search.results.into_iter().next().unwrap().data.into();
        let expected = Summary {
            id: 16800,
            name: "Arcanist Boots".into(),
            quality: "epic".into(),
            item_level: 66,
            required_level: 60,
            inventory_type: "feet".into(),
            slot: "Feet".into(),
            item_class: "Armor".into(),
            item_subclass: "Cloth".into(),
            is_equippable: true,
        };
        assert_eq!(summary, expected);

        let page: ItemPage = serde_json::from_value(serde_json::json!({
            "id": 16800, "name": "Arcanist Boots",
            "quality": {"type": "EPIC", "name": "Epic"},
            "level": 66, "required_level": 60, "is_equippable": true,
            "inventory_type": {"type": "FEET", "name": "Feet"},
            "item_class": {"id": 4, "name": "Armor"},
            "item_subclass": {"id": 1, "name": "Cloth"},
            "preview_item": {"name": "Arcanist Boots", "armor": {"value": 70}}
        }))
        .unwrap();
        assert_eq!(Summary::from(page.fields), expected);
        assert_eq!(page.preview_item["armor"]["value"], 70);
    }
}
