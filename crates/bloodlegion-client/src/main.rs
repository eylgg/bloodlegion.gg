//! Sends the guild roster the Blood Legion addon saved in game to the site, for its race to 60.
//!
//! `login` signs in through the site's OAuth2 (authorization code with PKCE, back to whatever
//! loopback port is free, as a native app does) and keeps the refresh token. `watch` then uploads
//! the addon's SavedVariables file, for every account in the game folder, whenever the game
//! rewrites it (on `/reload` and logout); `upload` does it once. Only an officer's sign-in can
//! upload, and the site takes only Blood Legion's roster.

mod lua;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, bail};
use base64ct::{Base64UrlUnpadded, Encoding};
use clap::{Parser, Subcommand};
use rand::Rng;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

/// The scopes the client asks for: who you are, and the right to upload the roster.
const SCOPE: &str = "openid roster";
/// How often `watch` looks at the files.
const POLL: Duration = Duration::from_secs(2);
/// The addon's saved file, under each account in the game folder.
const SAVED_FILE: &str = "SavedVariables/BloodLegion.lua";

/// Where WoW: Forever installs by default.
fn default_game_folder() -> PathBuf {
    if cfg!(target_os = "macos") {
        PathBuf::from("/Applications/World of Warcraft/_forever_")
    } else {
        PathBuf::from(r"C:\Program Files (x86)\World of Warcraft\_forever_")
    }
}

/// Which saved files to send: one named outright, or every account's in the game folder.
#[derive(clap::Args)]
struct Files {
    /// The game folder (the one with `WTF` in it).
    #[arg(long, default_value_os_t = default_game_folder())]
    game: PathBuf,
    /// One saved file to send instead of every account's:
    /// `WTF/Account/<ACCOUNT>/SavedVariables/BloodLegion.lua`.
    #[arg(long, conflicts_with = "game")]
    file: Option<PathBuf>,
}

impl Files {
    /// The saved files there are now: the account folders are read again each time, since one
    /// gets its file only once the addon has run on it.
    fn find(&self) -> anyhow::Result<Vec<PathBuf>> {
        if let Some(file) = &self.file {
            return Ok(vec![file.clone()]);
        }
        let accounts = self.game.join("WTF").join("Account");
        let entries = std::fs::read_dir(&accounts).with_context(|| {
            format!(
                "reading {} (is the game installed somewhere else? pass --game)",
                accounts.display()
            )
        })?;
        let mut files: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path().join(SAVED_FILE))
            .filter(|path| path.is_file())
            .collect();
        files.sort();
        Ok(files)
    }
}

#[derive(Parser)]
#[command(
    about = "Blood Legion's companion app: sends the addon's guild roster to bloodlegion.com"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Signs in on the site in your browser (an officer's account) and remembers it.
    Login {
        /// The site.
        #[arg(long, default_value = "https://bloodlegion.com")]
        site: Url,
        /// The OAuth2 client registered for this app on the site. A public client's id is no
        /// secret: PKCE, not the id, keeps a sign-in from being taken.
        #[arg(long, default_value = "F1Kys8ISZVsmC2Scrtr3ulbM3xXs2Apz")]
        client_id: String,
    },
    /// Forgets the sign-in.
    Logout,
    /// Uploads the roster once.
    Upload {
        #[command(flatten)]
        files: Files,
    },
    /// Uploads the roster now and whenever the game saves it again, until stopped.
    Watch {
        #[command(flatten)]
        files: Files,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Login { site, client_id } => login(site, client_id).await,
        Command::Logout => {
            match std::fs::remove_file(login_path()?) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(error).context("removing the saved sign-in");
                }
                _ => {}
            }
            println!("Signed out.");
            Ok(())
        }
        Command::Upload { files } => {
            let found = files.find()?;
            if found.is_empty() {
                bail!("no saved roster yet: log in on a character in the guild, then /reload");
            }
            for file in found {
                upload(&file).await?;
            }
            Ok(())
        }
        Command::Watch { files } => watch(&files).await,
    }
}

/// What `login` keeps between runs. The refresh token is a credential: the file is the user's own.
#[derive(serde::Serialize, serde::Deserialize)]
struct Login {
    site: Url,
    client_id: String,
    token_endpoint: String,
    refresh_token: String,
}

/// Where the sign-in is kept: the platform's per-user config folder.
fn login_path() -> anyhow::Result<PathBuf> {
    let var = |name| std::env::var_os(name).map(PathBuf::from);
    let base = if cfg!(windows) {
        var("APPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library/Application Support"))
    } else {
        var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|home| home.join(".config")))
    }
    .context("no home folder to keep the sign-in in")?;
    Ok(base.join("bloodlegion-client").join("login.json"))
}

fn load_login() -> anyhow::Result<Login> {
    let path = login_path()?;
    let text = std::fs::read_to_string(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => anyhow::anyhow!("not signed in: run `login` first"),
        _ => anyhow::Error::new(error).context(format!("reading {}", path.display())),
    })?;
    serde_json::from_str(&text).context("reading the saved sign-in")
}

fn save_login(login: &Login) -> anyhow::Result<()> {
    let path = login_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(login)?;
    // Readable by the user alone, where the platform has modes.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(&path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()))
        .with_context(|| format!("writing {}", path.display()))
}

#[derive(serde::Deserialize)]
struct Discovery {
    authorization_endpoint: String,
    token_endpoint: String,
}

#[derive(serde::Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("bloodlegion-client/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("the HTTP client builds")
}

/// The response, or an error with the site's own explanation (its problem `detail`, or an OAuth2
/// `error_description`) when it refused.
async fn checked(response: reqwest::Response) -> anyhow::Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body: serde_json::Value = response.json().await.unwrap_or_default();
    let reason = ["detail", "error_description", "error"]
        .iter()
        .find_map(|key| body.get(key).and_then(|v| v.as_str()))
        .unwrap_or("no reason given");
    bail!("the site said {status}: {reason}")
}

fn random_token<const N: usize>() -> String {
    let mut bytes = [0u8; N];
    rand::rng().fill_bytes(&mut bytes);
    Base64UrlUnpadded::encode_string(&bytes)
}

async fn login(site: Url, client_id: String) -> anyhow::Result<()> {
    let http = http();
    let discovery: Discovery = checked(
        http.get(site.join("/.well-known/openid-configuration")?)
            .send()
            .await
            .with_context(|| format!("reaching {site}"))?,
    )
    .await?
    .json()
    .await
    .context("reading the site's OpenID configuration")?;

    // Any free port: the site takes any on its registered `http://127.0.0.1/callback`.
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .context("listening for the browser to come back")?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/callback",
        listener.local_addr()?.port()
    );
    let verifier = random_token::<32>();
    let challenge = Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()));
    let state = random_token::<16>();

    let mut authorize = Url::parse(&discovery.authorization_endpoint)
        .context("reading the site's authorization endpoint")?;
    authorize
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", SCOPE)
        .append_pair("state", &state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
    println!("Sign in on the site in your browser. If it does not open, go to:\n{authorize}");
    open_browser(authorize.as_str());

    let code = callback(&listener, &state).await?;
    let tokens: Tokens = checked(
        http.post(&discovery.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", &redirect_uri),
                ("client_id", &client_id),
                ("code_verifier", &verifier),
            ])
            .send()
            .await
            .context("reaching the token endpoint")?,
    )
    .await?
    .json()
    .await
    .context("reading the tokens")?;
    save_login(&Login {
        site,
        client_id,
        token_endpoint: discovery.token_endpoint,
        refresh_token: tokens.refresh_token,
    })?;
    println!("Signed in. Now run `watch`.");
    Ok(())
}

fn open_browser(url: &str) {
    let opened = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).status()
    } else if cfg!(windows) {
        // `start`'s first quoted argument is the window title.
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(url)
            .status()
    } else {
        std::process::Command::new("xdg-open").arg(url).status()
    };
    if !opened.is_ok_and(|status| status.success()) {
        eprintln!("Could not open a browser; open the link above yourself.");
    }
}

/// Waits for the browser to come back to `/callback`, and returns the authorization code.
async fn callback(listener: &TcpListener, state: &str) -> anyhow::Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") && request.len() < 64 * 1024 {
            let read = stream.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        let line = String::from_utf8_lossy(&request);
        let target = line.split_whitespace().nth(1).unwrap_or("/");
        let url = Url::parse("http://127.0.0.1")?.join(target)?;
        if url.path() != "/callback" {
            // The browser's favicon request, and the like.
            stream
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await?;
            continue;
        }
        let param = |name| {
            url.query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        let result = if param("state").as_deref() != Some(state) {
            Err(anyhow::anyhow!(
                "the sign-in came back for another request; try again"
            ))
        } else if let Some(error) = param("error") {
            Err(anyhow::anyhow!(
                "the site refused: {}",
                param("error_description").unwrap_or(error)
            ))
        } else {
            param("code").context("the site sent no authorization code")
        };
        let message = match &result {
            Ok(_) => "Signed in. You can close this tab and go back to the Blood Legion client.",
            Err(_) => "Signing in failed. See the Blood Legion client for why.",
        };
        let page = format!("<!doctype html><title>Blood Legion client</title><p>{message}</p>");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{page}",
            page.len()
        );
        stream.write_all(response.as_bytes()).await?;
        return result;
    }
}

/// A fresh access token from the saved refresh token, which the site replaces with a new one
/// each time (so it is saved again).
async fn access_token(http: &reqwest::Client, login: &mut Login) -> anyhow::Result<String> {
    let tokens: Tokens = checked(
        http.post(&login.token_endpoint)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", &login.refresh_token),
                ("client_id", &login.client_id),
            ])
            .send()
            .await
            .context("reaching the token endpoint")?,
    )
    .await
    .context("refreshing the sign-in (run `login` again if it has expired)")?
    .json()
    .await
    .context("reading the tokens")?;
    login.refresh_token = tokens.refresh_token;
    save_login(login)?;
    Ok(tokens.access_token)
}

#[derive(Debug, PartialEq, serde::Serialize)]
struct Roster {
    guild: String,
    members: Vec<Member>,
}

#[derive(Debug, PartialEq, serde::Serialize)]
struct Member {
    name: String,
    level: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reached_60_at: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RosterResult {
    matched: usize,
    unmatched: Vec<Unmatched>,
}

#[derive(Debug, serde::Deserialize)]
struct Unmatched {
    name: String,
    level: i64,
    class: Option<String>,
}

/// The roster in the addon's saved file (`BloodLegionDB.roster`), as the site takes it.
fn read_roster(source: &str) -> anyhow::Result<Roster> {
    let globals = lua::parse(source).context("reading the saved file")?;
    let roster = globals
        .get("BloodLegionDB")
        .and_then(|db| db.get("roster"))
        .context("no roster saved yet: log in on a character in the guild, then /reload")?;
    let guild = roster
        .get("guild")
        .and_then(lua::Value::as_str)
        .context("the roster names no guild")?
        .to_string();
    let mut members = Vec::new();
    for (name, member) in roster
        .get("members")
        .and_then(lua::Value::as_table)
        .into_iter()
        .flatten()
    {
        let Some(level) = member.get("level").and_then(lua::Value::as_f64) else {
            continue;
        };
        let reached_60_at = match member.get("reached60").and_then(lua::Value::as_f64) {
            Some(seconds) => Some(
                time::OffsetDateTime::from_unix_timestamp(seconds as i64)?
                    .format(&time::format_description::well_known::Rfc3339)?,
            ),
            None => None,
        };
        members.push(Member {
            name: name.clone(),
            level: level as i64,
            class: member
                .get("class")
                .and_then(lua::Value::as_str)
                .map(str::to_string),
            reached_60_at,
        });
    }
    Ok(Roster { guild, members })
}

async fn upload(file: &Path) -> anyhow::Result<()> {
    let source = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let roster = read_roster(&String::from_utf8_lossy(&source))?;
    let mut login = load_login()?;
    let http = http();
    let token = access_token(&http, &mut login).await?;
    let result: RosterResult = checked(
        http.post(login.site.join("/api/characters/roster")?)
            .bearer_auth(token)
            .json(&roster)
            .send()
            .await
            .context("reaching the site")?,
    )
    .await?
    .json()
    .await
    .context("reading the site's answer")?;
    println!(
        "{}: uploaded {} members, {} matched characters on the site.",
        file.display(),
        roster.members.len(),
        result.matched
    );
    if !result.unmatched.is_empty() {
        println!("Not on the site yet (add them as characters to track them):");
        for member in &result.unmatched {
            let class = member.class.as_deref().unwrap_or("?");
            println!("  {} ({class} {})", member.name, member.level);
        }
    }
    Ok(())
}

async fn watch(files: &Files) -> anyhow::Result<()> {
    // Fail now, not on the first change, if there is no sign-in or no game folder.
    load_login()?;
    files.find()?;
    match &files.file {
        Some(file) => println!("Watching {} (Ctrl-C to stop).", file.display()),
        None => println!(
            "Watching every account's saved roster in {} (Ctrl-C to stop).",
            files.game.display()
        ),
    }
    println!("The game saves it on /reload and logout.");
    let modified = |file: &Path| std::fs::metadata(file).and_then(|m| m.modified()).ok();
    let mut last: std::collections::HashMap<PathBuf, SystemTime> = Default::default();
    loop {
        match files.find() {
            Ok(found) => {
                for file in found {
                    let Some(now) = modified(&file) else { continue };
                    if last.get(&file) == Some(&now) {
                        continue;
                    }
                    last.insert(file.clone(), now);
                    if let Err(error) = upload(&file).await {
                        eprintln!("{}: upload failed: {error:#}", file.display());
                    }
                }
            }
            Err(error) => eprintln!("{error:#}"),
        }
        tokio::time::sleep(POLL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_addons_roster() {
        let source = r#"
BloodLegionDB = {
["roster"] = {
["guild"] = "Blood Legion",
["members"] = {
["Garr Bonecrush"] = {
["level"] = 60,
["class"] = "Warrior",
["reached60"] = 1763608500,
},
["Lira Moonwell"] = {
["level"] = 23,
["class"] = "Priest",
},
},
["scannedAt"] = 1763608600,
},
}
"#;
        assert_eq!(
            read_roster(source).unwrap(),
            Roster {
                guild: "Blood Legion".into(),
                members: vec![
                    Member {
                        name: "Garr Bonecrush".into(),
                        level: 60,
                        class: Some("Warrior".into()),
                        reached_60_at: Some("2025-11-20T03:15:00Z".into()),
                    },
                    Member {
                        name: "Lira Moonwell".into(),
                        level: 23,
                        class: Some("Priest".into()),
                        reached_60_at: None,
                    },
                ],
            }
        );
        assert!(read_roster("BloodLegionDB = {}").is_err());
    }
}
