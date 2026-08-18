# Task 6 report — Cluster sulla mappa

## What I implemented

- Added `GeoRepo::clusters(AuthContext, MapBounds, zoom, MapScope)` in
  `keeppix-db`.
- Added `GET /api/v1/map/clusters?bbox=&zoom=&scope=&scope_id=` using `Auth`
  and `keeppix_api::Json`.
- Supported `library`, `album`, `folder`, and saved `search` scopes. Unknown or
  foreign ids return `403 keeppix/forbidden`.
- Reused `VisibilityScope::filter("f.path", "f.library_id", "a.id", ...)` in
  the candidate query; no parallel visibility SQL was introduced.
- Clustered the effective coordinate
  `COALESCE(asset_overrides.location, assets.location)`.
- Selected covers by the caller's rating, then capture time and id.
- Split antimeridian bboxes into the `west..180` and `-180..east` envelopes.
- Used a deterministic `90 / 2^zoom` degree grid. At zoom >= 15, at most 500
  points are returned individually; a 501st candidate causes grid fallback.
- Loaded saved-search text through `SearchRepo`, checked ownership, parsed the
  existing search grammar, and reused the existing parameterized search
  compiler with effective GPS semantics.
- Registered the route and schema in OpenAPI and regenerated
  `docs/api/openapi.json`.
- Added no migration and no timezone, PMTiles, frontend, or timeline-bbox work.

## TDD evidence

### RED — database contract absent

Command:

```bash
cargo test -p keeppix-db --test geo --jobs 1 -- --test-threads=1
```

Observed failure:

```text
error[E0432]: unresolved imports `keeppix_db::GeoRepo`,
`keeppix_db::MapBounds`, `keeppix_db::MapScope`
could not compile `keeppix-db` (test "geo")
```

### GREEN — database behavior

The focused suite passed 9/9 executions, including:

- folder visibility and per-viewer cover rating;
- effective override location;
- Fiji-style antimeridian split excluding an Atlantic point;
- unclustered zoom 15 and 500-point cap fallback;
- all four scope kinds and foreign-scope `Forbidden`;
- 4,000 synthetic geotagged assets under the one-second local bound.

### RED — HTTP route absent

Command:

```bash
cargo test -p keeppix-api --test map --jobs 1 -- --test-threads=1
```

Observed result before mounting the route:

```text
4 tests failed; every request returned 404
```

### GREEN — HTTP behavior

The focused endpoint suite passed 4/4: authentication, payload shape, stable
RFC 9457 validation errors, and foreign-scope 403.

### RED/GREEN — OpenAPI

Before registration, the contract test failed with:

```text
manca il percorso /api/v1/map/clusters
```

After registration and intentional snapshot regeneration, all 6 OpenAPI tests
passed.

## Verification

All commands required by the task context passed:

```text
cargo fmt --check
  PASS

cargo clippy -p keeppix-db -p keeppix-api --all-targets -- -D warnings
  PASS

cargo test -p keeppix-db --jobs 1 -- --test-threads=1
  PASS — every test binary and doc-test green

cargo test -p keeppix-api --jobs 1 -- --test-threads=1
  PASS — every test binary and doc-test green
```

`./scripts/test.sh` was not run, as requested.

## Files changed

- `crates/keeppix-db/src/geo.rs`
- `crates/keeppix-db/src/lib.rs`
- `crates/keeppix-db/src/search.rs`
- `crates/keeppix-db/tests/geo.rs`
- `crates/keeppix-api/src/routes/map.rs`
- `crates/keeppix-api/src/routes/mod.rs`
- `crates/keeppix-api/src/lib.rs`
- `crates/keeppix-api/src/openapi.rs`
- `crates/keeppix-api/tests/map.rs`
- `crates/keeppix-api/tests/openapi.rs`
- `docs/api/openapi.json`
- `.superpowers/sdd/2026-08-18-keeppix-fase-4/progress.md`
- `.superpowers/sdd/task-6-report.md`

## Self-review

- Confirmed production SQL exists only in `keeppix-db`.
- Confirmed `GeoRepo::clusters` and `SearchRepo::saved_query` take
  `AuthContext` first.
- Confirmed no production `unwrap()` or `expect()` was introduced.
- Confirmed bbox and search values are bound parameters; formatted SQL contains
  only clauses generated from closed Rust enums/functions.
- Confirmed the high-zoom cap probes only 501 rows, not the full viewport.
- Confirmed router, OpenAPI path, security requirement, operation id, schema,
  operation count, and committed snapshot agree.
- Confirmed changes remain within Task 6 boundaries.

## Concerns

None blocking. Saved searches persist text rather than an AST, so the server
must mirror the frontend parser to execute a saved-search map scope. The ruling
and migration cost are recorded in the phase ledger.

## Commits

- `a42e8cc feat(api): grid-clustered map endpoint scoped by permission`
