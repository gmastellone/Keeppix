import { apiFetch } from './client'
import type { TimelineAsset } from './timeline'

export type Pick = 'none' | 'pick' | 'reject'

export interface AssetFlags {
  rating: number | null
  pick: Pick
  color_label: string | null
  /** «Preferito» (`AssetFlagsBody::favorite`, crates/keeppix-api/src/routes/flags.rs):
   * asse indipendente da `pick`, non un suo alias — stessa distinzione già
   * nel backend. Aggiunto qui in Task 7 perché la timeline/Preferiti/SP-2
   * lo scrivono tramite lo stesso endpoint di rimpiazzo completo, non una
   * patch: chi scrive `favorite` deve prima leggere gli altri tre campi,
   * altrimenti li azzera in silenzio (`stores/favorites.ts`). */
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

/** Le tre opzioni della cancellazione (spec §6): nessun default, il chiamante sceglie sempre. */
export type DiskAction = 'kept' | 'moved_to_trash' | 'purged'

export function deleteAsset(assetId: string, diskAction: DiskAction): Promise<null> {
  return apiFetch(`/api/v1/assets/${assetId}`, {
    method: 'DELETE',
    body: JSON.stringify({ disk_action: diskAction })
  })
}

/** §14 griglia dei lotti, §64 "<N> lotti attivi": un lotto è una cartella
 * di primo livello sotto la radice di culling della libreria
 * (`CullingRepo::list_lots`, Fase 9 Task 3, esposta via HTTP in Fase 11
 * Task 17). Vuoto — non un errore — se la libreria non ha ancora una
 * radice designata. */
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

/** §15 "Scelta"/"Scarta": fuori da un lotto resta solo un voto, come
 * `setFlags`; dentro un lotto sposta anche fisicamente il file in
 * `_taken`/`_skipped` (`CullingRepo::set_pick`, Fase 9 Task 4, esposta via
 * HTTP in Fase 11 Task 17). Restituisce l'asset aggiornato — `folder_id`
 * cambia se si è spostato — non solo `204`, perché il chiamante deve saperlo
 * per aggiornarsi senza un secondo giro. */
export function pickAsset(assetId: string, pick: Pick): Promise<TimelineAsset> {
  return apiFetch(`/api/v1/assets/${assetId}/pick`, {
    method: 'POST',
    body: JSON.stringify({ pick })
  })
}

/** Riuscita parziale (spec Fase 10 §3): un asset il cui purge fallisce non
 * blocca gli altri. */
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

/** "Svuota scartati" (§15): elimina definitivamente ogni asset oggi in
 * `_skipped` per questo lotto (`CullingRepo::empty_skipped`, Fase 9 Task 4,
 * esposta via HTTP in Fase 11 Task 17). `lotFolderId` è l'id del lotto
 * stesso, non di `_skipped`: la rotta risolve la sottocartella da sé. */
export function emptySkipped(lotFolderId: string): Promise<BulkOutcome> {
  return apiFetch(`/api/v1/culling/lots/${lotFolderId}/empty-skipped`, {
    method: 'POST'
  })
}
