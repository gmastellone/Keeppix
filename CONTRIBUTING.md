# Contributing to Keeppix

Keeppix targets a **Raspberry Pi 5, 8 GB RAM, NVMe disk** serving **200,000 photos** as the hardware floor, not an aspirational stretch goal. Weigh every change against that machine, not the one you're developing on. Extreme usability and extreme lightness are the same requirement seen from two sides, not a tradeoff.

- **No new dependency without a reason in the PR description.** Every crate is build time, CVE surface, and runtime RAM.
- **Nothing loads a full table into memory.** Paginate with keyset cursors; never a `SELECT` with no `LIMIT` on a table that grows.
- **The frontend only grows through lazy chunks.** The entry bundle stays under **150 KB gzip** (enforced in CI) — someone just browsing photos shouldn't pay for the admin pages.
- **Never `thread::sleep` in an async context.** It blocks a whole worker-pool thread, not just that job — on 4 cores that's a quarter of the pool's capacity.
- **Every hot-path operation has a budget, checked by a test.**
- **A function the user can't reach doesn't count as done.** A repository method with no route, or a route with no UI, is incomplete work — not finished work waiting on the rest. CI enforces this: `scripts/check-wired.py` fails the build if a public function or mounted route has no real production caller (see `scripts/wired-exceptions.txt` for the two legitimate reasons an exception is allowed: the consumer lands in a not-yet-built follow-up, or the function is deliberately kept as tested internal API with no HTTP surface).

## Invariants — violating one is a real defect, not a style choice

### Architecture

- **No SQL outside `crates/keeppix-db`.** HTTP handlers never write queries. This is enforced mechanically: `sqlx` is a dependency of `keeppix-db` alone, so a query written in a handler crate does not compile.
- **`keeppix-media` doesn't know about the database; `keeppix-db` doesn't know about images.** Enforced by a `[[bans.deny]]` rule in `deny.toml` — adding that edge fails `cargo deny check bans`.
- **Every repository method that reads a specific user's data takes an `AuthContext` as its first parameter.** The only exceptions are already documented in code with the reason in the doc comment (bootstrap/admin-creation paths, username lookup for login, and similar). Don't add a new one without the same explicit justification.
- **`Auth` is the only way an `AuthContext` enters the HTTP layer.** Don't write a helper that fabricates one.

### Security

- **Probing an id you don't own returns `Forbidden`, never `NotFound`.** Otherwise the endpoint becomes an existence oracle — you learn which ids exist by probing them. Applies to users, libraries, folders, assets, albums, everything.
- **Queries are always parameterized.** Never string-concatenate SQL. The only allowed interpolation is of code-level constants (column lists), never of anything that came from outside.
- **`sqlx` only in function form** (`sqlx::query`, `sqlx::query_as`) plus `#[derive(sqlx::FromRow)]` for mapping. Never the `query!` macros, never a `.sqlx/` directory, never `SQLX_OFFLINE`.
- **No `unwrap()` / `expect()` in production code.** Fine in tests, with a local `#[allow(clippy::unwrap_used)]` on the test function.
- **No filesystem path ever arrives from the client.** Media is addressed by id or content hash; the server resolves the real path from its own tree.
- **C decoders (ffmpeg, libraw) run in a disposable, separate process** with `rlimit` and seccomp. Never call them in-process.
- **The `__Host-kpx_session` cookie always carries `Secure`, unconditionally.** Don't reintroduce host-conditional logic here — it has been wrong once already.

### HTTP

- **Every error is RFC 9457** `application/problem+json` with a stable `type` field prefixed `keeppix/`. The backend never translates: `title` is English and exists for debugging; the frontend translates from the `type` code.
- **`/api/v1` is frozen**: additions only, never removals or meaning changes. A breaking change gets `/api/v2`.
- **`.fallback(...)` is registered *before* `with_common_layers(...)`.** In Axum 0.8, `Router::fallback` replaces the catch-all instead of merging with an already-wrapped one — registering it after means every 404 comes back without security headers. Applies to every mount point, including `embed::mount()`.
- **Axum rejections go through `keeppix_api::Json<T>`**, not `axum::Json`, so 415/400/422 stay in `problem+json`.

### Data

- **An asset's identity is `(folder_id, filename)`.** `content_hash` is indexed but **not** unique: the same photo in two folders is two assets with independent deletions. De-duplication is a presentation-layer choice, not an identity rule.
- **No denormalized absolute path on an asset.** It's rebuilt from the `ltree` tree; otherwise moving a folder with 40,000 photos becomes a 40,000-row `UPDATE`.
- **Original metadata is immutable.** `asset_exif` is never rewritten; user edits live in `asset_overrides`, and the displayed value is `COALESCE(override, exif)`.
- **No per-user materialized visibility table.** Changing a permission must take effect immediately.
- **A RAW file is never rewritten.** Metadata goes into an `.xmp` sidecar.

## Working method

### TDD, for real

1. Write the failing test.
2. **Run it and watch it fail.** Don't skip this — a test you haven't seen fail is a test you don't know proves anything.
3. Implement the minimum that makes it pass.
4. Run it again.

A test should fail if the behavior its name claims regresses. Ask yourself: *if I deliberately break the thing this test is supposed to protect, does it fail?*

### Verify before calling anything done

Before considering a task closed, run all of this and read the output:

```bash
cd frontend && npm ci && npm run build   # required: the backend doesn't compile without dist/
cd .. && cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/test.sh
```

`frontend/dist` isn't a test prerequisite, it's a *compile* prerequisite: `rust-embed` bakes that directory in at compile time.

`./scripts/test.sh` runs `cargo test --workspace --jobs 1 -- --test-threads=1` and, even if tests fail, tears down testcontainers and runs `cargo clean`. Don't run `cargo test --workspace` by hand — without `--jobs 1` it starts one PostGIS instance per crate in parallel and `target/` grows to ~9 GB. `--test-threads=1` is required by `keeppix-server/tests/config.rs`, which manipulates process environment. Run clippy *before* the tests — the script deletes `target/` afterward.

Don't say "done" without having seen green output. If something is red, it's red.

### Commits

Conventional commits, one per logical unit of work — not one giant commit at the end of a task.

```
feat(db): add library repository
fix(api): a database outage is a 503, not a session expiry
test(domain): add coverage for User::is_active()
docs: explain the migration checksum error
```

The body explains **why**, not what — the diff already says what.

## Branches and pull requests

- `main` is the last released, stable state. `dev` is the integration branch — branch off it, and open your PR back into it. `test` is a pre-release staging branch, promoted from `dev` periodically before a `main` release; you shouldn't normally need to touch it directly.
- If a decision isn't fully specified by the issue or your own PR description, write down what you decided and why in the PR description — don't decide it silently. A reviewer (human or CI) should be able to see the reasoning without reconstructing it from the diff.

## What not to do

- Don't add a dependency without a real need — every dependency is CI build time and CVE surface. `cargo deny` checks licenses and advisories.
- Don't implement something from a future milestone "since it's barely more work." Scope creep skips review.
- Don't "fix" code outside your current task's scope. If you notice a real defect, open an issue for it instead.
- Don't modify a migration that has already shipped. From the first release onward, only new migration files are added — `sqlx` checks the checksum of applied migrations and refuses to start if one changed.

## Stack

- **Rust 1.88.0**, edition 2024. Axum 0.8, sqlx 0.8, PostgreSQL 17 + PostGIS 3.5 (pgvector on the same instance for AI).
- **Vue 3 + TypeScript + Vite + Tailwind v4 + Reka UI.** No Vuetify.
- Frontend initial bundle budget: **150 KB gzip**, checked in CI. Lazy per-route chunks are outside that budget.
- Docker image: **distroless**, no shell, non-root.
- Translations live in `frontend/src/i18n/{it,en}.json`. No hardcoded user-facing string in a component. Both locales must carry the same keys — CI checks this.
