# Task 5 report — UserRepo

Branch: `fase-0`. Commit: `3f7e1ca` — `feat(db): add user repository with auth-context enforcement`.

## What was done, per step

**Step 1 — write the failing tests.**
Created `crates/keeppix-db/tests/users.rs` with the 8 tests from the brief verbatim
(logic, assertions, and structure unchanged). Two adjustments, both mechanical:
- Added `#[allow(clippy::unwrap_used)]` on the `new_user()` helper and on each
  `#[tokio::test]` function that calls `.unwrap()`/`.expect()`, matching the
  "localized allow on test functions" pattern already used in
  `crates/keeppix-db/tests/migrations.rs` and the domain crate's test modules
  (the brief's snippet omitted these attributes; the workspace lints
  `unwrap_used`/`expect_used` are `warn`, and CI runs `-D warnings`).
- Split the two `use keeppix_domain::...` imports as in the brief (kept as-is).

**Step 2 — run and confirm the failure.**
```
$ cargo test -p keeppix-db --test users
   Compiling keeppix-db v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-db)
error[E0432]: unresolved import `keeppix_db::UserRepo`
 --> crates/keeppix-db/tests/users.rs:4:5
  |
4 | use keeppix_db::UserRepo;
  |     ^^^^^^^^^^^^^^^^^^^^ no `UserRepo` in the root

For more information about this error, try `rustc --explain E0432`.
error: could not compile `keeppix-db` (test "users") due to 1 previous error
```
Matches the expected red-phase failure exactly.

**Step 3 — add `DbError::Forbidden`.**
Added to `crates/keeppix-db/src/error.rs`:
```rust
#[error("forbidden")]
Forbidden,
```

**Step 4 — implement `users.rs`.**
Created `crates/keeppix-db/src/users.rs` with the exact code from the brief
(`UserRepo`, `UserRow`, `role_str`, `map_unique_violation`, `insert_user`, and
the five methods: `new`, `count`, `create_bootstrap_admin`, `create`,
`find_by_username`, `find_by_id`). No logic changes from the brief's snippet.

**Step 5 — export from `lib.rs`.**
Added to `crates/keeppix-db/src/lib.rs`:
```rust
pub mod users;
pub use users::UserRepo;
```

**Step 6 — run the new test file.**
```
$ cargo test -p keeppix-db --test users
running 8 tests
test login_lookup_returns_none_for_unknown_user ... ok
test fresh_instance_has_no_users ... ok
test unknown_id_is_not_found ... ok
test login_lookup_returns_user_and_hash ... ok
test plain_user_can_only_read_itself ... ok
test duplicate_username_is_a_conflict ... ok
test bootstrap_admin_can_be_created_once ... ok
test only_admins_can_create_users ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 17.12s
```
All 8 passed on the first attempt after implementation — no debugging needed.

**Step 7 — full suite, clippy, fmt.**
```
$ cargo test -p keeppix-db
running 0 tests   (unittests)                       -> ok
running 4 tests   (tests/migrations.rs)              -> ok. 4 passed
running 8 tests   (tests/users.rs)                   -> ok. 8 passed
running 0 tests   (Doc-tests keeppix_db)              -> ok

$ cargo clippy -p keeppix-db --all-targets -- -D warnings
    Checking keeppix-db v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.48s
(no warnings, no errors)

$ cargo fmt --check
(diffs found on first run in src/users.rs and tests/users.rs — see Decisions;
 fixed by running `cargo fmt`; second `cargo fmt --check` run: exit 0, no output)
```
Re-ran `cargo test -p keeppix-db` after `cargo fmt` to confirm the formatting
change didn't break anything: 4 + 8 = 12 tests, all green, same as above.

Also ran `cargo check --workspace` as an extra safety check (not requested,
but cheap) to confirm the new direct `uuid` dependency and the `Forbidden`
variant don't break any other crate: finished cleanly, all seven crates
compiled.

**Step 8 — commit.**
```
git add crates/keeppix-db
git commit -m "feat(db): add user repository with auth-context enforcement" (+body)
```
Commit SHA: `3f7e1ca`.

## Final verification summary

- `cargo test -p keeppix-db --test users` — 8/8 passed.
- `cargo test -p keeppix-db` — 12/12 passed (4 migrations + 8 users).
- `cargo clippy -p keeppix-db --all-targets -- -D warnings` — clean, no warnings.
- `cargo fmt --check` — clean (after running `cargo fmt` once).

## Decisions

1. **Added `uuid` as a direct dependency of `keeppix-db`.** The brief's
   context note claimed uuid was already available in `keeppix-db` both as a
   dependency and a dev-dependency. Reading `crates/keeppix-db/Cargo.toml`
   directly showed `uuid.workspace = true` only under `[dev-dependencies]`,
   not `[dependencies]`. `src/users.rs` names `uuid::Uuid` directly as a
   `UserRow` field type, and Rust's extern prelude only exposes a crate's
   *direct* dependencies, not sqlx's transitive `uuid` dependency — so this
   would not have compiled without the addition. Added
   `uuid.workspace = true` to `[dependencies]`. Left the (now redundant)
   `[dev-dependencies]` entry in place to minimize the diff; it's harmless.
2. **Test-function `#[allow(clippy::unwrap_used)]` attributes.** The brief's
   Step 1 code block doesn't carry these, but the task instructions
   explicitly say "Test functions carry localized
   `#[allow(clippy::unwrap_used)]` / `#[allow(clippy::expect_used)]` ...
   Match that pattern," and clippy would otherwise warn (→ deny under
   `-D warnings`) on every `.unwrap()`/`.expect()` call in the test file.
   Added the attributes per-function, matching `tests/migrations.rs`.
3. **`cargo fmt` reformatted a few lines** (one `Err(...)` construction in
   `src/users.rs`, three chained `.await` calls in `tests/users.rs`) that
   exceeded the line-width in the brief's exact snippets. Ran `cargo fmt`
   once and re-verified tests/clippy still pass — required by the
   `cargo fmt --check` verification gate, and the brief's code was given as
   semantic content, not as a formatting mandate.

## Concerns

None outstanding. The let-chain in `map_unique_violation` compiled without
modification as expected under Rust 1.88.0 / edition 2024. No later-task
functionality (SettingsRepo, SessionRepo, HTTP) was touched.

## Fix round 1

Addressed two Important findings from task review. Nothing else was changed.

**Finding 1 — `DbError::Migration` overloaded for row-corruption.**
Added a new variant to `crates/keeppix-db/src/error.rs`:
```rust
#[error("corrupted row: {0}")]
Corrupted(String),
```
`Migration` and its `impl From<sqlx::migrate::MigrateError>` were left
untouched. In `crates/keeppix-db/src/users.rs`, `UserRow::into_domain` now
maps both the malformed-`username` case and the unknown-`role` case to
`DbError::Corrupted(...)` instead of `DbError::Migration(...)`.

**Finding 2 — access-control tests didn't assert the right failure.**
In `crates/keeppix-db/tests/users.rs`:
- `only_admins_can_create_users`: the denied-create assertion is now
  `assert!(matches!(denied, Err(keeppix_db::DbError::Forbidden)));` (was a
  bare `.is_err()`).
- `plain_user_can_only_read_itself`: the cross-user read assertion is now
  `assert!(matches!(repo.find_by_id(&mario_ctx, admin.id).await, Err(keeppix_db::DbError::Forbidden)));`
  (was a bare `.is_err()`).
- Added a new test, `plain_user_probing_an_unknown_id_gets_forbidden_not_not_found`,
  in which a non-admin (`mario`) calls `find_by_id` with a freshly generated
  `UserId::new()` that exists nowhere, asserting
  `Err(keeppix_db::DbError::Forbidden)` — explicitly not `NotFound` — with a
  comment explaining this pins the no-existence-oracle property (a non-admin
  must not be able to distinguish "forbidden" from "doesn't exist" by error
  variant). No production logic was changed for this finding — `find_by_id`
  already checks authorization before existence, the test just now proves it.

No other files or tests were touched.

### Commands run and output

```
$ cargo fmt && cargo test -p keeppix-db --test users
   Compiling keeppix-db v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-db)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.79s
     Running tests/users.rs (target/debug/deps/users-e1edb079b36ce4cf)

running 9 tests
test login_lookup_returns_none_for_unknown_user ... ok
test fresh_instance_has_no_users ... ok
test login_lookup_returns_user_and_hash ... ok
test plain_user_probing_an_unknown_id_gets_forbidden_not_not_found ... ok
test bootstrap_admin_can_be_created_once ... ok
test plain_user_can_only_read_itself ... ok
test duplicate_username_is_a_conflict ... ok
test only_admins_can_create_users ... ok
test unknown_id_is_not_found ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.13s
```

```
$ cargo clippy -p keeppix-db --all-targets -- -D warnings
    Checking keeppix-db v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-db)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.54s
```
(no warnings, no errors)

```
$ cargo fmt --check
```
Exit code 0, no output — clean.

Test count is now 9 in `tests/users.rs` (was 8), 13 total for `keeppix-db`
(was 12), reflecting the one new test added for finding 2.

Commit: `78be6fc` — `fix(db): distinguish corrupted rows from migration failures, harden access-control tests`.
