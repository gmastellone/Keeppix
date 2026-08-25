import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import { ApiProblem, apiFetch } from '@/api/client'
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
  /** Id della cartella di `cover_asset_id` — per "Apri cartella" dal
   * popover della mappa (Task 10, §27) senza una seconda richiesta. */
  folder_id: string
  /** Etichetta leggibile del luogo (geocodifica inversa) — assente
   * (`serde(skip_serializing_if)`, mai `null`) finché l'asset di
   * copertina non ha un luogo assegnato. */
  place_label?: string
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
  source_url: string
  checksum_sha256: string
}

export interface RegionDownloadRequest {
  id: string
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

export function mapErrorKey(error: unknown, context?: 'tile'): string {
  if (!(error instanceof ApiProblem)) return 'common.unexpectedError'
  if (context === 'tile' && error.status === 404 && error.type.startsWith('keeppix/')) {
    return 'maps.errors.regionUnavailable'
  }
  const keys: Record<string, string> = {
    'keeppix/service-unavailable': 'common.unavailable',
    'keeppix/unauthenticated': 'maps.errors.unauthenticated',
    'keeppix/forbidden': 'maps.errors.forbidden',
    'keeppix/region-source-not-allowed': 'maps.errors.regionSourceNotAllowed',
    'keeppix/invalid-region': 'maps.errors.invalidRegion',
    'keeppix/conflict': 'maps.errors.downloadAlreadyActive'
  }
  return keys[error.type] ?? 'common.unexpectedError'
}

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

  async function downloadRegion(entry: RegionDownloadRequest): Promise<void> {
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

  async function reverseGeocode(lat: number, lon: number): Promise<Place | null> {
    const params = new URLSearchParams({ lat: String(lat), lon: String(lon) })
    const result = await apiFetch<Place | null>(`/api/v1/places/reverse?${params}`)
    return result
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
    loadAsset,
    reverseGeocode
  }
})
