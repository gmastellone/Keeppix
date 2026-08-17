import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'

vi.mock('@/api/folders', () => ({
  fetchTree: vi.fn(),
  fetchChildren: vi.fn(),
  moveFolder: vi.fn()
}))

import FoldersView from './FoldersView.vue'

const { fetchTree, fetchChildren, moveFolder } = await import('@/api/folders')

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

async function mountFolders() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/folders', component: FoldersView }
    ]
  })
  await router.push('/folders')
  await router.isReady()
  const wrapper = mount(FoldersView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  vi.mocked(fetchTree).mockResolvedValue([root])
  vi.mocked(fetchChildren).mockResolvedValue({ folders: [child], assets: [] })
  vi.mocked(moveFolder).mockResolvedValue(null)
})

afterEach(() => {
  vi.resetAllMocks()
})

describe('FoldersView', () => {
  it('carica solo la radice, e i figli solo quando si espande', async () => {
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

  it('spostare una cartella chiama moveFolder verso un fratello visibile', async () => {
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
