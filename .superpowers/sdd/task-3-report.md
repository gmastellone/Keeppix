# Task 3 report — reverse and forward geocoding

## Status

DONE. Task 3 only was implemented. No assignment, map clustering, PMTiles, or
other Task 4+ work was started.

Implementation commit: `f0813d4` —
`feat(api): reverse and forward geocoding against places`.

## What changed

- `PlaceRepo::nearest` now filters each locality by a population-weighted
  radius. The SQL linearly interpolates 3 km at population 600 to 25 km at
  500,000 and clamps outside that range. It then tries region rows within
  200 km and country rows within 1,000 km before returning `None`.
- `PlaceRepo::search` now takes `AuthContext` first. It ranks by trigram
  similarity, a 250 km boost around the centroid of the caller's last 50
  manual override locations, population, then id. With no history (or
  `near_user=false`) population is the tie-breaker. The repository clamps the
  result count to ten.
- Added authenticated `GET /api/v1/places/reverse` and
  `GET /api/v1/places/suggest`. They return `keeppix_api::Json`, validate
  query input, and use RFC 9457 problem responses. A reverse miss is
  `404 keeppix/place-not-found`; a query under two characters is
  `400 keeppix/place-query-too-short`.
- Added `PlaceView`, route wiring, two utoipa operations, the places tag, and
  regenerated `docs/api/openapi.json`. The operation count is now 56.

## TDD evidence

### RED — repository interface and behavior

Command:

```text
cargo test -p keeppix-db --test places --jobs 1 -- --test-threads=1
```

Observed expected compile failure before production changes:

```text
error[E0061]: this method takes 2 arguments but 4 arguments were supplied
... .search(&ctx, "Munch", false, 10)
error: could not compile `keeppix-db` (test "places") due to 6 previous errors
```

After the interface implementation, the weighted-radius test initially
reported village id 10 instead of city id 11. The fixture longitude was only
2.28 km away at latitude 45, so the fixture—not the formula—was inside the
specified 3 km radius. Moving it to 3.23 km produced the intended pinned
scenario.

### GREEN — repository

```text
cargo test -p keeppix-db --test places --jobs 1 -- --test-threads=1
running 13 tests
test result: ok. 13 passed; 0 failed
```

The suite includes the village/city radius case, region-then-country fallback,
ocean miss, personalized Sorrento ranking from exactly 50 overrides, and the
empty-history population fallback. The two Sorrento rows deliberately have
equal normalized names while California has the larger population, so removing
the history boost makes the test fail.

### RED — HTTP routes

Command:

```text
cargo test -p keeppix-api --test places --jobs 1 -- --test-threads=1
```

Before route implementation all six tests failed on the real router:

```text
places_endpoints_require_an_authenticated_session: left 404, right 401
reverse_returns_the_large_city_inside_its_population_radius: left 404, right 200
suggest_rejects_queries_shorter_than_two_characters: left 404, right 400
suggest_ranks_sorrento_near_the_users_recent_overrides_first: left 404, right 200
suggest_returns_at_most_ten_places: left 404, right 200
test result: FAILED. 0 passed; 6 failed
```

The ocean test also distinguished the generic fallback
`keeppix/not-found` from the required `keeppix/place-not-found`.

### GREEN — HTTP

```text
cargo test -p keeppix-api --test places --jobs 1 -- --test-threads=1
running 6 tests
test result: ok. 6 passed; 0 failed
```

## OpenAPI

The additive snapshot was regenerated deliberately:

```text
UPDATE_OPENAPI=1 cargo test -p keeppix-api --test openapi \
  openapi_snapshot_matches_the_committed_file -- --exact
test result: ok. 1 passed; 0 failed
```

The complete OpenAPI suite verifies both new operations are mounted, protected
by the session cookie scheme, uniquely named, and present in the snapshot.

## Final verification

Fresh required verification after the final source changes:

```text
cargo test -p keeppix-db --test places --jobs 1 -- --test-threads=1
13 passed; 0 failed

cargo test -p keeppix-api --test places --jobs 1 -- --test-threads=1
6 passed; 0 failed

cargo test -p keeppix-api --test openapi --jobs 1 -- --test-threads=1
6 passed; 0 failed

cargo fmt --check
exit 0

cargo clippy -p keeppix-db -p keeppix-api --all-targets -- -D warnings
exit 0, no warnings
```

As requested, `./scripts/test.sh` was not run. These crates do not compile the
embedded server frontend, so rebuilding `frontend/dist` was not required.

## Files

- `crates/keeppix-db/src/places.rs`
- `crates/keeppix-db/tests/places.rs`
- `crates/keeppix-api/src/routes/places.rs`
- `crates/keeppix-api/src/routes/mod.rs`
- `crates/keeppix-api/src/lib.rs`
- `crates/keeppix-api/src/openapi.rs`
- `crates/keeppix-api/tests/places.rs`
- `crates/keeppix-api/tests/openapi.rs`
- `docs/api/openapi.json`
- `.superpowers/sdd/2026-08-18-keeppix-fase-4/progress.md`
- `.superpowers/sdd/task-3-report.md`

## Concerns

The frozen `places` schema has no feature-kind column, so administrative rows
use the documented `population = 0` convention. The current Task 2 Docker
normalizer resolves `admin1` and country data onto city rows but does not emit
dedicated administrative rows. The fallback logic is therefore ready and
covered with seeded region/country rows, as requested, but the baked production
catalog must emit those rows before region/country fallback can occur outside
tests. Adding a feature-kind column later would also remove the small ambiguity
with a GeoNames locality whose population is zero.

## Fix after review

Commits:

- `824c604` — `fix(db): prevent wildcard place searches`
- `437bbd9` — `fix(build): include GeoNames fallback rows`

The normalizer now emits one `population = 0` row per admin1/country that has
city coordinates, using the city-coordinate average. Its optional fixture
directory avoids all network access in the regression test. Place search now
uses only the spec's `ascii_name % $query` trigram predicate.

RED evidence for `q=%%`:

```text
$ cargo test -p keeppix-db --test places --jobs 1 -- --test-threads=1
test search_does_not_treat_sql_wildcards_as_match_all ... FAILED
assertion failed: results.is_empty()
test result: FAILED. 13 passed; 1 failed
```

Fresh verification after both fixes:

```text
$ scripts/test-build-geonames.sh
GeoNames normalizer fixture test: ok (2 cities, 1 region, 1 country)

$ cargo test -p keeppix-db --test places --jobs 1 -- --test-threads=1
running 14 tests
test result: ok. 14 passed; 0 failed

$ cargo test -p keeppix-api --test places --jobs 1 -- --test-threads=1
running 6 tests
test result: ok. 6 passed; 0 failed

$ cargo fmt --check
exit 0

$ cargo clippy -p keeppix-db -p keeppix-api --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.54s
exit 0, no warnings
```

As required, `./scripts/test.sh` was not run. No Task 4 work was started.
