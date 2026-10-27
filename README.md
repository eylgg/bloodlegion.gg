# Blood Legion

The website for Blood Legion, at [bloodlegion.gg](https://bloodlegion.gg).

## Layout

- `apps/web`: the SvelteKit site, built with the Node adapter and shipped as the `bloodlegion-web`
  image. It serves the pages and forwards `/api`, `/auth`, and `/.well-known` to the backend.
- `apps/addon`: the in-game addon, `BloodLegion`. `/bl import` loads a keybind plan exported
  from the site (Profile, Keybinds): the spells on each bar and stance bar, and the key bindings.
  Copy `apps/addon/BloodLegion` into the game's `Interface/AddOns`.
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
   bloodlegion-server oauth2-providers update battlenet --allow-multiple-connections true
   ```

   Battle.net is a standard OpenID Connect provider, discovered from its issuer. Its `openid`
   scope asserts no email and no username, only a BattleTag, so on a first sign-in the person
   chooses their own username (the page suggests one from the tag) and the account has no
   email. `wow.profile` additionally asks the person for access to their WoW account profile
   (their characters); the site keeps each account's token from its latest sign-in. A member
   may link only one account at a provider unless it allows multiple connections, as Battle.net
   does, since many keep characters on several.

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

4. Optionally, let members link their Discord account. Create an application at
   <https://discord.com/developers/applications> with the redirect URL
   `<ORIGIN>/auth/oauth2/callback`, then:

   ```sh
   bloodlegion-server oauth2-providers add \
       --slug discord --name Discord \
       --issuer https://discord.com --client-id ... --client-secret ... \
       --scope "openid identify"
   bloodlegion-server oauth2-providers update discord --connection-only true --allow-disconnection true
   ```

   Discord is an OpenID Connect provider too. Connection-only, it is never a way to sign in: the
   front page offers no Discord button, and a linked Discord account never counts as a way in, so
   every member keeps at least one Battle.net account. Members link it from their profile, one
   Discord account each, and may unlink it.

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
bloodlegion-server users rank <username> <rank>  # leader, officer, raider, trial, member, friend
bloodlegion-server users characters <username>   # the WoW characters on each linked Battle.net account
bloodlegion-server characters seed              # test characters: four per class, linked to no one
bloodlegion-server characters unseed            # remove them (keeps any that raided or won loot)
bloodlegion-server launch                # every member's WoW: Forever launch sign-ups
bloodlegion-server users disable <username>     # suspend: sessions stop, login refused
bloodlegion-server users enable <username>
bloodlegion-server passwords status | enable
bloodlegion-server oauth2-providers list
bloodlegion-server oauth2-providers remove <slug>
bloodlegion-server oauth2-providers update <slug> --allow-registration <bool> --allow-disconnection <bool> --allow-multiple-connections <bool> --connection-only <bool>
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

- **Ranks**: leader, officer, raider, trial, member, friend. New accounts are members.
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
- **Administration** (`/admin`, superusers): the guild's time zone (default `America/New_York`) and
  the time raids usually start (default 20:00), which a new raid is scheduled in and defaults to.
- **The raid builder**, on each raid's page: the raid's groups of five (two for the Barrow Deeps,
  four for Hyjal Summit, eight for Onyxia's Lair), with the roster beside them. Everyone on a raid
  stands in a group; there is no bench. Officers drag people in from the roster (or click one, then
  a free slot), move them between slots (onto someone swaps the two), switch the spec someone plays
  that night, and drag them back to the roster (or ×) to take them off; everyone sees the layout.
  Each group lists the group-only buffs it has; alongside, what the raid brings: buffs, debuffs,
  utility (combat resses, dispels, interrupts), and the roles.
- **The raid planner** (`/raids/plan`): a raid week's raids side by side, the roster beside them,
  sorted by player (username) and then the characters no member plays. Raids have no names: a
  zone's raids in a week are numbered ("Onyxia's Lair 2"), and "Add a group" adds another of the
  same zone at the same time. A player (an account, whichever of their characters) is in one raid
  at a time: raids starting within three hours of each other overlap (`raids::RAID_LENGTH`). A
  character is saved to a zone for the raid week. The server refuses either; the planner offers to
  move the player out of the other raid instead.
- **Specs and talents**: a character plays any of its class's specs, each with the notable talents
  it takes there, the ones that change what a character brings (`launch::catalog`); at most one is
  its main (or none). In a raid each plays one of them: the one picked that night, else the main,
  else the only one. What each class, spec,
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
  served from it: no third-party tooltip script. It is a snapshot in the repository
  (`crates/bloodlegion-server/data/game-items`: `items.json`, one item a line, and the icons) that
  the server embeds. Every migrate, so every start, loads it when its hash differs from the one last
  loaded: the site works offline, and each deploy has the items of its own commit. A snapshot that
  no longer has an item drops it, unless the guild has won it. The Items page browses it, 50 at a
  time; recording loot searches it, and a guild item picked from it is linked by the game's item id.

  It is refreshed with the rest of Blizzard's data; see [Blizzard's data](#blizzards-data).
- **Questions**: officers ask yes-or-no questions; members answer and may change their minds.

## The Discord server

The guild's Discord server is kept by a bot, the same Discord application members link (step 4
above). It makes a role for each rank (Leader, Officer, Raider, Trial, Member, Friend) and four
categories that follow the site's access levels, each open to its level and those above it: Public
(everyone), Members (every role), Raiders (leader, officers, raiders, trials), and Officers (leader
and officers). In Public it makes #announcements, which everyone reads and the leader and officers
post in; its posts show on the front page, kept in step as they are edited or deleted.

It never removes anything, and gives no one a role. It makes what is missing (taking over a role or
category of the same name made by hand), and on its own categories sets only its own permission
entries, so others added by hand stay. Renaming its roles or categories is fine: it keeps track by
id.

The **Discord page** (`/admin/discord`, superusers) reads the server and shows how it stands, a
heading per check: the bot's state (running, since when, last seen, its last check and error); the
server's people and bots, how many members linked Discord and are in it, and #announcements (with
its id) and its kept posts; its **roles** and **categories** with their Discord ids (each role with
its holders against the linked members whose rank calls for it, each category with its channels),
and whether they stand top to bottom as the site has them; linked members not in the server, without
their rank's role, or holding another of the bot's roles besides; and people holding one of the
bot's roles who have not linked Discord. The bot never reorders anything on its own: an order that
is off has a **Fix order** button, which moves only the bot's own roles or categories, among the
places they already hold, so everything else keeps its place. Discord lets the bot move only roles
below its own, which the page warns of.

To set it up, in the application's **Bot** page turn on the **Message Content** intent (for the
announcements) and **Server Members** (for the role check), and invite it to the server (**OAuth2 →
URL Generator**, scope `bot`) with Manage Roles, Manage Channels, View Channels, Send Messages, and
Read Message History. Then, as a superuser, enter the bot's token and the Discord server's id
(right-click the server with Developer Mode on, **Copy Server ID**) under **Discord bot** on the
Administration page (`/admin`). The token is checked with Discord, stored encrypted with
`ENCRYPTION_KEY`, and never shown again.

Until both are set there is no bot, and changing either restarts it. With several instances
running, one runs the bot at a time (a Postgres advisory lock), and another takes over within a
minute if it goes away.

## Blizzard's data

Everything the site shows of the game is vendored, so nothing reads Blizzard at run time and the
site works offline: the item snapshot above (`crates/bloodlegion-server/data/game-items`), the
keybind planner's spells (`apps/web/src/lib/wow/spells.json` and `apps/web/static/wow/spells`), and
the class and talent tree icons (`apps/web/static/wow/classes` and `specs`). One command refreshes
it all, then the diff is reviewed and committed:

```sh
BATTLENET_CLIENT_ID=... BATTLENET_CLIENT_SECRET=... cargo xtask blizzard sync
cargo xtask blizzard sync --only classes   # just some: items, spells, classes (classes needs no credentials)
```

The credentials are the `battlenet` sign-in client's. Battle.net's Game Data API is read as Classic
Era (`static-classic1x-us`) until it serves WoW: Forever; icons come from Blizzard's render
service. A call that fails for a moment (a timeout, a 5xx, a 429) is retried.

- **Items**: every epic and legendary item, with its tooltip and icon name, fetched again (a few
  minutes); only icons not already vendored are downloaded. An item that still fails keeps its last
  snapshot entry.
- **Spells**: which spells each class has, their levels, trees, ranks, and tooltip lines are edited
  by hand in `spells.json`, since the API serves none of them. The sync updates each spell's name,
  description (unless the API leaves a formula unworked), and icon where the API has them, and
  keeps the rest.
- **Classes**: each class's and talent tree's icon, by the game's icon name.

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
