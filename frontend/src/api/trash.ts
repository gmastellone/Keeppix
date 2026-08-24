import { apiFetch } from './client'

/** Rispecchia `TrashItemView` (`crates/keeppix-api/src/routes/trash.rs:
 * 176-185`). Bug reale trovato e corretto qui (Task 13 1/N): il tipo
 * precedente dichiarava `filename`/`expires_at`, mai esistiti sul
 * backend — la vista renderizzava `undefined` per entrambi a runtime.
 * `id` qui è l'id della **riga di cestino** (`TrashEntry`), non
 * dell'asset: chi vuole ripristinare o eliminare definitivamente deve
 * usare `asset_id`, il solo id che `POST /assets/{id}/restore` e
 * `DELETE /assets/{id}` accettano. */
export interface TrashedItem {
  id: string
  asset_id: string
  deleted_at: string
  original_path: string
  trash_path?: string
  disk_action: string
  /** Reale, calcolato lato server da `deleted_at` + 30 giorni
   * (`crates/keeppix-api/src/routes/trash.rs:199-202`) — a differenza
   * del mockup, dove il conto alla rovescia è un hash finto dell'id
   * (§45.2, "annunciata ma non implementata"): qui lo è per davvero. */
  days_remaining: number
}

export interface TrashListPage {
  items: TrashedItem[]
  next_cursor?: string
}

export function fetchTrash(cursor?: string): Promise<TrashListPage> {
  return apiFetch(`/api/v1/trash${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`)
}

/** Riporta la foto a `pick = "non ancora deciso"` (§45.3) — non tramite
 * questo id di cestino ma tramite l'asset, come vuole la rotta reale. */
export function restoreAsset(assetId: string): Promise<null> {
  return apiFetch(`/api/v1/assets/${assetId}/restore`, { method: 'POST' })
}

export function emptyTrash(): Promise<{ emptied: number }> {
  return apiFetch('/api/v1/trash/empty', { method: 'POST' })
}
