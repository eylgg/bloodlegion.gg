# Blood Legion

The website for Blood Legion, at [bloodlegion.gg](https://bloodlegion.gg).

## Layout

- `apps/web`: the SvelteKit site, built with the Node adapter and shipped as the `bloodlegion-web`
  image. It serves the pages and forwards `/api`, `/auth`, and `/.well-known` to the backend.
- `crates/bloodlegion-server`: the backend (Rust, axum, Postgres), shipped as the `bloodlegion-server` image.
  Accounts and sessions, username/password login, OAuth2 / OpenID Connect login sources such as
  Battle.net, and an OpenID Connect provider so other guild tools can sign people in with their
  Blood Legion account. The same binary is the admin CLI.
- `crates/bloodlegion-macros`: the `Problem` derive the backend's error responses use.
- `crates/xtask`: the local dev stack and image builds (`cargo xtask ...`).

## Local Development

The dev stack is a Postgres container, managed by the Rust `xtask` (needs `podman`):

```sh
cargo xtask dev up      # start postgres (idempotent)
cargo xtask dev status  # what is running
cargo xtask dev down    # stop, keeping data
cargo xtask dev reset   # stop and delete data; the next `up` starts fresh
```

The backend reads its configuration from the environment, e.g. a gitignored
`crates/bloodlegion-server/.env`:

```sh
ORIGIN=https://bloodlegion.localhost
DATABASE_URL=postgres://bloodlegion@localhost:5432/bloodlegion
ENCRYPTION_KEY=   # a 32-byte key, base64: openssl rand -base64 32
```

`ENCRYPTION_KEY` encrypts secrets at rest (provider client secrets, the JWT signing key). Then:

```sh
cargo run -p bloodlegion-server   # applies migrations and serves on :8080
pnpm -C apps/web dev       # the site on :5173
```

Server-side page loads reach the backend at `BACKEND_URL` (default `http://localhost:8080`).
The browser's own API calls need the reverse proxy in front of both, which is what the `Caddyfile`
does for `bloodlegion.localhost`. TLS for it uses `mkcert`. On macOS:

```sh
brew install mkcert
brew install nss # in addition, if using Firefox
```

Trust its certificate authority, then generate the certificate the `Caddyfile` serves
(the two files are gitignored):

```sh
mkcert -install
mkcert -cert-file .cert.pem -key-file .key.pem bloodlegion.localhost
```

With the backend and `pnpm dev` running, `caddy run` serves the site at
<https://bloodlegion.localhost>.

### Tests

```sh
cargo test --workspace        # needs DATABASE_URL: each test gets a throwaway database
pnpm -C apps/web test:e2e     # Playwright, against a production build; no backend needed
```

The backend's queries are checked against the database at compile time. After changing one, run
`cargo sqlx prepare -- --all-targets` inside `crates/bloodlegion-server` (needs
`cargo install sqlx-cli`) to refresh the offline data in its `.sqlx`, which is what the container
and CI builds compile against.

## Accounts and login

Everything an operator does is a subcommand of the `bloodlegion-server` binary, run with the server's
environment (in the container: `podman exec <container> bloodlegion-server ...`).

Bootstrap, from a fresh database:

1. Register Battle.net as the login source. Create an OAuth client at
   <https://develop.battle.net> with the redirect URL `<ORIGIN>/auth/oauth2/callback`, then:

   ```sh
   bloodlegion-server oauth2-providers add \
       --slug battlenet --name Battle.net \
       --issuer https://oauth.battle.net/oauth --client-id ... --client-secret ... \
       --scope "openid wow.profile" --allow-registration
   ```

   Battle.net is a standard OpenID Connect provider, discovered from its issuer. Its `openid`
   scope asserts no email and no username, only a BattleTag, so on a first sign-in the person
   chooses their own username (the page suggests one from the tag) and the account has no
   email. `wow.profile` additionally asks the person for access to their WoW account profile
   (their characters); the site keeps each account's token from its latest sign-in.

2. Sign in with the button under the logo on the front page, then make that account an admin and
   the guild leader, and turn password login off (the site has no password form, so it only
   matters for the API):

   ```sh
   bloodlegion-server users promote <the username you chose>
   bloodlegion-server users rank <the username you chose> leader
   bloodlegion-server passwords disable
   ```

   Disabling is refused while no other login source is enabled, so nobody gets locked out.

3. Optionally, let members unlink Battle.net accounts from their profile (it never lets them
   unlink their last way to sign in):

   ```sh
   bloodlegion-server oauth2-providers update battlenet --allow-disconnection true
   ```

A member may link several Battle.net accounts from their profile page (`/profile`), and sign in
with any of them. Linking signs in at Battle.net again, asking it to prompt for an account; if
Battle.net signs straight back into an account already linked, the page says so, and signing out
at battle.net first lets the person pick another. An account linked to one member cannot be linked
to another until it is unlinked.

The rest:

```sh
bloodlegion-server users list
bloodlegion-server users demote <username>
bloodlegion-server users rename <username> <new username>
bloodlegion-server users rank <username> <rank>  # leader, officer, raider, trial, member, friend, retired
bloodlegion-server users characters <username>   # the WoW characters on each linked Battle.net account
bloodlegion-server items sync [--id <item id>]... # sync the item mirror now (the server syncs daily)
bloodlegion-server characters seed              # test characters: four per class, linked to no one
bloodlegion-server characters unseed            # remove them (keeps any that raided or won loot)
bloodlegion-server launch                # every member's WoW: Forever launch sign-ups
bloodlegion-server users disable <username>     # suspend: sessions stop, login refused
bloodlegion-server users enable <username>
bloodlegion-server passwords status | enable
bloodlegion-server oauth2-providers list
bloodlegion-server oauth2-providers remove <slug>
bloodlegion-server oauth2-providers update <slug> --allow-registration <bool> --allow-disconnection <bool>
bloodlegion-server oauth2-providers add --help  # any OpenID Connect or plain OAuth2 provider
bloodlegion-server rotate-jwk                   # rotate the OpenID Connect signing key
```

`oauth2-providers add` takes `--claim claim=target` mappings (targets: `username`, `email`,
`first_name`, `last_name`, `is_email_verified`, and for plain OAuth2 `subject`). Omit every mapping
to use the standard OpenID Connect claims. Whenever a provider asserts no usable username, the
person picks their own on first sign-in; `:normalize` on a username mapping instead coerces the
asserted value into the username shape. Mapping an `email` is optional throughout: an account may
have none.

Usernames are 2 to 32 characters, a letter then letters and digits. The capitalization the person
chose is kept for display; sign-in and uniqueness are case-insensitive.

## The guild

The loot tracker from the old site (Project Blood Legion, a Django app for a Classic realm), reshaped
for WoW: Forever. Every page needs a signed-in member; leaders and officers (and superusers) make
the changes.

- **Ranks**: leader, officer, raider, trial, member, friend, retired. New accounts are members.
  Officers set ranks from the roster; only the leader makes or unmakes officers, and nobody sets
  their own.
- **Characters** have a first and a last name (2 to 24 letters each) and no realm: Forever has
  rulesets instead, and the guild plays on one (PvP), so a full name is unique across the site.
  Members add their own on their profile, with one main; officers may add anyone's, including a
  character no account claims (a pug who won loot). A character's notes are written by its player
  and read by officers (`/notes`).
- **Raids** are one night in a zone, with the characters who came and the loot they won. The zones
  and their sizes are the app's catalog (`raids::catalog`, mirrored by the database's
  `raid_zone_size()`, which caps attendance): Barrow Deeps (10), Hyjal Summit (20), and Onyxia's
  Lair (40). A raid's start is stored as local time (`starts_local`, the guild's clock: 8 PM) with
  its time zone; the database derives the instant (`starts_at`). A raid keeps the zone it was
  scheduled in.
- **Settings** (`/settings`, superusers): the guild's time zone (default `America/New_York`) and
  the time raids usually start (default 20:00), which a new raid is scheduled in and defaults to.
- **The raid builder**, on each raid's page: the attendees in groups of five (two groups for the
  Barrow Deeps, four for Hyjal Summit, eight for Onyxia's Lair) and a bench. Officers move people
  (dragging, or clicking one then where they go) and switch the spec someone plays that night;
  everyone sees the layout. Alongside, what the groups bring: raid-wide and group-only buffs (the
  latter per group), debuffs, utility (combat resses, dispels, interrupts), and the roles.
- **The raid planner** (`/raids/plan`): a raid week's raids side by side, with the roster by
  player to fill them from (drag, or click then click). A player (an account, whichever of their
  characters) is in one raid at a time: raids starting within three hours of each other overlap
  (`raids::RAID_LENGTH`). A character is saved to a zone for the raid week. The server refuses
  either; the planner offers to move the player out of the other raid instead.
- **Specs and talents**: characters have two specs (dual spec), each with the notable talents it
  takes, the ones that change what a character brings (`launch::catalog`). What each class, spec,
  and talent brings is `raids::effects`: Classic's to start, to correct as Forever's become known.
  Characters not linked to a member show no player.
- **Weeks** number the lockouts (`raids::calendar`). The raids open December 9, 2026 at 6 PM New
  York time, kept as that wall-clock time and zone (the timezone database is bundled into the
  binary). Lockouts reset every Tuesday at 15:00 UTC, so in local time the reset moves by an hour
  with daylight saving. Week 1 runs from the release to the first reset (December 15); each reset
  starts the next. A raid's week is derived from its start, never stored, so moving the release
  renumbers everything.
- **Bosses and items** are entered as the guild meets them, since Forever's raids are new. Only
  Onyxia is seeded. Recording loot by item name creates the item on first sight. A boss's page shows
  how often each item dropped.
- **The item database** is a local mirror of the game's (`game_items`), with tooltips and icons
  served from it: no third-party tooltip script. It syncs from Battle.net's Game Data API with the
  `battlenet` provider's client (a client credentials token; no extra configuration), daily and on
  demand. Until the API serves WoW: Forever it mirrors Classic Era (`static-classic1x-us`), listing
  every epic and legendary item (dropping any other the guild has not won) and fetching each one's tooltip and icon; a refresh asks with
  `If-Modified-Since`, so unchanged items cost little; a call that fails for a moment (a timeout,
  a 5xx, a 429) is retried, and an item still missing its tooltip or icon is retried next sync.
  The Items page browses it, 50 at a time; recording loot searches it, and a guild item picked from
  it is linked by the game's item id.

  ```sh
  bloodlegion-server items sync               # everything (the first run fetches thousands of items)
  bloodlegion-server items sync --id 16800    # just these, listed or not
  ```
- **Questions**: officers ask yes-or-no questions; members answer and may change their minds.

## Images

```sh
podman build -t bloodlegion-server -f crates/bloodlegion-server/Containerfile .
podman build -t bloodlegion-web apps/web
cargo xtask build-images   # both, tagged with the version
```

The backend container needs `ORIGIN`, `DATABASE_URL`, and `ENCRYPTION_KEY`, and listens on 8080.
The web container needs `ORIGIN` and `BACKEND_URL` (e.g. `http://bloodlegion-server:8080`), and listens on
3000. Put a reverse proxy in front that sends `/api`, `/auth`, and `/.well-known` to the backend and
everything else to the web container, and that sets `X-Forwarded-For` (the backend's login
throttle keys on it).

Every push to `main` builds the images and publishes them to the GitHub Container Registry as
`ghcr.io/<owner>/bloodlegion-web` and `ghcr.io/<owner>/bloodlegion-server`, tagged `latest` and with the
commit SHA; tags matching `v*` are published under their version (see `.github/workflows/`).

## License

Licensed under the [MIT License] or the [Apache License, Version 2.0], at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the [Apache License, Version 2.0], shall be dual licensed as above,
without any additional terms or conditions.

[Apache License, Version 2.0]: LICENSES/Apache-2.0.txt
[MIT License]: LICENSES/MIT.txt
