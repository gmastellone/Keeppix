# Task 1bis Report — Tarare Postgres sull'hardware

**Branch:** `fase-10`
**Status:** DONE

## Summary

Postgres GUC parameters are now configurable via Compose env vars (SSD/NVMe defaults
documented, microSD override path). Migration `0033` sets aggressive autovacuum on
`assets`. Library discover schedules `VacuumAnalyze` after `mark_scanned`. EXPLAIN
evidence recorded in the ledger.

## Deliverables

| # | Item | File(s) |
|---|------|---------|
| 1 | Compose GUC via env | `compose.yaml` |
| 2 | Two profiles documented | `docs/DEPLOY.md`, `.env.example` |
| 3 | `autovacuum_vacuum_scale_factor = 0.05` | `0033_assets_autovacuum.sql` |
| 4 | Post-import `VACUUM ANALYZE` | `crates/keeppix-jobs/src/discover.rs` |
| 5 | EXPLAIN in ledger | `progress.md` |

## TDD

### RED — `discover_schedules_vacuum_analyze_after_scan`

Added test in `crates/keeppix-jobs/tests/discover.rs` asserting a `vacuum_analyze`
job with dedup key `vacuum_analyze` exists after `discover::run` completes.

Before wiring `maintenance::schedule_vacuum_analyze` in `discover.rs`, the test would
fail with `assertion failed: n == 1` (count 0).

### GREEN

After `discover.rs` calls `maintenance::schedule_vacuum_analyze(db)` after
`mark_scanned`:

```
cargo test -p keeppix-jobs --test discover discover_schedules -- --test-threads=1
→ test discover_schedules_vacuum_analyze_after_scan ... ok
```

### Migration test

```
cargo test -p keeppix-db --test migrations assets_autovacuum -- --test-threads=1
→ test assets_autovacuum_scale_factor_is_aggressive ... ok
```

## EXPLAIN evidence

15k synthetic `indexed` rows, Postgres 17 testcontainer.

**Timeline page** (month 2015-06, LIMIT 200): `Bitmap Index Scan on assets_timeline_idx`
at both `random_page_cost=4.0` and `1.1` — index already chosen for this selectivity.

**Geometry stand-in** (`ORDER BY folder_id, taken_at_utc DESC, id DESC`): `Seq Scan`
at both settings — no covering index yet (Task 2).

Full plans pasted in `progress.md`.

## Verification run

```bash
cargo fmt --check          # after cargo fmt
cargo clippy -p keeppix-jobs -p keeppix-db --all-targets -- -D warnings  # clean
cargo test -p keeppix-jobs --test discover discover_schedules -- --test-threads=1
cargo test -p keeppix-db --test migrations assets_autovacuum -- --test-threads=1
cargo test -p keeppix-db --test migrations explain_guc -- --ignored --nocapture --test-threads=1
```

Full `./scripts/test.sh` not run (Docker/testcontainers cost); focused tests above cover
new behavior.

## Rulings

- `autovacuum_vacuum_scale_factor = 0.05` on `assets` (vs default 0.2).
- Compose defaults = SSD/NVMe profile; microSD via `.env` override.

## Out of scope (confirmed)

- Task 2 geometry index/endpoint — not implemented.
- No push / PR.
