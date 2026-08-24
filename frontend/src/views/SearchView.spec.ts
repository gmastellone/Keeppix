import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import type { TimelineAsset } from '@/api/timeline'
import { runSearch } from '@/api/library'
import { fetchSavedSearches, fetchSuggestions } from '@/api/search'

import SearchView from './SearchView.vue'

vi.mock('@/api/library', () => ({
  runSearch: vi.fn(async () => ({ assets: [] }))
}))

vi.mock('@/api/search', () => ({
  fetchSavedSearches: vi.fn(async () => []),
  fetchSuggestions: vi.fn(async () => ({ suggestions: [] }))
}))

vi.mock('@/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const { apiFetch } = await import('@/api/client')

afterEach(() => vi.resetAllMocks())

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

async function mountSearch(initialPath = '/search') {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/search', component: SearchView }]
  })
  setActivePinia(createPinia())
  vi.mocked(fetchSavedSearches).mockResolvedValue([])
  vi.mocked(fetchSuggestions).mockResolvedValue({ suggestions: [] })

  await router.push(initialPath)
  await router.isReady()
  const wrapper = mount(SearchView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, wrapper }
}

describe('SearchView lightbox in the URL', () => {
  it('clicking a result pushes ?photo= (alongside the existing q) and opens the viewer', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a')] })
    const { wrapper, router } = await mountSearch('/search?q=urbino')

    await wrapper.find('li button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.query.photo).toBe('a')
    expect(router.currentRoute.value.query.q).toBe('urbino')
    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(true)
  })

  it('reloading on a ?photo= URL restores the viewer by loading the asset directly', async () => {
    vi.mocked(apiFetch).mockResolvedValue(photo('a'))
    const { wrapper } = await mountSearch('/search?photo=a')

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/a')
    expect(wrapper.findComponent({ name: 'AssetViewer' }).exists()).toBe(true)
  })

  it('closing the viewer removes ?photo= but keeps q', async () => {
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a')] })
    const { wrapper, router } = await mountSearch('/search?q=urbino')
    await wrapper.find('li button').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query.photo).toBe('a')

    wrapper.findComponent({ name: 'AssetViewer' }).vm.$emit('close')
    await flushPromises()

    expect(router.currentRoute.value.query.photo).toBeUndefined()
    expect(router.currentRoute.value.query.q).toBe('urbino')
  })
})
