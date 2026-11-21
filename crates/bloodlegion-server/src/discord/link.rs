//! A superuser linking a Discord account to a member by hand, for someone in the server who has
//! not linked it on the site themselves, and taking one off again.
//!
//! The link is the one the member's own sign-in at Discord makes: a credential at the Discord
//! provider whose subject is the Discord user id, holding what Discord says of that user. The bot
//! asks Discord for it (`GET /users/{id}`), which answers as it does a sign-in, less what only the
//! person themselves is told (their email). The member sees it on their profile like any other, and
//! can unlink it there.

use anyhow::Context;
use serenity::all::{Http, HttpError, UserId as DiscordUserId};
use serenity::http::{LightMethod, Request, Route};

use super::{DISCORD_PROVIDER_SLUG, db};
use crate::auth::oauth2::providers::{ProviderError, links};
use crate::users::UserId;
use crate::{Error, Problem, Result, Slug, State};

#[derive(Debug, thiserror::Error, Problem)]
pub enum LinkError {
    #[error("the bot is not set up")]
    #[problem(
        status = CONFLICT,
        title = "Bot Not Set Up",
        detail = "Set the bot's token and the server's id first."
    )]
    NotSetUp,
    #[error("discord is not a provider")]
    #[problem(
        status = CONFLICT,
        title = "Discord Not Set Up",
        detail = "The site has no Discord provider to link accounts at. Add it first."
    )]
    NoProvider,
    #[error("not a discord user id")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Not a Discord ID",
        detail = "A Discord user's id is a number, such as 123456789012345678."
    )]
    InvalidId,
    #[error("no such discord user")]
    #[problem(
        status = NOT_FOUND,
        title = "No Such Discord User",
        detail = "Discord knows no user with that id."
    )]
    NoSuchDiscordUser,
    #[error("a bot")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "A Bot",
        detail = "That Discord account is a bot's, which is no one's to link."
    )]
    Bot,
    #[error("no such member")]
    #[problem(
        status = NOT_FOUND,
        title = "No Such Member",
        detail = "No one on the site has that id."
    )]
    NoSuchMember,
    #[error("linked to {0} already")]
    #[problem(
        status = CONFLICT,
        title = "Already Linked",
        detail = format!("That Discord account is {_0}'s already, linked now or unlinked by them. \
                          Unlink it from them first.")
    )]
    AlreadyLinked(String),
    #[error("the member has a discord account")]
    #[problem(
        status = CONFLICT,
        title = "Already Has One",
        detail = "That member has a Discord account linked already, and only one can be. Unlink it \
                  first."
    )]
    MemberHasOne,
    #[error("not linked")]
    #[problem(
        status = NOT_FOUND,
        title = "Not Linked",
        detail = "That Discord account is not linked to anyone."
    )]
    NotLinked,
    #[error("linked to {0}")]
    #[problem(
        status = CONFLICT,
        title = "Has an Account",
        detail = format!("That Discord account is {_0}'s on the site, linked now or unlinked by \
                          them, so it needs no placeholder: set their rank on the users page.")
    )]
    HasAccount(String),
    #[error("not a placeholder rank")]
    #[problem(
        status = UNPROCESSABLE_ENTITY,
        title = "Not a Placeholder Rank",
        detail = "A placeholder is a raider, trial, member, or friend. The leader and officers \
                  need an account, and the public need no placeholder."
    )]
    NotAPlaceholderRank,
    #[error("no placeholder")]
    #[problem(
        status = NOT_FOUND,
        title = "No Placeholder",
        detail = "That Discord account has no placeholder rank."
    )]
    NoPlaceholder,
    #[error("discord: {0}")]
    #[problem(
        status = BAD_GATEWAY,
        title = "Discord Unavailable",
        detail = "Discord could not be asked about that user. Please try again."
    )]
    Discord(String),
}

/// A Discord user id as typed or sent: digits, not zero.
pub(super) fn parse_id(input: &str) -> Option<u64> {
    input.trim().parse().ok().filter(|&id| id != 0)
}

fn slug() -> Slug {
    Slug::try_from(DISCORD_PROVIDER_SLUG).expect("the Discord provider's slug is a valid one")
}

/// What Discord says of the user `id`, untouched: what a sign-in's userinfo holds, less the email.
pub(super) async fn discord_user(state: &State, id: u64) -> Result<serde_json::Value, LinkError> {
    let settings = db::settings(&state.pool)
        .await
        .context("reading the bot's settings")?;
    let Some(token) = settings.bot_token else {
        return Err(Error::External(LinkError::NotSetUp));
    };
    let token = state
        .decrypt(&token)
        .map_err(|error| anyhow::anyhow!("{error}"))
        .context("decrypting the bot's token")?;
    let request = Request::new(
        Route::User {
            user_id: DiscordUserId::new(id),
        },
        LightMethod::Get,
    );
    match Http::new(&token).fire::<serde_json::Value>(request).await {
        Ok(user) => Ok(user),
        Err(serenity::Error::Http(HttpError::UnsuccessfulRequest(response)))
            if response.status_code.as_u16() == 404 =>
        {
            Err(Error::External(LinkError::NoSuchDiscordUser))
        }
        Err(error) => Err(Error::External(LinkError::Discord(error.to_string()))),
    }
}

/// Links the Discord account `discord_id` to the member `user_id`. Refused when the account is
/// someone else's (linked, or unlinked by them), or the member has one linked already. Linking one
/// that is theirs already changes nothing.
pub async fn link(state: &State, discord_id: &str, user_id: UserId) -> Result<(), LinkError> {
    let id = parse_id(discord_id).ok_or(Error::External(LinkError::InvalidId))?;
    if !db::member_exists(&state.pool, user_id)
        .await
        .context("looking up the member")?
    {
        return Err(Error::External(LinkError::NoSuchMember));
    }
    let user = discord_user(state, id).await?;
    if user.get("bot").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(Error::External(LinkError::Bot));
    }
    // The subject is Discord's own spelling of the id, as a sign-in stores it.
    let subject = id.to_string();
    if user.get("id").and_then(serde_json::Value::as_str) != Some(subject.as_str()) {
        return Err(Error::External(LinkError::Discord(
            "the user Discord answered with is not the one asked for".into(),
        )));
    }
    match links::link_by_hand(&state.pool, &slug(), user_id, &subject, &user).await {
        Ok(_) => {
            tracing::info!(discord_id = id, %user_id, "discord: linked an account by hand");
            Ok(())
        }
        Err(Error::External(ProviderError::AlreadyLinked(_))) => {
            let owner = db::account_owner(&state.pool, id)
                .await
                .context("looking up whose the Discord account is")?
                .unwrap_or_else(|| "someone else".into());
            Err(Error::External(LinkError::AlreadyLinked(owner)))
        }
        Err(Error::External(ProviderError::MultipleConnectionsNotAllowed(_))) => {
            Err(Error::External(LinkError::MemberHasOne))
        }
        Err(Error::External(ProviderError::UnknownProvider)) => {
            Err(Error::External(LinkError::NoProvider))
        }
        Err(Error::External(other)) => Err(Error::Internal(
            anyhow::Error::new(other).context("linking the Discord account"),
        )),
        Err(Error::Internal(error)) => Err(Error::Internal(error)),
    }
}

/// Takes the Discord account `discord_id` off the member it is linked to (or who unlinked it),
/// leaving it free for anyone to link.
pub async fn unlink(state: &State, discord_id: &str) -> Result<(), LinkError> {
    let id = parse_id(discord_id).ok_or(Error::External(LinkError::InvalidId))?;
    if !links::unlink_by_hand(&state.pool, &slug(), &id.to_string()).await? {
        return Err(Error::External(LinkError::NotLinked));
    }
    tracing::info!(discord_id = id, "discord: unlinked an account by hand");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_discord_id_is_a_positive_number() {
        assert_eq!(parse_id(" 123456789012345678 "), Some(123456789012345678));
        assert_eq!(parse_id("0"), None);
        assert_eq!(parse_id("someone"), None);
    }
}
