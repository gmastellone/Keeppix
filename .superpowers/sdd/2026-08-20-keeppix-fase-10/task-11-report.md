# Task 11 — Drop per-row counts (except culling)

## Ruling

**Per-row list counts never shipped in `/api/v1`.** — `FolderView`, `AlbumView`, and
`LinkView` never exposed `asset_count`, `member_count`, or `item_count`; the gap
analysis planned them but Task 11 cancels that work instead of adding fields we would
then freeze. `view_count` on share links stays: it counts link **accesses**, not shared
items. — *Cost if wrong:* clients that assumed prototype numbers must not expect them
from the API; Fase 11 sidebar renders names only.

**Culling counts unchanged.** — Badge and batch selector counts live in the culling
session (client-side + flags), not in folder/album/share list endpoints.

## Verification

- `keeppix-db` `sidebar_load_emits_no_per_row_aggregation_queries`: traced sqlx
  statements for `roots`/`tree`/`albums`/`list_by_creator` — no `COUNT(`,
  `GROUP BY`, or `folder_month_counts`.
- `keeppix-api` `sidebar_endpoints_do_not_expose_per_row_counts`: JSON responses
  lack `asset_count` / `member_count` / `item_count`; `view_count` present on links.

## Commits

- `60b5097` test(db): aggregation guard + `TestDb::start_traced`
- `8942209` docs(api) + test(api) + frontend album list cleanup
- (lockfile) tracing-subscriber dev-dep for query capture
