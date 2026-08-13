# Task 6 Report — Token di sessione e segreti persistiti

## Steps performed

### Step 1 — Add dependencies

```
cargo add sha2 -p keeppix-domain
cargo add rand --features std,std_rng -p keeppix-domain
cargo add base64 -p keeppix-domain
cargo add serde_json -p keeppix-db
```

Resolved versions: `sha2 0.11.0`, `rand 0.10.2`, `base64 0.23.1`, `serde_json` (workspace, `1`).

Note: `serde_json` is added to `keeppix-db` per the brief's Step 1, but `settings.rs` (Step 9,
followed verbatim) does not import it — the query encodes the secret via SQL's
`to_jsonb($2::text)` cast rather than the `serde_json` crate. Left it as an unused dependency
per the brief's exact instructions; it compiles clean (Rust does not lint unused *dependencies*
by default, only unused *imports*), and clippy `-D warnings` confirms no warning is produced.
See Decisions.

### Step 2 — Write failing domain tests

Created `crates/keeppix-domain/src/token.rs` with only the `#[cfg(test)] mod tests` block from
the brief. Declared `pub mod token;` in `crates/keeppix-domain/src/lib.rs` so the crate would
compile far enough to show the intended failure.

### Step 3 — Confirm red phase (domain)

```
$ cargo test -p keeppix-domain token
error[E0433]: failed to resolve: use of undeclared type `SessionToken`
  --> crates/keeppix-domain/src/token.rs:13:20
...
error: could not compile `keeppix-domain` (lib test) due to 9 previous errors; 1 warning emitted
```

Matches the brief's expected failure (`cannot find type SessionToken in this scope`, surfaced by
`rustc` as `E0433: use of undeclared type SessionToken`).

### Step 4 — Implement `token.rs`

Implemented `SessionToken` exactly as specified, with one adaptation forced by the resolved
`rand` version — see Decisions: `use rand::Rng;` instead of `use rand::RngCore;`. Everything
else (struct shape, `generate`, `from_string`, `as_str`, `digest`, custom `Debug`) is verbatim
from the brief.

### Step 5 — Export from domain `lib.rs`

Added `pub mod token;` and `pub use token::SessionToken;` to
`crates/keeppix-domain/src/lib.rs`.

### Step 6 — Run domain tests

```
$ cargo test -p keeppix-domain
running 22 tests
test auth::tests::admin_context_reports_admin ... ok
test ids::tests::ids_are_time_ordered ... ok
test auth::tests::plain_user_context_is_not_admin ... ok
test ids::tests::id_roundtrips_through_string ... ok
test password::tests::password_accepts_ten_characters ... ok
test password::tests::malformed_hash_returns_false_without_panicking ... ok
test password::tests::password_rejects_short_input ... ok
test token::tests::debug_does_not_leak_the_secret ... ok
test token::tests::generated_tokens_are_unique ... ok
test token::tests::digest_is_stable_for_the_same_token ... ok
test token::tests::digest_differs_between_tokens ... ok
test token::tests::token_carries_at_least_256_bits ... ok
test user::tests::user_is_active_when_disabled_at_is_none ... ok
test user::tests::user_is_inactive_when_disabled_at_is_some ... ok
test user::tests::username_is_normalised_to_lowercase ... ok
test user::tests::username_rejects_invalid_characters ... ok
test user::tests::username_rejects_too_short ... ok
test user::tests::username_accepts_allowed_punctuation ... ok
test password::tests::hash_is_argon2id_with_owasp_parameters ... ok
test password::tests::hash_is_verifiable ... ok
test password::tests::hash_rejects_wrong_password ... ok
test password::tests::same_password_produces_different_hashes ... ok

test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.66s
```

22 tests, not the 20 the brief expected — the crate already had 17 tests from Tasks 1-5 before
this task added 5 (`token::tests::*`), for 22. Noted under Decisions; not a defect.

### Step 7 — Write failing db tests

Created `crates/keeppix-db/tests/settings.rs` verbatim from the brief (3 `#[tokio::test]`
functions), each annotated `#[allow(clippy::unwrap_used)]` to match the localized-lint style
used in `tests/users.rs`.

### Step 8 — Confirm red phase (db)

```
$ cargo test -p keeppix-db --test settings
error[E0432]: unresolved import `keeppix_db::SettingsRepo`
 --> crates/keeppix-db/tests/settings.rs:4:5
  |
4 | use keeppix_db::SettingsRepo;
  |     ^^^^^^^^^^^^^^^^^^^^^^^^ no `SettingsRepo` in the root

error[E0282]: type annotations needed
  --> crates/keeppix-db/tests/settings.rs:38:18
...
error: could not compile `keeppix-db` (test "settings") due to 3 previous errors
```

Matches the brief's expected failure (`unresolved import keeppix_db::SettingsRepo`). The second
error (`E0282`, `tokio::join!` can't infer types without `SettingsRepo` existing) is an expected
downstream consequence of the same missing type, not a separate concern.

### Step 9 — Implement `settings.rs`

Implemented `SettingsRepo` exactly as specified, with the same `RngCore` → `Rng` adaptation as
in `token.rs` (see Decisions). Query shape (`INSERT ... ON CONFLICT (key) DO NOTHING` followed
by a `SELECT ... WHERE key = $1` read-back) kept exactly as given — this is what makes
concurrent first-access calls converge on one secret instead of racing.

### Step 10 — Add missing db dependencies and export

```
cargo add base64 -p keeppix-db
cargo add rand --features std,std_rng -p keeppix-db
```

(Ran as two separate calls — `cargo add base64 rand --features std,std_rng -p keeppix-db` in
one invocation fails because `cargo add` requires per-dependency feature qualification like
`rand/std` when adding multiple crates in the same command; see Decisions.)

Added `pub mod settings;` and `pub use settings::SettingsRepo;` to
`crates/keeppix-db/src/lib.rs`.

### Step 11 — Run db settings tests

```
$ cargo test -p keeppix-db --test settings
running 3 tests
test different_keys_get_different_secrets ... ok
test secret_is_generated_once_and_then_stable ... ok
test concurrent_generation_yields_a_single_secret ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.96s
```

## Verification (final, in order requested)

```
$ cargo test -p keeppix-domain
running 22 tests
...
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.69s

   Doc-tests keeppix_domain
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```
$ cargo test -p keeppix-db --test settings
running 3 tests
test secret_is_generated_once_and_then_stable ... ok
test different_keys_get_different_secrets ... ok
test concurrent_generation_yields_a_single_secret ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.85s
```

```
$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 10.24s
```

(No warnings emitted; exit code 0.)

```
$ cargo fmt --check
```

(Exit code 0, no output — after running `cargo fmt` once to reformat the two long `assert_eq!`/
`assert_ne!`/`assert!` lines copied verbatim from the brief's multi-line test snippets, which
didn't match `rustfmt`'s wrapping width. `token.rs` and `settings.rs` (tests) were the two files
reformatted; the implementation files needed no reformatting.)

## Decisions

1. **`rand::RngCore` → `rand::Rng`.** The brief's snippets `use rand::RngCore;` and
   `rand::rng().fill_bytes(&mut bytes)` do not compile against the resolved `rand 0.10.2`
   (cargo picked 0.10.2, not the 0.9.x the brief's comment anticipated). In `rand` 0.10 /
   `rand_core` 0.10, `fill_bytes` moved onto the `Rng` trait; `RngCore` still exists but is now
   a marker trait (`pub trait RngCore: Rng {}`) that isn't re-exported from the `rand` crate
   root at all (`rand::RngCore` fails to resolve — confirmed with a standalone scratch build).
   Fix: `use rand::Rng;` instead of `use rand::RngCore;` in both `token.rs` and `settings.rs`.
   `rand::rng()` and `.fill_bytes(&mut bytes)` are otherwise unchanged and behave as the brief
   describes. No other part of the brief's code needed adaptation.
2. **`serde_json` added but unused in `keeppix-db`.** Followed Step 1 verbatim
   (`cargo add serde_json -p keeppix-db`), but the Step 9 implementation of
   `get_or_create_secret` (also followed verbatim) never imports `serde_json` — it round-trips
   the secret through Postgres via `to_jsonb($2::text)` / `value #>> '{}'` instead. Kept the
   dependency as instructed since the brief says "the code in the brief is exact"; it is inert
   (no warning, since Rust only lints unused *imports*, not unused *dependencies*, under the
   lint set this workspace enables).
3. **`cargo add base64 rand --features std,std_rng -p keeppix-db` (Step 10) fails as written.**
   `cargo add` rejects unqualified `--features` when adding more than one package in the same
   invocation (`feature std must be qualified by the dependency it's being activated for, like
   base64/std, rand/std`). Split into two separate `cargo add` calls instead; end state
   (`Cargo.toml` entries for both `base64` and `rand`) is identical to what the brief intends.
4. **Domain test count is 22, not the brief's stated 20.** Tasks 1-5 already left 17 tests in
   `keeppix-domain` (7 `auth`+`ids`, 6 `password`, 6 `user`... — see the "before" run implied by
   the diff); this task's 5 new `token::tests::*` bring the total to 22. Not a defect — the
   brief's "20" was presumably written against a slightly different baseline count from an
   earlier draft of Tasks 1-5.

## Concerns

None outstanding. Both crates build clean, all specified tests pass (22 domain / 3 db-settings),
`clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` are clean across the
whole workspace, and the security property under test (opaque token, one-way digest, `Debug`
never leaking the plaintext, concurrent-safe secret generation via `ON CONFLICT DO NOTHING` +
read-back) is exercised exactly as the brief's tests pin it down. Did not touch `SessionRepo`,
HTTP, or cookies — those remain Task 7/10 as instructed.

## Fix round 1

**Finding (Important):** `get_or_create_secret` in `crates/keeppix-db/src/settings.rs` mapped
both malformed-stored-secret cases (`STANDARD.decode` failure, and a decoded value that isn't
32 bytes) to `DbError::Migration`. The brief's Step 9 specified `Migration` and was followed
correctly at the time, but Task 5's review had since introduced `DbError::Corrupted(String)`
specifically for data already in the database that the code cannot parse (as opposed to a
schema migration failing), and `crates/keeppix-db/src/users.rs` (`UserRow::into_domain`)
already uses `Corrupted` for the equivalent situation. Leaving `settings.rs` on `Migration`
split the error taxonomy across two files for the same class of problem. The controller ruled
the finding wins over the brief text.

**What changed:** In `crates/keeppix-db/src/settings.rs`, `get_or_create_secret`:
- `.map_err(|e| DbError::Migration(format!("stored secret is not base64: {e}")))?` →
  `.map_err(|e| DbError::Corrupted(format!("stored secret is not base64: {e}")))?`
- `.map_err(|_| DbError::Migration("stored secret is not 32 bytes".to_owned()))` →
  `.map_err(|_| DbError::Corrupted("stored secret is not 32 bytes".to_owned()))`
- Updated the adjacent `# Errors` doc comment from `DbError::Migration` to `DbError::Corrupted`
  to keep it accurate (directly part of the same change, not a separate edit).

The `INSERT ... ON CONFLICT (key) DO NOTHING` + read-back query shape was left untouched, as
instructed. `token.rs` was not touched.

**Covering test file:** `crates/keeppix-db/tests/settings.rs` — the existing 3 tests
(`secret_is_generated_once_and_then_stable`, `different_keys_get_different_secrets`,
`concurrent_generation_yields_a_single_secret`) don't exercise the malformed-value branch
directly (no test seeds a corrupt row), so this fix is a pure variant swap covered by type-
checking and the existing happy-path tests continuing to pass; behaviour on the corrupted-data
path itself is not under test in this crate (matching the untested-error-branch pattern already
accepted for `UserRepo`'s equivalent `Corrupted` usage).

**Commands run and output:**

```
$ cargo test -p keeppix-db --test settings
   Compiling keeppix-db v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-db)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.41s
     Running tests/settings.rs (target/debug/deps/settings-4e30bf6b207d726a)

running 3 tests
test secret_is_generated_once_and_then_stable ... ok
test different_keys_get_different_secrets ... ok
test concurrent_generation_yields_a_single_secret ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.86s
```

```
$ cargo clippy -p keeppix-db --all-targets -- -D warnings
    Checking bollard-buildkit-proto v0.7.0
    Checking keeppix-db v0.1.0 (/Users/giovannimastellone/Documents/GitHub/Keeppix/crates/keeppix-db)
    Checking bollard-stubs v1.52.1-rc.29.1.3
    Checking bollard v0.20.2
    Checking testcontainers v0.27.3
    Checking testcontainers-modules v0.15.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.14s
```

Both clean, exit code 0.
