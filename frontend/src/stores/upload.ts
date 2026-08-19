import { defineStore } from 'pinia'
import { ref } from 'vue'

import * as uploadApi from '@/api/upload'
import { ApiProblem } from '@/api/client'

export type UploadStatus = 'queued' | 'uploading' | 'paused' | 'done' | 'error' | 'skipped'
export type CollisionOutcome = 'created' | 'skipped_duplicate' | 'renamed'

export interface UploadSessionState {
  /** id di sessione lato server una volta creata; un placeholder locale
   * (`local:...`) fino a quel momento, non persistito con quello stato. */
  id: string
  filename: string
  targetFolderId: string
  expectedSize: number
  receivedBytes: number
  status: UploadStatus
  collision?: CollisionOutcome
  existingAssetId?: string
  /** Nome effettivo sul server dopo la finalizzazione, se diverso
   * dall'originale (`collision === 'renamed'`). */
  savedFilename?: string
  /** Chiave i18n dell'errore, mai testo già tradotto: la vista traduce. */
  error?: string
}

interface PersistedSession {
  id: string
  filename: string
  targetFolderId: string
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
 * Store del pannello di upload persistente (Fase 5, Task 3). Le sessioni
 * vivono in memoria per la UI reattiva e sono duplicate in `localStorage`
 * (solo i campi serializzabili: niente `File`, che non sopravvive a un
 * refresh) per essere ritrovate da `initFromStorage()` al prossimo avvio.
 *
 * Concorrenza: al più `MAX_CONCURRENT_UPLOADS` file in avanzamento insieme,
 * non chunk in parallelo — i chunk di un singolo file sono sequenziali. Le
 * chiamate che avviano un upload (`pump`) sono differite con `setTimeout`,
 * non sincrone: così `addFiles` può restituire lo stato "queued" a chi
 * osserva subito dopo, senza una gara con l'avvio effettivo.
 */
export const useUploadStore = defineStore('upload', () => {
  const sessions = ref<UploadSessionState[]>([])

  /** File in memoria per id di sessione — mai persistiti, mai in `sessions`. */
  const files = new Map<string, File>()
  const chunkSizes = new Map<string, number>()
  let activeCount = 0

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
   * Legge `localStorage` e per ogni sessione non ancora finita chiede al
   * server l'offset vero con `HEAD` — mai quello salvato in locale, che può
   * essere scaduto. `410` è una sessione scaduta lato server (spec §1.3): si
   * segna "error", non si nasconde.
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
   * Pre-check in batch (spec §1.2), poi accoda solo i file che il server non
   * ha già. I duplicati restano visibili nel pannello come "skipped" —
   * l'utente vede che non sono stati ignorati per errore.
   */
  async function addFiles(fileList: File[], folderId: string): Promise<void> {
    if (fileList.length === 0) return

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

  function schedulePump(): void {
    setTimeout(() => pump(), 0)
  }

  /** Avvia fino a `MAX_CONCURRENT_UPLOADS` upload in coda, tutti insieme. */
  function pump(): void {
    while (activeCount < MAX_CONCURRENT_UPLOADS) {
      const next = sessions.value.find((s) => s.status === 'queued')
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

    try {
      let remoteId = session.id.startsWith('local:') ? undefined : session.id
      if (!remoteId) {
        const created = await uploadApi.createSession({
          target_folder_id: session.targetFolderId,
          filename: session.filename,
          expected_size: session.expectedSize
        })
        remoteId = created.id
        files.delete(id)
        files.set(remoteId, file)
        session.id = remoteId
        persist()
      }

      let chunkSize = chunkSizes.get(remoteId) ?? START_CHUNK_BYTES
      while (session.receivedBytes < session.expectedSize) {
        if (session.status === 'paused') return

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
    } catch (err) {
      session.status = 'error'
      session.error = err instanceof ApiProblem ? err.type : 'upload.errors.unknown'
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
      session.error = 'upload.errors.missingFile'
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

  function removeCompleted(): void {
    sessions.value = sessions.value.filter((s) => s.status !== 'done' && s.status !== 'skipped')
    persist()
  }

  return {
    sessions,
    initFromStorage,
    addFiles,
    pause,
    resume,
    retry,
    removeCompleted
  }
})
