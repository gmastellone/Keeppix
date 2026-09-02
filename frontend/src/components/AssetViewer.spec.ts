import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Face } from '@/api/faces'
import type { AssetTagDetail, Tag } from '@/api/tags'
import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'
import { useToastStore } from '@/stores/toast'

import AssetViewer from './AssetViewer.vue'
import { originalSrc, previewSrc } from '@/api/media'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))
vi.mock('@/api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/api/client')>()),
  apiFetch
}))

const deleteAssetMock = vi.fn()
vi.mock('@/api/culling', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/api/culling')>()),
  deleteAsset: (...args: unknown[]) => deleteAssetMock(...args)
}))

// AlbumPickerDialog/RenameFormulaDialog stay mounted (closed) at all times:
// without these mocks their harmless `watch(open,...)` at `open=false`
// wouldn't fire, but if a test opens them they'd hit the real `apiFetch`
// (mocked above to return nothing) — same fix already used in
// BatchEditView.spec.ts. `fetchAlbumsForAsset`/`addAssets`/`removeAsset`
// are needed because `AssetViewer` (which now calls `fetchAlbumsForAsset`
// in `loadPanelData`) and `AlbumPickerDialog` (which calls
// `addAssets`/`removeAsset` when a toggle is touched) would otherwise
// find `undefined` functions.
const {
  fetchAlbumsForAssetMock,
  fetchAlbumsMock,
  fetchAlbumMock,
  addAssetsMock,
  removeAssetMock
} = vi.hoisted(() => ({
  fetchAlbumsForAssetMock: vi.fn(async (): Promise<{ id: string; name: string }[]> => []),
  fetchAlbumsMock: vi.fn(async () => []),
  fetchAlbumMock: vi.fn(async () => ({ id: 'x', name: '', assets: [] })),
  addAssetsMock: vi.fn(async () => null),
  removeAssetMock: vi.fn(async () => null)
}))
vi.mock('@/api/albums', () => ({
  fetchAlbumsForAsset: fetchAlbumsForAssetMock,
  fetchAlbums: fetchAlbumsMock,
  fetchAlbum: fetchAlbumMock,
  addAssets: addAssetsMock,
  removeAsset: removeAssetMock
}))
vi.mock('@/api/rename', () => ({
  previewRename: vi.fn(async () => []),
  applyRenameBatch: vi.fn(async () => ({ operation_id: 'op' }))
}))
vi.mock('@/api/operations', () => ({
  cancelOperation: vi.fn(async () => ({ succeeded: [], failed: [], batch_id: null }))
}))
// RenameFormulaDialog (embedded here for the lightbox's "Rename…") opens a
// real WebSocket connection to follow the actual rename progress — none of
// these tests exercise it, the mock only exists so it doesn't attempt a
// real connection in jsdom.
vi.mock('@/api/events', () => ({
  startLiveEvents: vi.fn(() => ({ close: vi.fn() }))
}))

afterEach(() => {
  apiFetch.mockReset()
  deleteAssetMock.mockReset()
  fetchAlbumsForAssetMock.mockReset()
  fetchAlbumsMock.mockReset()
  fetchAlbumMock.mockReset()
  addAssetsMock.mockReset()
  removeAssetMock.mockReset()
})

function photo(id: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: `${id}${'a'.repeat(63)}`.slice(0, 64),
    size_bytes: 1,
    kind: 'image',
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

/** `/map/regions` (`maps.loadRegions()`, called every time the panel
 * opens) and `/tags`/`/assets/{id}/tags` (tag+category list and the
 * asset's own tags) always want an array — no test here actually
 * exercises them, but without an explicit route they fall into each
 * mock's fallback branch, which often returns a single object and breaks
 * `.filter()`/`.length` downstream. */
function isArrayEndpoint(path: string): boolean {
  return path.endsWith('/map/regions') || path.endsWith('/tags')
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => {
    resolve = done
  })
  return { promise, resolve }
}

beforeEach(() => {
  setActivePinia(createPinia())
  i18n.global.locale.value = 'it'
  apiFetch.mockResolvedValue([])
  deleteAssetMock.mockResolvedValue(null)
  fetchAlbumsForAssetMock.mockResolvedValue([])
  fetchAlbumsMock.mockResolvedValue([])
  fetchAlbumMock.mockResolvedValue({ id: 'x', name: '', assets: [] })
  addAssetsMock.mockResolvedValue(null)
  removeAssetMock.mockResolvedValue(null)
})

describe('AssetViewer — stage, arrows, filmstrip', () => {
  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  it('renders no arrows/filmstrip without neighbors, and never closes on background click', () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('aaaa'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    expect(wrapper.find('[aria-label="Foto precedente"]').exists()).toBe(false)
    expect(wrapper.find('[aria-label="Foto successiva"]').exists()).toBe(false)

    wrapper.get('[role="dialog"]').trigger('click')
    expect(wrapper.emitted('close')).toBeUndefined()
  })

  it('emits "step" with the resolved neighbour on arrow click/keydown, omitting the edge arrow', async () => {
    const a = photo('a')
    const b = photo('b')
    const c = photo('c')
    wrapper = mount(AssetViewer, {
      props: { asset: b, neighbors: [a, b, c], isFavorite: false },
      global: { plugins: [i18n] }
    })

    // "b" is in the middle: both arrows exist.
    expect(wrapper.find('[aria-label="Foto precedente"]').exists()).toBe(true)
    expect(wrapper.find('[aria-label="Foto successiva"]').exists()).toBe(true)

    await wrapper.get('[aria-label="Foto successiva"]').trigger('click')
    expect(wrapper.emitted('step')?.[0]).toEqual([c])

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft' }))
    expect(wrapper.emitted('step')?.[1]).toEqual([a])
  })

  it('omits the left arrow on the first neighbour and the right arrow on the last', () => {
    const a = photo('a')
    const b = photo('b')
    wrapper = mount(AssetViewer, {
      props: { asset: a, neighbors: [a, b], isFavorite: false },
      global: { plugins: [i18n] }
    })
    expect(wrapper.find('[aria-label="Foto precedente"]').exists()).toBe(false)
    expect(wrapper.find('[aria-label="Foto successiva"]').exists()).toBe(true)
  })

  it('renders a filmstrip thumbnail per neighbour, highlighting the current one, and clicking one steps to it', async () => {
    const a = photo('a')
    const b = photo('b')
    const c = photo('c')
    wrapper = mount(AssetViewer, {
      props: { asset: b, neighbors: [a, b, c], isFavorite: false },
      global: { plugins: [i18n] }
    })

    const filmstrip = wrapper.get('.overflow-x-auto')
    const thumbs = filmstrip.findAll('img[alt="a.jpg"], img[alt="b.jpg"], img[alt="c.jpg"]')
    expect(thumbs).toHaveLength(3)

    await filmstrip.get('img[alt="c.jpg"]').trigger('click')
    expect(wrapper.emitted('step')?.[0]).toEqual([c])
  })

  it('keeps src reactive across an asset change', async () => {
    const first = photo('aaaa')
    const second = photo('bbbb')
    wrapper = mount(AssetViewer, {
      props: { asset: first, isFavorite: false },
      global: { plugins: [i18n] }
    })
    expect(wrapper.get('img[alt="aaaa.jpg"]').attributes('src')).toBe(previewSrc(first.content_hash!))

    await wrapper.setProps({ asset: second })
    expect(wrapper.get('img[alt="bbbb.jpg"]').attributes('src')).toBe(previewSrc(second.content_hash!))
  })

  it('falls back to the original when the preview 404s (e.g. an already-small original, whose separate preview derivative is deliberately never generated)', async () => {
    const asset = photo('aaaa')
    wrapper = mount(AssetViewer, {
      props: { asset, isFavorite: false },
      global: { plugins: [i18n] }
    })
    const img = wrapper.get('img[alt="aaaa.jpg"]')
    expect(img.attributes('src')).toBe(previewSrc(asset.content_hash!))

    await img.trigger('error')

    expect(wrapper.get('img[alt="aaaa.jpg"]').attributes('src')).toBe(originalSrc(asset.id))
  })

  it('retries the real preview on the next asset, instead of getting stuck on the fallback', async () => {
    const first = photo('aaaa')
    const second = photo('bbbb')
    wrapper = mount(AssetViewer, {
      props: { asset: first, isFavorite: false },
      global: { plugins: [i18n] }
    })
    await wrapper.get('img[alt="aaaa.jpg"]').trigger('error')
    expect(wrapper.get('img[alt="aaaa.jpg"]').attributes('src')).toBe(originalSrc(first.id))

    await wrapper.setProps({ asset: second })

    expect(wrapper.get('img[alt="bbbb.jpg"]').attributes('src')).toBe(previewSrc(second.content_hash!))
  })
})

describe('AssetViewer — top bar', () => {
  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  it('close/favorite/info buttons work; the heart reflects isFavorite and toggles via "f"', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: true },
      global: { plugins: [i18n] }
    })
    expect(wrapper.find('[aria-label="Rimuovi dai preferiti"]').exists()).toBe(true)

    await wrapper.get('[aria-label="Chiudi"]').trigger('click')
    expect(wrapper.emitted('close')).toHaveLength(1)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f' }))
    expect(wrapper.emitted('toggle-favorite')).toHaveLength(1)
  })

  it('the info panel starts open ("forced open on every opening"), and "i"/the icon close and reopen it', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()
    expect(wrapper.find('#lbTitleInput').exists()).toBe(true)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    await flushPromises()
    expect(wrapper.find('#lbTitleInput').exists()).toBe(false)

    await wrapper.get('[aria-label="Informazioni"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('#lbTitleInput').exists()).toBe(true)
  })

  it('Esc closes the ⋯ menu on the first press, the lightbox only on the second', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    await wrapper.get('[aria-label="Altre azioni"]').trigger('click')
    await flushPromises()
    expect(document.body.querySelector('a,button')).toBeTruthy()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(wrapper.emitted('close')).toBeUndefined()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(wrapper.emitted('close')).toHaveLength(1)
  })
})

describe('AssetViewer — ⋯ menu', () => {
  function menuItemWithText(text: string) {
    return Array.from(document.body.querySelectorAll('a,button')).find((el) => el.textContent?.trim() === text)
  }

  // The three tests mount with `attachTo: document.body` (needed for the
  // teleported popover): without unmounting, the previous test's DOM stays
  // attached and `menuItemWithText` can find the wrong button.
  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  it('"Scarica originale" is a real same-origin download link, not a toast', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] },
      attachTo: document.body
    })
    await wrapper.get('[aria-label="Altre azioni"]').trigger('click')
    await flushPromises()

    const link = menuItemWithText('Scarica originale') as HTMLAnchorElement
    expect(link.tagName).toBe('A')
    expect(link.getAttribute('href')).toBe('/media/original/a')
    expect(link.getAttribute('download')).toBe('a.jpg')
  })

  it('"Ruota" patches the real orientation override — 90° clockwise', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] },
      attachTo: document.body
    })
    await wrapper.get('[aria-label="Altre azioni"]').trigger('click')
    await flushPromises()

    menuItemWithText('Ruota')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/assets/a/metadata',
      expect.objectContaining({ method: 'PATCH', body: JSON.stringify({ orientation: 90 }) })
    )
  })

  it('"Elimina…" opens the 3-way delete dialog; choosing an option deletes and closes the lightbox', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] },
      attachTo: document.body
    })
    await wrapper.get('[aria-label="Altre azioni"]').trigger('click')
    await flushPromises()
    menuItemWithText('Elimina…')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    const trashOption = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Sposta nel cestino')
    )
    trashOption?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(deleteAssetMock).toHaveBeenCalledWith('a', 'moved_to_trash')
    expect(wrapper.emitted('close')).toHaveLength(1)
  })
})

describe('AssetViewer — info panel (mini-map)', () => {
  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  it('shows a compact cluster map only when effective metadata has a location', async () => {
    apiFetch.mockImplementation((path: string) =>
      isArrayEndpoint(path)
        ? Promise.resolve([])
        : Promise.resolve({
          title: null,
          description: null,
          taken_at: null,
          location: { lat: 41.9, lon: 12.5 },
          place_id: null,
          orientation: null
        })
    )
    wrapper = mount(AssetViewer, {
      props: { asset: photo('aaaa'), isFavorite: false },
      global: {
        plugins: [i18n],
        stubs: {
          MapClusterLayer: {
            props: ['center'],
            template: '<div data-testid="mini-map">{{ center }}</div>'
          }
        }
      }
    })

    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/aaaa/metadata')
    expect(wrapper.find('[data-testid="mini-map"]').exists()).toBe(true)
  })

  it('ignores stale metadata when the viewed asset changes', async () => {
    const firstMetadata = deferred<{ location: { lat: number; lon: number } }>()
    const secondMetadata = deferred<{ location: { lat: number; lon: number } }>()
    apiFetch.mockImplementation((path: string) => {
      if (isArrayEndpoint(path)) return Promise.resolve([])
      return path.includes('/aaaa/') ? firstMetadata.promise : secondMetadata.promise
    })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('aaaa'), isFavorite: false },
      global: {
        plugins: [i18n],
        stubs: {
          MapClusterLayer: {
            props: ['center'],
            template: '<div data-testid="mini-map">{{ center.lat }},{{ center.lon }}</div>'
          }
        }
      }
    })

    await wrapper.setProps({ asset: photo('bbbb'), isFavorite: false })
    secondMetadata.resolve({ location: { lat: 45, lon: 9 } })
    await flushPromises()
    expect(wrapper.get('[data-testid="mini-map"]').text()).toBe('45,9')

    firstMetadata.resolve({ location: { lat: 41.9, lon: 12.5 } })
    await flushPromises()
    expect(wrapper.get('[data-testid="mini-map"]').text()).toBe('45,9')
  })
})

describe('AssetViewer — title, rating, shot info', () => {
  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  function mockPanelFetch(opts: {
    title?: string | null
    rating?: number | null
    exif?: Record<string, unknown>
    location?: { lat: number; lon: number } | null
  } = {}) {
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (isArrayEndpoint(path)) return Promise.resolve([])
      if (path.endsWith('/metadata') && method === 'GET') {
        return Promise.resolve({
          title: opts.title ?? null,
          description: null,
          taken_at: null,
          location: opts.location ?? null,
          place_id: null,
          orientation: null
        })
      }
      if (path.endsWith('/metadata') && method === 'PATCH') return Promise.resolve(null)
      if (path.endsWith('/flags') && method === 'GET') {
        return Promise.resolve({ rating: opts.rating ?? null, pick: 'none', color_label: null, favorite: false })
      }
      if (path.endsWith('/flags') && method === 'PUT') return Promise.resolve(null)
      // GET /api/v1/assets/{id} (detail, never /metadata or /flags)
      return Promise.resolve({ ...photo('a'), full_exif: opts.exif })
    })
  }

  it('loads the existing title into the field; clearing it resets to null (not empty string) and saves only on change', async () => {
    mockPanelFetch({ title: 'Tramonto' })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    await flushPromises()

    const input = wrapper.get('#lbTitleInput')
    expect((input.element as HTMLInputElement).value).toBe('Tramonto')

    await input.setValue('  ')
    await input.trigger('change')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/assets/a/metadata',
      expect.objectContaining({ method: 'PATCH', body: JSON.stringify({ title: null }) })
    )
  })

  it('trims the title and sends it only on change, not on every keystroke (onchange, not oninput)', async () => {
    mockPanelFetch({ title: null })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    await flushPromises()
    apiFetch.mockClear()

    // Simulates typing (only `input`, never `change`) without going
    // through `setValue()`: that vue-test-utils method fires *both*
    // events for `<input>`s, so it couldn't distinguish `onchange` from
    // `oninput` — exactly what this test verifies.
    const input = wrapper.get('#lbTitleInput')
    ;(input.element as HTMLInputElement).value = '  Tramonto  '
    await input.trigger('input')
    expect(apiFetch).not.toHaveBeenCalled()

    await input.trigger('change')
    await flushPromises()
    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/assets/a/metadata',
      expect.objectContaining({ method: 'PATCH', body: JSON.stringify({ title: 'Tramonto' }) })
    )
  })

  it('click on a star sets the rating, a second click on the same star resets it to 0', async () => {
    mockPanelFetch({ rating: 2 })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    await flushPromises()

    await wrapper.get('[aria-label="4 stelle"]').trigger('click')
    await flushPromises()
    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/assets/a/flags',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ rating: 4, pick: 'none', color_label: null, favorite: false })
      })
    )

    apiFetch.mockClear()
    await wrapper.get('[aria-label="4 stelle"]').trigger('click')
    await flushPromises()
    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/assets/a/flags',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ rating: 0, pick: 'none', color_label: null, favorite: false })
      })
    )
  })

  it('SHOT section: shows camera/lens/exposure/dimensions only when the exif carries them', async () => {
    mockPanelFetch({
      exif: {
        camera_make: 'Sony',
        camera_model: 'α7R V',
        lens: 'FE 24-70mm',
        iso: 400,
        f_number: 3.5,
        exposure: '1/250',
        focal_length: 50
      }
    })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Sony α7R V')
    expect(wrapper.text()).toContain('FE 24-70mm')
    expect(wrapper.text()).toContain('f/3.5 · 1/250s · ISO 400')
    expect(wrapper.text()).toContain('100×100')
  })

  it('SHOT section: omits camera/lens/exposure rows the asset has no exif for, keeping dimensions (sourced from the asset itself, not exif)', async () => {
    mockPanelFetch({ exif: undefined })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    await flushPromises()

    expect(wrapper.text()).not.toContain('Fotocamera')
    expect(wrapper.text()).not.toContain('Obiettivo')
    expect(wrapper.text()).not.toContain('Esposizione')
    expect(wrapper.text()).toContain('100×100')
  })
})

describe('AssetViewer — LOCATION section', () => {
  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  function mockPanelFetch(location: { lat: number; lon: number } | null) {
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (isArrayEndpoint(path)) return Promise.resolve([])
      if (path.endsWith('/metadata') && method === 'GET') {
        return Promise.resolve({
          title: null,
          description: null,
          taken_at: null,
          location,
          place_id: null,
          orientation: null
        })
      }
      if (path.endsWith('/metadata') && method === 'PATCH') return Promise.resolve(null)
      if (path.endsWith('/flags') && method === 'GET') {
        return Promise.resolve({ rating: null, pick: 'none', color_label: null, favorite: false })
      }
      if (path.endsWith('/places/reverse')) return Promise.resolve(null)
      return Promise.resolve({ ...photo('a') })
    })
  }

  it('shows the empty state and "Imposta posizione…" when the asset has no location', async () => {
    mockPanelFetch(null)
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Nessuna posizione impostata.')
    expect(wrapper.find('button').exists()).toBe(true)
    expect(wrapper.text()).toContain('Imposta posizione…')
    expect(wrapper.text()).not.toContain('Modifica posizione…')
  })

  it('shows coordinates (4 decimals) and "Modifica posizione…" when the asset has a location', async () => {
    mockPanelFetch({ lat: 41.9, lon: 12.5 })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('41.9000, 12.5000')
    expect(wrapper.text()).toContain('Modifica posizione…')
    expect(wrapper.text()).not.toContain('Nessuna posizione impostata.')
  })

  it('opens the position dialog on click, and "Nessuna posizione" clears the location and reloads the panel', async () => {
    mockPanelFetch({ lat: 41.9, lon: 12.5 })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: {
        plugins: [i18n],
        stubs: { MapClusterLayer: true }
      },
      attachTo: document.body
    })
    await flushPromises()

    const positionButton = wrapper.findAll('button').find((b) => b.text() === 'Modifica posizione…')
    expect(positionButton).toBeTruthy()
    await positionButton!.trigger('click')
    await flushPromises()

    const clearButton = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Nessuna posizione'
    )
    expect(clearButton).toBeTruthy()

    apiFetch.mockClear()
    clearButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/assets/a/metadata',
      expect.objectContaining({ method: 'PATCH', body: JSON.stringify({ location: null, place_id: null }) })
    )
    wrapper.unmount()
  })
})

describe('AssetViewer — PEOPLE section and face boxes', () => {
  function face(id: string, personId: string): Face {
    return {
      id,
      asset_id: 'a',
      bbox: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 },
      person_id: personId,
      proposed_person_id: null,
      proposed_score: null,
      assigned_by_human: true
    }
  }

  function personWithFaces(faces: TimelineAsset['faces']): TimelineAsset {
    return { ...photo('a'), faces }
  }

  function mockPanelFetch(opts: {
    faces?: Face[]
    persons?: { id: string; name: string | null; hidden: boolean; face_count: number | null }[]
    createdPerson?: { id: string; name: string | null; hidden: boolean; face_count: number | null }
  } = {}) {
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (isArrayEndpoint(path)) return Promise.resolve([])
      if (path.endsWith('/metadata') && method === 'GET') {
        return Promise.resolve({
          title: null,
          description: null,
          taken_at: null,
          location: null,
          place_id: null,
          orientation: null
        })
      }
      if (path.endsWith('/flags') && method === 'GET') {
        return Promise.resolve({ rating: null, pick: 'none', color_label: null, favorite: false })
      }
      if (path.endsWith('/faces') && method === 'GET') return Promise.resolve(opts.faces ?? [])
      if (/\/faces\/.+\/assign$/.test(path) && method === 'POST') return Promise.resolve(null)
      if (/\/faces\/.+\/reject$/.test(path) && method === 'POST') return Promise.resolve(null)
      if (path.endsWith('/persons') && method === 'GET') return Promise.resolve(opts.persons ?? [])
      if (path.endsWith('/persons') && method === 'POST') {
        return Promise.resolve(opts.createdPerson ?? { id: 'new', name: null, hidden: false, face_count: null })
      }
      // GET /api/v1/assets/{id} (detail)
      return Promise.resolve({ ...photo('a') })
    })
  }

  // `AssetViewer`'s global `keydown` listener (opening/closing the panel
  // with `i`) stays registered until the component is unmounted: a
  // wrapper that's never unmounted keeps responding to *later* tests'
  // `dispatchEvent` calls, re-invoking its own `loadPanelData()` against
  // the `apiFetch` mocked for that other test — discovered here because it
  // produced `faces.value.filter is not a function` in a completely
  // different section (TAG). Always unmount.
  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  it('renders a chip per confirmed face, falling back to a generic label for unnamed persons', async () => {
    mockPanelFetch({ faces: [face('f1', 'p1'), face('f2', 'p2')] })
    wrapper = mount(AssetViewer, {
      props: {
        asset: personWithFaces([
          { person_id: 'p1', person_name: 'Anna' },
          { person_id: 'p2', person_name: null }
        ]),
        isFavorite: false
      },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Anna')
    expect(wrapper.text()).toContain('Persona senza nome')
  })

  it('hovering a chip shows its face box; leaving hides it after 200ms unless re-entered', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    mockPanelFetch({ faces: [face('f1', 'p1')] })
    wrapper = mount(AssetViewer, {
      props: {
        asset: personWithFaces([{ person_id: 'p1', person_name: 'Anna' }]),
        isFavorite: false
      },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await vi.runAllTimersAsync()

    const boxSelector = '.border-accent'
    expect(wrapper.findAll(boxSelector)).toHaveLength(0)

    const chip = wrapper.findAll('button').find((b) => b.text() === 'Anna')!
    await chip.trigger('mouseenter')
    expect(wrapper.findAll(boxSelector)).toHaveLength(1)

    await chip.trigger('mouseleave')
    expect(wrapper.findAll(boxSelector)).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(100)
    await chip.trigger('mouseenter')
    await vi.advanceTimersByTimeAsync(150)
    expect(wrapper.findAll(boxSelector)).toHaveLength(1)

    await chip.trigger('mouseleave')
    await vi.advanceTimersByTimeAsync(200)
    expect(wrapper.findAll(boxSelector)).toHaveLength(0)

    vi.useRealTimers()
  })

  it('"Non è un volto" rejects the face and shows the exact toast', async () => {
    mockPanelFetch({ faces: [face('f1', 'p1')] })
    wrapper = mount(AssetViewer, {
      props: {
        asset: personWithFaces([{ person_id: 'p1', person_name: 'Anna' }]),
        isFavorite: false
      },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } },
      attachTo: document.body
    })
    const toast = useToastStore()
    await flushPromises()

    const chip = wrapper.findAll('button').find((b) => b.text() === 'Anna')!
    await chip.trigger('click')
    await flushPromises()
    const notAFaceButton = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Non è un volto')
    )
    notAFaceButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/faces/f1/reject', expect.objectContaining({ method: 'POST' }))
    expect(toast.toasts.at(-1)?.message).toBe('Segnato come "non è un volto" — non verrà più riproposto.')
  })

  it('"Correggi persona…" opens the person picker; picking an existing person reassigns the face and shows the toast', async () => {
    mockPanelFetch({
      faces: [face('f1', 'p1')],
      persons: [{ id: 'p9', name: 'Marco', hidden: false, face_count: 3 }]
    })
    wrapper = mount(AssetViewer, {
      props: {
        asset: personWithFaces([{ person_id: 'p1', person_name: 'Anna' }]),
        isFavorite: false
      },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } },
      attachTo: document.body
    })
    const toast = useToastStore()
    await flushPromises()

    const chip = wrapper.findAll('button').find((b) => b.text() === 'Anna')!
    await chip.trigger('click')
    await flushPromises()
    const correctButton = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Correggi persona…')
    )
    correctButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    const marcoButton = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Marco')
    )
    expect(marcoButton).toBeTruthy()
    marcoButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/faces/f1/assign',
      expect.objectContaining({ method: 'POST', body: JSON.stringify({ person_id: 'p9' }) })
    )
    expect(toast.toasts.at(-1)?.message).toBe('Persona corretta.')
  })

  it('the PERSONE section, and its "+ aggiungi persona" toggle, show even with zero confirmed faces', async () => {
    mockPanelFetch({ faces: [] })
    wrapper = mount(AssetViewer, {
      props: { asset: personWithFaces([]), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Persone')
    expect(wrapper.findAll('button').find((b) => b.text() === '+ aggiungi persona')).toBeTruthy()
  })

  it('"+ aggiungi persona" fetches faces even when asset.faces is empty (needsFaces\' usual gate) and boxes every one of them, including unconfirmed', async () => {
    const unconfirmed: Face = {
      id: 'f1',
      asset_id: 'a',
      bbox: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 },
      person_id: null,
      proposed_person_id: null,
      proposed_score: null,
      assigned_by_human: false
    }
    mockPanelFetch({ faces: [unconfirmed, face('f2', 'p1')] })
    wrapper = mount(AssetViewer, {
      props: { asset: personWithFaces([]), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()
    expect(apiFetch).not.toHaveBeenCalledWith('/api/v1/assets/a/faces')

    const toggle = wrapper.findAll('button').find((b) => b.text() === '+ aggiungi persona')!
    await toggle.trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/a/faces')
    expect(wrapper.findAll('[role="button"].absolute.rounded-sm')).toHaveLength(2)
    expect(wrapper.findAll('button').find((b) => b.text() === 'Fatto')).toBeTruthy()
  })

  it('clicking an unconfirmed face\'s box in "+ aggiungi persona" mode opens the picker and assigns that exact face', async () => {
    const unconfirmed: Face = {
      id: 'f1',
      asset_id: 'a',
      bbox: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 },
      person_id: null,
      proposed_person_id: null,
      proposed_score: null,
      assigned_by_human: false
    }
    mockPanelFetch({
      faces: [unconfirmed],
      persons: [{ id: 'p9', name: 'Marco', hidden: false, face_count: 3 }]
    })
    wrapper = mount(AssetViewer, {
      props: { asset: personWithFaces([]), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } },
      attachTo: document.body
    })
    await flushPromises()
    await wrapper.findAll('button').find((b) => b.text() === '+ aggiungi persona')!.trigger('click')
    await flushPromises()

    const box = wrapper.find('[role="button"].absolute.rounded-sm')
    expect(box.exists()).toBe(true)
    await box.trigger('click')
    await flushPromises()

    const marcoButton = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Marco')
    )
    expect(marcoButton).toBeTruthy()
    marcoButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/faces/f1/assign',
      expect.objectContaining({ method: 'POST', body: JSON.stringify({ person_id: 'p9' }) })
    )
  })

  it('"Vai alla persona" closes the lightbox and navigates to the person detail route', async () => {
    const { createMemoryHistory, createRouter } = await import('vue-router')
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/', component: { template: '<div/>' } }, { path: '/persons/:id', component: { template: '<div/>' } }]
    })
    await router.push('/')
    await router.isReady()

    mockPanelFetch({ faces: [face('f1', 'p1')] })
    wrapper = mount(AssetViewer, {
      props: {
        asset: personWithFaces([{ person_id: 'p1', person_name: 'Anna' }]),
        isFavorite: false
      },
      global: { plugins: [i18n, router], stubs: { MapClusterLayer: true } },
      attachTo: document.body
    })
    await flushPromises()

    const chip = wrapper.findAll('button').find((b) => b.text() === 'Anna')!
    await chip.trigger('click')
    await flushPromises()
    const goToPersonButton = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Vai alla persona')
    )
    goToPersonButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(wrapper.emitted('close')).toBeTruthy()
    expect(router.currentRoute.value.path).toBe('/persons/p1')
  })
})

describe('AssetViewer — TAG section', () => {
  function tag(id: string, opts: Partial<AssetTagDetail> = {}): AssetTagDetail {
    return { id, name: id, color: '#3b82f6', category_id: null, state: 'confirmed', source: 'user', ...opts }
  }

  function category(id: string, name: string): Tag {
    return { id, name, kind: 'category', parent_id: null, color: null, assignment_count: 0 }
  }

  function mockPanelFetch(opts: { assetTags?: AssetTagDetail[]; categories?: Tag[] } = {}) {
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (path.endsWith('/map/regions')) return Promise.resolve([])
      if (path.endsWith('/metadata') && method === 'GET') {
        return Promise.resolve({
          title: null,
          description: null,
          taken_at: null,
          location: null,
          place_id: null,
          orientation: null
        })
      }
      if (path.endsWith('/flags') && method === 'GET') {
        return Promise.resolve({ rating: null, pick: 'none', color_label: null, favorite: false })
      }
      if (/\/assets\/.+\/tags$/.test(path) && method === 'GET') return Promise.resolve(opts.assetTags ?? [])
      if (path === '/api/v1/tags' && method === 'GET') return Promise.resolve(opts.categories ?? [])
      if (/\/tags\/.+\/assets\/.+\/(confirm|reject|remove)$/.test(path) && method === 'POST') {
        return Promise.resolve(null)
      }
      // GET /api/v1/assets/{id} (detail)
      return Promise.resolve({ ...photo('a') })
    })
  }

  // Same reason as the PEOPLE block above: the `keydown` listener stays
  // alive until the wrapper is unmounted.
  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  it('groups confirmed tags by category, "Senza categoria" last, and renders the "+ aggiungi" chip', async () => {
    mockPanelFetch({
      assetTags: [
        tag('t1', { name: 'Spiaggia', category_id: 'cat-luoghi' }),
        tag('t2', { name: 'Senza tag' }),
        tag('t3', { name: 'Estate', category_id: 'cat-stagioni' })
      ],
      categories: [category('cat-stagioni', 'Stagioni'), category('cat-luoghi', 'Luoghi')]
    })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    const text = wrapper.text()
    expect(text).toContain('Luoghi')
    expect(text).toContain('Spiaggia')
    expect(text).toContain('Stagioni')
    expect(text).toContain('Estate')
    expect(text).toContain('Senza categoria')
    expect(text).toContain('Senza tag')
    expect(text.indexOf('Senza categoria')).toBeGreaterThan(text.indexOf('Stagioni'))
    expect(wrapper.text()).toContain('+ aggiungi')
  })

  it('"×" on a confirmed tag removes it permanently and shows the exact toast', async () => {
    mockPanelFetch({ assetTags: [tag('t1', { name: 'Spiaggia' })] })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    const toast = useToastStore()
    await flushPromises()

    await wrapper.get('[aria-label="Rimuovi tag Spiaggia"]').trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/tags/t1/assets/a/remove',
      expect.objectContaining({ method: 'POST' })
    )
    expect(toast.toasts.at(-1)?.message).toBe('Tag rimosso.')
  })

  it('proposed tags render in a separate "In attesa di conferma" section; ✓ confirms and × rejects', async () => {
    mockPanelFetch({ assetTags: [tag('t1', { name: 'Forse spiaggia', state: 'proposed', source: 'ai' })] })
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    const toast = useToastStore()
    await flushPromises()

    expect(wrapper.text()).toContain('In attesa di conferma')
    expect(wrapper.text()).toContain('Forse spiaggia')

    await wrapper.get('[aria-label="Conferma tag Forse spiaggia"]').trigger('click')
    await flushPromises()
    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/tags/t1/assets/a/confirm',
      expect.objectContaining({ method: 'POST' })
    )
    expect(toast.toasts.at(-1)?.message).toBe('Tag confermato.')

    mockPanelFetch({ assetTags: [tag('t1', { name: 'Forse spiaggia', state: 'proposed', source: 'ai' })] })
    await wrapper.get('[aria-label="Rifiuta suggerimento Forse spiaggia"]').trigger('click')
    await flushPromises()
    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/tags/t1/assets/a/reject',
      expect.objectContaining({ method: 'POST' })
    )
    expect(toast.toasts.at(-1)?.message).toBe('Suggerimento rifiutato — non verrà riproposto.')
  })

  it('"+ aggiungi" opens the shared TagPickerDialog', async () => {
    mockPanelFetch()
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } },
      attachTo: document.body
    })
    await flushPromises()

    const addButton = wrapper.findAll('button').find((b) => b.text() === '+ aggiungi')!
    await addButton.trigger('click')
    await flushPromises()

    expect(document.body.textContent).toContain('Aggiungi tag')
  })
})

describe('AssetViewer — ALBUM and ACTIONS sections', () => {
  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  it('lists the albums the asset belongs to (read-only), and "+ aggiungi" opens AlbumPickerDialog', async () => {
    fetchAlbumsForAssetMock.mockResolvedValue([
      { id: 'al1', name: 'Vacanze 2024' },
      { id: 'al2', name: 'Famiglia' }
    ])
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } },
      attachTo: document.body
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Vacanze 2024')
    expect(wrapper.text()).toContain('Famiglia')

    const addButtons = wrapper.findAll('button').filter((b) => b.text() === '+ aggiungi')
    // The second "+ aggiungi" is the ALBUM section's (the first is TAG,
    // same text by construction — see the TAG section above).
    await addButtons[1]!.trigger('click')
    await flushPromises()

    expect(document.body.textContent).toContain('Aggiungi ad album')
  })

  it('Esc closes only AlbumPickerDialog, not the lightbox underneath — and reloads the album list', async () => {
    fetchAlbumsForAssetMock.mockResolvedValue([])
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()
    fetchAlbumsForAssetMock.mockClear()

    const addButtons = wrapper.findAll('button').filter((b) => b.text() === '+ aggiungi')
    await addButtons[1]!.trigger('click')
    await flushPromises()
    expect(fetchAlbumsForAssetMock).not.toHaveBeenCalled()

    // Real bug found while writing this test: `AssetViewer` handles Esc
    // with a hand-written `window.addEventListener`, which doesn't
    // coordinate at all with reka-ui's internal Esc handling — without
    // the explicit check across the six dialogs, this same keypress would
    // have also closed the lightbox underneath the dialog (`close`
    // emitted), not just the dialog.
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(wrapper.emitted('close')).toBeUndefined()
    expect(fetchAlbumsForAssetMock).toHaveBeenCalledWith('a')
  })

  it('ACTIONS section: renders the same six actions as the ⋯ menu, as visible buttons', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    const downloadLink = wrapper.findAll('a').find((a) => a.text() === 'Scarica originale')!
    expect(downloadLink.attributes('href')).toBe('/media/original/a')
    expect(downloadLink.attributes('download')).toBe('a.jpg')

    await wrapper.findAll('button').find((b) => b.text() === 'Ruota')!.trigger('click')
    await flushPromises()
    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/assets/a/metadata',
      expect.objectContaining({ method: 'PATCH', body: JSON.stringify({ orientation: 90 }) })
    )

    await wrapper.findAll('button').find((b) => b.text() === 'Rinomina…')!.trigger('click')
    expect(document.body.textContent).toContain('1 foto — a.jpg')
  })

  it('"Ruota" wraps back to 0° after four clicks — a full turn, not an ever-growing value', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()
    const rotateBtn = wrapper.findAll('button').find((b) => b.text() === 'Ruota')!

    for (const expected of [90, 180, 270, 0]) {
      await rotateBtn.trigger('click')
      await flushPromises()
      expect(apiFetch).toHaveBeenLastCalledWith(
        '/api/v1/assets/a/metadata',
        expect.objectContaining({ method: 'PATCH', body: JSON.stringify({ orientation: expected }) })
      )
    }
  })

  it('a failed rotate reverts the optimistic orientation change and shows an error toast', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    const toast = useToastStore()
    await flushPromises()
    apiFetch.mockImplementationOnce(() => Promise.reject(new Error('network error')))

    await wrapper.findAll('button').find((b) => b.text() === 'Ruota')!.trigger('click')
    await flushPromises()
    // Reverted: the *next* rotate still starts from 0°, not 90° (which it
    // would if the optimistic write to metadata.value.orientation had
    // stuck around after the request failed).
    await wrapper.findAll('button').find((b) => b.text() === 'Ruota')!.trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenLastCalledWith(
      '/api/v1/assets/a/metadata',
      expect.objectContaining({ method: 'PATCH', body: JSON.stringify({ orientation: 90 }) })
    )
    expect(toast.toasts.some((t) => t.kind === 'error')).toBe(true)
  })

  it('"Condividi…" opens ShareSelectionDialog for this single asset', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    await wrapper.findAll('button').find((b) => b.text() === 'Condividi…')!.trigger('click')
    await flushPromises()

    expect(document.body.textContent).toContain('Condividi 1 elemento')
  })
})

describe('AssetViewer — RAW/JPEG switcher', () => {
  function stackMember(id: string, rawKind: 'raw' | 'jpeg', sizeBytes: number) {
    return { ...photo(id), raw_kind: rawKind, size_bytes: sizeBytes, content_hash: `${id}${'b'.repeat(63)}`.slice(0, 64) }
  }

  function mockPanelFetch(opts: { members?: ReturnType<typeof stackMember>[] } = {}) {
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (isArrayEndpoint(path)) return Promise.resolve([])
      if (path.endsWith('/metadata') && method === 'GET') {
        return Promise.resolve({
          title: null,
          description: null,
          taken_at: null,
          location: null,
          place_id: null,
          orientation: null
        })
      }
      if (path.endsWith('/flags') && method === 'GET') {
        return Promise.resolve({ rating: null, pick: 'none', color_label: null, favorite: false })
      }
      if (path.endsWith('/stack') && method === 'GET') {
        return Promise.resolve({ stack_id: 's1', primary_asset_id: 'a', members: opts.members ?? [] })
      }
      // GET /api/v1/assets/{id} (detail)
      return Promise.resolve({ ...photo('a') })
    })
  }

  let wrapper: ReturnType<typeof mount> | undefined
  afterEach(() => wrapper?.unmount())

  it('raw+jpeg: renders both chips; clicking the RAW one switches the stage image and the download target', async () => {
    const raw = stackMember('a-raw', 'raw', 62_000_000)
    const jpg = stackMember('a', 'jpeg', 4_200_000)
    mockPanelFetch({ members: [raw, jpg] })
    wrapper = mount(AssetViewer, {
      props: { asset: { ...photo('a'), raw_kind: 'raw+jpeg' }, isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('RAW · 62 MB')
    expect(wrapper.text()).toContain('JPEG · 4,2 MB')
    // Opened as "a" (the JPEG member): the stage already shows that file.
    expect(wrapper.get('img[alt="a.jpg"]').attributes('src')).toBe(previewSrc(jpg.content_hash!))

    const rawButton = wrapper.findAll('button').find((b) => b.text() === 'RAW · 62 MB')!
    await rawButton.trigger('click')

    expect(wrapper.get('img[alt="a.jpg"]').attributes('src')).toBe(previewSrc(raw.content_hash!))
    const downloadLink = wrapper.findAll('a').find((a) => a.text() === 'Scarica originale')!
    expect(downloadLink.attributes('href')).toBe('/media/original/a-raw')
    expect(downloadLink.attributes('download')).toBe('a-raw.jpg')
  })

  it('raw_only (no JPEG sibling): a single non-clickable chip with the exact label', async () => {
    mockPanelFetch({ members: [] })
    wrapper = mount(AssetViewer, {
      props: { asset: { ...photo('a'), raw_kind: 'raw', size_bytes: 62_000_000 }, isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('RAW · 62 MB · nessun JPEG associato')
    expect(wrapper.findAll('button').filter((b) => b.text().startsWith('RAW'))).toHaveLength(0)
  })

  it('plain jpeg (raw_kind null): no RAW/JPEG block at all, and no /stack request', async () => {
    mockPanelFetch()
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    await flushPromises()

    expect(wrapper.text()).not.toContain('RAW')
    expect(wrapper.text()).not.toContain('JPEG ·')
    expect(apiFetch).not.toHaveBeenCalledWith(expect.stringContaining('/stack'))
  })
})
