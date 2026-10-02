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
   (their characters); the site does not store that token yet.

2. Sign in with the button under the logo on the front page, then make that account an admin and
   turn password login off (the site has no password form, so it only matters for the API):

   ```sh
   bloodlegion-server users promote <the username you chose>
   bloodlegion-server passwords disable
   ```

   Disabling is refused while no other login source is enabled, so nobody gets locked out.

The rest:

```sh
bloodlegion-server users list
bloodlegion-server users demote <username>
bloodlegion-server users rename <username> <new username>
bloodlegion-server users disable <username>     # suspend: sessions stop, login refused
bloodlegion-server users enable <username>
bloodlegion-server passwords status | enable
bloodlegion-server oauth2-providers list
bloodlegion-server oauth2-providers remove <slug>
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
