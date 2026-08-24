import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'

import { runSearch } from '@/api/library'
import { fetchPersons, type Person } from '@/api/persons'
import { i18n } from '@/i18n'

import PeopleView from './PeopleView.vue'

vi.mock('@/api/persons', () => ({
  fetchPersons: vi.fn(),
  createPerson: vi.fn()
}))

vi.mock('@/api/library', () => ({
  runSearch: vi.fn(async () => ({ assets: [] }))
}))

function person(overrides: Partial<Person> = {}): Person {
  return { id: 'p1', name: 'Marta', hidden: false, face_count: 3, ...overrides }
}

let wrapper: VueWrapper | undefined
let router: Router

beforeEach(() => {
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

async function mountPeople() {
  router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/persons', component: PeopleView },
      { path: '/persons/:id', component: { template: '<div/>' } }
    ]
  })
  setActivePinia(createPinia())
  await router.push('/persons')
  await router.isReady()
  wrapper = mount(PeopleView, { global: { plugins: [router, i18n] }, attachTo: document.body })
  await flushPromises()
  return { wrapper, router }
}

describe('PeopleView — §31 Persone (griglia ridotta, senza gruppi)', () => {
  it('lists only visible people with at least one confirmed face', async () => {
    vi.mocked(fetchPersons).mockImplementation(async (includeHidden) =>
      includeHidden
        ? [person({ id: 'p1' }), person({ id: 'p2', face_count: 0 }), person({ id: 'p3', hidden: true })]
        : [person({ id: 'p1' }), person({ id: 'p2', face_count: 0 })]
    )
    const { wrapper } = await mountPeople()

    expect(wrapper.text()).toContain('Marta')
    expect(wrapper.text()).toContain('3 foto')
  })

  it('shows the unnamed hint for a nameless person', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([person({ name: null })])
    const { wrapper } = await mountPeople()

    expect(wrapper.text()).toContain('Persona senza nome')
    expect(wrapper.text()).toContain('da nominare')
  })

  it('shows the hidden-count footer line using a second include_hidden fetch', async () => {
    vi.mocked(fetchPersons).mockImplementation(async (includeHidden) =>
      includeHidden ? [person({ id: 'p1' }), person({ id: 'p2', hidden: true })] : [person({ id: 'p1' })]
    )
    const { wrapper } = await mountPeople()

    expect(fetchPersons).toHaveBeenCalledWith(true)
    expect(wrapper.text()).toContain('1 persona nascosta non mostrata qui.')
  })

  it('the empty state shows when there is no visible person', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([])
    const { wrapper } = await mountPeople()

    expect(wrapper.text()).toContain('Nessuna persona riconosciuta ancora.')
  })

  it('loads a real cover photo per card via runSearch({op:"person"}, undefined, 1)', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([person({ id: 'p1' })])
    vi.mocked(runSearch).mockResolvedValue({
      assets: [{ id: 'a1', content_hash: 'deadbeef', thumbhash: null } as never]
    })
    await mountPeople()

    expect(runSearch).toHaveBeenCalledWith({ op: 'person', id: 'p1' }, undefined, 1)
  })

  it('clicking a card navigates to the person detail route', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([person({ id: 'p1' })])
    const { wrapper, router } = await mountPeople()

    await wrapper.get('button').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/persons/p1')
  })
})
