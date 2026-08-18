<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import {
  fetchBuckets,
  fetchPage,
  promoteViewport,
  type MonthBucket,
  type TimelineAsset
} from '@/api/timeline'
import { startLiveEvents, type LiveSocket } from '@/api/events'
import { thumbSrc as mediaThumbSrc } from '@/api/media'
import Button from '@/components/ui/Button.vue'
import AssetViewer from '@/components/AssetViewer.vue'
import { useCullingStore } from '@/stores/culling'
import { useSessionStore } from '@/stores/session'
import { clampDensity, justify, bucketMinHeight } from '@/timeline/justify'
import { monthAtOffset, yearLabel } from '@/timeline/scrubber'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

const DENSITY_KEY = 'keeppix.density'
const KIND_KEY = 'keeppix.kindFilter'

type KindFilter = 'all' | 'photo' | 'video'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()
const culling = useCullingStore()

const buckets = ref<MonthBucket[]>([])
const assetsByBucket = ref<Record<string, TimelineAsset[]>>({})
const density = ref(clampDensity(Number(localStorage.getItem(DENSITY_KEY) ?? 6)))
const kind = ref<KindFilter>((localStorage.getItem(KIND_KEY) as KindFilter) || 'all')
const gridWidth = ref(800)
const gridEl = ref<HTMLElement | null>(null)
const scrubY = ref<string | undefined>()
const empty = ref(false)
const query = ref('')
const viewing = ref<TimelineAsset | null>(null)

const visibleHashes = new Set<string>()
const placeholders = new Map<string, string>()
let promoteTimer: ReturnType<typeof setTimeout> | undefined
let live: LiveSocket | undefined

function isKind(asset: TimelineAsset): boolean {
  if (kind.value === 'all') return true
  if (kind.value === 'video') return asset.kind === 'video'
  return asset.kind === 'image' || asset.kind === 'raw_image'
}

const days = computed(() => {
  const items: { key: string; label: string; assets: TimelineAsset[] }[] = []
  for (const bucket of buckets.value) {
    const assets = (assetsByBucket.value[bucket.month] ?? []).filter(isKind)
    const groups = new Map<string, TimelineAsset[]>()
    for (const asset of assets) {
      const day = asset.taken_at_utc?.slice(0, 10) ?? bucket.month
      const list = groups.get(day) ?? []
      list.push(asset)
      groups.set(day, list)
    }
    for (const [key, group] of groups) {
      items.push({ key, label: key, assets: group })
    }
  }
  return items
})

const daysByMonth = computed(() => {
  const map = new Map<string, typeof days.value>()
  for (const day of days.value) {
    const month = day.key.slice(0, 7)
    const list = map.get(month) ?? []
    list.push(day)
    map.set(month, list)
  }
  return map
})

const grids = computed(() => {
  const out = new Map<string, ReturnType<typeof cellsFor>>()
  for (const day of days.value) {
    out.set(day.key, cellsFor(day.assets))
  }
  return out
})

const flatAssets = computed(() => days.value.flatMap((d) => d.assets))

function viewingNeighbour(delta: number): TimelineAsset | undefined {
  const list = flatAssets.value
  const i = list.findIndex((a) => a.id === viewing.value?.id)
  if (i < 0) return undefined
  return list[i + delta]
}

function rowHeight(): number {
  return Math.max(48, gridWidth.value / density.value)
}

function cellsFor(assets: TimelineAsset[]) {
  const byId = new Map(assets.map((asset) => [asset.id, asset]))
  return justify(
    assets.map((a) => ({
      id: a.id,
      width: a.width ?? 3,
      height: a.height ?? 2
    })),
    gridWidth.value,
    rowHeight()
  ).map((row) => ({
    ...row,
    cells: row.cells.flatMap((cell) => {
      const asset = byId.get(cell.id)
      return asset ? [{ ...cell, asset, placeholder: placeholder(asset) }] : []
    })
  }))
}

function placeholder(asset: TimelineAsset): string | undefined {
  if (!asset.thumbhash) return undefined
  const cached = placeholders.get(asset.id)
  if (cached) return cached
  const url = thumbhashToDataURL(asset.thumbhash)
  if (!url) return undefined
  placeholders.set(asset.id, url)
  return url
}

function reserveHeight(bucket: MonthBucket): number {
  return bucketMinHeight(bucket.count, gridWidth.value, density.value)
}

function thumbSrc(asset: TimelineAsset): string | undefined {
  return asset.content_hash ? mediaThumbSrc(asset.content_hash) : undefined
}

async function signOut() {
  await session.logout()
  await router.push('/login')
}

async function goSearch() {
  await router.push({ path: '/search', query: { q: query.value } })
}

/**
 * Unico punto d'ingresso al culling (spec §4.2, hard rule del piano): un
 * pulsante, non scorciatoie sparse. In assenza di una vista per cartella o
 * di una selezione multipla in questa fase del frontend, l'insieme di
 * lavoro è quanto già caricato in timeline — coerente con "cartella o
 * selezione" finché quelle viste non esistono.
 */
async function startCulling() {
  culling.start(flatAssets.value)
  await router.push('/culling')
}

function setDensity(next: number) {
  density.value = clampDensity(next)
  localStorage.setItem(DENSITY_KEY, String(density.value))
}

function setKind(next: KindFilter) {
  kind.value = next
  localStorage.setItem(KIND_KEY, next)
}

async function loadBucket(month: string) {
  if (assetsByBucket.value[month]) return
  const collected: TimelineAsset[] = []
  let cursor: string | undefined
  do {
    const page = await fetchPage(month, cursor)
    collected.push(...page.assets)
    cursor = page.next_cursor
  } while (cursor)
  assetsByBucket.value = { ...assetsByBucket.value, [month]: collected }
}

async function refreshTimeline() {
  const list = await fetchBuckets()
  buckets.value = list
  empty.value = list.length === 0
  assetsByBucket.value = {}
  await Promise.all(list.slice(0, 2).map((b) => loadBucket(b.month)))
}

function onScrubMove(event: MouseEvent) {
  if (event.buttons & 1) onScrub(event)
}

function stepViewer(delta: number) {
  const next = viewingNeighbour(delta)
  if (next) viewing.value = next
}

function onScrub(event: MouseEvent) {
  const track = event.currentTarget as HTMLElement
  const rect = track.getBoundingClientRect()
  const month = monthAtOffset(buckets.value, event.clientY - rect.top, rect.height)
  scrubY.value = month
  if (month) {
    document.getElementById(`bucket-${month}`)?.scrollIntoView({ block: 'start' })
  }
}

let observer: IntersectionObserver | undefined

function schedulePromote() {
  if (promoteTimer) clearTimeout(promoteTimer)
  promoteTimer = setTimeout(() => {
    void promoteViewport([...visibleHashes].slice(0, 200))
  }, 250)
}

function observe() {
  observer?.disconnect()
  if (typeof IntersectionObserver === 'undefined') return
  observer = new IntersectionObserver(
    (entries) => {
      let hashesChanged = false
      for (const entry of entries) {
        const month = (entry.target as HTMLElement).dataset.month
        if (entry.isIntersecting && month) {
          void loadBucket(month)
        }
        const hash = (entry.target as HTMLElement).dataset.hash
        if (hash) {
          hashesChanged = true
          if (entry.isIntersecting) visibleHashes.add(hash)
          else visibleHashes.delete(hash)
        }
      }
      if (hashesChanged) schedulePromote()
    },
    { root: gridEl.value, rootMargin: '200px' }
  )
  nextTick(() => {
    gridEl.value?.querySelectorAll('[data-month],[data-hash]').forEach((el) => observer?.observe(el))
  })
}

function measure() {
  if (gridEl.value) gridWidth.value = gridEl.value.clientWidth
}

onMounted(async () => {
  measure()
  await refreshTimeline()
  observe()
  window.addEventListener('resize', measure)
  live = startLiveEvents((msg) => {
    if (
      msg.type === 'resync' ||
      msg.type === 'assets.upserted' ||
      msg.type === 'assets.deleted'
    ) {
      void refreshTimeline()
    }
  })
})

onUnmounted(() => {
  live?.close()
  observer?.disconnect()
  if (promoteTimer) clearTimeout(promoteTimer)
  window.removeEventListener('resize', measure)
})

watch([days, density, gridWidth], () => observe())
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="flex flex-wrap items-center gap-3 border-b border-border px-4 py-3">
      <h1 class="text-lg font-semibold">
        {{ t('home.greeting', { name: session.user?.display_name ?? '' }) }}
      </h1>
      <form
        class="flex min-w-48 flex-1 gap-2"
        @submit.prevent="goSearch"
      >
        <input
          v-model="query"
          class="w-full rounded-lg border border-border bg-surface-elevated px-3 py-1.5 text-sm"
          :placeholder="t('search.placeholder')"
          type="search"
        >
      </form>
      <RouterLink
        class="text-sm underline"
        to="/folders"
      >
        {{ t('folders.entry') }}
      </RouterLink>
      <RouterLink
        class="text-sm underline"
        to="/trash"
      >
        {{ t('trash.entry') }}
      </RouterLink>
      <RouterLink
        class="text-sm underline"
        to="/albums"
      >
        {{ t('albums.entry') }}
      </RouterLink>
      <RouterLink
        class="text-sm underline"
        to="/shares"
      >
        {{ t('shares.entry') }}
      </RouterLink>
      <RouterLink
        v-if="session.user?.role === 'admin'"
        class="text-sm underline"
        to="/users"
      >
        {{ t('users.entry') }}
      </RouterLink>
      <RouterLink
        v-if="session.user?.role === 'admin'"
        class="text-sm underline"
        to="/groups"
      >
        {{ t('groups.entry') }}
      </RouterLink>
      <RouterLink
        class="text-sm underline"
        to="/problems"
      >
        {{ t('problems.title') }}
      </RouterLink>
      <button
        v-if="flatAssets.length > 0"
        type="button"
        class="rounded-lg border border-border px-3 py-1 text-sm"
        @click="startCulling"
      >
        {{ t('culling.entry') }}
      </button>
      <div
        class="flex rounded-lg border border-border text-sm"
        role="group"
        :aria-label="t('timeline.kind')"
      >
        <button
          v-for="option in (['all', 'photo', 'video'] as const)"
          :key="option"
          class="px-3 py-1"
          :class="kind === option ? 'bg-accent text-white' : ''"
          @click="setKind(option)"
        >
          {{ t(`timeline.kinds.${option}`) }}
        </button>
      </div>
      <div class="ml-auto flex items-center gap-2">
        <button
          class="rounded-lg border border-border px-2 py-1"
          :aria-label="t('timeline.densityDown')"
          @click="setDensity(density - 1)"
        >
          −
        </button>
        <span class="w-4 text-center text-sm">{{ density }}</span>
        <button
          class="rounded-lg border border-border px-2 py-1"
          :aria-label="t('timeline.densityUp')"
          @click="setDensity(density + 1)"
        >
          +
        </button>
        <div class="w-28">
          <Button @click="signOut">
            {{ t('home.logout') }}
          </Button>
        </div>
      </div>
    </header>

    <p
      v-if="empty"
      class="p-6 text-content-muted"
    >
      {{ t('timeline.empty') }}
    </p>

    <div
      v-else
      class="flex min-h-0 flex-1"
    >
      <div
        ref="gridEl"
        class="min-h-0 flex-1 overflow-auto px-4 py-3"
      >
        <section
          v-for="bucket in buckets"
          :id="`bucket-${bucket.month}`"
          :key="bucket.month"
          :data-month="bucket.month"
          :style="{ minHeight: `${reserveHeight(bucket)}px` }"
        >
          <h2 class="sticky top-0 z-10 bg-surface py-2 text-sm font-medium">
            {{ bucket.month }} · {{ bucket.count }}
          </h2>
          <div
            v-for="day in daysByMonth.get(bucket.month) ?? []"
            :key="day.key"
          >
            <h3 class="py-1 text-xs text-content-muted">
              {{ day.label }}
            </h3>
            <div
              v-for="(row, index) in grids.get(day.key) ?? []"
              :key="index"
              class="relative mb-1"
              :style="{ height: `${row.height}px` }"
            >
              <article
                v-for="cell in row.cells"
                :key="cell.id"
                class="absolute cursor-pointer overflow-hidden"
                :data-hash="cell.asset.content_hash ?? undefined"
                :style="{
                  left: `${cell.x}px`,
                  width: `${cell.w}px`,
                  height: `${cell.h}px`
                }"
                @click="viewing = cell.asset"
              >
                <img
                  v-if="cell.placeholder"
                  :src="cell.placeholder"
                  alt=""
                  class="absolute inset-0 h-full w-full object-cover"
                >
                <img
                  v-if="thumbSrc(cell.asset)"
                  :src="thumbSrc(cell.asset)"
                  :alt="cell.asset.filename"
                  class="absolute inset-0 h-full w-full object-cover"
                  :width="cell.w"
                  :height="cell.h"
                >
              </article>
            </div>
          </div>
        </section>
      </div>
      <aside
        class="relative w-10 shrink-0 cursor-ns-resize border-l border-border"
        @mousedown="onScrub"
        @mousemove="onScrubMove"
      >
        <div
          v-for="bucket in buckets"
          :key="bucket.month"
          class="px-1 text-[10px] text-content-muted"
        >
          {{ yearLabel(bucket.month) }}
        </div>
        <p
          v-if="scrubY"
          class="absolute right-full mr-1 rounded bg-surface-elevated px-1 text-xs"
        >
          {{ scrubY }}
        </p>
      </aside>
    </div>
    <AssetViewer
      v-if="viewing"
      :asset="viewing"
      :prev="viewingNeighbour(-1)"
      :next="viewingNeighbour(1)"
      @close="viewing = null"
      @prev="stepViewer(-1)"
      @next="stepViewer(1)"
    />
  </div>
</template>
