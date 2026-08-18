# Task 4 report — Assegnare una posizione

## What I implemented

- Search assignment and map pins continue to use
  `POST /api/v1/metadata/batch`. The HTTP boundary infers:
  - `user` for a coordinate with a `place_id`;
  - `map_pin` for a free coordinate, clearing any previous `place_id`.
- Added `POST /api/v1/metadata/batch/copy-location`:
  - reads the source asset through `OverrideRepo::effective`;
  - applies its effective coordinate (and effective place when present) to all
    targets as one undoable batch;
  - records `location_source = copied`;
  - returns `403 keeppix/forbidden` for a foreign source.
- Added `POST /api/v1/metadata/batch/import-gpx`:
  - accepts GPX text, asset ids, and an optional tolerance in minutes;
  - defaults to exactly five minutes;
  - records `location_source = gpx`;
  - returns RFC 9457 `keeppix/invalid-gpx` for malformed tracks.
- Added a database-free GPX parser in `keeppix-media` using the existing
  `quick-xml` dependency. It parses timestamped `trkpt` values, validates WGS84
  coordinates, sorts timestamps, linearly interpolates points inside the
  track, uses endpoint coordinates within tolerance, and never extrapolates
  motion outside the track.
- Added `keeppix-jobs::geotag`, which loads effective capture timestamps,
  invokes the pure media interpolator, and passes all matched `(asset_id,
  GeoPoint)` pairs to one database writer.
- Added repository operations that:
  - load effective timestamps in one query with visibility/edit checks;
  - write per-asset GPX points with one `UNNEST`, not one update per photo;
  - atomically update `assets.location_source`;
  - preserve one `metadata_batches` row and enqueue one sidecar sweep.
- Extended the internal undo snapshot compatibly with old JSON payloads so
  undo restores both override rows and the previous `location_source`, including
  the distinction between “no override row” and “an all-NULL override row”.
- Hardened EXIF rescans: they may update only an unset/EXIF source, so new
  `copied` and `gpx` assignments cannot later be relabelled as EXIF.
- Registered both routes in the router and OpenAPI, updated OpenAPI contract
  tests, and regenerated `docs/api/openapi.json`.
- No PMTiles region or external network access is involved in any assignment
  test or implementation path.

## What I tested and results

All requested verification commands passed:

```text
cargo fmt --check
  PASS

cargo clippy -p keeppix-media -p keeppix-jobs -p keeppix-db -p keeppix-api --all-targets -- -D warnings
  PASS

cargo test -p keeppix-media --jobs 1 -- --test-threads=1
  PASS — 64 tests, 0 failed

cargo test -p keeppix-jobs --jobs 1 -- --test-threads=1
  PASS — 88 tests, 0 failed

cargo test -p keeppix-db --jobs 1 -- --test-threads=1
  PASS — every test binary green, including 21 override tests and 16 asset tests

cargo test -p keeppix-api --jobs 1 -- --test-threads=1
  PASS — 151 tests, 0 failed
```

Focused Task 4 coverage:

- search assignment writes coordinate, `place_id`, and `user`;
- explicit user assignment overwrites an EXIF source;
- undo restores the EXIF coordinate and source;
- map pin writes `map_pin` and no place;
- copy uses the source’s effective coordinate and writes `copied`;
- foreign copy source returns 403;
- GPX midpoint is linearly interpolated;
- assets two minutes before/after use endpoint coordinates;
- an asset twenty minutes after remains untagged under the five-minute default;
- assigned `copied`/`gpx` coordinates survive a later EXIF rescan;
- all flows pass without mounting PMTiles.

## TDD Evidence

### RED — search assignment did not update the source

Command:

```bash
cargo test -p keeppix-api --jobs 1 --test geotag search_assignment_overwrites_exif_and_undo_restores_its_source -- --test-threads=1
```

Observed failure:

```text
test search_assignment_overwrites_exif_and_undo_restores_its_source ... FAILED
assertion `left == right` failed
  left: Some("exif")
 right: Some("user")
test result: FAILED. 0 passed; 1 failed
```

### RED — GPX module did not exist

Command:

```bash
cargo test -p keeppix-media --jobs 1 --test gpx -- --test-threads=1
```

Observed failure:

```text
error[E0432]: unresolved import `keeppix_media::gpx`
could not find `gpx` in `keeppix_media`
```

### RED — EXIF rescan overwrote the new copied/GPX sources

Command:

```bash
cargo test -p keeppix-db --jobs 1 --test assets exif_location_does_not_overwrite_any_assigned_location -- --test-threads=1
```

Observed failure excerpt:

```text
left: [("copied.jpg", Some(-34.5), Some(-58.375), Some("exif")),
       ("gpx.jpg", Some(-34.5), Some(-58.375), Some("exif")), ...]
right: [("copied.jpg", Some(-33.8), Some(151.2), Some("copied")),
        ("gpx.jpg", Some(50.0), Some(18.0), Some("gpx")), ...]
test result: FAILED. 0 passed; 1 failed
```

### GREEN

Commands and observed results:

```text
cargo test -p keeppix-media --jobs 1 --test gpx -- --test-threads=1
  2 passed; 0 failed

cargo test -p keeppix-api --jobs 1 --test geotag -- --test-threads=1
  5 passed; 0 failed

cargo test -p keeppix-db --jobs 1 --test assets exif_location_does_not_overwrite_any_assigned_location -- --test-threads=1
  1 passed; 0 failed
```

The complete crate suites listed above then passed.

## Files changed

- `.superpowers/sdd/2026-08-18-keeppix-fase-4/progress.md`
- `crates/keeppix-api/src/lib.rs`
- `crates/keeppix-api/src/openapi.rs`
- `crates/keeppix-api/src/routes/geotag.rs`
- `crates/keeppix-api/src/routes/metadata.rs`
- `crates/keeppix-api/src/routes/mod.rs`
- `crates/keeppix-api/tests/geotag.rs`
- `crates/keeppix-api/tests/openapi.rs`
- `crates/keeppix-db/src/assets.rs`
- `crates/keeppix-db/src/overrides.rs`
- `crates/keeppix-db/tests/assets.rs`
- `crates/keeppix-jobs/src/geotag.rs`
- `crates/keeppix-jobs/src/lib.rs`
- `crates/keeppix-media/src/gpx.rs`
- `crates/keeppix-media/src/lib.rs`
- `crates/keeppix-media/tests/gpx.rs`
- `docs/api/openapi.json`

## Self-review findings

- Found that Task 1’s EXIF guard protected only `user` and `map_pin`; with Task
  4 that would let a rescan overwrite/relabel `copied` and `gpx`. Added a
  failing regression test and restricted EXIF writes to unset/EXIF sources.
- Found the five-minute default duplicated at the API boundary. Reused
  `keeppix_media::gpx::DEFAULT_TOLERANCE` so the tested value cannot drift.
- Corrected undo snapshot comments after changing the internal representation.
- Confirmed SQL remains entirely in `keeppix-db`, all new user-data repository
  methods take `AuthContext` first, GPX uses one `UNNEST`, and no production
  `unwrap`/`expect` was introduced.
- Confirmed router/OpenAPI paths, security declarations, operation ids, and the
  committed snapshot all agree.

## Concerns

None.

## Commits

- `3fef1dd feat(api): assign locations by search, pin, copy, and GPX import`
- `8e56f9e docs(sdd): record Task 4 complete`

## Review fix round — 2026-08-18

### Finding 1 RED — non-location undo erased a later EXIF source

Command:

```bash
cargo test -p keeppix-db --jobs 1 --test overrides undoing_a_title_batch_does_not_restore_location_source -- --test-threads=1
```

Observed failure before the fix:

```text
test undoing_a_title_batch_does_not_restore_location_source ... FAILED
assertion `left == right` failed: undoing a non-location batch must not overwrite a later EXIF source
  left: None
 right: Some("exif")
test result: FAILED. 0 passed; 1 failed; 21 filtered out
```

GREEN after making `load_previous` capture `location_source` only for batches
that change `location`/`place_id`:

```text
test undoing_a_title_batch_does_not_restore_location_source ... ok
test result: ok. 1 passed; 0 failed; 21 filtered out
```

### Finding 2 RED — interpolation crossed a `trkseg` gap

Command:

```bash
cargo test -p keeppix-media --jobs 1 --test gpx interpolation_does_not_cross_segment_gaps -- --test-threads=1
```

Observed failure before the fix:

```text
test interpolation_does_not_cross_segment_gaps ... FAILED
a timestamp farther than tolerance from both segment endpoints must stay untagged
test result: FAILED. 0 passed; 1 failed; 2 filtered out
```

GREEN after preserving each `trkseg` and using the nearest segment endpoint
only within tolerance:

```text
test interpolation_does_not_cross_segment_gaps ... ok
test result: ok. 1 passed; 0 failed; 2 filtered out
```

### Covering and final verification

```text
cargo test -p keeppix-media --jobs 1 --test gpx -- --test-threads=1
  PASS — 3 passed; 0 failed

cargo test -p keeppix-db --jobs 1 --test overrides -- --test-threads=1
  PASS — 22 passed; 0 failed

cargo test -p keeppix-api --jobs 1 --test geotag -- --test-threads=1
  PASS — 5 passed; 0 failed

cargo fmt --check
  PASS

cargo clippy -p keeppix-media -p keeppix-jobs -p keeppix-db -p keeppix-api --all-targets -- -D warnings
  PASS

cargo test -p keeppix-media --jobs 1 -- --test-threads=1
  PASS — 65 passed; 0 failed

cargo test -p keeppix-db --jobs 1 -- --test-threads=1
  PASS — every test binary and doc-test passed
```

Review fix implementation commit: `961546d`.
