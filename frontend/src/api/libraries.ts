import { apiFetch } from './client'

export interface Library {
  id: string
  name: string
  owner_id: string
  root_path: string
  scan_enabled: boolean
  exclude_patterns: string[]
  status: string
  last_scan_at: string | null
  created_at: string
}

export interface LibraryPreview {
  total: number
  extensions: Record<string, number>
}

export interface CreateLibraryPayload {
  name: string
  root_path: string
  exclude_patterns?: string[]
}

export interface ScanStatus {
  library_id: string
  library_status: string
  phase: string
  asset_count: number
  job_status: string | null
  last_error: string | null
  eta_seconds: number | null
  last_scan_at: string | null
}

export function previewLibraryPath(path: string): Promise<LibraryPreview> {
  const query = new URLSearchParams({ path })
  return apiFetch(`/api/v1/libraries/preview?${query}`)
}

export function createLibrary(payload: CreateLibraryPayload): Promise<Library> {
  return apiFetch('/api/v1/libraries', {
    method: 'POST',
    body: JSON.stringify(payload)
  })
}

export function startLibraryScan(libraryId: string): Promise<{ library_id: string; status: string }> {
  return apiFetch(`/api/v1/libraries/${libraryId}/scan`, { method: 'POST' })
}

export function fetchLibraryScanStatus(libraryId: string): Promise<ScanStatus> {
  return apiFetch(`/api/v1/libraries/${libraryId}/scan`)
}
