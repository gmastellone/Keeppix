import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import { apiFetch } from '@/api/client'
import type { TimelineAsset } from '@/api/timeline'

export type MapScope = 'library' | 'album' | 'folder' | 'search'

export interface MapBounds {
  west: number
  south: number
  east: number
  north: number
}

export interface MapCluster {
  lat: number
  lon: number
  count: number
  cover_asset_id: string
  clustered: boolean
}

export interface MapRegion {
  id: string
  label: string
  size_bytes: number
  version: string
  downloaded_at: string | null
  status: 'available' | 'downloading' | 'error'
  downloaded_bytes: number
  last_error: string | null
}

export interface RegionCatalogEntry {
  id: string
  continent: 'europe' | 'asia' | 'americas'
  label: string
  size_bytes: number
  version: string
  source_url: string
  checksum_sha256: string
}

export interface Place {
  id: number
  name: string
  ascii_name: string
  country_code: string | null
  admin1: string | null
  admin2: string | null
  lat: number
  lon: number
  population: number
}

const CATALOG_VERSION = '2026-08'
const CATALOG_CHECKSUM = 'ab'.repeat(32)

/**
 * Catalogo volutamente piccolo: il server applica comunque allowlist e
 * checksum. Una futura API manifest può sostituire questi metadati senza
 * cambiare i componenti che consumano `REGION_CATALOG`.
 */
export const REGION_CATALOG: readonly RegionCatalogEntry[] = [
  {
    id: 'IT',
    continent: 'europe',
    label: 'Italy',
    size_bytes: 712_000_000,
    version: CATALOG_VERSION,
    source_url: 'https://build.protomaps.com/IT.pmtiles',
    checksum_sha256: CATALOG_CHECKSUM
  },
  {
    id: 'GR',
    continent: 'europe',
    label: 'Greece',
    size_bytes: 398_000_000,
    version: CATALOG_VERSION,
    source_url: 'https://build.protomaps.com/GR.pmtiles',
    checksum_sha256: CATALOG_CHECKSUM
  },
  {
    id: 'JP',
    continent: 'asia',
    label: 'Japan',
    size_bytes: 1_100_000_000,
    version: CATALOG_VERSION,
    source_url: 'https://build.protomaps.com/JP.pmtiles',
    checksum_sha256: CATALOG_CHECKSUM
  },
  {
    id: 'US',
    continent: 'americas',
    label: 'United States',
    size_bytes: 8_600_000_000,
    version: CATALOG_VERSION,
    source_url: 'https://build.protomaps.com/US.pmtiles',
    checksum_sha256: CATALOG_CHECKSUM
  }
] as const

function replaceRegion(regions: MapRegion[], region: MapRegion): MapRegion[] {
  return [...regions.filter((item) => item.id !== region.id), region]
}

export const useMapsStore = defineStore('maps', () => {
  const regions = ref<MapRegion[]>([])
  const loading = ref(false)
  const loaded = ref(false)
  const error = ref<unknown>()

  const availableRegionIds = computed(() =>
    regions.value.filter((region) => region.status === 'available').map((region) => region.id)
  )

  async function loadRegions(): Promise<void> {
    loading.value = true
    error.value = undefined
    try {
      regions.value = await apiFetch<MapRegion[]>('/api/v1/map/regions')
      loaded.value = true
    } catch (cause) {
      error.value = cause
      throw cause
    } finally {
      loading.value = false
    }
  }

  async function downloadRegion(entry: RegionCatalogEntry): Promise<void> {
    const region = await apiFetch<MapRegion>('/api/v1/map/regions', {
      method: 'POST',
      body: JSON.stringify({
        id: entry.id,
        label: entry.label,
        size_bytes: entry.size_bytes,
        version: entry.version,
        source_url: entry.source_url,
        checksum_sha256: entry.checksum_sha256
      })
    })
    regions.value = replaceRegion(regions.value, region)
  }

  async function cancelRegion(id: string): Promise<void> {
    await apiFetch(`/api/v1/map/regions/${encodeURIComponent(id)}/cancel`, {
      method: 'POST'
    })
    await loadRegions()
  }

  async function deleteRegion(id: string): Promise<void> {
    await apiFetch(`/api/v1/map/regions/${encodeURIComponent(id)}`, {
      method: 'DELETE'
    })
    regions.value = regions.value.filter((region) => region.id !== id)
  }

  async function fetchClusters(
    bounds: MapBounds,
    zoom: number,
    scope: MapScope,
    scopeId: string
  ): Promise<MapCluster[]> {
    const query = new URLSearchParams({
      bbox: `${bounds.west},${bounds.south},${bounds.east},${bounds.north}`,
      zoom: String(zoom),
      scope,
      scope_id: scopeId
    })
    return apiFetch(`/api/v1/map/clusters?${query}`)
  }

  async function suggestPlaces(query: string): Promise<Place[]> {
    const params = new URLSearchParams({ q: query, near_user: 'true' })
    return apiFetch(`/api/v1/places/suggest?${params}`)
  }

  async function applyPlace(assetIds: string[], place: Place): Promise<void> {
    await apiFetch('/api/v1/metadata/batch', {
      method: 'POST',
      body: JSON.stringify({
        asset_ids: assetIds,
        patch: {
          location: { lat: place.lat, lon: place.lon },
          place_id: place.id
        }
      })
    })
  }

  function loadAsset(id: string): Promise<TimelineAsset> {
    return apiFetch(`/api/v1/assets/${encodeURIComponent(id)}`)
  }

  return {
    regions,
    loading,
    loaded,
    error,
    availableRegionIds,
    loadRegions,
    downloadRegion,
    cancelRegion,
    deleteRegion,
    fetchClusters,
    suggestPlaces,
    applyPlace,
    loadAsset
  }
})
