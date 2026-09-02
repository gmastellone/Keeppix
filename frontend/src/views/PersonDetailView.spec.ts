import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { runSearch } from '@/api/library'
import { startLiveEvents, type LiveMessage } from '@/api/events'
import { fetchPerson, fetchPersons, mergePersons, patchPerson, type Person, type PersonGroup } from '@/api/persons'
import type { TimelineAsset } from '@/api/timeline'
import PhotoTile from '@/components/ui/PhotoTile.vue'
import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import PersonDetailView from './PersonDetailView.vue'
import PeopleView from './PeopleView.vue'

vi.mock('@/api/library', () => ({
  runSearch: vi.fn(async () => ({ assets: [] }))
}))

vi.mock('@/api/persons', () => ({
  fetchPerson: vi.fn(),
  fetchPersons: vi.fn(async () => []),
  patchPerson: vi.fn(),
  createPerson: vi.fn(),
  fetchPersonGroups: vi.fn(async () => []),
  fetchGroupMembers: vi.fn(async () => []),
  createPersonGroup: vi.fn(),
  renamePersonGroup: vi.fn(),
  deletePersonGroup: vi.fn(),
  addGroupMember: vi.fn(async () => null),
  removeGroupMember: vi.fn(async () => null),
  mergePersons: vi.fn(),
  separatePerson: vi.fn()
}))

vi.mock('@/api/faces', () => ({
  fetchPersonFaceTiles: vi.fn(async () => [])
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
// Unlike `FavoritesView.spec.ts` (which this file is based on), the
// cover/split dialogs here teleport (reka-ui `DialogPortal`) into the
// real `document.body` even when `wrapper` is never unmounted — without
// explicitly unmounting, a test that opens a dialog leaves it there for
// the *next* test, which finds it still in the global DOM. Found while
// writing the "no dialog open" assertion in the "fewer than two faces"
// test below, never an issue before because no earlier test checked for
// the absence of a dialog.
let wrapper: ReturnType<typeof mount> | undefined

beforeEach(() => {
  i18n.global.locale.value = 'it'
  vi.mocked(apiFetch).mockResolvedValue([])
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
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
  wrapper = mount(PersonDetailView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, wrapper }
}

function person(overrides: Partial<Person> = {}): Person {
  return { id: 'p1', name: 'Marta', hidden: false, face_count: 2, ...overrides }
}

function group(overrides: Partial<PersonGroup> = {}): PersonGroup {
  return { id: 'g1', name: 'Famiglia', created_by: 'u1', created_at: '2024-01-01T00:00:00Z', ...overrides }
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

describe('PersonDetailView — person detail', () => {
  it('loads via runSearch({op:"person", id}) and shows the name and photo count', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person({ name: 'Marta' }))
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a'), photo('b')] })

    const { wrapper } = await mountDetail('p1')

    expect(runSearch).toHaveBeenCalledWith({ op: 'person', id: 'p1' }, undefined)
    expect(wrapper.text()).toContain('Marta')
    expect(wrapper.text()).toContain('2 foto')
    expect(wrapper.findAllComponents(PhotoTile)).toHaveLength(2)
  })

  it('shows the current group in the summary line, and the matching action', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person())
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { fetchPersonGroups, fetchGroupMembers } = await import('@/api/persons')
    vi.mocked(fetchPersonGroups).mockResolvedValue([group({ id: 'g1', name: 'Famiglia' })])
    vi.mocked(fetchGroupMembers).mockResolvedValue(['p1'])

    const { wrapper } = await mountDetail('p1')

    expect(wrapper.text()).toContain('gruppo Famiglia')
    expect(wrapper.findAll('button').find((b) => b.text() === 'Cambia gruppo')).toBeTruthy()
  })

  it('shows "senza gruppo" when the person has none', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person())
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountDetail('p1')

    expect(wrapper.text()).toContain('senza gruppo')
    expect(wrapper.findAll('button').find((b) => b.text() === 'Assegna a gruppo')).toBeTruthy()
  })

  it('shows the unnamed label and hidden marker when applicable', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person({ name: null, hidden: true }))
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountDetail('p1')

    expect(wrapper.text()).toContain('Persona senza nome')
    expect(wrapper.text()).toContain('nascosta')
  })

  it('empty state: no photos at all vs filtered-empty are worded differently', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person())
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })

    const { wrapper } = await mountDetail('p1')

    expect(wrapper.text()).toContain('Nessuna foto qui')
    expect(wrapper.text()).not.toContain('Nessuna foto corrisponde ai filtri')
  })

  it('falls back to the grid when the person cannot be loaded', async () => {
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

  it('"Nascondi" patches hidden:true and navigates back to the grid', async () => {
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

  it('refreshes photos on a live "assets.upserted" event, after the debounce settles', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    try {
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
      // Debounced (see PersonDetailView.vue): nothing yet.
      expect(wrapper.findComponent(PhotoTile).exists()).toBe(false)

      await vi.advanceTimersByTimeAsync(800)
      await wrapper.vm.$nextTick()

      expect(wrapper.findComponent(PhotoTile).exists()).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })

  it('"Scegli copertina" opens the cover dialog', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person())
    vi.mocked(runSearch).mockResolvedValue({ assets: [] })
    const { wrapper } = await mountDetail('p1')

    const coverBtn = wrapper.findAll('button').find((b) => b.text() === 'Scegli copertina')
    await coverBtn!.trigger('click')
    await flushPromises()

    expect(document.body.textContent).toContain('Scegli copertina')
  })

  it('"Dividi…" with fewer than two photos shows the toast instead of opening', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person())
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a')] })
    const { wrapper } = await mountDetail('p1')

    const splitBtn = wrapper.findAll('button').find((b) => b.text() === 'Dividi…')
    await splitBtn!.trigger('click')
    await flushPromises()

    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Servono almeno due volti per poter dividere questa persona.')).toBe(true)
    expect(document.body.querySelector('[role="dialog"]')).toBeFalsy()
  })

  it('"Dividi…" with two or more photos opens the split dialog', async () => {
    vi.mocked(fetchPerson).mockResolvedValue(person())
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a'), photo('b')] })
    const { wrapper } = await mountDetail('p1')

    const splitBtn = wrapper.findAll('button').find((b) => b.text() === 'Dividi…')
    await splitBtn!.trigger('click')
    await flushPromises()

    expect(document.body.querySelector('[role="dialog"]')).toBeTruthy()
    expect(document.body.textContent).toContain('Dividi Marta')
  })

  it('"Unisci con…" opens the person picker, excluding this person, then the merge dialog with both people', async () => {
    vi.mocked(fetchPerson).mockImplementation(async (id: string) =>
      id === 'p1' ? person({ id: 'p1', name: 'Marta', face_count: 2 }) : person({ id: 'p2', name: 'Luca', face_count: 3 })
    )
    vi.mocked(fetchPersons).mockResolvedValue([
      person({ id: 'p1', name: 'Marta' }),
      person({ id: 'p2', name: 'Luca' })
    ])
    vi.mocked(runSearch).mockImplementation(async (ast: unknown) => {
      const node = ast as { op: string }
      // The merge preview's union query, distinct from loadPhotos()'s
      // plain per-person query used everywhere else in this file.
      if (node.op === 'or') return { assets: [photo('a'), photo('b'), photo('c'), photo('d')] }
      return { assets: [photo('a'), photo('b')] }
    })
    const { wrapper } = await mountDetail('p1')

    const mergeBtn = wrapper.findAll('button').find((b) => b.text() === 'Unisci con…')
    await mergeBtn!.trigger('click')
    await flushPromises()
    expect(document.body.textContent).toContain('Unisci con chi?')
    expect(document.body.textContent).not.toContain('Marta')

    const targetBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent?.includes('Luca'))
    targetBtn!.click()
    await flushPromises()

    expect(document.body.textContent).toContain('Unisci 2 persone')
    expect(document.body.textContent).toContain('4 foto in tutto')
    expect(document.body.textContent).toContain('Marta')
    expect(document.body.textContent).toContain('Luca')
  })

  it('merging with this person as the survivor refreshes the page in place', async () => {
    vi.mocked(fetchPerson).mockImplementation(async (id: string) =>
      id === 'p1' ? person({ id: 'p1', name: 'Marta' }) : person({ id: 'p2', name: 'Luca' })
    )
    vi.mocked(fetchPersons).mockResolvedValue([
      person({ id: 'p1', name: 'Marta' }),
      person({ id: 'p2', name: 'Luca' })
    ])
    vi.mocked(runSearch).mockResolvedValue({ assets: [photo('a')] })
    vi.mocked(mergePersons).mockResolvedValue(undefined as never)
    const { wrapper, router } = await mountDetail('p1')

    await wrapper.findAll('button').find((b) => b.text() === 'Unisci con…')!.trigger('click')
    await flushPromises()
    Array.from(document.body.querySelectorAll('button'))
      .find((b) => b.textContent?.includes('Luca'))!
      .click()
    await flushPromises()
    Array.from(document.body.querySelectorAll('button'))
      .find((b) => b.textContent === 'Unisci')!
      .click()
    await flushPromises()

    expect(mergePersons).toHaveBeenCalledWith('p1', ['p2'])
    expect(router.currentRoute.value.path).toBe('/persons/p1')
    expect(document.body.querySelector('[role="dialog"]')).toBeFalsy()
  })

  it('merging with this person as the absorbed side redirects to /persons — load() already handles "this person no longer exists"', async () => {
    let p1Calls = 0
    vi.mocked(fetchPerson).mockImplementation(async (id: string) => {
      if (id === 'p2') return person({ id: 'p2', name: 'Luca' })
      p1Calls += 1
      // First call (initial page load) succeeds; the second (load()
      // re-running after the merge, now that p1 was absorbed into p2)
      // must fail — the same "person disappeared" path an outright
      // deletion already takes, reused rather than special-cased here.
      if (p1Calls === 1) return person({ id: 'p1', name: 'Marta' })
      throw new Error('person no longer visible')
    })
    vi.mocked(fetchPersons).mockResolvedValue([
      person({ id: 'p1', name: 'Marta' }),
      person({ id: 'p2', name: 'Luca' })
    ])
    vi.mocked(mergePersons).mockResolvedValue(undefined as never)
    const { wrapper, router } = await mountDetail('p1')

    await wrapper.findAll('button').find((b) => b.text() === 'Unisci con…')!.trigger('click')
    await flushPromises()
    Array.from(document.body.querySelectorAll('button'))
      .find((b) => b.textContent?.includes('Luca'))!
      .click()
    await flushPromises()
    // Pick Luca as the survivor instead of the default (Marta, first
    // named) — the merge dialog's own radiogroup, not the picker's list.
    document.body.querySelector<HTMLButtonElement>('[role="radio"][aria-checked="false"]')!.click()
    Array.from(document.body.querySelectorAll('button'))
      .find((b) => b.textContent === 'Unisci')!
      .click()
    await flushPromises()

    expect(mergePersons).toHaveBeenCalledWith('p2', ['p1'])
    expect(router.currentRoute.value.path).toBe('/persons')
  })
})
