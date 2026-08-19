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
const { useUploadStore } = await import('@/stores/upload')

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
})
