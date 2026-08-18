Ruling: visibility filter uses three bind slots (grants, holes, asset_ids) — callers must not reuse $3 after the third slot; search `run` starts AST compile at $4, suggest uses ILIKE at $4 — cost if wrong: silent 503/SQL errors on hot paths
Ruling: `filter_for_folder_aggregate` for `folder_month_counts` queries — asset grants use EXISTS on folder_id because there is no `assets` alias in FROM — cost if wrong: timeline buckets always 503
Ruling: share-link album access in `AlbumRepo::assert_visible` checks `Actor::ShareLink` object_id — cost if wrong: public album links return empty assets
Ruling: `SessionNotShare` / `SessionOrShare` split session routes from share-token media — timeline/search reject `X-Share-Token` with 403 — cost if wrong: perimeter escape via search/timeline
Ruling: wired-exceptions Debiti cleared; only Rinvii fase-6/ops/ci remain — cost if wrong: false sense of shipped UI debt
Task fase-3: complete (tests green, frontend build green, check-wired green)
