import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'
import { useToastStore } from '@/stores/toast'

import AssetViewer from './AssetViewer.vue'
import { previewSrc } from '@/api/media'

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

// AlbumPickerDialog/RenameFormulaDialog restano montati (chiusi) sempre:
// senza questi mock la loro `watch(open,...)` innocua a `open=false` non
// scatterebbe, ma se un test li apre andrebbero al vero `apiFetch`
// (mockato sopra a vuoto) — stesso correttivo già di BatchEditView.spec.ts.
vi.mock('@/api/albums', () => ({
  fetchAlbums: vi.fn(async () => []),
  fetchAlbum: vi.fn(async () => ({ id: 'x', name: '', assets: [] }))
}))
vi.mock('@/api/rename', () => ({
  previewRename: vi.fn(async () => []),
  applyRenameBatch: vi.fn(async () => ({ operation_id: 'op', outcome: { succeeded: [], failed: [], batch_id: null } }))
}))

afterEach(() => {
  apiFetch.mockReset()
  deleteAssetMock.mockReset()
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
})

describe('AssetViewer — stage, frecce, filmino (§18.2-18.3)', () => {
  it('renders no arrows/filmstrip without neighbors, and never closes on background click (§18.4)', () => {
    const wrapper = mount(AssetViewer, {
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
    const wrapper = mount(AssetViewer, {
      props: { asset: b, neighbors: [a, b, c], isFavorite: false },
      global: { plugins: [i18n] }
    })

    // "b" è in mezzo: entrambe le frecce esistono.
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
    const wrapper = mount(AssetViewer, {
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
    const wrapper = mount(AssetViewer, {
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
    const wrapper = mount(AssetViewer, {
      props: { asset: first, isFavorite: false },
      global: { plugins: [i18n] }
    })
    expect(wrapper.get('img[alt="aaaa.jpg"]').attributes('src')).toBe(previewSrc(first.content_hash!))

    await wrapper.setProps({ asset: second })
    expect(wrapper.get('img[alt="bbbb.jpg"]').attributes('src')).toBe(previewSrc(second.content_hash!))
  })
})

describe('AssetViewer — barra superiore (§18.3)', () => {
  it('close/favorite/info buttons work; the heart reflects isFavorite and toggles via "f"', async () => {
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: true },
      global: { plugins: [i18n] }
    })
    expect(wrapper.find('[aria-label="Rimuovi dai preferiti"]').exists()).toBe(true)

    await wrapper.get('[aria-label="Chiudi"]').trigger('click')
    expect(wrapper.emitted('close')).toHaveLength(1)

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'f' }))
    expect(wrapper.emitted('toggle-favorite')).toHaveLength(1)
  })

  it('§18.5: Esc closes the ⋯ menu on the first press, the lightbox only on the second', async () => {
    const wrapper = mount(AssetViewer, {
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

describe('AssetViewer — menu ⋯ (§20)', () => {
  function menuItemWithText(text: string) {
    return Array.from(document.body.querySelectorAll('a,button')).find((el) => el.textContent?.trim() === text)
  }

  // I tre test montano con `attachTo: document.body` (serve al popover
  // teletrasportato): senza smontare, il DOM del test precedente resta
  // attaccato e `menuItemWithText` può trovare il bottone sbagliato.
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

  it('"Ruota" is still a demo toast (declared debt — no rotation pipeline yet)', async () => {
    wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] },
      attachTo: document.body
    })
    const toast = useToastStore()
    await wrapper.get('[aria-label="Altre azioni"]').trigger('click')
    await flushPromises()

    menuItemWithText('Ruota')?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()

    expect(toast.toasts.at(-1)?.message).toContain('Solo demo')
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

describe('AssetViewer — pannello informazioni (mini-mappa)', () => {
  it('shows a compact cluster map only when effective metadata has a location', async () => {
    apiFetch.mockImplementation((path: string) =>
      path.endsWith('/map/regions')
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
    const wrapper = mount(AssetViewer, {
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

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/aaaa/metadata')
    expect(wrapper.find('[data-testid="mini-map"]').exists()).toBe(true)
  })

  it('ignores stale metadata when the viewed asset changes', async () => {
    const firstMetadata = deferred<{ location: { lat: number; lon: number } }>()
    const secondMetadata = deferred<{ location: { lat: number; lon: number } }>()
    apiFetch.mockImplementation((path: string) => {
      if (path.endsWith('/map/regions')) return Promise.resolve([])
      return path.includes('/aaaa/') ? firstMetadata.promise : secondMetadata.promise
    })
    const wrapper = mount(AssetViewer, {
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

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    await wrapper.setProps({ asset: photo('bbbb'), isFavorite: false })
    secondMetadata.resolve({ location: { lat: 45, lon: 9 } })
    await flushPromises()
    expect(wrapper.get('[data-testid="mini-map"]').text()).toBe('45,9')

    firstMetadata.resolve({ location: { lat: 41.9, lon: 12.5 } })
    await flushPromises()
    expect(wrapper.get('[data-testid="mini-map"]').text()).toBe('45,9')
  })
})

describe('AssetViewer — titolo, valutazione, scatto (§19.2-19.3)', () => {
  function mockPanelFetch(opts: {
    title?: string | null
    rating?: number | null
    exif?: Record<string, unknown>
    location?: { lat: number; lon: number } | null
  } = {}) {
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (path.endsWith('/map/regions')) return Promise.resolve([])
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
      // GET /api/v1/assets/{id} (dettaglio, mai /metadata né /flags)
      return Promise.resolve({ ...photo('a'), full_exif: opts.exif })
    })
  }

  it('carica il titolo esistente nel campo; lasciarlo vuoto lo azzera a null (non stringa vuota) e salva solo al change', async () => {
    mockPanelFetch({ title: 'Tramonto' })
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
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

  it('trims the title and sends it only on change, not on every keystroke (§19.3: onchange, not oninput)', async () => {
    mockPanelFetch({ title: null })
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    await flushPromises()
    apiFetch.mockClear()

    // Simula la digitazione (solo `input`, mai `change`) senza passare da
    // `setValue()`: quel metodo di vue-test-utils spara *entrambi* gli
    // eventi per gli `<input>`, quindi non potrebbe distinguere `onchange`
    // da `oninput` — esattamente ciò che questo test verifica.
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

  it('§19.3: click on a star sets the rating, a second click on the same star resets it to 0', async () => {
    mockPanelFetch({ rating: 2 })
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
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

  it('§19.2 SCATTO: shows camera/lens/exposure/dimensions only when the exif carries them', async () => {
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
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    await flushPromises()

    expect(wrapper.text()).toContain('Sony α7R V')
    expect(wrapper.text()).toContain('FE 24-70mm')
    expect(wrapper.text()).toContain('f/3.5 · 1/250s · ISO 400')
    expect(wrapper.text()).toContain('100×100')
  })

  it('§19.2 SCATTO: omits camera/lens/exposure rows the asset has no exif for, keeping dimensions (sourced from the asset itself, not exif)', async () => {
    mockPanelFetch({ exif: undefined })
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n] }
    })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    await flushPromises()

    expect(wrapper.text()).not.toContain('Fotocamera')
    expect(wrapper.text()).not.toContain('Obiettivo')
    expect(wrapper.text()).not.toContain('Esposizione')
    expect(wrapper.text()).toContain('100×100')
  })
})

describe('AssetViewer — sezione POSIZIONE (§19.2-19.3)', () => {
  function mockPanelFetch(location: { lat: number; lon: number } | null) {
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (path.endsWith('/map/regions')) return Promise.resolve([])
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
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    await flushPromises()

    expect(wrapper.text()).toContain('Nessuna posizione impostata.')
    expect(wrapper.find('button').exists()).toBe(true)
    expect(wrapper.text()).toContain('Imposta posizione…')
    expect(wrapper.text()).not.toContain('Modifica posizione…')
  })

  it('shows coordinates (4 decimals) and "Modifica posizione…" when the asset has a location', async () => {
    mockPanelFetch({ lat: 41.9, lon: 12.5 })
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: { plugins: [i18n], stubs: { MapClusterLayer: true } }
    })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    await flushPromises()

    expect(wrapper.text()).toContain('41.9000, 12.5000')
    expect(wrapper.text()).toContain('Modifica posizione…')
    expect(wrapper.text()).not.toContain('Nessuna posizione impostata.')
  })

  it('opens the position dialog on click, and "Nessuna posizione" clears the location and reloads the panel', async () => {
    mockPanelFetch({ lat: 41.9, lon: 12.5 })
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('a'), isFavorite: false },
      global: {
        plugins: [i18n],
        stubs: { MapClusterLayer: true }
      },
      attachTo: document.body
    })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
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
