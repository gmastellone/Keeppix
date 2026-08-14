import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import Button from '@/components/ui/Button.vue'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

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
