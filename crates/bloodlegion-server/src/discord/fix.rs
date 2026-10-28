//! The fixes a superuser can ask for on the Discord page: none is ever done on its own.
//!
//! - Putting the bot's categories and roles in the site's order. The bot's own items move only
//!   among the places they already hold, so every other category or role keeps its place in the
//!   list. Discord lists categories top to bottom by position (then id), and roles top to bottom by
//!   position from the highest (then id). It can leave several sharing a position, so the fix
//!   numbers them afresh. A bot can move only roles below its own highest one, so nothing at or
//!   above that is ever touched. Officer always stands right under Leader: any role between them
//!   moves down past Officer.
//! - Making the bot's roles, categories, and announcements channels that are missing, the channels
//!   with its permission entries. The bot finds them by itself but makes none.
//! - Moving the announcements channels to the top of the list (out of any category) under their
//!   own names, in the site's order among the text channels there.
//! - Setting the bot's permission entries on its categories and announcements channels: only those
//!   missing or set otherwise, and only its own (entries added by hand stay).
//! - Giving the leader's and officers' roles Discord's Administrator permission (which the bot
//!   must have to give), keeping whatever else they allow; no role loses a permission.
//! - Renaming one of the bot's roles chosen by hand to the bot's own name for it.
//! - Giving the bot's roles their levels' colors.
//! - Giving linked members their rank's role, and, separately, taking the bot's other roles from
//!   them: only linked members, and only the bot's own roles.
//! - Syncing a channel in one of the bot's categories with it again: its permissions become the
//!   category's.
//! - Giving one of the bot's roles to everyone holding another of the server's roles (moving the
//!   holders of a role the server is dropping onto one of the bot's). It only gives: nobody loses a
//!   role, and the other role stays for someone to remove by hand.

use std::cmp::Reverse;
use std::collections::HashMap;

use anyhow::Context;
use serenity::all::{
    ChannelId, ChannelType, CreateChannel, EditChannel, EditRole, GuildId, Http,
    PermissionOverwriteType, Permissions, RoleId, UserId,
};

use super::check::{Channel, ServerMember, server_members, top_level_text};
use super::db;
use super::layout::{
    ANNOUNCEMENTS, Access, CATEGORIES, Roles, announcements_overwrites, category_overwrites,
    differing, has_own_entry,
};
use super::sync;
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

/// The ids of `items` from the top down, with those of `wanted` put in its order (top first)
/// within the places they hold: every other item keeps its place.
fn reordered(items: &[Slot], wanted: &[u64], list: List) -> Vec<u64> {
    let mut order = top_down(items, list);
    let places: Vec<usize> = (0..order.len())
        .filter(|&n| wanted.contains(&order[n]))
        .collect();
    let ours: Vec<u64> = wanted
        .iter()
        .copied()
        .filter(|id| order.contains(id))
        .collect();
    for (place, id) in places.into_iter().zip(ours) {
        order[place] = id;
    }
    order
}

/// The positions that put the categories `wanted` (top first) in order within the slots they
/// hold, every other one keeping its place: only those whose position changes, with the new one.
/// Every category is numbered afresh from 0, which also undoes any positions Discord left shared.
pub fn moves(items: &[Slot], wanted: &[u64]) -> Vec<(u64, u16)> {
    let old: HashMap<u64, u16> = items.iter().map(|slot| (slot.id, slot.position)).collect();
    reordered(items, wanted, List::Categories)
        .into_iter()
        .enumerate()
        .map(|(n, id)| (id, n as u16))
        .filter(|(id, position)| old.get(id) != Some(position))
        .collect()
}

/// Whether those of `ids` in `items` stand next to each other, no other item between them.
pub fn together(items: &[Slot], ids: &[u64], list: List) -> bool {
    let places: Vec<usize> = top_down(items, list)
        .iter()
        .enumerate()
        .filter(|(_, id)| ids.contains(id))
        .map(|(n, _)| n)
        .collect();
    places.windows(2).all(|pair| pair[1] == pair[0] + 1)
}

/// Whether the roles `wanted` (top first) stand in that order, and those of `together` (a run of
/// them) next to each other.
pub fn roles_in_order(items: &[Slot], wanted: &[u64], together_ids: &[u64]) -> bool {
    in_order(items, wanted, List::Roles) && together(items, together_ids, List::Roles)
}

/// `order` (top down) with those of `ids` moved up to stand right under the first of them, in
/// their order there; whatever stood between them moves down past them, keeping its own order.
fn grouped(order: Vec<u64>, ids: &[u64]) -> Vec<u64> {
    let present: Vec<u64> = order
        .iter()
        .copied()
        .filter(|id| ids.contains(id))
        .collect();
    let Some(first) = present.first() else {
        return order;
    };
    let at = order
        .iter()
        .position(|id| id == first)
        .expect("present in order");
    let mut rest: Vec<u64> = order.into_iter().filter(|id| !ids.contains(id)).collect();
    // Of the others, those above the first stay above it.
    let before = rest.len().min(at);
    rest.splice(before..before, present);
    rest
}

/// The positions that put the roles `wanted` (top first) in order within the places they hold,
/// those of `together_ids` next to each other (see [`grouped`]), touching only roles below `top` (the bot's own highest role's position): the bot can move no
/// other. Those are numbered afresh from 1 up, which also undoes any positions Discord left shared
/// (it gives several roles made at once the same one), and only those whose position changes are
/// returned. `None` when they do not fit below `top`: more roles share positions under the bot
/// than it has room to number apart.
pub fn role_moves(
    items: &[Slot],
    wanted: &[u64],
    together_ids: &[u64],
    top: u16,
) -> Option<Vec<(u64, u16)>> {
    let below: Vec<Slot> = items
        .iter()
        .copied()
        .filter(|slot| slot.position < top)
        .collect();
    let count = below.len();
    if count >= usize::from(top) {
        return None;
    }
    let old: HashMap<u64, u16> = below.iter().map(|slot| (slot.id, slot.position)).collect();
    Some(
        grouped(reordered(&below, wanted, List::Roles), together_ids)
            .into_iter()
            .enumerate()
            .map(|(n, id)| (id, (count - n) as u16))
            .filter(|(id, position)| old.get(id) != Some(position))
            .collect(),
    )
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
        detail = format!("The bot can only move roles below its own, and {_0} is not. In Server \
                          Settings, Roles, drag the bot's role above it, then try again.")
    )]
    OutOfReach(String),
    #[error("roles share positions under the bot")]
    #[problem(
        status = CONFLICT,
        title = "Roles Share Positions",
        detail = "Discord has several roles below the bot's sharing a position, with no room to \
                  number them apart. In Server Settings, Roles, drag any role one place and back \
                  (Discord then numbers every role afresh), then try again."
    )]
    NoRoomBelowBot,
    #[error("the bot is not an administrator")]
    #[problem(
        status = CONFLICT,
        title = "Bot Not an Administrator",
        detail = "Discord lets the bot give the Administrator permission only if it has it itself. \
                  In Server Settings, Roles, the server's owner turns on Administrator for the \
                  bot's role, then try again."
    )]
    BotNotAdministrator,
    #[error("not a role the bot can use")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Not a Usable Role",
        detail = "No role in the server has that id, or it is @everyone or one an integration \
                  keeps. In Server Settings, Roles, right-click the role and Copy Role ID (with \
                  Developer Mode on), then try again."
    )]
    NotAUsableRole,
    #[error("the role is already the bot's {0}")]
    #[problem(
        status = CONFLICT,
        title = "Role Already Used",
        detail = format!("That role is already the bot's {_0} role. Give that one another role \
                          first.")
    )]
    RoleAlreadyUsed(&'static str),
    #[error("no such role to give from")]
    #[problem(
        status = NOT_FOUND,
        title = "No Such Role",
        detail = "That role is not in the server, or is one of the bot's own, @everyone, or one an \
                  integration keeps. Check again and pick another."
    )]
    NoSuchRole,
    #[error("the role to give is missing")]
    #[problem(
        status = CONFLICT,
        title = "Role Missing",
        detail = "The server lacks the role to give. Create it with Create missing first."
    )]
    RoleMissing,
    #[error("not in one of the bot's categories")]
    #[problem(
        status = NOT_FOUND,
        title = "Not the Bot's",
        detail = "That channel is not in one of the bot's categories."
    )]
    NotInOurCategory,
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
    let moves = moves(&items, &wanted);
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
    let mut together_ids = Vec::new();
    for access in Access::ALL {
        if let Some(id) = db::object_id(&state.pool, access.key()).await? {
            wanted.push(id);
            if Access::TOGETHER.contains(&access) {
                together_ids.push(id);
            }
        }
    }
    if roles_in_order(&items, &wanted, &together_ids) {
        return Ok(());
    }

    // Only the bot's own roles are checked for reach: the moves touch nothing at or above the
    // bot's highest role, so no other role can stand in the way.
    let top = bot_top(&http, guild, &roles).await?;
    let out_of_reach: Vec<String> = wanted
        .iter()
        .filter_map(|id| roles.get(&RoleId::new(*id)))
        .filter(|role| role.position >= top)
        .map(|role| role.name.clone())
        .collect();
    if !out_of_reach.is_empty() {
        return Err(Error::External(FixError::OutOfReach(
            out_of_reach.join(", "),
        )));
    }
    let moves = role_moves(&items, &wanted, &together_ids, top)
        .ok_or(Error::External(FixError::NoRoomBelowBot))?;
    if moves.is_empty() {
        return Ok(());
    }

    move_roles(state, &token, guild, &moves).await?;
    tracing::info!("discord: put the roles in order");
    Ok(())
}

/// Whether one of the bot's roles has Discord's Administrator permission: what lets it give that
/// permission, and see and manage the closed categories and channels without an entry of its own.
async fn bot_is_administrator(
    http: &Http,
    guild: GuildId,
    roles: &HashMap<RoleId, serenity::all::Role>,
) -> Result<bool, FixError> {
    let me: UserId = http.get_current_user().await.map_err(discord)?.id;
    Ok(guild
        .member(http, me)
        .await
        .map_err(discord)?
        .roles
        .iter()
        .filter_map(|id| roles.get(id))
        .any(|role| role.permissions.contains(Permissions::ADMINISTRATOR)))
}

/// The bot's own highest role's position: it can move or give only roles below it.
async fn bot_top(
    http: &Http,
    guild: GuildId,
    roles: &HashMap<RoleId, serenity::all::Role>,
) -> Result<u16, FixError> {
    let me: UserId = http.get_current_user().await.map_err(discord)?.id;
    Ok(guild
        .member(http, me)
        .await
        .map_err(discord)?
        .roles
        .iter()
        .filter_map(|id| roles.get(id))
        .map(|role| role.position)
        .max()
        .unwrap_or(0))
}

/// Makes the server's role `id` the bot's role for `access`, in place of the one it had: what the
/// site's checks, fixes, and permissions use for that level from now on. The role is not renamed
/// and keeps its holders; the one it replaces is left as it is (nobody loses it). Refused when the
/// id is no role in the server, @everyone, or one an integration keeps, or is already the bot's
/// role for another level.
pub async fn set_role(state: &State, access: Access, id: u64) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let roles = guild.roles(&http).await.map_err(discord)?;
    let usable = id != guild.get()
        && roles
            .get(&RoleId::new(id))
            .is_some_and(|role| !role.managed);
    if !usable {
        return Err(Error::External(FixError::NotAUsableRole));
    }
    for other in Access::ALL.into_iter().filter(|other| *other != access) {
        if db::object_id(&state.pool, other.key()).await? == Some(id) {
            return Err(Error::External(FixError::RoleAlreadyUsed(
                other.role_name(),
            )));
        }
    }
    db::set_object_id(&state.pool, access.key(), id).await?;
    sqlx::query("SELECT pg_notify($1, '')")
        .bind(super::LAYOUT_CHANNEL)
        .execute(&state.pool)
        .await
        .context("telling the bot about the role")?;
    tracing::info!(role = access.role_name(), id, "discord: chose a role");
    Ok(())
}

/// One role for one member: `(discord user, role)`.
pub type RoleChange = (u64, RoleId);

/// Which of the bot's roles to give and take so linked members hold what their ranks call for:
/// `(discord user, role)` pairs, each member in the server given their rank's role when they lack
/// it (`give`) and the bot's other roles they hold taken (`take`). Only linked members, and only
/// the bot's roles (`roles`, those the server has); everyone and everything else is left alone.
pub fn member_role_changes(
    linked: &[db::LinkedAccount],
    members: &[ServerMember],
    roles: &HashMap<Access, RoleId>,
) -> (Vec<RoleChange>, Vec<RoleChange>) {
    let (mut give, mut take) = (Vec::new(), Vec::new());
    for account in linked {
        let Some(member) = account
            .subject
            .parse::<u64>()
            .ok()
            .and_then(|id| members.iter().find(|m| m.id == id && !m.bot))
        else {
            continue;
        };
        let wanted = Access::of(account.rank);
        for (access, role) in roles {
            let holds = member.roles.contains(role);
            if *access == wanted && !holds {
                give.push((member.id, *role));
            } else if *access != wanted && holds {
                take.push((member.id, *role));
            }
        }
    }
    give.sort();
    take.sort();
    (give, take)
}

/// Which of linked members' roles to change: `give` their rank's missing role, or else `take` the
/// bot's other roles they hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberFix {
    Give,
    Take,
}

/// Gives linked members in the server their rank's role where they lack it, or takes the bot's
/// other roles they hold (see [`member_role_changes`]), returning how many roles changed hands.
/// Refused, changing nothing, when a role it would change is not below the bot's own.
pub async fn fix_member_roles(state: &State, which: MemberFix) -> Result<usize, FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let existing = guild.roles(&http).await.map_err(discord)?;
    let mut roles = HashMap::new();
    for access in Access::ALL {
        if let Some(id) = db::object_id(&state.pool, access.key())
            .await?
            .map(RoleId::new)
            .filter(|id| existing.contains_key(id))
        {
            roles.insert(access, id);
        }
    }
    let linked = db::linked_accounts(&state.pool).await?;
    let members = server_members(&http, guild).await.map_err(discord)?;
    let (give, take) = member_role_changes(&linked, &members, &roles);
    let changes = match which {
        MemberFix::Give => give,
        MemberFix::Take => take,
    };
    if changes.is_empty() {
        return Ok(0);
    }

    let top = bot_top(&http, guild, &existing).await?;
    let mut out_of_reach: Vec<&str> = changes
        .iter()
        .filter_map(|(_, role)| existing.get(role))
        .filter(|role| role.position >= top)
        .map(|role| role.name.as_str())
        .collect();
    out_of_reach.sort_unstable();
    out_of_reach.dedup();
    if !out_of_reach.is_empty() {
        return Err(Error::External(FixError::OutOfReach(
            out_of_reach.join(", "),
        )));
    }

    for (user, role) in &changes {
        let user = UserId::new(*user);
        match which {
            MemberFix::Give => http.add_member_role(guild, user, *role, Some(REASON)).await,
            MemberFix::Take => {
                http.remove_member_role(guild, user, *role, Some(REASON))
                    .await
            }
        }
        .map_err(discord)?;
    }
    tracing::info!(
        ?which,
        changed = changes.len(),
        "discord: fixed linked members' roles"
    );
    Ok(changes.len())
}

/// Gives the bot's role for `to` to everyone in the server (bots aside) who holds the role `from`
/// and lacks it, returning how many it was given to. It takes no role from anyone.
pub async fn give_role(state: &State, from: u64, to: Access) -> Result<usize, FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let roles = guild.roles(&http).await.map_err(discord)?;
    let mut ours = Vec::new();
    for access in Access::ALL {
        if let Some(id) = db::object_id(&state.pool, access.key()).await? {
            ours.push(RoleId::new(id));
        }
    }
    let from = RoleId::new(from);
    let everyone = RoleId::new(guild.get());
    if from == everyone || ours.contains(&from) || roles.get(&from).is_none_or(|role| role.managed)
    {
        return Err(Error::External(FixError::NoSuchRole));
    }
    let target = db::object_id(&state.pool, to.key())
        .await?
        .map(RoleId::new)
        .and_then(|id| roles.get(&id))
        .ok_or(Error::External(FixError::RoleMissing))?;
    if target.position >= bot_top(&http, guild, &roles).await? {
        return Err(Error::External(FixError::OutOfReach(target.name.clone())));
    }

    let members = server_members(&http, guild).await.map_err(discord)?;
    let mut given = 0;
    for member in members
        .iter()
        .filter(|m| !m.bot && m.roles.contains(&from) && !m.roles.contains(&target.id))
    {
        http.add_member_role(guild, UserId::new(member.id), target.id, Some(REASON))
            .await
            .map_err(discord)?;
        given += 1;
    }
    tracing::info!(role = %target.name, given, "discord: gave a role");
    Ok(given)
}

/// Gives roles new positions, all at once. serenity moves one role per request, and Discord shifts
/// the rest around each; one request with every move places them together.
async fn move_roles(
    state: &State,
    token: &str,
    guild: GuildId,
    moves: &[(u64, u16)],
) -> Result<(), FixError> {
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
    Ok(())
}

/// Makes the bot's roles, categories, and announcements channels that the server lacks (found
/// neither by their stored id nor by name), the channels with the bot's permission entries, then
/// tells the running bot. Roles are made top down, each new one going in at the bottom of the list,
/// so they end up in order among themselves.
pub async fn create_channels(state: &State) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let existing = guild.roles(&http).await.map_err(discord)?;
    let mut made = false;
    let mut ids = Vec::new();
    for access in Access::ALL {
        let id = match sync::find_role(&state.pool, &existing, access).await? {
            Some(id) => id,
            None => {
                let role = guild
                    .create_role(
                        &http,
                        EditRole::new()
                            .name(access.role_name())
                            .colour(access.color())
                            .hoist(true)
                            .mentionable(false)
                            .permissions(Permissions::empty())
                            .audit_log_reason(REASON),
                    )
                    .await
                    .map_err(discord)?;
                db::set_object_id(&state.pool, access.key(), role.id.get()).await?;
                tracing::info!(role = access.role_name(), "discord: made a role");
                made = true;
                role.id
            }
        };
        ids.push(id);
    }
    let roles = Roles(ids.try_into().expect("one role per access"));
    let channels = guild.channels(&http).await.map_err(discord)?;
    let everyone = RoleId::new(guild.get());

    for category in &CATEGORIES {
        let kind = ChannelType::Category;
        if sync::find_channel(&state.pool, &channels, category.key, category.name, kind)
            .await?
            .is_some()
        {
            continue;
        }
        let builder = CreateChannel::new(category.name)
            .kind(kind)
            .permissions(category_overwrites(category.audience, &roles, everyone))
            .audit_log_reason(REASON);
        let channel = guild
            .create_channel(&http, builder)
            .await
            .map_err(discord)?;
        db::set_object_id(&state.pool, category.key, channel.id.get()).await?;
        tracing::info!(name = category.name, "discord: made a category");
        made = true;
    }
    for wanted in ANNOUNCEMENTS {
        let kind = ChannelType::Text;
        if sync::find_channel(&state.pool, &channels, wanted.key, wanted.name, kind)
            .await?
            .is_some()
        {
            continue;
        }
        let builder = CreateChannel::new(wanted.name)
            .kind(kind)
            .permissions(announcements_overwrites(wanted.audience, &roles, everyone))
            .audit_log_reason(REASON);
        let channel = guild
            .create_channel(&http, builder)
            .await
            .map_err(discord)?;
        db::set_object_id(&state.pool, wanted.key, channel.id.get()).await?;
        tracing::info!(name = wanted.name, "discord: made a channel");
        made = true;
    }
    if made {
        sqlx::query("SELECT pg_notify($1, '')")
            .bind(super::LAYOUT_CHANNEL)
            .execute(&state.pool)
            .await
            .context("telling the bot about what was made")?;
    }
    Ok(())
}

/// Moves the announcements channels to the top of the list, out of any category, under their own
/// names, then puts them in order there among the slots they hold. A missing one is left for
/// Create missing.
pub async fn fix_announcements(state: &State) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let channels = guild.channels(&http).await.map_err(discord)?;
    for wanted in ANNOUNCEMENTS {
        let Some(id) = db::object_id(&state.pool, wanted.key).await? else {
            continue;
        };
        let Some(channel) = channels.get(&ChannelId::new(id)) else {
            continue;
        };
        if channel.name == wanted.name && channel.parent_id.is_none() {
            continue;
        }
        channel
            .id
            .edit(
                &http,
                EditChannel::new()
                    .name(wanted.name)
                    .category(None)
                    .audit_log_reason(REASON),
            )
            .await
            .map_err(discord)?;
        tracing::info!(
            name = wanted.name,
            "discord: moved an announcements channel to the top"
        );
    }

    let channels: Vec<Channel> = guild
        .channels(&http)
        .await
        .map_err(discord)?
        .into_values()
        .map(|channel| Channel {
            id: channel.id.get(),
            name: channel.name,
            category: channel.kind == ChannelType::Category,
            text: matches!(channel.kind, ChannelType::Text | ChannelType::News),
            parent: channel.parent_id.map(|id| id.get()),
            position: channel.position,
            overwrites: vec![],
        })
        .collect();
    let mut wanted = Vec::new();
    for channel in ANNOUNCEMENTS {
        if let Some(id) = db::object_id(&state.pool, channel.key).await? {
            wanted.push(id);
        }
    }
    let moves = moves(&top_level_text(&channels), &wanted);
    if !moves.is_empty() {
        guild
            .reorder_channels(
                &http,
                moves
                    .into_iter()
                    .map(|(id, position)| (ChannelId::new(id), u64::from(position))),
            )
            .await
            .map_err(discord)?;
        tracing::info!("discord: put the announcements channels in order");
    }
    Ok(())
}

/// Gives the leader's and officers' roles Discord's Administrator permission where they lack it,
/// keeping what else they allow. Refused, changing nothing, when one of those roles is missing or
/// not below the bot's own highest role, or when the bot lacks Administrator itself (Discord lets
/// nobody give a permission they do not have).
pub async fn fix_administrators(state: &State) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let existing = guild.roles(&http).await.map_err(discord)?;
    let mut lacking = Vec::new();
    for access in Access::ALL.into_iter().filter(|a| a.is_administrator()) {
        let role = db::object_id(&state.pool, access.key())
            .await?
            .and_then(|id| existing.get(&RoleId::new(id)))
            .ok_or(Error::External(FixError::RoleMissing))?;
        if !role.permissions.contains(Permissions::ADMINISTRATOR) {
            lacking.push(role);
        }
    }
    if lacking.is_empty() {
        return Ok(());
    }

    if !bot_is_administrator(&http, guild, &existing).await? {
        return Err(Error::External(FixError::BotNotAdministrator));
    }
    let top = bot_top(&http, guild, &existing).await?;
    let out_of_reach: Vec<&str> = lacking
        .iter()
        .filter(|role| role.position >= top)
        .map(|role| role.name.as_str())
        .collect();
    if !out_of_reach.is_empty() {
        return Err(Error::External(FixError::OutOfReach(
            out_of_reach.join(", "),
        )));
    }

    for role in lacking {
        guild
            .edit_role(
                &http,
                role.id,
                EditRole::new()
                    .permissions(role.permissions | Permissions::ADMINISTRATOR)
                    .audit_log_reason(REASON),
            )
            .await
            .map_err(discord)?;
        tracing::info!(role = %role.name, "discord: made a role an administrator");
    }
    Ok(())
}

/// Gives the bot's roles their levels' colors (see [`Access::color`]) where they differ. Refused,
/// changing nothing, when one that differs is not below the bot's own highest role.
pub async fn fix_colors(state: &State) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let existing = guild.roles(&http).await.map_err(discord)?;
    let mut differing = Vec::new();
    for access in Access::ALL {
        let Some(role) = db::object_id(&state.pool, access.key())
            .await?
            .and_then(|id| existing.get(&RoleId::new(id)))
        else {
            continue;
        };
        if role.colour.0 != access.color() {
            differing.push((role, access.color()));
        }
    }
    if differing.is_empty() {
        return Ok(());
    }
    let top = bot_top(&http, guild, &existing).await?;
    let out_of_reach: Vec<&str> = differing
        .iter()
        .filter(|(role, _)| role.position >= top)
        .map(|(role, _)| role.name.as_str())
        .collect();
    if !out_of_reach.is_empty() {
        return Err(Error::External(FixError::OutOfReach(
            out_of_reach.join(", "),
        )));
    }
    for (role, color) in differing {
        guild
            .edit_role(
                &http,
                role.id,
                EditRole::new().colour(color).audit_log_reason(REASON),
            )
            .await
            .map_err(discord)?;
        tracing::info!(role = %role.name, color = format!("#{color:06x}"), "discord: colored a role");
    }
    Ok(())
}

/// Renames the bot's role for `access` to the bot's own name for it (a role chosen by hand keeps
/// its name until then). Refused when the server lacks the role, or it is not below the bot's own
/// highest role.
pub async fn rename_role(state: &State, access: Access) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let existing = guild.roles(&http).await.map_err(discord)?;
    let role = db::object_id(&state.pool, access.key())
        .await?
        .and_then(|id| existing.get(&RoleId::new(id)))
        .ok_or(Error::External(FixError::RoleMissing))?;
    if role.name == access.role_name() {
        return Ok(());
    }
    if role.position >= bot_top(&http, guild, &existing).await? {
        return Err(Error::External(FixError::OutOfReach(role.name.clone())));
    }
    guild
        .edit_role(
            &http,
            role.id,
            EditRole::new()
                .name(access.role_name())
                .audit_log_reason(REASON),
        )
        .await
        .map_err(discord)?;
    tracing::info!(from = %role.name, to = access.role_name(), "discord: renamed a role");
    Ok(())
}

/// Sets the bot's permission entries on its categories and announcements channels where they are
/// missing or set otherwise, and removes any entry it once kept there for itself (its role's
/// Administrator permission lets it see them now). Refused while the server lacks one of the bot's
/// roles, and, while such an entry is left, unless the bot has Administrator: removing it would
/// otherwise hide the closed channels from the bot.
pub async fn fix_permissions(state: &State) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let existing = guild.roles(&http).await.map_err(discord)?;
    let mut ids = Vec::new();
    for access in Access::ALL {
        let id = db::object_id(&state.pool, access.key())
            .await?
            .map(RoleId::new)
            .filter(|id| existing.contains_key(id))
            .ok_or(Error::External(FixError::RoleMissing))?;
        ids.push(id);
    }
    let roles = Roles(ids.try_into().expect("one role per access"));

    let everyone = RoleId::new(guild.get());
    let me: UserId = http.get_current_user().await.map_err(discord)?.id;
    let channels = guild.channels(&http).await.map_err(discord)?;

    let mut wanted = Vec::new();
    for category in &CATEGORIES {
        let entries = category_overwrites(category.audience, &roles, everyone);
        wanted.push((category.key, entries));
    }
    for channel in ANNOUNCEMENTS {
        let entries = announcements_overwrites(channel.audience, &roles, everyone);
        wanted.push((channel.key, entries));
    }
    let mut ours = Vec::new();
    for (key, entries) in wanted {
        if let Some(channel) = db::object_id(&state.pool, key)
            .await?
            .and_then(|id| channels.get(&ChannelId::new(id)))
        {
            ours.push((channel, entries));
        }
    }
    let leftovers = ours
        .iter()
        .any(|(channel, _)| has_own_entry(&channel.permission_overwrites, me));
    if leftovers && !bot_is_administrator(&http, guild, &existing).await? {
        return Err(Error::External(FixError::BotNotAdministrator));
    }

    for (channel, entries) in ours {
        let differing = differing(&entries, &channel.permission_overwrites);
        let leftover = has_own_entry(&channel.permission_overwrites, me);
        if differing.is_empty() && !leftover {
            continue;
        }
        for entry in differing {
            channel
                .id
                .create_permission(&http, entry.clone())
                .await
                .map_err(discord)?;
        }
        if leftover {
            channel
                .id
                .delete_permission(&http, PermissionOverwriteType::Member(me))
                .await
                .map_err(discord)?;
        }
        tracing::info!(name = %channel.name, "discord: set the bot's permissions");
    }
    Ok(())
}

/// Syncs `channel`, in one of the bot's categories, with that category again: its permissions
/// become the category's, which it then follows.
pub async fn sync_channel(state: &State, channel: u64) -> Result<(), FixError> {
    let (token, guild) = connection(state).await?;
    let http = Http::new(&token);
    let channels = guild.channels(&http).await.map_err(discord)?;
    let mut ours = Vec::new();
    for category in &CATEGORIES {
        if let Some(id) = db::object_id(&state.pool, category.key).await? {
            ours.push(ChannelId::new(id));
        }
    }
    let target = channels
        .get(&ChannelId::new(channel))
        .ok_or(Error::External(FixError::NotInOurCategory))?;
    let parent = target
        .parent_id
        .filter(|parent| ours.contains(parent))
        .and_then(|parent| channels.get(&parent))
        .ok_or(Error::External(FixError::NotInOurCategory))?;
    target
        .id
        .edit(
            &http,
            EditChannel::new()
                .permissions(parent.permission_overwrites.clone())
                .audit_log_reason(REASON),
        )
        .await
        .map_err(discord)?;
    tracing::info!(name = %target.name, "discord: synced a channel with its category");
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
        let wanted = [1, 2, 3, 4]; // Officers, Raiders, Members, Public.
        assert!(!in_order(&items, &wanted, List::Categories));
        let moved = moves(&items, &wanted);
        // General and Voice keep their slots; the bot's four fill theirs in order.
        let mut after = items.clone();
        for (id, position) in &moved {
            after.iter_mut().find(|s| s.id == *id).unwrap().position = *position;
        }
        assert_eq!(top_down(&after, List::Categories), vec![9, 1, 8, 2, 3, 4]);
        assert!(in_order(&after, &wanted, List::Categories));
        assert!(moved.iter().all(|(id, _)| ![9, 8].contains(id)));
        // In order already: nothing to do.
        assert!(moves(&after, &wanted).is_empty());
    }

    /// `items` with `moved` applied.
    fn applied(items: &[Slot], moved: &[(u64, u16)]) -> Vec<Slot> {
        let mut after = items.to_vec();
        for (id, position) in moved {
            after.iter_mut().find(|s| s.id == *id).unwrap().position = *position;
        }
        after
    }

    #[test]
    fn roles_run_from_the_highest_position_down() {
        // Top down: Bot(7, at 7), Admin(9), Member(5), Officer(2), Bots(8), Leader(1), Raider(3).
        let items = slots(&[(7, 7), (9, 6), (5, 5), (2, 4), (8, 3), (1, 2), (3, 1)]);
        let wanted = [1, 2, 3, 5]; // Leader, Officer, Raider, Member (Trial missing).
        assert!(!in_order(&items, &wanted, List::Roles));
        let moved = role_moves(&items, &wanted, &[], 7).unwrap();
        let after = applied(&items, &moved);
        assert_eq!(top_down(&after, List::Roles), vec![7, 9, 1, 2, 8, 3, 5]);
        assert!(moved.iter().all(|(id, _)| ![7, 9, 8].contains(id)));
        assert!(in_order(&after, &wanted, List::Roles));
        // In order already: nothing to do.
        assert!(role_moves(&after, &wanted, &[], 7).unwrap().is_empty());
    }

    #[test]
    fn linked_members_get_their_ranks_role_and_lose_the_others() {
        use crate::guild::Rank;
        let account = |subject: &str, rank| db::LinkedAccount {
            subject: subject.into(),
            username: subject.into(),
            rank,
        };
        let member = |id, roles: &[u64]| ServerMember {
            id,
            name: id.to_string(),
            bot: false,
            roles: roles.iter().copied().map(RoleId::new).collect(),
        };
        let roles = HashMap::from([
            (Access::Leader, RoleId::new(1)),
            (Access::Officer, RoleId::new(2)),
            (Access::Raider, RoleId::new(3)),
        ]);
        let linked = [
            account("10", Rank::Raider),  // Lacks Raider.
            account("20", Rank::Raider),  // A demoted officer: has both.
            account("30", Rank::Officer), // Right, with a role of the server's own (77).
            account("40", Rank::Raider),  // Not in the server.
        ];
        let members = [
            member(10, &[]),
            member(20, &[2, 3]),
            member(30, &[2, 77]),
            member(50, &[1]), // Not linked: left alone.
        ];
        let (give, take) = member_role_changes(&linked, &members, &roles);
        assert_eq!(give, vec![(10, RoleId::new(3))]);
        assert_eq!(take, vec![(20, RoleId::new(2))]);
    }

    #[test]
    fn officer_stands_right_under_leader() {
        // Top down: Bot(99), Leader(1), Reaction Roles(50), Officer(2), Gamers(51), Raider(3).
        let items = slots(&[(99, 6), (1, 5), (50, 4), (2, 3), (51, 2), (3, 1)]);
        let wanted = [1, 2, 3];
        // The bot's roles are in order among their places, but something stands between the two.
        assert!(in_order(&items, &wanted, List::Roles));
        assert!(!roles_in_order(&items, &wanted, &[1, 2]));
        let moved = role_moves(&items, &wanted, &[1, 2], 6).unwrap();
        let after = applied(&items, &moved);
        // Officer moves up under Leader; Reaction Roles moves down past it, the rest stays.
        assert_eq!(top_down(&after, List::Roles), vec![99, 1, 2, 50, 51, 3]);
        assert!(roles_in_order(&after, &wanted, &[1, 2]));
        assert!(role_moves(&after, &wanted, &[1, 2], 6).unwrap().is_empty());
    }

    #[test]
    fn roles_sharing_positions_are_numbered_apart_below_the_bot() {
        // The bot on top at 9, Booster (managed, 20) and the five roles it made at once all at 1,
        // as Discord leaves them: top down by id, Leader(10) Officer(11) Raider(12) Trial(13)
        // Member(14), then Booster(20). Wanted: Member above Trial.
        let items = slots(&[
            (99, 9),
            (10, 1),
            (11, 1),
            (12, 1),
            (13, 1),
            (14, 1),
            (20, 1),
        ]);
        let wanted = [10, 11, 12, 14, 13];
        assert!(!in_order(&items, &wanted, List::Roles));
        let moved = role_moves(&items, &wanted, &[], 9).unwrap();
        // The bot's own role never moves, and nothing reaches its position.
        assert!(moved.iter().all(|(id, to)| *id != 99 && *to < 9));
        let after = applied(&items, &moved);
        assert_eq!(
            top_down(&after, List::Roles),
            vec![99, 10, 11, 12, 14, 13, 20]
        );
        assert!(in_order(&after, &wanted, List::Roles));

        // With no room to number them apart under the bot, nothing moves.
        let crowded = slots(&[(99, 3), (10, 1), (11, 1), (12, 1)]);
        assert_eq!(role_moves(&crowded, &[11, 10], &[], 3), None);
    }

    #[test]
    fn ties_break_by_id_as_discord_does() {
        // Two categories at the same position: the smaller id is higher.
        let items = slots(&[(2, 0), (1, 0)]);
        assert!(in_order(&items, &[1, 2], List::Categories));
        assert!(!in_order(&items, &[2, 1], List::Categories));
    }
}
