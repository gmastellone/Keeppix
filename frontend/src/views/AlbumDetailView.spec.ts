import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import type { Album, AlbumAsset } from '@/api/albums'
import { i18n } from '@/i18n'
import PhotoTile from '@/components/ui/PhotoTile.vue'
import ErrorState from '@/components/ui/ErrorState.vue'
import { useSessionStore } from '@/stores/session'

import AlbumDetailView from './AlbumDetailView.vue'

const fetchAlbumMock = vi.fn()
const fetchAlbumAssetsMock = vi.fn()
const refreshAlbumMock = vi.fn()

vi.mock('@/api/albums', () => ({
  fetchAlbum: (...args: unknown[]) => fetchAlbumMock(...args),
  fetchAlbumAssets: (...args: unknown[]) => fetchAlbumAssetsMock(...args),
  refreshAlbum: (...args: unknown[]) => refreshAlbumMock(...args),
  addAssets: vi.fn(async () => null),
  removeAsset: vi.fn(async () => null),
  fetchAlbums: vi.fn(async () => []),
  // `AssetViewer.vue` calls `fetchAlbumsForAsset` for the ALBUMS section
  // of the panel (open by default) — same mock as in
  // FavoritesView.spec.ts.
  fetchAlbumsForAsset: vi.fn(async () => [])
}))

vi.mock('@/api/events', () => ({
  startLiveEvents: vi.fn(() => ({ close: vi.fn() }))
}))

vi.mock('@/api/culling', () => ({
  fetchFlags: vi.fn(async () => ({ rating: null, pick: 'none', color_label: null, favorite: false })),
  setFlags: vi.fn(async () => null),
  deleteAsset: vi.fn(async () => null),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
}))

vi.mock('@/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const { apiFetch } = await import('@/api/client')

function stubLayout(width: number, height: number) {
  const widthDesc = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth')
  const heightDesc = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientHeight')
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { configurable: true, value: width })
  Object.defineProperty(HTMLElement.prototype, 'clientHeight', { configurable: true, value: height })
  return () => {
    if (widthDesc) Object.defineProperty(HTMLElement.prototype, 'clientWidth', widthDesc)
    if (heightDesc) Object.defineProperty(HTMLElement.prototype, 'clientHeight', heightDesc)
  }
}

function stubMatchMedia() {
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({
      matches: false,
      media: '',
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn()
    }))
  )
}

let unstubLayout: () => void

function album(overrides: Partial<Album> = {}): Album {
  return {
    id: 'album-1',
    name: 'Urbino',
    description: '',
    owner_id: 'u1',
    created_at: '',
    updated_at: '',
    is_shared: false,
    monochrome: false,
    ...overrides
  }
}

function albumAsset(id: string, takenAt: string | null = '2026-07-10T12:00:00Z'): AlbumAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: 'ab'.repeat(32),
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: takenAt,
    width: 100,
    height: 100,
    thumbhash: null,
    raw_kind: null,
    favorite: false,
    camera_model: null,
    tags: [],
    faces: [],
    position: 0,
    added_by: 'u1',
    added_at: ''
  }
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
  vi.mocked(apiFetch).mockResolvedValue([])
  fetchAlbumMock.mockResolvedValue(album())
  fetchAlbumAssetsMock.mockResolvedValue([])
  refreshAlbumMock.mockResolvedValue({ succeeded: [] })
})

afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllGlobals()
  unstubLayout?.()
})

async function mountDetail(id = 'album-1') {
  unstubLayout = stubLayout(1200, 900)
  stubMatchMedia()
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/albums', component: { template: '<div />' } },
      { path: '/albums/:id', component: AlbumDetailView }
    ]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser
  session.initialised = true
  session.ready = true

  await router.push(`/albums/${id}`)
  await router.isReady()
  const wrapper = mount(AlbumDetailView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, wrapper }
}

describe('AlbumDetailView — detail', () => {
  it('loads the album and its members from the route id, rendering a tile per member', async () => {
    fetchAlbumAssetsMock.mockResolvedValue([albumAsset('a'), albumAsset('b')])

    const { wrapper } = await mountDetail('album-1')

    expect(fetchAlbumMock).toHaveBeenCalledWith('album-1')
    expect(fetchAlbumAssetsMock).toHaveBeenCalledWith('album-1')
    expect(wrapper.findAllComponents(PhotoTile)).toHaveLength(2)
  })

  it('composes the subtitle from real member dates, plus shared/dynamic suffixes', async () => {
    fetchAlbumMock.mockResolvedValue(album({ is_shared: true, rule: { op: 'favorite' } }))
    fetchAlbumAssetsMock.mockResolvedValue([
      albumAsset('a', '2026-01-05T00:00:00Z'),
      albumAsset('b', '2026-07-20T00:00:00Z')
    ])

    const { wrapper } = await mountDetail()

    expect(wrapper.text()).toContain('2 foto · gennaio 2026 – luglio 2026')
    expect(wrapper.text()).toContain('condiviso')
    expect(wrapper.text()).toContain('si aggiorna da solo in base al filtro')
    expect(wrapper.text()).toContain('dinamico')
  })

  it('shows the manual-empty state when the album has no rule and no members', async () => {
    fetchAlbumMock.mockResolvedValue(album())
    fetchAlbumAssetsMock.mockResolvedValue([])

    const { wrapper } = await mountDetail()

    expect(wrapper.text()).toContain('Album vuoto')
  })

  it('shows the dynamic-no-match state when the album has a rule and no members', async () => {
    fetchAlbumMock.mockResolvedValue(album({ rule: { op: 'favorite' } }))
    fetchAlbumAssetsMock.mockResolvedValue([])

    const { wrapper } = await mountDetail()

    expect(wrapper.text()).toContain('Nessuna foto corrisponde al filtro')
  })

  it('shows the filtered-empty state when the album has members but the quick filter hides them all', async () => {
    fetchAlbumAssetsMock.mockResolvedValue([{ ...albumAsset('a'), raw_kind: 'jpeg' }])

    const { wrapper } = await mountDetail()
    wrapper.findComponent({ name: 'QuickFilter' }).vm.$emit('update:selection', { type: new Set(['raw']) })
    await flushPromises()

    expect(wrapper.text()).toContain('Nessuna foto corrisponde ai filtri')
  })

  it('shows "Aggiorna album" only for albums with a rule, and it reloads after refreshing', async () => {
    fetchAlbumMock.mockResolvedValueOnce(album({ rule: { op: 'favorite' } })).mockResolvedValue(
      album({ rule: { op: 'favorite' } })
    )
    fetchAlbumAssetsMock.mockResolvedValueOnce([]).mockResolvedValue([albumAsset('a')])

    const { wrapper } = await mountDetail()
    const refreshBtn = wrapper.findAll('button').find((b) => b.text() === 'Aggiorna album')
    expect(refreshBtn).toBeTruthy()

    await refreshBtn!.trigger('click')
    await flushPromises()

    expect(refreshAlbumMock).toHaveBeenCalledWith('album-1')
    expect(wrapper.findAllComponents(PhotoTile)).toHaveLength(1)
  })

  it('does not show "Aggiorna album" for a manual album', async () => {
    const { wrapper } = await mountDetail()

    expect(wrapper.findAll('button').find((b) => b.text() === 'Aggiorna album')).toBeUndefined()
  })

  it('"Tutti gli album" navigates back to the grid', async () => {
    const { wrapper, router } = await mountDetail()

    await wrapper.get('button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/albums')
  })

  it('shows a full-view ErrorState on load failure', async () => {
    fetchAlbumMock.mockRejectedValue(new Error('boom'))

    const { wrapper } = await mountDetail()

    expect(wrapper.findComponent(ErrorState).exists()).toBe(true)
  })
})
