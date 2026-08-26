import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Tag } from '@/api/tags'
import { i18n } from '@/i18n'
import { useToastStore } from '@/stores/toast'

import TagEditorDialog from './TagEditorDialog.vue'

const createTagMock = vi.fn()
const patchTagMock = vi.fn()
const deleteTagMock = vi.fn()

vi.mock('@/api/tags', () => ({
  createTag: (...args: unknown[]) => createTagMock(...args),
  patchTag: (...args: unknown[]) => patchTagMock(...args),
  deleteTag: (...args: unknown[]) => deleteTagMock(...args)
}))

function tag(overrides: Partial<Tag> = {}): Tag {
  return {
    id: 't1',
    name: 'Tramonti',
    kind: 'tag',
    parent_id: null,
    color: 'hsl(24, 60%, 50%)',
    threshold: 0.75,
    prompt: undefined,
    assignment_count: 0,
    ...overrides
  }
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

let wrapper: VueWrapper | undefined

function mountHost(props: { tag: Tag | null; categories?: Tag[]; tagCount?: number }) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const Host = defineComponent({
    components: { TheDialog: TagEditorDialog },
    props: {
      initialTag: { type: Object as () => Tag | null, default: null },
      categories: { type: Array as () => Tag[], default: () => [] },
      tagCount: { type: Number, default: 0 }
    },
    setup(hostProps) {
      const open = ref(true)
      return { open, hostProps }
    },
    template: `<TheDialog v-model:open="open" :tag="hostProps.initialTag" :categories="hostProps.categories ?? []" :tag-count="hostProps.tagCount ?? 0" />`
  })
  wrapper = mount(Host, {
    props: { initialTag: props.tag, categories: props.categories, tagCount: props.tagCount },
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

function fillField(label: string, value: string) {
  const labelEl = Array.from(document.body.querySelectorAll('label')).find((l) => l.textContent?.startsWith(label))
  const input = labelEl?.parentElement?.querySelector('input')
  if (!input) throw new Error(`field not found: ${label}`)
  input.value = value
  input.dispatchEvent(new Event('input'))
}

describe('TagEditorDialog — §53 "Dialog modifica tag"', () => {
  it('creation mode: empty name shows the reactive error without closing', async () => {
    mountHost({ tag: null })
    await tick()

    const createBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Crea tag')
    createBtn?.click()
    await tick()

    expect(createTagMock).not.toHaveBeenCalled()
    expect(document.body.textContent).toContain('Dai un nome al tag prima di salvarlo.')
  })

  it('creates a tag with the threshold converted from percent to fraction', async () => {
    createTagMock.mockResolvedValue(tag())
    mountHost({ tag: null, tagCount: 0 })
    await tick()

    fillField('Nome', 'Tramonti')
    const createBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Crea tag')
    createBtn?.click()
    await tick()

    expect(createTagMock).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'Tramonti', kind: 'tag', threshold: 0.2 })
    )
    const toast = useToastStore()
    expect(toast.toasts.some((t) => t.message === 'Tag "Tramonti" creato.')).toBe(true)
  })

  it('edit mode pre-fills fields and shows the threshold percent from the fraction', async () => {
    mountHost({ tag: tag({ threshold: 0.18, prompt: 'un prompt' }) })
    await tick()

    expect(document.body.textContent).toContain('Soglia di confidenza — 18%')
    const promptInput = Array.from(document.body.querySelectorAll('input')).find(
      (el) => (el as HTMLInputElement).value === 'un prompt'
    )
    expect(promptInput).toBeTruthy()
  })

  it('a 409 name conflict shows a real error, unlike the mockup which allows duplicates', async () => {
    patchTagMock.mockRejectedValue(new Error('conflict'))
    mountHost({ tag: tag() })
    await tick()

    const saveBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Salva')
    saveBtn?.click()
    await tick()

    expect(document.body.textContent).toContain('Esiste già un tag con questo nome.')
  })

  it('"Elimina tag" closes the editor first, then opens the confirm dialog', async () => {
    mountHost({ tag: tag({ name: 'Tramonti', assignment_count: 4 }) })
    await tick()

    const deleteBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Elimina tag')
    deleteBtn?.click()
    await tick()

    // The editor's own fields are gone (dialog closed) but the confirm dialog is up.
    expect(document.body.querySelector('label')).toBeFalsy()
    expect(document.body.textContent).toContain('Eliminare il tag "Tramonti"?')
    expect(document.body.textContent).toContain('Verrà rimosso da 4 foto')
  })

  it('confirming deletion calls deleteTag and emits deleted', async () => {
    deleteTagMock.mockResolvedValue(null)
    mountHost({ tag: tag({ id: 't1', name: 'Tramonti' }) })
    await tick()

    const deleteBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Elimina tag')
    deleteBtn?.click()
    await tick()
    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Elimina tag')
    confirmBtn?.click()
    await tick()

    expect(deleteTagMock).toHaveBeenCalledWith('t1')
  })
})
