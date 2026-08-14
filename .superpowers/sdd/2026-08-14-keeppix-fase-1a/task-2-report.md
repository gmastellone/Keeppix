# Task 2 report: Migrazione librerie e cartelle

**Status:** DONE  
**Branch:** `fase-1`  
**Commit:** `971e5f4` `feat(db): add libraries and the ltree folder tree`

## What I implemented

Schema for libraries and the ltree folder tree, plus a shared test helper:

- Migration `0004_libraries_folders.sql`: `ltree` extension, `libraries`, `folders`, `folder_month_counts`, uniqueness/GIST indexes.
- **F1:** `next_folder_seq bigint NOT NULL DEFAULT 1` is on `CREATE TABLE libraries` (per-library ltree label counter). No `ALTER TABLE`.
- Tests in `schema_0004.rs` (5 behaviours). **F2:** every `INSERT INTO folders` includes `depth` (`1` for root, `2` for children of the root). No `DEFAULT` on the column.
- `harness::seed_admin` creates a bootstrap admin via `UserRepo::create_bootstrap_admin` and `NewUser` as currently defined (`username`, `email`, `display_name`, `password_hash`, `role`). `#[allow(clippy::expect_used, dead_code)]` as required.

No `LibraryRepo` / `FolderRepo`. `expected_tables_exist` left unchanged (subset assertion; extra tables are fine).

## What you tested and test results

### Step 3 / TDD RED (tests + `seed_admin`, no migration)

```
cargo test -p keeppix-db --test schema_0004 -- --test-threads=1
```

FAIL as expected. Key lines:

```
test a_library_requires_an_existing_owner ... ok
test deleting_a_library_removes_its_folders ... FAILED
test ltree_extension_is_enabled ... FAILED
test root_path_is_unique ... FAILED
test sibling_folders_cannot_share_a_name ... FAILED

relation "libraries" does not exist
ltree serve all'albero delle cartelle

test result: FAILED. 4 passed; 4 failed
```

`a_library_requires_an_existing_owner` is `assert!(orphan.is_err())`, so it is already true when the table is missing. The other four fail for the missing relation / missing `ltree`.

### Step 5 / TDD GREEN (after `0004`)

```
cargo test -p keeppix-db --test schema_0004 -- --test-threads=1
```

PASS: `8 passed; 0 failed` (5 schema tests + 3 harness unit tests compiled into the binary).

First GREEN run after writing the SQL did not recompile (`sqlx::migrate!` did not see the new file). `touch crates/keeppix-db/src/lib.rs` forced the rebuild; no source change, not committed.

### Step 6 / previous tests still green

```
cargo test -p keeppix-db -- --test-threads=1
```

PASS. **49 tests**:

| Binary | Passed |
|---|---|
| unittests `src/lib.rs` | 1 |
| `tests/migrations.rs` | 8 |
| `tests/schema_0004.rs` | 8 |
| `tests/sessions.rs` | 14 |
| `tests/settings.rs` | 6 |
| `tests/users.rs` | 12 |
| doc-tests | 0 |
| **Total** | **49** |

`migrations_are_idempotent`, `expected_tables_exist`, and `required_extensions_are_enabled` all passed. `ltree_extension_is_enabled` passed.

```
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Clippy PASS. `cargo fmt` applied rustfmt to `schema_0004.rs` (long assert lines); `cargo fmt --check` then PASS. `frontend/dist` already present.

Postgres via testcontainers (`postgis/postgis:17-3.5`); `KEEPPIX_TEST_DATABASE_URL` unset.

## TDD Evidence

- **RED:** `cargo test -p keeppix-db --test schema_0004 -- --test-threads=1` → **FAILED**, `relation "libraries" does not exist` (and `ltree` assertion).
- **GREEN:** same command after `0004` → **8 passed**. Full crate: **49 passed**.

## Files changed

- Create: `crates/keeppix-db/migrations/0004_libraries_folders.sql`
- Create: `crates/keeppix-db/tests/schema_0004.rs`
- Modify: `crates/keeppix-db/tests/harness/mod.rs` (`seed_admin`)

Code only. `.superpowers/` and docs not committed.

## Controller rulings applied

- **F1:** `next_folder_seq bigint NOT NULL DEFAULT 1` in `CREATE TABLE libraries`, with a short SQL comment. No `ALTER TABLE`.
- **F2:** all three folder INSERTs list `depth` (`1` root, `2` child). No `DEFAULT` on `folders.depth`.

## Self-review findings

- `NewUser` fields match `keeppix-domain` / `create_bootstrap_admin`; nothing invented.
- `seed_admin` returns `User.id` (`UserId`); callers use `as_uuid()`.
- `dead_code` allow is required: other integration binaries compile the whole harness.
- Folder INSERTs would fail after GREEN without F2 (`depth int NOT NULL`, no default).
- Indexes match the brief: `libraries_root_path_key`, sibling name unique where `parent_id IS NOT NULL`, single root per library.
- `expected_tables_exist` still lists only the Fase 0 tables; extra tables do not fail it.

## Issues or concerns

None blocking.

Minor, non-blocking: `a_library_requires_an_existing_owner` is green on RED because any INSERT error satisfies `is_err()`. After `0004` it is a real FK check. Assertions left as specified.

Minor, non-blocking: adding a migration file does not always invalidate the `sqlx::migrate!` compile. Task 5 should `touch` `keeppix-db/src/lib.rs` (or equivalent) if a follow-up SQL file is not picked up. No `build.rs` added (not requested).
