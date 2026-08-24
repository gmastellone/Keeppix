import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import { useCullingStore } from '@/stores/culling'
import type { TimelineAsset } from '@/api/timeline'

vi.mock('@/api/culling', () => ({
  setFlags: vi.fn(async () => null),
  deleteAsset: vi.fn(async () => null),
  fetchFlags: vi.fn(async () => ({ rating: null, pick: 'none', color_label: null, favorite: false })),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
}))

// Task 8 (10/N): il pulsante info dello stage apre `AssetViewer.vue` per
// davvero — `loadPanelData()` chiama `apiFetch` (via `fetchMetadata`/
// `fetchAsset`) direttamente, mai mockato finora in questo file perché
// prima d'ora nessun test apriva il lightbox. Un default a `[]` (mai un
// valore fisso globale — vedi il correttivo dello stesso tipo già
// maturato in `SearchView.spec.ts`/`TimelineView.spec.ts` per lo stesso
// bug, Task 8 9/N) basta: i test qui sotto verificano solo che il
// lightbox si apra col contesto giusto, non i dati che carica.
vi.mock('@/api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/api/client')>()),
  apiFetch: vi.fn(async () => [])
}))

import CullingView from './CullingView.vue'
import { fullSrc } from '@/api/media'

function photo(id: string, kind = 'image', filename = `${id}.jpg`): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename,
    content_hash: `${id}${'a'.repeat(63)}`.slice(0, 64),
    size_bytes: 1,
    kind,
    status: 'indexed',
    taken_at_utc: '2024-07-10T12:00:00Z',
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

async function mountCulling() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/culling', component: CullingView }
    ]
  })
  await router.push('/culling')
  await router.isReady()
  const wrapper = mount(CullingView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, wrapper }
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('CullingView keyboard shortcuts', () => {
  it('rating a photo with a digit key advances to the next one', async () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b')])

    const { wrapper } = await mountCulling()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: '3' }))
    await flushPromises()

    expect(store.flagsFor('a').rating).toBe(3)
    expect(store.currentAsset?.id).toBe('b')
    wrapper.unmount()
  })

  it('does not fire shortcuts while the user is typing in a text field', async () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b')])

    const { wrapper } = await mountCulling()

    const input = document.createElement('input')
    document.body.appendChild(input)
    input.focus()

    input.dispatchEvent(new KeyboardEvent('keydown', { key: '3', bubbles: true }))
    await flushPromises()

    expect(store.flagsFor('a').rating).toBeNull()
    expect(store.currentAsset?.id).toBe('a')

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', bubbles: true }))
    await flushPromises()
    expect(store.flagsFor('a').pick).toBe('none')

    wrapper.unmount()
  })

  it('ignores shortcuts typed into a textarea or a contenteditable element', async () => {
    const store = useCullingStore()
    store.start([photo('a')])
    const { wrapper } = await mountCulling()

    const textarea = document.createElement('textarea')
    document.body.appendChild(textarea)
    textarea.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', bubbles: true }))
    await flushPromises()
    expect(store.flagsFor('a').pick).toBe('none')

    const editable = document.createElement('div')
    editable.setAttribute('contenteditable', 'true')
    document.body.appendChild(editable)
    editable.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', bubbles: true }))
    await flushPromises()
    expect(store.flagsFor('a').pick).toBe('none')

    wrapper.unmount()
  })
})

describe('CullingView — §21, il lightbox aperto da un lotto', () => {
  it('the round info button opens AssetViewer for the current photo, with isCulling hiding TAG/ALBUM/PERSONE/Elimina/Aggiungi ad album', async () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b')])
    const { wrapper } = await mountCulling()

    const infoButton = wrapper.get('[aria-label="Photo details — EXIF, location, rename"]')
    await infoButton.trigger('click')
    await flushPromises()

    const viewer = wrapper.findComponent({ name: 'AssetViewer' })
    expect(viewer.exists()).toBe(true)
    expect(viewer.props('asset').id).toBe('a')
    expect(viewer.props('isCulling')).toBe(true)

    const text = wrapper.text()
    expect(text).not.toContain('Tags')
    expect(text).not.toContain('Albums')
    expect(text).not.toContain('Delete…')
    expect(text).not.toContain('Add to album')
    // Everything else stays (§21.2, "cosa resta identico"): title,
    // stars, SCATTO section, Rename….
    expect(text).toContain('Rename…')
    expect(text).toContain('Download original')

    wrapper.unmount()
  })

  it('closing the lightbox returns to the same stage photo, and suppresses culling shortcuts while open', async () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b')])
    const { wrapper } = await mountCulling()

    await wrapper.get('[aria-label="Photo details — EXIF, location, rename"]').trigger('click')
    await flushPromises()

    // §21.5: le scorciatoie del culling sono soppresse finché il
    // lightbox è aperto — la stella non deve votare la foto sotto.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: '3' }))
    await flushPromises()
    expect(store.flagsFor('a').rating).toBeNull()

    wrapper.findComponent({ name: 'AssetViewer' }).vm.$emit('close')
    await flushPromises()

    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(false)
    expect(store.currentAsset?.id).toBe('a')

    wrapper.unmount()
  })
})

describe('CullingView zoom', () => {
  it('loads the full derivative on z, not the RAW original', async () => {
    const store = useCullingStore()
    const raw = photo('a', 'raw_image', 'a.arw')
    store.start([raw])

    const { wrapper } = await mountCulling()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z' }))
    await flushPromises()

    const img = wrapper.get(`img[src="${fullSrc(raw.content_hash!)}"]`)
    expect(img.attributes('src')).toBe(fullSrc(raw.content_hash!))
    expect(wrapper.find(`img[src="/media/original/${raw.id}"]`).exists()).toBe(false)
    wrapper.unmount()
  })

  it('does not preload full until zoomed: demosaic is seconds and RAM-gated', async () => {
    const loaded: string[] = []
    vi.stubGlobal(
      'fetch',
      (input: RequestInfo | URL) => {
        loaded.push(String(input))
        return Promise.resolve(new Response())
      }
    )

    const store = useCullingStore()
    const first = photo('a', 'raw_image', 'a.arw')
    const second = photo('b', 'raw_image', 'b.arw')
    const third = photo('c', 'raw_image', 'c.arw')
    store.start([first, second, third])

    const { wrapper } = await mountCulling()
    await flushPromises()

    expect(loaded.some((src) => src.includes('/media/full/'))).toBe(false)
    expect(loaded.some((src) => src.includes('/media/original/'))).toBe(false)
    wrapper.unmount()
    vi.unstubAllGlobals()
  })

  it('when zoomed, preloads at most the next photo, never three ahead', async () => {
    const loaded: string[] = []
    vi.stubGlobal(
      'fetch',
      (input: RequestInfo | URL) => {
        loaded.push(String(input))
        return Promise.resolve(new Response())
      }
    )

    const store = useCullingStore()
    const photos = ['a', 'b', 'c', 'd'].map((id) => photo(id, 'raw_image', `${id}.arw`))
    store.start(photos)

    const { wrapper } = await mountCulling()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z' }))
    await flushPromises()

    const fullLoads = loaded.filter((src) => src.includes('/media/full/'))
    expect(fullLoads.some((src) => src.includes('/media/original/'))).toBe(false)
    expect(fullLoads).toContain(fullSrc(photos[1].content_hash!))
    expect(fullLoads).not.toContain(fullSrc(photos[2].content_hash!))
    expect(fullLoads).not.toContain(fullSrc(photos[3].content_hash!))
    wrapper.unmount()
    vi.unstubAllGlobals()
  })

  it('shows a loading state while the full image has not loaded', async () => {
    const store = useCullingStore()
    store.start([photo('a', 'raw_image', 'a.arw')])
    const { wrapper } = await mountCulling()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z' }))
    await flushPromises()

    expect(wrapper.text()).toContain('Loading')
    wrapper.unmount()
  })
})
