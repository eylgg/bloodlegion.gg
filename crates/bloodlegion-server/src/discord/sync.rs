//! Bringing the server in line with the site, through Discord's HTTP API: the bot's roles and
//! categories (made when missing, its own permission entries set), each linked member's role, and
//! the stored copy of #announcements.
//!
//! It never deletes anything in Discord. What it made is found by the id it stored; failing that,
//! by name, so a role or category made by hand before the bot is taken over rather than doubled.

use std::collections::HashMap;

use anyhow::Context;
use serenity::all::{
    ChannelId, ChannelType, CreateChannel, EditRole, GetMessages, GuildChannel, GuildId, Http,
    Member, Message, MessageType, Permissions, RoleId, UserId,
};
use sqlx::PgPool;
use time::OffsetDateTime;

use super::db;
use super::layout::{
    ANNOUNCEMENTS_KEY, ANNOUNCEMENTS_NAME, Access, CATEGORIES, Layout, PUBLIC, Roles,
    announcements_overwrites, category_overwrites, role_changes,
};
use super::markdown::Mentions;

/// Shown in the server's audit log for everything the bot does.
const REASON: &str = "Blood Legion site sync";

/// How many of #announcements' latest posts a catch-up reads.
const CATCH_UP: u8 = 50;

/// Makes whatever of the bot's roles and categories is missing and sets its permission entries on
/// them, returning where everything is.
pub async fn layout(
    http: &Http,
    pool: &PgPool,
    guild: GuildId,
    bot: UserId,
) -> anyhow::Result<Layout> {
    let existing = guild
        .roles(http)
        .await
        .context("listing the server's roles")?;
    let mut ids = HashMap::new();
    for access in Access::ALL {
        let stored = db::object_id(pool, access.key()).await?.map(RoleId::new);
        let id = match stored.filter(|id| existing.contains_key(id)) {
            Some(id) => id,
            None => {
                let id = match existing
                    .values()
                    .find(|role| role.name == access.role_name())
                {
                    Some(role) => role.id,
                    None => {
                        let role = guild
                            .create_role(
                                http,
                                EditRole::new()
                                    .name(access.role_name())
                                    .hoist(true)
                                    .mentionable(false)
                                    .permissions(Permissions::empty())
                                    .audit_log_reason(REASON),
                            )
                            .await
                            .with_context(|| format!("making the {} role", access.role_name()))?;
                        tracing::info!(role = access.role_name(), "discord: made a role");
                        role.id
                    }
                };
                db::set_object_id(pool, access.key(), id.get()).await?;
                id
            }
        };
        ids.insert(access, id);
    }
    let roles = Roles {
        officer: ids[&Access::Officer],
        raider: ids[&Access::Raider],
        member: ids[&Access::Member],
    };
    // @everyone's id is the server's.
    let everyone = RoleId::new(guild.get());

    let channels = guild
        .channels(http)
        .await
        .context("listing the server's channels")?;
    let mut public = None;
    for category in &CATEGORIES {
        let id = ensure_channel(
            http,
            pool,
            guild,
            &channels,
            category.key,
            category.name,
            None,
        )
        .await?;
        for overwrite in category_overwrites(category.audience, &roles, everyone, bot) {
            id.create_permission(http, overwrite)
                .await
                .with_context(|| {
                    format!("setting permissions on the {} category", category.name)
                })?;
        }
        if category.key == PUBLIC.key {
            public = Some(id);
        }
    }
    let public = public.expect("the categories include Public");

    let announcements = ensure_channel(
        http,
        pool,
        guild,
        &channels,
        ANNOUNCEMENTS_KEY,
        ANNOUNCEMENTS_NAME,
        Some(public),
    )
    .await?;
    for overwrite in announcements_overwrites(&roles, everyone) {
        announcements
            .create_permission(http, overwrite)
            .await
            .context("setting permissions on #announcements")?;
    }
    Ok(Layout {
        roles,
        announcements,
    })
}

/// The channel the bot keeps under `key`: a category when `parent` is `None`, else a text channel
/// in `parent`. Found by its stored id, else by name (in `parent`), else made.
async fn ensure_channel(
    http: &Http,
    pool: &PgPool,
    guild: GuildId,
    existing: &HashMap<ChannelId, GuildChannel>,
    key: &str,
    name: &str,
    parent: Option<ChannelId>,
) -> anyhow::Result<ChannelId> {
    let kind = if parent.is_some() {
        ChannelType::Text
    } else {
        ChannelType::Category
    };
    let fits = |channel: &GuildChannel| match parent {
        None => channel.kind == ChannelType::Category,
        // A text channel may have been made an announcement (news) channel since.
        Some(_) => matches!(channel.kind, ChannelType::Text | ChannelType::News),
    };
    let stored = db::object_id(pool, key).await?.map(ChannelId::new);
    if let Some(id) = stored.filter(|id| existing.get(id).is_some_and(fits)) {
        return Ok(id);
    }
    let adopted = existing.values().find(|channel| {
        fits(channel) && channel.name.eq_ignore_ascii_case(name) && channel.parent_id == parent
    });
    let id = match adopted {
        Some(channel) => channel.id,
        None => {
            let mut builder = CreateChannel::new(name).kind(kind).audit_log_reason(REASON);
            if let Some(parent) = parent {
                builder = builder.category(parent);
            }
            let channel = guild
                .create_channel(http, builder)
                .await
                .with_context(|| format!("making {name}"))?;
            tracing::info!(name, "discord: made a channel");
            channel.id
        }
    };
    db::set_object_id(pool, key, id.get()).await?;
    Ok(id)
}

/// Gives every member in the server exactly the bot's role their linked account's rank calls for,
/// and takes the bot's roles from anyone without one. A member that fails is logged and skipped.
pub async fn members(
    http: &Http,
    pool: &PgPool,
    guild: GuildId,
    roles: &Roles,
) -> anyhow::Result<()> {
    let ranks: HashMap<String, Access> = db::linked_ranks(pool)
        .await?
        .into_iter()
        .map(|(subject, rank)| (subject, Access::of(rank)))
        .collect();
    let mut after = None;
    loop {
        let page = guild
            .members(http, Some(1000), after)
            .await
            .context("listing the server's members")?;
        for member in page.iter().filter(|member| !member.user.bot) {
            let access = ranks.get(&member.user.id.get().to_string()).copied();
            if let Err(error) = apply(http, guild, member, access, roles).await {
                let error = format!("{error:#}");
                tracing::warn!(user = %member.user.id, error, "discord: could not set roles");
            }
        }
        if page.len() < 1000 {
            return Ok(());
        }
        after = page.last().map(|member| member.user.id);
    }
}

/// [`members`], for one member (one who just joined).
pub async fn member(
    http: &Http,
    pool: &PgPool,
    guild: GuildId,
    roles: &Roles,
    member: &Member,
) -> anyhow::Result<()> {
    if member.user.bot {
        return Ok(());
    }
    let rank = db::linked_rank(pool, &member.user.id.get().to_string()).await?;
    apply(http, guild, member, rank.map(Access::of), roles).await
}

async fn apply(
    http: &Http,
    guild: GuildId,
    member: &Member,
    access: Option<Access>,
    roles: &Roles,
) -> anyhow::Result<()> {
    let (add, remove) = role_changes(&member.roles, access, roles);
    for role in add {
        http.add_member_role(guild, member.user.id, role, Some(REASON))
            .await
            .context("giving a role")?;
    }
    for role in remove {
        http.remove_member_role(guild, member.user.id, role, Some(REASON))
            .await
            .context("taking a role")?;
    }
    Ok(())
}

/// The names of the server's roles and channels, for the mentions in a post.
pub struct Names {
    roles: HashMap<RoleId, String>,
    channels: HashMap<ChannelId, String>,
}

impl Names {
    pub async fn fetch(http: &Http, guild: GuildId) -> anyhow::Result<Names> {
        let roles = guild
            .roles(http)
            .await
            .context("listing the server's roles")?;
        let channels = guild
            .channels(http)
            .await
            .context("listing the server's channels")?;
        Ok(Names {
            roles: roles
                .into_iter()
                .map(|(id, role)| (id, role.name))
                .collect(),
            channels: channels
                .into_iter()
                .map(|(id, channel)| (id, channel.name))
                .collect(),
        })
    }
}

/// Whether a message in #announcements is a post (not a pin notice, a thread starting, ...).
fn is_post(message: &Message) -> bool {
    matches!(
        message.kind,
        MessageType::Regular | MessageType::InlineReply
    )
}

/// The name a member goes by in the server: their nickname, else their display name, else their
/// username.
fn display_name(user: &serenity::all::User, nick: Option<&String>) -> String {
    nick.or(user.global_name.as_ref())
        .unwrap_or(&user.name)
        .clone()
}

fn timestamp(at: serenity::all::Timestamp) -> anyhow::Result<OffsetDateTime> {
    Ok(OffsetDateTime::from_unix_timestamp(at.unix_timestamp())?)
}

/// Stores (or updates) a message from #announcements, or drops it when it is not a post.
pub async fn store(pool: &PgPool, names: &Names, message: &Message) -> anyhow::Result<()> {
    if !is_post(message) {
        return Ok(db::delete_announcements(pool, &[message.id.get()]).await?);
    }
    let mut mentions = Mentions::default();
    for user in &message.mentions {
        let nick = user.member.as_ref().and_then(|member| member.nick.as_ref());
        mentions
            .users
            .insert(user.id.get().to_string(), display_name(user, nick));
    }
    for role in &message.mention_roles {
        if let Some(name) = names.roles.get(role) {
            mentions.roles.insert(role.get().to_string(), name.clone());
        }
    }
    for (id, name) in &names.channels {
        if message.content.contains(&format!("<#{id}>")) {
            mentions.channels.insert(id.get().to_string(), name.clone());
        }
    }
    let nick = message
        .member
        .as_ref()
        .and_then(|member| member.nick.as_ref());
    db::upsert_announcement(
        pool,
        &db::Announcement {
            message_id: message.id.get(),
            author: display_name(&message.author, nick),
            content: message.content.clone(),
            mentions: serde_json::to_value(&mentions)?,
            posted_at: timestamp(message.timestamp)?,
            edited_at: message.edited_timestamp.map(timestamp).transpose()?,
        },
    )
    .await?;
    Ok(())
}

/// Catches the stored copy up with #announcements' latest posts: new and edited ones stored,
/// deleted ones dropped (as far back as the read goes).
pub async fn announcements(
    http: &Http,
    pool: &PgPool,
    guild: GuildId,
    channel: ChannelId,
) -> anyhow::Result<()> {
    let names = Names::fetch(http, guild).await?;
    let messages = channel
        .messages(http, GetMessages::new().limit(CATCH_UP))
        .await
        .context("reading #announcements")?;
    for message in &messages {
        store(pool, &names, message).await?;
    }
    let seen: Vec<u64> = messages
        .iter()
        .filter(|message| is_post(message))
        .map(|message| message.id.get())
        .collect();
    let since = messages
        .iter()
        .map(|message| timestamp(message.timestamp))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .min();
    let whole_channel = messages.len() < usize::from(CATCH_UP);
    db::prune_announcements(pool, &seen, since, whole_channel).await?;
    Ok(())
}
