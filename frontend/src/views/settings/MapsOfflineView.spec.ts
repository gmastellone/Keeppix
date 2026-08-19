import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'
import { ApiProblem } from '@/api/client'
import { useSessionStore } from '@/stores/session'

import MapsOfflineView from './MapsOfflineView.vue'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))
vi.mock('@/api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/api/client')>()),
  apiFetch
}))

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

describe('MapsOfflineView', () => {
  it('shows the real empty list and an admin-supplied download form', async () => {
    apiFetch.mockImplementation((_path: string, init?: RequestInit) =>
      Promise.resolve(
        init?.method === 'POST'
          ? {
              id: 'custom-it',
              label: 'Italy 2026',
              size_bytes: 712000000,
              version: '2026-08',
              downloaded_at: null,
              status: 'downloading',
              downloaded_bytes: 0,
              last_error: null
            }
          : []
      )
    )
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('No offline regions have been added.')
    expect(wrapper.text()).not.toContain('Italy')
    expect(wrapper.text()).not.toContain('Downloaded regions')

    await wrapper.get('[name="region-id"]').setValue('custom-it')
    await wrapper.get('[name="region-label"]').setValue('Italy 2026')
    await wrapper.get('[name="region-size"]').setValue('712000000')
    await wrapper.get('[name="region-version"]').setValue('2026-08')
    await wrapper.get('[name="region-url"]').setValue('https://maps.example.test/it.pmtiles')
    await wrapper.get('[name="region-sha256"]').setValue('cd'.repeat(32))
    await wrapper.get('[data-action="download-region"]').trigger('submit')
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith('/api/v1/map/regions', {
      method: 'POST',
      body: JSON.stringify({
        id: 'custom-it',
        label: 'Italy 2026',
        size_bytes: 712000000,
        version: '2026-08',
        source_url: 'https://maps.example.test/it.pmtiles',
        checksum_sha256: 'cd'.repeat(32)
      })
    })
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

  it('polls a newly started download until it becomes available', async () => {
    vi.useFakeTimers()
    let listCalls = 0
    apiFetch.mockImplementation((path: string, init?: RequestInit) => {
      if (path === '/api/v1/map/regions' && init?.method === 'POST') {
        return Promise.resolve({
          id: 'IT',
          label: 'Italy',
          size_bytes: 1000,
          version: '2026-08',
          downloaded_at: null,
          status: 'downloading',
          downloaded_bytes: 100,
          last_error: null
        })
      }
      if (path === '/api/v1/map/regions') {
        listCalls += 1
        return Promise.resolve(
          listCalls === 1
            ? []
            : [{
                id: 'IT',
                label: 'Italy',
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

    await wrapper.get('[name="region-id"]').setValue('IT')
    await wrapper.get('[name="region-label"]').setValue('Italy')
    await wrapper.get('[name="region-size"]').setValue('1000')
    await wrapper.get('[name="region-version"]').setValue('2026-08')
    await wrapper.get('[name="region-url"]').setValue('https://maps.example.test/it.pmtiles')
    await wrapper.get('[name="region-sha256"]').setValue('ef'.repeat(32))
    await wrapper.get('[data-action="download-region"]').trigger('submit')
    await flushPromises()
    expect(wrapper.text()).toContain('10%')

    await vi.advanceTimersByTimeAsync(2000)
    await flushPromises()

    expect(wrapper.text()).toContain('Available')
    expect(wrapper.text()).not.toContain('10%')
  })

  it('translates RFC 9457 errors and labels raw download details', async () => {
    apiFetch.mockResolvedValueOnce([
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
