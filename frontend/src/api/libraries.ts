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

export function fetchLibraries(): Promise<Library[]> {
  return apiFetch('/api/v1/libraries')
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

/** §47, azione `"retry-connection"`: verifica se il percorso di rete della
 * libreria è di nuovo raggiungibile (`LibraryRepo::probe`, `crates/
 * keeppix-db/src/libraries.rs:180-193`) e aggiorna `status` di
 * conseguenza. A differenza del mockup — dove "il tentativo riesce
 * sempre", nessun ramo "riprovato e ancora offline" — questa chiamata
 * **non fallisce mai** se la libreria è ancora irraggiungibile: risponde
 * comunque `200` con `status:'offline'` invariato. Il chiamante deve
 * quindi leggere il campo `status` della risposta, non solo l'esito
 * della promise, per sapere se la riconnessione è riuscita davvero. */
export function probeLibrary(libraryId: string): Promise<Library> {
  return apiFetch(`/api/v1/libraries/${libraryId}/probe`, { method: 'POST' })
}
