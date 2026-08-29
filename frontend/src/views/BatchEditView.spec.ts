import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import type { AssetFlags } from '@/api/culling'
import type { TimelineAsset } from '@/api/timeline'
import AlbumPickerDialog from '@/components/AlbumPickerDialog.vue'
import RenameFormulaDialog from '@/components/RenameFormulaDialog.vue'
import TagPickerDialog from '@/components/TagPickerDialog.vue'
import { i18n } from '@/i18n'
import { useSelectionStore } from '@/stores/selection'
import { useShellStore } from '@/stores/shell'

import BatchEditView from './BatchEditView.vue'

const moveAssetsBatchMock = vi.fn()
const applyMetadataBatchMock = vi.fn()
const fetchFlagsMock = vi.fn()
const setFlagsMock = vi.fn()

vi.mock('@/api/assets', () => ({
  moveAssetsBatch: (...args: unknown[]) => moveAssetsBatchMock(...args)
}))
vi.mock('@/api/metadata', () => ({
  applyMetadataBatch: (...args: unknown[]) => applyMetadataBatchMock(...args)
}))
vi.mock('@/api/culling', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/culling')>()
  return {
    ...actual,
    fetchFlags: (...args: unknown[]) => fetchFlagsMock(...args),
    setFlags: (...args: unknown[]) => setFlagsMock(...args)
  }
})
// AlbumPickerDialog/TagPickerDialog/RenameFormulaDialog are always mounted
// (closed until the button that opens them is clicked) — their own calls
// already have a `.catch(() => [])`, but only if the real functions
// exist: without these mocks they would hit the real `apiFetch` (mocked
// above to return nothing) and receive `null` instead of a rejected
// `Promise`, which `.catch` would not intercept. The behavior of these
// three dialogs is already thoroughly verified in their own specs — mere
// harmless responses are enough here.
vi.mock('@/api/albums', () => ({
  fetchAlbums: vi.fn(async () => []),
  fetchAlbum: vi.fn(async () => ({ id: 'x', name: '', assets: [] }))
}))
vi.mock('@/api/tags', () => ({
  fetchTags: vi.fn(async () => []),
  assignTagBatch: vi.fn(async () => null),
  unassignTagBatch: vi.fn(async () => null)
}))
vi.mock('@/api/rename', () => ({
  previewRename: vi.fn(async () => []),
  applyRenameBatch: vi.fn(async () => ({ operation_id: 'op' }))
}))
vi.mock('@/api/operations', () => ({
  cancelOperation: vi.fn(async () => ({ succeeded: [], failed: [], batch_id: null }))
}))
// RenameFormulaDialog follows real progress over the WebSocket — none of
// these tests exercise that, the mock exists only to avoid attempting a
// real connection in jsdom.
vi.mock('@/api/events', () => ({
  startLiveEvents: vi.fn(() => ({ close: vi.fn() }))
}))

vi.mock('@/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})
const { apiFetch } = await import('@/api/client')

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

const unvoted: AssetFlags = { rating: null, pick: 'none', color_label: null, favorite: false }

async function mountBatchEdit(ids: string[]) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/batch-edit', component: BatchEditView },
      { path: '/', component: { template: '<div />' } }
    ]
  })
  setActivePinia(createPinia())
  const selection = useSelectionStore()
  const shell = useShellStore()
  shell.loaded = true
  shell.folders = [
    { id: 'f1', library_id: 'l', parent_id: null, name: 'Urbino', depth: 0 },
    { id: 'f2', library_id: 'l', parent_id: null, name: 'Chioggia', depth: 0 }
  ]

  await router.push(`/batch-edit${ids.length > 0 ? `?ids=${ids.join(',')}` : ''}`)
  await router.isReady()
  const wrapper = mount(BatchEditView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, wrapper, selection }
}

function buttonWithText(root: ReturnType<typeof mount>, text: string) {
  return root.findAll('button').find((b) => b.text().trim() === text)
}

/** The five star radios have no visible text distinguishing them (only
 * `aria-label`) — they are always the first five `[role=radio]` elements
 * in source order (the same tab order). */
function starRadios(root: ReturnType<typeof mount>) {
  return root.findAll('[role="radio"]').slice(0, 5)
}

/** Pick/Reject and Favorites (`SegmentedControl`) instead have a text
 * label per option — more robust than a fixed index, which would depend
 * on the exact order of sections in the template. */
function radioByText(root: ReturnType<typeof mount>, text: string) {
  return root.findAll('[role="radio"]').find((r) => r.text().trim() === text)
}

beforeEach(() => {
  vi.clearAllMocks()
  i18n.global.locale.value = 'it'
  vi.mocked(apiFetch).mockImplementation(async (url: string) => {
    const match = /^\/api\/v1\/assets\/([^/]+)$/.exec(url)
    return match ? photo(match[1]) : null
  })
  fetchFlagsMock.mockResolvedValue(unvoted)
  setFlagsMock.mockResolvedValue(null)
  moveAssetsBatchMock.mockResolvedValue(null)
  applyMetadataBatchMock.mockResolvedValue({ batch_id: 'b1' })
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('BatchEditView — empty state', () => {
  it('shows the empty state and nothing else when arriving with no selection', async () => {
    const { wrapper } = await mountBatchEdit([])

    expect(wrapper.text()).toContain(String(i18n.global.t('batchEdit.emptyTitle')))
    expect(wrapper.text()).not.toContain(String(i18n.global.t('batchEdit.title')))
    expect(wrapper.find('[role="radiogroup"]').exists()).toBe(false)
  })
})

describe('BatchEditView — loaded', () => {
  it('loads each asset by id and shows the count in the subtitle and preview strip', async () => {
    const { wrapper } = await mountBatchEdit(['a', 'b'])

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/a')
    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/b')
    expect(wrapper.text()).toContain('2 foto selezionate')
    expect(wrapper.findAll('img')).toHaveLength(2)
  })

  it('"Non modificare" is always the starting choice: no star checked, but the segmented controls\' "Non modificare" option is', async () => {
    const { wrapper } = await mountBatchEdit(['a'])

    starRadios(wrapper).forEach((star) => expect(star.attributes('aria-checked')).toBe('false'))
    expect(radioByText(wrapper, 'Pick')?.attributes('aria-checked')).toBe('false')
    expect(radioByText(wrapper, 'Aggiungi')?.attributes('aria-checked')).toBe('false')
    // "Non modificare" appears twice (Pick/Discard and Favorites): both start active.
    wrapper
      .findAll('[role="radio"]')
      .filter((r) => r.text().trim() === 'Non modificare')
      .forEach((r) => expect(r.attributes('aria-checked')).toBe('true'))
  })

  it('clicking the same star twice sets it back to "unchanged" (0) — only the exact value is aria-checked', async () => {
    const { wrapper } = await mountBatchEdit(['a'])
    const stars = starRadios(wrapper)

    await stars[2]?.trigger('click')
    expect(stars[2]?.attributes('aria-checked')).toBe('true')
    expect(stars[0]?.attributes('aria-checked')).toBe('false')

    await stars[2]?.trigger('click')
    expect(stars[2]?.attributes('aria-checked')).toBe('false')
  })

  it('opens AlbumPickerDialog/TagPickerDialog/RenameFormulaDialog bound to the loaded assets', async () => {
    const { wrapper } = await mountBatchEdit(['a', 'b'])

    expect(wrapper.findComponent(AlbumPickerDialog).props('open')).toBe(false)
    await buttonWithText(wrapper, 'Scegli album…')?.trigger('click')
    expect(wrapper.findComponent(AlbumPickerDialog).props('open')).toBe(true)
    expect(wrapper.findComponent(AlbumPickerDialog).props('assets').map((a: TimelineAsset) => a.id)).toEqual(['a', 'b'])

    await buttonWithText(wrapper, 'Aggiungi tag…')?.trigger('click')
    expect(wrapper.findComponent(TagPickerDialog).props('open')).toBe(true)

    await buttonWithText(wrapper, 'Rinomina con formula…')?.trigger('click')
    expect(wrapper.findComponent(RenameFormulaDialog).props('open')).toBe(true)
  })
})

describe('BatchEditView — "Applica"', () => {
  it('touches nothing when every field is left "Non modificare": no flags/metadata/move calls, but still clears the selection and navigates back', async () => {
    const { wrapper, router, selection } = await mountBatchEdit(['a'])
    selection.library.toggle('a')

    await buttonWithText(wrapper, 'Applica a 1 foto')?.trigger('click')
    await flushPromises()

    expect(setFlagsMock).not.toHaveBeenCalled()
    expect(applyMetadataBatchMock).not.toHaveBeenCalled()
    expect(moveAssetsBatchMock).not.toHaveBeenCalled()
    expect(selection.library.selectedIds.has('a')).toBe(false)
    expect(router.currentRoute.value.path).toBe('/')
  })

  it('rating>0 merges onto each asset\'s own current flags — untouched fields (pick/favorite) survive per-asset', async () => {
    fetchFlagsMock.mockImplementation(async (id: string) =>
      id === 'a' ? { rating: null, pick: 'pick', color_label: null, favorite: true } : unvoted
    )
    const { wrapper } = await mountBatchEdit(['a', 'b'])
    const stars = wrapper.findAll('[role="radio"]').slice(0, 5)
    await stars[3]?.trigger('click') // 4 stelle

    await buttonWithText(wrapper, 'Applica a 2 foto')?.trigger('click')
    await flushPromises()

    expect(setFlagsMock).toHaveBeenCalledWith('a', { rating: 4, pick: 'pick', color_label: null, favorite: true })
    expect(setFlagsMock).toHaveBeenCalledWith('b', { rating: 4, pick: 'none', color_label: null, favorite: false })
  })

  it('Pick/Scarta and Preferiti write the chosen value on top of each asset\'s current flags', async () => {
    const { wrapper } = await mountBatchEdit(['a'])
    await radioByText(wrapper, 'Scarta')?.trigger('click')
    await radioByText(wrapper, 'Aggiungi')?.trigger('click')

    await buttonWithText(wrapper, 'Applica a 1 foto')?.trigger('click')
    await flushPromises()

    expect(setFlagsMock).toHaveBeenCalledWith('a', { rating: null, pick: 'reject', color_label: null, favorite: true })
  })

  it('the title field is applied only when non-empty after trimming', async () => {
    const { wrapper } = await mountBatchEdit(['a'])
    await wrapper.find('input[type="text"]').setValue('  Tramonto  ')

    await buttonWithText(wrapper, 'Applica a 1 foto')?.trigger('click')
    await flushPromises()

    expect(applyMetadataBatchMock).toHaveBeenCalledWith(['a'], { title: 'Tramonto' })
  })

  it('the folder move is applied only when a real folder is chosen, not "Non modificare"', async () => {
    const { wrapper } = await mountBatchEdit(['a', 'b'])
    await wrapper.find('select').setValue('f2')

    await buttonWithText(wrapper, 'Applica a 2 foto')?.trigger('click')
    await flushPromises()

    expect(moveAssetsBatchMock).toHaveBeenCalledWith(['a', 'b'], 'f2')
  })
})

describe('BatchEditView — "Annulla"', () => {
  it('navigates back without touching flags/metadata/move and without clearing the selection', async () => {
    const { wrapper, router, selection } = await mountBatchEdit(['a'])
    selection.library.toggle('a')

    await buttonWithText(wrapper, 'Annulla')?.trigger('click')
    await flushPromises()

    expect(setFlagsMock).not.toHaveBeenCalled()
    expect(selection.library.selectedIds.has('a')).toBe(true)
    expect(router.currentRoute.value.path).toBe('/')
  })
})
