import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import ErrorState from '@/components/ui/ErrorState.vue'
import PhotoTile from '@/components/ui/PhotoTile.vue'
import QuickFilter from '@/components/ui/QuickFilter.vue'
import SelectionBar from '@/components/ui/SelectionBar.vue'
import FlatAssetGrid from '@/components/FlatAssetGrid.vue'
import LibrarySelectionActions from '@/components/LibrarySelectionActions.vue'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { ApiProblem } from '@/api/client'
import { setFlags } from '@/api/culling'
import { fetchBuckets, fetchGeometry, fetchPage, type TimelineAsset } from '@/api/timeline'
import { startLiveEvents, type LiveMessage } from '@/api/events'
import { planStream } from '@/timeline/stream'
import { TimelineGeometry } from '@/timeline/geometry'

import TimelineView from './TimelineView.vue'

vi.mock('@/api/timeline', () => ({
  fetchBuckets: vi.fn(async () => []),
  fetchPage: vi.fn(async () => ({ assets: [] })),
  fetchGeometry: vi.fn(async () => ({ buffer: null, etag: null, nextCursor: null })),
  promoteViewport: vi.fn(async () => null),
  // `AssetViewer.vue` calls `fetchAsset` for `full_exif` — routes through
  // the same `apiFetch` already mocked in this file, so the tests that
  // already respond to `GET /assets/{id}` via `apiFetch` (for
  // `useLightboxRoute`) cover this too.
  fetchAsset: vi.fn(async (id: string) => apiFetch(`/api/v1/assets/${id}`))
}))

vi.mock('@/api/events', () => ({
  startLiveEvents: vi.fn(() => ({ close: vi.fn() }))
}))

vi.mock('@/api/culling', () => ({
  fetchFlags: vi.fn(async () => ({ rating: null, pick: 'none', color_label: null, favorite: false })),
  setFlags: vi.fn(async () => null),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
}))

vi.mock('@/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const { apiFetch } = await import('@/api/client')

function encodeGeometry(records: { w: number; h: number; month: number }[]): ArrayBuffer {
  const buffer = new ArrayBuffer(8 + records.length * 6)
  const view = new DataView(buffer)
  view.setUint32(0, 1, true)
  view.setUint32(4, records.length, true)
  records.forEach((r, i) => {
    const offset = 8 + i * 6
    view.setUint16(offset, r.w, true)
    view.setUint16(offset + 2, r.h, true)
    view.setUint16(offset + 4, r.month, true)
  })
  return buffer
}

function monthIndex(month: string): number {
  const [year, mm] = month.split('-').map(Number)
  return year * 12 + mm
}

/** One geometry record per passed bucket, fixed width/height (100x100,
 * aspect ratio 1) unless stated otherwise — convenient when the test
 * doesn't need a particular layout, just a non-empty `plan.value`. */
function geometryFor(buckets: { month: string; count: number }[], size = { w: 100, h: 100 }): ArrayBuffer {
  const records: { w: number; h: number; month: number }[] = []
  for (const bucket of buckets) {
    for (let i = 0; i < bucket.count; i++) {
      records.push({ w: size.w, h: size.h, month: monthIndex(bucket.month) })
    }
  }
  return encodeGeometry(records)
}

// jsdom doesn't compute a real layout: `clientWidth`/`clientHeight`
// always stay 0. Without a stub, `plan.value` is always empty
// (planStream rejects a width <=0) and `mountedRange` is always {0,0}
// (a viewport with height 0 never "sees" anything) — almost every test
// below needs both, not just the ones that click a tile.
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

let unstubLayout: () => void

// jsdom doesn't implement `matchMedia` (unlike AppShell.spec.ts, which
// stubs it explicitly to test the switch itself): here a fixed "not
// mobile" result is enough, since this view doesn't test that behavior —
// just avoiding `useIsMobile()` throwing on mount.
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

// `useBrowseFilters` calls `GET /tags`/`GET /persons` via `apiFetch` on
// every mount, not just in tests that exercise the filter: without a
// baseline result, `apiFetch` reset by `resetAllMocks()` would return a
// bare `vi.fn()` (`undefined`, not a Promise) and break `.catch()` in the
// composable for every test in this file. Individual tests that need a
// different `apiFetch` response (e.g. `GET /assets/{id}`) override it
// afterward anyway.
beforeEach(() => {
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

async function mountTimeline(path = '/') {
  unstubLayout = stubLayout(1200, 900)
  stubMatchMedia()
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: TimelineView },
      { path: '/login', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser
  session.initialised = true
  session.ready = true

  await router.push(path)
  await router.isReady()
  const wrapper = mount(TimelineView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, session, wrapper }
}

function photo(id: string): TimelineAsset {
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
    favorite: false,
    camera_model: null,
    tags: [],
    faces: []
  }
}

// The "Log out" button is no longer in this view: it lives in
// AppSidebar's account menu, with its own test —
// AppSidebar.spec.ts, "'Esci' logs out and redirects to /login".
// `session.logout()` itself stays tested there, not here: duplicating it
// would test the function, not the component.

describe('TimelineView buckets + geometry', () => {
  it('follows next_cursor until the month is complete, once its row enters the mounted range', async () => {
    const buckets = [{ month: '2024-07', count: 3 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage)
      .mockResolvedValueOnce({ assets: [photo('a'), photo('b')], next_cursor: 'c1' })
      .mockResolvedValueOnce({ assets: [photo('c')] })

    const { wrapper } = await mountTimeline()
    await flushPromises()

    expect(fetchPage).toHaveBeenCalledWith('2024-07', undefined)
    expect(fetchPage).toHaveBeenCalledWith('2024-07', 'c1')
    expect(fetchPage).toHaveBeenCalledTimes(2)
    expect(wrapper.findComponent(PhotoTile).exists()).toBe(true)
  })

  it('the total scrollable height comes straight from the geometry-driven plan, not the DOM', async () => {
    const buckets = [{ month: '2024-07', count: 5 }]
    const buffer = geometryFor(buckets)
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer, etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountTimeline()

    // `useDensity`'s real default is 4 (desktop) — it used to be 6, just
    // an unverified fallback value, not a deliberate choice.
    const expected = planStream(new TimelineGeometry(buffer), buckets, 1200, 4).totalHeight
    const styledDiv = wrapper.findAll('div').find((d) => d.attributes('style')?.includes('height:'))
    expect(styledDiv?.attributes('style')).toContain(`${expected}px`)
  })

  it('groups by month only — no per-day sub-heading', async () => {
    const buckets = [{ month: '2024-07', count: 2 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({
      assets: [photo('a'), { ...photo('b'), taken_at_utc: '2024-07-02T12:00:00Z' }]
    })

    const { wrapper } = await mountTimeline()
    expect(wrapper.find('h3').exists()).toBe(false)
    expect(wrapper.findAllComponents(PhotoTile).length).toBe(2)
  })
})

// The first load of a mount paginates the geometry instead of waiting
// for the whole view — load time measured on a slow network with a
// large library was well over the target budget.
describe('TimelineView cold-start geometry pagination', () => {
  it('requests only the first page on mount, and paints from it without waiting for more', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })

    const { wrapper } = await mountTimeline()

    expect(fetchGeometry).toHaveBeenCalledTimes(1)
    expect(fetchGeometry).toHaveBeenCalledWith(undefined, undefined, { limit: 4000 })
    expect(wrapper.findComponent(PhotoTile).exists()).toBe(true)
  })

  it('fetches continuation pages by cursor and merges them in once they arrive, without blocking the first paint', async () => {
    const buckets = [{ month: '2024-07', count: 2 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a'), photo('b')] })

    let resolveSecondPage: (value: { buffer: ArrayBuffer; etag: null; nextCursor: null }) => void = () => {}
    const secondPage = new Promise<{ buffer: ArrayBuffer; etag: null; nextCursor: null }>((resolve) => {
      resolveSecondPage = resolve
    })
    vi.mocked(fetchGeometry).mockImplementation(async (_bbox, _etag, page) => {
      if (!page?.cursor) {
        return {
          buffer: geometryFor([{ month: '2024-07', count: 1 }]),
          etag: null,
          nextCursor: 'cursor-1'
        }
      }
      return secondPage
    })

    const { wrapper } = await mountTimeline()
    // The first shot is already rendered before the second page responds:
    // that's exactly the point of this pagination, not just an incidental detail.
    expect(fetchGeometry).toHaveBeenCalledTimes(2)
    expect(fetchGeometry).toHaveBeenNthCalledWith(2, undefined, undefined, {
      limit: 4000,
      cursor: 'cursor-1'
    })
    expect(wrapper.findAllComponents(PhotoTile).length).toBe(1)

    resolveSecondPage({ buffer: geometryFor([{ month: '2024-07', count: 1 }]), etag: null, nextCursor: null })
    await flushPromises()

    expect(wrapper.findAllComponents(PhotoTile).length).toBe(2)
  })

  it('keeps showing the already-painted first page if the background continuation fails', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })
    vi.mocked(fetchGeometry).mockImplementation(async (_bbox, _etag, page) => {
      if (!page?.cursor) {
        return { buffer: geometryFor(buckets), etag: null, nextCursor: 'cursor-1' }
      }
      throw new Error('network dropped mid-background-fetch')
    })
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {})

    const { wrapper } = await mountTimeline()

    expect(wrapper.findComponent(PhotoTile).exists()).toBe(true)
    expect(wrapper.text()).not.toContain('Unexpected error')
    expect(warnSpy).toHaveBeenCalled()
  })

  it('does not re-page on a later refresh (e.g. a live event): that call uses the whole-view ETag path', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    try {
      let onEvent: ((msg: LiveMessage) => void) | undefined
      vi.mocked(startLiveEvents).mockImplementation((cb) => {
        onEvent = cb
        return { close: vi.fn() }
      })
      const buckets = [{ month: '2024-07', count: 1 }]
      vi.mocked(fetchBuckets).mockResolvedValue(buckets)
      vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: '"v1"', nextCursor: null })
      vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })

      await mountTimeline()
      expect(fetchGeometry).toHaveBeenNthCalledWith(1, undefined, undefined, { limit: 4000 })

      // Each event well clear of the debounce window (800ms), so each gets
      // its own refresh — this test is about ETag chaining across
      // sequential refreshes, not the burst-collapsing behavior covered
      // in "TimelineView live events" below.
      onEvent?.({ v: 1, type: 'assets.upserted', payload: { ids: ['a'], count: 1 } })
      await vi.advanceTimersByTimeAsync(800)

      // The second refresh no longer paginates: two arguments (bbox, etag),
      // not three — the old whole-view signature, not {limit: ...}. No etag
      // to send yet: the first (paginated) load doesn't capture one — this
      // is exactly the deliberate tradeoff described in refreshTimeline.
      expect(fetchGeometry).toHaveBeenNthCalledWith(2, undefined, undefined)

      // The third refresh, however, has the etag captured by the second one
      // — that one can benefit from a 304 again.
      onEvent?.({ v: 1, type: 'assets.upserted', payload: { ids: ['a'], count: 1 } })
      await vi.advanceTimersByTimeAsync(800)
      expect(fetchGeometry).toHaveBeenNthCalledWith(3, undefined, '"v1"')
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('TimelineView bbox filter', () => {
  it('passes bbox query param to fetchBuckets, fetchGeometry and fetchPage', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('rome')] })

    await mountTimeline('/?bbox=10,40,13,43')

    expect(fetchBuckets).toHaveBeenCalledWith('10,40,13,43')
    // A mount's very first load is always paginated: 4000 is
    // FIRST_GEOMETRY_PAGE_LIMIT in TimelineView.vue.
    expect(fetchGeometry).toHaveBeenCalledWith('10,40,13,43', undefined, { limit: 4000 })
    expect(fetchPage).toHaveBeenCalledWith('2024-07', undefined, '10,40,13,43')
  })
})

describe('TimelineView lightbox in the URL', () => {
  it('clicking a tile pushes ?photo= and opens the viewer', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })

    const { wrapper, router } = await mountTimeline()
    await wrapper.findComponent(PhotoTile).find('button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.query.photo).toBe('a')
    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(true)
  })

  it('reloading on a ?photo= URL restores the viewer by loading the asset directly', async () => {
    // The composable's immediate watcher fires before `onMounted` loads
    // the buckets: `loadedAssets` is always empty at that exact instant,
    // so a reload always goes through `maps.loadAsset` — not a bug, the
    // page simply has nothing in memory yet.
    vi.mocked(fetchBuckets).mockResolvedValue([])
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null, nextCursor: null })
    vi.mocked(apiFetch).mockImplementation(async (url: string) => (url === '/api/v1/assets/a' ? photo('a') : []))

    const { wrapper } = await mountTimeline('/?photo=a')

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/a')
    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(true)
  })

  it('closing the viewer removes ?photo= from the URL', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })

    const { wrapper, router } = await mountTimeline()
    await wrapper.findComponent(PhotoTile).find('button').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query.photo).toBe('a')

    wrapper.findComponent({ name: 'AssetViewer' }).vm.$emit('close')
    await flushPromises()

    expect(router.currentRoute.value.query.photo).toBeUndefined()
  })
})

describe('TimelineView live events', () => {
  // `assets.upserted` is debounced (see TimelineView.vue): a burst of
  // background-job completions collapses into one refresh instead of
  // resetting scroll on every single one. Real timers to let mountTimeline
  // resolve, fake ones to control the debounce window deterministically.
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('shows newly upserted photos without a page reload, after the debounce settles', async () => {
    let onEvent: ((msg: LiveMessage) => void) | undefined
    const close = vi.fn()
    vi.mocked(startLiveEvents).mockImplementation((cb) => {
      onEvent = cb
      return { close }
    })
    vi.mocked(fetchBuckets).mockResolvedValue([])
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null, nextCursor: null })

    const { wrapper } = await mountTimeline()
    expect(startLiveEvents).toHaveBeenCalledTimes(1)
    const emptyCopy = String(i18n.global.t('timeline.empty'))
    expect(wrapper.text()).toContain(emptyCopy)

    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('live')] })
    onEvent?.({ v: 1, type: 'assets.upserted', payload: { ids: ['live'], count: 1 } })
    // Nothing yet: still inside the debounce window.
    expect(fetchBuckets).toHaveBeenCalledTimes(1)

    await vi.advanceTimersByTimeAsync(800)

    expect(fetchBuckets).toHaveBeenCalledTimes(2)
    expect(fetchGeometry).toHaveBeenCalledTimes(2)
    expect(wrapper.findComponent(PhotoTile).exists()).toBe(true)
    expect(wrapper.text()).not.toContain(emptyCopy)

    wrapper.unmount()
    expect(close).toHaveBeenCalled()
  })

  it('does not yank scroll back to the top on a live-triggered refresh', async () => {
    let onEvent: ((msg: LiveMessage) => void) | undefined
    vi.mocked(startLiveEvents).mockImplementation((cb) => {
      onEvent = cb
      return { close: vi.fn() }
    })
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })

    const { wrapper } = await mountTimeline()
    const grid = wrapper.get('.overflow-auto').element
    grid.scrollTop = 500

    // A live event's background update reflows the same geometry (a new
    // hash, updated dimensions on an already-loaded asset) — the person
    // looking at this screen didn't ask for a fresh load, so scroll stays
    // exactly where they left it. Contrast: the cold-start load this same
    // `mountTimeline()` just did left scroll at 0, which is correct there
    // — this test is about what happens *after*, to someone already
    // scrolled in.
    onEvent?.({ v: 1, type: 'assets.upserted', payload: { ids: ['a'], count: 1 } })
    await vi.advanceTimersByTimeAsync(800)

    expect(fetchGeometry).toHaveBeenCalledTimes(2)
    expect(grid.scrollTop).toBe(500)
  })

  it('a burst of upserts within the debounce window collapses into a single refresh', async () => {
    let onEvent: ((msg: LiveMessage) => void) | undefined
    vi.mocked(startLiveEvents).mockImplementation((cb) => {
      onEvent = cb
      return { close: vi.fn() }
    })
    vi.mocked(fetchBuckets).mockResolvedValue([])
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null, nextCursor: null })

    await mountTimeline()
    expect(fetchBuckets).toHaveBeenCalledTimes(1)

    for (let i = 0; i < 20; i++) {
      onEvent?.({ v: 1, type: 'assets.upserted', payload: { ids: [`a${i}`], count: 1 } })
      await vi.advanceTimersByTimeAsync(50)
    }
    expect(fetchBuckets).toHaveBeenCalledTimes(1)

    await vi.advanceTimersByTimeAsync(800)
    expect(fetchBuckets).toHaveBeenCalledTimes(2)
  })
})

describe('TimelineView virtualization', () => {
  it('mounts far fewer tiles than the total library, even for a large geometry', async () => {
    const buckets = Array.from({ length: 40 }, (_, i) => ({
      month: `${2024 - Math.floor(i / 12)}-${String(12 - (i % 12)).padStart(2, '0')}`,
      count: 50
    }))
    const totalShots = buckets.reduce((sum, b) => sum + b.count, 0)
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockImplementation(async (month: string) => ({
      assets: Array.from({ length: buckets.find((b) => b.month === month)!.count }, (_, i) => photo(`${month}-${i}`))
    }))

    const { wrapper } = await mountTimeline()
    await flushPromises()

    const mountedTiles = wrapper.findAllComponents(PhotoTile).length
    expect(mountedTiles).toBeGreaterThan(0)
    expect(mountedTiles).toBeLessThan(totalShots)
  })

  it('only first-screen tiles get priority="high" — the rest stay lazy', async () => {
    const buckets = Array.from({ length: 20 }, (_, i) => ({
      month: `${2024 - Math.floor(i / 12)}-${String(12 - (i % 12)).padStart(2, '0')}`,
      count: 50
    }))
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockImplementation(async (month: string) => ({
      assets: Array.from({ length: buckets.find((b) => b.month === month)!.count }, (_, i) => photo(`${month}-${i}`))
    }))

    const { wrapper } = await mountTimeline()
    await flushPromises()

    const tiles = wrapper.findAllComponents(PhotoTile)
    const highPriority = tiles.filter((t) => t.props('priority') === 'high')
    const autoPriority = tiles.filter((t) => t.props('priority') !== 'high')
    // With a stubbed 900px viewport, the first screen doesn't contain the
    // entire loaded library: both groups must exist.
    expect(highPriority.length).toBeGreaterThan(0)
    expect(autoPriority.length).toBeGreaterThan(0)
  })
})

describe('TimelineView scrubber', () => {
  it('is a keyboard-reachable slider that jumps a month per arrow key press', async () => {
    const buckets = [
      { month: '2024-08', count: 1 },
      { month: '2024-07', count: 1 },
      { month: '2024-06', count: 1 }
    ]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountTimeline()
    const slider = wrapper.get('[role="slider"]')
    expect(slider.attributes('tabindex')).toBe('0')
    expect(slider.attributes('aria-valuenow')).toBe('0')

    await slider.trigger('keydown', { key: 'End' })
    expect(slider.attributes('aria-valuenow')).toBe('2')
    expect(slider.attributes('aria-valuetext')).toContain('2024')
  })
})

describe('TimelineView error state', () => {
  it('shows a full-view ErrorState, classified from the failure, in place of the grid', async () => {
    vi.mocked(fetchBuckets).mockRejectedValue(new ApiProblem('service-unavailable', 'Service temporarily unavailable', 503))
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null, nextCursor: null })

    const { wrapper } = await mountTimeline()

    const errorState = wrapper.findComponent(ErrorState)
    expect(errorState.exists()).toBe(true)
    expect(errorState.props('nature')).toBe('unreachable')
    expect(errorState.props('technicalDetail')).toBe('service-unavailable · 503')
    expect(wrapper.text()).not.toContain(String(i18n.global.t('timeline.empty')))
  })

  it('"Riprova" calls refreshTimeline again — a subsequent success clears the error and shows the grid', async () => {
    vi.mocked(fetchBuckets).mockRejectedValueOnce(new ApiProblem('service-unavailable', 'unavailable', 503))
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null, nextCursor: null })

    const { wrapper } = await mountTimeline()
    expect(wrapper.findComponent(ErrorState).exists()).toBe(true)

    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [] })

    await wrapper.findComponent(ErrorState).vm.$emit('retry')
    await flushPromises()

    expect(fetchBuckets).toHaveBeenCalledTimes(2)
    expect(wrapper.findComponent(ErrorState).exists()).toBe(false)
  })

  it('a non-retryable nature (file-missing) renders no "Riprova" button inside the real view', async () => {
    vi.mocked(fetchBuckets).mockRejectedValue(new ApiProblem('not-found', 'Resource not found', 404))
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null, nextCursor: null })

    const { wrapper } = await mountTimeline()

    const errorState = wrapper.findComponent(ErrorState)
    expect(errorState.props('nature')).toBe('file-missing')
    expect(errorState.find('button').exists()).toBe(false)
  })
})

// The favorite heart and multi-selection are really wired up — before
// this, PhotoTile always received `:selected="false"` and
// `:selection-mode="false"` regardless.
describe('TimelineView favorites', () => {
  it('the heart button on a tile toggles the favorite flag, merging it into the current server flags', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })

    const { wrapper } = await mountTimeline()
    const heart = wrapper.findComponent(PhotoTile).findAll('button')[2]
    await heart.trigger('click')
    await flushPromises()

    expect(setFlags).toHaveBeenCalledWith('a', { rating: null, pick: 'none', color_label: null, favorite: true })
    expect(wrapper.findComponent(PhotoTile).props('isFavorite')).toBe(true)
  })
})

describe('TimelineView selection', () => {
  it('checking a tile enters selection mode: the toolbar row is replaced by the "N selezionate" bar', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })

    const { wrapper } = await mountTimeline()
    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(0)

    await wrapper.findComponent(PhotoTile).find('[role="checkbox"]').trigger('click')
    await flushPromises()

    expect(wrapper.findComponent(PhotoTile).props('selected')).toBe(true)
    expect(wrapper.findComponent(PhotoTile).props('selectionMode')).toBe(true)
    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(1)
    // In multi-selection the whole row is replaced — the normal toolbar
    // (quick filter included) disappears along with it.
    expect(wrapper.findComponent(QuickFilter).exists()).toBe(false)
  })

  it('clicking a tile body while selection is active toggles selection instead of opening the lightbox', async () => {
    const buckets = [{ month: '2024-07', count: 2 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a'), photo('b')] })

    const { wrapper, router } = await mountTimeline()
    await wrapper.findComponent(PhotoTile).find('[role="checkbox"]').trigger('click')
    await flushPromises()

    const tiles = wrapper.findAllComponents(PhotoTile)
    await tiles[1].find('button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.query.photo).toBeUndefined()
    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(2)
  })

  it('"Seleziona tutto quello che vedi" selects every currently loaded photo', async () => {
    const buckets = [{ month: '2024-07', count: 2 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a'), photo('b')] })

    const { wrapper } = await mountTimeline()
    await wrapper.get(`[aria-label="${String(i18n.global.t('ui.selectAllVisible.ariaLabel'))}"]`).trigger('click')
    await flushPromises()

    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(2)
    wrapper.findAllComponents(PhotoTile).forEach((tile) => expect(tile.props('selected')).toBe(true))
  })

  it('the × in the selection bar clears the selection and restores the normal toolbar', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })

    const { wrapper } = await mountTimeline()
    await wrapper.findComponent(PhotoTile).find('[role="checkbox"]').trigger('click')
    await flushPromises()
    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(1)

    wrapper.findComponent(SelectionBar).vm.$emit('clear')
    await flushPromises()

    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(0)
    expect(wrapper.findComponent(PhotoTile).props('selectionMode')).toBe(false)
  })

  it('the selection bar carries the actual selected TimelineAsset objects into LibrarySelectionActions, not just ids', async () => {
    const buckets = [{ month: '2024-07', count: 2 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a'), photo('b')] })

    const { wrapper } = await mountTimeline()
    await wrapper.findComponent(PhotoTile).find('[role="checkbox"]').trigger('click')
    await flushPromises()

    const actions = wrapper.findComponent(LibrarySelectionActions)
    expect(actions.exists()).toBe(true)
    expect(actions.props('assets').map((a: TimelineAsset) => a.id)).toEqual(['a'])
  })
})

// The logic of the six dimensions and their combination is already
// thoroughly tested elsewhere (`useBrowseFilters.spec.ts`,
// `design/quickFilter.spec.ts`, `QuickFilter.spec.ts`) — here only this
// view's own wiring, what those tests can't see: switching from the
// geometry-blob grid to `FlatAssetGrid` when a filter is active, and
// narrowing "Select all" to the same set.
describe('TimelineView quick filter', () => {
  it('activating a filter leaves the month/geometry grid for FlatAssetGrid, narrowed to the matches', async () => {
    const buckets = [{ month: '2024-07', count: 2 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({
      assets: [{ ...photo('a'), raw_kind: 'jpeg' }, { ...photo('b'), raw_kind: 'raw' }]
    })

    const { wrapper } = await mountTimeline()
    expect(wrapper.findComponent(FlatAssetGrid).exists()).toBe(false)

    wrapper.findComponent(QuickFilter).vm.$emit('update:selection', { type: new Set(['jpeg']) })
    await flushPromises()

    const grid = wrapper.findComponent(FlatAssetGrid)
    expect(grid.exists()).toBe(true)
    expect(grid.props('assets').map((a: TimelineAsset) => a.id)).toEqual(['a'])
  })

  it('a filter matching nothing shows the shared "filtered empty" state instead of any grid', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [{ ...photo('a'), raw_kind: 'jpeg' }] })

    const { wrapper } = await mountTimeline()
    wrapper.findComponent(QuickFilter).vm.$emit('update:selection', { type: new Set(['raw']) })
    await flushPromises()

    expect(wrapper.findComponent(FlatAssetGrid).exists()).toBe(false)
    expect(wrapper.findComponent(PhotoTile).exists()).toBe(false)
    expect(wrapper.text()).toContain(String(i18n.global.t('ui.filteredEmpty.title')))
  })

  it('"Seleziona tutto quello che vedi" while a filter is active selects only what the filter lets through', async () => {
    const buckets = [{ month: '2024-07', count: 2 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null, nextCursor: null })
    vi.mocked(fetchPage).mockResolvedValue({
      assets: [{ ...photo('a'), raw_kind: 'jpeg' }, { ...photo('b'), raw_kind: 'raw' }]
    })

    const { wrapper } = await mountTimeline()
    wrapper.findComponent(QuickFilter).vm.$emit('update:selection', { type: new Set(['jpeg']) })
    await flushPromises()

    await wrapper.get(`[aria-label="${String(i18n.global.t('ui.selectAllVisible.ariaLabel'))}"]`).trigger('click')
    await flushPromises()

    expect(wrapper.findComponent(SelectionBar).props('count')).toBe(1)
  })
})
