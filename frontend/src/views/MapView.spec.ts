import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { describe, expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'

import { i18n } from '@/i18n'

import MapView from './MapView.vue'

const { apiFetch } = vi.hoisted(() => ({ apiFetch: vi.fn() }))
vi.mock('@/api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/api/client')>()),
  apiFetch
}))

describe('MapView', () => {
  it('passes every visible library to the cluster layer', async () => {
    apiFetch.mockImplementation((path: string) => {
      if (path === '/api/v1/map/regions') {
        return Promise.resolve([
          {
            id: 'IT',
            label: 'Italy',
            size_bytes: 1000,
            version: '2026-08',
            downloaded_at: '2026-08-19T00:00:00Z',
            status: 'available',
            downloaded_bytes: 1000,
            last_error: null
          }
        ])
      }
      if (path === '/api/v1/libraries') {
        return Promise.resolve([
          { id: 'library-1', name: 'One' },
          { id: 'library-2', name: 'Two' }
        ])
      }
      throw new Error(`unexpected ${path}`)
    })

    const wrapper = mount(MapView, {
      global: {
        plugins: [createPinia(), i18n],
        mocks: { $router: { push: vi.fn() } },
        stubs: {
          AssetViewer: true,
          MapsOfflineView: true,
          MapClusterLayer: {
            name: 'MapClusterLayer',
            props: ['scopeId'],
            template: '<div data-testid="cluster-layer" />'
          }
        }
      }
    })
    await flushPromises()

    expect(wrapper.getComponent({ name: 'MapClusterLayer' }).props('scopeId')).toEqual([
      'library-1',
      'library-2'
    ])
  })

  it('§27 "Apri cartella": open-folder from the cluster layer navigates to /folders (no folder-scoped timeline route exists)', async () => {
    apiFetch.mockImplementation((path: string) => {
      if (path === '/api/v1/map/regions') {
        return Promise.resolve([
          {
            id: 'IT',
            label: 'Italy',
            size_bytes: 1000,
            version: '2026-08',
            downloaded_at: '2026-08-19T00:00:00Z',
            status: 'available',
            downloaded_bytes: 1000,
            last_error: null
          }
        ])
      }
      if (path === '/api/v1/libraries') {
        return Promise.resolve([{ id: 'library-1', name: 'One' }])
      }
      throw new Error(`unexpected ${path}`)
    })

    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/map', component: MapView },
        { path: '/folders', component: { template: '<div />' } }
      ]
    })
    await router.push('/map')
    await router.isReady()

    const wrapper = mount(MapView, {
      global: {
        plugins: [createPinia(), i18n, router],
        stubs: {
          AssetViewer: true,
          MapsOfflineView: true,
          MapClusterLayer: {
            name: 'MapClusterLayer',
            props: ['scopeId'],
            template: '<div data-testid="cluster-layer" />'
          }
        }
      }
    })
    await flushPromises()

    wrapper.getComponent({ name: 'MapClusterLayer' }).vm.$emit('open-folder', 'folder-1')
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/folders')
  })
})
