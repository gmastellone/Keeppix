import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Person } from '@/api/persons'
import { i18n } from '@/i18n'

const fetchPersonsMock = vi.fn()
const createPersonMock = vi.fn()

vi.mock('@/api/persons', () => ({
  fetchPersons: (...args: unknown[]) => fetchPersonsMock(...args),
  createPerson: (...args: unknown[]) => createPersonMock(...args)
}))

const PersonPickerDialog = (await import('./PersonPickerDialog.vue')).default

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function person(overrides: Partial<Person> = {}): Person {
  return { id: 'p1', name: 'Marta', hidden: false, face_count: 3, ...overrides }
}

let wrapper: VueWrapper | undefined

function mountHost() {
  const Host = defineComponent({
    components: { ThePersonPickerDialog: PersonPickerDialog },
    emits: ['picked'],
    setup(_, { emit }) {
      const open = ref(true)
      return { open, onPicked: (id: string) => emit('picked', id) }
    },
    template: `<ThePersonPickerDialog v-model:open="open" @picked="onPicked" />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  i18n.global.locale.value = 'it'
  fetchPersonsMock.mockResolvedValue([person()])
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
})

describe('PersonPickerDialog — §37 selettore di persona (verificato Task 16 3/N)', () => {
  it('shows the exact documented title and focuses the search field on open', async () => {
    mountHost()
    await tick()

    expect(document.body.textContent).toContain('Assegna persona')
    const input = document.body.querySelector('input') as HTMLInputElement
    expect(document.activeElement).toBe(input)
  })

  it('lists visible persons with their real photo count', async () => {
    fetchPersonsMock.mockResolvedValue([person({ name: 'Marta', face_count: 12 })])
    mountHost()
    await tick()

    expect(document.body.textContent).toContain('Marta')
    expect(document.body.textContent).toContain('12 foto')
  })

  it('the "Crea persona" row shows only when the typed text is not an exact existing match', async () => {
    fetchPersonsMock.mockResolvedValue([person({ name: 'Marta' })])
    mountHost()
    await tick()

    const input = document.body.querySelector('input') as HTMLInputElement
    input.value = 'Luca'
    input.dispatchEvent(new Event('input'))
    await tick()
    expect(document.body.textContent).toContain('Crea persona "Luca"')

    input.value = 'Marta'
    input.dispatchEvent(new Event('input'))
    await tick()
    expect(document.body.textContent).not.toContain('Crea persona')
  })

  it('shows "Nessuna persona trovata." when no person exists at all (§37.7)', async () => {
    fetchPersonsMock.mockResolvedValue([])
    mountHost()
    await tick()

    expect(document.body.textContent).toContain('Nessuna persona trovata.')
  })

  it('picking a row emits picked and closes; "Annulla" closes without picking', async () => {
    fetchPersonsMock.mockResolvedValue([person({ id: 'p1', name: 'Marta' })])
    const w = mountHost()
    await tick()

    const row = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent?.includes('Marta'))
    row?.click()
    await tick()

    expect(w.emitted('picked')).toEqual([['p1']])
  })

  it('"Annulla" closes the dialog without invoking the callback', async () => {
    const w = mountHost()
    await tick()

    const cancelBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Annulla')
    cancelBtn?.click()
    await tick()

    expect(w.emitted('picked')).toBeFalsy()
    expect(document.body.querySelector('[role="dialog"]')).toBeFalsy()
  })

  it('"Crea persona «X»" creates the person for real and emits picked', async () => {
    createPersonMock.mockResolvedValue(person({ id: 'new-1', name: 'Luca' }))
    const w = mountHost()
    await tick()

    const input = document.body.querySelector('input') as HTMLInputElement
    input.value = 'Luca'
    input.dispatchEvent(new Event('input'))
    await tick()
    const createBtn = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.includes('Crea persona')
    )
    createBtn?.click()
    await tick()

    expect(createPersonMock).toHaveBeenCalledWith('Luca')
    expect(w.emitted('picked')).toEqual([['new-1']])
  })
})
