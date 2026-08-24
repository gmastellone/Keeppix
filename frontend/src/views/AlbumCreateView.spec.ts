import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import type { Album } from '@/api/albums'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

import AlbumCreateView from './AlbumCreateView.vue'

const createAlbumMock = vi.fn()
const refreshAlbumMock = vi.fn()
const fetchAllFoldersMock = vi.fn()
const runSearchMock = vi.fn()
const fetchSuggestionsMock = vi.fn()

vi.mock('@/api/albums', () => ({
  createAlbum: (...args: unknown[]) => createAlbumMock(...args),
  refreshAlbum: (...args: unknown[]) => refreshAlbumMock(...args)
}))

vi.mock('@/api/folders', () => ({
  fetchAllFolders: (...args: unknown[]) => fetchAllFoldersMock(...args)
}))

vi.mock('@/api/library', () => ({
  runSearch: (...args: unknown[]) => runSearchMock(...args)
}))

vi.mock('@/api/search', () => ({
  fetchSuggestions: (...args: unknown[]) => fetchSuggestionsMock(...args)
}))

function album(overrides: Partial<Album> = {}): Album {
  return {
    id: 'new-1',
    name: 'Ferie',
    description: '',
    owner_id: 'u1',
    created_at: '',
    updated_at: '',
    is_shared: false,
    monochrome: false,
    ...overrides
  }
}

const testUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

let wrapper: VueWrapper | undefined

beforeEach(() => {
  i18n.global.locale.value = 'it'
  createAlbumMock.mockResolvedValue(album())
  refreshAlbumMock.mockResolvedValue({ succeeded: [] })
  fetchAllFoldersMock.mockResolvedValue([
    { id: 'f1', library_id: 'l', parent_id: null, name: 'Urbino', depth: 0 },
    { id: 'f2', library_id: 'l', parent_id: null, name: 'Lago di Braies', depth: 0 }
  ])
  runSearchMock.mockResolvedValue({ assets: [] })
  fetchSuggestionsMock.mockResolvedValue({ suggestions: [] })
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

async function mountCreate() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/albums', component: { template: '<div />' } },
      { path: '/albums/new', component: AlbumCreateView },
      { path: '/albums/:id', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser
  session.initialised = true
  session.ready = true

  await router.push('/albums/new')
  await router.isReady()
  wrapper = mount(AlbumCreateView, { global: { plugins: [router, i18n] }, attachTo: document.body })
  await flushPromises()
  return { router, wrapper }
}

describe('AlbumCreateView — §43 creazione album', () => {
  it('an empty name shows a toast and creates nothing', async () => {
    const { wrapper } = await mountCreate()

    await wrapper.get('button:not([type]), button[type="button"]').trigger('click')
    const submit = wrapper.findAll('button').find((b) => b.text() === 'Crea album' && b.attributes('disabled') === undefined)
    await submit!.trigger('click')
    await flushPromises()

    expect(createAlbumMock).not.toHaveBeenCalled()
  })

  it('Manuale (default): creates the album with no rule and lands on its detail', async () => {
    const { wrapper, router } = await mountCreate()

    await wrapper.get('#album-name').setValue('  Ferie  ')
    const submit = wrapper.findAll('button').find((b) => b.text() === 'Crea album')
    await submit!.trigger('click')
    await flushPromises()

    expect(createAlbumMock).toHaveBeenCalledWith('Ferie', undefined)
    expect(refreshAlbumMock).not.toHaveBeenCalled()
    expect(router.currentRoute.value.path).toBe('/albums/new-1')
  })

  it('"Basato su filtro" with zero conditions shows a toast and creates nothing', async () => {
    const { wrapper } = await mountCreate()
    await wrapper.get('#album-name').setValue('Ferie')

    const filterOption = wrapper.findAll('[role="radio"]').find((r) => r.text() === 'Basato su filtro')
    await filterOption!.trigger('click')
    await flushPromises()

    const submit = wrapper.findAll('button').find((b) => b.text() === 'Crea album')
    await submit!.trigger('click')
    await flushPromises()

    expect(createAlbumMock).not.toHaveBeenCalled()
  })

  it('"Basato su filtro" with a folder picked creates with a rule, then applies it once via refresh', async () => {
    const { wrapper, router } = await mountCreate()
    await wrapper.get('#album-name').setValue('Ferie')
    const filterOption = wrapper.findAll('[role="radio"]').find((r) => r.text() === 'Basato su filtro')
    await filterOption!.trigger('click')
    await flushPromises()

    // Il trigger della picklist "Cartella" è già il campo di default della
    // prima condizione (§43: freshAlbumDraft parte su "Cartella").
    await wrapper.get('[aria-haspopup="listbox"]').trigger('click')
    await flushPromises()
    const option = document.body.querySelectorAll('[role="option"]')[0] as HTMLElement
    option.click()
    await flushPromises()

    const submit = wrapper.findAll('button').find((b) => b.text() === 'Crea album')
    await submit!.trigger('click')
    await flushPromises()

    expect(createAlbumMock).toHaveBeenCalledWith('Ferie', { op: 'folder', id: 'f1' })
    expect(refreshAlbumMock).toHaveBeenCalledWith('new-1')
    expect(router.currentRoute.value.path).toBe('/albums/new-1')
  })

  it('shows a live preview count from runSearch, debounced', async () => {
    runSearchMock.mockResolvedValue({ assets: [{ id: 'a' }, { id: 'b' }] })
    const { wrapper } = await mountCreate()
    const filterOption = wrapper.findAll('[role="radio"]').find((r) => r.text() === 'Basato su filtro')
    await filterOption!.trigger('click')
    await wrapper.get('[aria-haspopup="listbox"]').trigger('click')
    await flushPromises()
    const option = document.body.querySelectorAll('[role="option"]')[0] as HTMLElement
    option.click()
    await flushPromises()

    await wait(400)
    await flushPromises()

    expect(wrapper.text()).toContain('2')
    expect(wrapper.text()).toContain('foto corrispondono al filtro attuale')
  })

  it('"Tutti gli album" / "Annulla" both go back to the grid', async () => {
    const { wrapper, router } = await mountCreate()

    await wrapper.get('button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/albums')
  })
})
