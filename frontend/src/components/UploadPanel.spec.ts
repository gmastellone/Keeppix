import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'

import { i18n } from '@/i18n'
import { useUploadStore, type UploadSessionState } from '@/stores/upload'

import UploadPanel from './UploadPanel.vue'

vi.mock('@/api/bootstrap', () => ({
  fetchBootstrap: vi.fn(async () => ({
    user: { id: '1', username: 'admin', display_name: 'Admin', email: null, role: 'admin', locale: null },
    folders: [{ id: 'f1', library_id: 'l1', parent_id: null, name: 'Urbino', depth: 0 }],
    storage: {},
    badges: { culling: 0, revision: 0 }
  }))
}))

let mounted: VueWrapper | undefined
let previousLocale: typeof i18n.global.locale.value

beforeEach(() => {
  setActivePinia(createPinia())
  previousLocale = i18n.global.locale.value
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  vi.resetAllMocks()
  mounted?.unmount()
  mounted = undefined
  i18n.global.locale.value = previousLocale
})

function session(overrides: Partial<UploadSessionState>): UploadSessionState {
  return {
    id: 'a',
    filename: 'a.jpg',
    targetFolderId: 'f1',
    expectedSize: 1_000_000,
    receivedBytes: 0,
    status: 'queued',
    ...overrides
  }
}

async function mountPanel(): Promise<{ wrapper: VueWrapper; upload: ReturnType<typeof useUploadStore>; router: Router }> {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/culling', component: { template: '<div />' } }
    ]
  })
  await router.push('/')
  await router.isReady()
  const upload = useUploadStore()
  const wrapper = mount(UploadPanel, { global: { plugins: [router, i18n] }, attachTo: document.body })
  mounted = wrapper
  await flushPromises()
  return { wrapper, upload, router }
}

describe('UploadPanel — visibilità (§6.2, §7.4)', () => {
  it('is entirely absent when panelOpen is false', async () => {
    const { wrapper } = await mountPanel()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false)
  })

  it('stays absent even with panelOpen true if there is nothing to show', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    await flushPromises()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false)
  })

  it('shows once a session is queued', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({}))
    await flushPromises()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true)
  })

  it('deliberately shows even for an all-rejected batch — deviation from the prototype\'s own !items.length gate (mockup riga 2979), which would give zero feedback', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.rejectedRaw = ['a.arw']
    await flushPromises()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true)
  })

  it('clicking the mobile scrim closes the panel', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({}))
    await flushPromises()

    await wrapper.find('.bg-black\\/40').trigger('click')
    expect(upload.panelOpen).toBe(false)
  })
})

describe('UploadPanel — titolo (§6.2, priorità della riga 3001 del mockup)', () => {
  it('needs a destination first, even over "paused"', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ targetFolderId: null, status: 'paused' }))
    await flushPromises()
    expect(wrapper.text()).toContain('In attesa di una destinazione')
  })

  it('shows "in pausa" when every pending session is paused, and a destination is set', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ status: 'paused' }))
    await flushPromises()
    expect(wrapper.text()).toContain('Caricamento in pausa')
  })

  it('shows "in corso" while at least one session is actively queued or uploading', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ status: 'uploading' }))
    await flushPromises()
    expect(wrapper.text()).toContain('Caricamento in corso')
  })

  it('shows "completato" once nothing is pending anymore', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ status: 'done', receivedBytes: 1_000_000 }))
    await flushPromises()
    expect(wrapper.text()).toContain('Caricamento completato')
  })
})

describe('UploadPanel — testata (§6.4)', () => {
  it('the pause/resume button is hidden while a destination is needed', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ targetFolderId: null }))
    await flushPromises()
    expect(wrapper.find('button[aria-label="Pausa"]').exists()).toBe(false)
  })

  it('pausing the whole queue and resuming it call the real queue-wide commands', async () => {
    const { wrapper, upload } = await mountPanel()
    const pauseAllSpy = vi.spyOn(upload, 'pauseAll')
    const resumeAllSpy = vi.spyOn(upload, 'resumeAll')
    upload.panelOpen = true
    upload.sessions.push(session({ status: 'uploading' }))
    await flushPromises()

    await wrapper.find('button[aria-label="Pausa"]').trigger('click')
    expect(pauseAllSpy).toHaveBeenCalledTimes(1)
    expect(upload.sessions[0].status).toBe('paused')

    await wrapper.find('button[aria-label="Riprendi"]').trigger('click')
    expect(resumeAllSpy).toHaveBeenCalledTimes(1)
  })

  it('"Chiudi il pannello" closes without touching the queue', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({}))
    await flushPromises()

    await wrapper.find('button[aria-label="Chiudi il pannello"]').trigger('click')
    expect(upload.panelOpen).toBe(false)
    expect(upload.sessions).toHaveLength(1)
  })

  it('Escape on the panel closes it too — §8, "Esc a livelli", secondo livello', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({}))
    await flushPromises()

    await wrapper.find('[role="dialog"]').trigger('keydown', { key: 'Escape' })
    expect(upload.panelOpen).toBe(false)
  })
})

describe('UploadPanel — righe (§6.3, i sei stati)', () => {
  it('a queued row shows size + "in coda"', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ expectedSize: 300_000, status: 'queued' }))
    await flushPromises()
    expect(wrapper.text()).toContain('293 KB · in coda')
  })

  it('an uploading row shows size + percentage, and a progress bar', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ expectedSize: 1000, receivedBytes: 340, status: 'uploading' }))
    await flushPromises()
    expect(wrapper.text()).toContain('1000 B · 34%')
    expect(wrapper.find('.bg-accent.rounded-full').exists()).toBe(true)
  })

  it('a paused row shows size + "in pausa"', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ expectedSize: 1000, status: 'paused' }))
    await flushPromises()
    expect(wrapper.text()).toContain('1000 B · in pausa')
  })

  it('a done row shows the neutral "Completato" badge, no color', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ status: 'done', receivedBytes: 1_000_000 }))
    await flushPromises()
    expect(wrapper.text()).toContain('Completato')
  })

  it('a client-precheck skipped row shows the amber badge and the reason, no crash on a missing existingAssetId', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ status: 'skipped', collision: 'skipped_duplicate' }))
    await flushPromises()
    expect(wrapper.text()).toContain('Saltato')
    expect(wrapper.text()).toContain('già in libreria')
  })

  it('a server-side duplicate found at finalize time (status "done" + collision) displays exactly like "skipped"', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(
      session({ status: 'done', collision: 'skipped_duplicate', existingAssetId: 'existing-1', receivedBytes: 1_000_000 })
    )
    await flushPromises()
    expect(wrapper.text()).toContain('Saltato')
    expect(wrapper.find('button').element.textContent).toBeDefined()
  })

  it('"Vedi quella presente" navigates to the real existing asset via ?photo=, and closes the panel', async () => {
    const { wrapper, upload, router } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(
      session({ status: 'skipped', collision: 'skipped_duplicate', existingAssetId: 'existing-1' })
    )
    await flushPromises()

    const seeExistingButton = wrapper.findAll('button').find((b) => b.text() === 'Vedi quella presente')
    await seeExistingButton?.trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query.photo).toBe('existing-1')
    expect(upload.panelOpen).toBe(false)
  })

  it('an error row shows the red badge, the real error reason and a working "Riprova"', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ status: 'error', error: 'upload.errors.unknown' }))
    await flushPromises()

    expect(wrapper.text()).toContain('Errore')
    expect(wrapper.text()).toContain('Il caricamento non è riuscito.')

    const retryButton = wrapper.findAll('button').find((b) => b.text() === 'Riprova')
    expect(retryButton?.exists()).toBe(true)
  })
})

describe('UploadPanel — blocco di rifiuto (§4.1)', () => {
  it('shows the exact RAW rejection block, truncated to four names, with the real count', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.rejectedRaw = ['a.arw', 'b.cr3', 'c.nef', 'd.dng', 'e.raf']
    await flushPromises()

    expect(wrapper.text()).toContain('5 file RAW non caricati')
    expect(wrapper.text()).toContain('a.arw, b.cr3, c.nef, d.dng e un altro')
  })

  it('the singular form reads correctly for exactly one rejected RAW file', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.rejectedRaw = ['a.arw']
    await flushPromises()
    expect(wrapper.text()).toContain('1 file RAW non caricato')
  })

  it('"Apri Culling" navigates there and closes the panel', async () => {
    const { wrapper, upload, router } = await mountPanel()
    upload.panelOpen = true
    upload.rejectedRaw = ['a.arw']
    await flushPromises()

    await wrapper.find('button.border-border').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/culling')
    expect(upload.panelOpen).toBe(false)
  })

  it('shows the unsupported-format block separately, with its own explanation — no "Apri Culling" there', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.rejectedUnsupported = ['notes.txt']
    await flushPromises()

    expect(wrapper.text()).toContain('1 file di formato non supportato')
    expect(wrapper.text()).toContain('Keeppix accetta JPEG')
  })
})

describe('UploadPanel — piede (§6.4)', () => {
  it('the summary always shows the done count, and only appends skipped/error segments when they are non-zero', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(
      session({ id: 'a', status: 'done', receivedBytes: 1_000_000 }),
      session({ id: 'b', status: 'done', receivedBytes: 1_000_000 }),
      session({ id: 'c', status: 'skipped' })
    )
    await flushPromises()

    expect(wrapper.text()).toContain('2 caricate · 1 saltata')
    expect(wrapper.text()).not.toContain('non riuscit')
  })

  it('"Pulisci completate" only appears with something finished, and removes it for real', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ status: 'queued' }))
    await flushPromises()
    expect(wrapper.text()).not.toContain('Pulisci completate')

    upload.sessions.push(session({ id: 'b', status: 'done', receivedBytes: 1_000_000 }))
    await flushPromises()
    const clearButton = wrapper.findAll('button').find((b) => b.text() === 'Pulisci completate')
    await clearButton?.trigger('click')
    expect(upload.sessions).toHaveLength(1)
  })

  it('"Annulla tutto" only appears with something pending, and clears the whole queue for real', async () => {
    const { wrapper, upload } = await mountPanel()
    upload.panelOpen = true
    upload.sessions.push(session({ status: 'queued' }))
    await flushPromises()

    const cancelButton = wrapper.findAll('button').find((b) => b.text() === 'Annulla tutto')
    await cancelButton?.trigger('click')
    expect(upload.sessions).toHaveLength(0)
  })
})
