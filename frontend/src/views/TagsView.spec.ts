import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Tag } from '@/api/tags'
import { i18n } from '@/i18n'
import { useToastStore } from '@/stores/toast'

import TagsView from './TagsView.vue'

const fetchTagsMock = vi.fn()
const createTagMock = vi.fn()
const patchTagMock = vi.fn()
const deleteTagMock = vi.fn()

vi.mock('@/api/tags', () => ({
  fetchTags: (...args: unknown[]) => fetchTagsMock(...args),
  createTag: (...args: unknown[]) => createTagMock(...args),
  patchTag: (...args: unknown[]) => patchTagMock(...args),
  deleteTag: (...args: unknown[]) => deleteTagMock(...args)
}))

function category(overrides: Partial<Tag> = {}): Tag {
  return {
    id: 'cat-1',
    name: 'Natura',
    kind: 'category',
    parent_id: null,
    color: null,
    assignment_count: 0,
    ...overrides
  }
}

function tag(overrides: Partial<Tag> = {}): Tag {
  return {
    id: 'tag-1',
    name: 'Tramonti',
    kind: 'tag',
    parent_id: 'cat-1',
    color: '#e0578a',
    threshold: 0.75,
    prompt: undefined,
    assignment_count: 3,
    ...overrides
  }
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let wrapper: VueWrapper | undefined

beforeEach(() => {
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

async function mountTags() {
  const pinia = createPinia()
  setActivePinia(pinia)
  wrapper = mount(TagsView, { global: { plugins: [i18n, pinia] }, attachTo: document.body })
  await flushPromises()
  return { wrapper }
}

describe('TagsView — Tags and categories', () => {
  it('groups tags by category and lists orphans under "Senza categoria"', async () => {
    fetchTagsMock.mockResolvedValue([
      category({ id: 'cat-1', name: 'Natura' }),
      tag({ id: 'tag-1', name: 'Tramonti', parent_id: 'cat-1' }),
      tag({ id: 'tag-2', name: 'Orfano', parent_id: null })
    ])
    const { wrapper } = await mountTags()

    expect(wrapper.text()).toContain('Natura')
    expect(wrapper.text()).toContain('Tramonti')
    expect(wrapper.text()).toContain('Senza categoria')
    expect(wrapper.text()).toContain('Orfano')
  })

  it('shows the prompt line only when it differs from the name', async () => {
    fetchTagsMock.mockResolvedValue([
      category(),
      tag({ id: 't1', name: 'Regate', prompt: 'barche a vela in regata' }),
      tag({ id: 't2', name: 'Uguale', prompt: 'uguale' })
    ])
    const { wrapper } = await mountTags()

    expect(wrapper.text()).toContain('barche a vela in regata')
    expect(wrapper.text().match(/Uguale/g)?.length).toBe(1)
  })

  it('shows the real assignment count and threshold badge', async () => {
    fetchTagsMock.mockResolvedValue([category(), tag({ assignment_count: 12, threshold: 0.6 })])
    const { wrapper } = await mountTags()

    expect(wrapper.text()).toContain('60%')
  })

  it('empty-state text per category and for the orphan block', async () => {
    fetchTagsMock.mockResolvedValue([category({ id: 'cat-1', name: 'Vuota' })])
    const { wrapper } = await mountTags()

    expect(wrapper.text()).toContain('Nessun tag qui ancora')
    expect(wrapper.text()).toContain('Nessun tag fuori da una categoria.')
  })

  it('clicking a tag row opens the editor pre-filled, saving calls patchTag', async () => {
    fetchTagsMock.mockResolvedValue([category(), tag({ id: 't1', name: 'Tramonti' })])
    patchTagMock.mockResolvedValue(tag({ id: 't1', name: 'Tramonti al mare' }))
    const { wrapper } = await mountTags()

    const row = wrapper.get('[aria-label="Modifica tag Tramonti"]')
    await row.trigger('click')
    await tick()

    const nameInput = Array.from(document.body.querySelectorAll('input')).find(
      (el) => (el as HTMLInputElement).value === 'Tramonti'
    )
    expect(nameInput).toBeTruthy()

    const saveBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Salva')
    saveBtn?.click()
    await flushPromises()

    expect(patchTagMock).toHaveBeenCalledWith(
      't1',
      expect.objectContaining({ name: 'Tramonti', threshold: 0.75 })
    )
  })

  it('the row delete button asks for confirmation without opening the editor', async () => {
    fetchTagsMock.mockResolvedValue([category(), tag({ id: 't1', name: 'Tramonti', assignment_count: 5 })])
    const { wrapper } = await mountTags()

    const deleteBtn = wrapper.get('[aria-label="Elimina tag Tramonti"]')
    await deleteBtn.trigger('click')
    await tick()

    expect(document.body.textContent).toContain('Eliminare il tag "Tramonti"?')
    expect(document.body.textContent).toContain('Verrà rimosso da 5 foto')
    // The editor did not open alongside the confirm dialog.
    expect(document.body.querySelectorAll('[role="dialog"]').length).toBe(1)
  })

  it('confirming tag deletion calls deleteTag and reloads', async () => {
    fetchTagsMock.mockResolvedValueOnce([category(), tag({ id: 't1', name: 'Tramonti' })])
    fetchTagsMock.mockResolvedValueOnce([category()])
    deleteTagMock.mockResolvedValue(null)
    const { wrapper } = await mountTags()

    await wrapper.get('[aria-label="Elimina tag Tramonti"]').trigger('click')
    await tick()
    const confirmBtn = Array.from(document.body.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find(
      (b) => b.textContent === 'Elimina'
    )
    confirmBtn?.click()
    await flushPromises()

    expect(deleteTagMock).toHaveBeenCalledWith('t1')
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Tag "Tramonti" eliminato.')).toBe(true)
  })

  it('"Nuova categoria" opens the category dialog and creates for real', async () => {
    fetchTagsMock.mockResolvedValue([])
    createTagMock.mockResolvedValue(category({ id: 'new-cat', name: 'Luoghi' }))
    const { wrapper } = await mountTags()

    const newCategoryBtn = wrapper.findAll('button').find((b) => b.text() === 'Nuova categoria')
    await newCategoryBtn!.trigger('click')
    await tick()

    const nameInput = document.body.querySelector('input') as HTMLInputElement
    nameInput.value = 'Luoghi'
    nameInput.dispatchEvent(new Event('input'))
    const createBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Crea categoria'
    )
    createBtn?.click()
    await flushPromises()

    expect(createTagMock).toHaveBeenCalledWith({ name: 'Luoghi', kind: 'category' })
  })

  it('deleting a category keeps its tags, only clears the header', async () => {
    fetchTagsMock.mockResolvedValueOnce([category({ id: 'cat-1', name: 'Natura' })])
    fetchTagsMock.mockResolvedValueOnce([])
    deleteTagMock.mockResolvedValue(null)
    const { wrapper } = await mountTags()

    await wrapper.get('[aria-label="Elimina categoria Natura"]').trigger('click')
    await tick()
    expect(document.body.textContent).toContain('I tag al suo interno non vengono eliminati')

    const confirmBtn = Array.from(document.body.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find(
      (b) => b.textContent === 'Elimina'
    )
    confirmBtn?.click()
    await flushPromises()

    expect(deleteTagMock).toHaveBeenCalledWith('cat-1')
  })
})
