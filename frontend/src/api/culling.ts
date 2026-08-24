import { apiFetch } from './client'

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
