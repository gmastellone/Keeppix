import { apiFetch } from './client'

/** Mirrors `TrashItemView` (`crates/keeppix-api/src/routes/trash.rs:
 * 176-185`). A real bug was found and fixed here: the previous type
 * declared `filename`/`expires_at`, which never existed on the
 * backend — the view rendered `undefined` for both at runtime.
 * `id` here is the id of the **trash row** (`TrashEntry`), not
 * the asset: restoring or permanently deleting requires
 * `asset_id`, the only id that `POST /assets/{id}/restore` and
 * `DELETE /assets/{id}` accept. */
export interface TrashedItem {
  id: string
  asset_id: string
  deleted_at: string
  original_path: string
  trash_path?: string
  disk_action: string
  /** Real, computed server-side from `deleted_at` + 30 days
   * (`crates/keeppix-api/src/routes/trash.rs:199-202`). */
  days_remaining: number
}

export interface TrashListPage {
  items: TrashedItem[]
  next_cursor?: string
}

export function fetchTrash(cursor?: string): Promise<TrashListPage> {
  return apiFetch(`/api/v1/trash${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`)
}

/** Restores the photo to `pick = "not yet decided"` — via the asset id,
 * not this trash-row id, as the real route requires. */
export function restoreAsset(assetId: string): Promise<null> {
  return apiFetch(`/api/v1/assets/${assetId}/restore`, { method: 'POST' })
}

export function emptyTrash(): Promise<{ emptied: number }> {
  return apiFetch('/api/v1/trash/empty', { method: 'POST' })
}
