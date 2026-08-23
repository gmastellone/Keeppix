import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import { fetchBootstrap } from '@/api/bootstrap'
import { useSessionStore } from '@/stores/session'

import AppSidebar from './AppSidebar.vue'

vi.mock('@/api/bootstrap', () => ({
  fetchBootstrap: vi.fn(async () => ({
    user: { id: '1', username: 'admin', display_name: 'Admin', email: null, role: 'admin', locale: null },
    folders: [],
    storage: { l1: { free_bytes: 250_000_000_000, total_bytes: 1_000_000_000_000 } },
    badges: { culling: 4, revision: 0 }
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

async function mountSidebar(path = '/') {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/search', component: { template: '<div />' } },
      { path: '/culling', component: { template: '<div />' } },
      { path: '/map', component: { template: '<div />' } },
      { path: '/shares', component: { template: '<div />' } },
      { path: '/albums', component: { template: '<div />' } },
      { path: '/trash', component: { template: '<div />' } },
      { path: '/problems', component: { template: '<div />' } },
      { path: '/login', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser

  await router.push(path)
  await router.isReady()
  const wrapper = mount(AppSidebar, { global: { plugins: [router, i18n] }, attachTo: document.body })
  mounted = wrapper
  await flushPromises()
  return { router, session, wrapper }
}

describe('AppSidebar', () => {
  it('loads shell data on mount and shows the real culling badge count', async () => {
    const { wrapper } = await mountSidebar()
    expect(fetchBootstrap).toHaveBeenCalledTimes(1)
    const cullingLink = wrapper.findAll('a').find((a) => a.attributes('href') === '/culling')
    expect(cullingLink?.text()).toContain('4')
  })

  it('every nav item is a real anchor — keyboard-reachable, unlike the prototype', async () => {
    // Montata su /trash: il gruppo "Manutenzione" si apre da solo perché
    // contiene la vista corrente, quindi anche le sue sotto-voci sono nel DOM.
    const { wrapper } = await mountSidebar('/trash')
    const links = wrapper.findAll('a')
    expect(links.length).toBeGreaterThanOrEqual(8) // 5 NAV_TOP + Album + Cestino + Problemi
    for (const link of links) {
      expect(link.element.tagName).toBe('A')
    }
  })

  it('highlights the current route and no other', async () => {
    const { wrapper } = await mountSidebar('/map')
    const mapLink = wrapper.findAll('a').find((a) => a.attributes('href') === '/map')
    const searchLink = wrapper.findAll('a').find((a) => a.attributes('href') === '/search')
    expect(mapLink?.classes()).toContain('font-semibold')
    expect(searchLink?.classes()).not.toContain('font-semibold')
  })

  it('the Manutenzione group opens by itself when the current route is one of its children', async () => {
    const { wrapper } = await mountSidebar('/trash')
    // Il gruppo è aperto se la sua sotto-voce è visibile nel DOM.
    const trashLink = wrapper.findAll('a').find((a) => a.attributes('href') === '/trash')
    expect(trashLink?.exists()).toBe(true)
  })

  it('shows real storage totals from the bootstrap response, not a static placeholder', async () => {
    const { wrapper } = await mountSidebar()
    expect(wrapper.text()).toContain('250')
    expect(wrapper.text()).toContain('1')
  })

  it('the account menu trigger shows the real display name, and "Esci" logs out and redirects to /login', async () => {
    const { wrapper, router } = await mountSidebar()
    expect(wrapper.text()).toContain('Admin')

    // wrapper.find('button') prenderebbe l'interruttore di "Manutenzione"
    // (compare prima nel DOM): serve il bottone che contiene il nome utente.
    const accountTrigger = wrapper.findAll('button').find((b) => b.text().includes('Admin'))
    await accountTrigger?.trigger('click')
    await tick()
    // Il menu account (Popover) è teleportato su document.body, fuori
    // dall'albero DOM del wrapper — stesso schema già visto per Dialog.
    const signOutButton = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Esci'
    )
    expect(signOutButton).toBeDefined()

    signOutButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/login')
  })
})
