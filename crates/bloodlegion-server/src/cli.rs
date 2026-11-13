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
    // The data the binary embeds follows the schema, so the database matches this build.
    match crate::game_items::load(&pool)
        .await
        .context("loading the item snapshot")?
    {
        crate::game_items::Loaded::Unchanged => tracing::debug!("game items: snapshot unchanged"),
        loaded => tracing::info!(?loaded, "game items: loaded the snapshot"),
    }
    let raids = crate::raids::catalog::sync(&pool)
        .await
        .context("syncing the raid zones and bosses")?;
    tracing::info!(?raids, "raids: synced data/raids.json");
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

/// `users battlenet-unread [--sign-out]`: lists the members with Battle.net connected that no sign-in
/// read characters from, and signs out their sessions.
#[tokio::main(flavor = "current_thread")]
pub async fn users_battlenet_unread(sign_out: bool) -> Result<()> {
    let pool = State::load_pool().await?;
    let unread = crate::wow::battlenet::unread(&pool)
        .await
        .context("listing the unread Battle.net members")?;
    if unread.is_empty() {
        eprintln!("Every member with Battle.net connected has had their characters read.");
        return Ok(());
    }
    println!("{:<32} {:<32} SESSIONS", "USERNAME", "BATTLE.NET");
    for user in &unread {
        println!(
            "{:<32} {:<32} {}",
            user.username,
            user.account.as_deref().unwrap_or("-"),
            user.sessions
        );
    }
    let sessions: i64 = unread.iter().map(|u| u.sessions).sum();
    if sign_out {
        let signed_out = crate::wow::battlenet::sign_out_unread(&pool)
            .await
            .context("signing out the unread members")?;
        eprintln!(
            "Signed out {signed_out} sessions of {} members: their next visit signs in again.",
            unread.len()
        );
    } else {
        eprintln!(
            "{} members, {sessions} sessions signed in. Run again with --sign-out to sign them out.",
            unread.len()
        );
    }
    Ok(())
}

/// `users rank`: sets a guild rank, without the officers' rules (the operator bootstraps the
/// first leader this way).
#[tokio::main(flavor = "current_thread")]
pub async fn users_set_rank(username: String, rank: String) -> Result<()> {
    let Some(rank) = crate::guild::Rank::parse(&rank) else {
        return Err(anyhow::anyhow!(
            "'{rank}' is not a rank: leader, officer, raider, trial, member, or public"
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
    raw: bool,
) -> Result<()> {
    let namespace = namespace.unwrap_or_else(|| crate::wow::profile_namespace(&region));
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
        let profile = match crate::wow::account_profile(
            &state.http_client,
            &region,
            &namespace,
            &locale,
            &tokens.access_token,
        )
        .await
        {
            Ok(Some(profile)) => profile,
            Ok(None) => {
                eprintln!("{account}: no characters in {namespace}");
                continue;
            }
            Err(error) => {
                eprintln!("{account}: {error:#}");
                continue;
            }
        };
        if raw {
            println!("{profile:#}");
            continue;
        }
        let characters = crate::wow::parse_characters(&profile);
        // Forever has no realms, so the column shows only for games that do.
        let realms = characters.iter().any(|c| !c.realm.is_empty());
        let realm = |name: &str| {
            if realms {
                format!("{name:<22} ")
            } else {
                String::new()
            }
        };
        println!(
            "{:<26} {}{:>5} {:<14} {:<20} FACTION",
            "NAME",
            realm("REALM"),
            "LEVEL",
            "CLASS",
            "RACE"
        );
        for character in &characters {
            println!(
                "{:<26} {}{:>5} {:<14} {:<20} {}",
                character.full_name(),
                realm(&character.realm),
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

/// `oauth2-clients add`: registers an app the way the admin API does, and prints its client id
/// (and a confidential client's generated secret, which is shown only now).
#[tokio::main(flavor = "current_thread")]
pub async fn clients_add(
    name: String,
    redirect_uris: Vec<String>,
    public: bool,
    auto_consent_scope: Option<String>,
) -> Result<()> {
    let pool = State::load_pool().await?;
    let payload = crate::auth::oauth2::clients::CreatePayload {
        id: None,
        name,
        redirect_uris,
        client_type: if public { "public" } else { "confidential" }.to_string(),
        is_pkce_required: true,
        is_email_masked: false,
        secret: None,
        auto_consent_scope,
        identity_provider_slug: None,
    };
    let client = match crate::auth::oauth2::clients::create(&pool, payload).await {
        Ok(client) => client,
        Err(crate::Error::External(problem)) => {
            return Err(anyhow::anyhow!("{}", crate::Problem::detail(&problem)).into());
        }
        Err(crate::Error::Internal(error)) => return Err(error.into()),
    };
    eprintln!("registered the client; its id:");
    println!("{}", client.id);
    if let Some(secret) = client.secret {
        eprintln!("its secret, shown only now:");
        println!("{secret}");
    }
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
    allow_multiple_connections: Option<bool>,
    connection_only: Option<bool>,
) -> Result<()> {
    let pool = State::load_pool().await?;
    let slug = crate::Slug::try_from(slug.as_str()).context("invalid slug")?;
    let updated = crate::auth::oauth2::providers::update_flags(
        &pool,
        &slug,
        allow_registration,
        allow_disconnection,
        allow_multiple_connections,
        connection_only,
    )
    .await
    .context("updating the provider")?;
    if !updated {
        return Err(anyhow::anyhow!("no OAuth2 provider has the slug '{slug}'").into());
    }
    eprintln!("updated provider '{slug}'");
    Ok(())
}

/// `characters seed`: makes the test characters, then raids and loot for them.
#[tokio::main(flavor = "current_thread")]
pub async fn characters_seed() -> Result<()> {
    let pool = State::load_pool().await?;
    let created = crate::characters::seed(&pool)
        .await
        .context("making the test characters")?;
    eprintln!("made {created} test characters (four of each class; names already taken skipped)");
    let characters = crate::characters::seeded_ids(&pool)
        .await
        .context("finding the test characters")?;
    let report = crate::raids::seed::seed(&pool, &characters)
        .await
        .context("making the test raids")?;
    eprintln!(
        "made {} test raids with {} loot (zones that had test raids skipped)",
        report.raids, report.loot
    );
    Ok(())
}

/// `characters armory`: what the armory says of one character.
#[tokio::main(flavor = "current_thread")]
pub async fn characters_armory(name: String, raw: bool) -> Result<()> {
    let state = State::new().await?;
    let token = crate::wow::armory::app_token(&state).await?;
    let Some(profile) = crate::wow::armory::profile(&state.http_client, &token, &name).await?
    else {
        return Err(anyhow::anyhow!("the armory has no character '{name}'").into());
    };
    if raw {
        println!("{profile:#}");
    } else {
        println!("{:#?}", crate::wow::armory::parse(&profile));
    }
    Ok(())
}

/// `characters armory-check`: every character against the armory.
#[tokio::main(flavor = "current_thread")]
pub async fn characters_armory_check(fill: bool) -> Result<()> {
    let state = State::new().await?;
    let checked = crate::wow::armory::check(&state, fill).await?;
    for character in &checked {
        let name = format!("{} {}", character.first_name, character.last_name);
        if let Some(error) = &character.error {
            println!("{name}: lookup failed: {error}");
        } else if !character.found && character.level < crate::wow::armory::MIN_LEVEL {
            println!(
                "{name}: not in the armory, which shows no character below level {}",
                crate::wow::armory::MIN_LEVEL
            );
        } else if !character.found {
            println!("{name}: not in the armory");
        } else {
            for difference in &character.differences {
                println!(
                    "{name}: {} is {} here, {} in the armory",
                    difference.field, difference.site, difference.armory
                );
            }
            if !character.filled.is_empty() {
                println!("{name}: filled in {}", character.filled.join(", "));
            }
        }
    }
    let found = checked.iter().filter(|c| c.found).count();
    eprintln!("{} characters, {found} in the armory", checked.len());
    Ok(())
}

/// `characters unseed`: removes the test raids, then the test characters that have no history.
#[tokio::main(flavor = "current_thread")]
pub async fn characters_unseed() -> Result<()> {
    let pool = State::load_pool().await?;
    let characters = crate::characters::seeded_ids(&pool)
        .await
        .context("finding the test characters")?;
    let report = crate::raids::seed::unseed(&pool, &characters)
        .await
        .context("removing the test raids")?;
    eprintln!(
        "removed {} test raids and {} test items",
        report.raids, report.items
    );
    let (removed, kept) = crate::characters::unseed(&pool)
        .await
        .context("removing the test characters")?;
    eprintln!("removed {removed} test characters; kept {kept} that raided or won loot");
    Ok(())
}
