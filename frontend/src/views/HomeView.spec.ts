import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

import HomeView from './HomeView.vue'

vi.mock('@/api/auth', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/auth')>()
  return { ...actual, logout: vi.fn() }
})

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

/** Router isolato, senza la guardia di produzione: qui interessa solo dove
 * atterra la navigazione dopo il click, non il flusso di bootstrap. */
async function mountHomeView() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: HomeView },
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
  const wrapper = mount(HomeView, { global: { plugins: [router, i18n] } })
  return { router, session, wrapper }
}

describe('HomeView signOut', () => {
  it('azzera lo stato e naviga a /login anche se la revoca server-side fallisce', async () => {
    vi.mocked(logout).mockRejectedValue(new Error('network error'))
    const { router, session, wrapper } = await mountHomeView()

    await wrapper.find('button').trigger('click')
    await flushPromises()

    expect(session.user).toBeNull()
    expect(session.logoutError).toBe(true)
    expect(router.currentRoute.value.path).toBe('/login')
  })

  it('azzera lo stato e naviga a /login quando la revoca riesce', async () => {
    vi.mocked(logout).mockResolvedValue(null)
    const { router, session, wrapper } = await mountHomeView()

    await wrapper.find('button').trigger('click')
    await flushPromises()

    expect(session.user).toBeNull()
    expect(session.logoutError).toBe(false)
    expect(router.currentRoute.value.path).toBe('/login')
  })
})
