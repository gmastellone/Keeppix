import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'
import { useCullingStore } from '@/stores/culling'
import type { TimelineAsset } from '@/api/timeline'

vi.mock('@/api/culling', () => ({
  setFlags: vi.fn(async () => null),
  deleteAsset: vi.fn(async () => null),
  fetchFlags: vi.fn(async () => ({ rating: null, pick: 'none', color_label: null })),
  unvotedFlags: { rating: null, pick: 'none', color_label: null }
}))

import CullingView from './CullingView.vue'

function photo(id: string, kind = 'image', filename = `${id}.jpg`): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename,
    content_hash: `${id}${'a'.repeat(63)}`.slice(0, 64),
    size_bytes: 1,
    kind,
    status: 'indexed',
    taken_at_utc: '2024-07-10T12:00:00Z',
    width: 100,
    height: 100,
    thumbhash: null
  }
}

async function mountCulling() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: { template: '<div />' } },
      { path: '/culling', component: CullingView }
    ]
  })
  await router.push('/culling')
  await router.isReady()
  const wrapper = mount(CullingView, { global: { plugins: [router, i18n] } })
  await flushPromises()
  return { router, wrapper }
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.clearAllMocks()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('CullingView keyboard shortcuts', () => {
  it('rating a photo with a digit key advances to the next one', async () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b')])

    const { wrapper } = await mountCulling()

    window.dispatchEvent(new KeyboardEvent('keydown', { key: '3' }))
    await flushPromises()

    expect(store.flagsFor('a').rating).toBe(3)
    expect(store.currentAsset?.id).toBe('b')
    wrapper.unmount()
  })

  it('does not fire shortcuts while the user is typing in a text field', async () => {
    const store = useCullingStore()
    store.start([photo('a'), photo('b')])

    const { wrapper } = await mountCulling()

    const input = document.createElement('input')
    document.body.appendChild(input)
    input.focus()

    input.dispatchEvent(new KeyboardEvent('keydown', { key: '3', bubbles: true }))
    await flushPromises()

    expect(store.flagsFor('a').rating).toBeNull()
    expect(store.currentAsset?.id).toBe('a')

    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', bubbles: true }))
    await flushPromises()
    expect(store.flagsFor('a').pick).toBe('none')

    wrapper.unmount()
  })

  it('ignores shortcuts typed into a textarea or a contenteditable element', async () => {
    const store = useCullingStore()
    store.start([photo('a')])
    const { wrapper } = await mountCulling()

    const textarea = document.createElement('textarea')
    document.body.appendChild(textarea)
    textarea.dispatchEvent(new KeyboardEvent('keydown', { key: 'p', bubbles: true }))
    await flushPromises()
    expect(store.flagsFor('a').pick).toBe('none')

    const editable = document.createElement('div')
    editable.setAttribute('contenteditable', 'true')
    document.body.appendChild(editable)
    editable.dispatchEvent(new KeyboardEvent('keydown', { key: 'x', bubbles: true }))
    await flushPromises()
    expect(store.flagsFor('a').pick).toBe('none')

    wrapper.unmount()
  })
})

describe('CullingView zoom', () => {
  it('loads the full derivative on z, not the RAW original', async () => {
    const store = useCullingStore()
    const raw = photo('a', 'raw_image', 'a.arw')
    store.start([raw])

    const { wrapper } = await mountCulling()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'z' }))
    await flushPromises()

    const img = wrapper.get('img')
    expect(img.attributes('src')).toBe(`/media/full/${raw.content_hash}`)
    expect(img.attributes('src')).not.toContain('/media/original/')
    wrapper.unmount()
  })

  it('preloads the full derivative for upcoming photos, never the RAW file', async () => {
    const loaded: string[] = []
    class FakeImage {
      set src(value: string) {
        loaded.push(value)
      }
    }
    vi.stubGlobal('Image', FakeImage)

    const store = useCullingStore()
    const first = photo('a', 'raw_image', 'a.arw')
    const second = photo('b', 'raw_image', 'b.arw')
    store.start([first, second])

    const { wrapper } = await mountCulling()
    await flushPromises()

    expect(loaded.some((src) => src.includes('/media/original/'))).toBe(false)
    expect(loaded).toContain(`/media/full/${second.content_hash}`)
    wrapper.unmount()
    vi.unstubAllGlobals()
  })
})
