# Task 4 report: `LibraryRepo`

**Status:** DONE  
**Branch:** `fase-1`  
**Commit:** `3506b5c` `feat(db): add library repository`

## What I implemented

`LibraryRepo` in `keeppix-db`: create (admin only, unique `root_path`), list (admin sees all, others own only), find_by_id (Forbidden before NotFound for non-admin, including unknown ids), set_status (reuses find_by_id), mark_scanned (no `AuthContext`; scanner exception).

Mapping: `#[derive(sqlx::FromRow)]` `LibraryRow` + `into_domain()` + `crate::row::corrupted` for unknown status. `COLUMNS` does **not** include `next_folder_seq`. `Vec<String>` ↔ `text[]` works with the existing sqlx `postgres` feature; `Cargo.toml` unchanged.

Harness: `seed_user(test, admin, username)` calls `UserRepo::create`.

## What you tested and test results

### Step 3 / TDD RED (tests + `seed_user` present, `LibraryRepo` not yet)

```
cargo test -p keeppix-db --test libraries -- --test-threads=1
```

FAIL as expected — compile error, not a runtime fail:

```
error[E0432]: unresolved import `keeppix_db::LibraryRepo`
 --> crates/keeppix-db/tests/libraries.rs:4:27
  |
4 | use keeppix_db::{DbError, LibraryRepo};
  |                           ^^^^^^^^^^^ no `LibraryRepo` in the root
```

### Step 6 / TDD GREEN

```
cargo test -p keeppix-db --test libraries -- --test-threads=1
```

PASS: **11 passed; 0 failed** — 8 library tests + 3 harness unit tests in the same binary.

Library tests: `an_admin_creates_a_library`, `a_plain_user_cannot_create_a_library`, `two_libraries_cannot_share_a_root_path`, `a_plain_user_lists_only_its_own_libraries`, `reading_someone_elses_library_is_forbidden_not_not_found`, `probing_an_unknown_library_id_is_also_forbidden`, `going_offline_never_deletes_anything`, `mark_scanned_records_the_time`.

### Crate + workspace

```
cargo test -p keeppix-db -- --test-threads=1
```

PASS (all keeppix-db binaries).

```
cargo test --workspace -- --test-threads=1
```

First run: FAIL on unrelated `keeppix-db` `users::unknown_id_is_not_found` — testcontainers `PortNotExposed { port: Tcp(5432) }` (infra flake, not LibraryRepo). Retry: PASS. `keeppix-server` did not fail for missing `frontend/dist`.

```
cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check
```

PASS after rustfmt and a clippy-driven match-arm merge (see deviations).

## TDD Evidence

- **RED:** `cargo test -p keeppix-db --test libraries -- --test-threads=1` → compile FAIL, `unresolved import keeppix_db::LibraryRepo`.
- **GREEN:** same command → **11 passed** (8 library + 3 harness). Re-run after clippy match-arm merge: **11 passed**.

## Files changed

- Create: `crates/keeppix-db/src/libraries.rs`
- Create: `crates/keeppix-db/tests/libraries.rs`
- Modify: `crates/keeppix-db/src/lib.rs` (`pub mod libraries;` + `pub use libraries::LibraryRepo;`)
- Modify: `crates/keeppix-db/tests/harness/mod.rs` (`seed_user`)

Code only (`crates/keeppix-db`). `.superpowers/` not committed. Migration 0004 untouched. No FolderRepo.

## Deviations from the brief (required to compile / clippy)

1. `list`: `owner_filter.map(UserId::as_uuid)` does not compile (`as_uuid` is `fn(&UserId) -> Uuid`). Used `owner_filter.map(|id| id.as_uuid())`.
2. `find_by_id`: brief’s two identical `Forbidden` arms trip `clippy::match_same_arms` under `-D warnings`. Merged to `None if ctx.is_admin() => NotFound` then `None | Some(_) => Forbidden`. Runtime is unchanged: non-admin still gets Forbidden for unknown ids (no existence oracle); admin still gets NotFound.
3. rustfmt rewrapped long signatures and test lines so `cargo fmt --check` passes.

## Controller rulings applied

- `format!` interpolates only constant `COLUMNS`; data goes through `bind`.
- `mark_scanned` has no `AuthContext` (documented fourth exception).
- Forbidden before NotFound for non-admin, including unknown ids.
- `seed_user(test, admin, username)` + `#[allow(clippy::expect_used, dead_code)]`.
- Export as specified.
- No FolderRepo; no migration 0004 change.
- No Cargo.toml change for `text[]`.

## Self-review findings

- `Library` fields match the domain type; `next_folder_seq` is not mapped.
- Create is admin-only; duplicate `root_path` maps unique violation `23505` to `Conflict`.
- List filters by `owner_id` unless admin (`$1::uuid IS NULL OR owner_id = $1`).
- find_by_id: owner or admin sees the row; anyone else, including probes of missing ids, gets Forbidden; only admin + missing id is NotFound.
- set_status reuses find_by_id (same visibility); Offline does not delete `root_path`.
- mark_scanned writes `last_scan_at = now()` without AuthContext. Missing id is still `Ok(())` (brief does not require NotFound).
- Default insert leaves `scan_enabled = true`, `status = 'active'`, `last_scan_at` null (schema defaults).

## Issues or concerns

Non-blocking: one workspace run hit a testcontainers `PortNotExposed` flake on an existing users test; retry was green. Not caused by this task.
