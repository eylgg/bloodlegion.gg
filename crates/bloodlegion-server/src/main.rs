mod api;
mod auth;
mod cleaner;
mod cli;
mod crypto;
mod error;
mod extract;
mod fallbacks;
mod newtype;
mod observability;
mod problem;
mod result;
mod slug;
mod state;
mod uri;
mod users;
mod well_known;
mod wow;

use anyhow::Context;
use axum::Router;
use clap::{Parser, Subcommand};
use sqlx::postgres::PgPool;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::{
    error::Error,
    problem::{IntoProblemResponse, Problem},
    result::Result,
    slug::Slug,
    state::State,
    uri::{HttpsUrl, RedirectUri},
};

/// The server, and the admin commands that run against the same database. With no subcommand it
/// applies pending migrations and serves.
#[derive(Debug, clap::Parser)]
#[command(name = "bloodlegion-server")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Apply pending database migrations (the server also does this on start).
    Migrate,
    /// Undo migrations down to `target` (a migration version).
    MigrateUndo { target: i64 },
    /// Rotate the JWT signing key; running instances pick the new key up live.
    RotateJwk,
    /// Create a superuser with a random local password, printed to stdout. Only useful while
    /// local login is enabled; once Battle.net is the login source, promote a user instead.
    CreateSuperuser {
        username: String,
        /// The account email. Optional: an account may have none.
        #[arg(long)]
        email: Option<String>,
    },
    /// Manage accounts.
    Users {
        #[command(subcommand)]
        command: UsersCommand,
    },
    /// Enable or disable local username/password login.
    Passwords {
        #[command(subcommand)]
        command: PasswordsCommand,
    },
    /// Manage the OAuth2 / OpenID Connect login sources, such as Battle.net.
    Oauth2Providers {
        #[command(subcommand)]
        command: ProvidersCommand,
    },
}

#[derive(Debug, Subcommand)]
enum UsersCommand {
    /// List every account.
    List,
    /// Make an account a superuser (the admin role: it unlocks every admin endpoint).
    Promote { username: String },
    /// Remove an account's superuser status.
    Demote { username: String },
    /// Suspend an account: its sessions stop resolving and it can no longer log in.
    Disable { username: String },
    /// Lift an account's suspension.
    Enable { username: String },
    /// List the WoW characters on a user's Battle.net account, using the token from their latest
    /// sign-in (which needs the `wow.profile` scope). Also shows that token's status.
    Characters {
        username: String,
        /// The provider the user signed in through.
        #[arg(long, default_value = "battlenet")]
        provider: String,
        /// The Battle.net region the account plays in: us, eu, kr, or tw.
        #[arg(long, default_value = "us")]
        region: String,
        #[arg(long, default_value = "en_US")]
        locale: String,
    },
}

#[derive(Debug, Subcommand)]
enum PasswordsCommand {
    /// Whether local username/password login is enabled.
    Status,
    /// Allow logging in with a username and password.
    Enable,
    /// Refuse username/password login. Only allowed while another login source (an OAuth2
    /// provider) is enabled, so nobody gets locked out.
    Disable,
}

// `Add` carries every flag of a provider while the others carry a slug at most; the enum is built
// once from the command line, so the size spread is irrelevant.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Subcommand)]
enum ProvidersCommand {
    /// List the configured providers.
    List,
    /// Register an OAuth2 or OpenID Connect provider. For Battle.net:
    /// `--slug battlenet --name Battle.net --issuer https://oauth.battle.net/oauth
    /// --scope "openid wow.profile" --allow-registration`.
    Add {
        #[arg(long)]
        slug: String,
        #[arg(long)]
        name: String,
        /// The OIDC issuer (the discovery anchor). Omit it for a plain OAuth2 provider, which
        /// then needs the three endpoints and a `subject` claim mapping instead.
        #[arg(long)]
        issuer: Option<String>,
        #[arg(long)]
        client_id: String,
        #[arg(long)]
        client_secret: String,
        /// Space-delimited scopes to request; defaults to `openid email profile`.
        #[arg(long)]
        scope: Option<String>,
        /// A claim mapping entry, `claim=target`. Targets: username, email, first_name,
        /// last_name, is_email_verified, subject. Omit every entry to use the standard OpenID
        /// Connect mapping. A provider that asserts no usable username (Battle.net) needs no
        /// mapping: the person chooses their username on first sign-in. Append `:normalize` to a
        /// username mapping to coerce the value into the username shape instead
        /// (`battletag=username:normalize` turns `Name#1234` into `Name1234`).
        #[arg(long = "claim")]
        claims: Vec<String>,
        #[arg(long)]
        authorization_endpoint: Option<String>,
        #[arg(long)]
        token_endpoint: Option<String>,
        #[arg(long)]
        userinfo_endpoint: Option<String>,
        /// New users may register through this provider.
        #[arg(long)]
        allow_registration: bool,
        /// A login may auto-link to an existing account by verified email.
        #[arg(long)]
        allow_auto_connection: bool,
        /// Treat the emails this provider asserts as verified.
        #[arg(long)]
        trust_email_verified: bool,
        /// Users may disconnect this provider from their account.
        #[arg(long)]
        allow_disconnection: bool,
        /// A first login may claim an unclaimed account by its asserted username.
        #[arg(long)]
        allow_unclaimed_username_connection: bool,
    },
    /// Remove a provider (and every credential linked through it).
    Remove { slug: String },
}

/// Spawns a task that listens on a Postgres `NOTIFY` channel and runs `handler` for every
/// notification, backing off briefly on a transient recv error. A connect or `LISTEN` failure
/// logs and ends the task. The JWK hot-reload is built on this.
fn spawn_pg_listener<F, Fut>(pool: PgPool, channel: &'static str, mut handler: F)
where
    F: FnMut(sqlx::postgres::PgNotification) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    tokio::spawn(async move {
        let mut listener = match sqlx::postgres::PgListener::connect_with(&pool).await {
            Ok(listener) => listener,
            Err(err) => {
                tracing::error!(error = %err, channel, "pg listener: connect failed");
                return;
            }
        };
        if let Err(err) = listener.listen(channel).await {
            tracing::error!(error = %err, channel, "pg listener: LISTEN failed");
            return;
        }
        loop {
            match listener.recv().await {
                Ok(notification) => handler(notification).await,
                Err(err) => {
                    tracing::error!(error = %err, channel, "pg listener: recv failed");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });
}

fn router(state: State) -> Router {
    Router::new()
        .nest("/api", api::router())
        .nest("/auth", auth::router())
        .nest("/.well-known", well_known::router())
        .method_not_allowed_fallback(fallbacks::method_not_allowed_fallback)
        .fallback(fallbacks::fallback)
        .with_state(state)
}

#[tokio::main]
async fn serve() -> Result<()> {
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding TCP listener on {addr}"))?;
    let local_addr = listener
        .local_addr()
        .context("reading listener local address")?;
    tracing::debug!("Listening on http://{local_addr}");

    let state = State::new().await?;

    // Hot-swap the in-memory signing key when a `rotate-jwk` run signals a rotation, so it takes
    // effect live across every instance without a restart.
    let signer_state = state.clone();
    spawn_pg_listener(state.pool.clone(), "jwk_rotated", move |_| {
        let state = signer_state.clone();
        async move {
            match state.reload_active_signer().await {
                Ok(()) => tracing::info!("reloaded active signing key after rotation"),
                Err(err) => tracing::error!(error = %err, "jwk rotation listener: reload failed"),
            }
        }
    });
    cleaner::spawn(state.pool.clone());

    let router = router(state);
    let app = router.layer(ServiceBuilder::new().layer(TraceLayer::new_for_http().on_request(())));
    // `into_make_service_with_connect_info` exposes the socket peer address as a `ConnectInfo`
    // extension, the fallback source for session IP capture when there is no `X-Forwarded-For`
    // header (see `extract::ClientInfo`).
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .context("serving HTTP")?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    observability::init();

    match args.command {
        None => {
            cli::migrate()?;
            serve()
        }
        Some(Command::Migrate) => cli::migrate(),
        Some(Command::MigrateUndo { target }) => cli::migrate_undo(target),
        Some(Command::RotateJwk) => cli::rotate_jwk(),
        Some(Command::CreateSuperuser { username, email }) => {
            cli::create_superuser(username, email)
        }
        Some(Command::Users { command }) => match command {
            UsersCommand::List => cli::users_list(),
            UsersCommand::Promote { username } => cli::users_set_superuser(username, true),
            UsersCommand::Demote { username } => cli::users_set_superuser(username, false),
            UsersCommand::Disable { username } => cli::users_set_disabled(username, true),
            UsersCommand::Enable { username } => cli::users_set_disabled(username, false),
            UsersCommand::Characters {
                username,
                provider,
                region,
                locale,
            } => cli::users_characters(username, provider, region, locale),
        },
        Some(Command::Passwords { command }) => match command {
            PasswordsCommand::Status => cli::passwords_status(),
            PasswordsCommand::Enable => cli::passwords_set_enabled(true),
            PasswordsCommand::Disable => cli::passwords_set_enabled(false),
        },
        Some(Command::Oauth2Providers { command }) => match command {
            ProvidersCommand::List => cli::providers_list(),
            ProvidersCommand::Add {
                slug,
                name,
                issuer,
                client_id,
                client_secret,
                scope,
                claims,
                authorization_endpoint,
                token_endpoint,
                userinfo_endpoint,
                allow_registration,
                allow_auto_connection,
                trust_email_verified,
                allow_disconnection,
                allow_unclaimed_username_connection,
            } => cli::providers_add(cli::custom_provider(cli::CustomProvider {
                slug,
                name,
                issuer,
                client_id,
                client_secret,
                scope,
                claims,
                authorization_endpoint,
                token_endpoint,
                userinfo_endpoint,
                allow_registration,
                allow_auto_connection,
                trust_email_verified,
                allow_disconnection,
                allow_unclaimed_username_connection,
            })?),
            ProvidersCommand::Remove { slug } => cli::providers_remove(slug),
        },
    }
}
