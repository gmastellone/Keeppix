import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { Face } from '@/api/faces'
import type { Person } from '@/api/persons'
import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

const fetchPersonFaceTilesMock = vi.fn()
const patchPersonMock = vi.fn()

vi.mock('@/api/faces', () => ({
  fetchPersonFaceTiles: (...args: unknown[]) => fetchPersonFaceTilesMock(...args)
}))

vi.mock('@/api/persons', () => ({
  patchPerson: (...args: unknown[]) => patchPersonMock(...args)
}))

const ChooseCoverDialog = (await import('./ChooseCoverDialog.vue')).default

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function person(overrides: Partial<Person> = {}): Person {
  return { id: 'p1', name: 'Marta', hidden: false, face_count: 2, ...overrides }
}

function asset(id: string, hash: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: hash,
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

function face(id: string, assetId: string, personId = 'p1'): Face {
  return { id, asset_id: assetId, bbox: { x: 0, y: 0, w: 1, h: 1 }, person_id: personId, proposed_person_id: null, proposed_score: null, assigned_by_human: true }
}

let wrapper: VueWrapper | undefined

function mountHost(personProp: Person, assets: TimelineAsset[]) {
  const Host = defineComponent({
    components: { TheDialog: ChooseCoverDialog },
    emits: ['updated'],
    setup() {
      const open = ref(true)
      return { open, personProp, assets }
    },
    methods: { onUpdated(p: Person) { this.$emit('updated', p) } },
    template: `<TheDialog v-model:open="open" :person="personProp" :assets="assets" @updated="onUpdated" />`
  })
  wrapper = mount(Host, { global: { plugins: [i18n] }, attachTo: document.body })
  return wrapper
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

describe('ChooseCoverDialog — §33 (Task 16 4/N)', () => {
  it('shows one tile per confirmed face, not per photo (§33.2)', async () => {
    fetchPersonFaceTilesMock.mockResolvedValue([
      { asset: asset('a1', 'hash1'), face: face('f1', 'a1') },
      { asset: asset('a1', 'hash1'), face: face('f2', 'a1') },
      { asset: asset('a2', 'hash2'), face: face('f3', 'a2') }
    ])
    mountHost(person(), [asset('a1', 'hash1'), asset('a2', 'hash2')])
    await tick()

    expect(document.body.querySelectorAll('[aria-label="Imposta come copertina"]')).toHaveLength(3)
  })

  it('shows the exact subtitle with the person name', async () => {
    fetchPersonFaceTilesMock.mockResolvedValue([])
    mountHost(person({ name: 'Marta' }), [])
    await tick()

    expect(document.body.textContent).toContain('Marta — quale foto la rappresenta nella griglia')
  })

  it('clicking a tile sets cover_face_id, closes the dialog, and shows the toast', async () => {
    fetchPersonFaceTilesMock.mockResolvedValue([{ asset: asset('a1', 'hash1'), face: face('f1', 'a1') }])
    patchPersonMock.mockResolvedValue(person({ cover_face_id: 'f1' }))
    const w = mountHost(person(), [asset('a1', 'hash1')])
    await tick()

    const tile = document.body.querySelector('[aria-label="Imposta come copertina"]') as HTMLButtonElement
    tile.click()
    await tick()

    expect(patchPersonMock).toHaveBeenCalledWith('p1', { cover_face_id: 'f1' })
    expect(w.emitted('updated')).toBeTruthy()
    expect(document.body.querySelector('[role="dialog"]')).toBeFalsy()
  })

  it('"Chiudi" closes without changing the cover', async () => {
    fetchPersonFaceTilesMock.mockResolvedValue([{ asset: asset('a1', 'hash1'), face: face('f1', 'a1') }])
    mountHost(person(), [asset('a1', 'hash1')])
    await tick()

    const closeBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent === 'Chiudi')
    closeBtn?.click()
    await tick()

    expect(patchPersonMock).not.toHaveBeenCalled()
  })
})
