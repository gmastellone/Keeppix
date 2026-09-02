import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'
import { ApiProblem } from '@/api/client'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import MapsOfflineView from './MapsOfflineView.vue'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))
vi.mock('@/api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/api/client')>()),
  apiFetch
}))

const CATALOG = [
  { id: 'FR', label: 'France', approx_size_bytes: 480000000 },
  { id: 'DE', label: 'Germany', approx_size_bytes: 520000000 },
  { id: 'GR', label: 'Greece', approx_size_bytes: 220000000 }
]

afterEach(() => {
  apiFetch.mockReset()
  vi.useRealTimers()
})

function adminPinia() {
  const pinia = createPinia()
  setActivePinia(pinia)
  useSessionStore().user = {
    id: 'admin-1',
    username: 'admin',
    display_name: 'Admin',
    email: null,
    role: 'admin',
    locale: null
  }
  return pinia
}

/** Routes the store's two independent GETs (regions list, catalog) plus
 * anything else a test layers on top via `extra`. */
function mockPanelFetch(regions: unknown[], extra?: (path: string, init?: RequestInit) => unknown) {
  apiFetch.mockImplementation((path: string, init?: RequestInit) => {
    if (extra) {
      const handled = extra(path, init)
      if (handled !== undefined) return Promise.resolve(handled)
    }
    if (path === '/api/v1/map/regions' && !init) return Promise.resolve(regions)
    if (path === '/api/v1/map/regions/catalog') return Promise.resolve(CATALOG)
    throw new Error(`unexpected ${path}`)
  })
}

describe('MapsOfflineView', () => {
  it('shows the real empty list and the region search closed by default', async () => {
    mockPanelFetch([])
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('No offline regions have been added.')
    expect(wrapper.text()).not.toContain('Downloaded regions')
    expect(wrapper.get('[data-action="open-region-search"]').text()).toContain('Add region')
    expect(wrapper.find('#regionSearchInput').exists()).toBe(false)
  })

  it('opens the search, filters the catalog, and excludes already-tracked regions', async () => {
    mockPanelFetch([{
      id: 'GR',
      label: 'Greece',
      size_bytes: 1000,
      version: '2026-08',
      downloaded_at: '2026-08-18T00:00:00Z',
      status: 'available',
      downloaded_bytes: 1000,
      last_error: null
    }])
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()

    await wrapper.get('[data-action="open-region-search"]').trigger('click')
    await flushPromises()

    expect(wrapper.find('[data-action="open-region-search"]').exists()).toBe(false)
    expect(wrapper.text()).toContain('Type to search available regions')

    const input = wrapper.get('#regionSearchInput')
    expect(input.attributes('autocomplete')).toBe('off')
    await input.setValue('ger')
    await flushPromises()

    const rows = wrapper.findAll('[role="option"]')
    expect(rows).toHaveLength(1)
    expect(rows[0]!.text()).toContain('Germany')
    // Greece is already tracked (in the regions list above) — never
    // offered again by search, matching the spec's "already in the list"
    // exclusion. (It still legitimately appears in the tracked-regions
    // list rendered above the search box.)
    expect(rows.some((row) => row.text().includes('Greece'))).toBe(false)

    await input.setValue('atlantis')
    await flushPromises()
    expect(wrapper.text()).toContain('No region found.')
  })

  it('caps results at 8 rows', async () => {
    const bigCatalog = Array.from({ length: 12 }, (_, i) => ({
      id: `Z${i}`,
      label: `Zealand ${i}`,
      approx_size_bytes: 1000
    }))
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/v1/map/regions' && !init) return Promise.resolve([])
      if (path === '/api/v1/map/regions/catalog') return Promise.resolve(bigCatalog)
      throw new Error(`unexpected ${path}`)
    })
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()
    await wrapper.get('[data-action="open-region-search"]').trigger('click')
    await flushPromises()
    await wrapper.get('#regionSearchInput').setValue('zealand')
    await flushPromises()

    expect(wrapper.findAll('[role="option"]')).toHaveLength(8)
  })

  it('clicking a result downloads it, closes the search, and toasts', async () => {
    mockPanelFetch([], (path, init) => {
      if (path === '/api/v1/map/regions/catalog/FR' && init?.method === 'POST') {
        return {
          id: 'FR',
          label: 'France',
          size_bytes: 480000000,
          version: 'pending',
          downloaded_at: null,
          status: 'downloading',
          downloaded_bytes: 0,
          last_error: null
        }
      }
      return undefined
    })
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()
    await wrapper.get('[data-action="open-region-search"]').trigger('click')
    await flushPromises()
    await wrapper.get('#regionSearchInput').setValue('fra')
    await flushPromises()

    await wrapper.get('[data-region-id="FR"]').trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/map/regions/catalog/FR', { method: 'POST' })
    expect(wrapper.find('#regionSearchInput').exists()).toBe(false)
    expect(
      useToastStore().toasts.some((toast) => toast.message === 'France added — download started.')
    ).toBe(true)
    expect(wrapper.text()).toContain('France')
  })

  it('activating a result with Enter also downloads it', async () => {
    mockPanelFetch([], (path, init) => {
      if (path === '/api/v1/map/regions/catalog/FR' && init?.method === 'POST') {
        return {
          id: 'FR',
          label: 'France',
          size_bytes: 480000000,
          version: 'pending',
          downloaded_at: null,
          status: 'downloading',
          downloaded_bytes: 0,
          last_error: null
        }
      }
      return undefined
    })
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()
    await wrapper.get('[data-action="open-region-search"]').trigger('click')
    await flushPromises()
    await wrapper.get('#regionSearchInput').setValue('fra')
    await flushPromises()

    await wrapper.get('[data-region-id="FR"]').trigger('keydown', { key: 'Enter' })
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/map/regions/catalog/FR', { method: 'POST' })
  })

  it('Esc closes the search from anywhere, without adding anything', async () => {
    mockPanelFetch([])
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] },
      attachTo: document.body
    })
    await flushPromises()
    await wrapper.get('[data-action="open-region-search"]').trigger('click')
    await flushPromises()
    await wrapper.get('#regionSearchInput').setValue('fra')
    await flushPromises()

    // Dispatched globally, not on the input — matches the spec's "Esc
    // closes independently of where focus currently is".
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()

    expect(wrapper.find('#regionSearchInput').exists()).toBe(false)
    expect(wrapper.find('[data-action="open-region-search"]').exists()).toBe(true)
    expect(apiFetch).not.toHaveBeenCalledWith(
      expect.stringContaining('/catalog/'),
      expect.anything()
    )
    wrapper.unmount()
  })

  it('shows download progress and admin cancel/delete controls', async () => {
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/v1/map/regions' && !init) {
        return Promise.resolve([
          {
            id: 'IT',
            label: 'Italy',
            size_bytes: 1000,
            version: '2026-08',
            downloaded_at: null,
            status: 'downloading',
            downloaded_bytes: 250,
            last_error: null
          },
          {
            id: 'GR',
            label: 'Greece',
            size_bytes: 1000,
            version: '2026-08',
            downloaded_at: '2026-08-18T00:00:00Z',
            status: 'available',
            downloaded_bytes: 1000,
            last_error: null
          }
        ])
      }
      if (path.endsWith('/cancel') || init?.method === 'DELETE') return Promise.resolve(null)
      throw new Error(`unexpected ${path}`)
    })
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()

    expect(wrapper.get('progress').attributes('value')).toBe('25')
    await wrapper.get('[data-action="cancel-IT"]').trigger('click')
    await wrapper.get('[data-action="delete-GR"]').trigger('click')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/map/regions/IT/cancel', { method: 'POST' })
    expect(apiFetch).toHaveBeenCalledWith('/api/v1/map/regions/GR', { method: 'DELETE' })
  })

  it('polls a newly started catalog download until it becomes available', async () => {
    vi.useFakeTimers()
    let listCalls = 0
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/v1/map/regions/catalog') return Promise.resolve(CATALOG)
      if (path === '/api/v1/map/regions/catalog/FR' && init?.method === 'POST') {
        return Promise.resolve({
          id: 'FR',
          label: 'France',
          size_bytes: 1000,
          version: 'pending',
          downloaded_at: null,
          status: 'downloading',
          downloaded_bytes: 100,
          last_error: null
        })
      }
      if (path === '/api/v1/map/regions' && !init) {
        listCalls += 1
        return Promise.resolve(
          listCalls === 1
            ? []
            : [{
                id: 'FR',
                label: 'France',
                size_bytes: 1000,
                version: '2026-08',
                downloaded_at: '2026-08-19T00:00:00Z',
                status: 'available',
                downloaded_bytes: 1000,
                last_error: null
              }]
        )
      }
      throw new Error(`unexpected ${path}`)
    })
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()
    await wrapper.get('[data-action="open-region-search"]').trigger('click')
    await flushPromises()
    await wrapper.get('#regionSearchInput').setValue('fra')
    await flushPromises()
    await wrapper.get('[data-region-id="FR"]').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('10%')

    await vi.advanceTimersByTimeAsync(2000)
    await flushPromises()

    expect(wrapper.text()).toContain('Available')
    expect(wrapper.text()).not.toContain('10%')
  })

  it('translates RFC 9457 errors and still supports the manual advanced form', async () => {
    mockPanelFetch([
      {
        id: 'IT',
        label: 'Italy',
        size_bytes: 1000,
        version: '2026-08',
        downloaded_at: null,
        status: 'error',
        downloaded_bytes: 0,
        last_error: 'checksum mismatch at byte 400'
      }
    ])
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Download failed')
    expect(wrapper.text()).toContain('checksum mismatch at byte 400')

    await wrapper.get('summary').trigger('click')
    apiFetch.mockRejectedValueOnce(
      new ApiProblem(
        'keeppix/region-source-not-allowed',
        'Region source URL is not allowed',
        422
      )
    )
    await wrapper.get('[name="region-id"]').setValue('FR')
    await wrapper.get('[name="region-label"]').setValue('France')
    await wrapper.get('[name="region-size"]').setValue('1')
    await wrapper.get('[name="region-version"]').setValue('2026-08')
    await wrapper.get('[name="region-url"]').setValue('https://invalid.example/fr.pmtiles')
    await wrapper.get('[name="region-sha256"]').setValue('ab'.repeat(32))
    await wrapper.get('[data-action="download-region"]').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('This region download source is not allowed.')
    expect(wrapper.text()).not.toContain('Region source URL is not allowed')
  })
})
