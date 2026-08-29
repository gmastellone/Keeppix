import { apiFetch } from './client'

/** "Move to folder" from the bulk-edit menu — backed by
 * `AssetRepo::move_to_folder`, exposed via a route for the first time
 * here: no move endpoint existed before, only `PATCH /folders/{id}`
 * which moves a **folder**, not the assets inside it. */
export function moveAssetsBatch(assetIds: string[], folderId: string): Promise<null> {
  return apiFetch('/api/v1/assets/batch/move', {
    method: 'POST',
    body: JSON.stringify({ asset_ids: assetIds, folder_id: folderId })
  })
}
