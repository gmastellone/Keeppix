# Task 21 — report

Batch discover writes (3 `UNNEST` queries/batch + folder cache) instead of
per-file round trips: 1000 files 1.698s → 73ms (~23×). Full pipeline (3
real JPEGs) unaffected (382ms, exif/derive dominate) — confirms discover
was never the bottleneck for the real 1.558-file archive (7m52s, exif+hash
bound). **Ruling: two-tempo import stays a deferred architectural decision**,
not this task's batch insert. `default_night_window()` fixed to 2:00–7:00
(UI wins). `region.progress` pushed on the websocket from `RegionRepo::list`
(region_id, status, downloaded_bytes, size_bytes, last_error).

Fixed a pre-existing-shape defect in `scan.rs`'s cancellation test (`TOTAL`
below `PRODUCTION_BATCH_SIZE`), same class already fixed for
`discover_operations.rs`/`ws.rs`.

Commits: 0c239bc, f9c0ddc, 29c6921, f8d3059, 414ce6b. Full test suites for
`keeppix-db`, `keeppix-jobs`, `keeppix-api`, `keeppix-server` green (except
the already-documented `bootstrap` cross-test flake from Task 19). fmt +
clippy clean. Ledger updated. Not pushed.
