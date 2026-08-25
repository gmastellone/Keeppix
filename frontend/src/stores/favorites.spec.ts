import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'

const fetchFlagsMock = vi.fn()
const setFlagsMock = vi.fn()

vi.mock('@/api/culling', () => ({
  fetchFlags: (...args: unknown[]) => fetchFlagsMock(...args),
  setFlags: (...args: unknown[]) => setFlagsMock(...args),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
}))

const { useFavoritesStore } = await import('./favorites')
const { useToastStore } = await import('./toast')
const { i18n } = await import('@/i18n')

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

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  fetchFlagsMock.mockResolvedValue({ rating: 3, pick: 'pick', color_label: 'red', favorite: false })
  setFlagsMock.mockResolvedValue(null)
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  vi.useRealTimers()
})

describe('favorites store — toggleOne', () => {
  it('reads the full current flags before writing, so rating/pick/color_label survive the toggle', async () => {
    const store = useFavoritesStore()
    await store.toggleOne(photo('a'))

    expect(fetchFlagsMock).toHaveBeenCalledWith('a')
    expect(setFlagsMock).toHaveBeenCalledWith('a', { rating: 3, pick: 'pick', color_label: 'red', favorite: true })
  })

  it('is optimistic: isFavorite flips immediately, before the write resolves', () => {
    const store = useFavoritesStore()
    const asset = photo('a', false)

    void store.toggleOne(asset)

    expect(store.isFavorite(asset)).toBe(true)
  })

  it('toggles false → true and true → false correctly against the overlay', async () => {
    const store = useFavoritesStore()
    const asset = photo('a', true)

    await store.toggleOne(asset)
    expect(store.isFavorite(asset)).toBe(false)

    await store.toggleOne(asset)
    expect(store.isFavorite(asset)).toBe(true)
  })

  it('rolls back the optimistic flip and shows no toast, only an error, when the write fails', async () => {
    setFlagsMock.mockRejectedValue(new Error('network'))
    const store = useFavoritesStore()
    const toast = useToastStore()
    const asset = photo('a', false)

    await store.toggleOne(asset)

    expect(store.isFavorite(asset)).toBe(false)
    expect(toast.toasts.at(-1)?.kind).toBe('error')
  })

  it('falls back to unvotedFlags — not a thrown error — when fetchFlags itself fails', async () => {
    fetchFlagsMock.mockRejectedValue(new Error('gone'))
    const store = useFavoritesStore()
    await store.toggleOne(photo('a'))

    expect(setFlagsMock).toHaveBeenCalledWith('a', { rating: null, pick: 'none', color_label: null, favorite: true })
  })
})

describe('favorites store — setMany (SP-2 group toggle)', () => {
  it('is a no-op on an empty selection: no toast, no writes', async () => {
    const store = useFavoritesStore()
    const toast = useToastStore()
    await store.setMany([], true)

    expect(setFlagsMock).not.toHaveBeenCalled()
    expect(toast.toasts).toHaveLength(0)
  })

  it('applies to every asset and shows the exact documented toast on full success — §12.3', async () => {
    const store = useFavoritesStore()
    const toast = useToastStore()
    const assets = [photo('a'), photo('b'), photo('c')]

    await store.setMany(assets, true)

    expect(setFlagsMock).toHaveBeenCalledTimes(3)
    assets.forEach((asset) => expect(store.isFavorite(asset)).toBe(true))
    expect(toast.toasts.at(-1)?.message).toBe('Aggiunti ai preferiti.')
    expect(toast.toasts.at(-1)?.kind).toBe('ok')
  })

  it('shows the exact "removed" wording when removing — §12.3', async () => {
    const store = useFavoritesStore()
    const toast = useToastStore()
    await store.setMany([photo('a', true)], false)

    expect(toast.toasts.at(-1)?.message).toBe('Rimossi dai preferiti.')
  })

  it('rolls back only the assets whose write failed, and reports a partial-success toast', async () => {
    setFlagsMock.mockImplementation((id: string) => (id === 'b' ? Promise.reject(new Error('x')) : Promise.resolve(null)))
    const store = useFavoritesStore()
    const toast = useToastStore()
    const assets = [photo('a'), photo('b'), photo('c')]

    await store.setMany(assets, true)

    expect(store.isFavorite(assets[0])).toBe(true)
    expect(store.isFavorite(assets[1])).toBe(false)
    expect(store.isFavorite(assets[2])).toBe(true)
    expect(toast.toasts.at(-1)?.kind).toBe('partial')
  })

  it('shows a plain error toast, not a partial one, when every write in the batch fails', async () => {
    setFlagsMock.mockRejectedValue(new Error('x'))
    const store = useFavoritesStore()
    const toast = useToastStore()
    const assets = [photo('a'), photo('b')]

    await store.setMany(assets, true)

    assets.forEach((asset) => expect(store.isFavorite(asset)).toBe(false))
    expect(toast.toasts.at(-1)?.kind).toBe('error')
  })
})
