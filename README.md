# Blood Legion

The website for Blood Legion, at [bloodlegion.gg](https://bloodlegion.gg).

## Layout

- `apps/web`: the SvelteKit site, built with the Node adapter and shipped as the `bloodlegion-web`
  image.

## Local Development

```sh
cd apps/web
pnpm install
pnpm dev
```

TLS for `bloodlegion.localhost` uses `mkcert`. On macOS:

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

With `pnpm dev` running, `caddy run` serves the site at <https://bloodlegion.localhost>.

## Images

```sh
podman build -t bloodlegion-web apps/web
podman run --rm -p 3000:3000 -e ORIGIN=http://localhost:3000 bloodlegion-web
```

Every push to `main` builds the image and publishes it to the GitHub Container Registry as
`ghcr.io/<owner>/bloodlegion-web`, tagged `latest` and with the commit SHA; tags matching `v*`
are published under their version (see `.github/workflows/web.yml`). Set `ORIGIN` to the public
URL wherever the container runs so absolute links resolve correctly.

## License

Licensed under the [MIT License] or the [Apache License, Version 2.0], at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the
work by you, as defined in the [Apache License, Version 2.0], shall be dual licensed as above,
without any additional terms or conditions.

[Apache License, Version 2.0]: LICENSES/Apache-2.0.txt
[MIT License]: LICENSES/MIT.txt
