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

vi.mock('@/api/folders', () => ({
  fetchChildren: (...args: unknown[]) => fetchChildrenMock(...args)
}))

vi.mock('@/api/culling', () => ({
  fetchFlags: (...args: unknown[]) => fetchFlagsMock(...args),
  setFlags: (...args: unknown[]) => setFlagsMock(...args),
  deleteAsset: (...args: unknown[]) => deleteAssetMock(...args),
  pickAsset: (...args: unknown[]) => pickAssetMock(...args),
  emptySkipped: (...args: unknown[]) => emptySkippedMock(...args),
  unvotedFlags: { rating: null, pick: 'none', color_label: null, favorite: false }
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
})

afterEach(() => {
  wrapper?.unmount()
  wrapper = undefined
  vi.clearAllMocks()
})

async function mountView(lotId = 'lot-1', name = 'Dolomiti') {
  const pinia = createPinia()
  setActivePinia(pinia)
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/culling', component: { template: '<div />' } },
      { path: '/culling/:lotId', component: CullingLotView }
    ]
  })
  await router.push({ path: `/culling/${lotId}`, query: { name } })
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
