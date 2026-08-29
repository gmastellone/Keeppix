import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import type { Album, AlbumAsset } from '@/api/albums'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

import AlbumsView from './AlbumsView.vue'

const fetchAlbumsMock = vi.fn()
const fetchAlbumAssetsMock = vi.fn()

vi.mock('@/api/albums', () => ({
  fetchAlbums: (...args: unknown[]) => fetchAlbumsMock(...args),
  fetchAlbumAssets: (...args: unknown[]) => fetchAlbumAssetsMock(...args)
}))

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

function albumAsset(takenAt: string | null): AlbumAsset {
  return {
    id: `a-${takenAt}`,
    folder_id: 'f',
    filename: 'x.jpg',
    content_hash: null,
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

let wrapper: VueWrapper | undefined

beforeEach(() => {
  i18n.global.locale.value = 'it'
  fetchAlbumsMock.mockResolvedValue([])
  fetchAlbumAssetsMock.mockResolvedValue([])
})

afterEach(() => {
  // `attachTo: document.body` (needed to reach the teleported
  // `DialogPortal` — same reason as in `AlbumPickerDialog.spec.ts`) leaves
  // the previous test's DOM in the body if not unmounted: a
  // `document.body.querySelector` in the next test could otherwise hit a
  // dialog left open by an already-finished test.
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

async function mountAlbums() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/albums', component: AlbumsView },
      { path: '/albums/new', component: { template: '<div />' } },
      { path: '/albums/:id', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser
  session.initialised = true
  session.ready = true

  await router.push('/albums')
  await router.isReady()
  wrapper = mount(AlbumsView, { global: { plugins: [router, i18n] }, attachTo: document.body })
  await flushPromises()
  return { router, wrapper }
}

describe('AlbumsView — the grid', () => {
  it('shows the empty state when there are no albums yet (deviation: mockup has no such state, ALBUMS is pre-populated there)', async () => {
    const { wrapper } = await mountAlbums()

    expect(wrapper.text()).toContain('Nessun album ancora')
  })

  it('renders a card per album with name and "<N> foto · <intervallo>" from real member dates', async () => {
    fetchAlbumsMock.mockResolvedValue([album()])
    fetchAlbumAssetsMock.mockResolvedValue([albumAsset('2026-03-15T10:00:00Z'), albumAsset('2026-07-01T10:00:00Z')])

    const { wrapper } = await mountAlbums()

    expect(wrapper.text()).toContain('Urbino')
    expect(wrapper.text()).toContain('2 foto · marzo 2026 – luglio 2026')
  })

  it('shows "condiviso"/"dinamico" badges only when the album is shared/has a rule', async () => {
    fetchAlbumsMock.mockResolvedValue([
      album({ id: 'a1', name: 'Plain' }),
      album({ id: 'a2', name: 'Shared', is_shared: true }),
      album({ id: 'a3', name: 'Dynamic', rule: { op: 'favorite' } })
    ])

    const { wrapper } = await mountAlbums()

    expect(wrapper.text()).toContain('condiviso')
    expect(wrapper.text()).toContain('dinamico')
  })

  it('a zero-member album shows "nessuna foto ancora" (manual) or "nessuna foto corrisponde" (has a rule)', async () => {
    fetchAlbumsMock.mockResolvedValue([album({ id: 'a1' }), album({ id: 'a2', rule: { op: 'favorite' } })])
    fetchAlbumAssetsMock.mockResolvedValue([])

    const { wrapper } = await mountAlbums()

    expect(wrapper.text()).toContain('nessuna foto ancora')
    expect(wrapper.text()).toContain('nessuna foto corrisponde')
  })

  it('clicking a card navigates to the album detail route', async () => {
    fetchAlbumsMock.mockResolvedValue([album({ id: 'album-42' })])

    const { wrapper, router } = await mountAlbums()
    await wrapper.get('[role="button"]').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/albums/album-42')
  })

  it('"Crea album" navigates to the full creation page — not a dialog', async () => {
    const { wrapper, router } = await mountAlbums()

    await wrapper.get('button[type="button"]').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/albums/new')
  })
})
