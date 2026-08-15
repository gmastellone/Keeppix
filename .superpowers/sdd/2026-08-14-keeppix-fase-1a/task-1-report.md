# Task 1 report: Mapping delle righe uniforme

**Status:** DONE  
**Branch:** `fase-1`  
**Commit:** `63dd6a6` `refactor(db): map rows with sqlx::FromRow instead of by hand`

## What I implemented

Uniform row mapping for the three Fase 0 repos in `keeppix-db`:

- Private `row` module with `row::corrupted(field, detail)` producing `stored {field} is invalid: {detail}`.
- Each mapped query uses a `#[derive(sqlx::FromRow)]` struct whose field names match SQL columns, plus `into_domain()` for domain conversion (where a domain type exists).
- Call sites switched from `sqlx::query` + `try_get` to `sqlx::query_as`.

Concrete mappings:

- `UserRow` → `User`; `UserWithHashRow` (`#[sqlx(flatten)]` over `UserRow` + `password_hash`) → `(User, PasswordHash)`.
- `AuthRow` → `AuthContext`. Unknown role stays `DbError::Corrupted` via `row::corrupted` (ruling R3).
- `RotateRow` includes `now() AS db_now`. No `into_domain`: rotation is control flow over raw columns, not a domain type.
- `SecretRow` (`value` column) → `[u8; 32]`. Base64/length checks kept; existing `DbError::Corrupted` messages kept (ruling 3 — those strings are more specific than stretching `row::corrupted` onto a non-domain `value`).

**`#[sqlx(flatten)]`:** kept. sqlx 0.8.6 compiled and decoded `UserWithHashRow` correctly (`login_lookup_returns_user_and_hash` passed). Fallback not used.

Behaviour unchanged except Corrupted *message text* for invalid username/role, as required by ruling 2. No test files modified. No sqlx features added.

## What you tested and test results

### Step 1 / TDD RED (baseline, before any code change)

```
cargo test -p keeppix-db -- --test-threads=1
```

PASS. **41 tests** (same count required at the end):

| Binary | Passed |
|---|---|
| unittests `src/lib.rs` | 1 |
| `tests/migrations.rs` | 8 |
| `tests/sessions.rs` | 14 |
| `tests/settings.rs` | 6 |
| `tests/users.rs` | 12 |
| doc-tests | 0 |
| **Total** | **41** |

### Step 4 (users only, after `UserRow` / `UserWithHashRow`)

```
cargo test -p keeppix-db --test users -- --test-threads=1
```

PASS: `12 passed; 0 failed` (same as baseline users binary). Flatten exercised by `login_lookup_returns_user_and_hash`.

### Step 7 / TDD GREEN (after full implementation)

```
cargo test -p keeppix-db -- --test-threads=1
```

PASS. **41 tests**, identical split to Step 1 (1 + 8 + 14 + 6 + 12 + 0 docs).

```
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Both PASS (`frontend/dist` already present; no `npm` build needed). Postgres via testcontainers (`postgis/postgis:17-3.5`); `KEEPPIX_TEST_DATABASE_URL` unset.

## TDD Evidence

- **RED/baseline:** `cargo test -p keeppix-db -- --test-threads=1` → **41 passed** (characterization suite; no new tests — ruling 4).
- **GREEN:** same command after implementation → **41 passed**, same binaries and counts.

## Files changed

- Create: `crates/keeppix-db/src/row.rs`
- Modify: `crates/keeppix-db/src/lib.rs` (`mod row;` private)
- Modify: `crates/keeppix-db/src/users.rs`
- Modify: `crates/keeppix-db/src/sessions.rs`
- Modify: `crates/keeppix-db/src/settings.rs`

Code only. `.superpowers/` and docs not committed.

## Self-review findings

- Convention is copyable for LibraryRepo/FolderRepo/AssetRepo: `FromRow` fields = column names, `into_domain` validates, `row::corrupted` for domain-rejected stored values.
- `UserWithHashRow::into_domain` reuses `UserRow::into_domain` for username/role checks (no duplicated validation).
- Role match remains duplicated between `users.rs` and `sessions.rs` (pre-existing; no new helper — YAGNI).
- `RotateRow` correctly has no `into_domain`.
- `SecretRow` uses `into_domain` for the byte conversion but not `row::corrupted`, per ruling 3.
- `find_by_username` uses `Option::map` + `transpose` instead of an explicit `let Some`; equivalent, clippy-clean.
- Error strings `unknown role: {other}` / `stored username is invalid: {e}` now go through the helper. The API test that builds `DbError::Corrupted("unknown role: root")` itself is unaffected.

## Issues or concerns

None blocking. Flatten was **not** abandoned.

Minor, non-blocking: unknown-role / invalid-username `Display` text changed to `stored {field} is invalid: {detail}`. No keeppix-db test asserts those strings. Settings secret messages were left as-is.
