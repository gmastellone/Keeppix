import { apiFetch } from './client'
import type { SearchNode } from '@/search/ast'
import type { TimelineAsset } from './timeline'

/** Mirrors `AlbumView` (`crates/keeppix-api/src/routes/albums.rs:20
 * -44`). A real bug was found and fixed here: the previous type declared
 * `cover_hash` (never existed on the backend, always `undefined` at
 * runtime) instead of `cover_asset_id`, and `fetchAlbum()` was typed as
 * if `GET /albums/{id}` also returned `assets` — it never does (verified
 * by reading the full `routes::albums::get` handler): the member list
 * only lives on `GET /albums/{id}/assets`, a separate route the frontend
 * never called before. `AlbumPickerDialog.vue` papered over the gap with
 * `detail?.assets ?? []`, which therefore always returned `[]` in
 * production — the membership shown by the picker never reflected the
 * server's actual state. */
export interface Album {
  id: string
  name: string
  description: string
  owner_id: string
  cover_asset_id?: string
  created_at: string
  updated_at: string
  /** Present only if the album can be refreshed via `POST .../refresh`
   * (there is no "dynamic" album that updates itself — just a filter
   * that re-runs when the user asks for it). */
  rule?: SearchNode
  rule_run_at?: string
  is_shared: boolean
  /** Always absent in practice: no route allows setting it
   * (`PatchAlbumBody` has no `cover_tint`/`monochrome` field) — the
   * gradient cover is therefore computed client-side, deterministic on
   * the id, never read from here. The field stays typed for
   * completeness, in case the backend populates it one day. */
  cover_tint?: string
  monochrome: boolean
}

/** An album member as returned by `GET /albums/{id}/assets`
 * (`AlbumAssetView`, `#[serde(flatten)]` of `AssetView` plus three of its
 * own fields) — a superset of `TimelineAsset`; the extra fields are
 * ignored by callers that only expect that shape. */
export interface AlbumAsset extends TimelineAsset {
  position: number
  added_by: string
  added_at: string
}

export function fetchAlbums(): Promise<Album[]> {
  return apiFetch('/api/v1/albums')
}

export function fetchAlbum(id: string): Promise<Album> {
  return apiFetch(`/api/v1/albums/${id}`)
}

/** The actual contents of an album (see the comment on `Album` above for
 * the bug this route exposed). */
export function fetchAlbumAssets(id: string): Promise<AlbumAsset[]> {
  return apiFetch(`/api/v1/albums/${id}/assets`)
}

export function createAlbum(name: string, rule?: SearchNode): Promise<Album> {
  return apiFetch('/api/v1/albums', {
    method: 'POST',
    body: JSON.stringify(rule ? { name, rule } : { name })
  })
}

export function deleteAlbum(id: string): Promise<null> {
  return apiFetch(`/api/v1/albums/${id}`, { method: 'DELETE' })
}

/** "Refresh album": re-runs the `rule` the album was created with —
 * there is no continuous auto-recomputation, only a filter that re-runs
 * on request. Responds with the same `BulkOutcome` used by every other
 * bulk operation (`crates/keeppix-api/src/bulk.rs`), reused here in an
 * unusual way: `succeeded` is the concatenation of ids that entered
 * **and** left the album, without distinguishing which — the handler
 * (`routes::albums::refresh`) merges them before responding. A separate
 * "added"/"removed" count is therefore not possible from here. */
export function refreshAlbum(id: string): Promise<{ succeeded: string[] }> {
  return apiFetch(`/api/v1/albums/${id}/refresh`, { method: 'POST' })
}

/** Adds multiple assets to an album — `POST /albums/{id}/assets/{asset_id}`
 * (`routes/albums.rs::add_asset`) takes **one** id at a time, verified
 * against the real backend: no batch endpoint exists on this path
 * (`GET /albums/{id}/assets` is something else entirely, the member
 * list). A real bug was found here — the previous version POSTed an
 * `{asset_ids}` body to a URL that only accepts one id — fixed with a
 * sequential loop over the same single-item endpoint, the same approach
 * as `stores/favorites.ts`'s `setMany`. */
export async function addAssets(albumId: string, assetIds: string[]): Promise<null> {
  for (const assetId of assetIds) {
    await apiFetch(`/api/v1/albums/${albumId}/assets/${assetId}`, { method: 'POST' })
  }
  return null
}

export function removeAsset(albumId: string, assetId: string): Promise<null> {
  return apiFetch(`/api/v1/albums/${albumId}/assets/${assetId}`, {
    method: 'DELETE'
  })
}

/** An album an asset already belongs to — id and name only, the chips
 * are not clickable. */
export interface AlbumBadge {
  id: string
  name: string
}

/** The reverse of `fetchAlbum` — given an asset, which albums it already
 * belongs to (manual and dynamic albums alike, both materialized in the
 * same `album_assets` table). */
export function fetchAlbumsForAsset(assetId: string): Promise<AlbumBadge[]> {
  return apiFetch(`/api/v1/assets/${assetId}/albums`)
}
