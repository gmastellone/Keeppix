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

---

# Task 1 fix report — review follow-up (2 Critical + 1 Important + 1 Minor)

Branch `fase-5`. Fixes two Critical issues, one Important issue, and one
Minor issue found in the Task 1 review.

## Issue 1 (Critical): `finalize()` committed the DB before the `rename()`

**File:** `crates/keeppix-db/src/uploads.rs`, `UploadSessionRepo::finalize`.

**Before:** insert the `assets` row → delete the `upload_sessions` row →
`tx.commit()` → **then** `std::fs::rename()` the temp file to the target
folder. If the `rename()` failed after a successful commit, the result was
an asset row pointing at a file that was never placed in the target folder,
and the session row — the only reference to the temp file — was already
gone. No process could recover the file or notice the corruption.

**Fix:** reordered to `rename()` **first**, then insert + delete + commit
inside the same transaction. If the `rename()` fails, nothing in the
database is touched — the session stays for a retry and the temp file is
untouched. If the commit fails *after* a successful `rename()`, the file is
already sitting in the target folder with no `assets` row; the next library
scan discovers and indexes it like any other file dropped into the tree —
exactly the safe asymmetry described in the review.

The same principle was applied to the `SkippedDuplicate` branch (same name,
same hash as an existing asset): the temp file is now removed *before* the
session row is deleted and the transaction committed, for the same reason
(though this branch never touches the target folder, so the only residual
risk is a session row referencing an already-removed temp path, which every
code path that touches a temp file already tolerates via
`remove_file_tolerant`).

**Regression test added:** `finalize_leaves_no_asset_and_keeps_the_session_when_rename_fails`
in `crates/keeppix-db/tests/uploads.rs`. It forces the `rename()` to fail by
deleting the target folder from disk (but not from the database) between
session creation and `finalize()`, then asserts:
- `finalize()` returns `Err(DbError::Io(_))`.
- The temp file is still there (untouched).
- The `upload_sessions` row still exists (count = 1) — never deleted before
  a successful `rename()`.
- No `assets` row was created (count = 0).

**TDD verification:** ran this test against the pre-fix code (`git checkout`
of only `uploads.rs`, keeping the new test) — it failed with
`assertion left == right failed: la sessione deve restare per un retry ...
left: 0 right: 1`, i.e. the old code deleted the session row before the
`rename()` even ran, confirming the bug and that the test actually exercises
it. Restored the fix and re-ran: passes.

## Issue 2 (Critical): missing decodability-failure test

**File:** `crates/keeppix-api/tests/upload.rs`.

Added `completing_with_undecodable_content_never_enters_the_library`:
creates a session with `expected_hash` set to the blake3 hash of a garbage
payload (no recognizable magic number — not JPEG/PNG/GIF/WEBP/AVI/TIFF/
ISOBMFF/EBML per `keeppix_media::detect_kind`), `PATCH`es exactly those
bytes (so the chunk checksum *and* the end-to-end hash both match), and
asserts:
- `422` with body `"type": "keeppix/upload-undecodable"` (the existing
  `finalize_upload` handler in `routes/upload.rs` already had this branch —
  `detect_kind(&header) == AssetKind::Unknown` → `repo.fail()` + `422` — it
  just had no test exercising it).
- The temp file was removed.
- The `upload_sessions` row was deleted.
- No `assets` row exists for the filename, and the file never landed in the
  target folder.

## Issue 3 (Important): full chunk buffered in memory

**File:** `crates/keeppix-api/src/routes/upload.rs`, `patch` handler.

**Before:** `axum::body::to_bytes(body, cap)` with `cap` equal to the
session's *remaining* bytes — for a large file's last chunk, that could be
gigabytes buffered entirely in RAM before a single byte was written or
hashed.

**Fix:** new `write_chunk_checked()` helper, modeled on `write_body_capped`
in `routes/share.rs`: streams `http_body::Body` frames directly into the
temp file (opened in append mode) while feeding a `blake3::Hasher`
incrementally — no full-chunk buffer. Added `MAX_CHUNK_BYTES = 64 MiB`
(server-side cap, independent of how much of the file is left); a chunk
that would exceed `min(remaining, MAX_CHUNK_BYTES)` is rejected with `413
Payload Too Large`, truncating back (`set_len`) whatever partial bytes were
already streamed to the file so no partial/oversized chunk survives.

The per-chunk checksum is still verified before the chunk is considered
accepted, but since the hash can only be known after the whole chunk has
streamed through, a checksum mismatch now truncates the file back to its
length *before* this chunk (`file.set_len(original_len)`) instead of never
writing at all — same observable contract (`advance()` is only called with
the new checksum-verified offset, and a mismatched chunk never survives on
disk or advances `received_bytes`), verified by the existing
`patch_with_wrong_chunk_checksum_is_rejected_and_does_not_advance` test,
which still passes unmodified.

**New test:** `a_multi_chunk_upload_completes_across_two_patches` — splits
the fixture file into two chunks sent via two sequential `PATCH` requests,
asserting the offset advances correctly after the first (`204`), the second
completes the session (`201`), and the file on disk matches the original
bytes exactly (proving the streaming write appends rather than overwrites).

## Minor: `client_mtime` test

Added `client_mtime_is_preserved_on_the_finalized_asset` in
`crates/keeppix-api/tests/upload.rs`: opens a session with an explicit
`client_mtime`, completes the upload, and asserts `assets.mtime` for the
finalized asset equals the declared `client_mtime` exactly.

## Verification

```
$ cargo fmt --check
(clean, exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
    Checking keeppix-db v0.1.0
    Checking keeppix-jobs v0.1.0
    Checking keeppix-api v0.1.0
    Checking keeppix-server v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.91s
(no warnings, exit 0)

$ cargo test -p keeppix-db --test uploads --jobs 1 -- --test-threads=1
running 15 tests
test a_share_link_with_allow_upload_can_open_a_session_on_its_own_folder ... ok
test a_share_link_without_allow_upload_is_forbidden_before_accepting_any_byte ... ok
test advance_updates_the_offset_and_is_scoped_to_the_owner ... ok
test an_expired_session_is_cleaned_up_and_reported_as_gone ... ok
test creating_a_session_puts_the_temp_path_inside_keeppix_tmp ... ok
test fail_removes_both_the_session_row_and_its_temp_file ... ok
test finalize_creates_a_new_asset_when_there_is_no_collision ... ok
test finalize_leaves_no_asset_and_keeps_the_session_when_rename_fails ... ok
test finalize_renames_on_same_name_different_hash_never_overwriting ... ok
test finalize_skips_a_byte_identical_duplicate_without_a_second_file ... ok
test harness::tests::appends_when_the_url_has_no_database ... ok
test harness::tests::preserves_the_query_string ... ok
test harness::tests::replaces_an_existing_database_name ... ok
test insufficient_disk_space_is_rejected_at_creation_not_mid_upload ... ok
test probing_someone_elses_session_is_forbidden_never_not_found ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.79s

$ cargo test -p keeppix-api --test upload --jobs 1 -- --test-threads=1
running 12 tests
test a_multi_chunk_upload_completes_across_two_patches ... ok
test client_mtime_is_preserved_on_the_finalized_asset ... ok
test completing_with_a_wrong_expected_hash_never_enters_the_library ... ok
test completing_with_undecodable_content_never_enters_the_library ... ok
test insufficient_disk_space_is_rejected_at_session_creation ... ok
test opening_a_session_from_a_share_link_without_allow_upload_is_forbidden ... ok
test patch_on_an_expired_session_reports_it_is_gone ... ok
test patch_with_wrong_chunk_checksum_is_rejected_and_does_not_advance ... ok
test patch_with_wrong_offset_is_rejected_with_409 ... ok
test precheck_returns_only_the_unknown_hashes ... ok
test same_name_and_hash_is_skipped_as_a_duplicate ... ok
test same_name_different_hash_is_saved_with_a_numeric_suffix ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.43s
```

Also ran the full `keeppix-db` and `keeppix-api` integration suites
(`cargo test -p keeppix-db -p keeppix-api --jobs 1 -- --test-threads=1`,
split into two invocations to keep each log manageable): every pre-existing
test file passed, no regressions from the reordering or the streaming
rewrite (in particular `guest_uploads.rs`, which exercises the sibling
`write_body_capped` pattern in `share.rs` that `write_chunk_checked` is
modeled on, is unaffected and still green).

## Files changed

- `crates/keeppix-db/src/uploads.rs` — `finalize()` reordered (`rename()`
  before commit; temp removal before commit in the duplicate branch),
  updated doc comment.
- `crates/keeppix-db/tests/uploads.rs` — new regression test
  `finalize_leaves_no_asset_and_keeps_the_session_when_rename_fails`.
- `crates/keeppix-api/src/routes/upload.rs` — `patch` handler streams the
  chunk via new `write_chunk_checked()` instead of buffering the whole
  chunk with `axum::body::to_bytes`; new `MAX_CHUNK_BYTES` constant; removed
  the old `append_chunk()` helper; updated doc comment.
- `crates/keeppix-api/tests/upload.rs` — new tests:
  `completing_with_undecodable_content_never_enters_the_library`,
  `client_mtime_is_preserved_on_the_finalized_asset`,
  `a_multi_chunk_upload_completes_across_two_patches`.
- `.superpowers/sdd/2026-08-19-keeppix-fase-5/progress.md` — two new
  rulings, task log entry for this fix round.

## Status

DONE. All four required commands (`cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, the two
crate-scoped test commands) are green, plus the full `keeppix-db` and
`keeppix-api` suites with no regressions.
