//! The parts of Battle.net's Game Data API the item mirror reads: item search (the listing), an
//! item's own page (the tooltip), its media (the icon), and the icon image itself.

use std::collections::HashMap;

use anyhow::Context;
use serde::Deserialize;

/// Where the mirror reads from: the US API, in English, Classic Era's static data until the API
/// serves WoW: Forever.
pub const API_HOST: &str = "https://us.api.blizzard.com";
pub const NAMESPACE: &str = "static-classic1x-us";
pub const LOCALE: &str = "en_US";

/// The most results a search page holds; the listing pages by id ranges past it.
pub const SEARCH_PAGE_SIZE: usize = 1000;

/// A client with the site's token. Cheap to clone: `reqwest::Client` is a handle.
#[derive(Clone)]
pub struct Client {
    pub http: reqwest::Client,
    pub token: String,
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

/// An item's summary, as the mirror stores it.
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
    pub last_modified: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ItemPage {
    #[serde(flatten)]
    fields: ItemFields<String>,
    preview_item: serde_json::Value,
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

impl Client {
    fn url(&self, path: &str) -> anyhow::Result<url::Url> {
        let mut url = url::Url::parse(&format!("{API_HOST}{path}")).context("building a URL")?;
        url.query_pairs_mut().append_pair("namespace", NAMESPACE);
        Ok(url)
    }

    /// One page of items of `quality` (`EPIC`), by id from `from_id`.
    pub async fn search(&self, quality: &str, from_id: i32) -> anyhow::Result<Vec<Summary>> {
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
            .context("searching items")?
            .json()
            .await
            .context("reading an item search page")?;
        Ok(page.results.into_iter().map(|r| r.data.into()).collect())
    }

    /// The item's page, or `None` when it has not changed since `if_modified_since` (a 304) or
    /// the API does not know the id (a 404).
    pub async fn item(
        &self,
        id: i32,
        if_modified_since: Option<&str>,
    ) -> anyhow::Result<Option<Detail>> {
        let mut url = self.url(&format!("/data/wow/item/{id}"))?;
        url.query_pairs_mut().append_pair("locale", LOCALE);
        let mut request = self.http.get(url).bearer_auth(&self.token);
        if let Some(since) = if_modified_since {
            request = request.header(reqwest::header::IF_MODIFIED_SINCE, since);
        }
        let response = request.send().await.context("fetching an item")?;
        match response.status() {
            reqwest::StatusCode::NOT_MODIFIED | reqwest::StatusCode::NOT_FOUND => return Ok(None),
            _ => {}
        }
        let response = response.error_for_status().context("fetching an item")?;
        let last_modified = response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let page: ItemPage = response.json().await.context("reading an item")?;
        Ok(Some(Detail {
            summary: page.fields.into(),
            preview: page.preview_item,
            last_modified,
        }))
    }

    /// The item's icon, when it has one.
    pub async fn icon(&self, id: i32) -> anyhow::Result<Option<Icon>> {
        let response = self
            .http
            .get(self.url(&format!("/data/wow/media/item/{id}"))?)
            .bearer_auth(&self.token)
            .send()
            .await
            .context("fetching an item's media")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let media: Media = response
            .error_for_status()
            .context("fetching an item's media")?
            .json()
            .await
            .context("reading an item's media")?;
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

    /// The icon image's bytes and content type (from Blizzard's render service, which needs no
    /// token).
    pub async fn download(&self, url: &str) -> anyhow::Result<(Vec<u8>, String)> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .context("downloading an icon")?
            .error_for_status()
            .context("downloading an icon")?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();
        let bytes = response.bytes().await.context("downloading an icon")?;
        Ok((bytes.to_vec(), content_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
