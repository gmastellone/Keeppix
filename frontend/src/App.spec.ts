import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { useUploadStore } from '@/stores/upload'

import App from './App.vue'

vi.mock('@/api/bootstrap', () => ({
  fetchBootstrap: vi.fn(async () => ({
    user: { id: '1', username: 'admin', display_name: 'Admin', email: null, role: 'admin', locale: null },
    folders: [],
    storage: {},
    badges: { culling: 0, revision: 0 }
  }))
}))

const testUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

let mounted: VueWrapper | undefined
let previousLocale: typeof i18n.global.locale.value

beforeEach(() => {
  previousLocale = i18n.global.locale.value
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllGlobals()
  mounted?.unmount()
  mounted = undefined
  i18n.global.locale.value = previousLocale
})

function installMatchMedia(isMobile = false) {
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({
      matches: isMobile,
      media: '(max-width: 767px)',
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn()
    }))
  )
}

async function mountApp(path = '/', isMobile = false) {
  installMatchMedia(isMobile)
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div data-testid="home">contenuto della vista</div>' } },
      { path: '/search', component: { template: '<div />' } },
      { path: '/culling', component: { template: '<div />' } },
      { path: '/map', component: { template: '<div />' } },
      { path: '/shares', component: { template: '<div />' } },
      { path: '/albums', component: { template: '<div />' } },
      { path: '/more', component: { template: '<div />' } },
      { path: '/trash', component: { template: '<div />' } },
      { path: '/problems', component: { template: '<div />' } },
      { path: '/login', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()

  await router.push(path)
  await router.isReady()
  const wrapper = mount(App, {
    global: {
      plugins: [router, i18n],
      // L'overlay di upload ha il proprio motore già testato altrove
      // (`UploadPanel.spec.ts`, `stores/upload.spec.ts`): qui non
      // serve, e caricarlo per davvero (import differito + store reale)
      // sposterebbe il test dall'oggetto di questo file — il cablaggio
      // dell'impalcatura — a tutt'altro.
      stubs: { UploadPanel: true }
    },
    attachTo: document.body
  })
  mounted = wrapper
  return { router, session, wrapper }
}

describe('App', () => {
  it('shows the real sidebar, topbar and routed view once a session exists', async () => {
    const { wrapper, session } = await mountApp()
    session.user = testUser
    session.unavailable = false
    await flushPromises()

    // Un solo <a href="/culling"> reale prova che AppSidebar è montata
    // per davvero, non un segnaposto.
    expect(wrapper.findAll('a').some((a) => a.attributes('href') === '/culling')).toBe(true)
    // Il campo di scorciatoia della ricerca prova che AppTopbar è montata.
    expect(wrapper.find('#topSearch').exists()).toBe(true)
    expect(wrapper.find('[data-testid="home"]').text()).toBe('contenuto della vista')
  })

  it('on a mobile viewport, shows the mobile header/tabbar instead of the sidebar/topbar', async () => {
    const { wrapper, session } = await mountApp('/', true)
    session.user = testUser
    session.unavailable = false
    await flushPromises()

    // La tab bar mobile ha un <a href="/more"> reale, che AppTopbar/
    // AppSidebar non hanno mai.
    expect(wrapper.findAll('a').some((a) => a.attributes('href') === '/more')).toBe(true)
    expect(wrapper.find('#topSearch').exists()).toBe(false)
    expect(wrapper.find('[data-testid="home"]').text()).toBe('contenuto della vista')
  })

  it.each([
    ['desktop', false],
    ['mobile', true]
  ])('shows the real upload queue strip on %s once something is queued (§6.1)', async (_label, isMobile) => {
    const { wrapper, session } = await mountApp('/', isMobile)
    session.user = testUser
    session.unavailable = false
    const upload = useUploadStore()
    upload.sessions.push({
      id: 'a',
      filename: 'a.jpg',
      targetFolderId: null,
      expectedSize: 10,
      receivedBytes: 0,
      status: 'queued'
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Scegli dove')
  })

  it('shows only the unavailable screen — no shell — when the instance is unreachable', async () => {
    const { wrapper, session } = await mountApp()
    session.user = null
    session.unavailable = true
    await flushPromises()

    expect(wrapper.text()).toContain('Il server non è raggiungibile')
    expect(wrapper.findAll('a').some((a) => a.attributes('href') === '/culling')).toBe(false)
    expect(wrapper.find('#topSearch').exists()).toBe(false)
  })
})
