# Task 23 — Report

Not blocked. The router↔spec parity test (`router_registered_routes_are_all_documented`)
started red — 49 mounted operations across `albums`, `share`, `groups`,
`permissions`, `audit`, `backup`, `restore`, `upload`, `health`, `bootstrap`
and `operations` had no `#[utoipa::path]`. Annotated every handler in
`audit`, `permissions`, `backup`, `restore`, `share`, `upload` and `health`
(`albums`/`groups`/`bootstrap`/`operations` were already annotated from
earlier tasks); registered the paths, tags and new view schemas
(`AuditEntryView`, `PermissionGrantView`, `GrantSummaryView`, `ExplainView`,
`ExplainChainLinkView`, `SharedWithMeView`) in `openapi.rs` so `keeppix-db`
stays free of `utoipa`. `docs/api/openapi.json` regenerated; now describes
139 operations (was 90).

`extract_route_calls` in `tests/openapi.rs` parses `lib.rs`'s `.route(...)`
calls (axum 0.8 has no router introspection API) and diffs them against
`openapi.json`'s `paths` — this is the CI check. `upload::patch` and
`share::public_upload` document `Vec<u8>`/octet-stream bodies since they
move raw chunks, not JSON.

Point 3 (`generate-api-clients.sh` in CI) was already wired in
`.github/workflows/ci.yml`'s `api-clients` job from commit `40a0ae9` —
verified, not added. Ran it locally against the regenerated spec: TypeScript
and Swift clients generate cleanly, and the TypeScript client type-checks
with `tsc --noEmit` at zero errors.

All 8 `tests/openapi.rs` tests green. `cargo fmt --check` and `cargo clippy
--workspace --all-targets -- -D warnings` clean. `cargo test -p keeppix-api
--jobs 1 -- --test-threads=1` full suite green, no regressions. 4 commits on
`fase-10`, ledger updated, no push.

Deferred, out of scope: `scripts/check-wired.py` still flags unused public
functions and frontend-less routes (pre-existing, confirmed independent of
this task via `git stash`) — expected mid-phase since most Fase 10 API
groups have no frontend consumer yet.
