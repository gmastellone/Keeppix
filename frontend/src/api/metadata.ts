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

export interface TimezonePreview {
  count: number
  example: {
    asset_id: string
    filename: string
    before: string
    after: string
    timezone: string
  } | null
  preview_token: string
}

export interface TimezoneApplyResult {
  changed_count: number
  batch_id: string | null
}

export function previewTimezones(library_id: string): Promise<TimezonePreview> {
  return apiFetch('/api/v1/metadata/batch/recalculate-timezones/preview', {
    method: 'POST',
    body: JSON.stringify({ library_id })
  })
}

export function applyTimezones(
  library_id: string,
  preview_token: string
): Promise<TimezoneApplyResult> {
  return apiFetch('/api/v1/metadata/batch/recalculate-timezones', {
    method: 'POST',
    body: JSON.stringify({ library_id, preview_token })
  })
}

export function copyLocation(
  source_asset_id: string,
  target_asset_ids: string[]
): Promise<{ batch_id: string }> {
  return apiFetch('/api/v1/metadata/batch/copy-location', {
    method: 'POST',
    body: JSON.stringify({ source_asset_id, target_asset_ids })
  })
}

export function importGpx(
  asset_ids: string[],
  gpx: string,
  tolerance_minutes?: number
): Promise<{ batch_id: string }> {
  return apiFetch('/api/v1/metadata/batch/import-gpx', {
    method: 'POST',
    body: JSON.stringify({ asset_ids, gpx, tolerance_minutes })
  })
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
