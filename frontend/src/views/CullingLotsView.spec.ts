import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import type { CullingLot } from '@/api/culling'
import type { FolderView } from '@/api/folders'
import type { Library } from '@/api/libraries'
import { i18n } from '@/i18n'

import CullingLotsView from './CullingLotsView.vue'

const fetchLibrariesMock = vi.fn()
const fetchAllFoldersMock = vi.fn()
const fetchCullingLotsMock = vi.fn()

vi.mock('@/api/libraries', () => ({
  fetchLibraries: (...args: unknown[]) => fetchLibrariesMock(...args)
}))

vi.mock('@/api/folders', () => ({
  fetchAllFolders: (...args: unknown[]) => fetchAllFoldersMock(...args)
}))

vi.mock('@/api/culling', () => ({
  fetchCullingLots: (...args: unknown[]) => fetchCullingLotsMock(...args)
}))

function library(overrides: Partial<Library> = {}): Library {
  return {
    id: 'lib-1',
    name: 'Lago di Braies',
    owner_id: 'u1',
    root_path: '/data/lago-di-braies',
    scan_enabled: true,
    faces_enabled: true,
    exclude_patterns: [],
    status: 'active',
    last_scan_at: null,
    created_at: '',
    culling_root_folder_id: 'culling-1',
    ...overrides
  }
}

function folder(overrides: Partial<FolderView> = {}): FolderView {
  return {
    id: 'root-1',
    library_id: 'lib-1',
    parent_id: null,
    name: 'Lago di Braies',
    depth: 0,
    ...overrides
  }
}

function lot(overrides: Partial<CullingLot> = {}): CullingLot {
  return {
    folder_id: 'lot-1',
    name: 'Dolomiti',
    created_at: '2026-08-14T10:00:00Z',
    pending: 184,
    taken: 0,
    skipped: 0,
    ...overrides
  }
}

let wrapper: VueWrapper | undefined

beforeEach(() => {
  i18n.global.locale.value = 'it'
  fetchLibrariesMock.mockResolvedValue([library()])
  fetchAllFoldersMock.mockResolvedValue([
    folder({ id: 'root-1', parent_id: null, name: 'Lago di Braies' }),
    folder({ id: 'culling-1', parent_id: 'root-1', name: 'Culling', depth: 1 })
  ])
  fetchCullingLotsMock.mockResolvedValue([lot()])
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

async function mountView() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/culling', component: CullingLotsView },
      { path: '/culling/:lotId', component: { template: '<div />' } },
      { path: '/settings', component: { template: '<div />' } }
    ]
  })
  await router.push('/culling')
  await router.isReady()
  wrapper = mount(CullingLotsView, { global: { plugins: [i18n, router] } })
  await flushPromises()
  return { wrapper, router }
}

describe('CullingLotsView — §14 griglia dei lotti', () => {
  it('shows the culling root line and the real lots grid', async () => {
    const { wrapper } = await mountView()

    expect(wrapper.text()).toContain('Cartella di culling: Lago di Braies / Culling')
    expect(wrapper.text()).toContain('Dolomiti')
    expect(wrapper.text()).toContain('184 foto')
    expect(wrapper.text()).toContain('184 da vedere')
  })

  it('shows a hint linking to settings when no library has a culling root', async () => {
    fetchLibrariesMock.mockResolvedValue([library({ culling_root_folder_id: null })])
    const { wrapper } = await mountView()

    expect(wrapper.text()).toContain('Nessuna libreria ha ancora una cartella di culling designata.')
    expect(wrapper.text()).not.toContain('Dolomiti')
  })

  it('clicking a lot card navigates to the open-lot route with the lot name', async () => {
    const { wrapper, router } = await mountView()

    await wrapper.get('button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/culling/lot-1')
    expect(router.currentRoute.value.query.name).toBe('Dolomiti')
    expect(router.currentRoute.value.query.library).toBe('lib-1')
  })
})
