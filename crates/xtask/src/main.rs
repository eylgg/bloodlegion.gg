use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
struct Args {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Print the build version derived from the crate version and git state.
    Version,
    /// Build the `bloodlegion-server` and `bloodlegion-web` container images.
    BuildImages,
    /// Build, then tag and push the images to a registry. `registry` is the
    /// registry/namespace prefix, e.g. `ghcr.io/eylgg` -> images at
    /// `ghcr.io/eylgg/bloodlegion-server:<version>`. `latest` is updated only for a real
    /// release (a bare version, no pre-release). Run `podman login` first.
    PushImages { registry: String },
    /// Manage the local development stack, the `bloodlegion-postgres` container that
    /// backs a host-run `cargo run` / Vite session.
    Dev {
        #[command(subcommand)]
        command: DevCmd,
    },
}

#[derive(Debug, Subcommand)]
enum DevCmd {
    /// Start postgres if not already up (idempotent).
    Up,
    /// Stop the dev container, keeping its volume (data survives).
    Down,
    /// `down`, then delete the dev volume so the next `up` starts fresh.
    Reset,
    /// Show the dev container and whether it is running.
    Status,
}

const POSTGRES_IMAGE: &str = "docker.io/library/postgres:18.4-trixie";
const POSTGRES_CONTAINER: &str = "bloodlegion-postgres";
const POSTGRES_VOLUME: &str = "bloodlegion-postgres-data";

fn workspace_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir.pop();
    dir
}

/// Echoes a prepared command line to stderr in bold cyan, so the commands we shell
/// out to stand apart from their own output. The colour is dropped when stderr is
/// not a terminal (a captured log gets clean text rather than escape sequences);
/// `NO_COLOR` disables it too.
fn echo_line(line: &str) {
    if std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        eprintln!("\x1b[1;36m{line}\x1b[0m");
    } else {
        eprintln!("{line}");
    }
}

/// Echoes a `program args` command line (see [`echo_line`]).
fn echo(program: &str, args: &[&str]) {
    echo_line(&format!("{program} {}", args.join(" ")));
}

/// Runs a command to completion, sending its stdout to the void (container and
/// image ids would otherwise pollute our own stdout) and inheriting stderr.
fn run(workspace: &Path, program: &str, args: &[&str]) -> Result<()> {
    echo(program, args);
    let status = Command::new(program)
        .args(args)
        .current_dir(workspace)
        .stdout(Stdio::null())
        .status()
        .with_context(|| format!("spawning `{program}`"))?;
    ensure!(status.success(), "`{program}` exited with {status}");
    Ok(())
}

/// Runs a command and returns its trimmed stdout, inheriting stderr, without
/// echoing the command line, for queries whose only wanted output is what they
/// print (the `version` derivation, container-state lookups).
fn capture_quiet(workspace: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(workspace)
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("spawning `{program}`"))?;
    ensure!(
        output.status.success(),
        "`{program}` exited with {}",
        output.status
    );
    let stdout = String::from_utf8(output.stdout).with_context(|| format!("`{program}` output"))?;
    Ok(stdout.trim().to_owned())
}

/// Runs a command silently and reports only whether it exited zero, for the
/// questions whose answer is a boolean (`git diff --quiet`, readiness probes)
/// and for best-effort cleanup. A non-zero exit is the `false` answer, but
/// failing to spawn at all is still an error: a missing binary must not read as
/// "not ready".
fn succeeded(workspace: &Path, program: &str, args: &[&str]) -> Result<bool> {
    let status = Command::new(program)
        .args(args)
        .current_dir(workspace)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("spawning `{program}`"))?;
    Ok(status.success())
}

/// Runs a command and returns its trimmed stdout, or `None` if it fails. Unlike
/// [`capture`], a non-zero exit is a normal outcome (`git describe` off a tag),
/// not a fatal error.
fn try_capture(workspace: &Path, program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(workspace)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The crate version from `crates/bloodlegion-server/Cargo.toml` (the first
/// `version = "..."`), which is the `[package]` version.
fn crate_version(workspace: &Path) -> Result<String> {
    let path = workspace.join("crates/bloodlegion-server/Cargo.toml");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    text.lines()
        .find_map(|line| {
            let rest = line.trim_start().strip_prefix("version")?;
            let value = rest
                .trim_start()
                .strip_prefix('=')?
                .trim()
                .trim_matches('"');
            (!value.is_empty()).then(|| value.to_owned())
        })
        .with_context(|| format!("no version found in {}", path.display()))
}

/// Derives a build version from the crate version (what we're working toward)
/// and git state.
///
/// The crate version splits into `<full>` and an optional pre-release after `-`.
/// The pre-release `dev` is special, it marks "in development" and gets a
/// git-derived build version; anything else (a bare `<full>`, or a real
/// pre-release like `<full>-alpha.1`) is a tagged release emitted verbatim:
///
/// - crate version `<full>-dev` -> `<full>-dev.<count>+<sha>[.dirty]`
///   (e.g. `0.0.2-dev.239+3efd61`); `<count>` is commits since the nearest tag,
///   `+<sha>[.dirty]` is semver build metadata.
/// - crate version `<full>` or `<full>-<pre>` (pre != dev) -> that exact string,
///   but only when HEAD is on its tag (`v<version>`) and the tree is clean.
///
/// It fails loudly on the mistakes worth catching: a release version that isn't
/// on its tag, a dirty release checkout, or a `-dev` version colliding with an
/// existing release tag.
fn version(workspace: &Path) -> Result<String> {
    let raw = crate_version(workspace)?;
    let (full, pre) = match raw.split_once('-') {
        Some((full, pre)) => (full, Some(pre)),
        None => (raw.as_str(), None),
    };
    let clean = succeeded(workspace, "git", &["diff", "--quiet", "HEAD"])?;

    // Anything other than the `dev` marker is a real (pre-)release: it must sit
    // on its own tag, clean, and is emitted verbatim.
    if pre != Some("dev") {
        let release_tag = format!("v{raw}");
        let tag = try_capture(workspace, "git", &["describe", "--tags", "--exact-match"])
            .with_context(|| {
                format!(
                    "version {raw} is a release version but HEAD is not on its tag \
                     (expected {release_tag}); use a -dev crate version while developing"
                )
            })?;
        ensure!(
            tag == release_tag,
            "git tag {tag} does not match version {raw} (expected {release_tag})"
        );
        ensure!(
            clean,
            "working tree is dirty on tag {tag}; refusing to build {raw}"
        );
        return Ok(raw);
    }

    // `-dev`: a development build, versioned by commit distance from the nearest
    // tag plus the commit sha.
    let count = match try_capture(workspace, "git", &["describe", "--tags", "--abbrev=0"]) {
        Some(closest) => {
            ensure!(
                closest != format!("v{full}"),
                "version {full} already has the release tag {closest}; bump the crate version"
            );
            capture_quiet(
                workspace,
                "git",
                &["rev-list", "--count", &format!("{closest}..HEAD")],
            )
            .with_context(|| format!("counting commits since {closest}"))?
        }
        None => capture_quiet(workspace, "git", &["rev-list", "--count", "HEAD"])
            .context("not a git repository, or it has no commits")?,
    };
    let count: u64 = count
        .parse()
        .with_context(|| format!("could not parse commit count {count:?}"))?;

    let sha = capture_quiet(workspace, "git", &["rev-parse", "--short=6", "HEAD"])
        .context("reading the HEAD commit sha")?;

    // `dev.<count>` carries the position; the sha (and `.dirty`) are build metadata.
    Ok(if clean {
        format!("{full}-dev.{count}+{sha}")
    } else {
        format!("{full}-dev.{count}+{sha}.dirty")
    })
}

/// Builds both images, tagging each with the OCI version tag and `latest`, and
/// returns that version tag (for `push`).
fn build_images(workspace: &Path) -> Result<String> {
    // The OCI tag drops the `+<sha>[.dirty]` build metadata; what remains
    // (`<full>` or `<full>-<pre>.<count>`) is already a valid tag. Tag both the
    // version and the bare name, so the images can be referenced either way.
    let version = version(workspace)?;
    let tag = version
        .split_once('+')
        .map_or(version.as_str(), |(v, _)| v)
        .to_owned();
    eprintln!("building images for version {version} (tag {tag})");
    let server_versioned = format!("bloodlegion-server:{tag}");
    let web_versioned = format!("bloodlegion-web:{tag}");
    run(
        workspace,
        "podman",
        &[
            "build",
            "-t",
            "bloodlegion-server",
            "-t",
            server_versioned.as_str(),
            "-f",
            "crates/bloodlegion-server/Containerfile",
            ".",
        ],
    )?;
    run(
        workspace,
        "podman",
        &[
            "build",
            "-t",
            "bloodlegion-web",
            "-t",
            web_versioned.as_str(),
            "apps/web",
        ],
    )?;
    Ok(tag)
}

/// Builds the images, then tags and pushes them under `registry` (a
/// registry/namespace prefix, e.g. `ghcr.io/eylgg`). Refuses a dirty
/// working tree (unlike `build-images`, which allows one locally). `latest` is
/// updated only for a real release, a bare version with no pre-release or build
/// metadata, which is the only case the version tag has no `-`. Requires a prior
/// `podman login <registry-host>`.
fn push_images(workspace: &Path, registry: &str) -> Result<()> {
    let registry = registry.trim_end_matches('/');
    // A dirty tree is fine to build locally, but never to publish, fail before
    // spending a build.
    ensure!(
        succeeded(workspace, "git", &["diff", "--quiet", "HEAD"])?,
        "refusing to push a dirty working tree; commit or stash your changes first"
    );
    let tag = build_images(workspace)?;
    let is_release = !tag.contains('-');
    for image in ["bloodlegion-server", "bloodlegion-web"] {
        push_one(workspace, registry, image, &tag)?;
        if is_release {
            push_one(workspace, registry, image, "latest")?;
        }
    }
    if is_release {
        eprintln!("pushed release {tag} and updated latest");
    } else {
        eprintln!("pushed {tag} (latest left unchanged, not a release)");
    }
    Ok(())
}

fn push_one(workspace: &Path, registry: &str, image: &str, tag: &str) -> Result<()> {
    let local = format!("{image}:{tag}");
    let remote = format!("{registry}/{image}:{tag}");
    run(
        workspace,
        "podman",
        &["tag", local.as_str(), remote.as_str()],
    )?;
    run(workspace, "podman", &["push", remote.as_str()])
}

/// Whether a container named exactly `name` is currently running.
fn container_running(workspace: &Path, name: &str) -> Result<bool> {
    let out = capture_quiet(
        workspace,
        "podman",
        &[
            "ps",
            "--filter",
            &format!("name=^{name}$"),
            "--filter",
            "status=running",
            "--format",
            "{{.Names}}",
        ],
    )?;
    Ok(out.lines().any(|line| line == name))
}

/// Whether a container named `name` exists at all (running or stopped).
fn container_exists(workspace: &Path, name: &str) -> Result<bool> {
    succeeded(workspace, "podman", &["container", "exists", name])
}

/// Runs a command inheriting stdout and stderr so its output reaches the user
/// (unlike [`run`], which discards stdout), for `dev status`.
fn run_inherit(workspace: &Path, program: &str, args: &[&str]) -> Result<()> {
    echo(program, args);
    let status = Command::new(program)
        .args(args)
        .current_dir(workspace)
        .status()
        .with_context(|| format!("spawning `{program}`"))?;
    ensure!(status.success(), "`{program}` exited with {status}");
    Ok(())
}

/// Ensures the postgres dev container is running: reuse it if present (starting a
/// stopped one), else create it with a named volume so the database survives
/// re-creation. Idempotent.
fn postgres_up(workspace: &Path) -> Result<()> {
    if container_running(workspace, POSTGRES_CONTAINER)? {
        eprintln!("{POSTGRES_CONTAINER} already running");
        return Ok(());
    }
    if container_exists(workspace, POSTGRES_CONTAINER)? {
        run(workspace, "podman", &["start", POSTGRES_CONTAINER])?;
        eprintln!("{POSTGRES_CONTAINER} started (postgres on localhost:5432)");
        return Ok(());
    }
    // postgres:18 declares its VOLUME at /var/lib/postgresql (18+ moved it there
    // from /var/lib/postgresql/data), so the named volume mounts that parent and
    // keeps the DB across `dev down`/`up`. Trust auth + checksums mirror the README.
    let volume = format!("{POSTGRES_VOLUME}:/var/lib/postgresql");
    run(
        workspace,
        "podman",
        &[
            "run",
            "--detach",
            "--name",
            POSTGRES_CONTAINER,
            "-e",
            "POSTGRES_USER=bloodlegion",
            "-e",
            "POSTGRES_INITDB_ARGS=--data-checksums",
            "-e",
            "POSTGRES_HOST_AUTH_METHOD=trust",
            "-p",
            "5432:5432",
            "--volume",
            &volume,
            POSTGRES_IMAGE,
        ],
    )?;
    eprintln!("{POSTGRES_CONTAINER} created (postgres on localhost:5432)");
    Ok(())
}

/// `dev up`: ensure postgres is up.
fn dev_up(workspace: &Path) -> Result<()> {
    postgres_up(workspace)
}

/// `dev down`: stop the postgres container, keeping it and its volume so a later `up` just
/// restarts it with its data intact. Best-effort: a missing container is ignored.
fn dev_down(workspace: &Path) -> Result<()> {
    let args = ["stop", POSTGRES_CONTAINER];
    echo("podman", &args);
    let _ = succeeded(workspace, "podman", &args);
    Ok(())
}

/// `dev reset`: remove the postgres container and its volume, so the next `up` starts from
/// scratch. A missing container/volume is not an error.
fn dev_reset(workspace: &Path) -> Result<()> {
    // A missing container/volume is fine, but a failed removal is not: reporting
    // "starts fresh" while an old volume survives would silently reuse its data.
    let mut leftovers = Vec::new();
    // --force removes it even if still running; must precede the volume removal.
    let args = ["rm", "--force", "--ignore", POSTGRES_CONTAINER];
    echo("podman", &args);
    if !succeeded(workspace, "podman", &args)? {
        leftovers.push(POSTGRES_CONTAINER);
    }
    if volume_exists(workspace, POSTGRES_VOLUME)? {
        let args = ["volume", "rm", POSTGRES_VOLUME];
        echo("podman", &args);
        if !succeeded(workspace, "podman", &args)? {
            leftovers.push(POSTGRES_VOLUME);
        }
    }
    ensure!(
        leftovers.is_empty(),
        "could not remove {}; the next `dev up` would reuse its data",
        leftovers.join(", ")
    );
    eprintln!("removed the dev container and volume; the next `dev up` starts fresh");
    Ok(())
}

/// Whether a podman volume exists (so `reset` can tell "already gone" from "removal
/// failed").
fn volume_exists(workspace: &Path, volume: &str) -> Result<bool> {
    succeeded(workspace, "podman", &["volume", "exists", volume])
}

/// `dev status`: list the dev container (running or stopped) with its ports.
fn dev_status(workspace: &Path) -> Result<()> {
    run_inherit(
        workspace,
        "podman",
        &[
            "ps",
            "--all",
            "--filter",
            &format!("name=^{POSTGRES_CONTAINER}$"),
            "--format",
            "table {{.Names}}\t{{.Status}}\t{{.Ports}}",
        ],
    )
}

fn main() -> Result<()> {
    let args = Args::parse();
    let workspace = workspace_dir();
    match args.command {
        Cmd::Version => println!("{}", version(&workspace)?),
        Cmd::BuildImages => {
            build_images(&workspace)?;
        }
        Cmd::PushImages { registry } => push_images(&workspace, &registry)?,
        Cmd::Dev { command } => match command {
            DevCmd::Up => dev_up(&workspace)?,
            DevCmd::Down => dev_down(&workspace)?,
            DevCmd::Reset => dev_reset(&workspace)?,
            DevCmd::Status => dev_status(&workspace)?,
        },
    }
    Ok(())
}
