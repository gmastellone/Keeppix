import { apiFetch } from './client'
import type { TimelineAsset } from './timeline'

export interface FolderView {
  id: string
  library_id: string
  parent_id: string | null
  name: string
  depth: number
}

export interface FolderChildren {
  folders: FolderView[]
  assets: TimelineAsset[]
}

export function fetchTree(): Promise<FolderView[]> {
  return apiFetch('/api/v1/folders/tree?roots=true')
}

/** Tutte le cartelle visibili, appiattite (senza `?roots=true`, quindi
 * `FolderRepo::tree` invece di `FolderRepo::roots`) — a differenza di
 * `fetchTree()` (solo radici, per l'albero pigro di `FoldersView`/
 * `SharesView`), il gruppo "Cartella" della barra di ricerca (Task 9,
 * §23-24) deve poter filtrare per qualunque sottocartella, non solo le
 * radici di import. */
export function fetchAllFolders(): Promise<FolderView[]> {
  return apiFetch('/api/v1/folders/tree')
}

export function fetchChildren(id: string): Promise<FolderChildren> {
  return apiFetch(`/api/v1/folders/${id}/children`)
}

export function moveFolder(id: string, parent_id: string): Promise<null> {
  return apiFetch(`/api/v1/folders/${id}`, {
    method: 'PATCH',
    body: JSON.stringify({ parent_id })
  })
}
