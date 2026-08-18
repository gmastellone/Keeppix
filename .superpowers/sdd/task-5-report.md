# Task 5 report — Correzione del fuso orario

## Status

DONE. Task 5 is implemented on `fase-4` in commit `9f7b481`.

## What I implemented

- Added schema-only migration `0021_tz_boundaries.sql` with the PostGIS
  `geography(MultiPolygon, 4326)` catalog and GiST index.
- Extended `GeoRepo` with:
  - `timezone_for(GeoPoint)`, using `ST_Contains`, deterministic ordering, and
    `LIMIT 1` so a boundary/overlap never produces a multiple-row error;
  - `timezone_changes(AuthContext, LibraryId)`, which reads the effective GPS
    coordinate, interprets the immutable UTC-stored clock face in the matched
    IANA timezone, and returns candidate UTC corrections without writing;
  - transactional, idempotent startup seeding from normalized TSV. A missing
    file is a no-op; a malformed present file fails atomically.
- Extended `OverrideRepo` with one parameterized per-asset timestamp writer.
  It records one existing `metadata_batches` undo snapshot, preserves unrelated
  override fields, queues the existing sidecar sweep, and creates no batch for
  an empty assignment.
- Added `RecalculateTimezones::preview` and `apply` orchestration in
  `keeppix-jobs`. Preview writes nothing; apply recalculates at confirmation
  time and uses the existing undo mechanism.
- Added authenticated endpoints:
  - `POST /api/v1/metadata/batch/recalculate-timezones/preview`
  - `POST /api/v1/metadata/batch/recalculate-timezones`
- Both endpoints use `Auth` and `keeppix_api::Json`; foreign library ids return
  RFC 9457 `403 application/problem+json` with type `keeppix/forbidden`.
- Registered both operations and schemas in OpenAPI and regenerated
  `docs/api/openapi.json`.
- Added an offline dataset build pipeline pinned to the real
  timezone-boundary-builder `2026c` release:
  - Debian build stage downloads and normalizes the source archive;
  - Python converts Polygon/MultiPolygon features to simplified MultiPolygon
    GeoJSON rows in TSV;
  - only the normalized artifact enters the distroless runtime;
  - server startup seeds it after migrations.
- Did not implement PMTiles, MapLibre, home geofence, or a duplicate manual
  no-GPS shift endpoint.

## Semantics

- A reflex timestamp such as `2026-08-18T14:00:00Z` at Tokyo is treated as the
  naive local clock face `14:00` and corrected to `05:00Z`.
- Effective location is `COALESCE(asset_overrides.location, assets.location)`.
- Assets without effective GPS, without a containing timezone polygon, without
  a capture timestamp, or with an existing `taken_at` override are skipped.
- The original asset timestamp is immutable. The correction is an override, so
  a second apply finds no candidates and creates no empty batch.

## TDD evidence

### RED

The tests were written before their corresponding implementations and produced
the expected failures:

```text
keeppix-db migration test:
  expected `tz_boundaries` was absent before 0021_tz_boundaries.sql

keeppix-db geo tests:
  error[E0599]: no method named `timezone_for` found for `GeoRepo`
  error[E0599]: no method named `seed_timezones_from_csv_if_empty`

keeppix-jobs timezone tests:
  error[E0432]: unresolved import
  `keeppix_jobs::geotag::RecalculateTimezones`

keeppix-api timezone tests:
  preview/apply requests returned 404 before route registration

OpenAPI contract:
  operation count and committed snapshot differed after adding the routes

offline normalizer test:
  scripts/build-tz-boundaries.sh did not exist
```

### GREEN

Focused coverage now verifies:

- Tokyo lookup and open-ocean no-match;
- a shared-edge point returns zero or one timezone without error;
- one-time seed, missing-file no-op, and corrupt-file transaction rollback;
- preview count/example and zero writes;
- Tokyo conversion from `14:00Z` to `05:00Z`;
- one-batch apply, undo restoration, and idempotent second apply;
- no-GPS, ocean, and existing user override remain unchanged;
- foreign library preview/apply are forbidden at jobs and HTTP layers;
- API payloads, RFC 9457 response, OpenAPI mounting, and snapshot;
- fixture-driven Polygon/MultiPolygon normalization with a fake `curl` that
  fails if the script attempts network access.

## Verification

Fresh verification after implementation commit:

```text
cargo fmt --check
  PASS

./scripts/test-build-tz-boundaries.sh
  PASS — 2 zones normalized, no network

cargo clippy -p keeppix-db -p keeppix-jobs -p keeppix-api \
  -p keeppix-server --all-targets -- -D warnings
  PASS

cargo test -p keeppix-db --jobs 1 -- --test-threads=1
cargo test -p keeppix-jobs --jobs 1 -- --test-threads=1
cargo test -p keeppix-api --jobs 1 -- --test-threads=1
  PASS — every test binary and doc-test completed with zero failures
```

`./scripts/test.sh` was not run, as requested.

## Files changed

- `Dockerfile`
- `crates/keeppix-db/migrations/0021_tz_boundaries.sql`
- `crates/keeppix-db/src/geo.rs`
- `crates/keeppix-db/src/lib.rs`
- `crates/keeppix-db/src/overrides.rs`
- `crates/keeppix-db/tests/geo.rs`
- `crates/keeppix-db/tests/migrations.rs`
- `crates/keeppix-jobs/src/geotag.rs`
- `crates/keeppix-jobs/tests/timezones.rs`
- `crates/keeppix-api/src/lib.rs`
- `crates/keeppix-api/src/openapi.rs`
- `crates/keeppix-api/src/routes/metadata.rs`
- `crates/keeppix-api/tests/openapi.rs`
- `crates/keeppix-api/tests/timezones.rs`
- `crates/keeppix-server/src/main.rs`
- `docs/api/openapi.json`
- `scripts/build-tz-boundaries.py`
- `scripts/build-tz-boundaries.sh`
- `scripts/test-build-tz-boundaries.sh`
- `.superpowers/sdd/2026-08-18-keeppix-fase-4/progress.md`
- `.superpowers/sdd/task-5-report.md`

## Self-review

- Confirmed all production SQL is inside `keeppix-db` and uses function-form,
  parameterized sqlx calls.
- Confirmed user-data reads take `AuthContext`; timezone catalog lookup/seeding
  are documented global bootstrap exceptions.
- Confirmed no production `unwrap()` or `expect()` was introduced.
- Confirmed HTTP obtains context only through `Auth` and uses the custom JSON
  extractor.
- Confirmed foreign ids cannot become a not-found existence oracle.
- Confirmed `metadata_batches` and `undo_batch` are reused; no undo table or
  migration was added.
- Confirmed the runtime image performs no download and contains no downloader.
- Confirmed the routes, operation ids, schemas, security declarations,
  operation count, and committed OpenAPI snapshot agree.
- Confirmed the pinned `2026c` GitHub release and `timezones.geojson.zip` asset
  exist.

## Concerns

No blocking concerns. Per task instructions, the 48.9 MB production archive was
not downloaded/normalized and the complete production Docker image was not
built; the network-denying fixture path was exercised instead. Startup import,
PostGIS conversion, and rollback behavior are covered with small fixtures in
Testcontainers.

## Commits

- `9f7b481 feat(jobs): correct capture timestamps from GPS timezone boundaries`

## Review fix round

Commit `e9ae7f4` closes the four Important findings:

- apply computes candidates and writes them in one transaction; the upsert
  rechecks `asset_overrides.taken_at IS NULL` and records only rows actually
  changed, so an existing/concurrent user override is preserved;
- seed validates each parameterized batch against `pg_timezone_names`;
- both lookup paths use bare-geography `&&` plus `ST_Covers`, preserving
  `ORDER BY tz_name LIMIT 1`;
- preview asks the repository for a windowed count and one example instead of
  collecting every candidate in `keeppix-jobs`.

The existing after-commit `enqueue_sidecar_sweep` pattern is intentionally
unchanged and still matches `apply_batch`.

### Review RED

```text
cargo test -p keeppix-db --test overrides \
  timezone_writer_preserves_a_taken_at_override_present_at_write_time \
  --jobs 1 -- --exact --test-threads=1
  FAIL — returned Some(batch) instead of skipping the write-time override

cargo test -p keeppix-db --test geo \
  timezone_seed_rejects_an_unknown_iana_name_atomically \
  --jobs 1 -- --exact --test-threads=1
  FAIL — Mars/Olympus_Mons was accepted

cargo test -p keeppix-db --lib \
  timezone_match_keeps_the_geography_column_bare_for_gist \
  --jobs 1 -- --exact --test-threads=1
  FAIL — timezone_match_sql did not exist; lookup still cast boundary::geometry
```

### Review GREEN and verification

```text
All three focused regressions: PASS
cargo fmt --check: PASS
cargo clippy -p keeppix-db -p keeppix-jobs -p keeppix-api \
  --all-targets -- -D warnings: PASS
cargo test -p keeppix-db --jobs 1 -- --test-threads=1: PASS
cargo test -p keeppix-jobs --jobs 1 -- --test-threads=1: PASS
cargo test -p keeppix-api --jobs 1 -- --test-threads=1: PASS
```

`./scripts/test.sh` was not run, as requested. PMTiles, MapLibre, and geofence
were not implemented or modified.
