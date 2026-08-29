import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Face } from '@/api/faces'
import type { Person } from '@/api/persons'
import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

const fetchPersonFaceTilesMock = vi.fn()
const separatePersonMock = vi.fn()

vi.mock('@/api/faces', () => ({
  fetchPersonFaceTiles: (...args: unknown[]) => fetchPersonFaceTilesMock(...args)
}))

vi.mock('@/api/persons', () => ({
  separatePerson: (...args: unknown[]) => separatePersonMock(...args)
}))

const SplitPersonDialog = (await import('./SplitPersonDialog.vue')).default

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function person(overrides: Partial<Person> = {}): Person {
  return { id: 'p1', name: 'Chiara', hidden: false, face_count: 3, ...overrides }
}

function asset(id: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: `${id}hash`,
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: null,
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

function face(id: string, assetId: string): Face {
  return { id, asset_id: assetId, bbox: { x: 0, y: 0, w: 1, h: 1 }, person_id: 'p1', proposed_person_id: null, proposed_score: null, assigned_by_human: true }
}

function tilesFor(n: number) {
  return Array.from({ length: n }, (_, i) => ({ asset: asset(`a${i}`), face: face(`f${i}`, `a${i}`) }))
}

let wrapper: VueWrapper | undefined

function mountHost(personProp: Person, assets: TimelineAsset[]) {
  const Host = defineComponent({
    components: { TheDialog: SplitPersonDialog },
    emits: ['split'],
    setup() {
      const open = ref(true)
      return { open, personProp, assets }
    },
    methods: { onSplit() { this.$emit('split') } },
    template: `<TheDialog v-model:open="open" :person="personProp" :assets="assets" @split="onSplit" />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
}

function tilesInDom(): HTMLButtonElement[] {
  return Array.from(document.body.querySelectorAll('[role="checkbox"]'))
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  i18n.global.locale.value = 'it'
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
})

describe('SplitPersonDialog', () => {
  it('opens with zero faces preselected — no AI sub-cluster signal exists on the real backend', async () => {
    fetchPersonFaceTilesMock.mockResolvedValue(tilesFor(3))
    mountHost(person(), [asset('a0'), asset('a1'), asset('a2')])
    await tick()

    expect(tilesInDom()).toHaveLength(3)
    tilesInDom().forEach((t) => expect(t.getAttribute('aria-checked')).toBe('false'))
  })

  it('the confirm button is disabled with zero selected', async () => {
    fetchPersonFaceTilesMock.mockResolvedValue(tilesFor(3))
    mountHost(person(), [asset('a0'), asset('a1'), asset('a2')])
    await tick()

    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Dividi in una nuova persona'
    ) as HTMLButtonElement
    expect(confirmBtn.disabled).toBe(true)
  })

  it('shows the "cannot extract all" warning and disables confirm when every face is selected', async () => {
    fetchPersonFaceTilesMock.mockResolvedValue(tilesFor(2))
    mountHost(person(), [asset('a0'), asset('a1')])
    await tick()

    tilesInDom().forEach((t) => t.click())
    await tick()

    expect(document.body.textContent).toContain("Non puoi estrarli tutti")
    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Dividi in una nuova persona'
    ) as HTMLButtonElement
    expect(confirmBtn.disabled).toBe(true)
  })

  it('selecting some (not all) faces enables confirm; confirming calls separatePerson with the typed name', async () => {
    fetchPersonFaceTilesMock.mockResolvedValue(tilesFor(3))
    separatePersonMock.mockResolvedValue(person({ id: 'new-1', name: 'Nuova' }))
    const w = mountHost(person(), [asset('a0'), asset('a1'), asset('a2')])
    await tick()

    tilesInDom()[0].click()
    await tick()

    const input = document.body.querySelector('input') as HTMLInputElement
    input.value = 'Nuova persona'
    input.dispatchEvent(new Event('input'))

    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent === 'Dividi in una nuova persona'
    ) as HTMLButtonElement
    expect(confirmBtn.disabled).toBe(false)
    confirmBtn.click()
    await tick()

    expect(separatePersonMock).toHaveBeenCalledWith('p1', ['f0'], 'Nuova persona')
    expect(w.emitted('split')).toBeTruthy()
  })
})
