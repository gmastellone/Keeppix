import type { FolderView } from '@/api/folders'

/**
 * A breadcrumb of folder **names**, not a disk path: the backend doesn't
 * expose an absolute path for an arbitrary folder (only `Library.root_path`,
 * the whole library's root — `FolderView` only carries
 * `name`/`parent_id`/`depth`). Walks `parent_id` up through `byId` to the
 * root. Returns `null` if `folderId` is no longer in `byId` — e.g. the
 * configured folder was moved or deleted.
 */
export function folderPathName(folderId: string, byId: Map<string, FolderView>): string | null {
  const chain: string[] = []
  let current = byId.get(folderId)
  while (current) {
    chain.unshift(current.name)
    current = current.parent_id ? byId.get(current.parent_id) : undefined
  }
  return chain.length > 0 ? chain.join(' / ') : null
}
