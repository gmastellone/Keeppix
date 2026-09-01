import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { ApiProblem } from '@/api/client'
import { runSearch } from '@/api/library'
import { startLiveEvents, type LiveMessage } from '@/api/events'
import type { TimelineAsset } from '@/api/timeline'
import PhotoTile from '@/components/ui/PhotoTile.vue'
import QuickFilter from '@/components/ui/QuickFilter.vue'
import SelectionBar from '@/components/ui/SelectionBar.vue'
import ErrorState from '@/components/ui/ErrorState.vue'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

import FavoritesView from './FavoritesView.vue'

vi.mock('@/api/library', () => ({
  runSearch: vi.fn(async () => ({ assets: [] }))
}))

vi.mock('@/api/events', () => ({
  startLiveEvents: vi.fn(() => ({ close: vi.fn() }))
}))

vi.mock('@/api/culling', () => ({
  fetchFlags: vi.fn(async () => ({ rating: null, pick: 'none', color_label: null, favorite: true })),
  setFlags: vi.fn(async () => null),
  deleteAsset: vi.fn(async () => null),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
}))

vi.mock('@/api/albums', () => ({
  fetchAlbums: vi.fn(async () => []),
  fetchAlbum: vi.fn(async () => ({ id: 'x', name: '', assets: [] })),
  addAssets: vi.fn(async () => null),
  removeAsset: vi.fn(async () => null),
  // `AssetViewer.vue` calls `fetchAlbumsForAsset` for the ALBUMS section
  // of the panel, open by default.
  fetchAlbumsForAsset: vi.fn(async () => [])
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

// `useBrowseFilters` calls `GET /tags`/`GET /persons` via `apiFetch` on
// every mount: without a baseline result, `apiFetch` (reset by
// `resetAllMocks()` on every test) would return a bare `vi.fn()` and
// break `.catch()` inside the composable — same fix as in
// `TimelineView.spec.ts`.
beforeEach(() => {
  i18n.global.locale.value = 'it'
  vi.mocked(apiFetch).mockResolvedValue([])
})

afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllGlobals()
  unstubLayout?.()
})

const testUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

async function mountFavorites(path = '/favorites') {
  unstubLayout = stubLayout(1200, 900)
  stubMatchMedia()
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/favorites', component: FavoritesView }]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser
  session.initialised = true
  session.ready = true

  await router.push(path)
  await router.isReady()
  const wrapper = mount(FavoritesView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, wrapper }
}

function photo(id: string, favorite = true): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: 'ab'.repeat(32),
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: '2024-07-10T12:00:00Z',
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

describe('FavoritesView loading (SearchNode::Favorite, no separate geometry endpoint)', () => {
  it('fetches via runSearch({op:"favorite"}), following next_cursor to exhaustion', async () => {
    vi.mocked(runSearch)
      .mockResolvedValueOnce({ assets: [photo('a'), photo('b')], next_cursor: 'c1' })
      .mockResolvedValueOnce({ assets: [photo('c')] })

    const { wrapper } = await mountFavorites()

    expect(runSearch).toHaveBeenNthCalledWith(1, { op: 'favorite' }, undefined)
    expect(runSearch).toHaveBeenNthCalledWith(2, { op: 'favorite' }, 'c1')
    expect(wrapper.findAllComponents(PhotoTile)).toHaveLength(3)
  })

  it('shows the exact documented subtitle with the total count', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a'), photo('b'), photo('c')] })
    const { wrapper } = await mountFavorites()

    expect(wrapper.text()).toContain('3 foto, da tutte le cartelle')
  })
})

describe('FavoritesView empty states (two distinct states)', () => {
  it('the "no favorites at all" empty state has no toolbar at all', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountFavorites()

    expect(wrapper.text()).toContain('Nessun preferito ancora')
    expect(wrapper.text()).toContain('Premi il cuore su una foto per ritrovarla qui.')
    expect(wrapper.findComponent(SelectionBar).exists()).toBe(false)
    expect(wrapper.find('button').exists()).toBe(false)
  })

  it('unfavoriting the only visible photo shows the filtered-empty state, not the "no favorites at all" one', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a')] })
    const { wrapper } = await mountFavorites()
    expect(wrapper.findComponent(PhotoTile).exists()).toBe(true)

    const heart = wrapper.findComponent(PhotoTile).findAll('button')[2]
    await heart.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Nessuna foto corrisponde ai filtri')
    expect(wrapper.text()).not.toContain('Nessun preferito ancora')
  })
})

describe('FavoritesView heart button ("toglie la foto dalla vista", not just the flag)', () => {
  it('clicking the heart on a tile removes it from the grid immediately, no confirmation, no toast', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a'), photo('b')] })
    const { wrapper } = await mountFavorites()
    expect(wrapper.findAllComponents(PhotoTile)).toHaveLength(2)

    const heart = wrapper.findAllComponents(PhotoTile)[0].findAll('button')[2]
    await heart.trigger('click')
    await flushPromises()

    expect(wrapper.findAllComponents(PhotoTile)).toHaveLength(1)
    expect(wrapper.findAllComponents(PhotoTile)[0].props('filename')).toBe('b.jpg')
  })
})

describe('FavoritesView selection (same shared pools as Timeline)', () => {
  it('"Seleziona tutto quello che vedi" selects every currently visible (favorite) photo', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a'), photo('b')] })
    const { wrapper } = await mountFavorites()

    await wrapper.get(`[aria-label="${String(i18n.global.t('ui.selectAllVisible.ariaLabel'))}"]`).trigger('click')
    await flushPromises()

    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(2)
  })

  it('checking a tile enters selection mode and the SelectionBar carries the real selected assets', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a')] })
    const { wrapper } = await mountFavorites()

    await wrapper.findComponent(PhotoTile).find('[role="checkbox"]').trigger('click')
    await flushPromises()

    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(1)
  })
})

describe('FavoritesView lightbox and errors', () => {
  it('clicking a tile opens the lightbox via ?photo=', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a')] })
    const { wrapper, router } = await mountFavorites()

    await wrapper.findComponent(PhotoTile).find('button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.query.photo).toBe('a')
    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(true)
  })

  it('shows a full-view ErrorState, classified from the failure, in place of the grid', async () => {
    vi.mocked(runSearch).mockRejectedValue(new ApiProblem('service-unavailable', 'unavailable', 503))
    const { wrapper } = await mountFavorites()

    const errorState = wrapper.findComponent(ErrorState)
    expect(errorState.exists()).toBe(true)
    expect(errorState.props('nature')).toBe('unreachable')
  })

  it('refreshes on a live "assets.upserted" event, after the debounce settles', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    try {
      let onEvent: ((msg: LiveMessage) => void) | undefined
      vi.mocked(startLiveEvents).mockImplementation((cb) => {
        onEvent = cb
        return { close: vi.fn() }
      })
      vi.mocked(runSearch).mockResolvedValue({ assets: [] })

      const { wrapper } = await mountFavorites()
      expect(wrapper.text()).toContain('Nessun preferito ancora')

      vi.mocked(runSearch).mockResolvedValue({ assets: [photo('live')] })
      onEvent?.({ v: 1, type: 'assets.upserted', payload: { ids: ['live'], count: 1 } })
      // Debounced (see FavoritesView.vue): nothing yet.
      expect(wrapper.findComponent(PhotoTile).exists()).toBe(false)

      await vi.advanceTimersByTimeAsync(800)
      await wrapper.vm.$nextTick()

      expect(wrapper.findComponent(PhotoTile).exists()).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })
})

// The six dimensions and their combination are already thoroughly tested
// elsewhere (`useBrowseFilters.spec.ts`, `design/quickFilter.spec.ts`,
// `QuickFilter.spec.ts`) — here only this view's own wiring: the subtotal
// stays the favorites count ("before filters"), the grid and "Select
// all" narrow down to what the filter lets through.
describe('FavoritesView quick filter', () => {
  it('narrows the grid to the matches, without touching the subtitle count ("prima dei filtri")', async () => {
    vi.mocked(runSearch).mockResolvedValue({
      assets: [{ ...photo('a'), raw_kind: 'jpeg' }, { ...photo('b'), raw_kind: 'raw' }]
    })

    const { wrapper } = await mountFavorites()
    expect(wrapper.findAllComponents(PhotoTile).length).toBe(2)

    wrapper.findComponent(QuickFilter).vm.$emit('update:selection', { type: new Set(['jpeg']) })
    await flushPromises()

    expect(wrapper.findAllComponents(PhotoTile).map((t) => t.props('filename'))).toEqual(['a.jpg'])
    expect(wrapper.text()).toContain(String(i18n.global.t('favorites.subtitle', { n: 2 })))
  })

  it('a filter matching no favorite shows the shared "filtered empty" state', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [{ ...photo('a'), raw_kind: 'jpeg' }] })

    const { wrapper } = await mountFavorites()
    wrapper.findComponent(QuickFilter).vm.$emit('update:selection', { type: new Set(['raw']) })
    await flushPromises()

    expect(wrapper.findComponent(PhotoTile).exists()).toBe(false)
    expect(wrapper.text()).toContain(String(i18n.global.t('ui.filteredEmpty.title')))
  })

  it('"Seleziona tutto" while a filter is active selects only what the filter lets through', async () => {
    vi.mocked(runSearch).mockResolvedValue({
      assets: [{ ...photo('a'), raw_kind: 'jpeg' }, { ...photo('b'), raw_kind: 'raw' }]
    })

    const { wrapper } = await mountFavorites()
    wrapper.findComponent(QuickFilter).vm.$emit('update:selection', { type: new Set(['jpeg']) })
    await flushPromises()

    await wrapper.get(`[aria-label="${String(i18n.global.t('ui.selectAllVisible.ariaLabel'))}"]`).trigger('click')
    await flushPromises()

    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(1)
  })
})
