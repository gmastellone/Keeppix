# Keeppix frontend

Vue 3 + TypeScript + Vite. Built output is embedded into the Rust binary at compile time via
`rust-embed` — see the root [`README.md`](../README.md) for the full build and development flow.

```bash
npm ci
npm run dev     # dev server, proxies API calls to :5673
npm run build   # required before `cargo build`/`cargo run` on the backend
```

Initial bundle budget: 150 KB gzip, checked in CI (`npm run build` reports the size). Lazy
per-route chunks are outside that budget. See [`../CONTRIBUTING.md`](../CONTRIBUTING.md) for the
full frontend conventions (i18n key parity, no hardcoded user-facing strings, component structure).
