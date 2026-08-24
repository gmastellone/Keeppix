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
    apiFetch.mockResolvedValue({
      title: null,
      description: null,
      taken_at: null,
      location: { lat: 41.9, lon: 12.5 },
      place_id: null,
      orientation: null
    })
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
    apiFetch.mockImplementation((path: string) =>
      path.includes('/aaaa/')
        ? firstMetadata.promise
        : secondMetadata.promise
    )
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
