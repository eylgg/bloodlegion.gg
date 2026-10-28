use anyhow::Context;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serenity::all::{Http, HttpError};
use time::{OffsetDateTime, serde::iso8601};

use super::check::{self, Overview};
use super::db;
use super::fix::{self, FixError};
use super::layout::Access;
use super::markdown::{self, Mentions};
use crate::extract::{Admin, SameOrigin};
use crate::{Error, Problem, Result, State};

/// How many announcements the site shows.
const LATEST: i64 = 10;

/// A post from #public-announcements, rendered for the site.
#[derive(Debug, serde::Serialize)]
pub struct Announcement {
    /// The Discord message id, as a string: it does not fit a JavaScript number.
    pub id: String,
    /// The username of the member who posted it.
    pub author: String,
    /// The post as HTML (see `markdown`): escaped, so safe to put on the page as is.
    pub html: String,
    #[serde(with = "iso8601")]
    pub posted_at: OffsetDateTime,
    #[serde(with = "iso8601::option")]
    pub edited_at: Option<OffsetDateTime>,
}

/// `GET /api/announcements`: the latest posts in #public-announcements, newest first. Public, as
/// the channel is.
async fn latest(state: State) -> Result<Json<Vec<Announcement>>> {
    let posts = db::latest_announcements(&state.pool, LATEST)
        .await
        .context("reading the announcements")?;
    Ok(Json(
        posts
            .into_iter()
            .map(|post| {
                let db::Announcement { post, author } = post;
                let mentions: Mentions = serde_json::from_value(post.mentions).unwrap_or_default();
                Announcement {
                    id: post.message_id.to_string(),
                    author,
                    html: markdown::render(&post.content, &mentions),
                    posted_at: post.posted_at,
                    edited_at: post.edited_at,
                }
            })
            .collect(),
    ))
}

#[derive(Debug, thiserror::Error, Problem)]
pub enum SettingsError {
    #[error("invalid server id")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Invalid Server ID",
        detail = "A Discord server's id is a number, such as 123456789012345678."
    )]
    InvalidGuildId,
    #[error("token refused")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Token Refused",
        detail = "Discord did not accept that bot token. Copy it again from the Bot page of the application."
    )]
    TokenRefused,
    #[error("discord unavailable: {0}")]
    #[problem(
        status = BAD_GATEWAY,
        title = "Discord Unavailable",
        detail = "Discord could not be reached to check the token. Please try again."
    )]
    Unavailable(String),
}

/// The bot's settings as a superuser sees them: never the token itself, only whether there is one.
#[derive(Debug, serde::Serialize)]
pub struct SettingsView {
    /// As a string: it does not fit a JavaScript number.
    pub guild_id: Option<String>,
    pub has_bot_token: bool,
}

async fn view<E>(state: &State) -> Result<SettingsView, E> {
    let settings = db::settings(&state.pool)
        .await
        .context("reading the bot's settings")?;
    Ok(SettingsView {
        guild_id: settings.guild_id.map(|id| id.to_string()),
        has_bot_token: settings.bot_token.is_some(),
    })
}

/// `GET /api/admin/discord`: superusers only.
async fn settings(state: State, _admin: Admin) -> Result<Json<SettingsView>> {
    Ok(Json(view(&state).await?))
}

/// A change to the settings: an absent field stays as it is, `null` unsets it.
#[derive(Debug, serde::Deserialize)]
pub struct SettingsInput {
    #[serde(default, deserialize_with = "crate::api::double_option")]
    guild_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::api::double_option")]
    bot_token: Option<Option<String>>,
}

/// A server id as typed: digits, not zero.
fn parse_guild_id(input: &str) -> Option<u64> {
    input.trim().parse().ok().filter(|&id| id != 0)
}

/// `PUT /api/admin/discord`: superusers only. A new token is checked with Discord before it is
/// kept, then stored encrypted; the bot restarts with whatever changed.
async fn set_settings(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Json(input): Json<SettingsInput>,
) -> Result<Json<SettingsView>, SettingsError> {
    let guild_id = match input.guild_id {
        Some(Some(id)) => Some(Some(
            parse_guild_id(&id).ok_or(Error::External(SettingsError::InvalidGuildId))?,
        )),
        Some(None) => Some(None),
        None => None,
    };
    let bot_token = match input.bot_token {
        Some(Some(token)) => {
            let token = token.trim().to_string();
            if token.is_empty() {
                return Err(Error::External(SettingsError::TokenRefused));
            }
            check_token(&token).await?;
            Some(Some(
                state.encrypt(&token).context("encrypting the bot token")?,
            ))
        }
        Some(None) => Some(None),
        None => None,
    };
    if let Some(guild_id) = guild_id {
        db::set_guild_id(&state.pool, guild_id)
            .await
            .context("saving the server id")?;
    }
    if let Some(bot_token) = bot_token {
        db::set_bot_token(&state.pool, bot_token.as_ref())
            .await
            .context("saving the bot token")?;
    }
    Ok(Json(view(&state).await?))
}

/// Whether Discord takes `token` as a bot's: it answers who the bot is, or 401.
async fn check_token(token: &str) -> Result<(), SettingsError> {
    match Http::new(token).get_current_user().await {
        Ok(_) => Ok(()),
        Err(serenity::Error::Http(HttpError::UnsuccessfulRequest(response)))
            if response.status_code.as_u16() == 401 =>
        {
            Err(Error::External(SettingsError::TokenRefused))
        }
        Err(error) => Err(Error::External(SettingsError::Unavailable(
            error.to_string(),
        ))),
    }
}

/// `GET /api/admin/discord/check`: superusers only. How the bot and the server stand, and where
/// the server's roles differ from the site's ranks (see `check`), read from Discord now.
async fn role_check(state: State, _admin: Admin) -> Result<Json<Overview>> {
    Ok(Json(check::overview(&state).await?))
}

/// `POST /api/admin/discord/fix/categories`: superusers only. Puts the bot's categories in order
/// (see `order`); nothing else moves.
async fn fix_categories(
    state: State,
    _: SameOrigin,
    _admin: Admin,
) -> Result<StatusCode, FixError> {
    fix::fix_categories(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/admin/discord/fix/roles`: superusers only. Puts the bot's roles in order (see
/// `order`); nothing else moves.
async fn fix_roles(state: State, _: SameOrigin, _admin: Admin) -> Result<StatusCode, FixError> {
    fix::fix_roles(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/admin/discord/fix/create`: superusers only. Makes the bot's roles, categories, and
/// announcements channels the server lacks (see `fix`).
async fn create_channels(
    state: State,
    _: SameOrigin,
    _admin: Admin,
) -> Result<StatusCode, FixError> {
    fix::create_channels(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/admin/discord/fix/announcements`: superusers only. Moves the announcements channels
/// to the top of the list under their own names (see `fix`).
async fn fix_announcements(
    state: State,
    _: SameOrigin,
    _admin: Admin,
) -> Result<StatusCode, FixError> {
    fix::fix_announcements(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/admin/discord/fix/permissions`: superusers only. Sets the bot's permission entries on
/// its categories and announcements channels where they differ (see `fix`).
async fn fix_permissions(
    state: State,
    _: SameOrigin,
    _admin: Admin,
) -> Result<StatusCode, FixError> {
    fix::fix_permissions(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/admin/discord/fix/sync/{channel}`: superusers only. Syncs a channel in one of the
/// bot's categories with it again (see `fix`).
async fn sync_channel(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Path(channel): Path<String>,
) -> Result<StatusCode, FixError> {
    let channel = channel
        .parse()
        .map_err(|_| Error::External(FixError::NotInOurCategory))?;
    fix::sync_channel(&state, channel).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Which role to give to whom: the bot's role for `to` (its key, such as `role:member`) to
/// everyone holding the server's role `from` (a Discord id, as a string).
#[derive(Debug, serde::Deserialize)]
pub struct GiveInput {
    from: String,
    to: String,
}

#[derive(Debug, serde::Serialize)]
pub struct Given {
    /// How many people it was given to.
    pub given: usize,
}

/// `POST /api/admin/discord/fix/give`: superusers only. Gives one of the bot's roles to everyone
/// holding another of the server's roles (see `fix`); takes none away.
async fn give_role(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Json(input): Json<GiveInput>,
) -> Result<Json<Given>, FixError> {
    let from = input
        .from
        .parse()
        .map_err(|_| Error::External(FixError::NoSuchRole))?;
    let to = Access::ALL
        .into_iter()
        .find(|access| access.key() == input.to)
        .ok_or(Error::External(FixError::RoleMissing))?;
    let given = fix::give_role(&state, from, to).await?;
    Ok(Json(Given { given }))
}

/// `POST /api/admin/discord/fix/administrators`: superusers only. Gives the leader's and officers'
/// roles Administrator where they lack it (see `fix`).
async fn fix_administrators(
    state: State,
    _: SameOrigin,
    _admin: Admin,
) -> Result<StatusCode, FixError> {
    fix::fix_administrators(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/admin/discord/fix/colors`: superusers only. Gives the bot's roles their levels'
/// colors where they differ (see `fix`).
async fn fix_colors(state: State, _: SameOrigin, _admin: Admin) -> Result<StatusCode, FixError> {
    fix::fix_colors(&state).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/admin/discord/fix/rename/{key}`: superusers only. Renames the bot's role for that
/// level (`role:officer`) to the bot's own name for it (see `fix`).
async fn rename_role(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Path(key): Path<String>,
) -> Result<StatusCode, FixError> {
    let access = Access::ALL
        .into_iter()
        .find(|access| access.key() == key)
        .ok_or(Error::External(FixError::RoleMissing))?;
    fix::rename_role(&state, access).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The server's role to use, a Discord id as a string (it does not fit a JavaScript number).
#[derive(Debug, serde::Deserialize)]
pub struct RoleInput {
    id: String,
}

/// `PUT /api/admin/discord/roles/{key}`: superusers only. Makes another of the server's roles the
/// bot's role for that level (`role:officer`), renaming nothing (see `fix`).
async fn set_role(
    state: State,
    _: SameOrigin,
    _admin: Admin,
    Path(key): Path<String>,
    Json(input): Json<RoleInput>,
) -> Result<StatusCode, FixError> {
    let access = Access::ALL
        .into_iter()
        .find(|access| access.key() == key)
        .ok_or(Error::External(FixError::RoleMissing))?;
    let id = input
        .id
        .trim()
        .parse()
        .map_err(|_| Error::External(FixError::NotAUsableRole))?;
    fix::set_role(&state, access, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<State> {
    Router::new().route("/announcements", get(latest))
}

/// The superusers' routes, under `/api/admin`.
pub fn admin_router() -> Router<State> {
    Router::new()
        .route("/discord", get(settings).put(set_settings))
        .route("/discord/check", get(role_check))
        .route("/discord/fix/categories", post(fix_categories))
        .route("/discord/fix/roles", post(fix_roles))
        .route("/discord/fix/create", post(create_channels))
        .route("/discord/fix/announcements", post(fix_announcements))
        .route("/discord/fix/sync/{channel}", post(sync_channel))
        .route("/discord/fix/give", post(give_role))
        .route("/discord/roles/{key}", axum::routing::put(set_role))
        .route("/discord/fix/permissions", post(fix_permissions))
        .route("/discord/fix/administrators", post(fix_administrators))
        .route("/discord/fix/rename/{key}", post(rename_role))
        .route("/discord/fix/colors", post(fix_colors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_id_is_a_positive_number() {
        assert_eq!(
            parse_guild_id(" 123456789012345678 "),
            Some(123456789012345678)
        );
        assert_eq!(parse_guild_id("0"), None);
        assert_eq!(parse_guild_id("-5"), None);
        assert_eq!(parse_guild_id("guild"), None);
    }

    #[test]
    fn a_change_tells_absent_from_null() {
        let input: SettingsInput = serde_json::from_str(r#"{"bot_token": null}"#).unwrap();
        assert_eq!(input.guild_id, None);
        assert_eq!(input.bot_token, Some(None));
    }
}
