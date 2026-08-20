const SHARE_CACHE_NAME = 'keeppix-share-target-v1'
const SHARE_INDEX_KEY = '/__share-target-index__'

interface ShareTargetIndexEntry {
  key: string
  name: string
  type: string
}

/**
 * Legge (e consuma) i file lasciati dal service worker in Cache Storage
 * dopo un "Condividi -> Keeppix" dalla galleria del telefono (vedi
 * `public/sw.js`). Il nome della cache e le chiavi sono duplicati lì:
 * questo file non può importarli, perché il service worker è servito
 * com'è da `/public` e non passa dal bundler.
 *
 * Ritorna `[]` (mai un errore) sia quando non c'è nulla in coda, sia
 * quando `caches` non esiste (browser senza Cache Storage, o ambiente di
 * test jsdom): la vista che chiama questa funzione tratta "nessun file" e
 * "impossibile leggerli" allo stesso modo.
 */
export async function readAndClearSharedFiles(): Promise<File[]> {
  if (typeof caches === 'undefined') return []

  try {
    const cache = await caches.open(SHARE_CACHE_NAME)
    const indexResponse = await cache.match(SHARE_INDEX_KEY)
    if (!indexResponse) return []

    const index = (await indexResponse.json()) as ShareTargetIndexEntry[]
    const files: File[] = []

    for (const entry of index) {
      const fileResponse = await cache.match(entry.key)
      if (!fileResponse) continue
      const blob = await fileResponse.blob()
      files.push(new File([blob], entry.name, { type: entry.type }))
      await cache.delete(entry.key)
    }

    await cache.delete(SHARE_INDEX_KEY)
    return files
  } catch {
    return []
  }
}
