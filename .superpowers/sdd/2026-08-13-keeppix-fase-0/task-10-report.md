# Task 10 Report — First-run setup and session authentication

Branch: `fase-0` (unchanged, no new branch created).

## What was done, per step

### Step 1 — Dependencies

`keeppix-domain` was already a `keeppix-api` dependency from an earlier task, so that
`cargo add` was a no-op / skipped. Added:

```
cargo add --dev testcontainers-modules --features postgres -p keeppix-api
cargo add --dev reqwest --no-default-features --features json,cookies -p keeppix-api
cargo add time -p keeppix-api
```

Deviation from the brief: `reqwest`'s TLS feature in the resolved version (0.13.4) is
named `rustls`, not `rustls-tls` as the brief's command specifies — `rustls-tls` no
longer exists in that version. Used `--features json,rustls,cookies` instead. Did not
add a direct `testcontainers` dependency (the brief's step 1 lists it): `keeppix-db`'s
proven harness (`crates/keeppix-db/tests/harness/mod.rs`) reaches `ContainerAsync`,
`ImageExt`, and `AsyncRunner` entirely through `testcontainers_modules::testcontainers::*`
re-exports, with no direct `testcontainers` crate dependency. Mirrored that working
pattern instead of adding an unused direct dependency. `tokio` already carries
`macros`/`rt-multi-thread` via the workspace's `features = ["full"]`, so no separate
dev-dependency was needed there either.

Resulting `[dev-dependencies]` in `crates/keeppix-api/Cargo.toml`:
```
http-body-util = "0.1.5"
reqwest = { version = "0.13.4", default-features = false, features = ["json", "rustls", "cookies"] }
testcontainers-modules = { version = "0.15.0", features = ["postgres"] }
tower = { version = "0.5.3", features = ["util"] }
```
and `time = "0.3.55"` added to `[dependencies]`.

### Step 2 — HTTP harness

Wrote `crates/keeppix-api/tests/harness/mod.rs` exactly per the brief, following the
proven `keeppix-db` harness pattern (real Postgres container via testcontainers,
migrations run, then a real axum server bound to an ephemeral port via
`axum::serve`, driven by a `reqwest::Client` with a cookie store). Added
`#[allow(clippy::expect_used)]` on `TestServer::start` (mirroring
`keeppix-db/tests/harness/mod.rs`'s convention) since clippy's `expect_used` lint is
warn-level workspace-wide and the harness legitimately panics on setup failure.

### Step 3 — Failing tests

Wrote `crates/keeppix-api/tests/auth.rs`, the brief's ten tests verbatim, with
per-function `#[allow(clippy::unwrap_used)]` (and `expect_used` where needed) added to
match this codebase's established convention (confirmed by grepping
`crates/keeppix-db/tests/*.rs`, which uses localized per-function allows rather than a
blanket file-level allow).

### Step 4 — Red phase

Ran `cargo test -p keeppix-api --test auth`. All 10 tests failed as expected — every
assertion compared an actual `404` (route doesn't exist yet) against the expected
status/body. Full failure output is in the Verification section below.

### Step 5 — `cookie.rs`

Implemented exactly per the brief: `session_cookie(token, ttl, secure)`,
`clearing_cookie()`, `should_be_secure(host)`. Did not hardcode `secure: true` and kept
the `__Host-` prefix, per the controller ruling.

### Step 6 — `routes/setup.rs`

Implemented `status` (`GET /setup/status`) and `create` (`POST /setup`) exactly per the
brief, including the `409 keeppix/already-initialised` mapping from `DbError::Conflict`
and the `422 keeppix/invalid-username` / `422 keeppix/invalid-password` mappings from
domain parse errors.

### Step 7 — `routes/auth.rs`, with the controller's dummy-hash fix

Implemented `login`, `refresh`, `logout`, `me`, and `UserView` per the brief, **except**
for `dummy_hash()`, which was replaced per the controller ruling — see the dedicated
section below.

### Step 8 — Mounting routes

`crates/keeppix-api/src/routes/mod.rs`:
```rust
pub mod auth;
pub mod health;
pub mod setup;
```

`crates/keeppix-api/src/lib.rs`: added `pub mod cookie;`, added `api_routes()`, and
changed `base_router()` to nest it under `/api/v1`, preserving `.fallback(not_found)`
before the `.layer(...)` chain in `common_layers` untouched (per the ordering
constraint from Task 9 — did not move or duplicate it). `base_router_stateless()` was
left as-is (`/health` only), used only by `tests/health.rs`.

### Steps 9–10 — Verification

See the Verification section below for full real output.

### Step 11 — Commit

See Commit section below.

## The dummy-hash fix (controller ruling)

The brief's `dummy_hash()` returned a hand-typed PHC string
(`$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHR2YWx1ZQ$0000...0000`) whose final segment
is not valid base64 output for an Argon2 hash of that length/params — `argon2::PasswordHash::new`
rejects it at parse time, so `verify_password` returns `false` immediately without ever
running Argon2. That defeats the entire purpose of calling `verify_password` on a dummy
hash for unknown usernames: the response-time difference between "user doesn't exist"
(fails to parse, ~microseconds) and "wrong password" (parses, runs Argon2id, ~100ms) is
still fully observable, which is exactly the username-enumeration side channel the code
comment claims to close.

**Fix**: generated a real Argon2id hash with the project's own `hash_password`, using
the project's own OWASP-recommended parameters (`m=19456, t=2, p=1`, same constants
`keeppix-domain/src/password.rs` uses for every real hash), so `verify_password` parses
it successfully and performs the full Argon2 computation.

Generation method: added a temporary `#[test]` to `crates/keeppix-domain/src/password.rs`
that called `hash_password` on an arbitrary fixed password and printed the PHC string via
`eprintln!`, ran it with `cargo test -p keeppix-domain print_dummy_hash_for_task10 --
--nocapture`, captured the output, then reverted the temporary test with
`git checkout -- crates/keeppix-domain/src/password.rs` (confirmed via `git diff --stat`
showing no changes, and `cargo test -p keeppix-domain` afterwards still shows the
original 22 tests passing).

Resulting constant, embedded in `crates/keeppix-api/src/routes/auth.rs`:
```
$argon2id$v=19$m=19456,t=2,p=1$BKjMC3FKz54nTDnFf9fLRQ$Lckl7W7KbvukoSApSxfeAzdhbmnPBAyeHtIIl9Dhmhs
```

**Verification test** (`crates/keeppix-api/src/routes/auth.rs`,
`tests::dummy_hash_is_a_valid_argon2id_phc_string`):
- asserts `hash.as_str().starts_with("$argon2id$")`
- asserts `hash.as_str().contains("m=19456,t=2,p=1")`
- asserts `!verify_password(&attempted, &hash)` for an arbitrary login-attempt password
  (`"correct horse battery staple"`, distinct from the password used to *generate* the
  dummy hash) — i.e. the dummy hash never authenticates any real credential, it exists
  purely to make `verify_password` do real Argon2 work.

Caught during self-review: my first draft of this test used the *same* plaintext that
generated the hash as the "attempted" password, which would make `verify_password`
return `true` (an actual match) and fail the assertion — that would have been a real bug
in the test, not just a style issue. Fixed before running anything, by using a different
plaintext for the verification attempt.

`login_fails_identically_for_unknown_user` was kept exactly as specified in the brief —
no changes.

## Decisions

1. **`reqwest` TLS feature name**: used `rustls` instead of the brief's `rustls-tls`
   (renamed in reqwest 0.13.x). Smallest change to make `cargo add` succeed.
2. **No direct `testcontainers` dependency**: relied on `testcontainers-modules`'s
   re-export, matching `keeppix-db`'s already-working pattern, instead of adding an
   unused direct dependency the brief's step 1 lists.
3. **Test lint allows**: added `#[allow(clippy::unwrap_used)]` /
   `#[allow(clippy::expect_used)]` per test function in `tests/auth.rs` (the brief's
   listing omits these), matching the established per-function convention in
   `crates/keeppix-db/tests/*.rs`, since `cargo clippy --workspace --all-targets -- -D
   warnings` must be clean and these lints are workspace warn-level.
4. **Extra unit test**: added one unit test in `keeppix-api` (`dummy_hash_is_a_valid_argon2id_phc_string`)
   beyond the 13 the brief's own step 9 predicts, per the controller ruling's explicit
   instruction to add such a test. Total is 14 (3 health + 10 auth + 1 dummy-hash), not
   13.
5. Everything else in `cookie.rs`, `routes/setup.rs`, `routes/auth.rs` (apart from
   `dummy_hash`), and the router wiring follows the brief verbatim — no other
   simplifications made.

## Verification

### Red phase (Step 4) — real failure output

```
$ cargo test -p keeppix-api --test auth
running 10 tests
test login_succeeds_with_correct_credentials ... FAILED
test login_fails_identically_for_unknown_user ... FAILED
test refresh_rotates_the_session_cookie ... FAILED
test a_fresh_instance_reports_not_initialised ... FAILED
test login_fails_with_wrong_password ... FAILED
test setup_can_only_run_once ... FAILED
test logout_invalidates_the_session ... FAILED
test me_requires_authentication ... FAILED
test setup_creates_the_first_admin_and_logs_in ... FAILED
test setup_rejects_a_weak_password ... FAILED

failures:

---- login_succeeds_with_correct_credentials stdout ----
thread 'login_succeeds_with_correct_credentials' panicked at crates/keeppix-api/tests/auth.rs:131:5:
assertion `left == right` failed: lo username è case-insensitive
  left: 404
 right: 200

---- login_fails_identically_for_unknown_user stdout ----
thread 'login_fails_identically_for_unknown_user' panicked at crates/keeppix-api/tests/auth.rs:167:5:
assertion `left == right` failed
  left: 404
 right: 401

---- refresh_rotates_the_session_cookie stdout ----
thread 'refresh_rotates_the_session_cookie' panicked at crates/keeppix-api/tests/auth.rs:279:10:
set-cookie presente

---- a_fresh_instance_reports_not_initialised stdout ----
thread 'a_fresh_instance_reports_not_initialised' panicked at crates/keeppix-api/tests/auth.rs:20:5:
assertion `left == right` failed
  left: Null
 right: false

---- login_fails_with_wrong_password stdout ----
thread 'login_fails_with_wrong_password' panicked at crates/keeppix-api/tests/auth.rs:148:5:
assertion `left == right` failed
  left: 404
 right: 401

---- setup_can_only_run_once stdout ----
thread 'setup_can_only_run_once' panicked at crates/keeppix-api/tests/auth.rs:95:5:
assertion `left == right` failed
  left: 404
 right: 409

---- logout_invalidates_the_session stdout ----
thread 'logout_invalidates_the_session' panicked at crates/keeppix-api/tests/auth.rs:245:5:
assertion `left == right` failed
  left: 404
 right: 204

---- me_requires_authentication stdout ----
thread 'me_requires_authentication' panicked at crates/keeppix-api/tests/auth.rs:189:5:
assertion `left == right` failed
  left: 404
 right: 401

---- setup_creates_the_first_admin_and_logs_in stdout ----
thread 'setup_creates_the_first_admin_and_logs_in' panicked at crates/keeppix-api/tests/auth.rs:40:5:
assertion `left == right` failed
  left: 404
 right: 201

---- setup_rejects_a_weak_password stdout ----
thread 'setup_rejects_a_weak_password' panicked at crates/keeppix-api/tests/auth.rs:112:5:
assertion `left == right` failed
  left: 404
 right: 422

test result: FAILED. 0 passed; 10 failed; 0 ignored; 0 measured; 0 filtered out; finished in 17.76s
error: test failed, to rerun pass `-p keeppix-api --test auth`
```

### Green phase — `cargo test -p keeppix-api` (real output)

```
$ cargo test -p keeppix-api
     Running unittests src/lib.rs (target/debug/deps/keeppix_api-ac91c6ba9e1128ce)

running 1 test
test routes::auth::tests::dummy_hash_is_a_valid_argon2id_phc_string ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s

     Running tests/auth.rs (target/debug/deps/auth-a15f218e15bae5ec)

running 10 tests
test a_fresh_instance_reports_not_initialised ... ok
test refresh_rotates_the_session_cookie ... ok
test me_requires_authentication ... ok
test logout_invalidates_the_session ... ok
test login_fails_identically_for_unknown_user ... ok
test setup_can_only_run_once ... ok
test login_succeeds_with_correct_credentials ... ok
test login_fails_with_wrong_password ... ok
test setup_rejects_a_weak_password ... ok
test setup_creates_the_first_admin_and_logs_in ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.54s

     Running tests/health.rs (target/debug/deps/health-97aba4da10978c24)

running 3 tests
test health_returns_ok ... ok
test security_headers_are_present ... ok
test unknown_api_path_returns_problem_json ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests keeppix_api
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Total: 14 passed (3 health + 10 auth + 1 dummy-hash unit test).

### `cargo clippy --workspace --all-targets -- -D warnings` (real output)

```
$ cargo clippy --workspace --all-targets -- -D warnings
    Checking keeppix-api v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-api)
    Checking keeppix-server v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.89s
```
Exit code 0, no warnings. (An earlier run before adding `#[allow(clippy::expect_used)]`
to two test functions in `tests/auth.rs` failed with two `expect_used` errors on lines
using `.expect("il setup deve autenticare subito")` and `.expect("set-cookie presente")`
— fixed by adding the allow attribute to those two functions, matching the codebase's
established per-function convention.)

### `cargo fmt --check` (real output)

First run found formatting drift in `routes/auth.rs` and `routes/setup.rs` (long method
chains that `rustfmt` prefers to wrap). Ran `cargo fmt`, then re-ran the check:

```
$ cargo fmt --check
```
Exit code 0, no diff.

### Full workspace sanity check

`cargo test -p keeppix-domain` still passes with the original 22 tests (0 diff in
`crates/keeppix-domain/`), confirming the temporary scratch test used to generate the
dummy hash left no trace.

## Concerns

- None blocking. The `create_bootstrap_admin` conflict mapping (brief's Step 6) maps
  *any* `DbError::Conflict` from that call — both "instance already initialised" and a
  theoretical unique-constraint race on username/email during the bootstrap insert — to
  `409 keeppix/already-initialised`. In practice the bootstrap path can only race on the
  "table already has users" check (enforced by `LOCK TABLE users IN EXCLUSIVE MODE` in
  `keeppix-db`), so this is not reachable in normal operation, but it's worth flagging
  for anyone reading the error-mapping code later.
- `routes/auth.rs`'s `pub type Ctx = AuthContext;` (kept verbatim from the brief) is
  currently unused outside the module. Kept it since removing it would deviate from the
  brief without a concrete reason, and clippy is silent on unused `pub` items.
