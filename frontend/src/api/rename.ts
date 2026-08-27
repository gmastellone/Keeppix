import { apiFetch } from './client'

// "Rinomina con formula…" di Modifica in blocco (§13.3 campo 7) — su
// `RenameRepo` (Fase 9 Task 10), esposta da `/assets/batch/rename*` da
// allora: primo consumatore frontend, nessun cambiamento di backend
// necessario qui.

export interface RenamePreviewItem {
  asset_id: string
  folder_id: string
  current_name: string
  new_name: string
  collides: boolean
}

/** Nessuna scrittura: solo i nomi calcolati, per l'anteprima del dialog. */
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

/** Dal 27 agosto: `202` subito con solo `operation_id` — il lavoro gira in
 * background (`JobKind::BulkRename`), lo stesso motivo per cui la ri-scansione
 * di libreria (`startLibraryScan`) non torna mai l'esito sincrono. Il
 * chiamante segue l'avanzamento reale su `operation.progress`
 * (`api/events.ts`) e annulla con `cancelOperation` (`api/operations.ts`) —
 * stesso pattern di `ProblemsView.vue`. */
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
