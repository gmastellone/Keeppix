import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@/api/upload', () => ({
  checkHashes: vi.fn(),
  createSession: vi.fn(),
  headSession: vi.fn(),
  patchChunk: vi.fn(),
  hashBytes: vi.fn(),
  hashFile: vi.fn()
}))

const uploadApi = await import('@/api/upload')
const { useUploadStore } = await import('./upload')

function file(name: string, size = 10): File {
  return new File([new Uint8Array(size)], name, { type: 'image/jpeg' })
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  localStorage.clear()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('pannello di upload persistente — store', () => {
  it('pre_check_skips_files_already_in_library', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-b.jpg'] })

    const store = useUploadStore()
    await store.addFiles([file('a.jpg'), file('b.jpg')], 'folder-1')

    const queued = store.sessions.filter((s) => s.status === 'queued')
    expect(queued).toHaveLength(1)
    expect(queued[0].filename).toBe('b.jpg')

    const skipped = store.sessions.filter((s) => s.status === 'skipped')
    expect(skipped).toHaveLength(1)
    expect(skipped[0].filename).toBe('a.jpg')
    expect(skipped[0].collision).toBe('skipped_duplicate')
  })

  it('resumes_session_from_localstorage_on_init', async () => {
    localStorage.setItem(
      'keeppix.upload.sessions',
      JSON.stringify([
        {
          id: 'session-1',
          filename: 'c.jpg',
          targetFolderId: 'folder-1',
          expectedSize: 2048,
          receivedBytes: 0,
          status: 'uploading'
        }
      ])
    )
    vi.mocked(uploadApi.headSession).mockResolvedValue({ kind: 'ok', receivedBytes: 1024 })

    const store = useUploadStore()
    await store.initFromStorage()

    expect(uploadApi.headSession).toHaveBeenCalledWith('session-1')
    const session = store.sessions.find((s) => s.id === 'session-1')
    expect(session?.receivedBytes).toBe(1024)
    expect(session?.status).toBe('paused')
  })

  it('marks_session_gone_when_head_returns_410', async () => {
    localStorage.setItem(
      'keeppix.upload.sessions',
      JSON.stringify([
        {
          id: 'session-2',
          filename: 'd.jpg',
          targetFolderId: 'folder-1',
          expectedSize: 2048,
          receivedBytes: 512,
          status: 'paused'
        }
      ])
    )
    vi.mocked(uploadApi.headSession).mockResolvedValue({ kind: 'gone' })

    const store = useUploadStore()
    await store.initFromStorage()

    const session = store.sessions.find((s) => s.id === 'session-2')
    expect(session?.status).toBe('error')
    expect(session?.error).toBe('upload.errors.expired')
  })

  it('two_uploads_run_concurrently_up_to_three', async () => {
    vi.useFakeTimers()
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({
      unknown_hashes: ['hash-1.jpg', 'hash-2.jpg', 'hash-3.jpg', 'hash-4.jpg']
    })
    // Non risolve mai: basta a osservare lo stato "uploading" senza dover
    // simulare l'intero ciclo di chunk.
    vi.mocked(uploadApi.createSession).mockReturnValue(new Promise(() => {}))

    const store = useUploadStore()
    await store.addFiles(
      [file('1.jpg'), file('2.jpg'), file('3.jpg'), file('4.jpg')],
      'folder-1'
    )

    // L'avvio è differito (schedulePump): appena dopo addFiles, nulla è
    // ancora partito.
    expect(store.sessions.filter((s) => s.status === 'uploading')).toHaveLength(0)

    await vi.runOnlyPendingTimersAsync()

    expect(store.sessions.filter((s) => s.status === 'uploading')).toHaveLength(3)
    expect(store.sessions.filter((s) => s.status === 'queued')).toHaveLength(1)
  })

  it('shared_files_are_queued_for_upload', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-shared.jpg'] })

    const store = useUploadStore()
    await store.addSharedFiles([file('shared.jpg')])

    const queued = store.sessions.filter((s) => s.status === 'queued')
    expect(queued).toHaveLength(1)
    expect(queued[0].filename).toBe('shared.jpg')
    expect(queued[0].targetFolderId).toBeNull()
  })
})

describe('destinazione (§5, "Ordine di precedenza")', () => {
  it('a file added with no explicit context, and no queue in flight, stays destinationless', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg'] })

    const store = useUploadStore()
    await store.addFiles([file('a.jpg')])

    expect(store.sessions[0].targetFolderId).toBeNull()
    expect(store.sessions[0].status).toBe('queued')
  })

  it('setDestination assigns every destinationless queued session and starts the queue', async () => {
    vi.useFakeTimers()
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg', 'hash-b.jpg'] })
    vi.mocked(uploadApi.createSession).mockReturnValue(new Promise(() => {}))

    const store = useUploadStore()
    await store.addFiles([file('a.jpg'), file('b.jpg')])
    expect(store.sessions.every((s) => s.targetFolderId === null)).toBe(true)

    store.setDestination('folder-9')
    expect(store.sessions.every((s) => s.targetFolderId === 'folder-9')).toBe(true)

    await vi.runOnlyPendingTimersAsync()
    expect(store.sessions.filter((s) => s.status === 'uploading')).toHaveLength(2)
  })

  it('a file added while a destination-resolved queue is in flight inherits that destination — rule 3, "non si ridirigono file già partiti"', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg', 'hash-b.jpg'] })
    vi.mocked(uploadApi.createSession).mockReturnValue(new Promise(() => {}))

    const store = useUploadStore()
    await store.addFiles([file('a.jpg')], 'folder-1')
    await store.addFiles([file('b.jpg')])

    expect(store.sessions[1].targetFolderId).toBe('folder-1')
  })

  it('a file added once the previous queue has fully concluded does not inherit a stale destination — rule 2', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg'] })

    const store = useUploadStore()
    // "a.jpg" arriva già completato (nessuna sessione attiva con quella
    // cartella): simula una coda precedente conclusa senza dover rigirare
    // l'intero ciclo di chunk.
    store.sessions.push({
      id: 'done-1',
      filename: 'old.jpg',
      targetFolderId: 'folder-old',
      expectedSize: 10,
      receivedBytes: 10,
      status: 'done'
    })

    await store.addFiles([file('a.jpg')])
    expect(store.sessions.at(-1)?.targetFolderId).toBeNull()
  })
})

describe('comandi di coda (§6.4)', () => {
  it('pauseAll stops every queued/uploading session, leaving completed ones alone', async () => {
    vi.useFakeTimers()
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg', 'hash-b.jpg'] })
    vi.mocked(uploadApi.createSession).mockReturnValue(new Promise(() => {}))

    const store = useUploadStore()
    await store.addFiles([file('a.jpg'), file('b.jpg')], 'folder-1')
    await vi.runOnlyPendingTimersAsync()
    expect(store.sessions.filter((s) => s.status === 'uploading')).toHaveLength(2)

    store.pauseAll()
    expect(store.sessions.every((s) => s.status === 'paused')).toBe(true)
  })

  it('resumeAll requeues every paused session that still has its File in memory', async () => {
    vi.useFakeTimers()
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg'] })
    vi.mocked(uploadApi.createSession).mockReturnValue(new Promise(() => {}))

    const store = useUploadStore()
    await store.addFiles([file('a.jpg')], 'folder-1')
    await vi.runOnlyPendingTimersAsync()
    store.pauseAll()
    expect(store.sessions[0].status).toBe('paused')

    store.resumeAll()
    expect(store.sessions[0].status).toBe('queued')
    await vi.runOnlyPendingTimersAsync()
    expect(store.sessions[0].status).toBe('uploading')
  })

  it('resumeAll marks a session lost to a refresh as an error instead of a silently stuck "paused"', () => {
    const store = useUploadStore()
    store.sessions.push({
      id: 'from-storage',
      filename: 'c.jpg',
      targetFolderId: 'folder-1',
      expectedSize: 1024,
      receivedBytes: 100,
      status: 'paused'
    })

    store.resumeAll()
    expect(store.sessions[0].status).toBe('error')
    expect(store.sessions[0].error).toBe('upload.errors.missingFile')
  })

  it('cancelAll empties the queue, which resets stickyDestination to null on its own', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg'] })
    vi.mocked(uploadApi.createSession).mockReturnValue(new Promise(() => {}))

    const store = useUploadStore()
    await store.addFiles([file('a.jpg')], 'folder-1')
    expect(store.sessions).toHaveLength(1)

    store.cancelAll()
    expect(store.sessions).toHaveLength(0)

    // Un file nuovo, senza contesto esplicito, non eredita più folder-1.
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-b.jpg'] })
    await store.addFiles([file('b.jpg')])
    expect(store.sessions[0].targetFolderId).toBeNull()
  })

  it('cancelAll also clears the rejection lists — a fresh drop starts from nothing', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg'] })
    vi.mocked(uploadApi.createSession).mockReturnValue(new Promise(() => {}))

    const store = useUploadStore()
    await store.addFilesFromPicker([file('a.jpg'), file('b.arw')], 'folder-1')
    expect(store.rejectedRaw).toEqual(['b.arw'])

    store.cancelAll()
    expect(store.rejectedRaw).toEqual([])
  })

  it('removeCompleted removes done, skipped AND error sessions — §6.4 says "concluse, saltate ed errate"', () => {
    const store = useUploadStore()
    store.sessions.push(
      { id: 'a', filename: 'a.jpg', targetFolderId: 'f', expectedSize: 1, receivedBytes: 1, status: 'done' },
      { id: 'b', filename: 'b.jpg', targetFolderId: 'f', expectedSize: 1, receivedBytes: 0, status: 'skipped' },
      { id: 'c', filename: 'c.jpg', targetFolderId: 'f', expectedSize: 1, receivedBytes: 0, status: 'error' },
      { id: 'd', filename: 'd.jpg', targetFolderId: 'f', expectedSize: 1, receivedBytes: 0, status: 'queued' }
    )

    store.removeCompleted()
    expect(store.sessions.map((s) => s.id)).toEqual(['d'])
  })
})

describe('addFilesFromPicker — punto d\'ingresso comune al trascinamento, "Carica" e il "+" mobile (§4)', () => {
  it('classifies before queuing: RAW and unsupported files never become sessions', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg', 'hash-c.mp4'] })

    const store = useUploadStore()
    await store.addFilesFromPicker(
      [file('a.jpg'), file('b.arw'), file('c.mp4'), file('d.txt')],
      'folder-1'
    )

    expect(store.sessions.map((s) => s.filename)).toEqual(['a.jpg', 'c.mp4'])
    expect(store.rejectedRaw).toEqual(['b.arw'])
    expect(store.rejectedUnsupported).toEqual(['d.txt'])
  })

  it('a batch of only rejected files never calls the hash pre-check at all', async () => {
    const store = useUploadStore()
    await store.addFilesFromPicker([file('a.arw'), file('b.txt')], 'folder-1')

    expect(uploadApi.hashFile).not.toHaveBeenCalled()
    expect(store.sessions).toHaveLength(0)
    expect(store.rejectedRaw).toEqual(['a.arw'])
    expect(store.rejectedUnsupported).toEqual(['b.txt'])
  })

  it('rejections REPLACE the previous batch\'s, they never accumulate — verified against uploadAddFiles() (mockup riga 2754)', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: [] })

    const store = useUploadStore()
    await store.addFilesFromPicker([file('a.arw')], 'folder-1')
    expect(store.rejectedRaw).toEqual(['a.arw'])

    await store.addFilesFromPicker([file('b.arw')], 'folder-1')
    expect(store.rejectedRaw).toEqual(['b.arw'])

    // Un lotto tutto accettato azzera anche i rifiuti del lotto precedente.
    await store.addFilesFromPicker([file('c.jpg')], 'folder-1')
    expect(store.rejectedRaw).toEqual([])
  })

  it('addFilesFromPicker always opens the panel, even for an all-rejected batch — verified against uploadAddFiles() (mockup riga 2770)', async () => {
    const store = useUploadStore()
    expect(store.panelOpen).toBe(false)

    await store.addFilesFromPicker([file('a.arw')], 'folder-1')
    expect(store.panelOpen).toBe(true)
  })
})

describe('striscia della coda (§6.1)', () => {
  it('needsDestination is true only while a session is stuck queued without a folder', async () => {
    vi.mocked(uploadApi.hashFile).mockImplementation(async (f) => `hash-${(f as File).name}`)
    vi.mocked(uploadApi.checkHashes).mockResolvedValue({ unknown_hashes: ['hash-a.jpg'] })

    const store = useUploadStore()
    expect(store.needsDestination).toBe(false)

    await store.addFiles([file('a.jpg')])
    expect(store.needsDestination).toBe(true)

    store.setDestination('folder-1')
    expect(store.needsDestination).toBe(false)
  })

  it('togglePanel flips panelOpen, shared between the strip and the panel', () => {
    const store = useUploadStore()
    expect(store.panelOpen).toBe(false)
    store.togglePanel()
    expect(store.panelOpen).toBe(true)
    store.togglePanel()
    expect(store.panelOpen).toBe(false)
  })
})
