## Spec Compliance

**Compliant.** Migration `0004`, `schema_0004.rs`, and `seed_admin` match the brief. The only departures from the pasted SQL/tests are the controller rulings: **F1** `next_folder_seq bigint NOT NULL DEFAULT 1` is on `CREATE TABLE libraries` with no `ALTER TABLE`; **F2** every `INSERT INTO folders` supplies `depth` (`1` root, `2` children of the root) and `folders.depth` has no `DEFAULT`. Tables `libraries`, `folders`, and `folder_month_counts`, the `ltree` extension, uniqueness/GIST indexes, parameterized `sqlx::query`/`query_scalar` (no `query!`), localized clippy allows, and commit `feat(db): add libraries and the ltree folder tree` are all present.

## Strengths

- Schema is the brief SQL plus F1, including `libraries_root_path_key`, GIST `folders_path_gist`, `(library_id, path)` uniqueness, partial sibling-name uniqueness, and single-root-per-library.
- Tests hit real Postgres via `TestDb` and lock the behaviours the brief named: `ltree` enabled, owner FK, unique `root_path`, library delete cascades to folders, sibling name clash.
- `seed_admin` matches the specified helper (`create_bootstrap_admin`, `#[allow(clippy::expect_used, dead_code)]`).
- Production change is SQL only; unwrap/expect stay in tests with localized allows.

## Issues

### Critical

None.

### Important

None.

### Minor

1. **Sibling-name test also collides on `path`** — `crates/keeppix-db/tests/schema_0004.rs:121-133`
   - Both child inserts use name `'2024'` and path `'1.2'`. `folders_library_path_key` would fail the duplicate even if `folders_sibling_name_key` were missing, so the test does not isolate the named invariant.
   - Plan-inherited: the brief used the same pair of values. Not a schema bug; F2 only added `depth`. Optional lock: second insert with a distinct path (e.g. `'1.3'`).

2. **`folders_single_root_key` and `folder_month_counts` are untested** — `crates/keeppix-db/migrations/0004_libraries_folders.sql:57-68`
   - Both exist as specified. Dropping the table or the single-root index would still leave `schema_0004` green. Brief Step 1 did not require those assertions.

## Assessment

**Approved**

The schema and tests match the brief and F1/F2. Minors are coverage gaps in tests the brief prescribed, not missing objects. Task 5 should use `ON CONFLICT (parent_id, name) WHERE parent_id IS NOT NULL` because `folders_sibling_name_key` is partial.
