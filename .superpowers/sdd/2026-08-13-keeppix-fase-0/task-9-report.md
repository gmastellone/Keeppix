# Task 9 report — App state, RFC 9457 problems, auth extractors

## Steps performed

### Step 1 — Dependencies

Ran the `cargo add` commands from the brief. The combined `cargo add tower keeppix-db --path crates/keeppix-db -p keeppix-api` failed as warned in the task context (the `--path` flag would have applied to both crates), so it was split into two separate `cargo add` calls:

```
cargo add tower -p keeppix-api
cargo add keeppix-db --path crates/keeppix-db -p keeppix-api
```

All other `cargo add` invocations from the brief ran as written. Resulting `[dependencies]`/`[dev-dependencies]` in `crates/keeppix-api/Cargo.toml`:

```toml
[dependencies]
thiserror.workspace = true
keeppix-domain = { path = "../keeppix-domain" }
axum = { version = "0.8.9", features = ["macros"] }
axum-extra = { version = "0.12.6", features = ["cookie"] }
tower-http = { version = "0.7.0", features = ["set-header", "trace", "compression-br", "compression-gzip", "cors"] }
tower = "0.5.3"
keeppix-db = { version = "0.1.0", path = "../keeppix-db" }
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
tracing.workspace = true
http = "1.5.0"

[dev-dependencies]
http-body-util = "0.1.5"
tower = { version = "0.5.3", features = ["util"] }
```

(`compression-gzip` was added beyond the brief's literal `cargo add` — see Decisions.)

### Step 2 — Failing test

Created `crates/keeppix-api/tests/health.rs` verbatim from the brief (with `#[allow(clippy::unwrap_used)]` added on each test fn per the workspace convention for test code).

### Step 3 — Red phase

```
$ cargo test -p keeppix-api --test health
...
error[E0425]: cannot find function `router_without_state` in crate `keeppix_api`
 --> crates/keeppix-api/tests/health.rs:7:18
  |
7 |     keeppix_api::router_without_state()
  |                  ^^^^^^^^^^^^^^^^^^^^ not found in `keeppix_api`

error: could not compile `keeppix-api` (test "health") due to 1 previous error
```

Matches the expected failure in the brief exactly.

### Steps 4-8 — Implementation

Created, verbatim from the brief:
- `crates/keeppix-api/src/problem.rs`
- `crates/keeppix-api/src/state.rs`
- `crates/keeppix-api/src/extract.rs`
- `crates/keeppix-api/src/routes/health.rs`
- `crates/keeppix-api/src/routes/mod.rs`
- `crates/keeppix-api/src/lib.rs`

Two deviations from the brief's literal code, both under Decisions below:
1. Added `compression-gzip` feature to `tower-http` (brief's Step 1 only requested `compression-br`, but `lib.rs`'s `CompressionLayer::new().br(true).gzip(true)` needs both features to compile).
2. Removed `#[must_use]` from `router()` and `router_without_state()` — `Router` is already `#[must_use]`, and clippy pedantic's `double_must_use` fails the build otherwise.

### Step 9 — Green phase

```
$ cargo test -p keeppix-api --test health
running 3 tests
test unknown_api_path_returns_problem_json ... ok
test health_returns_ok ... ok
test security_headers_are_present ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### Step 10 — Server smoke test

Started a Postgres/PostGIS container on host port 55433, then ran the server exactly as the brief describes (adjusted port to avoid a clash) with `--config ./nonexistent.toml serve`:

```
$ curl -i -s http://127.0.0.1:5673/health
HTTP/1.1 200 OK
content-type: application/json
x-content-type-options: nosniff
referrer-policy: no-referrer
content-security-policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'
permissions-policy: camera=(), microphone=(), geolocation=()
vary: accept-encoding
content-length: 33
date: Thu, 13 Aug 2026 15:29:55 GMT

{"status":"ok","version":"0.1.0"}
```

Matches the expected body and headers exactly. Server process killed and the Postgres container removed afterward.

### Step 11 — Commit

```
git add crates/keeppix-api crates/keeppix-server Cargo.lock
git commit -m "feat(api): add app state, rfc9457 problems and auth extractors" ...
```

Commit SHA: `53d842d`.

## Verification commands (final, in order)

```
$ cargo test -p keeppix-api --test health
running 3 tests
test unknown_api_path_returns_problem_json ... ok
test health_returns_ok ... ok
test security_headers_are_present ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo build --workspace
   Compiling keeppix-api v0.1.0 (.../crates/keeppix-api)
   Compiling keeppix-server v0.1.0 (.../crates/keeppix-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.46s

$ cargo clippy --workspace --all-targets -- -D warnings
    Checking keeppix-api v0.1.0 (.../crates/keeppix-api)
    Checking keeppix-server v0.1.0 (.../crates/keeppix-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.83s
(no warnings/errors)

$ cargo fmt --check
(no output — clean)
```

## Decisions

1. **`compression-gzip` feature added to `tower-http`.** The brief's Step 1 `cargo add` only requests `set-header,trace,compression-br,cors`, but the brief's own `lib.rs` code calls `CompressionLayer::new().br(true).gzip(true)`. `.gzip()` is behind `tower_http`'s `compression-gzip` cargo feature (confirmed by reading `tower-http-0.7.0/src/compression/layer.rs`), so the code as given does not compile with only `compression-br`. Added the missing feature — smallest change that makes the brief's literal `lib.rs` code compile, and CI logs would have surfaced the same failure regardless.

2. **Removed `#[must_use]` from `router()` and `router_without_state()`.** With clippy pedantic (`-D warnings`), an explicit `#[must_use]` with no message on a function that already returns a type marked `#[must_use]` (`axum::Router` is `#[must_use]`) trips `clippy::double_must_use`. Removed the redundant attribute on both functions; behavior is unchanged since `Router`'s own `#[must_use]` still applies.

3. **`#[allow(clippy::unwrap_used)]` added to each test function** in `tests/health.rs` (the brief's test code snippet didn't include it), matching the workspace-wide pattern for test code described in the task context, since `unwrap_used` is `warn`-level workspace-wide and `-D warnings` would otherwise fail on the many `.unwrap()` calls in test bodies.

4. Used port 55433 (not 55432) and the scratchpad directory for `KEEPPIX_DATA_DIR` during the Step 10 smoke test, purely to avoid clashing with anything already bound on the developer machine; not a code change.

## Concerns

None. All four required verification commands are clean, the server smoke test matches the brief's expected output exactly, and no SQL was added to `keeppix-api` — every path to `AuthContext` still goes through `Auth`/`AdminAuth`, which call `SessionRepo::authenticate` in `keeppix-db`.

## Fix round 1

Review found one Critical and one Important finding on the original implementation. Both addressed; the three original Decisions were independently verified as sound and left unchanged.

### Critical — security headers did not apply to the fallback route

`common_layers` in `crates/keeppix-api/src/lib.rs` called `.fallback(not_found)` **after** the `.layer(...)` chain. In axum 0.8, `Router::fallback` replaces the router's fallback service directly, while `.layer()` only wraps whatever fallback is already present at the time it runs. Because `.fallback()` ran last, none of the four `SetResponseHeaderLayer`s, `CompressionLayer`, or `TraceLayer` wrapped the 404 fallback — every unmatched path (including malformed/unauthenticated API requests) shipped with no CSP, no nosniff, no referrer-policy, no permissions-policy.

**Fix:** moved `.fallback(not_found)` to before the `.layer(...)` calls in `common_layers`, so the layers wrap it. Added a comment above the call explaining the ordering requirement so a future refactor doesn't silently reintroduce the bug.

### Important — no test covered the fallback's headers

`security_headers_are_present` only exercised `/health`; `unknown_api_path_returns_problem_json` checked status, content-type and body but never headers. Neither would have caught the regression.

**Fix:** factored the four header assertions out of `security_headers_are_present` into a shared helper `assert_security_headers(headers: &HeaderMap)` in `crates/keeppix-api/tests/health.rs`, and call it from both `security_headers_are_present` (covers `/health`) and `unknown_api_path_returns_problem_json` (covers the fallback 404). Removing the layers from either route now fails a test.

### Red-then-green verification

Before applying the ordering fix, restored `lib.rs` to the previous commit's (buggy) code — `.fallback(not_found)` after the layers — while keeping the new test assertions, and ran the suite:

```
$ cargo test -p keeppix-api --test health
running 3 tests
test health_returns_ok ... ok
test security_headers_are_present ... ok
test unknown_api_path_returns_problem_json ... FAILED

---- unknown_api_path_returns_problem_json stdout ----
thread 'unknown_api_path_returns_problem_json' panicked at crates/keeppix-api/tests/health.rs:16:54:
called `Option::unwrap()` on a `None` value

test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

This confirms `unknown_api_path_returns_problem_json` genuinely fails without the ordering fix (on the `x-content-type-options` header, absent from the fallback response).

Then restored the ordering fix and re-ran the full verification set:

```
$ cargo test -p keeppix-api --test health
running 3 tests
test health_returns_ok ... ok
test security_headers_are_present ... ok
test unknown_api_path_returns_problem_json ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo clippy --workspace --all-targets -- -D warnings
    Checking keeppix-api v0.1.0 (.../crates/keeppix-api)
    Checking keeppix-server v0.1.0 (.../crates/keeppix-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.77s
(no warnings/errors)

$ cargo fmt --check
(no output — clean)
```

### Covering tests

- `security_headers_are_present` — asserts the four security headers on `/health` (matched route).
- `unknown_api_path_returns_problem_json` — asserts the four security headers (via the same `assert_security_headers` helper) plus status/content-type/body on the 404 fallback route.

### Files changed

- `crates/keeppix-api/src/lib.rs` — moved `.fallback(not_found)` before the `.layer(...)` chain in `common_layers`; added explanatory comment.
- `crates/keeppix-api/tests/health.rs` — added `assert_security_headers` helper; used it in both `security_headers_are_present` and `unknown_api_path_returns_problem_json`.

### Commit

```
git commit -m "fix(api): apply security headers to the 404 fallback route"
```
