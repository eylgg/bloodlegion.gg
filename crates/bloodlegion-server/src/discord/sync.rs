//! Bringing the server in line with the site, through Discord's HTTP API: the bot's roles,
//! categories, and announcements channels (found, never made: a superuser makes what is missing
//! with the Discord page's Create button), its own permission entries on them, and the stored copy
//! of #public-announcements.
//!
//! It never removes anything in Discord, and gives no member a role. What it made is found by the
//! id it stored; failing that, by name, so a role or channel made by hand before the bot is taken
//! over rather than doubled.

use std::collections::HashMap;

use anyhow::Context;
use serenity::all::{
    ChannelId, ChannelType, GetMessages, GuildChannel, GuildId, Http, Message, MessageType, Role,
    RoleId, UserId,
};
use sqlx::PgPool;
use time::OffsetDateTime;

use super::db;
use super::layout::{
    ANNOUNCEMENTS, Access, CATEGORIES, Layout, PUBLIC_ANNOUNCEMENTS, Roles,
    announcements_overwrites, category_overwrites,
};
use super::markdown::Mentions;

/// Shown in the server's audit log for everything the bot does.
pub(super) const REASON: &str = "Blood Legion site sync";

/// How many of #public-announcements' latest posts a catch-up reads.
const CATCH_UP: u8 = 50;

/// Finds the bot's roles, categories, and announcements channels, and sets its permission entries
/// on them, returning where everything is. It makes nothing: what is missing waits for a
/// superuser's Create button (`fix`), and until all six roles are there no permissions are set.
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
    let mut ids = Vec::new();
    for access in Access::ALL {
        if let Some(id) = find_role(pool, &existing, access).await? {
            ids.push(id);
        }
    }
    let roles = ids.try_into().ok().map(Roles);
    // @everyone's id is the server's.
    let everyone = RoleId::new(guild.get());

    let channels = guild
        .channels(http)
        .await
        .context("listing the server's channels")?;
    for category in &CATEGORIES {
        let Some(id) = find_channel(
            pool,
            &channels,
            category.key,
            category.name,
            ChannelType::Category,
        )
        .await?
        else {
            continue;
        };
        let Some(roles) = &roles else { continue };
        for overwrite in category_overwrites(category.audience, roles, everyone, bot) {
            id.create_permission(http, overwrite)
                .await
                .with_context(|| {
                    format!("setting permissions on the {} category", category.name)
                })?;
        }
    }

    let mut public = None;
    for channel in ANNOUNCEMENTS {
        let Some(id) = find_channel(
            pool,
            &channels,
            channel.key,
            channel.name,
            ChannelType::Text,
        )
        .await?
        else {
            continue;
        };
        if channel.key == PUBLIC_ANNOUNCEMENTS.key {
            public = Some(id);
        }
        let Some(roles) = &roles else { continue };
        for overwrite in announcements_overwrites(channel.audience, roles, everyone, bot) {
            id.create_permission(http, overwrite)
                .await
                .with_context(|| format!("setting permissions on #{}", channel.name))?;
        }
    }
    Ok(Layout {
        announcements: public,
    })
}

/// The bot's role for `access`: found by its stored id, else by name; `None` when it is in neither
/// place.
pub(super) async fn find_role(
    pool: &PgPool,
    existing: &HashMap<RoleId, Role>,
    access: Access,
) -> anyhow::Result<Option<RoleId>> {
    let stored = db::object_id(pool, access.key()).await?.map(RoleId::new);
    if let Some(id) = stored.filter(|id| existing.contains_key(id)) {
        return Ok(Some(id));
    }
    let Some(adopted) = existing
        .values()
        .find(|role| role.name == access.role_name())
    else {
        return Ok(None);
    };
    db::set_object_id(pool, access.key(), adopted.id.get()).await?;
    Ok(Some(adopted.id))
}

/// The channel the bot keeps under `key`: a category, or a text channel at the top of the list (in
/// no category). Found by its stored id wherever it now is, else by name at the top of the list;
/// `None` when it is in neither place.
pub(super) async fn find_channel(
    pool: &PgPool,
    existing: &HashMap<ChannelId, GuildChannel>,
    key: &str,
    name: &str,
    kind: ChannelType,
) -> anyhow::Result<Option<ChannelId>> {
    let fits = |channel: &GuildChannel| match kind {
        ChannelType::Category => channel.kind == ChannelType::Category,
        // A text channel may have been made an announcement (news) channel since.
        _ => matches!(channel.kind, ChannelType::Text | ChannelType::News),
    };
    let stored = db::object_id(pool, key).await?.map(ChannelId::new);
    if let Some(id) = stored.filter(|id| existing.get(id).is_some_and(fits)) {
        return Ok(Some(id));
    }
    let Some(adopted) = existing.values().find(|channel| {
        fits(channel) && channel.name.eq_ignore_ascii_case(name) && channel.parent_id.is_none()
    }) else {
        return Ok(None);
    };
    db::set_object_id(pool, key, adopted.id.get()).await?;
    Ok(Some(adopted.id))
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

/// Whether a message in #public-announcements is a post (not a pin notice, a thread starting, ...).
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

/// Stores (or updates) a message from #public-announcements, or drops it when it is not a post.
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

/// Catches the stored copy up with #public-announcements' latest posts: new and edited ones stored,
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
        .context("reading #public-announcements")?;
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
