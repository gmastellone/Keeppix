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

/** All visible folders, flattened (no `?roots=true`, so `FolderRepo::tree`
 * instead of `FolderRepo::roots`) — unlike `fetchTree()` (roots only, for
 * the lazy tree in `FoldersView`/`SharesView`), the search bar's "Folder"
 * group needs to filter by any subfolder, not just import roots. */
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
