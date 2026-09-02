import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import { ApiProblem, apiFetch } from '@/api/client'
import type { TimelineAsset } from '@/api/timeline'
// Not a component: `i18n.global` directly, same pattern as
// `stores/toast.ts`.
import { i18n } from '@/i18n'

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
  /** Id of `cover_asset_id`'s folder — for "Open folder" from the map
   * popover without a second request. */
  folder_id: string
  /** Human-readable place label (reverse geocoding) — absent
   * (`serde(skip_serializing_if)`, never `null`) until the cover asset
   * has a place assigned. */
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

export interface RegionCatalogEntry {
  id: string
  label: string
  approx_size_bytes: number
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
    'keeppix/unknown-region-catalog-id': 'maps.errors.unknownCatalogId',
    'keeppix/conflict': 'maps.errors.downloadAlreadyActive'
  }
  return keys[error.type] ?? 'common.unexpectedError'
}

function replaceRegion(regions: MapRegion[], region: MapRegion): MapRegion[] {
  return [...regions.filter((item) => item.id !== region.id), region]
}

/** A region/catalog entry's `label` is stored once, in Italian, at
 * download time (`map_catalog::CATALOG`) — this resolves the viewer's
 * own locale from `maps.regions.<id>` (it.json/en.json) when one exists,
 * falling back to the stored label otherwise (e.g. a region added
 * through the advanced manual-URL form, whose id isn't necessarily an
 * ISO country code at all). */
export function regionDisplayLabel(id: string, fallbackLabel: string): string {
  const key = `maps.regions.${id}`
  return i18n.global.te(key) ? i18n.global.t(key) : fallbackLabel
}

export const useMapsStore = defineStore('maps', () => {
  const regions = ref<MapRegion[]>([])
  const loading = ref(false)
  const loaded = ref(false)
  const error = ref<unknown>()
  const catalog = ref<RegionCatalogEntry[]>([])
  const catalogLoaded = ref(false)

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

  /** The 35-country search catalog (`docs/ui/documento-funzionale-ui.md`,
   * "B — Ricerca di regione") — fetched once and cached: it's a fixed,
   * small server-side list, not per-user state. */
  async function loadCatalog(): Promise<void> {
    if (catalogLoaded.value) return
    catalog.value = await apiFetch<RegionCatalogEntry[]>('/api/v1/map/regions/catalog')
    catalogLoaded.value = true
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

  /** The search-box counterpart to `downloadRegion`: a catalog id instead
   * of a hand-typed URL/checksum. Returns the queued region so callers
   * can show its name without a second lookup. */
  async function downloadFromCatalog(id: string): Promise<MapRegion> {
    const region = await apiFetch<MapRegion>(
      `/api/v1/map/regions/catalog/${encodeURIComponent(id)}`,
      { method: 'POST' }
    )
    regions.value = replaceRegion(regions.value, region)
    return region
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
    catalog,
    catalogLoaded,
    availableRegionIds,
    loadRegions,
    loadCatalog,
    downloadRegion,
    downloadFromCatalog,
    cancelRegion,
    deleteRegion,
    fetchClusters,
    suggestPlaces,
    applyPlace,
    loadAsset,
    reverseGeocode
  }
})
