import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'

import { i18n } from '@/i18n'
import { fetchBootstrap } from '@/api/bootstrap'
import { useSessionStore } from '@/stores/session'

import AppMobileHeader from './AppMobileHeader.vue'

vi.mock('@/api/bootstrap', () => ({
  fetchBootstrap: vi.fn(async () => ({
    user: { id: '1', username: 'admin', display_name: 'Admin', email: null, role: 'admin', locale: null },
    folders: [],
    storage: {},
    badges: { culling: 0, revision: 0 }
  }))
}))

vi.mock('@/api/auth', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/auth')>()
  return { ...actual, logout: vi.fn(async () => null) }
})

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let mounted: VueWrapper | undefined
let previousLocale: typeof i18n.global.locale.value

beforeEach(() => {
  previousLocale = i18n.global.locale.value
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  vi.resetAllMocks()
  mounted?.unmount()
  mounted = undefined
  i18n.global.locale.value = previousLocale
})

const testUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

async function mountHeader(path: string): Promise<{ router: Router; wrapper: VueWrapper }> {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/search', component: { template: '<div />' } },
      { path: '/albums', component: { template: '<div />' } },
      { path: '/more', component: { template: '<div />' } },
      { path: '/culling', component: { template: '<div />' } },
      { path: '/batch-edit', component: { template: '<div />' } },
      { path: '/trash', component: { template: '<div />' } },
      { path: '/login', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser

  await router.push(path)
  await router.isReady()
  const wrapper = mount(AppMobileHeader, { global: { plugins: [router, i18n] }, attachTo: document.body })
  mounted = wrapper
  await flushPromises()
  return { router, wrapper }
}

describe('AppMobileHeader', () => {
  it('shows the per-route title, shared with AppTopbar\'s breadcrumb map', async () => {
    const { wrapper } = await mountHeader('/')
    expect(wrapper.text()).toContain('Tutte le foto')
  })

  it('shows "Altro" on /more, even though it is not in the shared route-title map', async () => {
    const { wrapper } = await mountHeader('/more')
    expect(wrapper.text()).toContain('Altro')
  })

  it('hides the back arrow on the four tab-bar roots', async () => {
    for (const path of ['/', '/search', '/albums', '/more']) {
      const { wrapper } = await mountHeader(path)
      expect(wrapper.find('button[aria-label="Indietro"]').exists()).toBe(false)
      wrapper.unmount()
    }
  })

  it('the back arrow goes to / from /culling and /batch-edit', async () => {
    for (const path of ['/culling', '/batch-edit']) {
      const { wrapper, router } = await mountHeader(path)
      await wrapper.find('button[aria-label="Indietro"]').trigger('click')
      await flushPromises()
      expect(router.currentRoute.value.path).toBe('/')
      wrapper.unmount()
    }
  })

  it('the back arrow goes to /more from any other non-root view', async () => {
    const { wrapper, router } = await mountHeader('/trash')
    await wrapper.find('button[aria-label="Indietro"]').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/more')
  })

  it('the culling shortcut exists only on the Foto view (/)', async () => {
    const { wrapper } = await mountHeader('/trash')
    expect(wrapper.findAll('a').some((a) => a.attributes('href') === '/culling')).toBe(false)
  })

  it('the culling shortcut shows the real badge count, and hides the badge when it is zero', async () => {
    vi.mocked(fetchBootstrap).mockResolvedValue({
      user: testUser,
      folders: [],
      storage: {},
      badges: { culling: 0, revision: 0 }
    })
    const { wrapper } = await mountHeader('/')
    const cullingLink = wrapper.findAll('a').find((a) => a.attributes('href') === '/culling')
    expect(cullingLink?.exists()).toBe(true)
    expect(cullingLink?.find('span').exists()).toBe(false)
  })

  it('shows a real badge count when there are batches awaiting culling', async () => {
    vi.mocked(fetchBootstrap).mockResolvedValue({
      user: testUser,
      folders: [],
      storage: {},
      badges: { culling: 7, revision: 0 }
    })
    const { wrapper } = await mountHeader('/')
    const cullingLink = wrapper.findAll('a').find((a) => a.attributes('href') === '/culling')
    expect(cullingLink?.text()).toContain('7')
  })

  it('the account menu shows the real display name, and "Esci" logs out and redirects to /login', async () => {
    const { wrapper, router } = await mountHeader('/')

    const accountTrigger = wrapper.find('button[type="button"]:last-of-type')
    await accountTrigger.trigger('click')
    await tick()
    const signOutButton = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Esci'
    )
    expect(signOutButton).toBeDefined()

    signOutButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/login')
  })
})
