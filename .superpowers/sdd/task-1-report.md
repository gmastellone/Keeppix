# Task 1 report — EXIF GPS extraction and `LocationSource`

## What I implemented

- Added `LocationSource::as_str()` on the existing enum in
  `keeppix-domain::asset`, with the exact database CHECK values: `exif`,
  `user`, `map_pin`, `copied`, and `gpx`.
- Extended `ExifData` with `gps: Option<GeoPoint>` and reused the existing
  `keeppix_domain::GeoPoint`. `GeoPoint` now derives serde traits because
  `ExifData` is serializable.
- Parsed standard EXIF GPS latitude/longitude DMS rationals and their N/S/E/W
  references into signed decimal degrees.
- Discarded only a coordinate pair where both axes are within `1e-12` degrees
  of zero. A zero latitude or longitude with a non-zero other axis remains
  valid.
- Added `AssetRepo::set_exif_location`, a documented pipeline method without
  `AuthContext`. It uses parameterized SQL and refuses to overwrite
  `location_source = 'user'` or `'map_pin'`.
- Wired the metadata job to persist parsed EXIF GPS after writing immutable
  `asset_exif`.

## What I tested and results

- `cargo test -p keeppix-domain --jobs 1 -- --test-threads=1`
  - PASS: 47 unit tests, 0 failures; doc tests pass.
- `cargo test -p keeppix-media --jobs 1 -- --test-threads=1`
  - PASS: 60 integration tests, 0 failures; doc tests pass.
  - GPS coverage includes missing tags, S/W signs, zero-zero, one zero axis,
    and TIFF/MakerNote-only stand-in without a standard GPS IFD.
- `cargo test -p keeppix-db --test assets exif_location_does_not_overwrite_user_or_map_pin_locations --jobs 1 -- --exact --test-threads=1`
  - PASS: 1 focused database test, 0 failures.
- `cargo test -p keeppix-jobs --test metadata metadata_ --jobs 1 -- --test-threads=1`
  - PASS: 6 focused metadata tests, 0 failures.
  - The complete metadata integration binary also passed earlier: 9 tests,
    0 failures.
- `cargo fmt --check`
  - PASS.
- `cargo clippy -p keeppix-domain -p keeppix-media -p keeppix-db -p keeppix-jobs --all-targets -- -D warnings`
  - PASS.
- Per the task instruction, `./scripts/test.sh` was not run.

## TDD evidence

### `LocationSource` mapping

RED:

```text
$ cargo test -p keeppix-domain location_source_strings_match_the_database_constraint --jobs 1 -- --exact --nocapture
error[E0599]: no method named `as_str` found for enum `asset::LocationSource`
error: could not compile `keeppix-domain` (lib test) due to 5 previous errors
```

This was the expected failure because the existing enum had no database string
mapping.

GREEN:

```text
$ cargo test -p keeppix-domain asset::tests::location_source_strings_match_the_database_constraint --jobs 1 -- --exact --nocapture
running 1 test
test asset::tests::location_source_strings_match_the_database_constraint ... ok
test result: ok. 1 passed; 0 failed
```

### EXIF GPS parsing

The first RED run proved the requested `ExifData` interface was absent
(`error[E0609]: no field gps on type ExifData`). After adding only that field
with `gps: None`, the GPS behavior tests were run again before parse logic:

```text
$ cargo test -p keeppix-media --test exif_gps --jobs 1 -- --test-threads=1 --nocapture
running 5 tests
test a_zero_on_only_one_axis_is_a_valid_coordinate ... FAILED
test jpeg_without_gps_tags_has_no_gps ... ok
test south_and_west_references_make_coordinates_negative ... FAILED
test tiff_without_a_standard_gps_ifd_is_not_an_error ... ok
test zero_zero_without_a_fix_has_no_gps ... ok
test result: FAILED. 3 passed; 2 failed
```

Both failures were expected: `data.gps` was still `None`, proving the positive
fixtures exercised the missing parse behavior rather than passing accidentally.

GREEN:

```text
$ cargo test -p keeppix-media --test exif_gps --jobs 1 -- --test-threads=1 --nocapture
running 5 tests
test a_zero_on_only_one_axis_is_a_valid_coordinate ... ok
test jpeg_without_gps_tags_has_no_gps ... ok
test south_and_west_references_make_coordinates_negative ... ok
test tiff_without_a_standard_gps_ifd_is_not_an_error ... ok
test zero_zero_without_a_fix_has_no_gps ... ok
test result: ok. 5 passed; 0 failed
```

### Database guard and metadata wiring

RED repository API:

```text
$ cargo test -p keeppix-db --test assets exif_location_does_not_overwrite_user_or_map_pin_locations --jobs 1 -- --exact --test-threads=1 --nocapture
error[E0599]: no method named `set_exif_location` found for struct `AssetRepo`
```

RED metadata wiring (the pipeline call was deliberately absent for this run):

```text
$ cargo test -p keeppix-jobs --test metadata metadata_ingest_persists_standard_exif_gps --jobs 1 -- --exact --test-threads=1 --nocapture
assertion `left == right` failed
  left: (None, None, None)
 right: (Some(-34.5), Some(-58.375), Some("exif"))
test result: FAILED. 0 passed; 1 failed
```

These were the expected failures: the repository write did not exist, then the
real metadata path parsed GPS but did not persist it.

GREEN:

```text
$ cargo test -p keeppix-db --test assets exif_location_does_not_overwrite_user_or_map_pin_locations --jobs 1 -- --exact --test-threads=1
test exif_location_does_not_overwrite_user_or_map_pin_locations ... ok
test result: ok. 1 passed; 0 failed

$ cargo test -p keeppix-jobs --test metadata metadata_ --jobs 1 -- --test-threads=1
running 6 tests
test metadata_ingest_persists_standard_exif_gps ... ok
test metadata_rerun_does_not_overwrite_user_or_map_pin_locations ... ok
test result: ok. 6 passed; 0 failed
```

## Files changed

- `crates/keeppix-domain/src/asset.rs`
- `crates/keeppix-domain/src/exif.rs`
- `crates/keeppix-domain/src/overrides.rs`
- `crates/keeppix-media/src/exif.rs`
- `crates/keeppix-media/tests/exif_gps.rs`
- `crates/keeppix-db/src/assets.rs`
- `crates/keeppix-db/tests/assets.rs`
- `crates/keeppix-db/tests/search.rs`
- `crates/keeppix-jobs/src/metadata.rs`
- `crates/keeppix-jobs/tests/metadata.rs`
- `.superpowers/sdd/task-1-report.md`

## Self-review findings

- Production SQL remains exclusively in `keeppix-db`; jobs only calls the
  repository API, and media has no database dependency.
- All SQL values are bound parameters, including the three
  `LocationSource::as_str()` values.
- No production `unwrap()` or `expect()` was added.
- Every existing `ExifData` literal was updated.
- The metadata integration test proves the feature is wired into the real
  ingest path, not only reachable through an isolated public function.
- Both `user` and `map_pin` are tested through an actual metadata rerun, while
  the repository test independently pins the SQL guard.
- No new dependency or migration was added.

## Issues or concerns

None.

## Fix — spec and quality review

### What changed

- Replaced the empty-TIFF-plus-ASCII stand-in with a valid TIFF containing an
  EXIF IFD, a real MakerNote tag (`0x927C`), and a 102-byte binary MakerNote IFD
  whose private fields contain S/W GPS-like DMS rationals. The fixture has no
  standard GPS IFD pointer or standard GPS latitude/longitude fields.
- Strengthened the MakerNote test by independently parsing the fixture,
  asserting that the MakerNote binary field exists, and asserting that no
  standard GPS fields are exposed before checking that `read_exif` returns
  `gps: None` without error.
- Removed the Task 1 GPS rerun test and all of its raw SQL from
  `keeppix-jobs`. The remaining ingest-wiring test reads effective coordinates
  through `OverrideRepo`; the write-on-first-index and `user`/`map_pin`
  no-overwrite cases remain covered together in `keeppix-db/tests/assets.rs`.

### Covering tests, commands, and output

- `crates/keeppix-media/tests/exif_gps.rs`
  - Command: `cargo test -p keeppix-media --test exif_gps --jobs 1 -- --test-threads=1`
  - Output: `5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
- `crates/keeppix-db/tests/assets.rs`
  - Command: `cargo test -p keeppix-db --test assets exif_location --jobs 1 -- --test-threads=1`
  - Output: `1 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out`
    (`exif_location_does_not_overwrite_user_or_map_pin_locations`).
- `crates/keeppix-jobs/tests/metadata.rs`
  - Command: `cargo test -p keeppix-jobs --test metadata --jobs 1 -- --test-threads=1`
  - Output: `8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
- Formatting
  - Command: `cargo fmt --check`
  - Output: exit code 0, no output.
- Scoped lint
  - Command: `cargo clippy -p keeppix-media -p keeppix-db -p keeppix-jobs --all-targets -- -D warnings`
  - Output: exit code 0; all three crates checked with no warnings.

### Commits

- `728f280` — `feat(media): extract GPS coordinates from EXIF at ingest`
- `e44bde6` — `fix(media): strengthen EXIF GPS regression coverage`
