# Task 4 Report: Database Connection, Migrations and Test Harness

## Summary
Task 4 built the `keeppix-db` crate: a `Db`/`DbError` connection layer over `sqlx::PgPool`, three migrations (`users`/`groups`/`group_members`, `sessions`, `system_settings`), and a per-test Postgres+PostGIS container harness (`testcontainers`) used by four integration tests. Per the controller ruling, Steps 11–12 (sqlx-cli offline query cache) were skipped entirely — all queries in this task use the runtime `sqlx::query`/`query_scalar` function forms, not the compile-time `query!` macros, so there is nothing for `cargo sqlx prepare` to verify.

All 4 integration tests pass against a real Postgres 17 / PostGIS 3.5 container. Clippy (`all` + `pedantic`, `-D warnings`) and `cargo fmt --check` are clean.

**Commit SHA:** `ada768e`

---

## Implementation Steps

### Step 1: Add dependencies
Ran the brief's `cargo add` commands with two adjustments (see Decisions):

```bash
cargo add sqlx --no-default-features \
  --features runtime-tokio,tls-rustls-ring,postgres,uuid,chrono,macros,migrate -p keeppix-db
cargo add keeppix-domain --path crates/keeppix-domain -p keeppix-db   # already present from Task 1-3
cargo add serde chrono tracing tokio -p keeppix-db
cargo add --dev testcontainers-modules --features postgres -p keeppix-db
cargo add --dev tokio --features macros,rt-multi-thread -p keeppix-db
```

`uuid` (dev-only, needed for `uuid::Uuid::now_v7()` in the test) and `thiserror` were wired via `.workspace = true` directly in `Cargo.toml` rather than `cargo add`, and a direct `testcontainers` dev-dependency was **not** added (see Decisions #1 and #2).

Final `crates/keeppix-db/Cargo.toml`:
```toml
[package]
name = "keeppix-db"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true

[dependencies]
thiserror.workspace = true
keeppix-domain = { path = "../keeppix-domain" }
sqlx = { version = "0.8.6", default-features = false, features = ["runtime-tokio", "tls-rustls-ring", "postgres", "uuid", "chrono", "macros", "migrate"] }
serde.workspace = true
chrono.workspace = true
tracing.workspace = true
tokio.workspace = true

[lints]
workspace = true

[dev-dependencies]
testcontainers-modules = { version = "0.15.0", features = ["postgres"] }
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
uuid.workspace = true
```

### Step 2: `src/error.rs`
Transcribed verbatim from the brief: `DbError::{Connection, Migration, NotFound, Conflict}` plus `From<sqlx::migrate::MigrateError>`.

### Steps 3-5: Migrations
Created `crates/keeppix-db/migrations/0001_users.sql`, `0002_sessions.sql`, `0003_settings.sql` transcribed verbatim from the brief — `pg_trgm` extension, `users`/`groups`/`group_members` with the exact column/index names (`users_username_key`, `users_email_key`, `groups_name_key`, `group_members_user_idx`), `sessions` with `sessions_refresh_hash_key`/`sessions_family_idx`/`sessions_user_idx`/`sessions_expiry_idx`, and `system_settings`.

### Step 6: `src/lib.rs`
Transcribed verbatim: `Db { pool: PgPool }` with `connect`, `migrate` (via `static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations")`), `pool`, `ping`.

### Step 7: Test harness
`crates/keeppix-db/tests/harness/mod.rs` transcribed verbatim: `TestDb::start()` boots a `postgis/postgis:17-3.5` container via `testcontainers_modules::postgres::Postgres`, connects, runs migrations, and exposes `TestDb::db(&self) -> &Db`. Added `#[allow(clippy::expect_used)]` on `start()` per the project's established pattern for test code (clippy's `expect_used` is workspace-level `warn`, and `-D warnings` would otherwise fail it).

### Step 8: Migration tests
`crates/keeppix-db/tests/migrations.rs` transcribed verbatim: 4 tests — `migrations_apply_to_an_empty_database`, `migrations_are_idempotent`, `expected_tables_exist`, `usernames_are_unique_case_insensitively`. Added `#[allow(clippy::expect_used)]` per test function, matching the harness's pattern.

### Step 9: dev `uuid` dependency
Added `uuid.workspace = true` under `[dev-dependencies]` directly (see Decisions #3 for why `cargo add --dev uuid --features v7` was not used).

### Step 10: Run tests
Docker was confirmed running (`docker info`) before starting. First run pulled `postgis/postgis:17-3.5` and compiled ~180 crates; total wall time for that run was a few minutes. Result: 4/4 tests passing. A second, cache-warm run took under 7 seconds — see Final Verification below.

### Steps 11-12: Skipped
Per the controller's explicit override, `sqlx-cli`, `cargo sqlx prepare`, `.sqlx/`, and `SQLX_OFFLINE` were **not** touched. No `.sqlx/` directory was created.

### Step 13: Commit
See commit section below.

---

## Decisions

1. **No direct `testcontainers` dependency.** The brief's Step 1 lists `cargo add --dev testcontainers testcontainers-modules --features postgres -p keeppix-db`. Adding `testcontainers` as a *direct* dependency pulled the latest `testcontainers v0.28.0`, which requires a newer `bollard-stubs` than the one `testcontainers-modules v0.15.0` pins (it depends on `testcontainers ^0.27.0`) — this produced an unresolvable version conflict (`bollard-stubs` requirement mismatch). The harness code only ever refers to `testcontainers_modules::testcontainers::...` (a re-export), so a direct `testcontainers` dependency is unnecessary; I added only `testcontainers-modules = { version = "0.15.0", features = ["postgres"] }` to `[dev-dependencies]`. This resolves cleanly and the harness code compiles and runs unchanged.

2. **`keeppix-domain` path dependency without an explicit `version`.** `cargo add --path` wrote `keeppix-domain = { version = "0.1.0", path = "../keeppix-domain" }`; I trimmed the `version` field to match the style already used by `keeppix-api` and `keeppix-server` (`keeppix-domain = { path = "../keeppix-domain" }`), for consistency across the workspace. Not required for correctness, purely a style match.

3. **`uuid --features v7` via `cargo add` fails; worked around by hand-editing `Cargo.toml`.** `cargo add uuid --features v7 -p keeppix-db` (and even plain `cargo add uuid -p keeppix-db` with no explicit features) errors with `error: unrecognized feature for crate uuid: v7`, even though the registry index for the resolved `uuid v1.24.0` genuinely lists `v7` as a feature (verified by inspecting the cached index JSON) and pinning an explicit version (`cargo add uuid@1.24.0 --features v7`) works fine. This looks like a `cargo add` bug specific to *workspace-inherited* dependencies without a pinned version. Since `workspace.dependencies.uuid` already declares `features = ["v7", "serde"]`, no extra feature flags were needed anyway — I added `uuid.workspace = true` directly to `[dev-dependencies]` by editing the manifest, sidestepping the buggy command entirely. `cargo build`/`test` confirm `uuid::Uuid::now_v7()` resolves correctly.

4. **`cargo fmt` reformatted the brief's SQL/Rust.** The brief's exact Rust snippets (`Self { _container: container, db }`, the `for expected in [...]` loop, and one `assert!` call in `tests/migrations.rs`) don't match this workspace's rustfmt output. Since a clean `cargo fmt --check` is a hard verification requirement, I ran `cargo fmt` after transcription; this only reformatted line-wrapping/braces, no logic or identifiers changed. Column/index/table names, SQL, and all identifiers are untouched and match the brief verbatim.

5. **No genuine TDD red phase.** This task is transcription of exact, brief-specified SQL and Rust (not exploratory feature design), so there was no meaningful "write a failing test, then implement" cycle — the test file and the implementation it exercises were specified together in the brief. The closest real "failure" encountered was the dependency-resolution errors documented in Decisions #1 and #3 above, both captured with their actual error output. Once the manifest was fixed, `cargo check -p keeppix-db --all-targets` compiled clean on the first attempt, and `cargo test -p keeppix-db` passed 4/4 on the first execution.

---

## Final Verification

### `cargo test -p keeppix-db` (cache-warm run, image already pulled)
```
$ cargo test -p keeppix-db
   Compiling keeppix-db v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-db)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.55s
     Running unittests src/lib.rs (target/debug/deps/keeppix_db-e2e059ebd955cd47)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/migrations.rs (target/debug/deps/migrations-3199ce445a41c136)

running 4 tests
test migrations_apply_to_an_empty_database ... ok
test usernames_are_unique_case_insensitively ... ok
test expected_tables_exist ... ok
test migrations_are_idempotent ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.97s

   Doc-tests keeppix_db

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

First run (cold — no `postgis/postgis:17-3.5` image cached locally) produced the identical `4 passed; 0 failed` result, taking ~33s to compile plus ~32s for the 4 tests (each test starts its own container, confirming isolation).

### `cargo clippy -p keeppix-db --all-targets -- -D warnings` (after `cargo clean -p keeppix-db` to force a full re-check)
```
$ cargo clean -p keeppix-db
     Removed 676 files, 156.8MiB total
$ cargo clippy -p keeppix-db --all-targets -- -D warnings
    Checking keeppix-db v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-db)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.09s
```
Clean — no warnings, `all` + `pedantic` included via workspace lint config.

### `cargo fmt --check`
```
$ cargo fmt --check
$ echo $?
0
```
Clean, after the reformatting described in Decisions #4.

### `cargo build --workspace` (sanity check that the rest of the workspace still builds)
```
$ cargo build --workspace
   Compiling keeppix-api v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-api)
   Compiling keeppix-db v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-db)
   Compiling keeppix-server v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.80s
```

---

## Files Created / Modified

- **Created:** `crates/keeppix-db/src/error.rs`
- **Created:** `crates/keeppix-db/migrations/0001_users.sql`
- **Created:** `crates/keeppix-db/migrations/0002_sessions.sql`
- **Created:** `crates/keeppix-db/migrations/0003_settings.sql`
- **Created:** `crates/keeppix-db/tests/harness/mod.rs`
- **Created:** `crates/keeppix-db/tests/migrations.rs`
- **Modified:** `crates/keeppix-db/src/lib.rs`
- **Modified:** `crates/keeppix-db/Cargo.toml`
- **Modified:** `Cargo.lock` (dependency resolution)

No `.sqlx/` directory, no `sqlx-cli` installation, no `SQLX_OFFLINE` usage — per the controller's ruling.

---

## Concerns

- None blocking. The task delivers exactly `Db`, `DbError`, three migrations, and the test harness — no `UserRepo`/`SettingsRepo`/`SessionRepo` code was added, per the "Constraints on you" in the controller brief.
- Two `cargo add` quirks (documented in Decisions #1 and #3) required manual `Cargo.toml` edits instead of the brief's literal commands; the resulting dependency graph and code are functionally identical to what the brief specifies, and both issues are pre-existing `cargo`/registry-index behavior unrelated to this codebase.
- `cargo fmt` reformatted the three transcribed Rust files' whitespace/line-wrapping (documented in Decisions #4); no SQL, identifiers, or logic changed.
