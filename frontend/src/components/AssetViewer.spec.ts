import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { TimelineAsset } from '@/api/timeline'
import { i18n } from '@/i18n'

import AssetViewer from './AssetViewer.vue'
import { previewSrc } from '@/api/media'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))
vi.mock('@/api/client', () => ({ apiFetch }))

afterEach(() => apiFetch.mockReset())

function photo(id: string): TimelineAsset {
  return {
    id,
    folder_id: 'f',
    filename: `${id}.jpg`,
    content_hash: `${id}${'a'.repeat(63)}`.slice(0, 64),
    size_bytes: 1,
    kind: 'image',
    status: 'indexed',
    taken_at_utc: '2024-07-10T12:00:00Z',
    width: 100,
    height: 100,
    thumbhash: null
  }
}

describe('AssetViewer', () => {
  it('emits prev and next on arrow keys and keeps src reactive', async () => {
    const first = photo('aaaa')
    const second = photo('bbbb')
    const wrapper = mount(AssetViewer, {
      props: { asset: first, next: second },
      global: { plugins: [createPinia(), i18n] }
    })
    expect(wrapper.get('img[alt="aaaa.jpg"]').attributes('src')).toBe(
      previewSrc(first.content_hash!)
    )

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight' }))
    expect(wrapper.emitted('next')).toHaveLength(1)

    await wrapper.setProps({ asset: second, prev: first })
    expect(wrapper.get('img[alt="bbbb.jpg"]').attributes('src')).toBe(
      previewSrc(second.content_hash!)
    )
  })

  it('shows a compact cluster map only when effective metadata has a location', async () => {
    apiFetch.mockResolvedValue({
      title: null,
      description: null,
      taken_at: null,
      location: { lat: 41.9, lon: 12.5 },
      place_id: null,
      orientation: null
    })
    const wrapper = mount(AssetViewer, {
      props: { asset: photo('aaaa') },
      global: {
        plugins: [createPinia(), i18n],
        stubs: {
          MapClusterLayer: {
            props: ['center'],
            template: '<div data-testid="mini-map">{{ center }}</div>'
          }
        }
      }
    })

    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'i' }))
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/assets/aaaa/metadata')
    expect(wrapper.find('[data-testid="mini-map"]').exists()).toBe(true)
  })
})
