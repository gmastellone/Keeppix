const SHARE_CACHE_NAME = 'keeppix-share-target-v1'
const SHARE_INDEX_KEY = '/__share-target-index__'

interface ShareTargetIndexEntry {
  key: string
  name: string
  type: string
}

/**
 * Reads (and consumes) the files left by the service worker in Cache
 * Storage after a "Share -> Keeppix" from the phone's gallery (see
 * `public/sw.js`). The cache name and keys are duplicated there: this file
 * can't import them, because the service worker is served as-is from
 * `/public` and doesn't go through the bundler.
 *
 * Returns `[]` (never throws) both when there's nothing queued and when
 * `caches` doesn't exist (a browser without Cache Storage, or a jsdom test
 * environment): the view calling this function treats "no files" and
 * "couldn't read them" the same way.
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
