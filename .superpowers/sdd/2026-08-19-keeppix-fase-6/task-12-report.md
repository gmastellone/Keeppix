# Task 12 report — Prestazioni (indici, cache, N+1)

Branch: `fase-6`
Commit message: `perf(db): trigram indices, invalidated in-process permission cache, missing FK indices`

## Delivered

1. **Migration `0030_performance_indexes.sql`**
   - `asset_exif_camera_trgm` / `asset_exif_lens_trgm` (GIN `gin_trgm_ops`)
   - `stacks_primary_asset_idx`, `album_assets_added_by_idx`
2. **In-process `moka` caches on `Db`**
   - Permission / visibility scope keyed by user id
   - Settings JSON keyed by setting key
   - Explicit invalidation on write paths (no TTL-only correctness)
3. **`FolderRepo::ensure_path` keep-as-is** with measured evidence (see Ruling)

## Cache invalidation coverage

| Trigger | Test | Invalidation site |
|---|---|---|
| Permission revoke | `scope_drops_a_revoked_permission_even_after_the_scope_was_cached` | `PermissionRepo::revoke` (+ grant/patch) |
| Role change | `scope_updates_when_a_user_role_changes_after_being_cached` + API `a_role_change_takes_effect_on_the_existing_session_immediately` | `UserRepo::update_profile` + `SessionCache::clear` on PATCH |
| Group member removal | `scope_drops_group_access_when_a_member_is_removed_after_being_cached` | `GroupRepo::remove_member` / `add_member` / `delete` |
| User deletion / eviction | `scope_for_a_deleted_user_is_evicted_before_their_id_can_be_reused` | `Db::invalidate_permission_cache_for_user` |
| Settings write | `put_json_invalidates_a_cached_setting_immediately` | `SettingsRepo::put_json` |
| Library create/delete | `scope_updates_when_a_library_is_created` | `LibraryRepo::create` / `delete` |

Each invalidation test first mutates via raw SQL (cache stays stale) then via the
repo method (cache updates) — proving explicit invalidation, not accidental
re-read.

## Index evidence (`EXPLAIN ANALYZE`)

With the competing btree `asset_exif_camera_idx` temporarily marked invalid in a
rolled-back transaction (small 5k-row table otherwise prefers btree+filter):

```
Bitmap Index Scan on asset_exif_camera_trgm
  Index Cond: (camera_model ~~* '%EOS R5%'::text)
Bitmap Index Scan on asset_exif_lens_trgm
  Index Cond: (lens ~~* '%24-70%'::text)
```

See `crates/keeppix-db/tests/perf_task12.rs`. Index presence also asserted in
`migrations::performance_indexes_exist`.

## `ensure_path` measurement

From `ensure_path_cost_stays_acceptable_for_ingest_depths`:

| Scenario | Result |
|---|---|
| depth=1 ×100 existing | ~0.6 ms/call |
| depth=5 ×100 existing | ~1.4 ms/call |
| depth=10 ×100 existing | ~2.3 ms/call |
| depth=20 ×100 existing | ~3.9 ms/call |
| 50 cold depth-8 trees | ~110 ms total (~2.2 ms each) |

**Decision: keep current N+1-per-segment implementation.** Cost is acceptable for
ingest/scan writes; a single-query rewrite is deferred.

## Deviations from the plan sketch

- `moka` is a dependency of **`keeppix-db`**, not `keeppix-api`; no
  `keeppix-api/src/cache.rs` — caches sit on `Db` so every caller (API, jobs,
  repos) shares invalidation.
- Migration number **`0030`**, not the plan placeholder `0032`.

## Tests run (targeted, before full suite)

- `cargo test -p keeppix-db --test visibility --test settings --test migrations --test perf_task12`
- `cargo test -p keeppix-api --test users`

Full gate (`npm ci && npm run build`, `cargo fmt --check`, clippy `-D warnings`,
`./scripts/test.sh`) runs after this report/commit.

## Concerns

- On very small `asset_exif` tables the planner may still prefer the legacy
  btree partial index + filter for `ILIKE`; the GIN trigram indexes are used as
  data grows or when that btree is not competitive. This matches Postgres
  cost-based choice and is not a migration failure.
- Single-process `moka` only: correct for Keeppix's one-node model; multi-node
  would need shared invalidation (out of scope).
