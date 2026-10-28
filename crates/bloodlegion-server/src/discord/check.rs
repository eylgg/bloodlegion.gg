//! The role check, for a superuser: how the bot and the server stand, and where the server's roles
//! differ from the site's ranks. The bot hands out no roles on its own, so this is how someone finds
//! who needs one: linked members not in the server or without their rank's role (or with another of the
//! bot's besides), and people holding one of the bot's roles with no linked account. It also
//! counts the server's people, each role's holders (the server's other roles too, which a fix can
//! give one of the bot's roles to), and each category's channels, and says whether the categories
//! stand in order. It reads Discord and changes nothing.

use std::collections::{HashMap, HashSet};

use anyhow::Context;
use serenity::all::{
    ChannelType, GuildId, Http, Member, PermissionOverwrite, PermissionOverwriteType, Permissions,
    RoleId, UserId,
};
use time::OffsetDateTime;

use super::db::{self, LinkedAccount, Status};
use super::fix::{self, List, Slot};
use super::layout::{
    ANNOUNCEMENTS, Access, CATEGORIES, Roles, announcements_overwrites, category_overwrites,
    differing,
};
use crate::guild::Rank;
use crate::{Result, State};

/// Someone in the server, as the check needs them.
#[derive(Debug, Clone)]
pub struct ServerMember {
    pub id: u64,
    pub name: String,
    pub bot: bool,
    pub roles: Vec<RoleId>,
}

/// A channel in the server, as the check needs it.
#[derive(Debug, Clone)]
pub struct Channel {
    pub id: u64,
    pub name: String,
    pub category: bool,
    /// A text (or announcement) channel, as opposed to voice, a forum, a category, ...
    pub text: bool,
    pub parent: Option<u64>,
    pub position: u16,
    pub overwrites: Vec<PermissionOverwrite>,
}

/// The server as the check reads it, all at once.
pub struct Snapshot {
    pub server_name: String,
    /// The `@everyone` role, whose id is the server's.
    pub everyone: RoleId,
    /// Everyone in the server, bots included.
    pub members: Vec<ServerMember>,
    pub role_names: HashMap<RoleId, String>,
    /// Each role's position, `@everyone` left out.
    pub role_positions: HashMap<RoleId, u16>,
    /// The roles shown separately in the member list (Discord's "hoist").
    pub hoisted: HashSet<RoleId>,
    /// The roles anyone may mention ("Allow anyone to mention this role").
    pub mentionable: HashSet<RoleId>,
    /// The roles an integration keeps (a bot's own, the server booster role), which nobody gives.
    pub managed: HashSet<RoleId>,
    /// The roles with Discord's Administrator permission.
    pub administrators: HashSet<RoleId>,
    /// Each role's color, `0xRRGGBB`; 0 is Discord's default (no color).
    pub role_colors: HashMap<RoleId, u32>,
    /// The bot's own user id: its highest role decides which roles it can move.
    pub bot_id: u64,
    pub channels: Vec<Channel>,
    /// What the bot made, by its key (`discord_objects`), whether or not it is still there.
    pub objects: HashMap<String, u64>,
}

/// A linked member whose roles are not what their rank calls for.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct MemberStatus {
    pub username: String,
    pub rank: Rank,
    /// As a string: it does not fit a JavaScript number.
    pub discord_id: String,
    /// How the server shows them; `None` when they are not in it.
    pub discord_name: Option<String>,
    /// The role their rank calls for, by its name in the server.
    pub role: String,
    pub has_role: bool,
    /// The bot's other roles they hold, which their rank does not call for.
    pub other_roles: Vec<String>,
}

/// Someone in the server holding one of the bot's roles without a linked account.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Unlinked {
    pub discord_id: String,
    pub discord_name: String,
    pub roles: Vec<String>,
}

/// One of the bot's roles: whether the server has it, who holds it, and who should.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct RoleSummary {
    /// The bot's key for it (`role:member`), which the fix giving it names it by.
    pub key: &'static str,
    /// Its Discord id, as a string (it does not fit a JavaScript number); `None` when the server
    /// lacks it.
    pub id: Option<String>,
    /// As the server names it, or as the bot would when it is missing.
    pub name: String,
    /// What the bot would name it: a role chosen by hand may be named otherwise.
    pub expected_name: &'static str,
    /// People in the server holding it.
    pub holders: usize,
    /// Linked members in the server whose rank calls for it: how many should hold it.
    pub linked: usize,
    /// Whether it has Discord's Administrator permission, and whether its level should (the
    /// leader's and the officers'; see [`Access::is_administrator`]).
    pub administrator: bool,
    pub wants_administrator: bool,
    /// Its color now (`#rrggbb`), `None` for Discord's default or when the role is missing; and
    /// the one its level should have (see [`Access::color`]).
    pub color: Option<String>,
    pub expected_color: String,
}

/// The bot's permission entries on one of its categories or announcements channels.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct PermissionsStatus {
    /// Its Discord id, as a string.
    pub id: String,
    /// As the server names it.
    pub name: String,
    pub category: bool,
    /// Whose entries (the bot's for them) are missing or set otherwise: `@everyone`, a role's
    /// name, or "the bot". Empty when all are right.
    pub differing: Vec<String>,
}

/// One of the server's roles that is not the bot's, for giving one of the bot's roles to its
/// holders (`fix::give_role`).
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct OtherRole {
    /// Its Discord id, as a string.
    pub id: String,
    pub name: String,
    /// People in the server holding it.
    pub holders: usize,
}

/// One of the bot's categories.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct CategorySummary {
    /// Its Discord id, as a string; `None` when the server lacks it.
    pub id: Option<String>,
    pub name: String,
    pub channels: usize,
}

/// The bot's own highest role, which should be the server's top role.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct BotRole {
    /// Its Discord id, as a string.
    pub id: String,
    pub name: String,
    /// The server's roles above it, top first; none when it is the top role.
    pub above: Vec<String>,
    /// Whether the member list shows its holders (the bot) on their own, apart from everyone else.
    pub shown_separately: bool,
    /// Whether anyone may mention it, which only pings the bot: it should not be.
    pub mentionable: bool,
}

/// One of the announcements channels: where it is, against where it should be (at the top of the
/// list, in no category, under its own name).
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct AnnouncementsStatus {
    /// The name it should have.
    pub name: String,
    /// Its Discord id, as a string; `None` when the server lacks it.
    pub id: Option<String>,
    /// Its name now, when it is not the one it should have.
    pub named: Option<String>,
    /// The category it is in, when it is in one.
    pub category: Option<String>,
}

/// Someone in the server, for the list of everyone.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Person {
    pub discord_id: String,
    pub discord_name: String,
    /// The site account their Discord account is linked to, if any.
    pub username: Option<String>,
    /// Their roles, top first, as the server names them.
    pub roles: Vec<String>,
}

/// Everyone in the server, by how they stand with the site.
#[derive(Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Everyone {
    /// Linked to a site account.
    pub linked: Vec<Person>,
    /// Not linked, holding one of the bot's roles.
    pub unlinked_ours: Vec<Person>,
    /// Not linked, holding only the server's other roles.
    pub unlinked_others: Vec<Person>,
    /// Not linked, with no role at all.
    pub no_roles: Vec<Person>,
    pub bots: Vec<Person>,
}

/// A channel in one of the bot's categories whose permissions are its own rather than the
/// category's (Discord calls it not synced): it no longer follows who may see the category.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Unsynced {
    pub id: String,
    pub name: String,
    pub category: String,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Report {
    pub server_name: String,
    /// People in the server, and bots.
    pub people: usize,
    pub bots: usize,
    /// Members who linked Discord on the site, and how many of them are in the server.
    pub linked: usize,
    pub linked_in_server: usize,
    /// In the order the bot keeps them, top first.
    pub roles: Vec<RoleSummary>,
    /// Whether the roles that exist stand in that order in Discord's list.
    pub roles_in_order: bool,
    /// The bot's own highest role (Discord makes one for a bot when it joins): it can move only
    /// roles below it, and nobody can raise it but someone above it. `None` when it has none.
    pub bot_role: Option<BotRole>,
    /// The bot's roles not below the bot's own highest role, which it cannot move or give.
    pub roles_out_of_reach: Vec<String>,
    /// Whether the bot has Discord's Administrator permission, without which it cannot give it to
    /// a role.
    pub bot_is_administrator: bool,
    /// The server's roles besides the bot's, `@everyone`, and those integrations keep, top first.
    pub other_roles: Vec<OtherRole>,
    /// In the order the bot keeps them.
    pub categories: Vec<CategorySummary>,
    /// Whether the categories that exist stand in that order in Discord's list.
    pub categories_in_order: bool,
    /// The announcements channels, top first.
    pub announcements: Vec<AnnouncementsStatus>,
    /// Whether those at the top of the list stand in that order.
    pub announcements_in_order: bool,
    /// Channels in the bot's categories not synced with them.
    pub unsynced: Vec<Unsynced>,
    /// The bot's permission entries on its categories and announcements channels (those the server
    /// has), categories first; `None` until the server has all the bot's roles, which they name.
    pub permissions: Option<Vec<PermissionsStatus>>,
    pub members: Vec<MemberStatus>,
    pub unlinked: Vec<Unlinked>,
    pub everyone: Everyone,
}

/// A color as `#rrggbb`.
fn hex(color: u32) -> String {
    format!("#{color:06x}")
}

/// The check itself, given what the site and the server hold.
pub fn check(linked: &[LinkedAccount], server: &Snapshot) -> Report {
    // The bot's roles that the server still has.
    let roles: HashMap<Access, RoleId> = Access::ALL
        .into_iter()
        .filter_map(|access| {
            let id = RoleId::new(*server.objects.get(access.key())?);
            server.role_names.contains_key(&id).then_some((access, id))
        })
        .collect();
    let name = |access: Access| {
        roles
            .get(&access)
            .and_then(|id| server.role_names.get(id))
            .cloned()
            .unwrap_or_else(|| access.role_name().to_string())
    };
    let held = |member: &ServerMember| -> Vec<Access> {
        Access::ALL
            .into_iter()
            .filter(|access| {
                roles
                    .get(access)
                    .is_some_and(|id| member.roles.contains(id))
            })
            .collect()
    };
    let people: Vec<&ServerMember> = server.members.iter().filter(|m| !m.bot).collect();
    let in_server: HashMap<u64, &ServerMember> = people.iter().map(|m| (m.id, *m)).collect();
    let linked_member = |account: &LinkedAccount| {
        account
            .subject
            .parse::<u64>()
            .ok()
            .and_then(|id| in_server.get(&id).copied())
    };

    let mut statuses = Vec::new();
    for account in linked {
        let wanted = Access::of(account.rank);
        let member = linked_member(account);
        let holds = member.map(held).unwrap_or_default();
        let has_role = holds.contains(&wanted);
        let other_roles: Vec<String> = holds
            .iter()
            .filter(|access| **access != wanted)
            .map(|access| name(*access))
            .collect();
        // A role the server lacks cannot be held, so it is not counted against anyone.
        let missing = !has_role && roles.contains_key(&wanted);
        if member.is_none() || missing || !other_roles.is_empty() {
            statuses.push(MemberStatus {
                username: account.username.clone(),
                rank: account.rank,
                discord_id: account.subject.clone(),
                discord_name: member.map(|m| m.name.clone()),
                role: name(wanted),
                has_role,
                other_roles,
            });
        }
    }

    let linked_ids: Vec<&str> = linked.iter().map(|a| a.subject.as_str()).collect();
    let mut unlinked: Vec<Unlinked> = people
        .iter()
        .filter(|member| !linked_ids.contains(&member.id.to_string().as_str()))
        .filter_map(|member| {
            let holds = held(member);
            (!holds.is_empty()).then(|| Unlinked {
                discord_id: member.id.to_string(),
                discord_name: member.name.clone(),
                roles: holds.into_iter().map(name).collect(),
            })
        })
        .collect();
    unlinked.sort_by_key(|person| person.discord_name.to_lowercase());

    let role_summaries = Access::ALL
        .into_iter()
        .map(|access| RoleSummary {
            key: access.key(),
            id: roles.get(&access).map(|id| id.get().to_string()),
            name: name(access),
            expected_name: access.role_name(),
            administrator: roles
                .get(&access)
                .is_some_and(|id| server.administrators.contains(id)),
            wants_administrator: access.is_administrator(),
            color: roles
                .get(&access)
                .and_then(|id| server.role_colors.get(id))
                .filter(|color| **color != 0)
                .map(|color| hex(*color)),
            expected_color: hex(access.color()),
            holders: people.iter().filter(|m| held(m).contains(&access)).count(),
            linked: linked
                .iter()
                .filter(|a| Access::of(a.rank) == access && linked_member(a).is_some())
                .count(),
        })
        .collect();

    let channel = |key: &str| {
        let id = *server.objects.get(key)?;
        server.channels.iter().find(|channel| channel.id == id)
    };
    let categories: Vec<(CategorySummary, Option<&Channel>)> = CATEGORIES
        .iter()
        .map(|category| {
            let found = channel(category.key).filter(|channel| channel.category);
            let summary = CategorySummary {
                id: found.map(|c| c.id.to_string()),
                name: found.map_or(category.name.to_string(), |c| c.name.clone()),
                channels: found.map_or(0, |c| {
                    server
                        .channels
                        .iter()
                        .filter(|other| other.parent == Some(c.id))
                        .count()
                }),
            };
            (summary, found)
        })
        .collect();
    let category_slots: Vec<Slot> = server
        .channels
        .iter()
        .filter(|channel| channel.category)
        .map(|channel| Slot {
            id: channel.id,
            position: channel.position,
        })
        .collect();
    let ours: Vec<u64> = categories
        .iter()
        .filter_map(|(_, found)| found.map(|c| c.id))
        .collect();
    let categories_in_order = fix::in_order(&category_slots, &ours, List::Categories);

    let role_slots: Vec<Slot> = server
        .role_positions
        .iter()
        .map(|(id, position)| Slot {
            id: id.get(),
            position: *position,
        })
        .collect();
    let our_roles: Vec<u64> = Access::ALL
        .into_iter()
        .filter_map(|access| roles.get(&access).map(|id| id.get()))
        .collect();
    let together_ids: Vec<u64> = Access::TOGETHER
        .into_iter()
        .filter_map(|access| roles.get(&access).map(|id| id.get()))
        .collect();
    let roles_in_order = fix::roles_in_order(&role_slots, &our_roles, &together_ids);
    let bot_roles: &[RoleId] = server
        .members
        .iter()
        .find(|member| member.id == server.bot_id)
        .map_or(&[], |bot| &bot.roles);
    let bot_role = bot_roles
        .iter()
        .filter_map(|id| Some((*server.role_positions.get(id)?, *id)))
        .max();
    let bot_top = bot_role.map_or(0, |(position, _)| position);
    let roles_out_of_reach = Access::ALL
        .into_iter()
        .filter(|access| {
            roles
                .get(access)
                .and_then(|id| server.role_positions.get(id))
                .is_some_and(|position| *position >= bot_top)
        })
        .map(name)
        .collect();
    let mut other_roles: Vec<(u16, OtherRole)> = server
        .role_positions
        .iter()
        .filter(|(id, _)| !roles.values().any(|ours| ours == *id))
        .filter(|(id, _)| !server.managed.contains(id))
        .map(|(id, position)| {
            let role = OtherRole {
                id: id.get().to_string(),
                name: server.role_names.get(id).cloned().unwrap_or_default(),
                holders: people.iter().filter(|m| m.roles.contains(id)).count(),
            };
            (*position, role)
        })
        .collect();
    other_roles.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| x.id.cmp(&y.id)));
    let category_name = |id: u64| {
        server
            .channels
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
    };
    let announcements: Vec<AnnouncementsStatus> = ANNOUNCEMENTS
        .iter()
        .map(|wanted| {
            let found = channel(wanted.key).filter(|c| !c.category);
            AnnouncementsStatus {
                name: wanted.name.to_string(),
                id: found.map(|c| c.id.to_string()),
                named: found
                    .filter(|c| c.name != wanted.name)
                    .map(|c| c.name.clone()),
                category: found.and_then(|c| c.parent).and_then(category_name),
            }
        })
        .collect();
    let announcement_ids: Vec<u64> = ANNOUNCEMENTS
        .iter()
        .filter_map(|wanted| server.objects.get(wanted.key).copied())
        .collect();
    let permissions = permissions(server, &roles);
    let announcements_in_order = fix::in_order(
        &top_level_text(&server.channels),
        &announcement_ids,
        List::Categories,
    );
    let mut unsynced: Vec<Unsynced> = categories
        .iter()
        .filter_map(|(_, found)| *found)
        .flat_map(|category| {
            server
                .channels
                .iter()
                .filter(move |c| c.parent == Some(category.id))
                .filter(|c| !announcement_ids.contains(&c.id))
                .filter(move |c| !same_overwrites(&c.overwrites, &category.overwrites))
                .map(move |c| Unsynced {
                    id: c.id.to_string(),
                    name: c.name.clone(),
                    category: category.name.clone(),
                })
        })
        .collect();
    unsynced.sort_by(|a, b| (&a.category, &a.name).cmp(&(&b.category, &b.name)));

    Report {
        server_name: server.server_name.clone(),
        people: people.len(),
        bots: server.members.len() - people.len(),
        linked: linked.len(),
        linked_in_server: linked.iter().filter(|a| linked_member(a).is_some()).count(),
        roles: role_summaries,
        roles_in_order,
        bot_is_administrator: bot_roles
            .iter()
            .any(|id| server.administrators.contains(id)),
        bot_role: bot_role.map(|(position, id)| {
            let mut above: Vec<(u16, u64)> = server
                .role_positions
                .iter()
                .filter(|(_, p)| **p > position)
                .map(|(id, p)| (*p, id.get()))
                .collect();
            above.sort_by_key(|&(p, id)| (std::cmp::Reverse(p), id));
            BotRole {
                id: id.get().to_string(),
                name: server.role_names.get(&id).cloned().unwrap_or_default(),
                shown_separately: server.hoisted.contains(&id),
                mentionable: server.mentionable.contains(&id),
                above: above
                    .into_iter()
                    .filter_map(|(_, id)| server.role_names.get(&RoleId::new(id)).cloned())
                    .collect(),
            }
        }),
        roles_out_of_reach,
        other_roles: other_roles.into_iter().map(|(_, role)| role).collect(),
        categories: categories.into_iter().map(|(summary, _)| summary).collect(),
        categories_in_order,
        announcements,
        announcements_in_order,
        unsynced,
        permissions,
        members: statuses,
        unlinked,
        everyone: everyone(linked, server, &roles),
    }
}

/// How the bot's permission entries stand on its categories and announcements channels, or `None`
/// while the server lacks one of its roles.
fn permissions(
    server: &Snapshot,
    roles: &HashMap<Access, RoleId>,
) -> Option<Vec<PermissionsStatus>> {
    let ids: Vec<RoleId> = Access::ALL
        .into_iter()
        .map(|access| roles.get(&access).copied())
        .collect::<Option<_>>()?;
    let roles = Roles(ids.try_into().ok()?);
    let bot = UserId::new(server.bot_id);
    let whose = |kind: PermissionOverwriteType| match kind {
        PermissionOverwriteType::Role(id) if id == server.everyone => "@everyone".to_string(),
        PermissionOverwriteType::Role(id) => server
            .role_names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| id.to_string()),
        _ => "the bot".to_string(),
    };
    let channel = |key: &str, category: bool| {
        let id = *server.objects.get(key)?;
        server
            .channels
            .iter()
            .find(|c| c.id == id && c.category == category)
    };
    let status = |found: &Channel, wanted: Vec<PermissionOverwrite>| PermissionsStatus {
        id: found.id.to_string(),
        name: found.name.clone(),
        category: found.category,
        differing: differing(&wanted, &found.overwrites)
            .into_iter()
            .map(|entry| whose(entry.kind))
            .collect(),
    };
    let categories = CATEGORIES.iter().filter_map(|category| {
        let found = channel(category.key, true)?;
        let wanted = category_overwrites(category.audience, &roles, server.everyone, bot);
        Some(status(found, wanted))
    });
    let announcements = ANNOUNCEMENTS.iter().filter_map(|wanted| {
        let found = channel(wanted.key, false)?;
        let entries = announcements_overwrites(wanted.audience, &roles, server.everyone, bot);
        Some(status(found, entries))
    });
    Some(categories.chain(announcements).collect())
}

/// The text channels at the top of the list (in no category), where the announcements channels
/// stand; their positions run down the list like categories'.
pub(super) fn top_level_text(channels: &[Channel]) -> Vec<Slot> {
    channels
        .iter()
        .filter(|channel| channel.text && channel.parent.is_none())
        .map(|channel| Slot {
            id: channel.id,
            position: channel.position,
        })
        .collect()
}

/// Everyone in the server, grouped, each group by name.
fn everyone(
    linked: &[LinkedAccount],
    server: &Snapshot,
    ours: &HashMap<Access, RoleId>,
) -> Everyone {
    let usernames: HashMap<&str, &str> = linked
        .iter()
        .map(|a| (a.subject.as_str(), a.username.as_str()))
        .collect();
    let mut groups = Everyone::default();
    for member in &server.members {
        let mut roles: Vec<(u16, &RoleId)> = member
            .roles
            .iter()
            .filter_map(|id| Some((*server.role_positions.get(id)?, id)))
            .collect();
        roles.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| x.cmp(y)));
        let discord_id = member.id.to_string();
        let username = usernames.get(discord_id.as_str()).map(|u| u.to_string());
        let person = Person {
            discord_name: member.name.clone(),
            username,
            roles: roles
                .iter()
                .filter_map(|(_, id)| server.role_names.get(id).cloned())
                .collect(),
            discord_id,
        };
        let group = if member.bot {
            &mut groups.bots
        } else if person.username.is_some() {
            &mut groups.linked
        } else if member.roles.iter().any(|id| ours.values().any(|o| o == id)) {
            &mut groups.unlinked_ours
        } else if !person.roles.is_empty() {
            &mut groups.unlinked_others
        } else {
            &mut groups.no_roles
        };
        group.push(person);
    }
    for group in [
        &mut groups.linked,
        &mut groups.unlinked_ours,
        &mut groups.unlinked_others,
        &mut groups.no_roles,
        &mut groups.bots,
    ] {
        group.sort_by_key(|person| person.discord_name.to_lowercase());
    }
    groups
}

/// Whether two sets of permission entries are the same, in any order: a channel synced with its
/// category has exactly the category's.
fn same_overwrites(a: &[PermissionOverwrite], b: &[PermissionOverwrite]) -> bool {
    let key = |entries: &[PermissionOverwrite]| {
        let mut keys: Vec<(u8, u64, u64, u64)> = entries
            .iter()
            .map(|o| {
                let (kind, id) = match o.kind {
                    PermissionOverwriteType::Role(id) => (0, id.get()),
                    PermissionOverwriteType::Member(id) => (1, id.get()),
                    _ => (2, 0),
                };
                (kind, id, o.allow.bits(), o.deny.bits())
            })
            .collect();
        keys.sort_unstable();
        keys
    };
    key(a) == key(b)
}

/// The posts stored from #public-announcements.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Posts {
    pub stored: i64,
    #[serde(with = "time::serde::iso8601::option")]
    pub latest: Option<OffsetDateTime>,
}

/// Everything the page shows. The bot's status and the stored posts come from the site, so they
/// show even when the check cannot read Discord (then `failure` says why).
#[derive(Debug, serde::Serialize)]
pub struct Overview {
    /// Whether a superuser has set both the bot's token and the server.
    pub set_up: bool,
    pub status: Status,
    pub posts: Posts,
    pub report: Option<Report>,
    pub failure: Option<String>,
}

pub async fn overview(state: &State) -> Result<Overview> {
    let status = db::status(&state.pool)
        .await
        .context("reading the bot's status")?;
    let (stored, latest) = db::announcement_totals(&state.pool)
        .await
        .context("counting the announcements")?;
    let settings = db::settings(&state.pool)
        .await
        .context("reading the bot's settings")?;
    let mut overview = Overview {
        set_up: false,
        status,
        posts: Posts { stored, latest },
        report: None,
        failure: None,
    };
    let (Some(guild), Some(token)) = (settings.guild_id, settings.bot_token) else {
        return Ok(overview);
    };
    overview.set_up = true;
    let token = state
        .decrypt(&token)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    match snapshot(&state.pool, &Http::new(&token), GuildId::new(guild)).await {
        Ok(server) => {
            let linked = db::linked_accounts(&state.pool)
                .await
                .context("listing the linked Discord accounts")?;
            overview.report = Some(check(&linked, &server));
        }
        Err(error) => overview.failure = Some(format!("{error:#}")),
    }
    Ok(overview)
}

/// Reads the server: its name, everyone in it, its roles, and its channels.
async fn snapshot(pool: &sqlx::PgPool, http: &Http, guild: GuildId) -> anyhow::Result<Snapshot> {
    let server_name = guild
        .to_partial_guild(http)
        .await
        .context("reading the server (is the bot in it?)")?
        .name;
    let roles = guild
        .roles(http)
        .await
        .context("listing the server's roles")?;
    let everyone = RoleId::new(guild.get());
    let role_positions = roles
        .values()
        .filter(|role| role.id != everyone)
        .map(|role| (role.id, role.position))
        .collect();
    let hoisted = roles
        .values()
        .filter(|role| role.hoist)
        .map(|role| role.id)
        .collect();
    let mentionable = roles
        .values()
        .filter(|role| role.mentionable)
        .map(|role| role.id)
        .collect();
    let managed = roles
        .values()
        .filter(|role| role.managed)
        .map(|role| role.id)
        .collect();
    let administrators = roles
        .values()
        .filter(|role| role.permissions.contains(Permissions::ADMINISTRATOR))
        .map(|role| role.id)
        .collect();
    let role_colors = roles
        .values()
        .map(|role| (role.id, role.colour.0))
        .collect();
    let role_names = roles
        .into_iter()
        .map(|(id, role)| (id, role.name))
        .collect();
    let bot_id = http
        .get_current_user()
        .await
        .context("signing in as the bot")?
        .id
        .get();
    let channels = guild
        .channels(http)
        .await
        .context("listing the server's channels")?
        .into_values()
        .map(|channel| Channel {
            id: channel.id.get(),
            name: channel.name,
            category: channel.kind == ChannelType::Category,
            text: matches!(channel.kind, ChannelType::Text | ChannelType::News),
            parent: channel.parent_id.map(|id| id.get()),
            position: channel.position,
            overwrites: channel.permission_overwrites,
        })
        .collect();
    let members = server_members(http, guild)
        .await
        .context("listing the server's members (is the Server Members intent on?)")?;
    let mut objects = HashMap::new();
    for key in Access::ALL
        .into_iter()
        .map(Access::key)
        .chain(CATEGORIES.iter().map(|c| c.key))
        .chain(ANNOUNCEMENTS.iter().map(|a| a.key))
    {
        if let Some(id) = db::object_id(pool, key).await? {
            objects.insert(key.to_string(), id);
        }
    }
    Ok(Snapshot {
        server_name,
        everyone,
        members,
        role_names,
        role_positions,
        hoisted,
        mentionable,
        managed,
        administrators,
        role_colors,
        bot_id,
        channels,
        objects,
    })
}

pub(super) async fn server_members(
    http: &Http,
    guild: GuildId,
) -> anyhow::Result<Vec<ServerMember>> {
    let mut all = Vec::new();
    let mut after = None;
    loop {
        let page: Vec<Member> = guild.members(http, Some(1000), after).await?;
        let full = page.len() == 1000;
        after = page.last().map(|member| member.user.id);
        all.extend(page.into_iter().map(|m| {
            ServerMember {
                id: m.user.id.get(),
                name: m
                    .nick
                    .clone()
                    .or(m.user.global_name.clone())
                    .unwrap_or(m.user.name),
                bot: m.user.bot,
                roles: m.roles,
            }
        }));
        if !full {
            return Ok(all);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::layout::{MEMBER_ANNOUNCEMENTS, PUBLIC_ANNOUNCEMENTS, RAIDER_ANNOUNCEMENTS};
    use super::*;

    fn account(subject: &str, username: &str, rank: Rank) -> LinkedAccount {
        LinkedAccount {
            subject: subject.into(),
            username: username.into(),
            rank,
        }
    }

    fn member(id: u64, name: &str, roles: &[u64]) -> ServerMember {
        ServerMember {
            id,
            name: name.into(),
            bot: false,
            roles: roles.iter().copied().map(RoleId::new).collect(),
        }
    }

    fn category(id: u64, name: &str, position: u16) -> Channel {
        Channel {
            id,
            name: name.into(),
            category: true,
            text: false,
            parent: None,
            position,
            overwrites: vec![],
        }
    }

    fn text(id: u64, name: &str, parent: u64) -> Channel {
        Channel {
            id,
            name: name.into(),
            category: false,
            text: true,
            parent: Some(parent),
            position: 0,
            overwrites: vec![],
        }
    }

    /// A server where the bot made everything: roles 1 to 5 (Leader to Member, with Raider renamed
    /// by hand), categories 101 to 104 in order (Officers to Public), and #announcements (201) in
    /// Public. Under the bot's
    /// own role (99) and above the bot's roles, the server keeps Friend (6) and a booster role (7).
    fn server(members: Vec<ServerMember>) -> Snapshot {
        let mut objects: HashMap<String, u64> = Access::ALL
            .into_iter()
            .enumerate()
            .map(|(n, access)| (access.key().to_string(), n as u64 + 1))
            .collect();
        for (n, category) in CATEGORIES.iter().enumerate() {
            objects.insert(category.key.to_string(), 101 + n as u64);
        }
        objects.insert(PUBLIC_ANNOUNCEMENTS.key.to_string(), 201);
        let mut role_names: HashMap<RoleId, String> = Access::ALL
            .into_iter()
            .enumerate()
            .map(|(n, access)| (RoleId::new(n as u64 + 1), access.role_name().to_string()))
            .collect();
        // Renamed by hand: the check uses the server's name.
        role_names.insert(RoleId::new(3), "Raiders".into());
        // In order, Leader (1) at the top, under the bot's own role (99, at 10).
        let mut role_positions: HashMap<RoleId, u16> =
            (1..=5).map(|id| (RoleId::new(id), 6 - id as u16)).collect();
        role_positions.insert(RoleId::new(99), 10);
        role_names.insert(RoleId::new(99), "Blood Legion bot".into());
        role_positions.insert(RoleId::new(6), 7);
        role_names.insert(RoleId::new(6), "Friend".into());
        role_positions.insert(RoleId::new(7), 6);
        role_names.insert(RoleId::new(7), "Server Booster".into());
        let mut members = members;
        members.push(ServerMember {
            id: 1000,
            name: "Blood Legion bot".into(),
            bot: true,
            roles: vec![RoleId::new(99)],
        });
        Snapshot {
            server_name: "Blood Legion".into(),
            everyone: RoleId::new(100),
            members,
            role_names,
            role_positions,
            hoisted: HashSet::from([RoleId::new(99)]),
            mentionable: HashSet::new(),
            managed: HashSet::from([RoleId::new(7), RoleId::new(99)]),
            administrators: HashSet::from([RoleId::new(99), RoleId::new(1)]),
            // Leader has its color; the rest are Discord's default.
            role_colors: HashMap::from([(RoleId::new(1), 0xFF8000)]),
            bot_id: 1000,
            channels: vec![
                category(101, "Officers", 0),
                category(102, "Raiders", 1),
                category(103, "Members", 2),
                category(104, "Public", 3),
                text(201, "announcements", 104),
                text(202, "general", 103),
                text(203, "chat", 103),
            ],
            objects,
        }
    }

    const OFFICER: u64 = 2;
    const RAIDER: u64 = 3;
    const MEMBER: u64 = 5;

    #[test]
    fn lists_only_whats_out_of_line() {
        let linked = [
            account("10", "Right", Rank::Officer),
            account("20", "Lacking", Rank::Raider),
            account("30", "Promoted", Rank::Officer),
            account("40", "Away", Rank::Member),
        ];
        let mut members = vec![
            member(10, "right", &[OFFICER, 99]),
            member(20, "lacking", &[]),
            member(30, "promoted", &[OFFICER, RAIDER]),
            member(50, "stranger", &[MEMBER]),
            member(60, "nobody", &[99]),
        ];
        members.push(ServerMember {
            bot: true,
            ..member(70, "bot", &[OFFICER])
        });
        let report = check(&linked, &server(members));
        let status = |username: &str| {
            report
                .members
                .iter()
                .find(|s| s.username == username)
                .map(|s| {
                    (
                        s.discord_name.clone(),
                        s.role.clone(),
                        s.has_role,
                        s.other_roles.clone(),
                    )
                })
        };
        assert_eq!(status("Right"), None);
        assert_eq!(
            status("Lacking"),
            Some((Some("lacking".into()), "Raiders".into(), false, vec![]))
        );
        assert_eq!(
            status("Promoted"),
            Some((
                Some("promoted".into()),
                "Officer".into(),
                true,
                vec!["Raiders".into()]
            ))
        );
        assert_eq!(status("Away"), Some((None, "Member".into(), false, vec![])));
        // A bot holding a role is neither someone out of line nor counted among the people.
        assert_eq!(
            report.unlinked,
            vec![Unlinked {
                discord_id: "50".into(),
                discord_name: "stranger".into(),
                roles: vec!["Member".into()],
            }]
        );
        assert_eq!((report.people, report.bots), (5, 2));
        assert_eq!((report.linked, report.linked_in_server), (4, 3));
    }

    #[test]
    fn counts_each_roles_holders_and_who_should_hold_it() {
        let linked = [
            account("10", "A", Rank::Officer),
            account("20", "B", Rank::Officer),
            account("40", "Away", Rank::Officer),
        ];
        let members = vec![
            member(10, "a", &[OFFICER]),
            member(20, "b", &[]),
            member(50, "c", &[OFFICER]),
        ];
        let report = check(&linked, &server(members));
        let officer = report.roles.iter().find(|r| r.name == "Officer").unwrap();
        // Held by a and c; a and b should hold it (Away is not in the server).
        assert_eq!(
            (officer.id.as_deref(), officer.holders, officer.linked),
            (Some("2"), 2, 2)
        );
    }

    #[test]
    fn says_which_roles_should_be_administrators() {
        // The bot and Leader have Administrator; Officer lacks it.
        let report = check(&[], &server(vec![]));
        let role = |name: &str| {
            report
                .roles
                .iter()
                .find(|r| r.expected_name == name)
                .unwrap()
        };
        assert!(report.bot_is_administrator);
        assert!(role("Leader").administrator && role("Leader").wants_administrator);
        assert!(!role("Officer").administrator && role("Officer").wants_administrator);
        assert!(!role("Raider").wants_administrator && !role("Member").wants_administrator);
        // A role chosen by hand keeps its own name, and says what the bot would call it.
        assert_eq!(role("Raider").name, "Raiders");
        // Each role's color, against its level's.
        assert_eq!(role("Leader").color.as_deref(), Some("#ff8000"));
        assert_eq!(role("Leader").expected_color, "#ff8000");
        assert_eq!(role("Officer").color, None);
        assert_eq!(role("Officer").expected_color, "#a335ee");
        assert_eq!(role("Member").expected_color, "#1eff00");
    }

    #[test]
    fn says_how_the_categories_and_announcements_stand() {
        let report = check(&[], &server(vec![]));
        let channels: Vec<_> = report
            .categories
            .iter()
            .map(|c| (c.name.as_str(), c.channels))
            .collect();
        assert_eq!(
            channels,
            vec![
                ("Officers", 0),
                ("Raiders", 0),
                ("Members", 2),
                ("Public", 1)
            ]
        );
        assert!(report.categories_in_order);
        // #announcements, as made before: in Public under its old name. The others are missing.
        assert_eq!(
            report.announcements,
            vec![
                AnnouncementsStatus {
                    name: "raider-announcements".into(),
                    id: None,
                    named: None,
                    category: None,
                },
                AnnouncementsStatus {
                    name: "member-announcements".into(),
                    id: None,
                    named: None,
                    category: None,
                },
                AnnouncementsStatus {
                    name: "public-announcements".into(),
                    id: Some("201".into()),
                    named: Some("announcements".into()),
                    category: Some("Public".into()),
                },
            ]
        );
        let ids: Vec<_> = report.categories.iter().map(|c| c.id.as_deref()).collect();
        assert_eq!(
            ids,
            vec![Some("101"), Some("102"), Some("103"), Some("104")]
        );

        // Public dragged above Members.
        let mut moved = server(vec![]);
        moved.channels[3].position = 1;
        assert!(!check(&[], &moved).categories_in_order);
    }

    #[test]
    fn says_whether_the_roles_stand_in_order_and_within_reach() {
        let report = check(&[], &server(vec![]));
        assert!(report.roles_in_order);
        assert!(report.roles_out_of_reach.is_empty());
        assert_eq!(
            report.bot_role,
            Some(BotRole {
                id: "99".into(),
                name: "Blood Legion bot".into(),
                above: vec![],
                shown_separately: true,
                mentionable: false,
            })
        );

        // Member dragged above Leader, and Leader above the bot's own role.
        let mut moved = server(vec![]);
        moved.role_positions.insert(RoleId::new(5), 8);
        moved.role_positions.insert(RoleId::new(1), 11);
        let report = check(&[], &moved);
        assert!(!report.roles_in_order);
        assert_eq!(report.roles_out_of_reach, vec!["Leader".to_string()]);
        assert_eq!(report.bot_role.unwrap().above, vec!["Leader".to_string()]);

        // Shown in the member list with everyone else.
        let mut mixed = server(vec![]);
        mixed.hoisted.clear();
        assert!(!check(&[], &mixed).bot_role.unwrap().shown_separately);

        // Anyone may mention it.
        mixed.mentionable.insert(RoleId::new(99));
        assert!(check(&[], &mixed).bot_role.unwrap().mentionable);
    }

    #[test]
    fn a_role_or_category_the_server_lacks_is_reported_once_not_against_everyone() {
        let mut lacking = server(vec![member(10, "pal", &[])]);
        lacking.role_names.remove(&RoleId::new(5));
        lacking.channels.retain(|channel| channel.id != 101);
        // A friend on the site is due the Member role.
        let report = check(&[account("10", "Pal", Rank::Friend)], &lacking);
        let member = report.roles.iter().find(|r| r.name == "Member").unwrap();
        assert_eq!(member.id, None);
        assert!(report.members.is_empty());
        let officers = report
            .categories
            .iter()
            .find(|c| c.name == "Officers")
            .unwrap();
        assert_eq!(officers.id, None);
        assert!(report.categories_in_order);
    }

    #[test]
    fn lists_the_servers_other_roles_and_their_holders() {
        let members = vec![
            member(10, "pal", &[6]),
            member(20, "old pal", &[6, MEMBER]),
            member(30, "booster", &[7]),
        ];
        let report = check(&[], &server(members));
        // Not the bot's roles, its own, or the booster role an integration keeps.
        assert_eq!(
            report.other_roles,
            vec![OtherRole {
                id: "6".into(),
                name: "Friend".into(),
                holders: 2,
            }]
        );
    }

    #[test]
    fn says_whether_the_announcements_stand_in_order_at_the_top() {
        let top = |id, name: &str, position| Channel {
            parent: None,
            position,
            ..text(id, name, 0)
        };
        let mut snapshot = server(vec![]);
        snapshot
            .objects
            .insert(RAIDER_ANNOUNCEMENTS.key.into(), 301);
        snapshot
            .objects
            .insert(MEMBER_ANNOUNCEMENTS.key.into(), 302);
        snapshot
            .objects
            .insert(PUBLIC_ANNOUNCEMENTS.key.into(), 303);
        snapshot.channels.extend([
            top(303, "public-announcements", 0),
            top(400, "rules", 1),
            top(302, "member-announcements", 2),
            top(301, "raider-announcements", 3),
        ]);
        assert!(!check(&[], &snapshot).announcements_in_order);
        for channel in &mut snapshot.channels {
            match channel.id {
                301 => channel.position = 0,
                303 => channel.position = 3,
                _ => {}
            }
        }
        assert!(check(&[], &snapshot).announcements_in_order);
    }

    #[test]
    fn says_which_of_the_bots_permission_entries_differ() {
        let mut snapshot = server(vec![]);
        let bot = UserId::new(1000);
        let roles = Roles([1, 2, 3, 4, 5].map(RoleId::new));
        // Officers (101) as the bot sets it; Public (104) with nothing; the rest left out.
        snapshot.channels[0].overwrites =
            category_overwrites(CATEGORIES[0].audience, &roles, RoleId::new(100), bot);
        snapshot
            .channels
            .retain(|c| [101, 104, 201].contains(&c.id));
        let report = check(&[], &snapshot);
        let differing: Vec<(&str, Vec<String>)> = report
            .permissions
            .as_ref()
            .unwrap()
            .iter()
            .map(|status| (status.name.as_str(), status.differing.clone()))
            .collect();
        assert_eq!(
            differing,
            vec![
                ("Officers", vec![]),
                ("Public", vec!["@everyone".to_string()]),
                (
                    "announcements",
                    vec![
                        "@everyone".to_string(),
                        "Leader".to_string(),
                        "Officer".to_string()
                    ]
                ),
            ]
        );

        // Without all the bot's roles, nothing to check them against.
        snapshot.role_names.remove(&RoleId::new(4));
        assert_eq!(check(&[], &snapshot).permissions, None);
    }

    #[test]
    fn groups_everyone_in_the_server() {
        let members = vec![
            member(10, "Linked", &[MEMBER, 6]),
            member(20, "holder", &[OFFICER]),
            member(30, "pal", &[6]),
            member(40, "quiet", &[]),
        ];
        let report = check(&[account("10", "Ey", Rank::Member)], &server(members));
        let names = |group: &[Person]| -> Vec<String> {
            group.iter().map(|p| p.discord_name.clone()).collect()
        };
        let everyone = &report.everyone;
        assert_eq!(
            everyone.linked,
            vec![Person {
                discord_id: "10".into(),
                discord_name: "Linked".into(),
                username: Some("Ey".into()),
                // Top first: Friend (at 7) stands above Member (at 1).
                roles: vec!["Friend".into(), "Member".into()],
            }]
        );
        assert_eq!(names(&everyone.unlinked_ours), ["holder"]);
        assert_eq!(names(&everyone.unlinked_others), ["pal"]);
        assert_eq!(names(&everyone.no_roles), ["quiet"]);
        assert_eq!(names(&everyone.bots), ["Blood Legion bot"]);
    }

    #[test]
    fn finds_the_channels_not_synced_with_their_category() {
        let entry = |id: u64, allow: u64| PermissionOverwrite {
            allow: serenity::all::Permissions::from_bits_truncate(allow),
            deny: serenity::all::Permissions::empty(),
            kind: PermissionOverwriteType::Role(RoleId::new(id)),
        };
        let mut snapshot = server(vec![]);
        // Members (103) lets roles 1 and 5 see; #general (202) matches in another order, #chat
        // (203) has an entry of its own.
        snapshot.channels[2].overwrites = vec![entry(1, 1024), entry(5, 1024)];
        snapshot.channels[5].overwrites = vec![entry(5, 1024), entry(1, 1024)];
        snapshot.channels[6].overwrites = vec![entry(1, 1024), entry(5, 1024), entry(9, 2048)];
        let report = check(&[], &snapshot);
        assert_eq!(
            report.unsynced,
            vec![Unsynced {
                id: "203".into(),
                name: "chat".into(),
                category: "Members".into(),
            }]
        );
    }
}
