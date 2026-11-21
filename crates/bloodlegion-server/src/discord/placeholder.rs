//! Placeholder ranks: the rank a superuser says someone in the Discord server has before their
//! Discord account is linked to a site account (`discord_placeholders`). The role checks and fixes
//! take it for their rank, so they keep its role through a purge of everyone unaccounted for. When
//! the account is linked, by its member or a superuser, the site account takes the rank if it is
//! higher than its own and the placeholder goes (a trigger, so every way of linking does it).

use anyhow::Context;

use super::db;
use super::link::{LinkError, discord_user, parse_id};
use crate::guild::Rank;
use crate::users::UserId;
use crate::{Error, Result, State};

/// The ranks a placeholder can be: not the public (that is having none), nor the leader or an
/// officer, whose roles run the server and so need an account.
pub const RANKS: [Rank; 4] = [Rank::Raider, Rank::Trial, Rank::Member, Rank::Friend];

/// How Discord names a user: the name they show, else their username.
fn name(user: &serde_json::Value) -> Option<&str> {
    ["global_name", "username"].iter().find_map(|key| {
        user.get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.is_empty())
    })
}

/// Sets the placeholder rank of the Discord account `discord_id`, in place of any it had. Refused
/// for a bot, for an account that is someone's on the site already (their rank is their account's),
/// and for a rank a placeholder cannot be.
pub async fn set(state: &State, discord_id: &str, rank: Rank, by: UserId) -> Result<(), LinkError> {
    let id = parse_id(discord_id).ok_or(Error::External(LinkError::InvalidId))?;
    if !RANKS.contains(&rank) {
        return Err(Error::External(LinkError::NotAPlaceholderRank));
    }
    if let Some(owner) = db::account_owner(&state.pool, id)
        .await
        .context("looking up whose the Discord account is")?
    {
        return Err(Error::External(LinkError::HasAccount(owner)));
    }
    let user = discord_user(state, id).await?;
    if user.get("bot").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(Error::External(LinkError::Bot));
    }
    let discord_name = name(&user).unwrap_or(discord_id.trim());
    db::set_placeholder(&state.pool, id, discord_name, rank, by)
        .await
        .context("storing the placeholder")?;
    tracing::info!(
        discord_id = id,
        rank = rank.slug(),
        "discord: set a placeholder rank"
    );
    Ok(())
}

/// Removes the Discord account's placeholder rank.
pub async fn remove(state: &State, discord_id: &str) -> Result<(), LinkError> {
    let id = parse_id(discord_id).ok_or(Error::External(LinkError::InvalidId))?;
    if !db::delete_placeholder(&state.pool, id)
        .await
        .context("removing the placeholder")?
    {
        return Err(Error::External(LinkError::NoPlaceholder));
    }
    tracing::info!(discord_id = id, "discord: removed a placeholder rank");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_discord_user_as_they_show() {
        let shown = serde_json::json!({ "username": "zoom", "global_name": "Zoom" });
        assert_eq!(name(&shown), Some("Zoom"));
        let plain = serde_json::json!({ "username": "zoom", "global_name": null });
        assert_eq!(name(&plain), Some("zoom"));
        assert_eq!(name(&serde_json::json!({})), None);
    }

    #[test]
    fn no_placeholder_runs_the_server_or_is_public() {
        use super::super::layout::Access;
        for rank in RANKS {
            assert!(Access::of(rank).is_some_and(|access| !access.is_administrator()));
        }
    }
}
