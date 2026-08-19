# Task 8 report — Frontend vista Mappa

## Status

Implemented the lazy MapLibre map UI, offline region manager, place picker,
cluster interactions, viewer mini-map, and active timeline bbox filter.
`AssetView` still has no coordinates and no home geofence was added.

One deployment concern remains: the brief requires a tiny hardcoded region
catalog on `build.protomaps.com`, but that service publishes dated planet
archives, not stable country artifacts. The required sample country URLs
(`IT.pmtiles`, `GR.pmtiles`, etc.) return 404 and no published SHA-256 manifest
is available. The catalog wiring is complete and tests mock it as instructed,
but its URL/checksum metadata must be replaced by real prebuilt extracts before
region downloads can succeed in production.

## What changed

- Added lazy `/map` route, timeline map entry, `MapView`, and `maps` Pinia store.
- Added `MapClusterLayer` with local PMTiles protocol, light/dark system styles,
  cover thumbnails, cluster zoom, single-photo opening, and draw-area mode.
- Added `MapsOfflineView` with downloaded/catalog sections, continent
  aggregates, progress, polling, cancel, delete, and admin-only mutations.
- Added `PlacePicker`; missing tiles never block metadata assignment, while an
  admin can optionally start the matching region download.
- Added the viewer mini-map from effective metadata, without exposing
  coordinates on the public asset view.
- Added compatible `GET /api/v1/assets/{id}` returning the existing public
  `AssetView`, and documented it in OpenAPI.
- Added optional `bbox` to timeline buckets/pages, using the same WGS84 parser
  as map clusters and effective `COALESCE` semantics through explicit
  override-first SQL. The timeline displays and can clear the active area.
- Added matching Italian/English translations.

## TDD evidence

RED:

1. New MapClusterLayer, PlacePicker, and MapsOfflineView suites failed because
   the components did not exist (3 failed suites).
2. Viewer mini-map test failed with zero metadata API calls.
3. Cluster cover test failed because no marker image existed.
4. Asset detail integration test received 405 instead of 200.
5. Timeline bbox integration test returned count 2 instead of 1.

GREEN:

- Task component tests: 8/8 passed.
- Full Vitest suite: 74/74 passed across 21 files.
- Timeline API integration binary: 12/12 passed.
- OpenAPI integration binary: 6/6 passed.

## Verification

- `npm ci`: completed; npm reported the existing Node 22.14 engine warnings.
- `npx vitest run`: 21 files, 74 tests passed.
- `npx vue-tsc --noEmit`: passed.
- `npm run build`: passed.
- Scoped ESLint over every changed frontend file: passed with zero warnings.
- `cargo fmt --check`: passed.
- `cargo clippy -p keeppix-db -p keeppix-api --all-targets -- -D warnings`:
  passed.
- `cargo test -p keeppix-api --test timeline`: 12 passed.
- `cargo test -p keeppix-api --test openapi`: 6 passed.
- `./scripts/test.sh` was not run, as requested.

## Initial bundle measurement

Exact CI-style `gzip -c` byte counts for assets referenced by `dist/index.html`:

| Entry asset | gzip bytes |
|---|---:|
| `/assets/index-C9_iSiP3.js` | 19,084 |
| `/assets/useApi-CROJJdhE-CZ95aOAy.js` | 60,650 |
| `/assets/index-Be6h5K9t.css` | 4,884 |
| **Total** | **84,618** |

Budget: 153,600 bytes. Headroom: 68,982 bytes.

Neither `maplibre-gl-DBpNYYPa.js` (253.76 KB gzip) nor
`maplibre-gl-CKRTiAqP.css` is referenced by `dist/index.html`; both remain
behind the dynamic map import.

## Review fix round

RED:

- Review regression run: 9 tests failed as expected. The failures reproduced
  the fabricated region catalog, absent post-download polling, generic/silent
  API errors, first-library-only clustering, and stale mini-map metadata.

GREEN:

- Focused review suites: 14/14 passed.
- Full Vitest suite: 80/80 passed across 22 files.
- `npx vue-tsc --noEmit`, production build, and scoped ESLint passed.
- Entry assets referenced by `dist/index.html`: 19,473 + 60,650 + 4,877 =
  **85,000 gzip bytes**, below 153,600. MapLibre remains lazy and is not
  referenced by `dist/index.html`.
- Rust was unchanged; `./scripts/test.sh` and Cargo checks were intentionally
  not run.
