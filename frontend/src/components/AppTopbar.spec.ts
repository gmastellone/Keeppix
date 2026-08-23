import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter, RouterView, type Router } from 'vue-router'

import { i18n } from '@/i18n'
import { fetchSavedSearches, fetchSuggestions } from '@/api/search'
import { useUploadStore } from '@/stores/upload'

import SearchView from '@/views/SearchView.vue'
import AppTopbar from './AppTopbar.vue'

vi.mock('@/api/library', () => ({
  runSearch: vi.fn(async () => ({ assets: [] }))
}))

vi.mock('@/api/search', () => ({
  fetchSavedSearches: vi.fn(async () => []),
  fetchSuggestions: vi.fn(async () => ({ suggestions: [] }))
}))

let mounted: VueWrapper | undefined
let previousLocale: typeof i18n.global.locale.value

beforeEach(() => {
  previousLocale = i18n.global.locale.value
  i18n.global.locale.value = 'it'
  vi.mocked(fetchSavedSearches).mockResolvedValue([])
  vi.mocked(fetchSuggestions).mockResolvedValue({ suggestions: [] })
})

afterEach(() => {
  vi.resetAllMocks()
  mounted?.unmount()
  mounted = undefined
  i18n.global.locale.value = previousLocale
})

async function mountTopbar(path: string): Promise<{ router: Router; wrapper: VueWrapper }> {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/search', component: SearchView },
      { path: '/culling', component: { template: '<div />' } },
      { path: '/albums', component: { template: '<div />' } },
      { path: '/folders', component: { template: '<div />' } },
      // Rotta reale ma non collegata da AppSidebar (Task 6): deve
      // restare a briciola vuota, non un errore — a differenza di
      // /folders, che invece una voce reale in sidebar ce l'ha (Task 6
      // 4/N) e quindi una briciola reale (Task 6 6/N).
      { path: '/settings/maps/offline', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())

  await router.push(path)
  await router.isReady()
  // `openSearch` naviga E si aspetta di trovare il campo reale di
  // SearchView già montato: serve un <RouterView> vero, non solo
  // AppTopbar isolato, altrimenti la rotta cambia ma nessuna vista
  // corrispondente compare nel DOM da mettere a fuoco.
  const wrapper = mount(
    { components: { AppTopbar, RouterView }, template: '<AppTopbar /><RouterView />' },
    { global: { plugins: [router, i18n] }, attachTo: document.body }
  )
  mounted = wrapper
  await flushPromises()
  return { router, wrapper }
}

describe('AppTopbar', () => {
  it('shows the current-route breadcrumb in bold, per la mappa del documento funzionale', async () => {
    const { wrapper } = await mountTopbar('/')
    const bold = wrapper.find('b')
    expect(bold.exists()).toBe(true)
    expect(bold.text()).toBe('Tutte le foto')
  })

  it('shows a different label for a different route', async () => {
    const { wrapper } = await mountTopbar('/culling')
    expect(wrapper.find('b').text()).toBe('Culling')
  })

  it('shows the real "Cartelle" breadcrumb for /folders — not blank, unlike Task 6 (2/N)\'s first pass', async () => {
    const { wrapper } = await mountTopbar('/folders')
    expect(wrapper.find('b').text()).toBe('Cartelle')
  })

  it('leaves the breadcrumb empty for a real route not linked from AppSidebar', async () => {
    const { wrapper } = await mountTopbar('/settings/maps/offline')
    expect(wrapper.find('b').exists()).toBe(false)
  })

  it('the search shortcut is a readonly text field — never accepts typed text', async () => {
    const { wrapper } = await mountTopbar('/')
    const input = wrapper.find('input').element as HTMLInputElement
    expect(input.readOnly).toBe(true)
    expect(input.placeholder).toBe('Cerca per data, luogo, persona…')
  })

  it('clicking the search shortcut opens /search and focuses the real query field there', async () => {
    const { wrapper, router } = await mountTopbar('/')
    await wrapper.find('input').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/search')
    expect(document.activeElement?.id).toBe('search-query-input')
  })

  it('Invio activates the search shortcut too — corretto rispetto al prototipo (SP-8, §4.5)', async () => {
    const { wrapper, router } = await mountTopbar('/')
    await wrapper.find('input').trigger('keydown', { key: 'Enter' })
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/search')
  })

  it('Spazio activates the search shortcut too', async () => {
    const { wrapper, router } = await mountTopbar('/')
    await wrapper.find('input').trigger('keydown', { key: ' ' })
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/search')
  })

  it('the "Carica" button opens the hidden file picker, and a real pick reaches the upload store', async () => {
    const { wrapper } = await mountTopbar('/')
    const upload = useUploadStore()
    const spy = vi.spyOn(upload, 'addFilesFromPicker').mockImplementation(async () => {})

    const fileInput = wrapper.find('input[type="file"]').element as HTMLInputElement
    const clickSpy = vi.spyOn(fileInput, 'click')
    const uploadButton = wrapper.findAll('button').find((b) => b.text() === 'Carica')
    expect(uploadButton?.exists()).toBe(true)
    await uploadButton?.trigger('click')
    expect(clickSpy).toHaveBeenCalledTimes(1)

    const picked = [new File([new Blob(['x'])], 'a.jpg')]
    Object.defineProperty(fileInput, 'files', { value: picked, configurable: true })
    await wrapper.find('input[type="file"]').trigger('change')
    await flushPromises()

    expect(spy).toHaveBeenCalledWith(picked)
  })
})
