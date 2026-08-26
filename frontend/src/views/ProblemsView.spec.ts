import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import { startLiveEvents, type LiveMessage } from '@/api/events'
import type { Library, ScanAccepted } from '@/api/libraries'
import type { ProblemView, Problems } from '@/api/library'
import type { FolderChildren } from '@/api/folders'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import ProblemsView from './ProblemsView.vue'

const fetchProblemsMock = vi.fn()
const fetchLibrariesMock = vi.fn()
const probeLibraryMock = vi.fn()
const startLibraryScanMock = vi.fn()
const fetchChildrenMock = vi.fn()
const previewTimezonesMock = vi.fn()
const applyTimezonesMock = vi.fn()
const cancelOperationMock = vi.fn()

vi.mock('@/api/library', () => ({
  fetchProblems: (...args: unknown[]) => fetchProblemsMock(...args)
}))

vi.mock('@/api/libraries', () => ({
  fetchLibraries: (...args: unknown[]) => fetchLibrariesMock(...args),
  probeLibrary: (...args: unknown[]) => probeLibraryMock(...args),
  startLibraryScan: (...args: unknown[]) => startLibraryScanMock(...args)
}))

vi.mock('@/api/folders', () => ({
  fetchChildren: (...args: unknown[]) => fetchChildrenMock(...args)
}))

vi.mock('@/api/metadata', () => ({
  previewTimezones: (...args: unknown[]) => previewTimezonesMock(...args),
  applyTimezones: (...args: unknown[]) => applyTimezonesMock(...args)
}))

vi.mock('@/api/operations', () => ({
  cancelOperation: (...args: unknown[]) => cancelOperationMock(...args)
}))

vi.mock('@/api/events', () => ({
  startLiveEvents: vi.fn(() => ({ close: vi.fn() }))
}))

function scanAccepted(overrides: Partial<ScanAccepted> = {}): ScanAccepted {
  return { library_id: 'lib-1', status: 'accepted', operation_id: 'op-1', ...overrides }
}

function library(overrides: Partial<Library> = {}): Library {
  return {
    id: 'lib-1',
    name: 'Lago di Braies',
    owner_id: 'u1',
    root_path: '/data/lago-di-braies',
    scan_enabled: true,
    faces_enabled: true,
    exclude_patterns: [],
    status: 'offline',
    last_scan_at: '2026-08-20T00:00:00Z',
    created_at: '',
    culling_root_folder_id: null,
    ...overrides
  }
}

function offlineProblem(overrides: Partial<ProblemView> = {}): ProblemView {
  return {
    id: 'library-offline:lib-1',
    severity: 'error',
    title: 'Libreria offline: Lago di Braies',
    description: 'Il percorso di rete non risponde da 2 giorni.',
    library_id: 'lib-1',
    library_name: 'Lago di Braies',
    actions: [
      { action: 'retry-connection', label: 'Riprova connessione' },
      { action: 'details', label: 'Dettagli' }
    ],
    ...overrides
  }
}

function sidecarProblem(overrides: Partial<ProblemView> = {}): ProblemView {
  return {
    id: 'sidecar-permission:1',
    severity: 'warning',
    title: '3 file con sidecar XMP non scrivibile',
    description: 'Chioggia e Venezia — permessi di scrittura mancanti.',
    folder_id: 'folder-1',
    folder_name: 'Chioggia e Venezia',
    actions: [
      { action: 'view-files', label: 'Vedi i 3 file' },
      { action: 'ignore', label: 'Ignora' }
    ],
    ...overrides
  }
}

function problemsResult(problems: ProblemView[]): Problems {
  return { offline_libraries: [], failed_jobs: [], error_assets: [], problems }
}

const testUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

beforeEach(() => {
  i18n.global.locale.value = 'it'
  fetchProblemsMock.mockResolvedValue(problemsResult([]))
  fetchLibrariesMock.mockResolvedValue([])
  probeLibraryMock.mockResolvedValue(library({ status: 'active' }))
  startLibraryScanMock.mockResolvedValue(scanAccepted())
  fetchChildrenMock.mockResolvedValue({ folders: [], assets: [] } satisfies FolderChildren)
  previewTimezonesMock.mockResolvedValue({ count: 0, example: null, preview_token: 't' })
  applyTimezonesMock.mockResolvedValue({ changed_count: 0 })
  cancelOperationMock.mockResolvedValue({ succeeded: [], failed: [], batch_id: null })
})

afterEach(() => {
  vi.clearAllMocks()
})

async function mountProblems() {
  const pinia = createPinia()
  setActivePinia(pinia)
  const session = useSessionStore()
  session.user = testUser
  session.initialised = true
  session.ready = true
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/login', component: { template: '<div />' } },
      { path: '/', component: { template: '<div />' } },
      { path: '/problems', component: ProblemsView }
    ]
  })
  await router.push('/problems')
  await router.isReady()
  const wrapper = mount(ProblemsView, { global: { plugins: [router, i18n, pinia] } })
  await flushPromises()
  return { wrapper, router, session }
}

describe('ProblemsView — §47 Problemi', () => {
  it('shows an error and retry when loading fails, then loads the real empty state', async () => {
    fetchProblemsMock.mockRejectedValue(new Error('offline'))
    const { wrapper } = await mountProblems()

    expect(wrapper.text()).toContain('Si è verificato un errore imprevisto.')

    fetchProblemsMock.mockResolvedValue(problemsResult([]))
    await wrapper.get('button').trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Nessun problema rilevato')
  })

  it('su sessione scaduta (401) rimanda al login invece di mostrare un errore generico', async () => {
    fetchProblemsMock.mockRejectedValue(new ApiProblem('keeppix/unauthenticated', 'Authentication required', 401))
    const { wrapper, router, session } = await mountProblems()

    expect(router.currentRoute.value.path).toBe('/login')
    expect(session.user).toBeNull()
    expect(wrapper.text()).not.toContain('An unexpected error occurred.')
  })

  it('renders the real composed problems, primary action first then ghost actions', async () => {
    fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem(), sidecarProblem()]))
    const { wrapper } = await mountProblems()

    expect(wrapper.text()).toContain('Libreria offline: Lago di Braies')
    expect(wrapper.text()).toContain('3 file con sidecar XMP non scrivibile')
    expect(wrapper.text()).toContain('2 elementi richiedono attenzione')
    const buttons = wrapper.findAll('button').map((b) => b.text())
    expect(buttons).toEqual(
      expect.arrayContaining(['Riprova connessione', 'Dettagli', 'Vedi i 3 file', 'Ignora'])
    )
  })

  it('"Ignora" removes the problem and shows the documented toast, no confirmation', async () => {
    fetchProblemsMock.mockResolvedValue(problemsResult([sidecarProblem()]))
    const { wrapper } = await mountProblems()

    const ignoreBtn = wrapper.findAll('button').find((b) => b.text() === 'Ignora')
    await ignoreBtn!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).not.toContain('3 file con sidecar XMP non scrivibile')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Problema ignorato.')).toBe(true)
  })

  it('"Vedi i 3 file" opens the file dialog scoped to the problem\'s folder', async () => {
    fetchProblemsMock.mockResolvedValue(problemsResult([sidecarProblem()]))
    fetchChildrenMock.mockResolvedValue({
      folders: [],
      assets: [
        { id: 'a', filename: 'a.jpg', content_hash: null, taken_at_utc: null, thumbhash: null },
        { id: 'b', filename: 'b.jpg', content_hash: null, taken_at_utc: null, thumbhash: null }
      ]
    })
    const { wrapper } = await mountProblems()

    const viewBtn = wrapper.findAll('button').find((b) => b.text() === 'Vedi i 3 file')
    await viewBtn!.trigger('click')
    await flushPromises()

    expect(fetchChildrenMock).toHaveBeenCalledWith('folder-1')
    expect(document.body.textContent).toContain('a.jpg')
    expect(document.body.textContent).toContain('b.jpg')
  })

  it('"Riprova connessione": success removes the problem and shows the documented toast', async () => {
    fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem()]))
    probeLibraryMock.mockResolvedValue(library({ status: 'active' }))
    const { wrapper } = await mountProblems()

    const retryBtn = wrapper.findAll('button').find((b) => b.text() === 'Riprova connessione')
    await retryBtn!.trigger('click')
    await flushPromises()

    expect(probeLibraryMock).toHaveBeenCalledWith('lib-1')
    expect(wrapper.text()).not.toContain('Libreria offline: Lago di Braies')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message.includes('di nuovo online'))).toBe(true)
  })

  it('"Riprova connessione": a real still-offline failure keeps the problem, unlike the mockup which always succeeds', async () => {
    fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem()]))
    probeLibraryMock.mockResolvedValue(library({ status: 'offline' }))
    const { wrapper } = await mountProblems()

    const retryBtn = wrapper.findAll('button').find((b) => b.text() === 'Riprova connessione')
    await retryBtn!.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Libreria offline: Lago di Braies')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.kind === 'error' && t.message.includes('ancora offline'))).toBe(true)
  })

  it('"Dettagli" shows real library data (root_path, last_scan_at), not the mockup\'s fictional NAS text', async () => {
    fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem()]))
    fetchLibrariesMock.mockResolvedValue([library()])
    const { wrapper } = await mountProblems()

    const detailsBtn = wrapper.findAll('button').find((b) => b.text() === 'Dettagli')
    await detailsBtn!.trigger('click')
    await flushPromises()

    expect(document.body.textContent).toContain('/data/lago-di-braies')
  })

  it('keeps the real timezone tool at the bottom of the page, unrelated to §47', async () => {
    fetchLibrariesMock.mockResolvedValue([library({ id: 'lib-9', name: 'Urbino' })])
    const { wrapper } = await mountProblems()

    expect(wrapper.text()).toContain('Ricalcolo fusi orari')
    expect(wrapper.text()).toContain('Urbino')
  })

  describe('avanzamento reale della scansione dopo una riconnessione (debito wired-exceptions chiuso il 26 agosto)', () => {
    it('a successful reconnect starts a real scan and shows its progress card', async () => {
      fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem()]))
      probeLibraryMock.mockResolvedValue(library({ status: 'active' }))
      const { wrapper } = await mountProblems()

      const retryBtn = wrapper.findAll('button').find((b) => b.text() === 'Riprova connessione')
      await retryBtn!.trigger('click')
      await flushPromises()

      expect(startLibraryScanMock).toHaveBeenCalledWith('lib-1')
      expect(wrapper.text()).toContain('Scansione di Lago di Braies in corso')
    })

    it('does not show a progress card when a scan was already running (operation_id null on dedup)', async () => {
      fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem()]))
      startLibraryScanMock.mockResolvedValue(scanAccepted({ operation_id: null }))
      const { wrapper } = await mountProblems()

      const retryBtn = wrapper.findAll('button').find((b) => b.text() === 'Riprova connessione')
      await retryBtn!.trigger('click')
      await flushPromises()

      expect(wrapper.text()).not.toContain('in corso')
    })

    it('updates the progress card live from operation.progress WebSocket events', async () => {
      fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem()]))
      let onEvent: ((msg: LiveMessage) => void) | undefined
      vi.mocked(startLiveEvents).mockImplementation((cb) => {
        onEvent = cb
        return { close: vi.fn() }
      })
      const { wrapper } = await mountProblems()

      const retryBtn = wrapper.findAll('button').find((b) => b.text() === 'Riprova connessione')
      await retryBtn!.trigger('click')
      await flushPromises()

      onEvent?.({
        v: 1,
        type: 'operation.progress',
        payload: { operation_id: 'op-1', done: 40, total: 100, phase: 'scanning' }
      })
      await flushPromises()

      expect(wrapper.text()).toContain('40 di 100')
    })

    it('a "done" progress event removes the card and shows a completion toast', async () => {
      fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem()]))
      let onEvent: ((msg: LiveMessage) => void) | undefined
      vi.mocked(startLiveEvents).mockImplementation((cb) => {
        onEvent = cb
        return { close: vi.fn() }
      })
      const { wrapper } = await mountProblems()

      const retryBtn = wrapper.findAll('button').find((b) => b.text() === 'Riprova connessione')
      await retryBtn!.trigger('click')
      await flushPromises()

      onEvent?.({
        v: 1,
        type: 'operation.progress',
        payload: { operation_id: 'op-1', done: 100, total: 100, phase: 'done' }
      })
      await flushPromises()

      expect(wrapper.text()).not.toContain('in corso')
      const toast = useToastStore()
      expect(toast.toasts.some((t) => t.message.includes('completata'))).toBe(true)
    })

    it('a "failed" progress event removes the card and shows an error toast', async () => {
      fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem()]))
      let onEvent: ((msg: LiveMessage) => void) | undefined
      vi.mocked(startLiveEvents).mockImplementation((cb) => {
        onEvent = cb
        return { close: vi.fn() }
      })
      const { wrapper } = await mountProblems()

      const retryBtn = wrapper.findAll('button').find((b) => b.text() === 'Riprova connessione')
      await retryBtn!.trigger('click')
      await flushPromises()

      onEvent?.({
        v: 1,
        type: 'operation.progress',
        payload: { operation_id: 'op-1', done: 12, total: 100, phase: 'failed' }
      })
      await flushPromises()

      expect(wrapper.text()).not.toContain('in corso')
      const toast = useToastStore()
      expect(toast.toasts.some((t) => t.kind === 'error' && t.message.includes('interrotta'))).toBe(true)
    })

    it('clicking "Annulla" cancels the operation, removes the card and shows the succeeded count', async () => {
      fetchProblemsMock.mockResolvedValue(problemsResult([offlineProblem()]))
      cancelOperationMock.mockResolvedValue({ succeeded: ['a1', 'a2'], failed: [], batch_id: null })
      const { wrapper } = await mountProblems()

      const retryBtn = wrapper.findAll('button').find((b) => b.text() === 'Riprova connessione')
      await retryBtn!.trigger('click')
      await flushPromises()

      const cancelBtn = wrapper.findAll('button').find((b) => b.text() === 'Annulla')
      await cancelBtn!.trigger('click')
      await flushPromises()

      expect(cancelOperationMock).toHaveBeenCalledWith('op-1')
      expect(wrapper.text()).not.toContain('in corso')
      const toast = useToastStore()
      expect(toast.toasts.some((t) => t.message.includes('annullata') && t.message.includes('2 foto'))).toBe(true)
    })
  })
})
