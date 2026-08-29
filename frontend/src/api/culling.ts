import { apiFetch } from './client'
import type { TimelineAsset } from './timeline'

export type Pick = 'none' | 'pick' | 'reject'

export interface AssetFlags {
  rating: number | null
  pick: Pick
  color_label: string | null
  /** "Favorite" (`AssetFlagsBody::favorite`,
   * crates/keeppix-api/src/routes/flags.rs): an axis independent from
   * `pick`, not an alias for it — the same distinction already exists on
   * the backend. Written through the same full-replace endpoint as the
   * other flags, not a patch: whoever writes `favorite` must first read
   * the other three fields, or they get silently zeroed out
   * (`stores/favorites.ts`). */
  favorite: boolean
}

export const unvotedFlags: Readonly<AssetFlags> = Object.freeze({
  rating: null,
  pick: 'none',
  color_label: null,
  favorite: false
})

export function fetchFlags(assetId: string): Promise<AssetFlags> {
  return apiFetch(`/api/v1/assets/${assetId}/flags`)
}

export function setFlags(assetId: string, flags: AssetFlags): Promise<null> {
  return apiFetch(`/api/v1/assets/${assetId}/flags`, {
    method: 'PUT',
    body: JSON.stringify(flags)
  })
}

/** The three deletion options: no default, the caller always chooses. */
export type DiskAction = 'kept' | 'moved_to_trash' | 'purged'

export function deleteAsset(assetId: string, diskAction: DiskAction): Promise<null> {
  return apiFetch(`/api/v1/assets/${assetId}`, {
    method: 'DELETE',
    body: JSON.stringify({ disk_action: diskAction })
  })
}

/** Three-option deletion across the whole batch (`routes::trash::
 * batch_delete`). For `purged` the authorization is **all-or-nothing**,
 * checked by the server before touching any file — a single
 * non-purgeable asset rejects the entire batch (the `Promise` rejects,
 * `BulkOutcome` is never returned). Must be called once for the whole
 * selection, never in a per-asset loop over `deleteAsset`: a loop would
 * lose exactly that atomic guarantee on the app's only destructive
 * action. */
export function deleteAssetsBatch(assetIds: string[], diskAction: DiskAction): Promise<BulkOutcome> {
  return apiFetch('/api/v1/assets/batch/delete', {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds, disk_action: diskAction })
  })
}

/** A lot is a top-level folder under the library's culling root
 * (`CullingRepo::list_lots`). Empty — not an error — if the library
 * doesn't have a designated root yet. */
export interface CullingLot {
  folder_id: string
  name: string
  created_at: string
  pending: number
  taken: number
  skipped: number
}

export function fetchCullingLots(libraryId: string): Promise<CullingLot[]> {
  return apiFetch(`/api/v1/libraries/${libraryId}/culling/lots`)
}

/** "Pick"/"Reject": outside a lot this only records a vote, same as
 * `setFlags`; inside a lot it also physically moves the file into
 * `_taken`/`_skipped` (`CullingRepo::set_pick`). Returns the updated
 * asset — `folder_id` changes if it moved — not just `204`, since the
 * caller needs to know that to update itself without a second
 * round-trip. */
export function pickAsset(assetId: string, pick: Pick): Promise<TimelineAsset> {
  return apiFetch(`/api/v1/assets/${assetId}/pick`, {
    method: 'POST',
    body: JSON.stringify({ pick })
  })
}

/** Partial success: an asset whose purge fails doesn't block the others. */
export interface BulkFailure {
  id: string
  reason: string
  detail?: string
}

export interface BulkOutcome {
  succeeded: string[]
  failed: BulkFailure[]
  batch_id: string | null
}

/** "Empty rejected": permanently deletes every asset currently in
 * `_skipped` for this lot (`CullingRepo::empty_skipped`). `lotFolderId`
 * is the id of the lot itself, not of `_skipped`: the route resolves the
 * subfolder on its own. */
export function emptySkipped(lotFolderId: string): Promise<BulkOutcome> {
  return apiFetch(`/api/v1/culling/lots/${lotFolderId}/empty-skipped`, {
    method: 'POST'
  })
}
