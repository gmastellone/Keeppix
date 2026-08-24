import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { runSearch } from '@/api/library'
import { startLiveEvents, type LiveMessage } from '@/api/events'
import { fetchPerson, patchPerson, type Person } from '@/api/persons'
import type { TimelineAsset } from '@/api/timeline'
import PhotoTile from '@/components/ui/PhotoTile.vue'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

import PersonDetailView from './PersonDetailView.vue'
import PeopleView from './PeopleView.vue'

vi.mock('@/api/library', () => ({
  runSearch: vi.fn(async () => ({ assets: [] }))
}))

vi.mock('@/api/persons', () => ({
  fetchPerson: vi.fn(),
  fetchPersons: vi.fn(async () => []),
  patchPerson: vi.fn(),
  createPerson: vi.fn()
}))

vi.mock('@/api/events', () => ({
  startLiveEvents: vi.fn(() => ({ close: vi.fn() }))
}))

vi.mock('@/api/culling', () => ({
  fetchFlags: vi.fn(async () => ({ rating: null, pick: 'none', color_label: null, favorite: false })),
  setFlags: vi.fn(async () => null),
  deleteAsset: vi.fn(async () => null),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
}))

vi.mock('@/api/albums', () => ({
  fetchAlbums: vi.fn(async () => []),
  fetchAlbum: vi.fn(async () => ({ id: 'x', name: '', assets: [] })),
  addAssets: vi.fn(async () => null),
  removeAsset: vi.fn(async () => null),
  fetchAlbumsForAsset: vi.fn(async () => [])
}))

vi.mock('@/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const { apiFetch } = await import('@/api/client')

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

let unstubLayout: () => void

beforeEach(() => {
  i18n.global.locale.value = 'it'
  vi.mocked(apiFetch).mockResolvedValue([])
})

afterEach(() => {
  vi.resetAllMocks()
  vi.unstubAllGlobals()
  unstubLayout?.()
})

const testUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

async function mountDetail(id = 'p1') {
  unstubLayout = stubLayout(1200, 900)
  stubMatchMedia()
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/persons', component: PeopleView },
      { path: '/persons/:id', component: PersonDetailView }
    ]
  })
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser
  session.initialised = true
  session.ready = true

  await router.push(`/persons/${id}`)
  await router.isReady()
  const wrapper = mount(PersonDetailView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, wrapper }
}

function person(overrides: Partial<Person> = {}): Person {
  return { id: 'p1', name: 'Marta', hidden: false, face_count: 2, ...overrides }
}

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

describe('PersonDetailView — §32 dettaglio persona', () => {
  it('loads via runSearch({op:"person", id}) and shows the name and photo count', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person({ name: 'Marta' }))
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a'), photo('b')] })

    const { wrapper } = await mountDetail('p1')

    expect(runSearch).toHaveBeenCalledWith({ op: 'person', id: 'p1' }, undefined)
    expect(wrapper.text()).toContain('Marta')
    expect(wrapper.text()).toContain('2 foto')
    expect(wrapper.findAllComponents(PhotoTile)).toHaveLength(2)
  })

  it('shows the unnamed label and hidden marker when applicable', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person({ name: null, hidden: true }))
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountDetail('p1')

    expect(wrapper.text()).toContain('Persona senza nome')
    expect(wrapper.text()).toContain('nascosta')
  })

  it('§32.2 empty state: no photos at all vs filtered-empty are worded differently', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person())
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountDetail('p1')

    expect(wrapper.text()).toContain('Nessuna foto qui')
    expect(wrapper.text()).not.toContain('Nessuna foto corrisponde ai filtri')
  })

  it('falls back to the grid when the person cannot be loaded (§32.8)', async () => {
    vi.mocked(fetchPerson).mockRejectedValue(new Error('forbidden'))

    const { router } = await mountDetail('gone')

    expect(router.currentRoute.value.path).toBe('/persons')
  })

  it('"Rinomina" saves via patchPerson and updates the header', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person({ name: 'Marta' }))
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    vi.mocked(patchPerson).mockResolvedValue(person({ name: 'Marta Rossi' }))

    const { wrapper } = await mountDetail('p1')

    const renameBtn = wrapper.findAll('button').find((b) => b.text() === 'Rinomina')
    await renameBtn!.trigger('click')
    await flushPromises()

    const input = document.body.querySelector('input') as HTMLInputElement
    input.value = 'Marta Rossi'
    input.dispatchEvent(new Event('input'))
    const saveBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Salva')
    saveBtn?.click()
    await flushPromises()

    expect(patchPerson).toHaveBeenCalledWith('p1', { name: 'Marta Rossi' })
    expect(wrapper.text()).toContain('Marta Rossi')
  })

  it('"Nascondi" patches hidden:true and navigates back to the grid (§32.3 control 6)', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person({ hidden: false }))
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    vi.mocked(patchPerson).mockResolvedValue(person({ hidden: true }))

    const { wrapper, router } = await mountDetail('p1')

    const hideBtn = wrapper.findAll('button').find((b) => b.text() === 'Nascondi')
    await hideBtn!.trigger('click')
    await flushPromises()

    expect(patchPerson).toHaveBeenCalledWith('p1', { hidden: true })
    expect(router.currentRoute.value.path).toBe('/persons')
  })

  it('refreshes photos on a live "assets.upserted" event', async () => {
    let onEvent: ((msg: LiveMessage) => void) | undefined
    vi.mocked(startLiveEvents).mockImplementation((cb) => {
      onEvent = cb
      return { close: vi.fn() }
    })
    vi.mocked(fetchPerson).mockResolvedValue(person())
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountDetail('p1')
    expect(wrapper.text()).toContain('Nessuna foto qui')

    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('live')] })
    onEvent?.({ v: 1, type: 'assets.upserted', payload: { ids: ['live'], count: 1 } })
    await flushPromises()

    expect(wrapper.findComponent(PhotoTile).exists()).toBe(true)
  })
})
