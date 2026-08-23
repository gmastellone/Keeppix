import { defineStore } from 'pinia'
import { computed, ref } from 'vue'

import * as uploadApi from '@/api/upload'
import { classifyFiles } from '@/upload/classify'

export type UploadStatus = 'queued' | 'uploading' | 'paused' | 'done' | 'error' | 'skipped'
export type CollisionOutcome = 'created' | 'skipped_duplicate' | 'renamed'

export interface UploadSessionState {
  /** id di sessione lato server una volta creata; un placeholder locale
   * (`local:...`) fino a quel momento, non persistito con quello stato. */
  id: string
  filename: string
  /** `null` = destinazione non ancora scelta (vedi `addSharedFiles`): la
   * sessione resta "queued" ma `pump()` non la avvia mai finché non le
   * viene assegnata una cartella — non c'è ancora, in questo codebase,
   * un'interfaccia che gliela assegni (vedi ledger di Fase 5, Task 10). */
  targetFolderId: string | null
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
  /** Nomi dei file scartati all'ingresso (§4) — mai persistiti: solo il
   * testo per il blocco di rifiuto del pannello, non i `File` stessi (non
   * servono più, non entrano mai in coda). Si accumulano fra più
   * trascinamenti/selezioni successive, come `sessions`. */
  const rejectedRaw = ref<string[]>([])
  const rejectedUnsupported = ref<string[]>([])

  /** File in memoria per id di sessione — mai persistiti, mai in `sessions`. */
  const files = new Map<string, File>()
  const chunkSizes = new Map<string, number>()
  /** Hash blake3 già calcolato dal pre-check, per id di sessione locale —
   * evita di richiedere di nuovo il file al browser per `expected_hash`. */
  const expectedHashes = new Map<string, string>()
  /** Id (locali o del server, quello attuale di `session.id`) marcati da
   * `cancelAll()`: il ciclo a blocchi in `runUpload` li controlla a ogni
   * giro e si ferma, anche se la sessione è già stata tolta da `sessions`
   * (la promessa in corso continua a tenere il proprio riferimento
   * all'oggetto, rimuoverlo dall'array non la interrompe da sola). */
  const cancelledIds = new Set<string>()
  let activeCount = 0

  /**
   * Fase 11, sottosistema di caricamento (`caricamento-nuove-foto.md` §5,
   * "Ordine di precedenza"): se una coda è ancora in corso (una sessione
   * non ancora conclusa ha già una cartella assegnata), la sua
   * destinazione resta quella per i file aggiunti dopo, senza chiederla di
   * nuovo — "non si ridirigono file già partiti". Se nessuna sessione
   * attiva ha ancora una cartella, non c'è nulla da ereditare: chi chiama
   * `addFiles` senza un contesto esplicito resta con `null` (il chip
   * destinazione lo chiederà).
   */
  const stickyDestination = computed<string | null>(() => {
    const active = sessions.value.find(
      (s) => s.targetFolderId !== null && (s.status === 'queued' || s.status === 'uploading' || s.status === 'paused')
    )
    return active?.targetFolderId ?? null
  })

  /** Striscia della coda (`caricamento-nuove-foto.md` §6.1, `needsDest`
   * del prototipo, riga 2915): una sessione bloccata su "queued" perché
   * senza cartella — segnale più diretto della semplice assenza di
   * `stickyDestination`, che risulterebbe vuota anche a coda vuota. */
  const needsDestination = computed(() =>
    sessions.value.some((s) => s.status === 'queued' && s.targetFolderId === null)
  )

  /** Stato di apertura del pannello (§6.2): letto/scritto sia dalla
   * striscia (`UploadQueueStrip.vue`) sia dal pulsante "Chiudi" del
   * pannello stesso — vive qui, non in un componente, perché entrambi
   * devono vederlo. */
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
   *
   * `explicitFolderId` copre la prima precedenza del §5 (contesto esplicito
   * del comando premuto): quando è `null` — sempre, oggi, perché nessuna
   * vista porta un "dentro una cartella" osservabile (stesso debito già
   * dichiarato nel Task 6 per la timeline filtrata) — si ricade sulla
   * seconda/terza precedenza tramite `stickyDestination`.
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
   * Usata dalla vista `/share-target` (Fase 5, Task 10): riceve i file
   * condivisi dal sistema operativo (es. "Condividi -> Keeppix" dalla
   * galleria) e li accoda come un upload normale. Nessun contesto esplicito
   * possibile da lì (il sistema operativo non sa di cartelle Keeppix):
   * `addFiles` ricade su `stickyDestination`, e se anche quella è vuota la
   * sessione resta "queued" senza cartella finché non arriva da
   * `setDestination()` — il chip destinazione del pannello, non più "senza
   * modo di assegnarla" (Fase 5).
   */
  async function addSharedFiles(files: File[]): Promise<void> {
    await addFiles(files, null)
  }

  /**
   * Punto d'ingresso comune per trascinamento (§3.1), comando "Carica"
   * (§3.2) e `+` mobile (§3.3): divide il lotto con `classifyFiles` (§4)
   * prima di toccare la coda — i RAW e i formati non supportati non
   * diventano mai sessioni, restano solo nomi per il blocco di rifiuto.
   * "Rifiutare l'intero rilascio sarebbe ostile" (§4): solo gli accettati
   * passano da `addFiles`, il resto non blocca gli altri.
   *
   * Due dettagli verificati contro `uploadAddFiles()` del prototipo
   * (righe 2733-2773), non assunti dalla sola prosa del documento:
   * - I rifiuti **sostituiscono** quelli del lotto precedente, non si
   *   accumulano (`state.upload.rejected = ... : null`, riga 2754) — un
   *   nuovo trascinamento racconta solo se stesso, non la storia di
   *   tutti quelli prima.
   * - Aggiungere file **apre sempre il pannello** (`state.upload.open =
   *   true`, riga 2770), anche per un lotto di soli rifiuti: è così che
   *   il blocco di rifiuto RAW resta visibile senza dover cliccare la
   *   striscia — la striscia stessa, infatti, si basa solo su `items`
   *   (`sessions` qui), mai sui rifiuti (`renderUploadDock`, riga 2913).
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
   * Avvia fino a `MAX_CONCURRENT_UPLOADS` upload in coda, tutti insieme.
   * Una sessione "queued" senza `targetFolderId` resta visibile ma non
   * viene mai presa in carico finché `setDestination()` non gliene assegna
   * una — il "difetto tecnico reso spina dorsale dell'interfaccia" del
   * documento (§1).
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
      // Difensivo: `pump()` non dovrebbe mai avviare una sessione senza
      // cartella, ma il tipo di `session.targetFolderId` resta nullable
      // anche qui, quindi il controllo serve anche a soddisfare TypeScript.
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
        // `cancelAll()` non può interrompere il `fetch()` già in volo (nessun
        // `AbortController` in `api/upload.ts`), ma può fermare il prossimo
        // giro — l'oggetto `session` resta valido anche dopo essere stato
        // tolto da `sessions.value`, quindi il controllo va sul suo id, non
        // sulla presenza nell'array.
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
      // `session.error` è sempre una chiave i18n, mai testo grezzo dal
      // backend (`ApiProblem.type`, es. `keeppix/some-error`): la vista
      // traduce `session.error` con `t()` e una stringa senza corrispondenza
      // in `en.json`/`it.json` andrebbe in rendering rotto silenzioso.
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
      // Una sessione ripresa da `localStorage` non ha mai il `File` (non
      // sopravvive a un refresh): resta visibile come "error", non "paused"
      // silenzioso, così `UploadPanel` mostra il messaggio all'utente invece
      // di un pannello fermo senza spiegazione (vedi ledger di Fase 5).
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
   * §6.4: "Rimuove concluse, saltate **ed errate**" — le tre, non solo le
   * prime due. Bug reale trovato rileggendo la riga esatta del documento:
   * l'implementazione precedente lasciava le sessioni in errore in coda
   * anche dopo "Pulisci completate", quindi "a coda vuota il pannello si
   * chiude da solo" non si sarebbe mai verificato con anche un solo errore
   * presente.
   */
  function removeCompleted(): void {
    sessions.value = sessions.value.filter(
      (s) => s.status !== 'done' && s.status !== 'skipped' && s.status !== 'error'
    )
    persist()
  }

  /** §5, "stato che blocca": assegna la cartella a ogni sessione ancora in
   * coda senza una destinazione, poi avvia la coda — l'unica azione che la
   * sblocca. Non tocca le sessioni che una destinazione ce l'hanno già
   * (rule 3: "non si ridirigono file già partiti"). */
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

  /** §6.4, comando di coda `Pausa`: ferma tutta la coda, non una sessione
   * sola — il file in corso passa a "in pausa" (il ciclo a blocchi in
   * `runUpload` lo nota al prossimo giro). */
  function pauseAll(): void {
    for (const session of sessions.value) {
      if (session.status === 'queued' || session.status === 'uploading') {
        session.status = 'paused'
      }
    }
    persist()
  }

  /** §6.4, comando di coda `Riprendi`. Stessa cautela di `resume()`: una
   * sessione ripresa da `localStorage` non ha mai il `File` in memoria. */
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

  /** §6.4, `Annulla tutto`: "Svuota la coda e azzera la destinazione" — la
   * seconda parte non è un'azione a parte, è una conseguenza: con
   * `sessions` vuoto, `stickyDestination` torna da sola a `null`. Non può
   * interrompere un `fetch()` già in volo (v. commento su `cancelledIds`
   * in `runUpload`), solo impedire il prossimo blocco. */
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
