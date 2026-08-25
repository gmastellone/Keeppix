import type { FolderView } from '@/api/folders'

/**
 * Briciola di **nomi** di cartella, non un percorso su disco: il backend
 * non espone un percorso assoluto per una cartella qualunque (solo
 * `Library.root_path`, la radice della libreria intera —
 * `FolderView` porta solo `name`/`parent_id`/`depth`). Risale `parent_id`
 * dentro `byId` fino alla radice. `null` se `folderId` non è (più) in
 * `byId` — es. la cartella configurata è stata spostata o cancellata.
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
