import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'

vi.mock('@/api/folders', () => ({
  fetchTree: vi.fn(),
  fetchChildren: vi.fn(),
  moveFolder: vi.fn()
}))

vi.mock('@/api/libraries', () => ({
  fetchLibraries: vi.fn()
}))

import FoldersView from './FoldersView.vue'

const { fetchTree, fetchChildren, moveFolder } = await import('@/api/folders')
const { fetchLibraries } = await import('@/api/libraries')

const root = {
  id: 'root',
  library_id: 'lib',
  parent_id: null,
  name: 'Foto',
  depth: 0
}

const child = {
  id: 'y2024',
  library_id: 'lib',
  parent_id: 'root',
  name: '2024',
  depth: 1
}

let mounted: Awaited<ReturnType<typeof mount>> | undefined

async function mountFolders() {
  const pinia = createPinia()
  setActivePinia(pinia)
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/folders', component: FoldersView }
    ]
  })
  await router.push('/folders')
  await router.isReady()
  const wrapper = mount(FoldersView, {
    global: { plugins: [router, i18n, pinia] },
    attachTo: document.body
  })
  mounted = wrapper
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  vi.mocked(fetchTree).mockResolvedValue([root])
  vi.mocked(fetchChildren).mockResolvedValue({ folders: [child], assets: [] })
  vi.mocked(moveFolder).mockResolvedValue(null)
  vi.mocked(fetchLibraries).mockResolvedValue([])
})

afterEach(() => {
  mounted?.unmount()
  mounted = undefined
  vi.resetAllMocks()
})

describe('FoldersView', () => {
  it('loads only the root, and children only when expanded', async () => {
    const wrapper = await mountFolders()
    expect(fetchTree).toHaveBeenCalledTimes(1)
    expect(fetchChildren).not.toHaveBeenCalled()
    expect(wrapper.text()).toContain('Foto')
    expect(wrapper.text()).not.toContain('2024')

    await wrapper.get('[data-testid="expand-root"]').trigger('click')
    await flushPromises()

    expect(fetchChildren).toHaveBeenCalledTimes(1)
    expect(fetchChildren).toHaveBeenCalledWith('root')
    expect(wrapper.text()).toContain('2024')
  })

  it("labels the true filesystem root with the library's name instead of leaving it blank", async () => {
    const unnamedRoot = { ...root, name: '' }
    vi.mocked(fetchTree).mockResolvedValue([unnamedRoot])
    vi.mocked(fetchLibraries).mockResolvedValue([
      { id: 'lib', name: 'Vacanze 2024' }
    ] as Awaited<ReturnType<typeof fetchLibraries>>)

    const wrapper = await mountFolders()

    expect(wrapper.text()).toContain('Vacanze 2024')
  })

  it('falls back to a generic label if no library matches the empty root', async () => {
    const unnamedRoot = { ...root, name: '' }
    vi.mocked(fetchTree).mockResolvedValue([unnamedRoot])
    vi.mocked(fetchLibraries).mockResolvedValue([])

    const wrapper = await mountFolders()

    expect(wrapper.text()).toContain(i18n.global.t('folders.libraryRoot'))
  })

  it('offers "set location" for an expanded folder with photos, not an empty one', async () => {
    const photo = {
      id: 'a',
      folder_id: 'y2024',
      filename: 'a.jpg',
      content_hash: null,
      size_bytes: 1,
      kind: 'image' as const,
      status: 'indexed' as const,
      taken_at_utc: null,
      width: 100,
      height: 100,
      thumbhash: null,
      raw_kind: null,
      favorite: false,
      camera_model: null,
      tags: [],
      faces: []
    }
    vi.mocked(fetchChildren).mockResolvedValue({ folders: [], assets: [photo] })
    const wrapper = await mountFolders()

    // Root itself has no assets in the default mock — no button yet.
    expect(wrapper.find('[data-testid="location-root"]').exists()).toBe(false)

    await wrapper.get('[data-testid="expand-root"]').trigger('click')
    await flushPromises()

    const locationBtn = wrapper.get('[data-testid="location-root"]')
    await locationBtn.trigger('click')
    await flushPromises()

    expect(document.body.querySelector('input[type="search"]')).toBeTruthy()
  })

  it('moving a folder calls moveFolder onto a visible sibling', async () => {
    const archive = {
      id: 'archive',
      library_id: 'lib',
      parent_id: 'root',
      name: 'Archivio',
      depth: 1
    }
    vi.mocked(fetchChildren).mockResolvedValue({ folders: [child, archive], assets: [] })
    const wrapper = await mountFolders()
    await wrapper.get('[data-testid="expand-root"]').trigger('click')
    await flushPromises()

    await wrapper.get('[data-testid="move-y2024"]').trigger('click')
    await flushPromises()

    expect(moveFolder).toHaveBeenCalledWith('y2024', 'archive')
  })
})
