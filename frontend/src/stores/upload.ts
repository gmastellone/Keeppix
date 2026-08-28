import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import * as uploadApi from '@/api/upload'
import { classifyFiles } from '@/upload/classify'

export type UploadStatus = 'queued' | 'uploading' | 'paused' | 'done' | 'error' | 'skipped'
export type CollisionOutcome = 'created' | 'skipped_duplicate' | 'renamed'

export interface UploadSessionState {
  /** Server-side session id once created; a local placeholder (`local:...`)
   * until then, never persisted with that placeholder state. */
  id: string
  filename: string
  /** `null` = destination not chosen yet (see `addSharedFiles`): the
   * session stays "queued" but `pump()` never starts it until a folder is
   * assigned — there is no UI in this codebase yet that assigns one
   * automatically. */
  targetFolderId: string | null
  expectedSize: number
  receivedBytes: number
  status: UploadStatus
  collision?: CollisionOutcome
  existingAssetId?: string
  /** Actual filename on the server after finalization, if different from
   * the original (`collision === 'renamed'`). */
  savedFilename?: string
  /** i18n key for the error, never pre-translated text: the view
   * translates it. */
  error?: string
}

interface PersistedSession {
  id: string
  filename: string
  targetFolderId: string | null
  expectedSize: number
  receivedBytes: number
  status: UploadStatus
}

const STORAGE_KEY = 'keeppix.upload.sessions'
const MAX_CONCURRENT_UPLOADS = 3
const MIN_CHUNK_BYTES = 1 * 1024 * 1024
const MAX_CHUNK_BYTES = 8 * 1024 * 1024
const START_CHUNK_BYTES = MAX_CHUNK_BYTES

let placeholderCounter = 0

function nextPlaceholderId(): string {
  placeholderCounter += 1
  return `local:${Date.now()}:${placeholderCounter}`
}

function readStorage(): PersistedSession[] {
  const raw = localStorage.getItem(STORAGE_KEY)
  if (!raw) return []
  try {
    const parsed = JSON.parse(raw)
    return Array.isArray(parsed) ? (parsed as PersistedSession[]) : []
  } catch {
    return []
  }
}

/**
 * Store for the persistent upload panel. Sessions live in memory for the
 * reactive UI and are mirrored to `localStorage` (only the serializable
 * fields — no `File`, which doesn't survive a refresh) so they can be
 * picked back up by `initFromStorage()` on the next app start.
 *
 * Concurrency: at most `MAX_CONCURRENT_UPLOADS` files in flight at once,
 * not chunks in parallel — a single file's chunks are sequential. Calls
 * that start an upload (`pump`) are deferred with `setTimeout` rather than
 * synchronous, so `addFiles` can return the "queued" state to an immediate
 * observer without racing the actual start.
 */
export const useUploadStore = defineStore('upload', () => {
  const sessions = ref<UploadSessionState[]>([])
  /** Names of files rejected at the entry point — never persisted: only
   * the text for the panel's rejection block, not the `File` objects
   * themselves (they're not needed, they never enter the queue). These
   * accumulate across successive drag/drop or picker selections, like
   * `sessions`. */
  const rejectedRaw = ref<string[]>([])
  const rejectedUnsupported = ref<string[]>([])

  /** Files in memory keyed by session id — never persisted, never in
   * `sessions`. */
  const files = new Map<string, File>()
  const chunkSizes = new Map<string, number>()
  /** blake3 hash already computed by the pre-check, keyed by local
   * session id — avoids asking the browser for the file again for
   * `expected_hash`. */
  const expectedHashes = new Map<string, string>()
  /** Ids (local or server-side, whichever is current in `session.id`)
   * marked by `cancelAll()`: the chunk loop in `runUpload` checks this
   * set on every iteration and stops, even if the session has already
   * been removed from `sessions` (the in-flight promise still holds its
   * own reference to the object — removing it from the array doesn't
   * interrupt it by itself). */
  const cancelledIds = new Set<string>()
  let activeCount = 0

  /**
   * Destination precedence: if a queue is still in progress (an
   * unfinished session already has a folder assigned), its destination
   * carries over to files added afterward without asking again — files
   * already in flight are never redirected. If no active session has a
   * folder yet, there's nothing to inherit: a caller of `addFiles`
   * without an explicit context is left with `null` (the destination
   * chip will prompt for it).
   */
  const stickyDestination = computed<string | null>(() => {
    const active = sessions.value.find(
      (s) => s.targetFolderId !== null && (s.status === 'queued' || s.status === 'uploading' || s.status === 'paused')
    )
    return active?.targetFolderId ?? null
  })

  /** Strip and panel: no destination resolved for the current batch
   * **and** something is still pending. Checking only "queued" sessions
   * isn't enough — a session without a folder can end up "paused" too
   * (paused before `pump()` picked it up; `pauseAll`/`pause` don't check
   * the folder) — a real bug found while writing the panel tests
   * (`UploadPanel.spec.ts`), not assumed. */
  const needsDestination = computed(
    () =>
      stickyDestination.value === null &&
      sessions.value.some((s) => s.status === 'queued' || s.status === 'uploading' || s.status === 'paused')
  )

  /** Panel open/closed state: read and written both by the strip
   * (`UploadQueueStrip.vue`) and by the panel's own close button — it
   * lives here, not in a component, because both need to see it. */
  const panelOpen = ref(false)

  function togglePanel(): void {
    panelOpen.value = !panelOpen.value
  }

  function persist(): void {
    const toPersist: PersistedSession[] = sessions.value
      .filter((s) => s.status !== 'skipped' && !s.id.startsWith('local:'))
      .map((s) => ({
        id: s.id,
        filename: s.filename,
        targetFolderId: s.targetFolderId,
        expectedSize: s.expectedSize,
        receivedBytes: s.receivedBytes,
        status: s.status
      }))
    localStorage.setItem(STORAGE_KEY, JSON.stringify(toPersist))
  }

  function findSession(id: string): UploadSessionState | undefined {
    return sessions.value.find((s) => s.id === id)
  }

  /**
   * Reads `localStorage` and, for each unfinished session, asks the
   * server for the real offset via `HEAD` — never the one saved locally,
   * which may be stale. `410` means the session expired server-side: it's
   * marked "error", not silently dropped.
   */
  async function initFromStorage(): Promise<void> {
    const persisted = readStorage()
    for (const p of persisted) {
      if (p.status === 'done') {
        sessions.value.push({ ...p })
        continue
      }
      try {
        const head = await uploadApi.headSession(p.id)
        if (head.kind === 'gone') {
          sessions.value.push({ ...p, status: 'error', error: 'upload.errors.expired' })
        } else {
          sessions.value.push({
            ...p,
            receivedBytes: head.receivedBytes,
            status: 'paused'
          })
        }
      } catch {
        sessions.value.push({ ...p, status: 'error', error: 'upload.errors.expired' })
      }
    }
    persist()
  }

  /**
   * Batch pre-check, then only queues files the server doesn't already
   * have. Duplicates stay visible in the panel as "skipped" — so the user
   * can see they weren't silently dropped due to an error.
   *
   * `explicitFolderId` covers the highest-precedence case (explicit
   * context of the command that was invoked): when it's `null` — always,
   * today, since no view currently exposes an observable "inside a
   * folder" context — the destination falls back to `stickyDestination`.
   */
  async function addFiles(fileList: File[], explicitFolderId: string | null = null): Promise<void> {
    if (fileList.length === 0) return
    const folderId = explicitFolderId ?? stickyDestination.value

    const hashes = await Promise.all(fileList.map((file) => uploadApi.hashFile(file)))
    const { unknown_hashes: unknownHashes } = await uploadApi.checkHashes(hashes)
    const unknownSet = new Set(unknownHashes)

    fileList.forEach((file, i) => {
      const hash = hashes[i]
      if (!unknownSet.has(hash)) {
        sessions.value.push({
          id: nextPlaceholderId(),
          filename: file.name,
          targetFolderId: folderId,
          expectedSize: file.size,
          receivedBytes: 0,
          status: 'skipped',
          collision: 'skipped_duplicate'
        })
        return
      }

      const localId = nextPlaceholderId()
      files.set(localId, file)
      expectedHashes.set(localId, hash)
      sessions.value.push({
        id: localId,
        filename: file.name,
        targetFolderId: folderId,
        expectedSize: file.size,
        receivedBytes: 0,
        status: 'queued'
      })
    })

    persist()
    schedulePump()
  }

  /**
   * Used by the `/share-target` view: receives files shared from the OS
   * (e.g. "Share -> Keeppix" from the gallery) and queues them like a
   * normal upload. No explicit context is possible from there (the OS
   * doesn't know about Keeppix folders): `addFiles` falls back to
   * `stickyDestination`, and if that's empty too, the session stays
   * "queued" without a folder until `setDestination()` assigns one — via
   * the panel's destination chip.
   */
  async function addSharedFiles(files: File[]): Promise<void> {
    await addFiles(files, null)
  }

  /**
   * Common entry point for drag-and-drop, the "Upload" command, and the
   * mobile `+` button: splits the batch with `classifyFiles` before
   * touching the queue — RAW files and unsupported formats never become
   * sessions, they only end up as names in the rejection block. Rejecting
   * the entire drop would be hostile to the user: only the accepted files
   * go through `addFiles`, the rest don't block them.
   *
   * Two behavior details worth calling out:
   * - Rejections **replace** the previous batch's, they don't
   *   accumulate — a new drop only reports on itself, not the history of
   *   every drop before it.
   * - Adding files **always opens the panel**, even for a batch of
   *   nothing but rejections: that's how the RAW rejection block stays
   *   visible without the user having to click the strip — the strip
   *   itself only reacts to `items` (`sessions` here), never to
   *   rejections.
   */
  async function addFilesFromPicker(fileList: File[], explicitFolderId: string | null = null): Promise<void> {
    const { accepted, rejectedRaw: raw, rejectedUnsupported: unsupported } = classifyFiles(fileList)
    rejectedRaw.value = raw.map((f) => f.name)
    rejectedUnsupported.value = unsupported.map((f) => f.name)
    panelOpen.value = true
    await addFiles(accepted, explicitFolderId)
  }

  function schedulePump(): void {
    setTimeout(() => pump(), 0)
  }

  /**
   * Starts up to `MAX_CONCURRENT_UPLOADS` queued uploads at once. A
   * "queued" session without a `targetFolderId` stays visible but is
   * never picked up until `setDestination()` assigns one.
   */
  function pump(): void {
    while (activeCount < MAX_CONCURRENT_UPLOADS) {
      const next = sessions.value.find((s) => s.status === 'queued' && s.targetFolderId !== null)
      if (!next) return
      activeCount += 1
      next.status = 'uploading'
      void runUpload(next.id).finally(() => {
        activeCount -= 1
        pump()
      })
    }
  }

  async function runUpload(id: string): Promise<void> {
    const session = findSession(id)
    if (!session) return
    const file = files.get(id)
    if (!file) {
      session.status = 'error'
      session.error = 'upload.errors.missingFile'
      persist()
      return
    }

    const targetFolderId = session.targetFolderId
    if (!targetFolderId) {
      // Defensive: `pump()` should never start a session without a
      // folder, but `session.targetFolderId`'s type is still nullable
      // here, so this check also satisfies TypeScript.
      session.status = 'error'
      session.error = 'upload.errors.missingFolder'
      persist()
      return
    }

    try {
      let remoteId = session.id.startsWith('local:') ? undefined : session.id
      if (!remoteId) {
        const created = await uploadApi.createSession({
          target_folder_id: targetFolderId,
          filename: session.filename,
          expected_size: session.expectedSize,
          expected_hash: expectedHashes.get(id)
        })
        remoteId = created.id
        files.delete(id)
        files.set(remoteId, file)
        expectedHashes.delete(id)
        session.id = remoteId
        persist()
      }

      let chunkSize = chunkSizes.get(remoteId) ?? START_CHUNK_BYTES
      while (session.receivedBytes < session.expectedSize) {
        if (session.status === 'paused') return
        // `cancelAll()` can't abort a `fetch()` already in flight (no
        // `AbortController` in `api/upload.ts`), but it can stop the next
        // iteration — the `session` object stays valid even after being
        // removed from `sessions.value`, so the check is on its id, not
        // on its presence in the array.
        if (cancelledIds.has(session.id)) return

        const end = Math.min(session.receivedBytes + chunkSize, session.expectedSize)
        const chunk = file.slice(session.receivedBytes, end)
        const buffer = await chunk.arrayBuffer()
        const checksum = await uploadApi.hashBytes(buffer)

        try {
          const result = await uploadApi.patchChunk(
            remoteId,
            session.receivedBytes,
            checksum,
            chunk
          )
          chunkSize = Math.min(chunkSize * 2, MAX_CHUNK_BYTES)
          chunkSizes.set(remoteId, chunkSize)

          if (result.status === 'finalized') {
            session.receivedBytes = session.expectedSize
            session.status = 'done'
            session.collision = result.collision
            session.existingAssetId = result.existing_asset_id
            session.savedFilename = result.filename
            files.delete(remoteId)
          } else {
            session.receivedBytes = result.receivedBytes
          }
          persist()
        } catch (err) {
          chunkSize = Math.max(Math.floor(chunkSize / 2), MIN_CHUNK_BYTES)
          chunkSizes.set(remoteId, chunkSize)
          throw err
        }
      }
    } catch {
      // `session.error` is always an i18n key, never raw text from the
      // backend (`ApiProblem.type`, e.g. `keeppix/some-error`): the view
      // translates `session.error` with `t()`, and a string with no
      // matching entry in `en.json`/`it.json` would silently render
      // broken.
      session.status = 'error'
      session.error = 'upload.errors.unknown'
      persist()
    }
  }

  function pause(id: string): void {
    const session = findSession(id)
    if (!session) return
    if (session.status === 'queued' || session.status === 'uploading') {
      session.status = 'paused'
      persist()
    }
  }

  function resume(id: string): void {
    const session = findSession(id)
    if (!session) return
    if (session.status !== 'paused' && session.status !== 'error') return
    if (!files.has(session.id)) {
      // A session restored from `localStorage` never has its `File`
      // object (it doesn't survive a refresh): it stays visible as
      // "error" rather than silently "paused", so `UploadPanel` shows the
      // user a message instead of a stalled panel with no explanation.
      session.status = 'error'
      session.error = 'upload.errors.missingFile'
      persist()
      return
    }
    session.status = 'queued'
    session.error = undefined
    persist()
    schedulePump()
  }

  function retry(id: string): void {
    resume(id)
  }

  /**
   * Removes done, skipped, **and errored** sessions — all three, not just
   * the first two. A previous implementation left errored sessions in the
   * queue even after "Clear completed", so the panel auto-closing on an
   * empty queue would never happen while a single error remained.
   */
  function removeCompleted(): void {
    sessions.value = sessions.value.filter(
      (s) => s.status !== 'done' && s.status !== 'skipped' && s.status !== 'error'
    )
    persist()
  }

  /** Assigns the folder to every still-queued session without a
   * destination, then starts the queue — the only action that unblocks
   * it. Doesn't touch sessions that already have a destination: files
   * already in flight are never redirected. */
  function setDestination(folderId: string): void {
    let assigned = false
    for (const session of sessions.value) {
      if (session.targetFolderId === null && session.status === 'queued') {
        session.targetFolderId = folderId
        assigned = true
      }
    }
    persist()
    if (assigned) schedulePump()
  }

  /** Queue "Pause" command: stops the whole queue, not a single
   * session — the in-progress file transitions to "paused" (the chunk
   * loop in `runUpload` notices on its next iteration). */
  function pauseAll(): void {
    for (const session of sessions.value) {
      if (session.status === 'queued' || session.status === 'uploading') {
        session.status = 'paused'
      }
    }
    persist()
  }

  /** Queue "Resume" command. Same caution as `resume()`: a session
   * restored from `localStorage` never has its `File` in memory. */
  function resumeAll(): void {
    let didResume = false
    for (const session of sessions.value) {
      if (session.status !== 'paused') continue
      if (!files.has(session.id)) {
        session.status = 'error'
        session.error = 'upload.errors.missingFile'
        continue
      }
      session.status = 'queued'
      session.error = undefined
      didResume = true
    }
    persist()
    if (didResume) schedulePump()
  }

  /** "Cancel all": clears the queue and resets the destination — the
   * second part isn't a separate action, it's a consequence: with
   * `sessions` empty, `stickyDestination` naturally goes back to `null`.
   * Can't abort a `fetch()` already in flight (see the `cancelledIds`
   * comment in `runUpload`), only prevent the next chunk. */
  function cancelAll(): void {
    for (const session of sessions.value) {
      cancelledIds.add(session.id)
      files.delete(session.id)
      chunkSizes.delete(session.id)
      expectedHashes.delete(session.id)
    }
    sessions.value = []
    rejectedRaw.value = []
    rejectedUnsupported.value = []
    persist()
  }

  return {
    sessions,
    rejectedRaw,
    rejectedUnsupported,
    stickyDestination,
    needsDestination,
    panelOpen,
    togglePanel,
    initFromStorage,
    addFiles,
    addFilesFromPicker,
    addSharedFiles,
    pause,
    resume,
    retry,
    removeCompleted,
    setDestination,
    pauseAll,
    resumeAll,
    cancelAll
  }
})
