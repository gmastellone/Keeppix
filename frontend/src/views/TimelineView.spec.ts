import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import Button from '@/components/ui/Button.vue'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { fetchBuckets, fetchPage, type TimelineAsset } from '@/api/timeline'

import TimelineView from './TimelineView.vue'

vi.mock('@/api/auth', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/auth')>()
  return { ...actual, logout: vi.fn() }
})

vi.mock('@/api/timeline', () => ({
  fetchBuckets: vi.fn(async () => []),
  fetchPage: vi.fn(async () => ({ assets: [] })),
  promoteViewport: vi.fn(async () => null)
}))

const { logout } = await import('@/api/auth')

afterEach(() => vi.resetAllMocks())

const testUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

async function mountTimeline() {
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

  await router.push('/')
  await router.isReady()
  const wrapper = mount(TimelineView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, session, wrapper }
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
    thumbhash: null
  }
}

describe('TimelineView buckets', () => {
  it('follows next_cursor until the month is complete', async () => {
    vi.mocked(fetchBuckets).mockResolvedValue([{ month: '2024-07', count: 3 }])
    vi.mocked(fetchPage)
      .mockResolvedValueOnce({ assets: [photo('a'), photo('b')], next_cursor: 'c1' })
      .mockResolvedValueOnce({ assets: [photo('c')] })

    const { wrapper } = await mountTimeline()
    await flushPromises()

    expect(fetchPage).toHaveBeenCalledWith('2024-07', undefined)
    expect(fetchPage).toHaveBeenCalledWith('2024-07', 'c1')
    expect(fetchPage).toHaveBeenCalledTimes(2)
    expect(wrapper.text()).toContain('2024-07-10')
  })

  it('reserves section height from the bucket count before photos load', async () => {
    vi.mocked(fetchBuckets).mockResolvedValue([{ month: '2024-07', count: 12 }])
    vi.mocked(fetchPage).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountTimeline()
    expect(wrapper.get('section').attributes('style')).toMatch(/min-height:\s*[1-9]/)
  })
})
