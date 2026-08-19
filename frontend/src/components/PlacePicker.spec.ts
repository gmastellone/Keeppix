import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'
import { ApiProblem } from '@/api/client'
import { useSessionStore } from '@/stores/session'
import type { MapRegion } from '@/stores/maps'

import PlacePicker from './PlacePicker.vue'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))
vi.mock('@/api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/api/client')>()),
  apiFetch
}))

afterEach(() => apiFetch.mockReset())

const kyoto = {
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

function mockSuggestAndBatch() {
  apiFetch.mockImplementation((path: string) => {
    if (path.startsWith('/api/v1/places/suggest')) return Promise.resolve([kyoto])
    if (path === '/api/v1/metadata/batch') return Promise.resolve({ batch_id: 'batch-1' })
    if (path === '/api/v1/map/regions') return Promise.resolve({ id: 'JP', status: 'downloading' })
    throw new Error(`unexpected ${path}`)
  })
}

function errorRegion(): MapRegion {
  return {
    id: 'JP',
    label: 'Japan',
    size_bytes: 100_000_000,
    version: '2024-01',
    downloaded_at: null,
    status: 'error',
    downloaded_bytes: 0,
    last_error: 'checksum mismatch',
    source_url: 'https://example.com/jp.pmtiles',
    checksum_sha256: 'a'.repeat(64)
  }
}

function downloadingRegion(): MapRegion {
  return { ...errorRegion(), status: 'downloading', last_error: null, downloaded_bytes: 50_000_000 }
}

async function selectKyoto(wrapper: ReturnType<typeof mount>) {
  await wrapper.get('input[type="search"]').setValue('ky')
  await wrapper.get('form').trigger('submit')
  await flushPromises()
  await wrapper.get('[data-place-id="1857910"]').trigger('click')
}

describe('PlacePicker', () => {
  it('applies a place even when its offline map is unavailable', async () => {
    mockSuggestAndBatch()
    setActivePinia(createPinia())
    const session = useSessionStore()
    session.user = { id: '1', username: 'a', display_name: 'A', email: null, role: 'admin', locale: null }

    const wrapper = mount(PlacePicker, {
      props: { assetIds: ['asset-1', 'asset-2'], availableRegionIds: [] },
      global: { plugins: [i18n] }
    })

    await selectKyoto(wrapper)

    const banner = wrapper.get('[role="status"]')
    expect(banner.text()).toContain('Map unavailable for this area')
    expect(banner.get('[data-action="apply"]').text()).toBe('Apply')
    expect(banner.get('[data-action="download-region"]').text()).toBe('Download Region')
    expect(banner.get('[data-action="download-region"]').attributes('href')).toBe(
      '/settings/maps/offline'
    )
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

  it('shows downloading status when the matching region is in progress', async () => {
    mockSuggestAndBatch()
    setActivePinia(createPinia())
    const session = useSessionStore()
    session.user = { id: '1', username: 'a', display_name: 'A', email: null, role: 'admin', locale: null }

    const wrapper = mount(PlacePicker, {
      props: {
        assetIds: ['asset-1'],
        availableRegionIds: [],
        allRegions: [downloadingRegion()]
      },
      global: { plugins: [i18n] }
    })

    await selectKyoto(wrapper)
    expect(wrapper.find('[data-testid="region-downloading"]').exists()).toBe(true)
  })

  it('retries a failed region download directly via downloadRegion', async () => {
    mockSuggestAndBatch()
    setActivePinia(createPinia())
    const session = useSessionStore()
    session.user = { id: '1', username: 'a', display_name: 'A', email: null, role: 'admin', locale: null }

    const wrapper = mount(PlacePicker, {
      props: {
        assetIds: ['asset-1'],
        availableRegionIds: [],
        allRegions: [errorRegion()]
      },
      global: { plugins: [i18n] }
    })

    await selectKyoto(wrapper)
    const downloadBtn = wrapper.get('[data-action="download-region"]')
    expect(downloadBtn.text()).toContain('Japan')
    await downloadBtn.trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/map/regions', {
      method: 'POST',
      body: JSON.stringify({
        id: 'JP',
        label: 'Japan',
        size_bytes: 100_000_000,
        version: '2024-01',
        source_url: 'https://example.com/jp.pmtiles',
        checksum_sha256: 'a'.repeat(64)
      })
    })
  })

  it('hides download button for non-admin users', async () => {
    mockSuggestAndBatch()
    setActivePinia(createPinia())
    const session = useSessionStore()
    session.user = { id: '2', username: 'b', display_name: 'B', email: null, role: 'user', locale: null }

    const wrapper = mount(PlacePicker, {
      props: {
        assetIds: ['asset-1'],
        availableRegionIds: [],
        allRegions: [errorRegion()]
      },
      global: { plugins: [i18n] }
    })

    await selectKyoto(wrapper)
    expect(wrapper.find('[data-action="download-region"]').exists()).toBe(false)
  })

  it('translates API problems by their stable type', async () => {
    apiFetch.mockRejectedValue(
      new ApiProblem('keeppix/service-unavailable', 'Service temporarily unavailable', 503)
    )
    setActivePinia(createPinia())
    const session = useSessionStore()
    session.user = { id: '1', username: 'a', display_name: 'A', email: null, role: 'admin', locale: null }

    const wrapper = mount(PlacePicker, {
      props: { assetIds: ['asset-1'], availableRegionIds: [] },
      global: { plugins: [i18n] }
    })
    await wrapper.get('input[type="search"]').setValue('ky')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(wrapper.get('[role="alert"]').text()).toContain('The server is unreachable')
    expect(wrapper.text()).not.toContain('Service temporarily unavailable')
  })
})
