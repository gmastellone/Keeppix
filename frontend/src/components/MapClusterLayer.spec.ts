import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'

import MapClusterLayer from './MapClusterLayer.vue'

const {
  apiFetch,
  markerElements,
  mapHandlers,
  mediaHandlers,
  map,
  MapConstructor,
  addProtocol
} = vi.hoisted(() => {
  const markerElements: HTMLElement[] = []
  const mapHandlers = new Map<string, Set<(event?: unknown) => void>>()
  const mediaHandlers: Array<(event: MediaQueryListEvent) => void> = []
  const map = {
    easeTo: vi.fn(),
    getBounds: () => ({
      getWest: () => 10,
      getSouth: () => 40,
      getEast: () => 12,
      getNorth: () => 42
    }),
    getZoom: () => 7,
    on: vi.fn((event: string, handler: (event?: unknown) => void) => {
      const handlers = mapHandlers.get(event) ?? new Set()
      handlers.add(handler)
      mapHandlers.set(event, handlers)
    }),
    off: vi.fn(),
    remove: vi.fn(),
    setStyle: vi.fn(),
    unproject: vi.fn()
  }
  return {
    apiFetch: vi.fn(),
    markerElements,
    mapHandlers,
    mediaHandlers,
    map,
    MapConstructor: vi.fn(function MapConstructor() {
      return map
    }),
    addProtocol: vi.fn()
  }
})
vi.mock('@/api/client', () => ({ apiFetch }))

vi.mock('maplibre-gl', () => {
  class Marker {
    constructor(options: { element: HTMLElement }) {
      markerElements.push(options.element)
    }

    setLngLat() {
      return this
    }

    addTo() {
      return this
    }

    remove() {}
  }

  return { Map: MapConstructor, Marker, addProtocol }
})
vi.mock('maplibre-gl/dist/maplibre-gl.css', () => ({}))
vi.mock('pmtiles', () => ({
  Protocol: class {
    tile = vi.fn()
  }
}))

afterEach(() => {
  apiFetch.mockReset()
  markerElements.splice(0)
  mapHandlers.clear()
  mediaHandlers.splice(0)
  map.easeTo.mockReset()
  map.setStyle.mockReset()
  map.remove.mockReset()
  MapConstructor.mockClear()
  addProtocol.mockClear()
  vi.unstubAllGlobals()
})

function installMatchMedia() {
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({
      matches: false,
      media: '(prefers-color-scheme: dark)',
      onchange: null,
      addEventListener: (_event: string, handler: (event: MediaQueryListEvent) => void) => {
        mediaHandlers.push(handler)
      },
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn()
    }))
  )
}

describe('MapClusterLayer', () => {
  it('loads visible clusters, zooms clusters, and emits a single asset', async () => {
    installMatchMedia()
    apiFetch.mockImplementation((path: string) => {
      if (path.startsWith('/api/v1/map/clusters')) {
        return Promise.resolve([
          {
            lat: 41,
            lon: 11,
            count: 8,
            cover_asset_id: 'cluster-cover',
            clustered: true
          },
          {
            lat: 41.5,
            lon: 11.5,
            count: 1,
            cover_asset_id: 'asset-1',
            clustered: false
          }
        ])
      }
      return Promise.resolve({ content_hash: 'a'.repeat(64) })
    })

    const wrapper = mount(MapClusterLayer, {
      props: {
        scope: 'library',
        scopeId: '018f0000-0000-7000-8000-000000000001',
        regionIds: ['IT']
      },
      global: { plugins: [createPinia(), i18n] }
    })
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith(
      expect.stringContaining(
        '/api/v1/map/clusters?bbox=10%2C40%2C12%2C42&zoom=7&scope=library&scope_id='
      )
    )
    expect(markerElements).toHaveLength(2)
    expect(markerElements[0]!.querySelector('img')?.getAttribute('src')).toContain('/media/thumb/')

    markerElements[0]!.click()
    expect(map.easeTo).toHaveBeenCalledWith({ center: [11, 41], zoom: 9 })

    markerElements[1]!.click()
    expect(wrapper.emitted('asset-click')).toEqual([['asset-1']])
  })

  it('replaces the local map style when the system theme changes', async () => {
    installMatchMedia()
    apiFetch.mockResolvedValue([])
    mount(MapClusterLayer, {
      props: {
        scope: 'library',
        scopeId: '018f0000-0000-7000-8000-000000000001',
        regionIds: ['IT']
      },
      global: { plugins: [createPinia(), i18n] }
    })
    await flushPromises()

    mediaHandlers[0]?.({ matches: true } as MediaQueryListEvent)
    expect(map.setStyle).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'Keeppix dark' })
    )
  })
})
