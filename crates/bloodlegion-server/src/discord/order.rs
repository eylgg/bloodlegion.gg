//! Putting the bot's categories and roles in the site's order, when a superuser asks: it is never
//! done on its own. Only the bot's own items move, and only among the slots they already hold, so
//! every other category or role keeps its place.
//!
//! Discord lists categories top to bottom by position (then id), and roles top to bottom by
//! position from the highest. A bot can move only roles below its own highest one.

use std::cmp::Reverse;
use std::collections::HashMap;

use anyhow::Context;
use serenity::all::{ChannelId, ChannelType, GuildId, Http, RoleId, UserId};

use super::db;
use super::layout::{Access, CATEGORIES};
use super::sync::REASON;
use crate::{Error, Problem, Result, State};

/// Which list an item is in, which decides which way its positions run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    /// Positions count down the list from 0.
    Categories,
    /// Positions count up the list: the highest is the top (`@everyone`, at 0, is left out).
    Roles,
}

/// An item in a list: its id and position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub id: u64,
    pub position: u16,
}

/// The ids of `items` from the top of the list down.
fn top_down(items: &[Slot], list: List) -> Vec<u64> {
    let mut sorted = items.to_vec();
    match list {
        List::Categories => sorted.sort_by_key(|slot| (slot.position, slot.id)),
        List::Roles => sorted.sort_by_key(|slot| (Reverse(slot.position), slot.id)),
    }
    sorted.into_iter().map(|slot| slot.id).collect()
}

/// Whether those of `wanted` (top first) in `items` stand in that order.
pub fn in_order(items: &[Slot], wanted: &[u64], list: List) -> bool {
    let present: Vec<u64> = top_down(items, list)
        .into_iter()
        .filter(|id| wanted.contains(id))
        .collect();
    let expected: Vec<u64> = wanted
        .iter()
        .copied()
        .filter(|id| present.contains(id))
        .collect();
    present == expected
}

/// The positions that put `wanted` (top first) in order within the slots they hold, every other
/// item keeping its place: only the items whose position changes, with the new one.
pub fn moves(items: &[Slot], wanted: &[u64], list: List) -> Vec<(u64, u16)> {
    let mut order = top_down(items, list);
    let slots: Vec<usize> = (0..order.len())
        .filter(|&n| wanted.contains(&order[n]))
        .collect();
    let ours: Vec<u64> = wanted
        .iter()
        .copied()
        .filter(|id| order.contains(id))
        .collect();
    for (slot, id) in slots.into_iter().zip(ours) {
        order[slot] = id;
    }
    let count = order.len();
    let old: HashMap<u64, u16> = items.iter().map(|slot| (slot.id, slot.position)).collect();
    order
        .into_iter()
        .enumerate()
        .map(|(n, id)| {
            let position = match list {
                List::Categories => n,
                List::Roles => count - n,
            };
            (id, position as u16)
        })
        .filter(|(id, position)| old.get(id) != Some(position))
        .collect()
}

#[derive(Debug, thiserror::Error, Problem)]
pub enum FixError {
    #[error("the bot is not set up")]
    #[problem(
        status = CONFLICT,
        title = "Bot Not Set Up",
        detail = "Set the bot's token and the server's id first."
    )]
    NotSetUp,
    #[error("roles above the bot's own: {0}")]
    #[problem(
        status = CONFLICT,
        title = "Roles Out of Reach",
        detail = "The bot can only move roles below its own. In Server Settings, Roles, drag the \
                  bot's role above the guild's roles, then try again."
    )]
    OutOfReach(String),
    #[error("discord: {0}")]
    #[problem(
        status = BAD_GATEWAY,
        title = "Discord Refused",
        detail = "Discord did not make the change. Is the bot in the server, with Manage Channels \
                  and Manage Roles?"
    )]
    Discord(String),
}

fn discord(error: impl std::fmt::Display) -> Error<FixError> {
    Error::External(FixError::Discord(error.to_string()))
}

/// The bot's token and server, as the fixes use them.
async fn connection(state: &State) -> Result<(String, GuildId), FixError> {
    let settings = db::settings(&state.pool)
        .await
        .context("reading the bot's settings")?;
    let (Some(guild), Some(token)) = (settings.guild_id, settings.bot_token) else {
        return Err(Error::External(FixError::NotSetUp));
    };
    let token = state
        .decrypt(&token)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok((token, GuildId::new(guild)))
}

/// Puts the bot's categories in order.
pub async fn fix_categories(state: &State) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let channels = guild.channels(&http).await.map_err(discord)?;
    let items: Vec<Slot> = channels
        .values()
        .filter(|channel| channel.kind == ChannelType::Category)
        .map(|channel| Slot {
            id: channel.id.get(),
            position: channel.position,
        })
        .collect();
    let mut wanted = Vec::new();
    for category in &CATEGORIES {
        if let Some(id) = db::object_id(&state.pool, category.key).await? {
            wanted.push(id);
        }
    }
    let moves = moves(&items, &wanted, List::Categories);
    if moves.is_empty() {
        return Ok(());
    }
    guild
        .reorder_channels(
            &http,
            moves
                .into_iter()
                .map(|(id, position)| (ChannelId::new(id), u64::from(position))),
        )
        .await
        .map_err(discord)?;
    tracing::info!("discord: put the categories in order");
    Ok(())
}

/// Puts the bot's roles in order. Refused, changing nothing, when a role it would move is not below
/// the bot's own.
pub async fn fix_roles(state: &State) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let roles = guild.roles(&http).await.map_err(discord)?;
    let everyone = RoleId::new(guild.get());
    let items: Vec<Slot> = roles
        .values()
        .filter(|role| role.id != everyone)
        .map(|role| Slot {
            id: role.id.get(),
            position: role.position,
        })
        .collect();
    let mut wanted = Vec::new();
    for access in Access::ALL {
        if let Some(id) = db::object_id(&state.pool, access.key()).await? {
            wanted.push(id);
        }
    }
    let moves = moves(&items, &wanted, List::Roles);
    if moves.is_empty() {
        return Ok(());
    }

    let me: UserId = http.get_current_user().await.map_err(discord)?.id;
    let top = guild
        .member(&http, me)
        .await
        .map_err(discord)?
        .roles
        .iter()
        .filter_map(|id| roles.get(id))
        .map(|role| role.position)
        .max()
        .unwrap_or(0);
    let position = |id: u64| roles.get(&RoleId::new(id)).map_or(0, |role| role.position);
    let out_of_reach: Vec<String> = moves
        .iter()
        .filter(|(id, to)| position(*id).max(*to) >= top)
        .filter_map(|(id, _)| roles.get(&RoleId::new(*id)).map(|role| role.name.clone()))
        .collect();
    if !out_of_reach.is_empty() {
        return Err(Error::External(FixError::OutOfReach(
            out_of_reach.join(", "),
        )));
    }

    // serenity moves one role per request, and Discord shifts the rest around each; one request
    // with every move places them all at once.
    let body: Vec<serde_json::Value> = moves
        .iter()
        .map(|(id, position)| serde_json::json!({ "id": id.to_string(), "position": position }))
        .collect();
    let response = state
        .http_client
        .patch(format!("https://discord.com/api/v10/guilds/{guild}/roles"))
        .header("Authorization", format!("Bot {token}"))
        .header("X-Audit-Log-Reason", REASON)
        .json(&body)
        .send()
        .await
        .map_err(discord)?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(discord(format!("{status}: {text}")));
    }
    tracing::info!("discord: put the roles in order");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(list: &[(u64, u16)]) -> Vec<Slot> {
        list.iter()
            .map(|&(id, position)| Slot { id, position })
            .collect()
    }

    #[test]
    fn categories_move_only_among_their_own_slots() {
        // Top down: General(9), Officers(4), Voice(8), Public(1), Raiders(3), Members(2).
        let items = slots(&[(9, 0), (4, 1), (8, 2), (1, 3), (3, 4), (2, 5)]);
        let wanted = [1, 2, 3, 4]; // Public, Members, Raiders, Officers.
        assert!(!in_order(&items, &wanted, List::Categories));
        let moved = moves(&items, &wanted, List::Categories);
        // General and Voice keep their slots; the bot's four fill theirs in order.
        let mut after = items.clone();
        for (id, position) in &moved {
            after.iter_mut().find(|s| s.id == *id).unwrap().position = *position;
        }
        assert_eq!(top_down(&after, List::Categories), vec![9, 1, 8, 2, 3, 4]);
        assert!(in_order(&after, &wanted, List::Categories));
        assert!(moved.iter().all(|(id, _)| ![9, 8].contains(id)));
        // In order already: nothing to do.
        assert!(moves(&after, &wanted, List::Categories).is_empty());
    }

    #[test]
    fn roles_run_from_the_highest_position_down() {
        // Top down: Admin(9, at 6), Member(5), Officer(2), Bots(8), Leader(1), Raider(3).
        let items = slots(&[(9, 6), (5, 5), (2, 4), (8, 3), (1, 2), (3, 1)]);
        let wanted = [1, 2, 3, 5]; // Leader, Officer, Raider, Member (Trial, Friend missing).
        assert!(!in_order(&items, &wanted, List::Roles));
        let moved = moves(&items, &wanted, List::Roles);
        let mut after = items.clone();
        for (id, position) in &moved {
            after.iter_mut().find(|s| s.id == *id).unwrap().position = *position;
        }
        assert_eq!(top_down(&after, List::Roles), vec![9, 1, 2, 8, 3, 5]);
        assert!(moved.iter().all(|(id, _)| ![9, 8].contains(id)));
        assert!(in_order(&after, &wanted, List::Roles));
    }

    #[test]
    fn ties_break_by_id_as_discord_does() {
        // Two categories at the same position: the smaller id is higher.
        let items = slots(&[(2, 0), (1, 0)]);
        assert!(in_order(&items, &[1, 2], List::Categories));
        assert!(!in_order(&items, &[2, 1], List::Categories));
    }
}
