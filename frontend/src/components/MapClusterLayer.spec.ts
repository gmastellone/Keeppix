import { flushPromises, mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { i18n } from '@/i18n'
import { ApiProblem } from '@/api/client'

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
    unproject: vi.fn(),
    project: vi.fn(() => ({ x: 100, y: 50 }))
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
vi.mock('@/api/client', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/api/client')>()),
  apiFetch
}))

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
    add = vi.fn()
  },
  PMTiles: class {}
}))

afterEach(() => {
  apiFetch.mockReset()
  markerElements.splice(0)
  mapHandlers.clear()
  mediaHandlers.splice(0)
  map.easeTo.mockReset()
  map.setStyle.mockReset()
  map.remove.mockReset()
  map.project.mockReset()
  map.project.mockReturnValue({ x: 100, y: 50 })
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

function clusterFixtures(path: string) {
  if (path.startsWith('/api/v1/map/clusters')) {
    return Promise.resolve(
      path.includes('scope_id=library-1')
        ? [{
            lat: 41,
            lon: 11,
            count: 8,
            cover_asset_id: 'cluster-cover',
            clustered: true,
            folder_id: 'folder-1',
            place_label: 'Lago di Braies, Trentino-AA'
          }]
        : [{
            lat: 41.5,
            lon: 11.5,
            count: 1,
            cover_asset_id: 'asset-1',
            clustered: false,
            folder_id: 'folder-1'
          }]
    )
  }
  if (path.startsWith('/api/v1/folders/tree')) {
    return Promise.resolve([{ id: 'folder-1', library_id: 'lib', parent_id: null, name: 'Lago di Braies', depth: 0 }])
  }
  return Promise.resolve({ content_hash: 'a'.repeat(64) })
}

describe('MapClusterLayer', () => {
  it('loads visible clusters and emits a single asset for an unclustered marker', async () => {
    installMatchMedia()
    apiFetch.mockImplementation(clusterFixtures)

    const wrapper = mount(MapClusterLayer, {
      props: {
        scope: 'library',
        scopeId: ['library-1', 'library-2'],
        regionIds: ['IT']
      },
      global: { plugins: [createPinia(), i18n] }
    })
    await flushPromises()

    expect(apiFetch).toHaveBeenCalledWith(expect.stringContaining('scope_id=library-1'))
    expect(apiFetch).toHaveBeenCalledWith(expect.stringContaining('scope_id=library-2'))
    expect(markerElements).toHaveLength(2)
    expect(markerElements[0]!.querySelector('img')?.getAttribute('src')).toContain('/media/thumb/')

    markerElements[1]!.click()
    expect(wrapper.emitted('asset-click')).toEqual([['asset-1']])
  })

  it('a compact map (lightbox mini-map) keeps the old zoom-on-click behavior, no popover', async () => {
    installMatchMedia()
    apiFetch.mockImplementation(clusterFixtures)

    const wrapper = mount(MapClusterLayer, {
      props: {
        scope: 'library',
        scopeId: ['library-1', 'library-2'],
        regionIds: ['IT'],
        compact: true
      },
      global: { plugins: [createPinia(), i18n] }
    })
    await flushPromises()

    markerElements[0]!.click()
    expect(map.easeTo).toHaveBeenCalledWith({ center: [11, 41], zoom: 9 })
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false)
  })

  it('§27: a clustered marker opens a popover (cover, folder name, count + place, "Apri cartella") instead of zooming', async () => {
    installMatchMedia()
    apiFetch.mockImplementation(clusterFixtures)

    const wrapper = mount(MapClusterLayer, {
      props: {
        scope: 'library',
        scopeId: ['library-1', 'library-2'],
        regionIds: ['IT']
      },
      global: { plugins: [createPinia(), i18n] }
    })
    await flushPromises()

    markerElements[0]!.click()
    await flushPromises()

    expect(map.easeTo).not.toHaveBeenCalled()
    const dialog = wrapper.get('[role="dialog"]')
    expect(dialog.text()).toContain('Lago di Braies')
    expect(dialog.text()).toContain('8 photos · Lago di Braies, Trentino-AA')

    await dialog.get('button').trigger('click')
    expect(wrapper.emitted('open-folder')).toEqual([['folder-1']])
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false)
  })

  it('Escape closes the popover; a click on the base map also closes it', async () => {
    installMatchMedia()
    apiFetch.mockImplementation(clusterFixtures)

    const wrapper = mount(MapClusterLayer, {
      props: {
        scope: 'library',
        scopeId: 'library-1',
        regionIds: ['IT']
      },
      global: { plugins: [createPinia(), i18n] }
    })
    await flushPromises()

    markerElements[0]!.click()
    await flushPromises()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true)

    await wrapper.get('[role="dialog"]').trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false)

    markerElements[0]!.click()
    await flushPromises()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true)

    for (const handler of mapHandlers.get('click') ?? []) handler()
    await wrapper.vm.$nextTick()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false)
  })

  it('starting a pan (movestart) closes the popover, so it does not float detached mid-drag', async () => {
    installMatchMedia()
    apiFetch.mockImplementation(clusterFixtures)

    const wrapper = mount(MapClusterLayer, {
      props: {
        scope: 'library',
        scopeId: 'library-1',
        regionIds: ['IT']
      },
      global: { plugins: [createPinia(), i18n] }
    })
    await flushPromises()

    markerElements[0]!.click()
    await flushPromises()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true)

    for (const handler of mapHandlers.get('movestart') ?? []) handler()
    await wrapper.vm.$nextTick()
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false)
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

  it('shows translated cluster failures instead of failing silently', async () => {
    installMatchMedia()
    apiFetch.mockRejectedValue(
      new ApiProblem('keeppix/service-unavailable', 'Service temporarily unavailable', 503)
    )
    const wrapper = mount(MapClusterLayer, {
      props: {
        scope: 'library',
        scopeId: 'library-1',
        regionIds: ['IT']
      },
      global: { plugins: [createPinia(), i18n] }
    })
    await flushPromises()

    expect(wrapper.get('[role="alert"]').text()).toContain('The server is unreachable')
    expect(wrapper.text()).not.toContain('Service temporarily unavailable')
  })

  it('turns a tile RFC 9457 not-found error into a region-unavailable message', async () => {
    installMatchMedia()
    apiFetch.mockResolvedValue([])
    const wrapper = mount(MapClusterLayer, {
      props: {
        scope: 'library',
        scopeId: 'library-1',
        regionIds: ['IT']
      },
      global: { plugins: [createPinia(), i18n] }
    })
    await flushPromises()

    for (const handler of mapHandlers.get('error') ?? []) {
      handler({
        error: new ApiProblem('keeppix/not-found', 'Resource not found', 404)
      })
    }
    await wrapper.vm.$nextTick()

    expect(wrapper.get('[role="alert"]').text()).toContain('Region no longer available')
    expect(wrapper.text()).not.toContain('Resource not found')
  })
})
