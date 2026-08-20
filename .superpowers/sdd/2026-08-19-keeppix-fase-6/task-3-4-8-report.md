# Tasks 3 + 4 + 8 — Backup / restore / maintenance

Branch: `fase-6`
Date: 2026-08-20

## Status

**Complete** (honest minimum for remote destinations — see rulings).

| Task | Outcome |
|------|---------|
| 3 Backup format + wizard | Done |
| 4 Destinations + restore | Done (S3/SFTP limits recorded) |
| 8 Maintenance scheduler | Done |

## Commits

- `a78f602` — `feat(db): backup destinations/runs repo and maintenance cleanup helpers`
- `6be23b3` — `feat(jobs): .kpxb backup format, destinations, and maintenance scheduler`
- `eb3d8d0` — `feat(api): backup/restore wizard endpoints and settings views`
- (docs commit after this report)

## What landed

### Task 3
- Migration `0032_backup_config.sql` — `backup_destinations`, `backup_runs` (+ `path`, `keeppix_version`)
- `BackupRepo` with AES-GCM destination config, preferences, run lifecycle
- `.kpxb` = `age(zstd(tar))` with `manifest.json`, optional components
- Job `BackupDump`, BackupView with **mandatory originals warning**

### Task 4
- Destinations: local (full), S3/WebDAV (HTTP PUT + test), SFTP (`scp` + TCP test)
- Restore API: inspect, dry-run, safety dump before DB overwrite, reject newer version, hot maps-only
- Monthly restore proof job into temp schema then drop

### Task 8
- Scheduler additions on existing `schedule()` pattern:
  - purge expired sessions
  - cleanup done jobs >7d
  - cleanup transcode cache >90d (mtime)
  - cleanup idempotency keys >24h
  - VACUUM ANALYZE (night)
  - backup dump (night)
  - integrity scrub 5% report-only (night)
  - restore proof (night)
- Background priority → Interactive EnergyProfile will not claim them

## Tests run

| Suite | Result |
|-------|--------|
| `cargo fmt --check` | green |
| `cargo clippy -p keeppix-{db,domain,jobs,api,server} --all-targets -- -D warnings` | green |
| `cargo test -p keeppix-domain --lib job` | 4/4 |
| `cargo test -p keeppix-db --test backup` | 8/8 |
| `cargo test -p keeppix-jobs --lib backup` | 2/2 (incl. external age/zstd/tar) |
| `cargo test -p keeppix-jobs --test maintenance` | 6/6 |
| `cargo test -p keeppix-jobs --lib maintenance` | 1/1 |
| `cargo test -p keeppix-api --test backup` | 2/2 |
| `frontend npm run build` | green |

## Remaining / not run

- Full `./scripts/test.sh` (workspace) **not** run in this session — would recompile everything and `cargo clean` after. Targeted coverage above is green.
- OpenAPI snapshot / generated clients **not** updated for new backup/restore paths (Task 9 already closed earlier; additive routes only).
- Native AWS SigV4 and embedded SSH for SFTP are **not** implemented — see ledger rulings.
- Optional originals/sidecars/derivatives packaging creates empty marker dirs when selected; full library tree copy is deferred (catalogue-first honesty).

## Risks

1. Full `pg_restore --clean` can leave a partial DB if interrupted mid-restore (documented; safety dump is taken first).
2. S3 without SigV4 may fail against stock AWS — operators need a compatible endpoint or a follow-up.
3. Night jobs are polled hourly inside the 02:00–06:00 window; first enqueue may lag up to ~1h after night starts.
