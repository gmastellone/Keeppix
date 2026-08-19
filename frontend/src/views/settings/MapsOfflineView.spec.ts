import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'
import { useSessionStore } from '@/stores/session'

import MapsOfflineView from './MapsOfflineView.vue'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))
vi.mock('@/api/client', () => ({ apiFetch }))

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
  it('shows the continent catalog with sizes when no region is downloaded', async () => {
    apiFetch.mockResolvedValue([])
    const wrapper = mount(MapsOfflineView, {
      global: { plugins: [adminPinia(), i18n] }
    })
    await flushPromises()

    expect(wrapper.text()).toContain('Europe')
    expect(wrapper.text()).toContain('Italy')
    expect(wrapper.text()).toContain('712 MB')
    expect(wrapper.text()).not.toContain('Downloaded regions')
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
})
