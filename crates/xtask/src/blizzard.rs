//! `cargo xtask blizzard sync`: refreshes every piece of Blizzard's data the repository vendors,
//! for the diff to be reviewed and committed. Nothing reads Blizzard at run time: the server
//! embeds the item snapshot and loads it on migrate, and the web app serves the rest as files.
//!
//! - [`items`]: the item mirror's snapshot, from the Game Data API.
//! - [`spells`]: the keybind planner's spells, from the Game Data API, and their icons.
//! - [`classes`]: the class and talent tree icons, from the render service.
//!
//! The Game Data API's credentials are the `battlenet` sign-in client's, from
//! `BATTLENET_CLIENT_ID` and `BATTLENET_CLIENT_SECRET`: it takes a client credentials token from
//! the same client. The render service needs none.

mod api;
mod classes;
mod items;
mod spells;

use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Part {
    Items,
    Spells,
    Classes,
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} is not set"))
}

/// Syncs `parts`, or everything when it is empty.
#[tokio::main]
pub async fn sync(workspace: &Path, parts: &[Part]) -> Result<()> {
    let wants = |part| parts.is_empty() || parts.contains(&part);
    let http = api::http_client()?;
    if wants(Part::Items) || wants(Part::Spells) {
        let client = api::Client::new(
            http.clone(),
            &required_env("BATTLENET_CLIENT_ID")?,
            &required_env("BATTLENET_CLIENT_SECRET")?,
        )
        .await?;
        if wants(Part::Items) {
            items::sync(workspace, &client).await?;
        }
        if wants(Part::Spells) {
            spells::sync(workspace, &client).await?;
        }
    }
    if wants(Part::Classes) {
        classes::sync(workspace, &http).await?;
    }
    Ok(())
}
