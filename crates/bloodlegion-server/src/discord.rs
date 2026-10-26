//! The guild's Discord server, run by a bot: it keeps the bot's roles and categories as the site
//! has them (`layout`) and mirrors the public #announcements channel for the site (`markdown`
//! renders it). It hands out no roles; the role check (`check`) shows a superuser who lacks theirs.
//!
//! One bot connection serves every instance of the server. Each instance waits on a Postgres
//! advisory lock held on a connection of its own, and the one holding it runs the bot; when that
//! instance goes away its connection closes, the lock frees, and another takes over. Two
//! connections at once would act on every Discord event twice.
//!
//! A superuser sets the server's id and the bot's token on the site (`discord_settings`, the token
//! encrypted). Until both are set there is no bot, and changing either restarts it.

mod api;
mod check;
mod db;
mod layout;
mod markdown;
mod sync;

use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::Context as _;
use serenity::all::{
    ChannelId, Client, Context, EventHandler, GatewayIntents, GuildId, Http, Message, MessageId,
    MessageUpdateEvent, UserId,
};
use sqlx::PgPool;
use sqlx::postgres::PgListener;

pub use api::{admin_router, router};
use layout::Layout;

use crate::State;

/// The slug the Discord sign-in provider is registered under: a member's Discord account is their
/// credential there, its subject their Discord user id.
pub const DISCORD_PROVIDER_SLUG: &str = "discord";

/// The advisory-lock key the bot runs under ("discord" in ASCII).
const LOCK_KEY: i64 = 0x0064_6973_636f_7264;

/// How long an instance waits before trying for the lock again, or restarting a stopped bot.
const RETRY: Duration = Duration::from_secs(30);

/// How often the running bot checks it still holds the lock.
const LOCK_CHECK: Duration = Duration::from_secs(15);

/// How often the roles and categories are checked, in case someone changed them by hand.
const LAYOUT_EVERY: Duration = Duration::from_secs(60 * 60);

/// Where the bot's settings changing is announced.
const SETTINGS_CHANNEL: &str = "discord_settings";

/// What the bot hears from the gateway: messages, with their text. Message content is privileged:
/// it needs turning on for the bot in Discord's developer portal.
const INTENTS: GatewayIntents = GatewayIntents::GUILDS
    .union(GatewayIntents::GUILD_MESSAGES)
    .union(GatewayIntents::MESSAGE_CONTENT);

struct Config {
    token: String,
    guild: GuildId,
}

/// The bot's settings, or `None` until a superuser has set both.
async fn load(state: &State) -> anyhow::Result<Option<Config>> {
    let settings = db::settings(&state.pool)
        .await
        .context("reading the bot's settings")?;
    let (Some(guild), Some(token)) = (settings.guild_id, settings.bot_token) else {
        return Ok(None);
    };
    let token = state
        .decrypt(&token)
        .map_err(|error| anyhow::anyhow!("{error}"))
        .context("decrypting the bot's token")?;
    Ok(Some(Config {
        token,
        guild: GuildId::new(guild),
    }))
}

/// Starts this instance's part in running the bot: waiting for the lock, and running the bot while
/// it holds it, again after any failure.
pub fn spawn(state: State) {
    tokio::spawn(async move {
        loop {
            if let Err(error) = lead(&state).await {
                warn("the bot stopped", &error);
            }
            tokio::time::sleep(RETRY).await;
        }
    });
}

/// Waits to hold the lock, then runs the bot for as long as it does, with the settings as they
/// stand: waiting while they are unset, and starting again when they change.
async fn lead(state: &State) -> anyhow::Result<()> {
    // A connection of its own, outside the pool: the lock lasts exactly as long as it does.
    let mut lock = state
        .pool
        .acquire()
        .await
        .context("connecting for the bot's lock")?
        .detach();
    while !sqlx::query_scalar!(r#"SELECT pg_try_advisory_lock($1) AS "locked!""#, LOCK_KEY)
        .fetch_one(&mut lock)
        .await
        .context("trying for the bot's lock")?
    {
        tokio::time::sleep(RETRY).await;
    }
    let mut changes = PgListener::connect_with(&state.pool)
        .await
        .context("listening for changes")?;
    changes
        .listen(SETTINGS_CHANNEL)
        .await
        .context("listening for changes")?;
    loop {
        match load(state).await? {
            Some(config) => {
                tracing::info!("discord: this instance runs the bot");
                run(&state.pool, &config, &mut lock, &mut changes).await?;
                tracing::info!("discord: the bot's settings changed; starting it again");
            }
            None => {
                tracing::info!(
                    "discord: waiting for a superuser to set the bot's token and server"
                );
                wait_for_settings(&mut lock, &mut changes).await?;
            }
        }
    }
}

/// Returns once the settings change, keeping an eye on the lock meanwhile.
async fn wait_for_settings(
    lock: &mut sqlx::PgConnection,
    changes: &mut PgListener,
) -> anyhow::Result<()> {
    let mut lock_check = later(LOCK_CHECK);
    loop {
        tokio::select! {
            notification = changes.recv() => {
                let notification = notification.context("listening for changes")?;
                if notification.channel() == SETTINGS_CHANNEL {
                    return Ok(());
                }
            }
            _ = lock_check.tick() => still_locked(lock).await?,
        }
    }
}

/// A timer whose first tick is one period from now.
fn later(every: Duration) -> tokio::time::Interval {
    tokio::time::interval_at(tokio::time::Instant::now() + every, every)
}

/// What the event handler and the timers share.
struct Bot {
    pool: PgPool,
    guild: GuildId,
    /// Set before the gateway starts; changes when a check remakes something.
    layout: RwLock<Option<Layout>>,
}

impl Bot {
    fn layout(&self) -> Option<Layout> {
        *self
            .layout
            .read()
            .expect("the layout lock is never poisoned")
    }

    fn is_announcements(&self, channel: ChannelId) -> bool {
        self.layout()
            .is_some_and(|layout| layout.announcements == channel)
    }

    async fn sync_layout(&self, http: &Http, me: UserId) -> anyhow::Result<Layout> {
        let layout = sync::layout(http, &self.pool, self.guild, me).await?;
        *self
            .layout
            .write()
            .expect("the layout lock is never poisoned") = Some(layout);
        Ok(layout)
    }
}

/// Runs the bot until it fails (an error) or its settings change (`Ok`).
async fn run(
    pool: &PgPool,
    config: &Config,
    lock: &mut sqlx::PgConnection,
    changes: &mut PgListener,
) -> anyhow::Result<()> {
    let bot = Arc::new(Bot {
        pool: pool.clone(),
        guild: config.guild,
        layout: RwLock::new(None),
    });
    let mut client = Client::builder(&config.token, INTENTS)
        .event_handler(Handler(bot.clone()))
        .await
        .context("setting up the Discord client")?;
    let http = client.http.clone();
    let me = http
        .get_current_user()
        .await
        .context("signing in as the bot (is its token right?)")?
        .id;

    let layout = bot.sync_layout(&http, me).await?;
    if let Err(error) = sync::announcements(&http, pool, config.guild, layout.announcements).await {
        warn("could not catch up #announcements", &error);
    }

    let shards = client.shard_manager.clone();
    let mut gateway = tokio::spawn(async move { client.start().await });
    let (mut lock_check, mut layout_check) = (later(LOCK_CHECK), later(LAYOUT_EVERY));

    let result = loop {
        tokio::select! {
            joined = &mut gateway => break match joined {
                Ok(Ok(())) => Err(anyhow::anyhow!("the Discord gateway closed")),
                Ok(Err(error)) => Err(error).context(GATEWAY_FAILED),
                Err(error) => Err(error).context("the Discord gateway task"),
            },
            notification = changes.recv() => match notification {
                Ok(_) => break Ok(()),
                Err(error) => break Err(error).context("listening for changes"),
            },
            _ = layout_check.tick() => if let Err(error) = bot.sync_layout(&http, me).await {
                warn("could not check the roles and categories", &error);
            },
            _ = lock_check.tick() => if let Err(error) = still_locked(lock).await {
                break Err(error);
            },
        }
    };
    shards.shutdown_all().await;
    gateway.abort();
    result
}

/// Why the gateway most often refuses the bot.
const GATEWAY_FAILED: &str = "the Discord gateway failed (is the message content intent on?)";

/// Whether the lock's connection is still there (and with it the lock).
async fn still_locked(lock: &mut sqlx::PgConnection) -> anyhow::Result<()> {
    sqlx::query("SELECT 1")
        .execute(lock)
        .await
        .context("lost the bot's lock")?;
    Ok(())
}

fn warn(what: &str, error: &anyhow::Error) {
    tracing::warn!(error = format!("{error:#}"), "discord: {what}");
}

struct Handler(Arc<Bot>);

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, message: Message) {
        if message.guild_id != Some(self.0.guild) || !self.0.is_announcements(message.channel_id) {
            return;
        }
        let stored = async {
            let names = sync::Names::fetch(&ctx.http, self.0.guild).await?;
            sync::store(&self.0.pool, &names, &message).await
        };
        if let Err(error) = stored.await {
            warn("could not store an announcement", &error);
        }
    }

    async fn message_update(
        &self,
        ctx: Context,
        _old: Option<Message>,
        _new: Option<Message>,
        event: MessageUpdateEvent,
    ) {
        if !self.0.is_announcements(event.channel_id) {
            return;
        }
        // The event carries only what changed; the message itself has the rest.
        let stored = async {
            let message = event.channel_id.message(&ctx.http, event.id).await?;
            let names = sync::Names::fetch(&ctx.http, self.0.guild).await?;
            sync::store(&self.0.pool, &names, &message).await
        };
        if let Err(error) = stored.await {
            warn("could not store an edited announcement", &error);
        }
    }

    async fn message_delete(
        &self,
        _ctx: Context,
        channel: ChannelId,
        message: MessageId,
        _guild: Option<GuildId>,
    ) {
        self.deleted(channel, &[message]).await;
    }

    async fn message_delete_bulk(
        &self,
        _ctx: Context,
        channel: ChannelId,
        messages: Vec<MessageId>,
        _guild: Option<GuildId>,
    ) {
        self.deleted(channel, &messages).await;
    }
}

impl Handler {
    async fn deleted(&self, channel: ChannelId, messages: &[MessageId]) {
        if !self.0.is_announcements(channel) {
            return;
        }
        let ids: Vec<u64> = messages.iter().map(|id| id.get()).collect();
        if let Err(error) = db::delete_announcements(&self.0.pool, &ids).await {
            tracing::warn!(error = %error, "discord: could not drop a deleted announcement");
        }
    }
}
