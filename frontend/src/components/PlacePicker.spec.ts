import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'
import { ApiProblem } from '@/api/client'
import { useSessionStore } from '@/stores/session'
import type { MapRegion, RegionCatalogEntry } from '@/stores/maps'

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

const JAPAN_CATALOG_ENTRY: RegionCatalogEntry = { id: 'JP', label: 'Japan', approx_size_bytes: 100_000_000 }

/** Routes the store's own two boot-time GETs (`loadRegions`/`loadCatalog`,
 * fired from `PlacePicker`'s `onMounted`) plus places/suggest and the
 * apply/download POSTs. `regions`/`catalog` default to empty so a test
 * that doesn't care about the offline-map banner never has to seed them. */
function mockPicker(opts: { regions?: MapRegion[]; catalog?: RegionCatalogEntry[] } = {}) {
  apiFetch.mockImplementation((path: string, init?: RequestInit) => {
    if (path.startsWith('/api/v1/places/suggest')) return Promise.resolve([kyoto])
    if (path === '/api/v1/metadata/batch') return Promise.resolve({ batch_id: 'batch-1' })
    if (path === '/api/v1/map/regions' && !init) return Promise.resolve(opts.regions ?? [])
    if (path === '/api/v1/map/regions/catalog') return Promise.resolve(opts.catalog ?? [])
    if (path === '/api/v1/map/regions/catalog/JP' && init?.method === 'POST') {
      return Promise.resolve({ ...errorRegion(), status: 'downloading' })
    }
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

function adminSession() {
  const session = useSessionStore()
  session.user = { id: '1', username: 'a', display_name: 'A', email: null, role: 'admin', locale: null }
}

async function selectKyoto(wrapper: ReturnType<typeof mount>) {
  await wrapper.get('input[type="search"]').setValue('ky')
  await wrapper.get('form').trigger('submit')
  await flushPromises()
  await wrapper.get('[data-place-id="1857910"]').trigger('click')
}

describe('PlacePicker', () => {
  it('applies a place even when its offline map is unavailable', async () => {
    mockPicker()
    setActivePinia(createPinia())
    adminSession()

    const wrapper = mount(PlacePicker, {
      props: { assetIds: ['asset-1', 'asset-2'] },
      global: { plugins: [i18n] }
    })
    await flushPromises()
    await selectKyoto(wrapper)

    const banner = wrapper.get('[role="status"]')
    expect(banner.text()).toContain('Map unavailable for this area')
    expect(banner.get('[data-action="apply"]').text()).toBe('Apply')
    // No catalog entry for JP in this test: falls back to the Settings
    // link, same as a country entirely outside the 35-country catalog.
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
    mockPicker({ regions: [downloadingRegion()], catalog: [JAPAN_CATALOG_ENTRY] })
    setActivePinia(createPinia())
    adminSession()

    const wrapper = mount(PlacePicker, {
      props: { assetIds: ['asset-1'] },
      global: { plugins: [i18n] }
    })
    await flushPromises()
    await selectKyoto(wrapper)

    expect(wrapper.find('[data-testid="region-downloading"]').exists()).toBe(true)
    // Already downloading: no point offering to start another download —
    // only the Settings fallback link remains, not a `<button>`.
    expect(wrapper.get('[data-action="download-region"]').element.tagName).toBe('A')
  })

  it('downloads a catalog region inline, whether or not it was ever tried before', async () => {
    mockPicker({ regions: [errorRegion()], catalog: [JAPAN_CATALOG_ENTRY] })
    setActivePinia(createPinia())
    adminSession()

    const wrapper = mount(PlacePicker, {
      props: { assetIds: ['asset-1'] },
      global: { plugins: [i18n] }
    })
    await flushPromises()
    await selectKyoto(wrapper)

    const downloadBtn = wrapper.get('[data-action="download-region"]')
    expect(downloadBtn.text()).toContain('Japan')
    await downloadBtn.trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/map/regions/catalog/JP', { method: 'POST' })
  })

  it('hides download button for non-admin users', async () => {
    mockPicker({ regions: [errorRegion()], catalog: [JAPAN_CATALOG_ENTRY] })
    setActivePinia(createPinia())
    const session = useSessionStore()
    session.user = { id: '2', username: 'b', display_name: 'B', email: null, role: 'user', locale: null }

    const wrapper = mount(PlacePicker, {
      props: { assetIds: ['asset-1'] },
      global: { plugins: [i18n] }
    })
    await flushPromises()
    await selectKyoto(wrapper)

    expect(wrapper.find('[data-action="download-region"]').exists()).toBe(false)
  })

  it('translates API problems by their stable type', async () => {
    apiFetch.mockRejectedValue(
      new ApiProblem('keeppix/service-unavailable', 'Service temporarily unavailable', 503)
    )
    setActivePinia(createPinia())
    adminSession()

    const wrapper = mount(PlacePicker, {
      props: { assetIds: ['asset-1'] },
      global: { plugins: [i18n] }
    })
    await flushPromises()
    await wrapper.get('input[type="search"]').setValue('ky')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(wrapper.get('[role="alert"]').text()).toContain('The server is unreachable')
    expect(wrapper.text()).not.toContain('Service temporarily unavailable')
  })
})
