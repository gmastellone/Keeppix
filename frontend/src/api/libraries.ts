import { apiFetch } from './client'

export interface Library {
  id: string
  name: string
  owner_id: string
  root_path: string
  scan_enabled: boolean
  /** "Face recognition": real, but **per library** —
   * `LibraryView.faces_enabled` (`crates/keeppix-api/src/routes/
   * libraries.rs:27`), not a single instance-wide toggle. With more than
   * one library the only honest choice is to show one row per library,
   * not fake a single global switch the backend doesn't have. */
  faces_enabled: boolean
  exclude_patterns: string[]
  status: string
  last_scan_at: string | null
  created_at: string
  /** "Culling root folder": id of the lots' root folder, or `null` if
   * the owner hasn't designated one yet. */
  culling_root_folder_id: string | null
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

export interface ScanAccepted {
  library_id: string
  status: string
  /** Present only if this request is the one that actually queued the
   * job — `null` if a scan for the same library was already in progress
   * (the one sharing the `dedup_key` wins). */
  operation_id: string | null
}

export function startLibraryScan(libraryId: string): Promise<ScanAccepted> {
  return apiFetch(`/api/v1/libraries/${libraryId}/scan`, { method: 'POST' })
}

export function fetchLibraryScanStatus(libraryId: string): Promise<ScanStatus> {
  return apiFetch(`/api/v1/libraries/${libraryId}/scan`)
}

/** The `"retry-connection"` action: checks whether the library's network
 * path is reachable again (`LibraryRepo::probe`, `crates/keeppix-db/src/
 * libraries.rs:180-193`) and updates `status` accordingly. This call
 * **never fails** if the library is still unreachable: it still responds
 * `200` with `status:'offline'` unchanged. The caller must therefore
 * read the response's `status` field, not just whether the promise
 * resolved, to know whether the reconnection actually succeeded. */
export function probeLibrary(libraryId: string): Promise<Library> {
  return apiFetch(`/api/v1/libraries/${libraryId}/probe`, { method: 'POST' })
}

export interface LibraryPatch {
  name?: string
  scan_enabled?: boolean
  faces_enabled?: boolean
  exclude_patterns?: string[]
}

/** "Face recognition enabled": `PatchLibraryRequest`
 * (`routes/libraries.rs:63-68`) also accepts `faces_enabled`. */
export function patchLibrary(libraryId: string, patch: LibraryPatch): Promise<Library> {
  return apiFetch(`/api/v1/libraries/${libraryId}`, {
    method: 'PATCH',
    body: JSON.stringify(patch)
  })
}

/** "Culling root folder": a dedicated route instead of a field on
 * `patchLibrary` — `LibraryRepo::set_culling_root` requires explicit
 * owner/admin, stricter than the general `update` permission (see the
 * comment on the handler, `routes/libraries.rs`). `folderId: null`
 * clears the designated root. */
export function patchCullingRoot(libraryId: string, folderId: string | null): Promise<Library> {
  return apiFetch(`/api/v1/libraries/${libraryId}/culling-root`, {
    method: 'PATCH',
    body: JSON.stringify({ folder_id: folderId })
  })
}
