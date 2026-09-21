# MX Frontend

The MX frontend lives in this directory as part of the same repository and
release artifact as the Rust backend.

## Layout

- `src/` contains the Svelte 5 and TypeScript replacement frontend.
- `dist/` is the generated, verified production bundle shipped with MX so a
  fresh checkout can run without installing Node.js first.
- `public/` contains static assets copied into the production build.

Rust remains responsible for the API, authentication, WebSockets, SQLite, N1
integration, TLS, and static hosting. Svelte produces static HTML, CSS, and
JavaScript only; MX does not require a Node.js server in production.

## Development

Use Node.js 24 or newer:

```bash
cd frontend
npm ci
npm run dev
```

Vite listens on `http://127.0.0.1:5173` and proxies API traffic to
`https://127.0.0.1:21001` by default. Override the backend when needed:

```bash
MX_BACKEND_ORIGIN=https://127.0.0.1:22001 npm run dev
```

Validate and build:

```bash
npm run check
npm test
npm run build
```

The Rust server serves `frontend/dist` in production. Rebuild and commit that
bundle whenever frontend source changes. A Node.js process is not required at
runtime, and an unchanged fresh checkout can start directly with `cargo run`.
