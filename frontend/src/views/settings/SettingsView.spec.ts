import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import type { User } from '@/api/auth'
import type { CullingLot } from '@/api/culling'
import type { FolderView } from '@/api/folders'
import type { Library } from '@/api/libraries'
import type { UserPreferences } from '@/api/preferences'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import SettingsView from './SettingsView.vue'

const fetchPreferencesMock = vi.fn()
const patchPreferencesMock = vi.fn()
const fetchLibrariesMock = vi.fn()
const patchLibraryMock = vi.fn()
const patchCullingRootMock = vi.fn()
const deleteAllFaceDataMock = vi.fn()
const updateUserMock = vi.fn()
const fetchAllFoldersMock = vi.fn()
const fetchChildrenMock = vi.fn()
const fetchCullingLotsMock = vi.fn()

vi.mock('@/api/preferences', () => ({
  fetchPreferences: (...args: unknown[]) => fetchPreferencesMock(...args),
  patchPreferences: (...args: unknown[]) => patchPreferencesMock(...args)
}))

vi.mock('@/api/libraries', () => ({
  fetchLibraries: (...args: unknown[]) => fetchLibrariesMock(...args),
  patchLibrary: (...args: unknown[]) => patchLibraryMock(...args),
  patchCullingRoot: (...args: unknown[]) => patchCullingRootMock(...args)
}))

vi.mock('@/api/faces', () => ({
  deleteAllFaceData: (...args: unknown[]) => deleteAllFaceDataMock(...args)
}))

vi.mock('@/api/users', () => ({
  updateUser: (...args: unknown[]) => updateUserMock(...args)
}))

vi.mock('@/api/folders', () => ({
  fetchAllFolders: (...args: unknown[]) => fetchAllFoldersMock(...args),
  fetchChildren: (...args: unknown[]) => fetchChildrenMock(...args)
}))

vi.mock('@/api/culling', () => ({
  fetchCullingLots: (...args: unknown[]) => fetchCullingLotsMock(...args)
}))

function stubMatchMedia(matchesByQuery: Record<string, boolean> = {}) {
  vi.stubGlobal(
    'matchMedia',
    vi.fn().mockImplementation((query: string) => ({
      matches: matchesByQuery[query] ?? false,
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn()
    }))
  )
}

function preferences(overrides: Partial<UserPreferences> = {}): UserPreferences {
  return {
    theme: 'chiaro',
    grid_density: { desktop: 4, mobile: 3 },
    notifications: { digest: true, condivisioni: true, problemi: false },
    language: 'it',
    ...overrides
  }
}

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
    culling_root_folder_id: null,
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

function cullingLot(overrides: Partial<CullingLot> = {}): CullingLot {
  return {
    folder_id: 'lot-1',
    name: '2026-08-14',
    created_at: '',
    pending: 3,
    taken: 1,
    skipped: 0,
    ...overrides
  }
}

const adminUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

const memberUser = { ...adminUser, id: '2', role: 'user' as const }

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let wrapper: VueWrapper | undefined

beforeEach(() => {
  i18n.global.locale.value = 'it'
  stubMatchMedia()
  fetchPreferencesMock.mockResolvedValue(preferences())
  patchPreferencesMock.mockResolvedValue(preferences())
  fetchLibrariesMock.mockResolvedValue([library()])
  patchLibraryMock.mockResolvedValue(library({ faces_enabled: false }))
  deleteAllFaceDataMock.mockResolvedValue(null)
  updateUserMock.mockResolvedValue({ ...adminUser, locale: 'en' })
  fetchAllFoldersMock.mockResolvedValue([folder()])
  fetchChildrenMock.mockResolvedValue({ folders: [], assets: [] })
  fetchCullingLotsMock.mockResolvedValue([])
  patchCullingRootMock.mockResolvedValue(library({ culling_root_folder_id: 'root-1' }))
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
  vi.unstubAllGlobals()
  document.documentElement.removeAttribute('data-theme')
})

async function mountSettings(user: User = adminUser) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const session = useSessionStore()
  session.user = user
  session.initialised = true
  session.ready = true
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/settings', component: SettingsView },
      { path: '/settings/maps/offline', component: { template: '<div />' } }
    ]
  })
  await router.push('/settings')
  await router.isReady()
  wrapper = mount(SettingsView, { global: { plugins: [i18n, pinia, router] }, attachTo: document.body })
  await flushPromises()
  return { wrapper }
}

describe('SettingsView — Settings', () => {
  it('loads real preferences and libraries on mount', async () => {
    const { wrapper } = await mountSettings()

    expect(fetchPreferencesMock).toHaveBeenCalled()
    expect(fetchLibrariesMock).toHaveBeenCalled()
    expect(wrapper.text()).toContain('Riconoscimento facciale attivo — Lago di Braies')
  })

  it('changing theme applies immediately and persists via PATCH', async () => {
    const { wrapper } = await mountSettings()

    const darkOption = wrapper.findAll('[role="radio"]').find((b) => b.text() === 'Scuro')
    await darkOption!.trigger('click')
    await flushPromises()

    expect(patchPreferencesMock).toHaveBeenCalledWith({ theme: 'scuro' })
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark')
  })

  it('moving the density slider persists the desktop value', async () => {
    const { wrapper } = await mountSettings()

    const slider = wrapper.get('input[type="range"]')
    await slider.setValue('7')

    expect(patchPreferencesMock).toHaveBeenCalledWith({ grid_density: { desktop: 7 } })
  })

  it('toggling a notification applies immediately, before the PATCH resolves', async () => {
    // A promise that never resolves during the test: verifies the optimistic
    // state "immediately", without racing the rollback microtask (which,
    // with a mock rejected instantly, can run before the single
    // `nextTick()` inside `trigger()` even resolves — never the case with a
    // real network call, which always has real latency).
    patchPreferencesMock.mockReturnValueOnce(new Promise(() => {}))
    const { wrapper } = await mountSettings()

    const digestSwitch = wrapper.findAll('[role="switch"]')[0]
    await digestSwitch.trigger('click')

    expect(digestSwitch.attributes('aria-checked')).toBe('false')
  })

  it('a failed PATCH rolls back the notification and shows an error toast', async () => {
    patchPreferencesMock.mockRejectedValueOnce(new Error('network'))
    const { wrapper } = await mountSettings()

    const digestSwitch = wrapper.findAll('[role="switch"]')[0]
    await digestSwitch.trigger('click')
    await flushPromises()

    expect(digestSwitch.attributes('aria-checked')).toBe('true')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.kind === 'error')).toBe(true)
  })

  it('changing language calls the real session.changeLocale', async () => {
    const { wrapper } = await mountSettings()

    await wrapper.get('select').setValue('en')
    await flushPromises()

    expect(updateUserMock).toHaveBeenCalledWith('1', { locale: 'en' })
  })

  it('an admin can toggle "Riconoscimento volti" per library', async () => {
    const { wrapper } = await mountSettings()

    const facesLabel = wrapper.findAll('label').find((l) => l.text().includes('Riconoscimento facciale attivo'))
    const facesSwitch = facesLabel!.get('[role="switch"]')
    await facesSwitch.trigger('click')
    await flushPromises()

    expect(patchLibraryMock).toHaveBeenCalledWith('lib-1', { faces_enabled: false })
  })

  it('a non-admin cannot toggle "Riconoscimento volti" and sees the admin-only note', async () => {
    const { wrapper } = await mountSettings(memberUser)

    expect(wrapper.text()).toContain('Solo un amministratore può cambiare questa impostazione.')
    const facesLabel = wrapper.findAll('label').find((l) => l.text().includes('Riconoscimento facciale attivo'))
    const facesSwitch = facesLabel!.get('[role="switch"]')
    expect(facesSwitch.attributes('disabled')).toBeDefined()
  })

  it('"Elimina tutti i dati dei volti" asks for confirmation, then deletes and toasts', async () => {
    const { wrapper } = await mountSettings()

    await wrapper.get('button.text-danger').trigger('click')
    await tick()
    expect(deleteAllFaceDataMock).not.toHaveBeenCalled()

    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Elimina tutto'
    )
    confirmBtn?.click()
    await flushPromises()

    expect(deleteAllFaceDataMock).toHaveBeenCalled()
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Dati dei volti eliminati.')).toBe(true)
  })

  it('a non-admin does not see the "Elimina tutti i dati dei volti" button', async () => {
    const { wrapper } = await mountSettings(memberUser)

    expect(wrapper.text()).not.toContain('Elimina tutti i dati dei volti')
  })
})

describe('SettingsView — culling root folder', () => {
  it('shows "Non impostata" when the library has no culling root yet', async () => {
    const { wrapper } = await mountSettings()

    expect(wrapper.text()).toContain('Cartella di culling')
    expect(wrapper.text()).toContain('Non impostata')
  })

  it('resolves the configured folder to a real name breadcrumb and shows the lot count', async () => {
    fetchLibrariesMock.mockResolvedValue([library({ culling_root_folder_id: 'culling-1' })])
    fetchAllFoldersMock.mockResolvedValue([
      folder({ id: 'root-1', parent_id: null, name: 'Lago di Braies' }),
      folder({ id: 'culling-1', parent_id: 'root-1', name: 'Culling', depth: 1 })
    ])
    fetchCullingLotsMock.mockResolvedValue([cullingLot(), cullingLot({ folder_id: 'lot-2' })])
    const { wrapper } = await mountSettings()

    expect(fetchCullingLotsMock).toHaveBeenCalledWith('lib-1')
    expect(wrapper.text()).toContain('Culling')
    expect(wrapper.text()).toContain('2 lotti attivi')
  })

  it('an owner can open the picker and confirming a folder calls patchCullingRoot', async () => {
    fetchAllFoldersMock.mockResolvedValue([folder({ id: 'root-1', parent_id: null })])
    fetchChildrenMock.mockResolvedValue({
      folders: [folder({ id: 'sub-1', parent_id: 'root-1', name: 'Culling', depth: 1 })],
      assets: []
    })
    const { wrapper } = await mountSettings()

    const changeBtn = wrapper.findAll('button').find((b) => b.text() === 'Cambia…')
    await changeBtn!.trigger('click')
    await flushPromises()

    expect(fetchChildrenMock).toHaveBeenCalledWith('root-1')
    const rowBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent?.includes('Culling'))
    rowBtn?.click()
    await flushPromises()
    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Usa questa cartella')
    confirmBtn?.click()
    await flushPromises()

    expect(patchCullingRootMock).toHaveBeenCalledWith('lib-1', 'sub-1')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Cartella di culling aggiornata.')).toBe(true)
  })

  it('a non-owner non-admin does not see "Cambia…" for the culling root', async () => {
    fetchLibrariesMock.mockResolvedValue([library({ owner_id: 'someone-else' })])
    const { wrapper } = await mountSettings(memberUser)

    const cullingSection = wrapper.text()
    expect(cullingSection).toContain('Cartella di culling')
    const changeBtn = wrapper.findAll('button').find((b) => b.text() === 'Cambia…')
    expect(changeBtn).toBeUndefined()
  })
})
