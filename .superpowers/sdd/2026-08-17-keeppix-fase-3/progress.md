# SDD ledger — plan: docs/superpowers/plans/2026-08-17-keeppix-fase-3.md

Spec: docs/superpowers/specs/fase-3-multiutente.md
Branch: fase-3

The six one-liners below were the whole ledger at `ff03fcb`. They stay: they
are still true. Everything after them is the retroactive completion Task 13c
asked for, plus the Task 13 rulings.

Ruling: visibility filter uses three bind slots (grants, holes, asset_ids) — callers must not reuse $3 after the third slot; search `run` starts AST compile at $4, suggest uses ILIKE at $4 — cost if wrong: silent 503/SQL errors on hot paths
Ruling: `filter_for_folder_aggregate` for `folder_month_counts` queries — asset grants use EXISTS on folder_id because there is no `assets` alias in FROM — cost if wrong: timeline buckets always 503
Ruling: share-link album access in `AlbumRepo::assert_visible` checks `Actor::ShareLink` object_id — cost if wrong: public album links return empty assets
Ruling: `SessionNotShare` / `SessionOrShare` split session routes from share-token media — timeline/search reject `X-Share-Token` with 403 — cost if wrong: perimeter escape via search/timeline
Ruling: wired-exceptions Debiti cleared; only Rinvii fase-6/ops/ci remain — cost if wrong: false sense of shipped UI debt

## Retroactive map of `ff03fcb`

One commit, 92 files. Not rewritten. Areas that landed together:

| Area | What shipped in `ff03fcb` |
|---|---|
| Task 1 | `0015_permissions.sql`, `VisibilityScope` EXISTS/NOT EXISTS, `scale_200k.rs` |
| Task 2 | `GroupRepo` + admin `/groups` routes |
| Task 3 | `0016_albums.sql`, album CRUD, share-link asset list via album members |
| Task 4 | `/permissions` + `explain` |
| Task 5 | `0017_share_links.sql`, public info/auth/assets, revoke |
| Task 6 | `ShareAuth` / `SessionOrShare` on media; timeline/search reject share tokens |
| Task 7 | `0019_guest_uploads.sql`, queue + admin approve — **no public ingress** until 13d |
| Task 8 | `0018_audit_log.sql`, `GET /audit` admin-only, append-only |
| Task 9 | in-process `RateLimiter` on login and public share routes |
| Task 10 | SharePanel, SharedView, SharesView (lazy chunks) |
| Task 11 | journeys V5–V12 (V9 was SQL until 13d) |
| Task 12a–12g | admin users, session watchdog, folder tree, trash UI, batch metadata, suggest/saved searches, Debiti emptied |

---

## Task 1 — 200 000 assets, `EXPLAIN ANALYZE`

Measured 2026-08-18 on this agent VM (`cargo test -p keeppix-db --test scale_200k -- --nocapture`). Synthetic rows, no files. Budget from the plan: timeline **300 ms**, search **500 ms**. **Not raised.**

Owner / admin (`two_hundred_thousand_assets_keep_timeline_and_search_within_budget`):

| Probe | Wall | EXPLAIN ANALYZE execution |
|---|---|---|
| seed 200 000 | 5.94 s | n/a |
| `buckets` (274 months) | **3.55 ms** | 1.097 ms |
| timeline first page (200 rows) | **3.45 ms** | 0.353 ms (`assets_timeline_idx`) |
| timeline deep keyset (200 rows) | **2.78 ms** | (same index shape) |
| search `IMG_150000` trgm/ILIKE | **4.52 ms** (1 hit) | 1.574 ms (`assets_filename_trgm`) |

50 grants (`timeline_with_fifty_permissions_stays_under_budget_at_200k`):

| Probe | Wall | EXPLAIN ANALYZE execution |
|---|---|---|
| seed | 6.19 s | n/a |
| `buckets` | **6.56 ms** | — |
| timeline page | **4.93 ms** | — |
| EXISTS/NOT EXISTS (production) | — | **0.390 ms** |
| recursive CTE (comparison only) | — | **0.323 ms** |

Ruling: production filter is **EXISTS grant AND NOT EXISTS inherit=false hole**, nested in the main query — not a recursive CTE, not a materialized visibility table. At this scale both plans are a tie (0.3–0.4 ms, ~50× under budget). EXISTS wins because it is already a clause with three bind slots, `ltree <@` does the descendant walk, and `inherit=false` is a hole list rather than a recursive expansion that grows with folder count. Cost if wrong: a deep tree with many `inherit=false` holes could make the SubPlan seq-scans on `folders` visible; then cache resolved prefixes per user (plan mitigation), still without a visibility table.

The EXPLAIN for buckets/page on the admin path does **not** include the share-link filter (unrestricted admin). The 50-permission EXPLAIN is the one that exercises the clause.

---

## Task 2 — groups

Ruling: deleting a group with active permissions returns **409** unless `?cascade=true`. Silent CASCADE would drop other people's access without a prompt. Cost if wrong: an admin who expects POSIX-style recursive delete has to pass the flag once.

Ruling: group names unique case-insensitively; last member may leave, the group stays. Cost if wrong: empty groups linger until an admin deletes them.

## Task 3 — albums

Ruling: albums are virtual (`album_assets` only). Position gap is **MAX+1000** so insert-between does not renumber. Cost if wrong: a 5000-photo album reorder still touches one row; if gaps run out the client must pick a new integer (no auto-repack yet).

Ruling: sharing an album grants those asset ids, not their folders. Implemented by `VisibilityScope.asset_ids` plus `AlbumRepo::assert_visible` on `Actor::ShareLink`. Cost if wrong: an album link becomes a folder leak.

## Task 8 — audit

Ruling: `audit_log` is append-only; `GET /audit` is admin-only. No UPDATE/DELETE route. Cost if wrong: retention/cleanup, if ever needed, must be a maintenance job.

Ruling: **`sessions.ip` stays unpopulated** (Fase 0 debt still deferred). Filling it from `X-Forwarded-For` without a trusted-proxy config would store the proxy's address and look like a fact. Cost if wrong: audit lines have no client IP until ops defines the proxy list.

## Task 9 — rate limiting

Ruling: in-process sliding window (`HashMap` + `Mutex`, periodic sweep), **no Redis**. Login: 10 / 300 s. Public share token: 60 / 60 s. Restart resets the counters. Cost if wrong: two nodes behind a balancer do not share the limit (accepted: D5, single node).

## Task 12a–12g

Ruling 12a: disable user revokes sessions immediately (already the 2R contract); the UI is now the consumer. Cost if wrong: a disabled user keeps an open tab until TTL.

Ruling 12b: session watchdog calls `POST /auth/refresh` every **12 h** while `document.visibilityState === 'visible'`, and stops the timer when the tab hides. Cost if wrong: a culling session longer than the absolute TTL with the tab backgrounded still dies — that is the Pi constraint (no all-night refresh).

Ruling 12c: folder tree expands one level via `/folders/{id}/children`; `/folders/tree` is roots only. Cost if wrong: a 200k library with thousands of folders would ship the whole tree in one JSON.

Ruling 12d: trash UI lists expiry; empty requires a confirmation that shows counts. Cost if wrong: a misclick still has the retention window if they used delete-to-trash, not empty.

Ruling 12e: batch metadata writes `asset_overrides` only; original EXIF stays. Undo is the batch id returned by the API. Cost if wrong: a photographer cannot tell camera time from a bad shift.

Ruling 12f: suggest is debounced and the previous in-flight request is aborted. Cost if wrong: each keystroke is a 200k ILIKE.

Ruling 12g: Debiti section of `wired-exceptions.txt` is empty; leftover names are rinvii (`fase-6`, `ops`, `ci`). Cost if wrong: a new unused route ships and the guard is the only alarm.

---

## Task 13 — review findings

### 13a — password-protected links

`public_auth` verified the password, built `_ctx`, and discarded it. `ShareAuth` never read `password_hash`. URL alone was enough, including `/media/*`.

Ruling: after a correct password, issue an **opaque unlock token** (same shape as `ShareToken`) stored in-process in `ShareUnlockStore` with a **1 h TTL**. Restart → re-enter password. Same reason as the rate limiter: single node, no Redis. Cookies (always `Secure`, `__Host-` prefix):

- `__Host-kpx_share` — unlock proof
- `__Host-kpx_share_link` — share token so `<img>` can authenticate without `X-Share-Token`

Header fallback: `X-Share-Unlock`. `ShareAuth` requires a matching unlock when `password_hash` is `Some`. Links without a password do not.

Cost if wrong: a second Keeppix node does not share unlocks (guest re-enters). A 1 h window is stolen-cookie-sized; shorter would prompt on a long wedding-album browse.

Test that failed first: `GET /media/thumb/{hash}` with `X-Share-Token` and no `/auth` → 403 (was 200).

### 13b — fmt / clippy / confinement channels

`inside_id` was unused because the test never fetched inside media by id. It now asserts thumb/original for the inside hash and 403 on thumb/preview/full/original for the **outside** hash. Outside bytes are distinct so the two files do not share `content_hash`.

### 13c — this ledger

### 13d — public guest upload

Task 7 had shipped the queue and admin `POST /guest-uploads/{id}/approve` only. V9 inserted the asset via SQL.

Ruling: `POST /api/v1/share/{token}/uploads?filename=` with raw `application/octet-stream`. No multipart crate. **Folder links only** — album destination is unspecified. `upload_quota_bytes` NULL means **256 MiB**, not unlimited (a Pi disk). Quota is applied **while reading** (`DefaultBodyLimit` disabled on that route only; the handler stops and deletes the partial file). The asset is inserted `discovered` + `uploaded_by_guest` so `folder_month_counts` / timeline do not count it; public `/assets` lists indexed only. Approve clears the flag and enqueues `ExtractMetadata`.

Cost if wrong: album upload links 403 until a later phase; a link created with no quota caps at 256 MiB.

V9 now POSTs a real file through that route.

### 13e — commits after `ff03fcb`

One commit per unit from 13a onward. History of `ff03fcb` not rewritten.

### 13f — unknown share token

`public_info` / `public_auth` now return **forbidden** for an unknown/expired/revoked token, same as `ShareAuth`. Not a bypass (both denied); removes an existence oracle.

---

Task 13: in progress until fmt/clippy/`test.sh`/`npm run build` are green on this tree.
