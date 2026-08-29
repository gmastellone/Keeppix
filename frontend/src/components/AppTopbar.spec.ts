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
  fetchSuggestions: vi.fn(async () => ({ suggestions: [] })),
  createSavedSearch: vi.fn(async () => ({ id: 's1', name: '', query_text: '' }))
}))

vi.mock('@/api/tags', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/tags')>()
  return { ...actual, fetchTags: vi.fn(async () => []) }
})

vi.mock('@/api/folders', () => ({
  fetchAllFolders: vi.fn(async () => []),
  fetchTree: vi.fn(async () => [])
}))

let mounted: VueWrapper | undefined
let previousLocale: typeof i18n.global.locale.value

// Search's results area uses `FlatAssetGrid`, which mounts `useIsMobile`
// (`window.matchMedia`) and reads `clientWidth`/`clientHeight` for the
// justified layout — neither exists in plain jsdom. Same fix as
// `FavoritesView.spec.ts`/`SearchView.spec.ts` itself, needed here because
// this file actually mounts `SearchView` (clicking the search shortcut).
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

let unstubLayout: (() => void) | undefined

beforeEach(() => {
  previousLocale = i18n.global.locale.value
  i18n.global.locale.value = 'it'
  unstubLayout = stubLayout(1200, 900)
  stubMatchMedia()
  vi.mocked(fetchSavedSearches).mockResolvedValue([])
  vi.mocked(fetchSuggestions).mockResolvedValue({ suggestions: [] })
})

afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllGlobals()
  unstubLayout?.()
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
      // Real route but not linked from AppSidebar: it must stay at an
      // empty breadcrumb, not an error — unlike /folders, which does have
      // a real sidebar item and therefore a real breadcrumb.
      { path: '/settings/maps/offline', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())

  await router.push(path)
  await router.isReady()
  // `openSearch` navigates AND expects to find SearchView's real field
  // already mounted: it needs a real <RouterView>, not just an isolated
  // AppTopbar, otherwise the route changes but no matching view appears
  // in the DOM to focus.
  const wrapper = mount(
    { components: { AppTopbar, RouterView }, template: '<AppTopbar /><RouterView />' },
    { global: { plugins: [router, i18n] }, attachTo: document.body }
  )
  mounted = wrapper
  await flushPromises()
  return { router, wrapper }
}

describe('AppTopbar', () => {
  it('shows the current-route breadcrumb in bold, per the route-title map', async () => {
    const { wrapper } = await mountTopbar('/')
    const bold = wrapper.find('b')
    expect(bold.exists()).toBe(true)
    expect(bold.text()).toBe('Tutte le foto')
  })

  it('shows a different label for a different route', async () => {
    const { wrapper } = await mountTopbar('/culling')
    expect(wrapper.find('b').text()).toBe('Culling')
  })

  it('shows the real "Cartelle" breadcrumb for /folders — not blank, unlike the first pass', async () => {
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

  it('Invio activates the search shortcut too — fixed relative to the prototype', async () => {
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
