import { apiFetch } from './client'

// "Rename with formula…" from the bulk-edit menu — backed by
// `RenameRepo`, exposed via `/assets/batch/rename*`; no backend changes
// needed here.

export interface RenamePreviewItem {
  asset_id: string
  folder_id: string
  current_name: string
  new_name: string
  collides: boolean
}

/** No writes: just the computed names, for the dialog preview. */
export function previewRename(assetIds: string[], schema: string): Promise<RenamePreviewItem[]> {
  return apiFetch('/api/v1/assets/batch/rename/preview', {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds, schema })
  })
}

export interface RenameOperationOutcome {
  operation_id: string
  outcome: { succeeded: string[]; failed: { id: string; reason: string; detail?: string }[]; batch_id: string | null }
}

/** Returns `202` immediately with only `operation_id` — the work runs in
 * the background (`JobKind::BulkRename`), the same reason library
 * rescans (`startLibraryScan`) never return a synchronous outcome. The
 * caller tracks real progress via `operation.progress` (`api/events.ts`)
 * and cancels with `cancelOperation` (`api/operations.ts`) — same
 * pattern as `ProblemsView.vue`. */
export interface RenameAccepted {
  operation_id: string
}

export function applyRenameBatch(assetIds: string[], schema: string): Promise<RenameAccepted> {
  return apiFetch('/api/v1/assets/batch/rename', {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds, schema })
  })
}

export function undoRenameBatch(batchId: string): Promise<RenameOperationOutcome> {
  return apiFetch(`/api/v1/assets/batch/rename/${batchId}/undo`, { method: 'POST' })
}
