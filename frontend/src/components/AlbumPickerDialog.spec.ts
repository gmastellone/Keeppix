import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

const fetchAlbumsMock = vi.fn()
const fetchAlbumMock = vi.fn()
const addAssetsMock = vi.fn()
const removeAssetMock = vi.fn()

vi.mock('@/api/albums', () => ({
  fetchAlbums: (...args: unknown[]) => fetchAlbumsMock(...args),
  fetchAlbum: (...args: unknown[]) => fetchAlbumMock(...args),
  addAssets: (...args: unknown[]) => addAssetsMock(...args),
  removeAsset: (...args: unknown[]) => removeAssetMock(...args)
}))

const AlbumPickerDialog = (await import('./AlbumPickerDialog.vue')).default

// Stesso motivo di DeleteDialog.spec.ts: il `DialogPortal` di reka-ui
// teletrasporta sempre nel vero `document.body`, fuori dal sottoalbero DOM
// del wrapper — `wrapper.find`/`findAll` non lo vedrebbero mai. Si
// interroga `document.body` direttamente, con `tick()` invece di
// `flushPromises()` da solo: lo `watch(open, …, {immediate:true})` che
// avvia il caricamento degli album passa da più giri di microtask
// (fetchAlbums → Promise.all di fetchAlbum → assegnazione reattiva), e un
// singolo `setTimeout(0)` copre comunque tutta la catena una volta sola.
const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function photo(id: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: null,
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: null,
    width: 100,
    height: 100,
    thumbhash: null,
    raw_kind: null,
    favorite: false,
    camera_model: null,
    tags: [],
    faces: []
  }
}

let wrapper: VueWrapper | undefined

function mountHost(assets: TimelineAsset[]) {
  const Host = defineComponent({
    components: { TheAlbumPickerDialog: AlbumPickerDialog },
    setup() {
      const open = ref(true)
      return { open, assets }
    },
    template: `<TheAlbumPickerDialog v-model:open="open" :assets="assets" />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
}

function switches(): HTMLButtonElement[] {
  return Array.from(document.body.querySelectorAll('[role="switch"]'))
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  i18n.global.locale.value = 'it'
  fetchAlbumsMock.mockResolvedValue([
    { id: 'album-1', name: 'Urbino', cover_hash: null, created_at: '' },
    { id: 'album-2', name: 'Lago di Braies', cover_hash: null, created_at: '' }
  ])
  fetchAlbumMock.mockImplementation(async (id: string) => ({ id, name: '', assets: [] }))
  addAssetsMock.mockResolvedValue(null)
  removeAssetMock.mockResolvedValue(null)
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
})

describe('AlbumPickerDialog', () => {
  it('lists every album from fetchAlbums, all switched off when none contains the selection', async () => {
    mountHost([photo('a')])
    await tick()

    const rows = switches()
    expect(rows).toHaveLength(2)
    expect(document.body.textContent).toContain('Urbino')
    expect(document.body.textContent).toContain('Lago di Braies')
    rows.forEach((row) => expect(row.getAttribute('aria-checked')).toBe('false'))
  })

  it('shows a row as "on" only when every selected photo is already a member — not just some', async () => {
    fetchAlbumMock.mockImplementation(async (id: string) =>
      id === 'album-1' ? { id, name: 'Urbino', assets: [photo('a')] } : { id, name: '', assets: [] }
    )
    mountHost([photo('a'), photo('b')])
    await tick()

    // "a" è dentro album-1, "b" no: non è un membership pieno → resta spento.
    expect(switches()[0]?.getAttribute('aria-checked')).toBe('false')
  })

  it('clicking an off row adds every selected asset in one bulk call, then flips on', async () => {
    mountHost([photo('a'), photo('b')])
    await tick()

    switches()[0]?.click()
    await tick()

    expect(addAssetsMock).toHaveBeenCalledWith('album-1', ['a', 'b'])
    expect(switches()[0]?.getAttribute('aria-checked')).toBe('true')
  })

  it('clicking an on row (all members) removes every selected asset one by one, then flips off — §12.3 group toggle', async () => {
    fetchAlbumMock.mockImplementation(async (id: string) =>
      id === 'album-1' ? { id, name: 'Urbino', assets: [photo('a'), photo('b')] } : { id, name: '', assets: [] }
    )
    mountHost([photo('a'), photo('b')])
    await tick()
    expect(switches()[0]?.getAttribute('aria-checked')).toBe('true')

    switches()[0]?.click()
    await tick()

    expect(removeAssetMock).toHaveBeenCalledWith('album-1', 'a')
    expect(removeAssetMock).toHaveBeenCalledWith('album-1', 'b')
    expect(switches()[0]?.getAttribute('aria-checked')).toBe('false')
  })

  it('"Fatto" closes the dialog', async () => {
    mountHost([photo('a')])
    await tick()

    const done = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Fatto')
    done?.click()
    await tick()

    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })
})
