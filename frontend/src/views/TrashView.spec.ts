import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'
import type { TrashedItem } from '@/api/trash'
import { i18n } from '@/i18n'
import ErrorState from '@/components/ui/ErrorState.vue'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import TrashView from './TrashView.vue'

const fetchTrashMock = vi.fn()
const restoreAssetMock = vi.fn()
const emptyTrashMock = vi.fn()
const deleteAssetMock = vi.fn()
const fetchAssetMock = vi.fn()

vi.mock('@/api/trash', () => ({
  fetchTrash: (...args: unknown[]) => fetchTrashMock(...args),
  restoreAsset: (...args: unknown[]) => restoreAssetMock(...args),
  emptyTrash: (...args: unknown[]) => emptyTrashMock(...args)
}))

vi.mock('@/api/culling', () => ({
  deleteAsset: (...args: unknown[]) => deleteAssetMock(...args)
}))

vi.mock('@/api/timeline', () => ({
  fetchAsset: (...args: unknown[]) => fetchAssetMock(...args)
}))

const testUser = {
  id: '1',
  username: 'admin',
  display_name: 'Admin',
  email: null,
  role: 'admin' as const,
  locale: null
}

function trashItem(overrides: Partial<TrashedItem> = {}): TrashedItem {
  return {
    id: 'entry-1',
    asset_id: 'a1',
    deleted_at: '2026-08-01T00:00:00Z',
    original_path: '/lib/Urbino/photo.jpg',
    disk_action: 'moved_to_trash',
    days_remaining: 12,
    ...overrides
  }
}

function asset(id: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: 'ab'.repeat(32),
    size_bytes: 1,
    kind: 'image',
    status: 'trashed',
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

beforeEach(() => {
  i18n.global.locale.value = 'it'
  setActivePinia(createPinia())
  const session = useSessionStore()
  session.user = testUser
  session.initialised = true
  session.ready = true

  fetchTrashMock.mockResolvedValue({ items: [] })
  fetchAssetMock.mockImplementation(async (id: string) => asset(id))
  restoreAssetMock.mockResolvedValue(null)
  emptyTrashMock.mockResolvedValue({ emptied: 0 })
  deleteAssetMock.mockResolvedValue(null)
})

afterEach(() => {
  vi.clearAllMocks()
})

async function mountTrash() {
  const wrapper = mount(TrashView, { global: { plugins: [i18n] } })
  await flushPromises()
  return wrapper
}

describe('TrashView — §45 Cestino', () => {
  it('shows the documented empty state when there is nothing in trash', async () => {
    const wrapper = await mountTrash()

    expect(wrapper.text()).toContain('Il cestino è vuoto')
    expect(wrapper.find('button').exists()).toBe(false)
  })

  it('renders a real thumbnail per item, via fetchAsset — not a fake gradient', async () => {
    fetchTrashMock.mockResolvedValue({ items: [trashItem()] })

    const wrapper = await mountTrash()

    expect(fetchAssetMock).toHaveBeenCalledWith('a1')
    const img = wrapper.find('img')
    expect(img.exists()).toBe(true)
    expect(img.attributes('src')).toContain(asset('a1').content_hash)
  })

  it('shows the real days-remaining badge from the backend, singular/plural', async () => {
    fetchTrashMock.mockResolvedValue({
      items: [trashItem({ id: 'e1', asset_id: 'a1', days_remaining: 1 }), trashItem({ id: 'e2', asset_id: 'a2', days_remaining: 12 })]
    })

    const wrapper = await mountTrash()

    expect(wrapper.text()).toContain('1 giorno rimanente')
    expect(wrapper.text()).toContain('12 giorni rimanenti')
  })

  it('follows pagination via next_cursor to collect the full trash', async () => {
    fetchTrashMock
      .mockResolvedValueOnce({ items: [trashItem({ id: 'e1', asset_id: 'a1' })], next_cursor: 'c1' })
      .mockResolvedValueOnce({ items: [trashItem({ id: 'e2', asset_id: 'a2' })] })

    const wrapper = await mountTrash()

    expect(fetchTrashMock).toHaveBeenNthCalledWith(1, undefined)
    expect(fetchTrashMock).toHaveBeenNthCalledWith(2, 'c1')
    expect(wrapper.findAll('img')).toHaveLength(2)
  })

  it('restore calls restoreAsset with the ASSET id (not the trash entry id) and removes the tile', async () => {
    fetchTrashMock.mockResolvedValue({ items: [trashItem({ id: 'entry-1', asset_id: 'asset-9' })] })
    const wrapper = await mountTrash()

    await wrapper.get('[aria-label="Ripristina"]').trigger('click')
    await flushPromises()

    expect(restoreAssetMock).toHaveBeenCalledWith('asset-9')
    expect(wrapper.find('img').exists()).toBe(false)
    expect(wrapper.text()).toContain('Il cestino è vuoto')
  })

  it('"Elimina definitivamente" calls deleteAsset(id, "purged") and removes the tile, no confirmation dialog', async () => {
    fetchTrashMock.mockResolvedValue({ items: [trashItem({ asset_id: 'asset-9' })] })
    const wrapper = await mountTrash()

    await wrapper.get('[aria-label="Elimina definitivamente"]').trigger('click')
    await flushPromises()

    expect(deleteAssetMock).toHaveBeenCalledWith('asset-9', 'purged')
    expect(wrapper.text()).toContain('Il cestino è vuoto')
  })

  it('"Svuota cestino" empties instantly, no dialog', async () => {
    fetchTrashMock.mockResolvedValue({ items: [trashItem(), trashItem({ id: 'e2', asset_id: 'a2' })] })
    const wrapper = await mountTrash()

    await wrapper.get('button').trigger('click')
    await flushPromises()

    expect(emptyTrashMock).toHaveBeenCalled()
    expect(wrapper.text()).toContain('Il cestino è vuoto')
  })

  it('shows a full-view ErrorState on load failure', async () => {
    fetchTrashMock.mockRejectedValue(new Error('boom'))

    const wrapper = await mountTrash()

    expect(wrapper.findComponent(ErrorState).exists()).toBe(true)
  })

  it('a failed restore surfaces an error toast (real network failure, unlike the demo mockup)', async () => {
    fetchTrashMock.mockResolvedValue({ items: [trashItem()] })
    restoreAssetMock.mockRejectedValue(new Error('conflict'))
    const wrapper = await mountTrash()

    await wrapper.get('[aria-label="Ripristina"]').trigger('click')
    await flushPromises()

    const toast = useToastStore()
    expect(toast.toasts.some((entry) => entry.kind === 'error')).toBe(true)
  })
})
