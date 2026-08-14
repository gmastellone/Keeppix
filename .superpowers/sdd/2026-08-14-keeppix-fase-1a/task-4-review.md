# Task 4 review: `LibraryRepo`

**Base:** `0713b8e`  
**Head:** `3506b5c` `feat(db): add library repository`  
**Verdict:** Approved

## Spec Compliance

**Compliant.** The diff is the brief’s `LibraryRepo` plus the two documented compile/clippy fixes. Files, methods, auth rules, mapping, SQL discipline, and commit message match.

Checked against the produce list and global constraints:

| Required | In diff |
|---|---|
| `LibraryRepo::new`, `create`, `list`, `find_by_id`, `set_status`, `mark_scanned` | Yes |
| `create` admin-only; `Conflict` on duplicate `root_path` (`23505`) | Yes |
| `list`: non-admin own libraries only; admin all | Yes (`$1::uuid IS NULL OR owner_id = $1`) |
| `find_by_id`: Forbidden before NotFound for non-admin, including unknown ids; admin NotFound on missing | Yes (clippy-merged arms, same runtime) |
| `set_status` reuses `find_by_id`; Offline does not delete config | Yes |
| `mark_scanned` without `AuthContext`, documented as fourth/last exception | Yes |
| `AuthContext` first on every user-data method; no extra exceptions | Yes |
| SQL only in `keeppix-db`; `query` / `query_as`; `format!` only interpolates `COLUMNS` | Yes |
| `#[derive(sqlx::FromRow)]`, fields = columns, `into_domain()`, `row::corrupted` | Yes |
| No `next_folder_seq` on `LibraryRow` (not a domain field) | Yes |
| Tests: 8 brief cases + `seed_user` as specified | Yes (rustfmt wrapping only) |
| No `unwrap`/`expect` in production; localized test `#[allow]` | Yes |
| Conventional commit in English | Yes — `feat(db): add library repository` |

Accepted deviations (report; required to compile / `-D warnings`):

1. `owner_filter.map(|id| id.as_uuid())` — `as_uuid` is `&UserId -> &Uuid`, so `map(UserId::as_uuid)` does not compile.
2. `find_by_id` arms `None | Some(_) => Forbidden` after `None if ctx.is_admin() => NotFound` — equivalent to the brief’s two identical `Forbidden` arms; avoids `clippy::match_same_arms`.

⚠️ Not re-run (accepted from the report): Step 3 RED `unresolved import keeppix_db::LibraryRepo`; Step 6 **11 passed** (8 library + 3 harness unit tests in the same binary); workspace test/clippy/fmt green after rustfmt. The testcontainers flake on `users::unknown_id_is_not_found` is outside this diff.

**Missing / Extra / Misunderstood:** none that fail the brief. No FolderRepo, no migration 0004, no Cargo.toml.

## Strengths

- Implementation is the brief’s Step 4–5 code, not a parallel design. Create/list/find/status/scan behaviour, error taxonomy, and docs match.
- `find_by_id` does not leak existence: non-owner and unknown id both return `Forbidden`; `into_domain` (and thus `Corrupted`) runs only after the caller is allowed to see the row.
- Mapping follows the crate convention: `FromRow` is SQL-shaped, status is the only domain-rejected stored value, schema defaults cover `scan_enabled` / `status` / `last_scan_at`.
- `set_status` does not duplicate the visibility check. `mark_scanned` is documented at the call site the plan named.
- Tests hit the behaviours the brief named: admin create defaults, non-admin create denied, unique `root_path`, list scoping, Forbidden vs NotFound, Offline preserves `root_path`, `last_scan_at` set.

## Issues

### Critical

None.

### Important

None.

### Minor

1. **Admin unknown-id → `NotFound` is untested** — `crates/keeppix-db/src/libraries.rs` (`find_by_id`) and `crates/keeppix-db/tests/libraries.rs`. The non-admin probe test locks the existence-oracle rule; nothing asserts that an admin asking for a fresh `LibraryId` gets `NotFound`. The match arm is present and equivalent to the brief. Residual test gap only; **plan-mandated** tests did not include it.

2. **`set_status` Forbidden path is untested.** Visibility is delegated to `find_by_id` (which is tested). A non-owner `set_status` would still be Forbidden, but that call is not in the suite. Brief Step 1 did not require it.

## Assessment

**Approved.**

The task is spec-complete and well-built: one crate, one repo, parameterized SQL, AuthContext on user paths, scanner exception documented and not copied elsewhere. The compile/clippy diffs are equivalent, not behaviour changes. Residual risks that are **not** blockers: `mark_scanned` on a missing id returns `Ok(())` (brief does not require `NotFound`; report already notes this); `23505` is mapped as root-path conflict without checking the constraint name (same pattern as `UserRepo`, **plan-mandated**); PK collision would be mislabelled, which is not realistic with UUID v7.
