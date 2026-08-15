### Spec Compliance

**Verdict: met.**

- ✅ `crates/keeppix-db/src/row.rs` created; `row::corrupted` is `pub(crate)` and formats `stored {field} is invalid: {detail}` (`row.rs:14-16`). Module docs match the brief.
- ✅ `mod row;` is private in `lib.rs:4` — not `pub`, not re-exported.
- ✅ `UserRow` is `#[derive(sqlx::FromRow)]` with field names equal to columns, plus `into_domain` via `row::corrupted` for username/role (`users.rs:8-48`). Matches the brief snippet.
- ✅ `UserWithHashRow` uses `#[sqlx(flatten)]` over `UserRow` plus `password_hash`, with `into_domain` reusing `UserRow::into_domain` (`users.rs:50-64`).
- ✅ Every former hand-built `UserRow` / `try_get` site now uses `sqlx::query_as`: `find_by_username` (`users.rs:145-157`), `find_by_id` (`users.rs:165-175`), `insert_user` RETURNING (`users.rs:181-194`). Callers of `insert_user` still run `into_domain` (`users.rs:118-120`, `users.rs:131-133`).
- ✅ `AuthRow` + `into_domain` → `AuthContext`; unknown role is `DbError::Corrupted` via `row::corrupted` (ruling R3) (`sessions.rs:10-33`, `sessions.rs:84-95`).
- ✅ `RotateRow` includes `now() AS db_now` and has no `into_domain` — rotation stays control flow over raw columns (`sessions.rs:35-44`, `sessions.rs:115-166`).
- ✅ `SecretRow` is `FromRow` + `into_domain` (`settings.rs:11-25`, `settings.rs:54-60`). Secret decode errors still construct `DbError::Corrupted` by hand with the pre-existing strings (`settings.rs:20-23`), not `row::corrupted`. That is a local exception, not a missed mapping: the brief’s Step 5 example does not rewrite those messages, and the helper is specified for domain-rejected stored values.
- ✅ sqlx function forms only (`query` / `query_as`); no `query!`, no `.sqlx/`, no `SQLX_OFFLINE` in this diff. `use sqlx::Row` removed from all three repos.
- ✅ No new `AuthContext`-less repository reads. The Fase 0 trio (`count`, `create_bootstrap_admin`, `find_by_username`) is unchanged.
- ✅ Queries remain parameterized. SQL still lives only in `keeppix-db`.
- ✅ No `unwrap`/`expect` in the production diff. No test files modified (brief: behaviour identical, tests untouched).
- ✅ Single conventional-commit message matches Step 8 exactly: `refactor(db): map rows with sqlx::FromRow instead of by hand`.
- ✅ Files touched are exactly the brief’s list (create `row.rs`; modify `users.rs`, `sessions.rs`, `settings.rs`, `lib.rs`).

⚠️ Cannot verify from diff (accepted from the report, not re-run): Step 1/7 test count 41→41; `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`. Global `cargo test --workspace` was not in Step 7; the only API pin on a `Corrupted` string (`keeppix-api/src/extract.rs:110`) constructs `DbError::Corrupted("unknown role: root")` itself and does not assert the helper’s new `stored role is invalid: …` text. Edition 2024 / toolchain 1.88.0 are outside this diff.

**Missing / Extra / Misunderstood:** none that fail the brief. `AuthRow` / `SecretRow` / `UserWithHashRow::into_domain` are implied by “stesso schema”, not extras. Role-match duplication between `users.rs` and `sessions.rs` is pre-existing and not in scope.

### Strengths

- The diff is a mechanical, copyable convention: `FromRow` fields = columns, `into_domain` validates, `row::corrupted` for domain-rejected values. Later repos can follow `UserRow` without inventing a second style.
- `query_as` replaces every `try_get` mapping; INSERT/UPDATE/LOCK that return no domain row correctly stay on `sqlx::query`.
- `UserWithHashRow` does not duplicate username/role checks. Flatten is the brief’s prescribed shape, not a detour.
- `RotateRow` correctly refuses `into_domain` — `id` / `family_id` / `db_now` are transaction control, not a domain type.
- Unknown-role taxonomy in `authenticate` is preserved (variant `Corrupted`), only the message text changes to the helper form, as the brief’s `UserRow` sample requires.
- No Cargo.toml, no new sqlx features, no test churn, no docs in the commit.

### Issues

#### Critical

None.

#### Important

None.

#### Minor

None that fail the gate. Noted for later copy-paste, not a fix: `SecretRow::into_domain` (`settings.rs:20-23`) still builds `DbError::Corrupted` by hand. The `row` module comment says to always use the helper; the secret strings (`stored secret is not base64` / `not 32 bytes`) are format checks on opaque bytes, not a domain parse, and the brief did not ask to restyle them. Future tables should still go through `row::corrupted` for values the domain rejects.

**Outside-diff check:** `insert_user` callers still call `into_domain` (risk: RETURNING mapped to `UserRow` but converted twice or skipped). They do not skip it. Remaining `sessions.rs` methods (`revoke`, `purge_expired`) are execute-only; no leftover `try_get`. No `unwrap`/`expect` under `keeppix-db/src`.

### Assessment

**Task quality:** Approved

**Reasoning:** The mapping convention the rest of Fase 1a will copy is in place, private, and applied to every Fase 0 row-read. Behaviour is unchanged except the mandated Corrupted message shape for username/role; tests were left alone as required.
