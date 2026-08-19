<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import type {
  Map as LibreMap,
  Marker as LibreMarker,
  StyleSpecification
} from 'maplibre-gl'

import type { MapBounds, MapScope } from '@/stores/maps'
import { useMapsStore } from '@/stores/maps'
import { thumbSrc } from '@/api/media'

const props = withDefaults(
  defineProps<{
    scope: MapScope
    scopeId: string
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
}>()

const { t } = useI18n()
const maps = useMapsStore()
const container = ref<HTMLElement>()
const drawing = ref(false)

let map: LibreMap | undefined
let markers: LibreMarker[] = []
let themeQuery: MediaQueryList | undefined
let drawStart: { lat: number; lon: number } | undefined
let requestSequence = 0
let maplibreModule: typeof import('maplibre-gl') | undefined
let protocolRegistered = false
const coverHashes = new Map<string, string>()

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
    const archive = `${window.location.origin}/api/v1/map/tiles/${encodeURIComponent(regionId)}/0/0/0`
    sources[source] = { type: 'vector', url: `pmtiles://${archive}` }
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

async function refreshClusters() {
  if (!map || !maplibreModule) return
  const bounds = map.getBounds()
  const sequence = ++requestSequence
  const clusters = await maps.fetchClusters(
    {
      west: bounds.getWest(),
      south: bounds.getSouth(),
      east: bounds.getEast(),
      north: bounds.getNorth()
    },
    Math.round(map.getZoom()),
    props.scope,
    props.scopeId
  )
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
    element.addEventListener('click', () => {
      if (!map) return
      if (cluster.clustered) {
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
  const [maplibre, { Protocol }] = await Promise.all([
    import('maplibre-gl'),
    import('pmtiles'),
    import('maplibre-gl/dist/maplibre-gl.css')
  ])
  if (!container.value) return
  maplibreModule = maplibre
  if (!protocolRegistered) {
    const protocol = new Protocol()
    maplibre.addProtocol('pmtiles', protocol.tile)
    protocolRegistered = true
  }

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
  await refreshClusters()
})

onBeforeUnmount(() => {
  requestSequence += 1
  themeQuery?.removeEventListener('change', switchTheme)
  map?.off('moveend', refreshClusters)
  map?.off('load', refreshClusters)
  clearMarkers()
  map?.remove()
})

watch(
  () => props.regionIds,
  () => {
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
  </div>
</template>
