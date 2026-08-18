# Task 2 report — GeoNames-backed places

## Status

DONE. Task 2 only was implemented; no Task 3 population-radius or
user-frequency ranking was added.

Implementation commit: `03d2e6d` — `feat(db): add GeoNames-backed places table`.

## What I implemented

- Added schema-only migration `0020_places.sql` with the `places` table and
  GiST, trigram GIN, and population indexes. The `sqlx::migrate!` dependency
  comment now names `0020_places`.
- Added the domain `Place` type in `keeppix-domain::geo`, reusing the existing
  `GeoPoint` and leaving `LocationSource` in `asset.rs`.
- Added `PlaceRepo` with parameterized simple KNN `nearest`, trigram/ascii-name
  `search`, and idempotent `upsert` using `ON CONFLICT (id) DO UPDATE`.
- Added a bounded-memory importer. It streams the normalized tab-separated
  file, validates rows, upserts batches of 1,000 via bound PostgreSQL arrays,
  and commits the complete import in one transaction. It skips both a
  non-empty table and a missing file.
- Wired server startup after migrations to seed an empty table from
  `/usr/share/keeppix/places.csv`. A source checkout without that baked file
  continues booting with an empty table.
- Added a Docker-only `geonames` stage and `scripts/build-geonames.sh`. The
  stage downloads `cities500.zip`, `admin1CodesASCII.txt`,
  `admin2Codes.txt`, and `countryInfo.txt`, resolves administrative names,
  and emits only the normalized data file into the distroless runtime.
  No runtime HTTP client or download path was added.

## TDD evidence

### RED — migration

Command:

```text
cargo test -p keeppix-db --test migrations expected_tables_exist --jobs 1 -- --exact --test-threads=1 --nocapture
```

Observed expected failure:

```text
test expected_tables_exist ... FAILED
manca la tabella places
test result: FAILED. 0 passed; 1 failed
```

### RED — requested repository/domain interface

Command:

```text
cargo test -p keeppix-db --test places --jobs 1 -- --test-threads=1 --nocapture
```

Observed expected compile failure before adding production interfaces:

```text
error[E0432]: unresolved import `keeppix_db::PlaceRepo`
error[E0432]: unresolved import `keeppix_domain::Place`
```

After adding only the domain/interface skeleton, the same command compiled and
proved the behavior was still absent:

```text
test nearest_returns_the_closest_fixture_place ... FAILED
test normalized_csv_seeds_an_empty_table_only_once ... FAILED
test search_uses_ascii_trigrams_and_preserves_the_original_name ... FAILED
test upsert_updates_an_existing_geoname_id_without_duplication ... FAILED
test result: FAILED. 4 passed; 4 failed
```

### GREEN

After the migration and repository implementation:

```text
cargo test -p keeppix-db --test places --jobs 1 -- --test-threads=1 --nocapture
running 8 tests
test result: ok. 8 passed; 0 failed

cargo test -p keeppix-db --test migrations expected_tables_exist --jobs 1 -- --exact --test-threads=1 --nocapture
test result: ok. 1 passed; 0 failed
```

The fixture contains 12 places and includes `München`/`Munich`, `北京`/`Beijing`,
other non-ASCII names, both hemispheres, and similarly named localities.

## Final verification

Fresh required verification after the final source changes:

```text
cargo test -p keeppix-domain --jobs 1 -- --test-threads=1
47 passed; 0 failed; doc-tests passed

cargo test -p keeppix-db --test places --jobs 1 -- --test-threads=1
8 passed; 0 failed

cargo test -p keeppix-db --test migrations --jobs 1 -- --test-threads=1
8 passed; 0 failed

cargo fmt --check
exit 0

cargo clippy -p keeppix-domain -p keeppix-db -p keeppix-server --all-targets -- -D warnings
exit 0, no warnings
```

As requested, `./scripts/test.sh` and `docker compose` were not run.

Frontend assets required by `keeppix-server` were produced with:

```text
cd frontend && npm ci && npm run build
exit 0
```

The host Node 22.14 emitted engine warnings for packages requiring a newer Node
22 patch or Node 24, but the build completed. The project Docker stage and CI
use Node 24.

Docker build-time dataset validation:

```text
docker build --target geonames -t keeppix-geonames-test .
Successfully built 95ba28855d2a

docker run --rm --entrypoint wc keeppix-geonames-test -l -c /usr/share/keeppix/places.csv
235408 18917710 /usr/share/keeppix/places.csv
```

The first attempt used the environment's legacy Docker builder while the
GeoNames stage followed a BuildKit-only backend stage, so target traversal
stopped at the existing cache mount. Moving the independent GeoNames stage to
the top made the targeted legacy build isolated and green; normal full builds
remain BuildKit builds as before.

## Files

- `crates/keeppix-db/migrations/0020_places.sql`
- `crates/keeppix-db/src/lib.rs`
- `crates/keeppix-db/src/places.rs`
- `crates/keeppix-db/tests/migrations.rs`
- `crates/keeppix-db/tests/places.rs`
- `crates/keeppix-domain/src/geo.rs`
- `crates/keeppix-domain/src/lib.rs`
- `crates/keeppix-server/src/main.rs`
- `scripts/build-geonames.sh`
- `Dockerfile`
- `.superpowers/sdd/2026-08-18-keeppix-fase-4/progress.md`
- `.superpowers/sdd/task-2-report.md`

## Concerns

No implementation blocker or unverified task requirement remains. The baked
dataset currently contains 235,408 rows (18,917,710 bytes), reflecting the
current upstream `cities500` snapshot rather than the spec's approximate
200,000-row figure.
