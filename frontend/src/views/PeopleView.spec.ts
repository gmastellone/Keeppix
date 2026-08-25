import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter, type Router } from 'vue-router'

import { runSearch } from '@/api/library'
import {
  createPersonGroup,
  deletePersonGroup,
  fetchGroupMembers,
  fetchPersonGroups,
  fetchPersons,
  mergePersons,
  renamePersonGroup,
  type Person,
  type PersonGroup
} from '@/api/persons'
import { i18n } from '@/i18n'

import PeopleView from './PeopleView.vue'

vi.mock('@/api/persons', () => ({
  fetchPersons: vi.fn(),
  createPerson: vi.fn(),
  fetchPersonGroups: vi.fn(async () => []),
  fetchGroupMembers: vi.fn(async () => []),
  createPersonGroup: vi.fn(),
  renamePersonGroup: vi.fn(),
  deletePersonGroup: vi.fn(),
  addGroupMember: vi.fn(async () => null),
  removeGroupMember: vi.fn(async () => null),
  mergePersons: vi.fn()
}))

vi.mock('@/api/library', () => ({
  runSearch: vi.fn(async () => ({ assets: [] }))
}))

function person(overrides: Partial<Person> = {}): Person {
  return { id: 'p1', name: 'Marta', hidden: false, face_count: 3, ...overrides }
}

function group(overrides: Partial<PersonGroup> = {}): PersonGroup {
  return { id: 'g1', name: 'Famiglia', created_by: 'u1', created_at: '2024-01-01T00:00:00Z', ...overrides }
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

describe('PeopleView — §31 Persone (griglia ridotta senza gruppi, 1/N)', () => {
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

    await wrapper.get('[role="button"]').trigger('click')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/persons/p1')
  })
})

describe('PeopleView — §31 gruppi (2/N)', () => {
  it('partitions people into group blocks and a trailing "Senza gruppo" block', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([person({ id: 'p1', name: 'Marta' }), person({ id: 'p2', name: 'Davide' })])
    vi.mocked(fetchPersonGroups).mockResolvedValue([group({ id: 'g1', name: 'Famiglia' })])
    vi.mocked(fetchGroupMembers).mockResolvedValue(['p1'])
    const { wrapper } = await mountPeople()

    expect(wrapper.text()).toContain('Famiglia')
    expect(wrapper.text()).toContain('Senza gruppo')
    expect(wrapper.text()).toContain('1 persona')
  })

  it('"Nuovo gruppo" creates a group for real via createPersonGroup', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([])
    vi.mocked(createPersonGroup).mockResolvedValue(group({ id: 'g2', name: 'Amici' }))
    const { wrapper } = await mountPeople()

    await wrapper.get('button').trigger('click')
    await flushPromises()
    const input = document.body.querySelector('input') as HTMLInputElement
    input.value = 'Amici'
    input.dispatchEvent(new Event('input'))
    const createBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Crea')
    createBtn?.click()
    await flushPromises()

    expect(createPersonGroup).toHaveBeenCalledWith('Amici')
  })

  it('renaming a group (matita) precompiles the name and calls renamePersonGroup', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([person({ id: 'p1' })])
    vi.mocked(fetchPersonGroups).mockResolvedValue([group({ id: 'g1', name: 'Famiglia' })])
    vi.mocked(fetchGroupMembers).mockResolvedValue(['p1'])
    vi.mocked(renamePersonGroup).mockResolvedValue(group({ id: 'g1', name: 'Parenti' }))
    const { wrapper } = await mountPeople()

    await wrapper.get('[aria-label="Rinomina gruppo Famiglia"]').trigger('click')
    await flushPromises()

    const input = document.body.querySelector('input') as HTMLInputElement
    expect(input.value).toBe('Famiglia')
    input.value = 'Parenti'
    input.dispatchEvent(new Event('input'))
    const saveBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Salva')
    saveBtn?.click()
    await flushPromises()

    expect(renamePersonGroup).toHaveBeenCalledWith('g1', 'Parenti')
  })

  it('deleting a group calls deletePersonGroup and reloads', async () => {
    vi.mocked(fetchPersons).mockResolvedValueOnce([person({ id: 'p1' })])
    vi.mocked(fetchPersons).mockResolvedValueOnce([person({ id: 'p1' })])
    vi.mocked(fetchPersons).mockResolvedValueOnce([person({ id: 'p1' })])
    vi.mocked(fetchPersons).mockResolvedValueOnce([person({ id: 'p1' })])
    vi.mocked(fetchPersonGroups).mockResolvedValue([group({ id: 'g1', name: 'Famiglia' })])
    vi.mocked(fetchGroupMembers).mockResolvedValue(['p1'])
    vi.mocked(deletePersonGroup).mockResolvedValue(null)
    const { wrapper } = await mountPeople()

    await wrapper.get('[aria-label="Elimina gruppo Famiglia"]').trigger('click')
    await flushPromises()
    const confirmBtn = Array.from(document.body.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find(
      (b) => b.textContent === 'Elimina gruppo'
    )
    confirmBtn?.click()
    await flushPromises()

    expect(deletePersonGroup).toHaveBeenCalledWith('g1')
  })

  it('checking two cards enables "Unisci"; confirming calls mergePersons with the survivor', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([
      person({ id: 'p1', name: 'Marta', face_count: 5 }),
      person({ id: 'p2', name: '', face_count: 2 })
    ])
    vi.mocked(fetchPersonGroups).mockResolvedValue([])
    vi.mocked(fetchGroupMembers).mockResolvedValue([])
    vi.mocked(mergePersons).mockResolvedValue(person({ id: 'p1' }))
    const { wrapper } = await mountPeople()

    const checkboxes = wrapper.findAll('[role="checkbox"]')
    await checkboxes[0].trigger('click')
    await checkboxes[1].trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('2 selezionate')
    const mergeBtn = wrapper.findAll('button').find((b) => b.text() === 'Unisci')
    expect(mergeBtn).toBeTruthy()
    await mergeBtn!.trigger('click')
    await flushPromises()

    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Unisci' && b.closest('[role="dialog"]')
    )
    confirmBtn?.click()
    await flushPromises()

    expect(mergePersons).toHaveBeenCalledWith('p1', ['p2'])
  })

  it('checking one card shows "Assegna a gruppo" but not "Unisci"', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([person({ id: 'p1' })])
    vi.mocked(fetchPersonGroups).mockResolvedValue([])
    vi.mocked(fetchGroupMembers).mockResolvedValue([])
    const { wrapper } = await mountPeople()

    await wrapper.get('[role="checkbox"]').trigger('click')
    await flushPromises()

    expect(wrapper.findAll('button').find((b) => b.text() === 'Unisci')).toBeFalsy()
    expect(wrapper.findAll('button').find((b) => b.text() === 'Assegna a gruppo')).toBeTruthy()
  })

  it('assigning the selection to a group calls addGroupMember for each selected person', async () => {
    vi.mocked(fetchPersons).mockResolvedValue([person({ id: 'p1' }), person({ id: 'p2' })])
    vi.mocked(fetchPersonGroups).mockResolvedValue([group({ id: 'g1', name: 'Famiglia' })])
    vi.mocked(fetchGroupMembers).mockResolvedValue([])
    const { wrapper } = await mountPeople()

    const checkboxes = wrapper.findAll('[role="checkbox"]')
    await checkboxes[0].trigger('click')
    await checkboxes[1].trigger('click')
    await flushPromises()

    const assignBtn = wrapper.findAll('button').find((b) => b.text() === 'Assegna a gruppo')
    await assignBtn!.trigger('click')
    await flushPromises()

    const groupRow = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Famiglia')
    groupRow?.click()
    await flushPromises()

    const { addGroupMember } = await import('@/api/persons')
    expect(addGroupMember).toHaveBeenCalledWith('g1', 'p1')
    expect(addGroupMember).toHaveBeenCalledWith('g1', 'p2')
  })
})
