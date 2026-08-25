import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import type { AssetFlags } from '@/api/culling'
import type { FolderChildren } from '@/api/folders'
import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

import CullingLotView from './CullingLotView.vue'

const fetchChildrenMock = vi.fn()
const pickAssetMock = vi.fn()
const emptySkippedMock = vi.fn()
const fetchFlagsMock = vi.fn()
const setFlagsMock = vi.fn()
const deleteAssetMock = vi.fn()
const apiFetchMock = vi.fn()
const fetchCullingLotsMock = vi.fn()
const previewRenameMock = vi.fn()
const applyRenameBatchMock = vi.fn()

vi.mock('@/api/folders', () => ({
  fetchChildren: (...args: unknown[]) => fetchChildrenMock(...args)
}))

vi.mock('@/api/culling', () => ({
  fetchFlags: (...args: unknown[]) => fetchFlagsMock(...args),
  setFlags: (...args: unknown[]) => setFlagsMock(...args),
  deleteAsset: (...args: unknown[]) => deleteAssetMock(...args),
  pickAsset: (...args: unknown[]) => pickAssetMock(...args),
  emptySkipped: (...args: unknown[]) => emptySkippedMock(...args),
  fetchCullingLots: (...args: unknown[]) => fetchCullingLotsMock(...args),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
}))

vi.mock('@/api/rename', () => ({
  previewRename: (...args: unknown[]) => previewRenameMock(...args),
  applyRenameBatch: (...args: unknown[]) => applyRenameBatchMock(...args)
}))

const unvotedFlags: AssetFlags = { rating: null, pick: 'none', color_label: null, favorite: false }

vi.mock('@/api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/api/client')>()
  return { ...actual, apiFetch: (...args: unknown[]) => apiFetchMock(...args) }
})

function asset(overrides: Partial<TimelineAsset> = {}): TimelineAsset {
  return {
    id: 'a1',
    folder_id: 'lot-1',
    filename: 'IMG_0.jpg',
    content_hash: null,
    size_bytes: 100,
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
    faces: [],
    ...overrides
  } as TimelineAsset
}

function children(assets: TimelineAsset[], folders: FolderChildren['folders'] = []): FolderChildren {
  return { assets, folders }
}

let wrapper: VueWrapper | undefined

beforeEach(() => {
  i18n.global.locale.value = 'it'
  fetchChildrenMock.mockImplementation(async (id: string) => {
    if (id === 'lot-1') return children([asset({ id: 'a1' }), asset({ id: 'a2', filename: 'IMG_1.jpg' })])
    return children([])
  })
  fetchFlagsMock.mockResolvedValue({ ...unvotedFlags })
  setFlagsMock.mockResolvedValue(null)
  deleteAssetMock.mockResolvedValue(null)
  apiFetchMock.mockResolvedValue({})
  fetchCullingLotsMock.mockResolvedValue([])
  previewRenameMock.mockResolvedValue([])
  applyRenameBatchMock.mockResolvedValue({ operation_id: 'op1', outcome: { succeeded: [], failed: [], batch_id: null } })
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

async function mountView(lotId = 'lot-1', name = 'Dolomiti', library = 'lib-1') {
  const pinia = createPinia()
  setActivePinia(pinia)
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/culling', component: { template: '<div />' } },
      { path: '/culling/:lotId', component: CullingLotView }
    ]
  })
  await router.push({ path: `/culling/${lotId}`, query: { name, library } })
  await router.isReady()
  wrapper = mount(CullingLotView, { global: { plugins: [i18n, pinia, router] }, attachTo: document.body })
  await flushPromises()
  return { wrapper, router }
}

describe('CullingLotView — §15 lotto aperto', () => {
  it('composes the lot from fetchChildren and shows the real counters and stage', async () => {
    const { wrapper } = await mountView()

    expect(fetchChildrenMock).toHaveBeenCalledWith('lot-1')
    expect(wrapper.text()).toContain('Dolomiti')
    expect(wrapper.text()).toContain('2') // da vedere
    expect(wrapper.text()).toContain('IMG_0.jpg')
  })

  it('"Scelta" calls the real pick route and moves the asset to taken', async () => {
    pickAssetMock.mockResolvedValue(asset({ id: 'a1', folder_id: 'taken-1' }))
    const { wrapper } = await mountView()

    const pickBtn = wrapper.findAll('button').find((b) => b.text() === 'Scelta')!
    await pickBtn.trigger('click')
    await flushPromises()

    expect(pickAssetMock).toHaveBeenCalledWith('a1', 'pick')
  })

  it('clicking "Scelta" a second time on the same photo undoes it back to pending', async () => {
    fetchChildrenMock.mockImplementation(async (id: string) => {
      if (id === 'lot-1') return children([asset({ id: 'a1' })])
      if (id === 'taken-1') return children([])
      return children([])
    })
    pickAssetMock.mockResolvedValueOnce(asset({ id: 'a1', folder_id: 'taken-1' }))
    const { wrapper } = await mountView()

    const pickBtn = wrapper.findAll('button').find((b) => b.text() === 'Scelta')!
    await pickBtn.trigger('click')
    await flushPromises()
    pickAssetMock.mockResolvedValueOnce(asset({ id: 'a1', folder_id: 'lot-1' }))
    await pickBtn.trigger('click')
    await flushPromises()

    expect(pickAssetMock).toHaveBeenNthCalledWith(1, 'a1', 'pick')
    expect(pickAssetMock).toHaveBeenNthCalledWith(2, 'a1', 'none')
  })

  it('switching to the "Scartati" filter and back keeps the queue in sync', async () => {
    const { wrapper } = await mountView()

    const skippedChip = wrapper.findAll('button').find((b) => b.text() === 'Scartati')!
    await skippedChip.trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Niente da mostrare con questo filtro')
  })

  it('rating stars set the current photo\'s rating, toggling to 0 on repeat', async () => {
    const { wrapper } = await mountView()
    await flushPromises()

    const stars = wrapper.findAll('[aria-label]').filter((el) => el.attributes('aria-label')?.includes('stelle'))
    await stars[2].trigger('click') // 3 stelle
    await flushPromises()

    expect(setFlagsMock).toHaveBeenCalledWith('a1', expect.objectContaining({ rating: 3 }))
  })

  it('"Svuota scartati" asks for confirmation, then removes the purged assets for real', async () => {
    fetchChildrenMock.mockImplementation(async (id: string) => {
      if (id === 'lot-1') return children([asset({ id: 'a1' })], [{ id: 'skipped-1', library_id: 'lib-1', parent_id: 'lot-1', name: '_skipped', depth: 1 }])
      if (id === 'skipped-1') return children([asset({ id: 'a2', folder_id: 'skipped-1' })])
      return children([])
    })
    emptySkippedMock.mockResolvedValue({ succeeded: ['a2'], failed: [], batch_id: null })
    const { wrapper } = await mountView()

    const skippedChip = wrapper.findAll('button').find((b) => b.text() === 'Scartati')!
    await skippedChip.trigger('click')
    await flushPromises()

    const emptyBtn = wrapper.findAll('button').find((b) => b.text().includes('Svuota scartati'))!
    await emptyBtn.trigger('click')
    await flushPromises()

    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent?.includes('Svuota scartati') && b.closest('[role="dialog"]')
    )
    confirmBtn?.click()
    await flushPromises()

    expect(emptySkippedMock).toHaveBeenCalledWith('lot-1')
    expect(wrapper.text()).not.toContain('IMG_1.jpg')
  })

  it('keyboard shortcuts navigate and decide on the current photo', async () => {
    pickAssetMock.mockResolvedValue(asset({ id: 'a1', folder_id: 'taken-1' }))
    await mountView()
    await flushPromises()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight' }))
    await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft' }))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'p' }))
    await flushPromises()

    expect(pickAssetMock).toHaveBeenCalledWith('a1', 'pick')
  })

  it('keyboard shortcuts are inert while typing in a text field (Ruling)', async () => {
    document.body.innerHTML = '<input id="stray" type="text" />'
    const input = document.getElementById('stray') as HTMLInputElement
    input.focus()
    await mountView()
    await flushPromises()

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', bubbles: true, cancelable: true }))
    await flushPromises()

    expect(pickAssetMock).not.toHaveBeenCalled()
    input.remove()
  })

  it('the info button opens the lightbox on the current photo, in culling mode', async () => {
    const { wrapper } = await mountView()

    const infoBtn = wrapper.findAll('button').find((b) => b.attributes('aria-label') === 'Dettagli foto — EXIF, posizione, rinomina')!
    await infoBtn.trigger('click')
    await flushPromises()

    const viewer = wrapper.findComponent({ name: 'AssetViewer' })
    expect(viewer.exists()).toBe(true)
    expect(viewer.props('isCulling')).toBe(true)
  })

  it('shows the empty state when the lot has no photos', async () => {
    fetchChildrenMock.mockResolvedValue(children([]))
    const { wrapper } = await mountView()

    expect(wrapper.text()).toContain('Niente da mostrare con questo filtro')
  })
})

describe('CullingLotView — Task 17 (4/N) selezione multipla, rinomina, selettore rapido', () => {
  it('clicking a thumbnail checkbox enters selection mode and shows the selection bar', async () => {
    const { wrapper } = await mountView()

    await wrapper.findAll('[role="checkbox"]')[0].trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('1 selezionata')
    expect(wrapper.find('[role="toolbar"]').exists()).toBe(true)
    // Coi chip nascosti dalla barra di selezione, i filtri non compaiono più.
    expect(wrapper.findAll('button').find((b) => b.text() === 'Tutte')).toBeUndefined()
  })

  it('shift+click on a checkbox selects the whole range from the anchor', async () => {
    const { wrapper } = await mountView()

    const checkboxes = wrapper.findAll('[role="checkbox"]')
    await checkboxes[0].trigger('click')
    await checkboxes[1].trigger('click', { shiftKey: true })
    await flushPromises()

    expect(wrapper.text()).toContain('2 selezionate')
  })

  it('bulk "Scelta" in the selection bar forces every selected photo to taken and clears the selection', async () => {
    pickAssetMock.mockResolvedValue(asset({ id: 'a1', folder_id: 'taken-1' }))
    const { wrapper } = await mountView()

    const checkboxes = wrapper.findAll('[role="checkbox"]')
    await checkboxes[0].trigger('click')
    await checkboxes[1].trigger('click', { shiftKey: true })
    await flushPromises()

    const bulkPick = wrapper.findAll('button').find((b) => b.attributes('aria-label') === 'Scelta' && b.text() === '✓')!
    await bulkPick.trigger('click')
    await flushPromises()

    expect(pickAssetMock).toHaveBeenCalledWith('a1', 'pick')
    expect(pickAssetMock).toHaveBeenCalledWith('a2', 'pick')
    expect(wrapper.find('[role="toolbar"]').exists()).toBe(false)
  })

  it('shift+ArrowRight from the keyboard selects the range from the anchor to the new position', async () => {
    const { wrapper } = await mountView()
    await flushPromises()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', shiftKey: true }))
    await flushPromises()

    expect(wrapper.text()).toContain('2 selezionate')
  })

  it('a plain ArrowRight after a range selection clears it (Ruling: no Esc in this screen)', async () => {
    const { wrapper } = await mountView()
    await flushPromises()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', shiftKey: true }))
    await flushPromises()
    expect(wrapper.text()).toContain('2 selezionate')

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight' }))
    await flushPromises()

    expect(wrapper.find('[role="toolbar"]').exists()).toBe(false)
  })

  it('"Rinomina lotto…" opens the dialog scoped to pending photos only, with the subfolders toggle off by default', async () => {
    fetchChildrenMock.mockImplementation(async (id: string) => {
      if (id === 'lot-1') {
        return children([asset({ id: 'a1' })], [{ id: 'taken-1', library_id: 'lib-1', parent_id: 'lot-1', name: '_taken', depth: 1 }])
      }
      if (id === 'taken-1') return children([asset({ id: 'a2', folder_id: 'taken-1' })])
      return children([])
    })
    const { wrapper } = await mountView()

    const renameBtn = wrapper.findAll('button').find((b) => b.text() === 'Rinomina lotto…')!
    await renameBtn.trigger('click')
    await flushPromises()

    expect(previewRenameMock).toHaveBeenCalledWith(['a1'], '{data}_{luogo}_{n:3}')
    expect(document.body.querySelector('[role="switch"]')).not.toBeNull()
  })

  it('"Rinomina…" from the selection bar opens the dialog scoped to the selected photos and clears the selection once applied', async () => {
    previewRenameMock.mockResolvedValue([{ asset_id: 'a1', folder_id: 'lot-1', current_name: 'a.jpg', new_name: 'b.jpg', collides: false }])
    const { wrapper } = await mountView()

    await wrapper.findAll('[role="checkbox"]')[0].trigger('click')
    await flushPromises()
    const renameBtn = wrapper.findAll('button').find((b) => b.attributes('aria-label') === 'Rinomina…')!
    await renameBtn.trigger('click')
    await flushPromises()

    expect(previewRenameMock).toHaveBeenCalledWith(['a1'], '{data}_{luogo}_{n:3}')

    const applyBtn = Array.from(document.body.querySelectorAll('button')).find((b) => b.textContent?.trim() === 'Applica')!
    applyBtn.click()
    await flushPromises()

    expect(applyRenameBatchMock).toHaveBeenCalledWith(['a1'], '{data}_{luogo}_{n:3}')
    expect(wrapper.find('[role="toolbar"]').exists()).toBe(false)
  })

  it('the quick lot switcher lists the library\'s lots and navigates on click', async () => {
    fetchCullingLotsMock.mockResolvedValue([
      { folder_id: 'lot-1', name: 'Dolomiti', created_at: '2026-08-14T10:00:00Z', pending: 2, taken: 0, skipped: 0 },
      { folder_id: 'lot-2', name: 'Marina', created_at: '2026-08-15T10:00:00Z', pending: 5, taken: 0, skipped: 0 }
    ])
    const { wrapper, router } = await mountView()

    const trigger = wrapper.findAll('button').find((b) => b.text().includes('Dolomiti ⌄'))!
    await trigger.trigger('click')
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(fetchCullingLotsMock).toHaveBeenCalledWith('lib-1')
    expect(document.body.textContent).toContain('Marina')
    expect(document.body.textContent).toContain('5 da vedere')

    const marinaRow = Array.from(document.body.querySelectorAll('[role="option"]')).find((el) => el.textContent?.includes('Marina')) as HTMLElement
    marinaRow.click()
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/culling/lot-2')
    expect(router.currentRoute.value.query.name).toBe('Marina')
  })
})
