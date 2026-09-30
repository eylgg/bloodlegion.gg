# @bloodlegion/web

The Blood Legion site: a SvelteKit app built with the Node adapter.

```sh
pnpm install
pnpm dev        # http://localhost:5173
pnpm check      # svelte-check
pnpm lint       # prettier + eslint
pnpm test:e2e   # Playwright, against a production build on :4173
pnpm build      # writes the Node server to build/
```

The shipped image is built from `Containerfile`; see the repository README for how it is
published and run.
