import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, h, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi, beforeEach } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'
import { useShellStore } from '@/stores/shell'

const fetchTagsMock = vi.fn()
const fetchPersonsMock = vi.fn()

vi.mock('@/api/tags', () => ({
  fetchTags: (...args: unknown[]) => fetchTagsMock(...args)
}))
vi.mock('@/api/persons', () => ({
  fetchPersons: (...args: unknown[]) => fetchPersonsMock(...args)
}))

const { useBrowseFilters } = await import('./useBrowseFilters')

function photo(over: Partial<TimelineAsset> & { id: string }): TimelineAsset {
  return {
    folder_id: 'f1',
    filename: `${over.id}.jpg`,
    content_hash: null,
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: null,
    width: 100,
    height: 100,
    thumbhash: null,
    raw_kind: 'jpeg',
    favorite: false,
    camera_model: null,
    tags: [],
    faces: [],
    ...over
  }
}

function mountHook(assets: TimelineAsset[]) {
  const assetsRef = ref(assets)
  let result: ReturnType<typeof useBrowseFilters> | undefined
  const Host = defineComponent({
    setup() {
      result = useBrowseFilters(assetsRef)
      return () => h('div')
    }
  })
  const wrapper = mount(Host, { global: { plugins: [i18n] } })
  return { wrapper, get hook() { return result! }, assetsRef }
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
  fetchTagsMock.mockResolvedValue([])
  fetchPersonsMock.mockResolvedValue([])
  i18n.global.locale.value = 'it'
})

describe('useBrowseFilters — dimensions built from real data', () => {
  it('the "Tipo" dimension always has the three fixed RAW+JPEG/RAW/JPEG options', async () => {
    const { hook } = mountHook([])
    await flushPromises()

    const type = hook.dimensions.value.find((d) => d.id === 'type')
    expect(type?.options.map((o) => o.value)).toEqual(['raw+jpeg', 'raw', 'jpeg'])
    expect(type?.options.map((o) => o.label)).toEqual(['RAW+JPEG', 'RAW', 'JPEG'])
  })

  it('"Persone" is entirely absent when no visible person has at least one face — §11.2', async () => {
    fetchPersonsMock.mockResolvedValue([
      { id: 'p1', name: 'Marta', hidden: false, face_count: 0 },
      { id: 'p2', name: 'Nascosta', hidden: true, face_count: 5 }
    ])
    const { hook } = mountHook([])
    await flushPromises()

    expect(hook.dimensions.value.some((d) => d.id === 'person')).toBe(false)
  })

  it('"Persone" appears once a visible person with at least one face exists', async () => {
    fetchPersonsMock.mockResolvedValue([{ id: 'p1', name: 'Marta', hidden: false, face_count: 3 }])
    const { hook } = mountHook([])
    await flushPromises()

    const person = hook.dimensions.value.find((d) => d.id === 'person')
    expect(person?.options).toEqual([{ value: 'p1', label: 'Marta' }])
  })

  it('"Tag" and "Categorie" split the same /tags list by kind', async () => {
    fetchTagsMock.mockResolvedValue([
      { id: 't1', name: 'Montagna', kind: 'tag', parent_id: 'c1', color: '#336699', assignment_count: 1 },
      { id: 'c1', name: 'Viaggi', kind: 'category', parent_id: null, color: null, assignment_count: 1 }
    ])
    const { hook } = mountHook([])
    await flushPromises()

    expect(hook.dimensions.value.find((d) => d.id === 'tag')?.options).toEqual([
      { value: 't1', label: 'Montagna', color: '#336699' }
    ])
    expect(hook.dimensions.value.find((d) => d.id === 'category')?.options).toEqual([
      { value: 'c1', label: 'Viaggi' }
    ])
  })

  it('"Fotocamera" lists the distinct camera_model values found among the loaded assets, sorted', () => {
    const { hook } = mountHook([
      photo({ id: 'a', camera_model: 'FUJIFILM X-T5' }),
      photo({ id: 'b', camera_model: 'Canon EOS R5' }),
      photo({ id: 'c', camera_model: 'FUJIFILM X-T5' }),
      photo({ id: 'd', camera_model: null })
    ])

    const camera = hook.dimensions.value.find((d) => d.id === 'camera')
    expect(camera?.options.map((o) => o.value)).toEqual(['Canon EOS R5', 'FUJIFILM X-T5'])
  })

  it('"Luogo" lists every folder from the shell store', () => {
    const shell = useShellStore()
    shell.folders = [
      { id: 'f1', library_id: 'l', parent_id: null, name: 'Urbino', depth: 0 },
      { id: 'f2', library_id: 'l', parent_id: null, name: 'Chioggia', depth: 0 }
    ]
    const { hook } = mountHook([])

    expect(hook.dimensions.value.find((d) => d.id === 'folder')?.options).toEqual([
      { value: 'f1', label: 'Urbino' },
      { value: 'f2', label: 'Chioggia' }
    ])
  })
})

describe('useBrowseFilters — matching (AND across dimensions, OR within one — §11.3)', () => {
  it('with no filter selected, every asset passes', () => {
    const assets = [photo({ id: 'a' }), photo({ id: 'b' })]
    const { hook } = mountHook(assets)
    expect(hook.filteredAssets.value).toEqual(assets)
  })

  it('Tag and Categorie combine as an AND, exactly the documented example (Tipo=RAW AND Persone=Marta AND Luogo=Urbino, adapted)', () => {
    const raw = photo({ id: 'raw-urbino', raw_kind: 'raw', folder_id: 'urbino' })
    const jpeg = photo({ id: 'jpeg-urbino', raw_kind: 'jpeg', folder_id: 'urbino' })
    const rawElsewhere = photo({ id: 'raw-other', raw_kind: 'raw', folder_id: 'other' })
    const { hook } = mountHook([raw, jpeg, rawElsewhere])

    hook.selection.value = { type: new Set(['raw']), folder: new Set(['urbino']) }

    expect(hook.filteredAssets.value).toEqual([raw])
  })

  it('a tag filter with no visible person picked still lets non-matching persons through — proves the empty "person" dimension never appears in matchDimensions unexpectedly', () => {
    const a = photo({ id: 'a', tags: [{ id: 't1', name: 'Montagna', color: null, category_id: null }] })
    const b = photo({ id: 'b', tags: [] })
    const { hook } = mountHook([a, b])

    hook.selection.value = { tag: new Set(['t1']) }

    expect(hook.filteredAssets.value).toEqual([a])
  })
})
