import { apiFetch } from './client'

export interface AssetMetadata {
  exif: Record<string, string>
  overrides: Record<string, string>
}

export function fetchMetadata(assetId: string): Promise<AssetMetadata> {
  return apiFetch(`/api/v1/assets/${assetId}/metadata`)
}

export function updateOverrides(
  assetId: string,
  overrides: Record<string, string>
): Promise<null> {
  return apiFetch(`/api/v1/assets/${assetId}/metadata/overrides`, {
    method: 'PUT',
    body: JSON.stringify(overrides)
  })
}

export function batchUpdateOverrides(
  assetIds: string[],
  overrides: Record<string, string>
): Promise<null> {
  return apiFetch('/api/v1/assets/metadata/batch', {
    method: 'PUT',
    body: JSON.stringify({ asset_ids: assetIds, overrides })
  })
}

export function applyMetadataBatch(
  asset_ids: string[],
  patch: Record<string, string | null>
): Promise<{ batch_id: string }> {
  return apiFetch('/api/v1/metadata/batch', {
    method: 'POST',
    body: JSON.stringify({ asset_ids, patch })
  })
}

export function shiftTakenAtBatch(asset_ids: string[], hours: number): Promise<{ batch_id: string }> {
  return apiFetch('/api/v1/metadata/batch/shift-taken-at', {
    method: 'POST',
    body: JSON.stringify({ asset_ids, hours })
  })
}

export function undoMetadataBatch(batch_id: string): Promise<null> {
  return apiFetch(`/api/v1/metadata/batch/${batch_id}/undo`, { method: 'POST' })
}

export function batchSetFlags(
  asset_ids: string[],
  flags: { pick?: string | null; rating?: number | null }
): Promise<null> {
  return apiFetch('/api/v1/flags/batch', {
    method: 'POST',
    body: JSON.stringify({ asset_ids, ...flags })
  })
}
