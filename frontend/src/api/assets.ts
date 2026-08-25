import { apiFetch } from './client'

/** "Sposta in cartella" di Modifica in blocco (§13.3 campo 8) — su
 * `AssetRepo::move_to_folder` (Fase 9 Task 1), esposta da una rotta per la
 * prima volta nel Fase 11 Task 7: nessun endpoint di spostamento esisteva
 * finora, solo `PATCH /folders/{id}` che sposta una **cartella**, non gli
 * asset al suo interno. */
export function moveAssetsBatch(assetIds: string[], folderId: string): Promise<null> {
  return apiFetch('/api/v1/assets/batch/move', {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds, folder_id: folderId })
  })
}
