import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import Button from '@/components/ui/Button.vue'
import ErrorState from '@/components/ui/ErrorState.vue'
import PhotoTile from '@/components/ui/PhotoTile.vue'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { ApiProblem } from '@/api/client'
import { fetchBuckets, fetchGeometry, fetchPage, type TimelineAsset } from '@/api/timeline'
import { startLiveEvents, type LiveMessage } from '@/api/events'
import { planStream } from '@/timeline/stream'
import { TimelineGeometry } from '@/timeline/geometry'

import TimelineView from './TimelineView.vue'

vi.mock('@/api/auth', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/auth')>()
  return { ...actual, logout: vi.fn() }
})

vi.mock('@/api/timeline', () => ({
  fetchBuckets: vi.fn(async () => []),
  fetchPage: vi.fn(async () => ({ assets: [] })),
  fetchGeometry: vi.fn(async () => ({ buffer: null, etag: null })),
  promoteViewport: vi.fn(async () => null)
}))

vi.mock('@/api/events', () => ({
  startLiveEvents: vi.fn(() => ({ close: vi.fn() }))
}))

vi.mock('@/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const { logout } = await import('@/api/auth')
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

/** Un record di geometria per ogni bucket passato, larghezza/altezza
 * fisse (100x100, aspect ratio 1) salvo diversa indicazione — comodo
 * quando il test non ha bisogno di un layout particolare, solo che
 * `plan.value` non sia vuoto. */
function geometryFor(buckets: { month: string; count: number }[], size = { w: 100, h: 100 }): ArrayBuffer {
  const records: { w: number; h: number; month: number }[] = []
  for (const bucket of buckets) {
    for (let i = 0; i < bucket.count; i++) {
      records.push({ w: size.w, h: size.h, month: monthIndex(bucket.month) })
    }
  }
  return encodeGeometry(records)
}

// jsdom non calcola un vero layout: `clientWidth`/`clientHeight` restano
// sempre 0. Senza uno stub, `plan.value` è sempre vuoto (planStream
// rifiuta una larghezza <=0) e `mountedRange` è sempre {0,0} (un
// viewport di altezza 0 non "vede" mai nulla) — quasi ogni test qui
// sotto ha bisogno di entrambi, non solo quelli che cliccano una tessera.
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

afterEach(() => {
  vi.resetAllMocks()
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
    favorite: false
  }
}

describe('TimelineView signOut', () => {
  it('azzera lo stato e naviga a /login anche se la revoca server-side fallisce', async () => {
    vi.mocked(logout).mockRejectedValue(new Error('network error'))
    const { router, session, wrapper } = await mountTimeline()

    await wrapper.getComponent(Button).trigger('click')
    await flushPromises()

    expect(session.user).toBeNull()
    expect(session.logoutError).toBe(true)
    expect(router.currentRoute.value.path).toBe('/login')
  })

  it('azzera lo stato e naviga a /login quando la revoca riesce', async () => {
    vi.mocked(logout).mockResolvedValue(null)
    const { router, session, wrapper } = await mountTimeline()

    await wrapper.getComponent(Button).trigger('click')
    await flushPromises()

    expect(session.user).toBeNull()
    expect(session.logoutError).toBe(false)
    expect(router.currentRoute.value.path).toBe('/login')
  })
})

describe('TimelineView buckets + geometry', () => {
  it('follows next_cursor until the month is complete, once its row enters the mounted range', async () => {
    const buckets = [{ month: '2024-07', count: 3 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
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
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer, etag: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountTimeline()

    const expected = planStream(new TimelineGeometry(buffer), buckets, 1200, 6).totalHeight
    const styledDiv = wrapper.findAll('div').find((d) => d.attributes('style')?.includes('height:'))
    expect(styledDiv?.attributes('style')).toContain(`${expected}px`)
  })

  it('groups by month only — no per-day sub-heading (documento funzionale §8: "nessun raggruppamento per giorno")', async () => {
    const buckets = [{ month: '2024-07', count: 2 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
    vi.mocked(fetchPage).mockResolvedValue({
      assets: [photo('a'), { ...photo('b'), taken_at_utc: '2024-07-02T12:00:00Z' }]
    })

    const { wrapper } = await mountTimeline()
    expect(wrapper.find('h3').exists()).toBe(false)
    expect(wrapper.findAllComponents(PhotoTile).length).toBe(2)
  })
})

describe('TimelineView bbox filter', () => {
  it('passes bbox query param to fetchBuckets, fetchGeometry and fetchPage', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('rome')] })

    await mountTimeline('/?bbox=10,40,13,43')

    expect(fetchBuckets).toHaveBeenCalledWith('10,40,13,43')
    expect(fetchGeometry).toHaveBeenCalledWith('10,40,13,43', undefined)
    expect(fetchPage).toHaveBeenCalledWith('2024-07', undefined, '10,40,13,43')
  })
})

describe('TimelineView lightbox in the URL', () => {
  it('clicking a tile pushes ?photo= and opens the viewer', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('a')] })

    const { wrapper, router } = await mountTimeline()
    await wrapper.findComponent(PhotoTile).find('button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.query.photo).toBe('a')
    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(true)
  })

  it('reloading on a ?photo= URL restores the viewer by loading the asset directly', async () => {
    // Il watcher immediato del composable scatta prima che `onMounted`
    // carichi i bucket: `loadedAssets` è sempre vuota in quel preciso
    // istante, quindi un ricaricamento passa sempre da `maps.loadAsset`
    // — non un difetto, la pagina non ha ancora nulla in memoria.
    vi.mocked(fetchBuckets).mockResolvedValue([])
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null })
    vi.mocked(apiFetch).mockResolvedValue(photo('a'))

    const { wrapper } = await mountTimeline('/?photo=a')

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/a')
    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(true)
  })

  it('closing the viewer removes ?photo= from the URL', async () => {
    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
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
  it('shows newly upserted photos without a page reload', async () => {
    let onEvent: ((msg: LiveMessage) => void) | undefined
    const close = vi.fn()
    vi.mocked(startLiveEvents).mockImplementation((cb) => {
      onEvent = cb
      return { close }
    })
    vi.mocked(fetchBuckets).mockResolvedValue([])
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null })

    const { wrapper } = await mountTimeline()
    expect(startLiveEvents).toHaveBeenCalledTimes(1)
    const emptyCopy = String(i18n.global.t('timeline.empty'))
    expect(wrapper.text()).toContain(emptyCopy)

    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [photo('live')] })
    onEvent?.({ v: 1, type: 'assets.upserted', payload: { ids: ['live'], count: 1 } })
    await flushPromises()

    expect(fetchBuckets).toHaveBeenCalledTimes(2)
    expect(fetchGeometry).toHaveBeenCalledTimes(2)
    expect(wrapper.findComponent(PhotoTile).exists()).toBe(true)
    expect(wrapper.text()).not.toContain(emptyCopy)

    wrapper.unmount()
    expect(close).toHaveBeenCalled()
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
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
    vi.mocked(fetchPage).mockImplementation(async (month: string) => ({
      assets: Array.from({ length: buckets.find((b) => b.month === month)!.count }, (_, i) => photo(`${month}-${i}`))
    }))

    const { wrapper } = await mountTimeline()
    await flushPromises()

    const mountedTiles = wrapper.findAllComponents(PhotoTile).length
    expect(mountedTiles).toBeGreaterThan(0)
    expect(mountedTiles).toBeLessThan(totalShots)
  })

  it('only first-screen tiles get priority="high" — the rest stay lazy (Task 5bis)', async () => {
    const buckets = Array.from({ length: 20 }, (_, i) => ({
      month: `${2024 - Math.floor(i / 12)}-${String(12 - (i % 12)).padStart(2, '0')}`,
      count: 50
    }))
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
    vi.mocked(fetchPage).mockImplementation(async (month: string) => ({
      assets: Array.from({ length: buckets.find((b) => b.month === month)!.count }, (_, i) => photo(`${month}-${i}`))
    }))

    const { wrapper } = await mountTimeline()
    await flushPromises()

    const tiles = wrapper.findAllComponents(PhotoTile)
    const highPriority = tiles.filter((t) => t.props('priority') === 'high')
    const autoPriority = tiles.filter((t) => t.props('priority') !== 'high')
    // Con un viewport di 900px stubbato, la prima schermata non contiene
    // l'intera libreria caricata: entrambi i gruppi devono esistere.
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
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
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
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null })

    const { wrapper } = await mountTimeline()

    const errorState = wrapper.findComponent(ErrorState)
    expect(errorState.exists()).toBe(true)
    expect(errorState.props('nature')).toBe('unreachable')
    expect(errorState.props('technicalDetail')).toBe('service-unavailable · 503')
    expect(wrapper.text()).not.toContain(String(i18n.global.t('timeline.empty')))
  })

  it('"Riprova" calls refreshTimeline again — a subsequent success clears the error and shows the grid', async () => {
    vi.mocked(fetchBuckets).mockRejectedValueOnce(new ApiProblem('service-unavailable', 'unavailable', 503))
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null })

    const { wrapper } = await mountTimeline()
    expect(wrapper.findComponent(ErrorState).exists()).toBe(true)

    const buckets = [{ month: '2024-07', count: 1 }]
    vi.mocked(fetchBuckets).mockResolvedValue(buckets)
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: geometryFor(buckets), etag: null })
    vi.mocked(fetchPage).mockResolvedValue({ assets: [] })

    await wrapper.findComponent(ErrorState).vm.$emit('retry')
    await flushPromises()

    expect(fetchBuckets).toHaveBeenCalledTimes(2)
    expect(wrapper.findComponent(ErrorState).exists()).toBe(false)
  })

  it('a non-retryable nature (file-missing) renders no "Riprova" button inside the real view', async () => {
    vi.mocked(fetchBuckets).mockRejectedValue(new ApiProblem('not-found', 'Resource not found', 404))
    vi.mocked(fetchGeometry).mockResolvedValue({ buffer: null, etag: null })

    const { wrapper } = await mountTimeline()

    const errorState = wrapper.findComponent(ErrorState)
    expect(errorState.props('nature')).toBe('file-missing')
    expect(errorState.find('button').exists()).toBe(false)
  })
})
