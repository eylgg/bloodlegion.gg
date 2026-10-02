//! The admin commands: everything an operator does from a shell rather than the admin API. Each
//! one connects to the database the server uses, so they run wherever the server's environment
//! (`DATABASE_URL`, and for provider setup `ORIGIN` and `ENCRYPTION_KEY`) is available, the
//! container included.

use anyhow::Context;

use crate::auth::oauth2::providers::{ClaimInput, CreateProviderPayload, Oauth2Target};
use crate::users::UserId;
use crate::{Result, State};

#[tokio::main(flavor = "current_thread")]
pub async fn create_superuser(username: String, email: Option<String>) -> Result<()> {
    let pool = State::load_pool().await?;

    let password = crate::crypto::generate_token::<24>();
    let payload = crate::users::CreateUserPayload {
        username: username.clone(),
        email: email.clone(),
        first_name: None,
        last_name: None,
        is_superuser: true,
    };

    let mut tx = pool.begin().await.context("starting transaction")?;
    let user = crate::users::create_user(&mut tx, &payload, true)
        .await
        .context("creating superuser")?;
    crate::auth::local::set_password(&mut tx, user.id, None, &password)
        .await
        .context("setting superuser password")?;
    tx.commit().await.context("committing superuser")?;

    match email {
        Some(email) => eprintln!("Created superuser '{username}' <{email}>"),
        None => eprintln!("Created superuser '{username}'"),
    }
    println!("{password}");
    Ok(())
}

#[tokio::main]
pub async fn migrate() -> Result<()> {
    let pool = State::load_pool().await?;
    let migrator = sqlx::migrate!();
    migrator.run(&pool).await.context("running migrations")?;
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
pub async fn rotate_jwk() -> Result<()> {
    let pool = State::load_pool().await?;
    let encryption_key = State::load_encryption_key()?;
    let kid = crate::auth::jwk::rotate(&pool, &encryption_key)
        .await
        .context("rotating the signing key")?;
    // Tell running instances to load the new active key without a restart.
    crate::auth::jwk::notify_rotation(&pool, &kid).await?;
    eprintln!("Rotated signing key; new active kid: {kid}");
    Ok(())
}

#[tokio::main]
pub async fn migrate_undo(target: i64) -> Result<()> {
    let pool = State::load_pool().await?;
    let migrator = sqlx::migrate!();
    migrator
        .undo(&pool, target)
        .await
        .with_context(|| format!("undoing migrations to target {target}"))?;
    Ok(())
}

/// Resolves a username to its account id, or fails with a message naming it.
async fn user_id(pool: &sqlx::PgPool, username: &str) -> Result<UserId> {
    Ok(crate::users::find_user_id_by_username(pool, username)
        .await
        .context("looking up the user")?
        .with_context(|| format!("no user named '{username}'"))?)
}

/// `users list`: one line per account, as a fixed-width table.
#[tokio::main(flavor = "current_thread")]
pub async fn users_list() -> Result<()> {
    let pool = State::load_pool().await?;
    let users = crate::users::list_user_listings(&pool)
        .await
        .context("listing users")?;
    println!(
        "{:<32} {:<9} {:<8} {:<40} CREATED",
        "USERNAME", "ROLE", "STATE", "EMAIL"
    );
    for user in users {
        let email = user
            .emails
            .iter()
            .find(|email| email.is_primary)
            .or(user.emails.first())
            .map(|email| email.email.as_str())
            .unwrap_or("-");
        println!(
            "{:<32} {:<9} {:<8} {:<40} {}",
            user.username,
            if user.is_superuser {
                "superuser"
            } else {
                "user"
            },
            if user.disabled { "disabled" } else { "active" },
            email,
            user.created_at.date(),
        );
    }
    Ok(())
}

/// `users promote` / `users demote`: sets the superuser flag.
#[tokio::main(flavor = "current_thread")]
pub async fn users_set_superuser(username: String, is_superuser: bool) -> Result<()> {
    let pool = State::load_pool().await?;
    let id = user_id(&pool, &username).await?;
    crate::users::set_user_superuser(&pool, id, is_superuser)
        .await
        .context("updating the superuser flag")?;
    if is_superuser {
        eprintln!("'{username}' is now a superuser");
    } else {
        eprintln!("'{username}' is no longer a superuser");
    }
    Ok(())
}

/// `users rank`: sets a guild rank, without the officers' rules (the operator bootstraps the
/// first leader this way).
#[tokio::main(flavor = "current_thread")]
pub async fn users_set_rank(username: String, rank: String) -> Result<()> {
    let Some(rank) = crate::guild::Rank::parse(&rank) else {
        return Err(anyhow::anyhow!(
            "'{rank}' is not a rank: leader, officer, raider, trial, member, friend, or retired"
        )
        .into());
    };
    let pool = State::load_pool().await?;
    let id = user_id(&pool, &username).await?;
    crate::guild::force_rank(&pool, id, rank)
        .await
        .context("updating the guild rank")?;
    eprintln!("'{username}' is now {}", rank.slug());
    Ok(())
}

/// `users rename`.
#[tokio::main(flavor = "current_thread")]
pub async fn users_rename(username: String, new_username: String) -> Result<()> {
    let pool = State::load_pool().await?;
    let id = user_id(&pool, &username).await?;
    match crate::users::rename_user(&pool, id, &new_username).await {
        Ok(()) => {}
        Err(crate::Error::External(problem)) => {
            return Err(anyhow::anyhow!("{}", crate::Problem::detail(&problem)).into());
        }
        Err(crate::Error::Internal(error)) => return Err(error.into()),
    }
    eprintln!("renamed '{username}' to '{new_username}'");
    Ok(())
}

/// `users disable` / `users enable`: suspends or restores an account.
#[tokio::main(flavor = "current_thread")]
pub async fn users_set_disabled(username: String, disabled: bool) -> Result<()> {
    let pool = State::load_pool().await?;
    let id = user_id(&pool, &username).await?;
    crate::users::set_user_disabled(&pool, id, disabled)
        .await
        .context("updating the disabled state")?;
    if disabled {
        eprintln!("'{username}' is disabled");
    } else {
        eprintln!("'{username}' is enabled");
    }
    Ok(())
}

/// `users characters`: for each Battle.net account the user linked, the token status, then the
/// characters on it.
#[tokio::main(flavor = "current_thread")]
pub async fn users_characters(
    username: String,
    provider: String,
    region: String,
    namespace: Option<String>,
    locale: String,
) -> Result<()> {
    let namespace = namespace.unwrap_or_else(|| format!("profile-{region}"));
    let state = State::new().await?;
    let id = user_id(&state.pool, &username).await?;
    let slug = crate::Slug::try_from(provider.as_str()).context("invalid provider slug")?;
    let accounts = crate::auth::oauth2::providers::tokens_for_user(&state, id, &slug).await?;
    if accounts.is_empty() {
        return Err(anyhow::anyhow!(
            "'{username}' has no stored {slug} token; they need to sign in through {slug} again"
        )
        .into());
    }
    // One listing per linked account. A failing account (an expired token) is reported and
    // skipped, so the others still list.
    let now = time::OffsetDateTime::now_utc();
    for tokens in &accounts {
        let account = tokens.identity.as_deref().unwrap_or("(unnamed account)");
        let expiry = match tokens.expires_at {
            Some(at) if at <= now => format!("expired {at}"),
            Some(at) => format!("expires {at}"),
            None => "no expiry given".to_string(),
        };
        eprintln!(
            "{account}: {slug} token from the sign-in at {}: {expiry}; refresh token: {}; \
             scope: {}",
            tokens.updated_at,
            if tokens.has_refresh_token {
                "yes"
            } else {
                "no"
            },
            tokens.scope,
        );
        let characters = match crate::wow::account_characters(
            &state.http_client,
            &region,
            &namespace,
            &locale,
            &tokens.access_token,
        )
        .await
        {
            Ok(characters) => characters,
            Err(error) => {
                eprintln!("{account}: {error:#}");
                continue;
            }
        };
        println!(
            "{:<14} {:<22} {:>5} {:<14} {:<20} FACTION",
            "NAME", "REALM", "LEVEL", "CLASS", "RACE"
        );
        for character in &characters {
            println!(
                "{:<14} {:<22} {:>5} {:<14} {:<20} {}",
                character.name,
                character.realm,
                character.level,
                character.class,
                character.race,
                character.faction,
            );
        }
        eprintln!("{account}: {} characters", characters.len());
    }
    Ok(())
}

/// `launch`: every member's launch sign-ups, mains first, with class and specs by name.
#[tokio::main(flavor = "current_thread")]
pub async fn launch() -> Result<()> {
    let pool = State::load_pool().await?;
    let entries = crate::launch::list_all(&pool)
        .await
        .context("listing the launch sign-ups")?;
    println!("{:<24} {:<5} {:<9} SPECS", "MEMBER", "", "CLASS");
    for entry in &entries {
        let class = crate::launch::catalog::find(&entry.class);
        let specs: Vec<&str> = entry
            .specs
            .iter()
            .map(|slug| {
                class
                    .and_then(|c| c.specs.iter().find(|s| s.slug == slug))
                    .map_or(slug.as_str(), |s| s.name)
            })
            .collect();
        println!(
            "{:<24} {:<5} {:<9} {}",
            entry.username,
            if entry.is_main { "main" } else { "alt" },
            class.map_or(entry.class.as_str(), |c| c.name),
            specs.join(", "),
        );
    }
    eprintln!("{} sign-ups", entries.len());
    Ok(())
}

/// `passwords status`: prints `enabled` or `disabled`.
#[tokio::main(flavor = "current_thread")]
pub async fn passwords_status() -> Result<()> {
    let pool = State::load_pool().await?;
    let enabled = crate::auth::local::is_enabled(&pool)
        .await
        .context("reading the local login state")?;
    println!("{}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

/// `passwords enable` / `passwords disable`. Disabling is refused while no other login source is
/// enabled, the same guard the admin API applies, so an operator cannot lock everyone out.
#[tokio::main(flavor = "current_thread")]
pub async fn passwords_set_enabled(enabled: bool) -> Result<()> {
    let pool = State::load_pool().await?;
    let now_enabled = match crate::auth::local::set_enabled(&pool, enabled).await {
        Ok(now_enabled) => now_enabled,
        Err(crate::Error::External(problem)) => {
            return Err(anyhow::anyhow!("{}", crate::Problem::detail(&problem)).into());
        }
        Err(crate::Error::Internal(error)) => return Err(error.into()),
    };
    eprintln!(
        "local password login is {}",
        if now_enabled { "enabled" } else { "disabled" }
    );
    Ok(())
}

/// `oauth2-providers list`.
#[tokio::main(flavor = "current_thread")]
pub async fn providers_list() -> Result<()> {
    let pool = State::load_pool().await?;
    let providers = crate::auth::oauth2::providers::list(&pool)
        .await
        .context("listing providers")?;
    println!(
        "{:<16} {:<24} {:<6} {:<12} {:<14} ISSUER",
        "SLUG", "NAME", "OIDC", "REGISTRATION", "AUTO-CONNECT"
    );
    for provider in providers {
        println!(
            "{:<16} {:<24} {:<6} {:<12} {:<14} {}",
            provider.slug.as_ref(),
            provider.name,
            if provider.is_oidc { "yes" } else { "no" },
            if provider.is_registration_allowed {
                "allowed"
            } else {
                "closed"
            },
            if provider.is_auto_connection_allowed {
                "allowed"
            } else {
                "no"
            },
            provider.issuer.as_deref().unwrap_or("-"),
        );
    }
    Ok(())
}

/// The `oauth2-providers add` arguments, as the CLI parses them.
pub struct CustomProvider {
    pub slug: String,
    pub name: String,
    pub issuer: Option<String>,
    pub client_id: String,
    pub client_secret: String,
    pub scope: Option<String>,
    pub claims: Vec<String>,
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: Option<String>,
    pub userinfo_endpoint: Option<String>,
    pub allow_registration: bool,
    pub allow_auto_connection: bool,
    pub trust_email_verified: bool,
    pub allow_disconnection: bool,
    pub allow_unclaimed_username_connection: bool,
}

/// Parses one `--claim claim=target[:normalize]` argument.
fn parse_claim(spec: &str) -> Result<ClaimInput> {
    let (claim, target) = spec
        .split_once('=')
        .with_context(|| format!("claim mapping '{spec}' is not of the form claim=target"))?;
    let (target, normalize) = match target.strip_suffix(":normalize") {
        Some(target) => (target, true),
        None => (target, false),
    };
    let target: Oauth2Target = serde_json::from_value(serde_json::Value::String(
        target.to_string(),
    ))
    .with_context(|| {
        format!(
            "unknown claim target '{target}' (expected username, email, first_name, \
                 last_name, is_email_verified, or subject)"
        )
    })?;
    Ok(ClaimInput {
        claim: claim.trim().to_string(),
        target: Some(target),
        normalize,
    })
}

pub fn custom_provider(args: CustomProvider) -> Result<CreateProviderPayload> {
    let claims = args
        .claims
        .iter()
        .map(|spec| parse_claim(spec))
        .collect::<Result<Vec<_>>>()?;
    Ok(CreateProviderPayload {
        slug: args.slug,
        name: args.name,
        is_oidc: args.issuer.is_some(),
        issuer: args.issuer,
        client_id: args.client_id,
        client_secret: args.client_secret,
        is_registration_allowed: args.allow_registration,
        is_auto_connection_allowed: args.allow_auto_connection,
        is_email_verified: args.trust_email_verified,
        is_disconnection_allowed: args.allow_disconnection,
        is_unclaimed_username_connection_allowed: args.allow_unclaimed_username_connection,
        authorization_endpoint: args.authorization_endpoint,
        token_endpoint: args.token_endpoint,
        userinfo_endpoint: args.userinfo_endpoint,
        claims,
        scope: args.scope,
    })
}

/// `oauth2-providers add`: registers the provider the same way the admin API does (validation
/// included), so a mistake is reported rather than stored.
#[tokio::main(flavor = "current_thread")]
pub async fn providers_add(payload: CreateProviderPayload) -> Result<()> {
    let state = State::new().await?;
    let slug = payload.slug.clone();
    match crate::auth::oauth2::providers::create(&state, payload).await {
        Ok(()) => {}
        Err(crate::Error::External(problem)) => {
            return Err(anyhow::anyhow!("{}", crate::Problem::detail(&problem)).into());
        }
        Err(crate::Error::Internal(error)) => return Err(error.into()),
    }
    eprintln!(
        "registered provider '{slug}'; its login URL is {}/api/auth/oauth2/providers/{slug} and \
         the redirect URL to register upstream is {}/auth/oauth2/callback",
        state.origin, state.origin
    );
    Ok(())
}

/// `oauth2-providers remove`.
#[tokio::main(flavor = "current_thread")]
pub async fn providers_remove(slug: String) -> Result<()> {
    let pool = State::load_pool().await?;
    let slug = crate::Slug::try_from(slug.as_str()).context("invalid slug")?;
    let removed = crate::auth::oauth2::providers::remove(&pool, &slug)
        .await
        .context("removing the provider")?;
    if !removed {
        return Err(anyhow::anyhow!("no OAuth2 provider has the slug '{slug}'").into());
    }
    eprintln!("removed provider '{slug}'");
    Ok(())
}

/// `oauth2-providers update`: flips a provider's registration and disconnection flags.
#[tokio::main(flavor = "current_thread")]
pub async fn providers_update(
    slug: String,
    allow_registration: Option<bool>,
    allow_disconnection: Option<bool>,
) -> Result<()> {
    let pool = State::load_pool().await?;
    let slug = crate::Slug::try_from(slug.as_str()).context("invalid slug")?;
    let updated = crate::auth::oauth2::providers::update_flags(
        &pool,
        &slug,
        allow_registration,
        allow_disconnection,
    )
    .await
    .context("updating the provider")?;
    if !updated {
        return Err(anyhow::anyhow!("no OAuth2 provider has the slug '{slug}'").into());
    }
    eprintln!("updated provider '{slug}'");
    Ok(())
}

/// `items sync`: syncs the item mirror now, reporting what it did.
#[tokio::main]
pub async fn items_sync(ids: Vec<i32>) -> Result<()> {
    let state = State::new().await?;
    let only = (!ids.is_empty()).then_some(ids.as_slice());
    match crate::game_items::sync(&state, only).await? {
        crate::game_items::Outcome::NoProvider => Err(anyhow::anyhow!(
            "no '{}' provider is configured; the item sync uses its client",
            crate::game_items::PROVIDER
        )
        .into()),
        crate::game_items::Outcome::Busy => {
            Err(anyhow::anyhow!("another sync is running; try again when it finishes").into())
        }
        crate::game_items::Outcome::Done(report) => {
            eprintln!(
                "listed {}, pruned {}, fetched {}, unchanged {}, unknown {}, icons {}, failed {}",
                report.listed,
                report.pruned,
                report.fetched,
                report.unchanged,
                report.missing,
                report.icons,
                report.failed
            );
            Ok(())
        }
    }
}
