import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'

import PlacePicker from './PlacePicker.vue'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))
vi.mock('@/api/client', () => ({ apiFetch }))

afterEach(() => apiFetch.mockReset())

describe('PlacePicker', () => {
  it('applies a place even when its offline map is unavailable', async () => {
    apiFetch.mockImplementation((path: string) => {
      if (path.startsWith('/api/v1/places/suggest')) {
        return Promise.resolve([
          {
            id: 1857910,
            name: 'Kyoto',
            ascii_name: 'Kyoto',
            country_code: 'JP',
            admin1: 'Kyoto',
            admin2: null,
            lat: 35.0116,
            lon: 135.7681,
            population: 1475000
          }
        ])
      }
      if (path === '/api/v1/metadata/batch') {
        return Promise.resolve({ batch_id: 'batch-1' })
      }
      throw new Error(`unexpected ${path}`)
    })

    const wrapper = mount(PlacePicker, {
      props: { assetIds: ['asset-1', 'asset-2'], availableRegionIds: [] },
      global: { plugins: [createPinia(), i18n] }
    })

    await wrapper.get('input[type="search"]').setValue('ky')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    await wrapper.get('[data-place-id="1857910"]').trigger('click')

    expect(wrapper.text()).toContain('Map unavailable for this area')
    await wrapper.get('[data-action="apply"]').trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/metadata/batch', {
      method: 'POST',
      body: JSON.stringify({
        asset_ids: ['asset-1', 'asset-2'],
        patch: {
          location: { lat: 35.0116, lon: 135.7681 },
          place_id: 1857910
        }
      })
    })
    expect(wrapper.emitted('applied')).toEqual([[expect.objectContaining({ name: 'Kyoto' })]])
  })

  it('can start the optional region download without applying the place', async () => {
    apiFetch.mockImplementation((path: string) => {
      if (path.startsWith('/api/v1/places/suggest')) {
        return Promise.resolve([
          {
            id: 1857910,
            name: 'Kyoto',
            ascii_name: 'Kyoto',
            country_code: 'JP',
            admin1: 'Kyoto',
            admin2: null,
            lat: 35.0116,
            lon: 135.7681,
            population: 1475000
          }
        ])
      }
      if (path === '/api/v1/map/regions') {
        return Promise.resolve({ id: 'JP', status: 'downloading' })
      }
      throw new Error(`unexpected ${path}`)
    })

    const wrapper = mount(PlacePicker, {
      props: { assetIds: ['asset-1'], availableRegionIds: [], canDownload: true },
      global: { plugins: [createPinia(), i18n] }
    })
    await wrapper.get('input[type="search"]').setValue('ky')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    await wrapper.get('[data-place-id="1857910"]').trigger('click')
    await wrapper.get('[data-action="download"]').trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith(
      '/api/v1/map/regions',
      expect.objectContaining({ method: 'POST' })
    )
    expect(apiFetch).not.toHaveBeenCalledWith(
      '/api/v1/metadata/batch',
      expect.anything()
    )
  })
})
