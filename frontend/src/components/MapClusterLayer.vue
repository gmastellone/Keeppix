<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import type {
  Map as LibreMap,
  Marker as LibreMarker,
  StyleSpecification
} from 'maplibre-gl'
import type { RangeResponse, Source } from 'pmtiles'

import { ApiProblem } from '@/api/client'
import { fetchAllFolders } from '@/api/folders'
import { thumbSrc } from '@/api/media'
import type { MapBounds, MapCluster, MapScope } from '@/stores/maps'
import { mapErrorKey, useMapsStore } from '@/stores/maps'

const props = withDefaults(
  defineProps<{
    scope: MapScope
    scopeId: string | string[]
    regionIds: string[]
    center?: { lat: number; lon: number }
    compact?: boolean
    allowDraw?: boolean
  }>(),
  {
    center: undefined,
    compact: false,
    allowDraw: false
  }
)

const emit = defineEmits<{
  'asset-click': [id: string]
  'area-selected': [bounds: MapBounds]
  /** §27, "Apri cartella" — nessuna vista "Foto scoperta su una
   * cartella" esiste ancora nell'app reale (stessa lacuna già
   * dichiarata in `SearchView.vue`, Task 9 3/N): il chiamante decide
   * dove andare, oggi sempre `/folders`. */
  'open-folder': [id: string]
}>()

const { t } = useI18n()
const maps = useMapsStore()
const container = ref<HTMLElement>()
const drawing = ref(false)
const errorKey = ref('')

// --- popover di un marker aggregato (§27) — solo fuori da `compact`
// (la mini-mappa del lightbox è alta 176px con `overflow-hidden`: una
// card popover da 76px di sola copertina ci starebbe a stento e
// verrebbe tagliata; lì il click su un marker aggregato resta il solo
// zoom, comportamento preesistente invariato). Un marker **non**
// aggregato apre ancora la foto direttamente (`asset-click`,
// comportamento preesistente): il popover di §27 rappresenta una
// cartella/luogo con più foto, non una singola foto — un punto già
// alla granularità minima non ha nulla in più da mostrare che aprirlo.
const popoverCluster = ref<MapCluster | null>(null)
const popoverPos = ref<{ x: number; y: number } | null>(null)
const popoverFolderName = ref('')
const popoverButtonEl = ref<HTMLButtonElement | null>(null)
let popoverMarkerEl: HTMLElement | null = null
const folderNames = new Map<string, string>()
let folderNamesLoaded = false

let map: LibreMap | undefined
let markers: LibreMarker[] = []
let themeQuery: MediaQueryList | undefined
let drawStart: { lat: number; lon: number } | undefined
let requestSequence = 0
let maplibreModule: typeof import('maplibre-gl') | undefined
let pmtilesModule: typeof import('pmtiles') | undefined
let pmtilesProtocol: import('pmtiles').Protocol | undefined
const coverHashes = new Map<string, string>()

class ProblemRangeSource implements Source {
  readonly url: string

  constructor(url: string) {
    this.url = url
  }

  getKey(): string {
    return this.url
  }

  async getBytes(
    offset: number,
    length: number,
    signal?: AbortSignal
  ): Promise<RangeResponse> {
    const response = await fetch(this.url, {
      credentials: 'same-origin',
      headers: { range: `bytes=${offset}-${offset + length - 1}` },
      signal
    })
    if (!response.ok) {
      const contentType = response.headers.get('content-type') ?? ''
      if (contentType.includes('application/problem+json')) {
        const body = await response.json()
        throw new ApiProblem(body.type, body.title, body.status, body.detail)
      }
      throw new ApiProblem('keeppix/unexpected', response.statusText, response.status)
    }
    return {
      data: await response.arrayBuffer(),
      etag: response.headers.get('etag') ?? undefined,
      cacheControl: response.headers.get('cache-control') ?? undefined,
      expires: response.headers.get('expires') ?? undefined
    }
  }
}

function colors(dark: boolean) {
  return dark
    ? {
        background: '#171717',
        land: '#252525',
        water: '#183449',
        roads: '#525252',
        buildings: '#3f3f46'
      }
    : {
        background: '#f5f4f0',
        land: '#e9e7df',
        water: '#b7dce8',
        roads: '#ffffff',
        buildings: '#d6d3cc'
      }
}

function archiveUrl(regionId: string): string {
  return `${window.location.origin}/api/v1/map/tiles/${encodeURIComponent(regionId)}/0/0/0`
}

function registerRegionSources() {
  if (!pmtilesModule || !pmtilesProtocol) return
  for (const regionId of props.regionIds) {
    pmtilesProtocol.add(
      new pmtilesModule.PMTiles(new ProblemRangeSource(archiveUrl(regionId)))
    )
  }
}

function mapStyle(dark: boolean): StyleSpecification {
  const palette = colors(dark)
  const sources: StyleSpecification['sources'] = {}
  const layers: StyleSpecification['layers'] = [
    {
      id: 'keeppix-background',
      type: 'background',
      paint: { 'background-color': palette.background }
    }
  ]

  for (const regionId of props.regionIds) {
    const source = `region-${regionId}`
    sources[source] = { type: 'vector', url: `pmtiles://${archiveUrl(regionId)}` }
    layers.push(
      {
        id: `${source}-earth`,
        type: 'fill',
        source,
        'source-layer': 'earth',
        paint: { 'fill-color': palette.land }
      },
      {
        id: `${source}-water`,
        type: 'fill',
        source,
        'source-layer': 'water',
        paint: { 'fill-color': palette.water }
      },
      {
        id: `${source}-buildings`,
        type: 'fill',
        source,
        'source-layer': 'buildings',
        minzoom: 13,
        paint: { 'fill-color': palette.buildings }
      },
      {
        id: `${source}-roads`,
        type: 'line',
        source,
        'source-layer': 'roads',
        paint: { 'line-color': palette.roads, 'line-width': 1.2 }
      }
    )
  }

  return {
    version: 8,
    name: dark ? 'Keeppix dark' : 'Keeppix light',
    sources,
    layers
  }
}

function clearMarkers() {
  for (const marker of markers) marker.remove()
  markers = []
}

async function coverHash(assetId: string): Promise<string | undefined> {
  const cached = coverHashes.get(assetId)
  if (cached) return cached
  try {
    const asset = await maps.loadAsset(assetId)
    if (asset.content_hash) coverHashes.set(assetId, asset.content_hash)
    return asset.content_hash ?? undefined
  } catch {
    return undefined
  }
}

async function folderName(folderId: string): Promise<string> {
  if (!folderNamesLoaded) {
    folderNamesLoaded = true
    try {
      for (const folder of await fetchAllFolders()) folderNames.set(folder.id, folder.name)
    } catch {
      // Riprovato al prossimo popover — `folderNamesLoaded` resta true
      // solo se la richiesta è andata a buon fine, per non insistere a
      // vuoto a ogni marker cliccato in caso di errore di rete.
      folderNamesLoaded = false
    }
  }
  return folderNames.get(folderId) ?? ''
}

function updatePopoverPosition() {
  if (!map || !popoverCluster.value) return
  const p = map.project([popoverCluster.value.lon, popoverCluster.value.lat])
  popoverPos.value = { x: p.x, y: p.y }
}

async function openClusterPopover(cluster: MapCluster, markerEl: HTMLElement) {
  popoverCluster.value = cluster
  popoverMarkerEl = markerEl
  updatePopoverPosition()
  popoverFolderName.value = await folderName(cluster.folder_id)
  await nextTick()
  popoverButtonEl.value?.focus()
}

function closePopover(returnFocus: boolean) {
  if (!popoverCluster.value) return
  const marker = popoverMarkerEl
  popoverCluster.value = null
  popoverPos.value = null
  popoverMarkerEl = null
  if (returnFocus) marker?.focus()
}

function onPopoverKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.stopPropagation()
    closePopover(true)
  }
}

function openFolder() {
  if (!popoverCluster.value) return
  emit('open-folder', popoverCluster.value.folder_id)
  closePopover(false)
}

async function refreshClusters() {
  if (!map || !maplibreModule) return
  const bounds = map.getBounds()
  const sequence = ++requestSequence
  const scopeIds = Array.isArray(props.scopeId) ? props.scopeId : [props.scopeId]
  let clusters: MapCluster[]
  try {
    const requests = scopeIds.map((scopeId) =>
      maps.fetchClusters(
        {
          west: bounds.getWest(),
          south: bounds.getSouth(),
          east: bounds.getEast(),
          north: bounds.getNorth()
        },
        Math.round(map!.getZoom()),
        props.scope,
        scopeId
      )
    )
    clusters = (await Promise.all(requests)).flat()
    if (sequence === requestSequence) errorKey.value = ''
  } catch (cause) {
    if (sequence === requestSequence) errorKey.value = mapErrorKey(cause)
    return
  }
  if (sequence !== requestSequence || !map) return
  const covers = await Promise.all(
    clusters.map((cluster) => coverHash(cluster.cover_asset_id))
  )
  if (sequence !== requestSequence || !map) return

  clearMarkers()
  markers = clusters.map((cluster, index) => {
    const element = document.createElement('button')
    element.type = 'button'
    element.className =
      'relative grid h-12 min-w-12 place-items-center overflow-hidden rounded-full border-2 border-white bg-accent px-2 text-xs font-semibold text-white shadow'
    const hash = covers[index]
    if (hash) {
      const image = document.createElement('img')
      image.src = thumbSrc(hash)
      image.alt = ''
      image.className = 'absolute inset-0 h-full w-full object-cover'
      element.append(image)
    }
    const count = document.createElement('span')
    count.className = 'relative rounded-full bg-black/65 px-1.5 py-0.5'
    count.textContent = String(cluster.count)
    element.append(count)
    element.setAttribute(
      'aria-label',
      cluster.clustered
        ? t('maps.cluster', { count: cluster.count })
        : t('maps.openPhoto')
    )
    element.addEventListener('click', (event) => {
      if (!map) return
      closePopover(false)
      if (cluster.clustered && !props.compact) {
        event.stopPropagation()
        void openClusterPopover(cluster, element)
      } else if (cluster.clustered) {
        map.easeTo({
          center: [cluster.lon, cluster.lat],
          zoom: Math.min(map.getZoom() + 2, 20)
        })
      } else {
        emit('asset-click', cluster.cover_asset_id)
      }
    })
    return new maplibreModule!.Marker({ element })
      .setLngLat([cluster.lon, cluster.lat])
      .addTo(map!)
  })
}

function mapFailure(event: unknown) {
  const cause =
    typeof event === 'object' && event !== null && 'error' in event
      ? (event as { error: unknown }).error
      : event
  errorKey.value = mapErrorKey(cause, 'tile')
}

function switchTheme(event: MediaQueryListEvent) {
  map?.setStyle(mapStyle(event.matches))
}

function toggleDraw() {
  drawing.value = !drawing.value
  if (!drawing.value) {
    drawStart = undefined
    map?.dragPan.enable()
  }
}

function pointerCoordinate(event: PointerEvent): { lat: number; lon: number } | undefined {
  if (!map) return undefined
  const point = map.unproject([event.offsetX, event.offsetY])
  return { lat: point.lat, lon: point.lng }
}

function startDrawing(event: PointerEvent) {
  if (!drawing.value) return
  event.preventDefault()
  map?.dragPan.disable()
  drawStart = pointerCoordinate(event)
}

function finishDrawing(event: PointerEvent) {
  if (!drawing.value || !drawStart) return
  const end = pointerCoordinate(event)
  if (!end) return
  emit('area-selected', {
    west: Math.min(drawStart.lon, end.lon),
    south: Math.min(drawStart.lat, end.lat),
    east: Math.max(drawStart.lon, end.lon),
    north: Math.max(drawStart.lat, end.lat)
  })
  drawStart = undefined
  drawing.value = false
  map?.dragPan.enable()
}

onMounted(async () => {
  const [maplibre, pmtiles] = await Promise.all([
    import('maplibre-gl'),
    import('pmtiles'),
    import('maplibre-gl/dist/maplibre-gl.css')
  ])
  if (!container.value) return
  maplibreModule = maplibre
  pmtilesModule = pmtiles
  if (!pmtilesProtocol) {
    pmtilesProtocol = new pmtiles.Protocol()
    maplibre.addProtocol('pmtiles', pmtilesProtocol.tile)
  }
  registerRegionSources()

  themeQuery = window.matchMedia('(prefers-color-scheme: dark)')
  themeQuery.addEventListener('change', switchTheme)
  map = new maplibre.Map({
    container: container.value,
    style: mapStyle(themeQuery.matches),
    center: props.center ? [props.center.lon, props.center.lat] : [12, 42],
    zoom: props.center ? 11 : 4,
    attributionControl: props.compact ? false : {}
  })
  map.on('moveend', refreshClusters)
  map.on('load', refreshClusters)
  map.on('error', mapFailure)
  if (!props.compact) {
    // §26/§27: "clic altrove chiude" — un clic sulla base della mappa
    // (non su un marker, che chiude e riapre per conto proprio) chiude
    // il popover; l'inizio di un trascinamento fa lo stesso, altrimenti
    // il popover resterebbe fermo sullo schermo mentre la mappa scorre
    // sotto — non è previsto un riposizionamento continuo durante il
    // pan, solo alla fine (`moveend` → `refreshClusters`).
    map.on('click', () => closePopover(false))
    map.on('movestart', () => closePopover(false))
  }
  await refreshClusters()
})

onBeforeUnmount(() => {
  requestSequence += 1
  themeQuery?.removeEventListener('change', switchTheme)
  map?.off('moveend', refreshClusters)
  map?.off('load', refreshClusters)
  map?.off('error', mapFailure)
  clearMarkers()
  map?.remove()
})

watch(
  () => props.regionIds,
  () => {
    registerRegionSources()
    if (map && themeQuery) map.setStyle(mapStyle(themeQuery.matches))
  }
)
</script>

<template>
  <div
    class="relative overflow-hidden rounded-lg border border-border"
    :class="compact ? 'h-44' : 'h-full min-h-96'"
  >
    <div
      ref="container"
      class="h-full w-full"
      :class="{ 'cursor-crosshair': drawing }"
      @pointerdown="startDrawing"
      @pointerup="finishDrawing"
    />
    <button
      v-if="allowDraw"
      type="button"
      class="absolute left-3 top-3 rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm shadow"
      :aria-pressed="drawing"
      @click="toggleDraw"
    >
      {{ drawing ? t('maps.drawCancel') : t('maps.drawArea') }}
    </button>
    <p
      v-if="errorKey"
      class="absolute bottom-3 left-3 right-3 z-10 rounded-lg bg-surface-elevated p-3 text-sm text-danger shadow"
      role="alert"
    >
      {{ t(errorKey) }}
    </p>

    <!-- §27, popover di un marker aggregato: cartella/luogo, non una
         singola foto — vedi il commento sopra `popoverCluster` per il
         perché non appare in modalità `compact`. -->
    <div
      v-if="popoverCluster && popoverPos"
      role="dialog"
      :aria-label="popoverFolderName || t('maps.cluster', { count: popoverCluster.count })"
      class="absolute z-20 w-[190px] -translate-x-1/2 overflow-hidden rounded-[10px] border border-border bg-surface-elevated shadow-lg"
      :style="{ left: `${popoverPos.x}px`, top: `${popoverPos.y + 18}px` }"
      @keydown="onPopoverKeydown"
    >
      <img
        v-if="coverHashes.get(popoverCluster.cover_asset_id)"
        :src="thumbSrc(coverHashes.get(popoverCluster.cover_asset_id)!)"
        alt=""
        class="h-[76px] w-full object-cover"
      >
      <div
        v-else
        class="h-[76px] w-full bg-chip-bg"
      />
      <div class="p-2">
        <p class="truncate text-[13px] font-bold">
          {{ popoverFolderName }}
        </p>
        <p class="truncate text-[11px] text-content-muted">
          {{
            popoverCluster.place_label
              ? t('maps.popoverSubtitle', { count: popoverCluster.count, place: popoverCluster.place_label })
              : t('maps.cluster', { count: popoverCluster.count })
          }}
        </p>
        <button
          ref="popoverButtonEl"
          type="button"
          class="mt-2 w-full rounded-lg border border-border py-1.5 text-center text-[12.5px] font-semibold hover:bg-border/40"
          @click="openFolder"
        >
          {{ t('maps.openFolder') }}
        </button>
      </div>
    </div>
  </div>
</template>
