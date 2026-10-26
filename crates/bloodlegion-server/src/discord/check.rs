//! The role check, for a superuser: where the server's roles differ from the site's ranks. The bot
//! hands out no roles, so this is how someone finds who needs one: linked members not in the server
//! or without their rank's role (or with another of the bot's besides), and people holding one of
//! the bot's roles with no linked account. It reads Discord and changes nothing.

use std::collections::HashMap;

use anyhow::Context;
use serenity::all::{GuildId, Http, Member, RoleId};
use sqlx::PgPool;

use super::db::{self, LinkedAccount};
use super::layout::Access;
use crate::guild::Rank;
use crate::{Error, Problem, Result, State};

#[derive(Debug, thiserror::Error, Problem)]
pub enum CheckError {
    #[error("the bot is not set up")]
    #[problem(
        status = CONFLICT,
        title = "Bot Not Set Up",
        detail = "Set the bot's token and the server's id first."
    )]
    NotSetUp,
    #[error("discord: {0}")]
    #[problem(
        status = BAD_GATEWAY,
        title = "Discord Unavailable",
        detail = "Discord could not be asked about the server's members. Is the bot in the server, \
                  with the Server Members intent on?"
    )]
    Discord(String),
}

/// Someone in the server, as the check needs them.
#[derive(Debug, Clone)]
pub struct ServerMember {
    pub id: u64,
    pub name: String,
    pub roles: Vec<RoleId>,
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

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Report {
    /// The bot's roles the server does not have (not made yet, or deleted), by the name the bot
    /// gives them. The check of who holds them leaves these out.
    pub missing_roles: Vec<String>,
    /// How many members have a linked Discord account.
    pub linked: usize,
    pub members: Vec<MemberStatus>,
    pub unlinked: Vec<Unlinked>,
}

/// The check itself, given what the site and the server hold. `roles` is the bot's roles where
/// they exist; `names` names them as the server does.
pub fn check(
    linked: &[LinkedAccount],
    members: &[ServerMember],
    roles: &HashMap<Access, RoleId>,
    names: &HashMap<RoleId, String>,
) -> Report {
    let name = |access: Access| {
        roles
            .get(&access)
            .and_then(|id| names.get(id))
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
    let in_server: HashMap<u64, &ServerMember> = members.iter().map(|m| (m.id, m)).collect();

    let mut statuses = Vec::new();
    for account in linked {
        let wanted = Access::of(account.rank);
        let member = account
            .subject
            .parse::<u64>()
            .ok()
            .and_then(|id| in_server.get(&id).copied());
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
    let mut unlinked: Vec<Unlinked> = members
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
    unlinked.sort_by(|a, b| {
        a.discord_name
            .to_lowercase()
            .cmp(&b.discord_name.to_lowercase())
    });

    Report {
        missing_roles: Access::ALL
            .into_iter()
            .filter(|access| !roles.contains_key(access))
            .map(|access| access.role_name().to_string())
            .collect(),
        linked: linked.len(),
        members: statuses,
        unlinked,
    }
}

/// Runs the check against the server, reading it with the bot's token.
pub async fn run(state: &State) -> Result<Report, CheckError> {
    let settings = db::settings(&state.pool)
        .await
        .context("reading the bot's settings")?;
    let (Some(guild), Some(token)) = (settings.guild_id, settings.bot_token) else {
        return Err(Error::External(CheckError::NotSetUp));
    };
    let token = state
        .decrypt(&token)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let http = Http::new(&token);
    let guild = GuildId::new(guild);
    let discord = |error: serenity::Error| Error::External(CheckError::Discord(error.to_string()));

    let names: HashMap<RoleId, String> = guild
        .roles(&http)
        .await
        .map_err(discord)?
        .into_iter()
        .map(|(id, role)| (id, role.name))
        .collect();
    let roles = bot_roles(&state.pool, &names).await?;
    let members = server_members(&http, guild)
        .await
        .map_err(|error| Error::External(CheckError::Discord(format!("{error:#}"))))?;
    let linked = db::linked_accounts(&state.pool)
        .await
        .context("listing the linked Discord accounts")?;
    Ok(check(&linked, &members, &roles, &names))
}

/// The bot's roles that exist in the server, by the ids it stored when it made them.
async fn bot_roles(
    pool: &PgPool,
    names: &HashMap<RoleId, String>,
) -> anyhow::Result<HashMap<Access, RoleId>> {
    let mut roles = HashMap::new();
    for access in Access::ALL {
        if let Some(id) = db::object_id(pool, access.key()).await?.map(RoleId::new)
            && names.contains_key(&id)
        {
            roles.insert(access, id);
        }
    }
    Ok(roles)
}

async fn server_members(http: &Http, guild: GuildId) -> anyhow::Result<Vec<ServerMember>> {
    let mut all = Vec::new();
    let mut after = None;
    loop {
        let page: Vec<Member> = guild.members(http, Some(1000), after).await?;
        let full = page.len() == 1000;
        after = page.last().map(|member| member.user.id);
        all.extend(page.into_iter().filter(|m| !m.user.bot).map(|m| {
            ServerMember {
                id: m.user.id.get(),
                name: m
                    .nick
                    .clone()
                    .or(m.user.global_name.clone())
                    .unwrap_or(m.user.name),
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
            roles: roles.iter().copied().map(RoleId::new).collect(),
        }
    }

    fn setup() -> (HashMap<Access, RoleId>, HashMap<RoleId, String>) {
        let roles: HashMap<Access, RoleId> = Access::ALL
            .into_iter()
            .enumerate()
            .map(|(n, access)| (access, RoleId::new(n as u64 + 1)))
            .collect();
        let mut names: HashMap<RoleId, String> = roles
            .iter()
            .map(|(access, id)| (*id, access.role_name().to_string()))
            .collect();
        // Renamed by hand: the check uses the server's name.
        names.insert(roles[&Access::Raider], "Raiders".into());
        (roles, names)
    }

    #[test]
    fn lists_only_whats_out_of_line() {
        let (roles, names) = setup();
        let (officer, raider, member_role) = (2, 3, 5);
        let linked = [
            account("10", "Right", Rank::Officer),
            account("20", "Lacking", Rank::Raider),
            account("30", "Promoted", Rank::Officer),
            account("40", "Away", Rank::Member),
        ];
        let members = [
            member(10, "right", &[officer, 99]),
            member(20, "lacking", &[]),
            member(30, "promoted", &[officer, raider]),
            member(50, "stranger", &[member_role]),
            member(60, "nobody", &[99]),
        ];
        let report = check(&linked, &members, &roles, &names);
        assert_eq!(report.linked, 4);
        assert!(report.missing_roles.is_empty());
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
        assert_eq!(
            report.unlinked,
            vec![Unlinked {
                discord_id: "50".into(),
                discord_name: "stranger".into(),
                roles: vec!["Member".into()],
            }]
        );
    }

    #[test]
    fn a_role_the_server_lacks_is_reported_once_not_against_everyone() {
        let (mut roles, names) = setup();
        roles.remove(&Access::Friend);
        let report = check(
            &[account("10", "Pal", Rank::Friend)],
            &[member(10, "pal", &[])],
            &roles,
            &names,
        );
        assert_eq!(report.missing_roles, vec!["Friend".to_string()]);
        assert!(report.members.is_empty());
    }
}
