import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Tag } from '@/api/tags'
import { i18n } from '@/i18n'
import { useToastStore } from '@/stores/toast'

import CategoryEditorDialog from './CategoryEditorDialog.vue'

const createTagMock = vi.fn()
const patchTagMock = vi.fn()

vi.mock('@/api/tags', () => ({
  createTag: (...args: unknown[]) => createTagMock(...args),
  patchTag: (...args: unknown[]) => patchTagMock(...args)
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

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let wrapper: VueWrapper | undefined

function mountHost(cat: Tag | null) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const Host = defineComponent({
    components: { TheDialog: CategoryEditorDialog },
    props: {
      initialCategory: { type: Object as () => Tag | null, default: null }
    },
    setup(hostProps) {
      const open = ref(true)
      return { open, hostProps }
    },
    template: `<TheDialog v-model:open="open" :category="hostProps.initialCategory" />`
  })
  wrapper = mount(Host, {
    props: { initialCategory: cat },
    global: { plugins: [i18n, pinia] },
    attachTo: document.body
  })
  return wrapper
}

beforeEach(() => {
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

describe('CategoryEditorDialog', () => {
  it('rename mode pre-fills the current name, unlike the title which omits it', async () => {
    mountHost(category({ name: 'Natura' }))
    await tick()

    const input = document.body.querySelector('input') as HTMLInputElement
    expect(input.value).toBe('Natura')
    expect(document.body.textContent).toContain('Rinomina categoria')
    expect(document.body.textContent).not.toContain('Rinomina categoria Natura')
  })

  it('empty name shows the reactive error without calling the API', async () => {
    mountHost(null)
    await tick()

    const createBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Crea categoria'
    )
    createBtn?.click()
    await tick()

    expect(createTagMock).not.toHaveBeenCalled()
    expect(document.body.textContent).toContain('Dai un nome alla categoria prima di salvarla.')
  })

  it('creates a category for real', async () => {
    createTagMock.mockResolvedValue(category({ name: 'Luoghi' }))
    mountHost(null)
    await tick()

    const input = document.body.querySelector('input') as HTMLInputElement
    input.value = 'Luoghi'
    input.dispatchEvent(new Event('input'))
    const createBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Crea categoria'
    )
    createBtn?.click()
    await tick()

    expect(createTagMock).toHaveBeenCalledWith({ name: 'Luoghi', kind: 'category' })
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Categoria "Luoghi" creata.')).toBe(true)
  })

  it('renames a category for real via patchTag', async () => {
    patchTagMock.mockResolvedValue(category({ id: 'cat-1', name: 'Paesaggi' }))
    mountHost(category({ id: 'cat-1', name: 'Natura' }))
    await tick()

    const input = document.body.querySelector('input') as HTMLInputElement
    input.value = 'Paesaggi'
    input.dispatchEvent(new Event('input'))
    const saveBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Salva')
    saveBtn?.click()
    await tick()

    expect(patchTagMock).toHaveBeenCalledWith('cat-1', { name: 'Paesaggi' })
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Categoria rinominata in "Paesaggi".')).toBe(true)
  })

  it('a name conflict shows a real error', async () => {
    createTagMock.mockRejectedValue(new Error('conflict'))
    mountHost(null)
    await tick()

    const input = document.body.querySelector('input') as HTMLInputElement
    input.value = 'Natura'
    input.dispatchEvent(new Event('input'))
    const createBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Crea categoria'
    )
    createBtn?.click()
    await tick()

    expect(document.body.textContent).toContain('Esiste già una categoria con questo nome.')
  })
})
