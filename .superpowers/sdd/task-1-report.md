# Task 1 report — Sessioni di upload tus (schema e protocollo)

Fase 5, Task 1 di 10. Branch `fase-5`, commit `ea660f9`.

## What I implemented

- **Migration** `crates/keeppix-db/migrations/0026_upload_sessions.sql`: the
  `upload_sessions` table verbatim from the brief, plus
  `upload_sessions_expires_idx` on `expires_at`.
- **Domain types** `crates/keeppix-domain/src/upload.rs`: `UploadOwner`
  (`User`/`ShareLink`, mirroring the `upload_sessions_one_actor` CHECK),
  `UploadSession`, `ChunkChecksum` (wraps 32 raw bytes; hex parsing stays at
  the HTTP layer), and `CollisionOutcome` (`Created` /
  `SkippedDuplicate { existing_asset_id }` / `RenamedTo(String)`). Added
  `UploadSessionId` to `ids.rs`.
- **`UploadSessionRepo`** (`crates/keeppix-db/src/uploads.rs`):
  - `create` — checks permission (owner/admin/editor for a user, or
    `allow_upload` + matching `object_id` for a share link), checks free
    disk space via `libc::statvfs` on the library's filesystem
    (`DbError::InsufficientStorage` → `507`), creates `.keeppix-tmp/` inside
    the library root, inserts the row.
  - `load_owned` — enforces `Forbidden` (never `NotFound`) for a caller that
    isn't the owner/admin, and lazily deletes-and-cleans-up any session past
    `expires_at`, returning `DbError::Gone` (→ `410`).
  - `advance` — updates `received_bytes`, scoped to the owner.
  - `fail` — deletes the session row and its temp file (tolerant of a
    missing file).
  - `finalize` — resolves the name collision (same name + same hash →
    `SkippedDuplicate`, no second file; same name + different hash →
    `name_1.ext`; otherwise `Created`), inserts the asset row, deletes the
    session row, then does the atomic `rename()` — same filesystem because
    the temp file lives under the library root.
  - Added `AssetRepo::known_hashes` (visibility-scoped, like
    `find_by_hash`) to back the pre-check.
  - New `DbError` variants: `InsufficientStorage`, `Gone`.
- **API** `crates/keeppix-api/src/routes/upload.rs`, four handlers wired in
  `lib.rs` under `/api/v1/upload...` with `DefaultBodyLimit::disable()` on
  the `HEAD`/`PATCH` route:
  - `POST /upload/check` → `{ unknown_hashes: [...] }` (see Ruling below).
  - `POST /upload` → `201` + `Location: /api/v1/upload/{id}`; `403`/`507`
    surfaced from the repo.
  - `HEAD /upload/{id}` → `Upload-Offset` header, the server's real
    `received_bytes`.
  - `PATCH /upload/{id}` → validates `Upload-Offset` (`409` on mismatch),
    reads the chunk bounded to the remaining bytes, verifies
    `Upload-Checksum: blake3 <hex>` against the chunk (`460` on mismatch,
    chunk not written, offset unchanged), `fsync`s after each append, and on
    reaching `expected_size` finalizes: blake3 of the whole temp file
    against `expected_hash` (`422` + temp deleted + session failed on
    mismatch) and `keeppix_media::detect_kind` for decodability (`422` on
    `Unknown`, same cleanup), then calls `finalize` and returns `201` with
    the collision outcome.
  - New `Problem` constructors: `insufficient_storage` (507), `gone` (410),
    `offset_mismatch` (409), `chunk_checksum_mismatch` (460, via
    `StatusCode::from_u16(460)`).

## Deviations from the brief (see ledger)

Three rulings, written to
`.superpowers/sdd/2026-08-19-keeppix-fase-5/progress.md`:

1. The pre-check response field is `unknown_hashes`, not `known_hashes` as
   illustrated — the illustration contradicts the pinned test case («12
   sconosciuti su 47 → la risposta elenca esattamente quei 12»), which
   describes returning the unknowns. New API, not yet released, so renaming
   the field doesn't break the frozen `/api/v1`.
2. CSRF still applies to `/api/v1/upload/*` in this task; the exemption is
   explicitly Task 5's job per the phase plan, despite an outdated comment
   in `csrf.rs`.
3. No `JobKind::ExtractMetadata` enqueue on finalize — that's Task 2's job
   per the plan; the finalized asset keeps the default `Discovered` status.

## Tests (TDD)

All 8 brief cases are covered, plus the decodability-failure case and the
disk-space case, split across two test files.

`crates/keeppix-db/tests/uploads.rs` (14 tests, repo-level):
`creating_a_session_puts_the_temp_path_inside_keeppix_tmp`,
`insufficient_disk_space_is_rejected_at_creation_not_mid_upload`,
`a_share_link_without_allow_upload_is_forbidden_before_accepting_any_byte`,
`a_share_link_with_allow_upload_can_open_a_session_on_its_own_folder`,
`advance_updates_the_offset_and_is_scoped_to_the_owner`,
`probing_someone_elses_session_is_forbidden_never_not_found`,
`an_expired_session_is_cleaned_up_and_reported_as_gone`,
`fail_removes_both_the_session_row_and_its_temp_file`,
`finalize_creates_a_new_asset_when_there_is_no_collision`,
`finalize_skips_a_byte_identical_duplicate_without_a_second_file`,
`finalize_renames_on_same_name_different_hash_never_overwriting` — plus 3
harness tests.

`crates/keeppix-api/tests/upload.rs` (9 tests, HTTP-level):
`precheck_returns_only_the_unknown_hashes`,
`patch_with_wrong_offset_is_rejected_with_409`,
`patch_with_wrong_chunk_checksum_is_rejected_and_does_not_advance`,
`completing_with_a_wrong_expected_hash_never_enters_the_library` (covers the
decodability/hash-mismatch cleanup path),
`same_name_and_hash_is_skipped_as_a_duplicate`,
`same_name_different_hash_is_saved_with_a_numeric_suffix`,
`opening_a_session_from_a_share_link_without_allow_upload_is_forbidden`,
`patch_on_an_expired_session_reports_it_is_gone`,
`insufficient_disk_space_is_rejected_at_session_creation`.

TDD was followed: the test files were written and run first against the
unimplemented repo/handlers (compile failures / 404s), then
`upload.rs` (domain), `uploads.rs` (db), and `routes/upload.rs` (api) were
implemented incrementally, re-running the touched test module each time
until each case went green.

### Final verification

```
$ cargo fmt --check
(clean, exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s)
(no warnings, exit 0)

$ cargo test -p keeppix-db -p keeppix-api --jobs 1 -- --test-threads=1
... (all suites pass, including)
test result: ok. 14 passed; 0 failed   -- tests/uploads.rs (keeppix-db)
test result: ok. 9 passed; 0 failed    -- tests/upload.rs (keeppix-api)
```

Also ran the full-crate suites for `keeppix-db` and `keeppix-api` in the
same invocation (all pre-existing suites: `assets`, `folders`, `libraries`,
`share`, `trash`, `timeline`, `stacks`, `users`, `sessions`, `visibility`,
etc. and the full `keeppix-api` integration test binary) — all green, no
regressions from the new migration or the `known_hashes`/`Problem`
additions.

`./scripts/test.sh` and `cargo test --workspace` were deliberately not run,
per instructions.

## Files changed

- `crates/keeppix-db/migrations/0026_upload_sessions.sql` (new)
- `crates/keeppix-domain/src/upload.rs` (new), `ids.rs`, `lib.rs`
- `crates/keeppix-db/src/uploads.rs` (new), `tests/uploads.rs` (new),
  `assets.rs` (`known_hashes`), `error.rs` (`InsufficientStorage`, `Gone`),
  `lib.rs`, `Cargo.toml` (`libc`, dev-dep `blake3`)
- `crates/keeppix-api/src/routes/upload.rs` (new), `tests/upload.rs` (new),
  `lib.rs` (route wiring + `DefaultBodyLimit::disable()`), `routes/mod.rs`,
  `routes/share.rs` (`peek_header` made `pub(crate)`), `problem.rs`,
  `Cargo.toml` (`blake3`, dropped unused `hex`)
- `Cargo.lock`
- `.superpowers/sdd/2026-08-19-keeppix-fase-5/progress.md` (ledger entries)

## Self-review findings

- No SQL outside `keeppix-db`; handlers only call repo methods.
- Every repo method reading session/asset data by id takes `&AuthContext`
  first and returns `Forbidden` (never `NotFound`) for a caller that isn't
  the owner or an admin — verified by
  `probing_someone_elses_session_is_forbidden_never_not_found` and the
  share-link-without-`allow_upload` test.
- `sqlx` used only as `sqlx::query`/`sqlx::query_as`, no `query!` macro, no
  `.sqlx/`.
- No `unwrap()`/`expect()` in production code (checked by clippy across the
  whole workspace with `-D warnings`, which includes `clippy::unwrap_used`
  where configured); test-only helpers keep the existing
  `#![allow(clippy::unwrap_used, clippy::expect_used)]` pattern already used
  by sibling test files.
- Errors are RFC 9457 via `keeppix_api::json::Json`/`Problem`, with new
  `type` strings prefixed appropriately (`insufficient-storage`,
  `upload-session-expired`, `upload-offset-mismatch`,
  `chunk-checksum-mismatch`, `upload-hash-mismatch`, `upload-undecodable`).
- Temp files live at
  `{library_root}/.keeppix-tmp/{session_id}_{filename}`; the final rename is
  same-filesystem and atomic.
- A wrong-checksum chunk is never written and never advances
  `received_bytes` (asserted directly in the test via a follow-up `HEAD`).
- A failed finalization (bad hash or undecodable) never creates an asset row
  and never leaves a file in the library folder — asserted directly.
- Collision resolution never silently overwrites: same content is skipped
  (reported, no second file), different content gets a numeric suffix
  (`_1`), verified by reading the original file's bytes back unchanged.

## Issues or concerns

- None blocking. The `known_hashes` → `unknown_hashes` field rename and the
  two other rulings above are documented in the ledger in case a later task
  or reviewer expected the illustrated names verbatim.
- Disk-space checking uses `libc::statvfs`, which is Unix-only; there's a
  `#[cfg(not(unix))]` fallback that returns `u64::MAX` available bytes (never
  blocks). This matches the project's Linux/Docker-distroless deployment
  target but would not actually enforce the check on a hypothetical non-Unix
  build.
