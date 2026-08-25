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
const deleteAssetsBatchMock = vi.fn()

vi.mock('@/api/culling', () => ({
  fetchFlags: (...args: unknown[]) => fetchFlagsMock(...args),
  setFlags: (...args: unknown[]) => setFlagsMock(...args),
  deleteAssetsBatch: (...args: unknown[]) => deleteAssetsBatchMock(...args),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
}))

const fetchAlbumsMock = vi.fn()
const fetchAlbumMock = vi.fn()
const createAlbumMock = vi.fn()
const addAssetsMock = vi.fn()

vi.mock('@/api/albums', () => ({
  fetchAlbums: (...args: unknown[]) => fetchAlbumsMock(...args),
  fetchAlbum: (...args: unknown[]) => fetchAlbumMock(...args),
  createAlbum: (...args: unknown[]) => createAlbumMock(...args),
  addAssets: (...args: unknown[]) => addAssetsMock(...args),
  removeAsset: vi.fn(async () => null)
}))

const fetchUsersMock = vi.fn()
vi.mock('@/api/users', () => ({
  fetchUsers: (...args: unknown[]) => fetchUsersMock(...args)
}))

const grantPermissionMock = vi.fn()
const revokePermissionMock = vi.fn()
vi.mock('@/api/permissions', () => ({
  grantPermission: (...args: unknown[]) => grantPermissionMock(...args),
  revokePermission: (...args: unknown[]) => revokePermissionMock(...args)
}))

const createShareLinkMock = vi.fn()
vi.mock('@/api/shares', () => ({
  createShareLink: (...args: unknown[]) => createShareLinkMock(...args)
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
  deleteAssetsBatchMock.mockResolvedValue({ succeeded: [], failed: [], batch_id: null })
  fetchAlbumsMock.mockResolvedValue([])
  fetchAlbumMock.mockResolvedValue({ id: 'x', name: '', assets: [] })
  createAlbumMock.mockResolvedValue({ id: 'album-1', name: 'x', cover_hash: null, created_at: '' })
  addAssetsMock.mockResolvedValue(null)
  fetchUsersMock.mockResolvedValue([])
  grantPermissionMock.mockResolvedValue({ id: 'grant-1' })
  revokePermissionMock.mockResolvedValue(null)
  createShareLinkMock.mockResolvedValue({ id: 'link-1', token: 'tok123' })
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
})

describe('LibrarySelectionActions — the five documented buttons (§12.2-§12.3)', () => {
  it('renders all five action buttons in order: Preferiti, Album, Condividi, Modifica, Elimina', async () => {
    const w = await mountActions([photo('a')])
    const labels = w.findAll('button').map((b) => b.attributes('aria-label'))
    expect(labels).toEqual([
      'Aggiungi o rimuovi dai preferiti',
      'Aggiungi ad album',
      'Condividi selezione',
      'Modifica in blocco',
      'Elimina selezione'
    ])
  })
})

describe('LibrarySelectionActions — Condividi (Task 11, §30: an auto-generated album stands in for the non-existent "selection" object type)', () => {
  it('creating a public link auto-creates a hidden album with the selection, then shares that album', async () => {
    const w = await mountActions([photo('a'), photo('b')])
    const toast = useToastStore()

    await w.get('[aria-label="Condividi selezione"]').trigger('click')
    await tick()

    const createLinkBtn = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Crea link di condivisione')
    )
    createLinkBtn?.click()
    await tick()

    expect(createAlbumMock).toHaveBeenCalledTimes(1)
    expect(addAssetsMock).toHaveBeenCalledWith('album-1', ['a', 'b'])
    expect(createShareLinkMock).toHaveBeenCalledWith({ object_type: 'album', object_id: 'album-1' })
    expect(toast.toasts.at(-1)?.message).toBe('Link creato e copiato negli appunti.')
  })

  it('without admin rights, the "Persone" section is hidden — only the public link is offered', async () => {
    const w = await mountActions([photo('a')])

    await w.get('[aria-label="Condividi selezione"]').trigger('click')
    await tick()

    expect(fetchUsersMock).not.toHaveBeenCalled()
    expect(document.body.textContent).not.toContain('Persone')
    expect(document.body.textContent).toContain('Link pubblico')
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
  it('deletes the whole selection with one batch call, not a per-asset loop, and clears the selection', async () => {
    deleteAssetsBatchMock.mockResolvedValue({ succeeded: ['a', 'b'], failed: [], batch_id: null })
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

    expect(deleteAssetsBatchMock).toHaveBeenCalledTimes(1)
    expect(deleteAssetsBatchMock).toHaveBeenCalledWith(['a', 'b'], 'purged')
    expect(selection.library.selectedIds.size).toBe(0)
  })

  it('shows the exact documented toast on full success — §12.3 "N foto eliminate."', async () => {
    deleteAssetsBatchMock.mockResolvedValue({ succeeded: ['a'], failed: [], batch_id: null })
    const w = await mountActions([photo('a')])
    const toast = useToastStore()

    await w.get('[aria-label="Elimina selezione"]').trigger('click')
    await tick()
    const indexOption = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes("Rimuovi solo dall'indice")
    )
    indexOption?.click()
    await tick()

    expect(deleteAssetsBatchMock).toHaveBeenCalledWith(['a'], 'kept')
    expect(toast.toasts.at(-1)?.message).toBe('1 foto eliminata.')
  })

  it('shows a partial-failure toast when the batch outcome reports some failures', async () => {
    deleteAssetsBatchMock.mockResolvedValue({
      succeeded: ['a'],
      failed: [{ id: 'b', reason: 'forbidden' }],
      batch_id: null
    })
    const w = await mountActions([photo('a'), photo('b')])
    const toast = useToastStore()

    await w.get('[aria-label="Elimina selezione"]').trigger('click')
    await tick()
    const trashOption = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Sposta nel cestino')
    )
    trashOption?.click()
    await tick()

    expect(toast.toasts.at(-1)?.message).toBe('1 su 2 completate — 1 non è riuscita.')
  })

  it('§10 pre-merge: a rejected all-or-nothing purge (server 403 on the whole batch) touches no file and shows the error toast, not a partial one', async () => {
    // `purged` è l'unica opzione con l'autorizzazione all-or-nothing sul
    // server (`routes::trash::batch_delete`): un asset non purgabile
    // rifiuta l'intero lotto PRIMA che qualunque file venga toccato — la
    // promise va in reject, nessun `BulkOutcome` torna affatto.
    deleteAssetsBatchMock.mockRejectedValue(new Error('403 forbidden'))
    const w = await mountActions([photo('a'), photo('b')])
    const selection = useSelectionStore()
    selection.library.toggle('a')
    selection.library.toggle('b')
    const toast = useToastStore()

    await w.get('[aria-label="Elimina selezione"]').trigger('click')
    await tick()
    const diskOption = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Elimina dal disco adesso')
    )
    diskOption?.click()
    await tick()

    expect(deleteAssetsBatchMock).toHaveBeenCalledTimes(1)
    expect(toast.toasts.at(-1)?.message).toBe("Non è stato possibile completare l'eliminazione. Riprova.")
    expect(selection.library.selectedIds.size).toBe(0)
  })
})
