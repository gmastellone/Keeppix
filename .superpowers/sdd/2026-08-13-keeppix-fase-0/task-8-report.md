# Task 8 Report: Configurazione, telemetria e CLI del server

Branch: `fase-0` (unchanged, no new branch created).

## What was done, per step

### Step 1: dependencies

Ran the brief's `cargo add` commands with two adjustments (see Decisions):

```
cargo add figment --features toml,env -p keeppix-server
cargo add clap --features derive,env -p keeppix-server
cargo add tracing-subscriber --features json,env-filter -p keeppix-server
cargo add axum -p keeppix-server
cargo add tower-http --features fs,trace,compression-br,set-header,cors -p keeppix-server
cargo add serde -p keeppix-server
cargo add --dev tempfile -p keeppix-server
```

(`anyhow` and `keeppix-api` were already present in `keeppix-server`'s `Cargo.toml` from earlier tasks, so `cargo add serde anyhow keeppix-api --path crates/keeppix-api` as literally written in the brief was not run — see Decisions.)

Resulting `[dependencies]`/`[dev-dependencies]` in `crates/keeppix-server/Cargo.toml`:
`keeppix-domain`, `keeppix-db`, `keeppix-api` (all path deps, pre-existing), `anyhow`, `tokio`, `tracing` (workspace, pre-existing), `figment 0.10.19` (toml, env), `clap 4.6.6` (derive, env), `tracing-subscriber 0.3.23` (json, env-filter), `axum 0.8.9`, `tower-http 0.7.0` (fs, trace, compression-br, set-header, cors), `serde` (workspace); dev-dep `tempfile 3.27.0`.

### Step 2: failing tests

Created `crates/keeppix-server/tests/config.rs` with the brief's four tests verbatim, with one addition: `#[allow(clippy::unwrap_used)]` on the three test functions that call `.unwrap()` (`defaults_are_applied`, `environment_overrides_the_file`, `bare_database_url_is_accepted`), matching the existing convention in `crates/keeppix-db/tests/*.rs` — required because workspace clippy lints have `unwrap_used = "warn"` and CI runs `-D warnings`.

### Step 3: red phase

```
$ cargo test -p keeppix-server --test config -- --test-threads=1
...
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `keeppix_server`
 --> crates/keeppix-server/tests/config.rs:3:5
  |
3 | use keeppix_server::config::{Config, LogFormat};
  |     ^^^^^^^^^^^^^^ use of unresolved module or unlinked crate `keeppix_server`

error[E0433]: failed to resolve: use of unresolved module or unlinked crate `tempfile`
  --> crates/keeppix-server/tests/config.rs:36:15
   |
36 |     let dir = tempfile::tempdir().unwrap();
   |               ^^^^^^^^ use of unresolved module or unlinked crate `tempfile`

error: could not compile `keeppix-server` (test "config") due to 2 previous errors
```

Confirmed red, as expected (this run was taken right after Step 2, before `tempfile` was added — matches the brief's expectation of an unresolved-import failure).

### Step 4: lib+bin split

Added to `crates/keeppix-server/Cargo.toml`, before `[[bin]]`:

```toml
[lib]
name = "keeppix_server"
path = "src/lib.rs"
```

Created `crates/keeppix-server/src/lib.rs`:

```rust
pub mod config;
pub mod telemetry;
```

### Step 5: `config.rs`

Created `crates/keeppix-server/src/config.rs` verbatim from the brief (`Config`, `LogFormat`, `Defaults`, `Config::load` using Figment with the let-chain `if let Some(path) = config_path && path.exists()` — kept as-is per instructions, not rewritten as nested `if let`).

### Step 6: `telemetry.rs`

Created `crates/keeppix-server/src/telemetry.rs` verbatim from the brief (`telemetry::init(format: LogFormat)`).

### Step 7: green phase (config tests only, main.rs still stub-printing version at this point)

```
$ cargo test -p keeppix-server --test config -- --test-threads=1
...
running 4 tests
test bare_database_url_is_accepted ... ok
test database_url_is_required ... ok
test defaults_are_applied ... ok
test environment_overrides_the_file ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Sanity check: `cargo check -p keeppix-server --lib` — `Finished dev profile [unoptimized + debuginfo] target(s)` (clean).

### Step 8: `main.rs`

Replaced `crates/keeppix-server/src/main.rs` with the brief's version verbatim (CLI with `clap`, three subcommands `Serve`/`Migrate`/`Healthcheck`, `serve()` calling `keeppix_api::router(keeppix_api::AppState::new(db, config.session_ttl_secs))`, graceful shutdown on SIGTERM/Ctrl-C, `healthcheck()` using a raw TCP connect). As documented in the brief and in my task instructions, `keeppix_api::router` and `AppState` do not exist yet (Task 9), so this file does not compile in isolation — expected and not treated as a failure (see Verification below for how this was handled).

### Step 9: `.env.example`

Created `/Users/giovannimastellone/Documents/GitHub/Keeppix/.env.example` verbatim from the brief (repo root — no existing `.env*` file to conflict with).

### Step 10/11: verification and formatting

Ran `cargo fmt -p keeppix-server` to bring `main.rs` and `tests/config.rs` in line with rustfmt (the brief's inline snippets weren't rustfmt-formatted as pasted — e.g. long `assert!`/`assert_eq!` lines and the `clear_env` array literal needed wrapping). This changed only whitespace/line-wrapping, no logic. `cargo fmt --check` is now clean workspace-wide.

Then discovered and worked around a Cargo behavior gap between what the brief/my instructions assumed and actual `cargo test` semantics — see Decisions/Concerns below — and re-verified the 4 config tests pass via the working equivalent.

## Verification commands and real output

**1. `cargo test -p keeppix-server --lib`** (lib-only sanity, unaffected by main.rs):
```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.29s
     Running unittests src/lib.rs (target/debug/deps/keeppix_server-914f591951f61a66)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**2. `cargo check -p keeppix-server --lib`** (compiles `config.rs` + `telemetry.rs` without needing main.rs's `keeppix_api` calls, as suggested by my task instructions):
```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.21s
```
Clean, no warnings.

**3. Literal brief/instruction command `cargo test -p keeppix-server --test config -- --test-threads=1`** — does NOT pass as literally written once `main.rs` exists (see Decisions/Concerns for why):
```
   Compiling keeppix-server v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-server)
error[E0433]: failed to resolve: could not find `AppState` in `keeppix_api`
  --> crates/keeppix-server/src/main.rs:62:48
   |
62 |     let app = keeppix_api::router(keeppix_api::AppState::new(db, config.session_ttl_secs));
   |                                                ^^^^^^^^ could not find `AppState` in `keeppix_api`

error[E0425]: cannot find function `router` in crate `keeppix_api`
  --> crates/keeppix-server/src/main.rs:62:28
   |
62 |     let app = keeppix_api::router(keeppix_api::AppState::new(db, config.session_ttl_secs));
   |                            ^^^^^^ not found in `keeppix_api`

error: could not compile `keeppix-server` (bin "keeppix") due to 2 previous errors
```

**4. Working equivalent — `cargo build -p keeppix-server --test config --keep-going`, then run the produced test binary directly with `--test-threads=1`:**
```
$ cargo build -p keeppix-server --test config --keep-going
   Compiling keeppix-server v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-server)
error[E0433]: failed to resolve: could not find `AppState` in `keeppix_api`
error[E0425]: cannot find function `router` in crate `keeppix_api`
error: could not compile `keeppix-server` (bin "keeppix") due to 2 previous errors
   (non-zero exit, but --keep-going lets Cargo still finish and link the
    "config" test binary — its own rustc invocation succeeds independently)

$ target/debug/deps/config-aa2ac115bb4266a8 --test-threads=1
running 4 tests
test bare_database_url_is_accepted ... ok
test database_url_is_required ... ok
test defaults_are_applied ... ok
test environment_overrides_the_file ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
4/4 tests pass, matching Step 7's earlier green run.

**5. `cargo fmt --check`:**
```
(no output — exit 0, clean)
```

## Decisions

1. **`cargo add` command split for axum/tower-http.** The brief's literal `cargo add axum tower-http --features fs,trace,compression-br,set-header,cors -p keeppix-server` fails on this Cargo version: `error: feature 'fs' must be qualified by the dependency it's being activated for, like 'axum/fs', 'tower-http/fs'` — Cargo can't apply a shared `--features` list across two crates in one invocation. Split into `cargo add axum -p keeppix-server` (default features) followed by `cargo add tower-http --features fs,trace,compression-br,set-header,cors -p keeppix-server`, matching the dependency table the brief's `config.rs`/`main.rs`/`telemetry.rs` code actually needs.

2. **`cargo add serde anyhow keeppix-api --path crates/keeppix-api -p keeppix-server` not run literally.** Same class of issue: `--path` would apply to all three crates, but only `keeppix-api` should get a path, and it errored (`the crate 'serde@.../keeppix-api' could not be found`). `anyhow` and `keeppix-api` were already declared in `keeppix-server`'s `Cargo.toml` from earlier tasks (per the task's "State left by Tasks 1-7" notes), so only `cargo add serde -p keeppix-server` was needed and run.

3. **Added `#[allow(clippy::unwrap_used)]` to the three `.unwrap()`-using test functions** in `tests/config.rs`, per my task instructions ("Match that pattern in your new test file") and the existing convention in `crates/keeppix-db/tests/*.rs`. Test logic/assertions are otherwise verbatim from the brief.

4. **Ran `cargo fmt -p keeppix-server`** on `main.rs` and `tests/config.rs` after pasting the brief's code verbatim, since the brief's inline snippets are not rustfmt-clean (long single-line `assert!`/`assert_eq!` calls, an unwrapped array literal in `clear_env`). Only whitespace changed; `config.rs`, `telemetry.rs`, and `lib.rs` needed no reformatting. This was necessary to make `cargo fmt --check` (an explicit verification requirement) pass.

## Concerns

**The literal verification command `cargo test -p keeppix-server --test config -- --test-threads=1` cannot pass once `main.rs` contains the Task-9-dependent calls, contrary to what my instructions and the brief's Step 10 assumed.** The assumption was that scoping to `--test config` would build only the `config.rs` lib module and the `tests/config.rs` integration test, skipping the broken `bin "keeppix"` target. In practice, on this Cargo 1.88.0, `cargo test -p <pkg>` (with or without `--lib`/`--test <name>` narrowing) always attempts to compile **every** target in the package — including the `bin` — before running anything; a compile error in any target aborts the whole invocation with no tests run. This reproduced identically with `--test config`, `--lib --test config`, and is a known Cargo behavior (target-selection flags filter what's *executed*, not what's *built*), not a bug in my code.

I confirmed the config/telemetry code itself is correct and the 4 tests genuinely pass by using `cargo build -p keeppix-server --test config --keep-going` (lets Cargo continue past the unrelated bin failure and still link the `config` test binary) and then executing that binary directly — 4/4 pass, identical to the Step 7 green-phase run taken before `main.rs` referenced `keeppix_api`. `cargo check -p keeppix-server --lib` also stays clean throughout, confirming `config.rs`/`telemetry.rs`/`lib.rs` are sound in isolation.

Net effect: this is purely a verification-command ergonomics issue caused by `keeppix-server` being a single lib+bin package, not a functional problem — once Task 9 lands and `keeppix_api::router`/`AppState` exist, the plain `cargo test -p keeppix-server --test config -- --test-threads=1` command will work exactly as the brief describes. Flagging this explicitly so it's not mistaken for a regression when Task 9 is verified.

No other concerns. `keeppix-domain` remains an unused dependency of `keeppix-server` (pre-existing from earlier tasks, not touched — `main.rs` doesn't import it, only `keeppix_db::Db` is used).

## Commit

Staged `crates/keeppix-server` and `.env.example`, committed with:

```
feat(server): add layered config, telemetry and cli subcommands
```

## Fix round 1

Two Important review findings, both addressed exactly as scoped — nothing else touched.

### Finding 1: unannotated `.expect()` in `config.rs:42`

Replaced the fallible string parse with an infallible construction, removing the `.expect()` entirely:

`crates/keeppix-server/src/config.rs`, in `Defaults::default()`:
```rust
// before
bind: "0.0.0.0:5673".parse().expect("literal socket address"),
// after
bind: SocketAddr::from(([0, 0, 0, 0], 5673)),
```

### Finding 2: `healthcheck()` ignored the configured bind address

`crates/keeppix-server/src/main.rs`: `healthcheck()` now takes `config_path: &Path`, loads the layered `Config` via `Config::load(Some(config_path))`, and connects to `config.bind.port()` instead of re-deriving the port from a hand-parsed `KEEPPIX_BIND` env var with a hardcoded `5673` fallback. The call site in `main()` now passes `&cli.config`:

```rust
if matches!(cli.command, Some(Command::Healthcheck)) {
    return healthcheck(&cli.config).await;
}
...
async fn healthcheck(config_path: &Path) -> anyhow::Result<()> {
    let config = Config::load(Some(config_path))?;

    let stream = tokio::net::TcpStream::connect(("127.0.0.1", config.bind.port())).await?;
    drop(stream);
    Ok(())
}
```
Added `Path` to the existing `use std::path::{Path, PathBuf};` import. Rest of `main.rs` — including the `keeppix_api::router`/`AppState` calls that don't resolve until Task 9 — left untouched.

### Verification commands and real output

**`cargo fmt -p keeppix-server` then `cargo fmt --check`:**
```
(no output — exit 0, clean)
```

**`cargo clippy -p keeppix-server --lib -- -D warnings`** (the exact command the reviewer required — not `cargo check`):
```
    Checking keeppix-domain v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-domain)
    Checking keeppix-db v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-db)
    Checking keeppix-api v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-api)
    Checking keeppix-server v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.59s
```
Genuinely clean — no `clippy::expect_used` warning, no other warnings.

**`cargo check -p keeppix-server --lib`** (unchanged sanity check):
```
    Checking keeppix-server v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.31s
```

**Config tests, re-verified via the same documented workaround** (the `bin` target still cannot compile until Task 9 supplies `keeppix_api::router`/`AppState`, so plain `cargo test -p keeppix-server --test config` still aborts before running anything — this is unchanged from the original report and is not a regression from this fix):
```
$ cargo build -p keeppix-server --test config --keep-going
   Compiling keeppix-server v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-server)
error[E0433]: failed to resolve: could not find `AppState` in `keeppix_api`
error[E0425]: cannot find function `router` in crate `keeppix_api`
error: could not compile `keeppix-server` (bin "keeppix") due to 2 previous errors

$ target/debug/deps/config-aa2ac115bb4266a8 --test-threads=1
running 4 tests
test bare_database_url_is_accepted ... ok
test database_url_is_required ... ok
test defaults_are_applied ... ok
test environment_overrides_the_file ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
4/4 pass, unchanged.

### Commit

```
fix(server): remove expect_used lint and fix healthcheck to honor configured bind
```
