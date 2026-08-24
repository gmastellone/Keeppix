import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'

import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'
import { useFavoritesStore } from '@/stores/favorites'
import { useSelectionStore } from '@/stores/selection'
import { useToastStore } from '@/stores/toast'

const fetchFlagsMock = vi.fn()
const setFlagsMock = vi.fn()
const deleteAssetMock = vi.fn()

vi.mock('@/api/culling', () => ({
  fetchFlags: (...args: unknown[]) => fetchFlagsMock(...args),
  setFlags: (...args: unknown[]) => setFlagsMock(...args),
  deleteAsset: (...args: unknown[]) => deleteAssetMock(...args),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
}))

const fetchAlbumsMock = vi.fn()
const fetchAlbumMock = vi.fn()

vi.mock('@/api/albums', () => ({
  fetchAlbums: (...args: unknown[]) => fetchAlbumsMock(...args),
  fetchAlbum: (...args: unknown[]) => fetchAlbumMock(...args),
  addAssets: vi.fn(async () => null),
  removeAsset: vi.fn(async () => null)
}))

const LibrarySelectionActions = (await import('./LibrarySelectionActions.vue')).default

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function photo(id: string, favorite = false): TimelineAsset {
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
    favorite,
    camera_model: null,
    tags: [],
    faces: []
  }
}

let wrapper: VueWrapper | undefined
let router: Router

async function mountActions(assets: TimelineAsset[]) {
  router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/batch-edit', component: { template: '<div />' } }
    ]
  })
  await router.push('/')
  wrapper = mount(LibrarySelectionActions, {
    props: { assets },
    global: { plugins: [router, i18n] },
    attachTo: document.body
  })
  return wrapper
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  i18n.global.locale.value = 'it'
  fetchFlagsMock.mockResolvedValue({ rating: null, pick: 'none', color_label: null, favorite: false })
  setFlagsMock.mockResolvedValue(null)
  deleteAssetMock.mockResolvedValue(null)
  fetchAlbumsMock.mockResolvedValue([])
  fetchAlbumMock.mockResolvedValue({ id: 'x', name: '', assets: [] })
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
})

describe('LibrarySelectionActions — "Condividi" is a declared omission, not a stub (backend has no ad-hoc-selection share)', () => {
  it('renders exactly four action buttons: Preferiti, Album, Modifica, Elimina — no fifth "Condividi"', async () => {
    const w = await mountActions([photo('a')])
    const labels = w.findAll('button').map((b) => b.attributes('aria-label'))
    expect(labels).toEqual([
      'Aggiungi o rimuovi dai preferiti',
      'Aggiungi ad album',
      'Modifica in blocco',
      'Elimina selezione'
    ])
  })
})

describe('LibrarySelectionActions — Preferiti (§12.3 group toggle)', () => {
  it('adds every selected photo to favorites when not all of them are favorite yet', async () => {
    const w = await mountActions([photo('a', false), photo('b', true)])
    const favorites = useFavoritesStore()
    const toast = useToastStore()

    await w.get('[aria-label="Aggiungi o rimuovi dai preferiti"]').trigger('click')
    await tick()

    expect(setFlagsMock).toHaveBeenCalledTimes(2)
    expect(favorites.isFavorite(photo('a'))).toBe(true)
    expect(toast.toasts.at(-1)?.message).toBe('Aggiunti ai preferiti.')
  })

  it('removes every selected photo from favorites once they are all already favorite', async () => {
    const w = await mountActions([photo('a', true), photo('b', true)])
    const toast = useToastStore()

    await w.get('[aria-label="Aggiungi o rimuovi dai preferiti"]').trigger('click')
    await tick()

    expect(setFlagsMock).toHaveBeenCalledWith('a', expect.objectContaining({ favorite: false }))
    expect(toast.toasts.at(-1)?.message).toBe('Rimossi dai preferiti.')
  })
})

describe('LibrarySelectionActions — Modifica (§12.3)', () => {
  it('navigates to /batch-edit with the selected ids in the query', async () => {
    const w = await mountActions([photo('a'), photo('b')])

    await w.get('[aria-label="Modifica in blocco"]').trigger('click')
    await tick()

    expect(router.currentRoute.value.path).toBe('/batch-edit')
    expect(router.currentRoute.value.query.ids).toBe('a,b')
  })
})

describe('LibrarySelectionActions — Elimina (§12.3, three-way dialog + selection cleared)', () => {
  it('deletes every selected asset with the chosen disk action and clears the selection', async () => {
    const w = await mountActions([photo('a'), photo('b')])
    const selection = useSelectionStore()
    selection.library.toggle('a')
    selection.library.toggle('b')

    await w.get('[aria-label="Elimina selezione"]').trigger('click')
    await tick()

    const diskOption = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Elimina dal disco adesso')
    )
    diskOption?.click()
    await tick()

    expect(deleteAssetMock).toHaveBeenCalledWith('a', 'purged')
    expect(deleteAssetMock).toHaveBeenCalledWith('b', 'purged')
    expect(selection.library.selectedIds.size).toBe(0)
  })

  it('shows the exact documented toast on full success — §12.3 "N foto eliminate."', async () => {
    const w = await mountActions([photo('a')])
    const toast = useToastStore()

    await w.get('[aria-label="Elimina selezione"]').trigger('click')
    await tick()
    const indexOption = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes("Rimuovi solo dall'indice")
    )
    indexOption?.click()
    await tick()

    expect(deleteAssetMock).toHaveBeenCalledWith('a', 'kept')
    expect(toast.toasts.at(-1)?.message).toBe('1 foto eliminata.')
  })
})
