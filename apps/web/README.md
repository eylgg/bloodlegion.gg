# @bloodlegion/web

The Blood Legion site: a SvelteKit app built with the Node adapter.

```sh
pnpm install
pnpm dev        # http://localhost:5173
pnpm check      # svelte-check
pnpm lint       # prettier + eslint
pnpm test:e2e   # Playwright, against a production build on :4173 (no backend needed)
pnpm build      # writes the Node server to build/
```

Server-side loads call the backend at `BACKEND_URL` (default `http://localhost:8080`); the
browser's own `/api` calls rely on the reverse proxy described in the repository README. With no
backend reachable, every page still renders: visitors are simply anonymous and the front page
shows only the logo. Sign-in is the provider button under the logo; there is no separate sign-in
page.

The shipped image is built from `Containerfile`; see the repository README for how it is
published and run.
