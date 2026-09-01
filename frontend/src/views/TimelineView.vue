<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { fetchBuckets, fetchGeometry, fetchPage, promoteViewport, type MonthBucket, type TimelineAsset } from '@/api/timeline'
import { ApiProblem } from '@/api/client'
import { startLiveEvents, type LiveSocket } from '@/api/events'
import { thumbSrc as mediaThumbSrc } from '@/api/media'
import FlatAssetGrid from '@/components/FlatAssetGrid.vue'
import ErrorState from '@/components/ui/ErrorState.vue'
import PhotoTile, { type StackType } from '@/components/ui/PhotoTile.vue'
import QuickFilter from '@/components/ui/QuickFilter.vue'
import SelectAllVisible from '@/components/ui/SelectAllVisible.vue'
import SelectionBar from '@/components/ui/SelectionBar.vue'
import AssetViewer from '@/components/AssetViewer.vue'
import LibrarySelectionActions from '@/components/LibrarySelectionActions.vue'
import { useBrowseFilters } from '@/composables/useBrowseFilters'
import { useDebouncedCallback } from '@/composables/useDebouncedCallback'
import { useDensity } from '@/composables/useDensity'
import { useIsMobile } from '@/composables/useIsMobile'
import { useLightboxRoute } from '@/composables/useLightboxRoute'
import { useScrollRestoration } from '@/composables/useScrollRestoration'
import { useFavoritesStore } from '@/stores/favorites'
import { useMapsStore } from '@/stores/maps'
import { useSelectionStore } from '@/stores/selection'
import { classifyError } from '@/errors/classify'
import { activeFilterCount } from '@/design/quickFilter'
import { TimelineGeometry } from '@/timeline/geometry'
import { LruPageCache } from '@/timeline/pageCache'
import { monthAbbrev, monthAtOffset, monthFull } from '@/timeline/scrubber'
import { planStream, STREAM_OVERSCAN, type GridCell, type GridRow, type StreamRow } from '@/timeline/stream'
import { thumbhashToDataURL } from '@/timeline/thumbhash'
import { RowVirtualizer } from '@/timeline/virtualize'

// The real-scale timeline. Two behaviors from a previous implementation
// were not carried forward: day-level grouping inside a month (no such
// grouping exists here) and a sticky month header during scroll (month
// headers just scroll away normally). The "All/Photos/Videos" filter was
// removed along with them: it isn't one of the quick-filter's six
// dimensions (which uses "Type" for RAW/JPEG, a different axis), and it
// has no structural place here — the geometry that drives the layout
// doesn't carry the shot's kind, only w/h/month.

// Density belongs in Settings, not a view-level control. The +/- here is
// a temporary stand-in until that Settings view exists — removing it now
// without a replacement would leave density fixed at 6 for everyone.
// Extracted into `useDensity()` once a second consumer appeared
// (Favorites).
/** Up to 50 resident months, ~10,000 assets expected — only the pages,
 * never the geometry itself, which lives outside this cache and is never
 * evicted. */
const PAGE_CACHE_CAPACITY = 50

const { t, locale } = useI18n()
const route = useRoute()
const router = useRouter()
const maps = useMapsStore()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const { isMobile } = useIsMobile()

const buckets = ref<MonthBucket[]>([])
// shallowRef, not ref: TimelineGeometry wraps a DataView and must never
// go through UnwrapRef (which would tear the instance apart field by
// field, losing the private `view` field) — it's also semantically
// correct, since an immutable binary blob doesn't need deep reactivity
// once loaded.
const geometry = shallowRef<TimelineGeometry | null>(null)
const geometryEtag = ref<string | null>(null)
// Only this mount's very first refreshTimeline() paginates the geometry:
// it's the only truly cold-screen case, where load time on a slow
// network actually matters. Subsequent refreshes (map filter change,
// live event) stay on the full-view ETag path — the session is already
// "warm", so the extra complexity isn't worth it.
const hasLoadedGeometryOnce = ref(false)
/** ~24 KB per page (6 bytes/shot): plenty for the first screen at any
 * density, still small on a slow network. */
const FIRST_GEOMETRY_PAGE_LIMIT = 4000
/** Header only (version 1, count 0), the same binary layout as
 * `encode_geometry` — a defensive fallback in case the backend ever
 * responds with no buffer on a paginated request. */
const EMPTY_GEOMETRY_BUFFER = new Uint8Array([1, 0, 0, 0, 0, 0, 0, 0]).buffer
const pageCache = new LruPageCache<string, TimelineAsset[]>(PAGE_CACHE_CAPACITY)
const loadingMonths = new Set<string>()
/** Explicit bump: `pageCache` is an imperative class, not reactive —
 * this ref is the only way to tell Vue "something inside changed". */
const cacheTick = ref(0)

const { density, setDensity } = useDensity()
const gridEl = ref<HTMLElement | null>(null)
const contentEl = ref<HTMLElement | null>(null)
const scrubberEl = ref<HTMLElement | null>(null)
const gridWidth = ref(800)
const viewportHeight = ref(600)
const scrollTop = ref(0)
const empty = ref(false)
const loadError = ref<unknown>(null)
const placeholders = new Map<string, string>()
const bbox = computed(() => (typeof route.query.bbox === 'string' ? route.query.bbox : undefined))
const errorNature = computed(() => (loadError.value ? classifyError(loadError.value) : null))
/** Optional technical line, for whoever administers the server — only
 * when the error actually carries an RFC 9457 `Problem`, not for a
 * generic network `TypeError` that has nothing more precise to say. */
const errorDetail = computed(() =>
  loadError.value instanceof ApiProblem ? `${loadError.value.type} · ${loadError.value.status}` : undefined
)

let promoteTimer: ReturnType<typeof setTimeout> | undefined
let live: LiveSocket | undefined
let resizeObserver: ResizeObserver | undefined
let scrollRaf = 0

// During a large import, `assets.upserted` arrives once per finished
// background job — tens per second. Reacting immediately to each one used
// to trigger `refreshTimeline()`, which resets scroll to the top
// (`resetGridForNewGeometry`): the grid became unscrollable, flashing back
// to the top on every single file. Debounced: a whole burst collapses into
// one refresh once things go quiet, instead of one refresh (and one scroll
// reset) per event.
const scheduleLiveRefresh = useDebouncedCallback(() => void refreshTimeline(), 800)

const plan = computed(() => {
  if (!geometry.value) return { rows: [] as StreamRow[], rowHeights: [] as number[], totalHeight: 0 }
  return planStream(geometry.value, buckets.value, gridWidth.value, density.value)
})
const virtualizer = computed(() => new RowVirtualizer(plan.value.rowHeights))
const overscanPx = computed(() => viewportHeight.value * STREAM_OVERSCAN)
const mountedRange = computed(() =>
  virtualizer.value.visibleRange(scrollTop.value, viewportHeight.value, overscanPx.value)
)
const mountedRows = computed(() => {
  const { start, end } = mountedRange.value
  const out: { index: number; top: number; row: StreamRow }[] = []
  for (let i = start; i < end; i++) {
    const row = plan.value.rows[i]
    if (row) out.push({ index: i, top: virtualizer.value.rowTop(i), row })
  }
  return out
})
const mountedMonths = computed(() => {
  const months = new Set<string>()
  for (const entry of mountedRows.value) months.add(entry.row.month)
  return months
})

function assetFor(month: string, offsetInMonth: number): TimelineAsset | undefined {
  void cacheTick.value
  return pageCache.get(month)?.[offsetInMonth]
}

async function ensureMonthLoaded(month: string) {
  if (pageCache.has(month) || loadingMonths.has(month)) return
  loadingMonths.add(month)
  try {
    const collected: TimelineAsset[] = []
    let cursor: string | undefined
    do {
      const page = bbox.value ? await fetchPage(month, cursor, bbox.value) : await fetchPage(month, cursor)
      collected.push(...page.assets)
      cursor = page.next_cursor
    } while (cursor)
    pageCache.set(month, collected)
    cacheTick.value++
  } finally {
    loadingMonths.delete(month)
  }
}

watch(mountedMonths, (months) => {
  months.forEach((month) => void ensureMonthLoaded(month))
}, { immediate: true })

// A concatenation of the months currently resident in cache, in the same
// order as `buckets` (most recent first): the equivalent of "all photos
// loaded so far", not the whole library — used by the lightbox's
// prev/next navigation (a known limitation: at the edges of a loaded
// month, "next" may find nothing even though it exists, simply because
// it isn't in cache yet).
const loadedAssets = computed(() => {
  void cacheTick.value
  const out: TimelineAsset[] = []
  for (const bucket of buckets.value) {
    const cached = pageCache.get(bucket.month)
    if (cached) out.push(...cached)
  }
  return out
})

// The quick filter's result count is computed on this view's own list,
// here `loadedAssets` — the same "however much has been loaded so far"
// already noted above for the lightbox, not a new limitation. The
// server-side geometry blob assumes a whole month: an active filter
// switches to `FlatAssetGrid`, the same justified flat grid already used
// by Favorites, instead of trying to carve cells out of rows designed
// for a whole month.
const { selection: filterSelection, dimensions: filterDimensions, filteredAssets } = useBrowseFilters(loadedAssets)
const filterActive = computed(() => activeFilterCount(filterSelection.value) > 0)
/** The "currently on screen" set: everything loaded with no filter, only
 * what passes otherwise — governs both `FlatAssetGrid` and the
 * lightbox's prev/next navigation, which must stay within what's
 * visible (same principle already used by Favorites). */
const displayedAssets = computed(() => (filterActive.value ? filteredAssets.value : loadedAssets.value))

const lightbox = useLightboxRoute<TimelineAsset>(
  (id) => displayedAssets.value.find((asset) => asset.id === id),
  (id) => maps.loadAsset(id)
)

useScrollRestoration(gridEl)

function stepViewer(asset: TimelineAsset) {
  void lightbox.step(asset)
}

function openViewerAsset(id: string) {
  void lightbox.openById(id)
}

function placeholderFor(asset: TimelineAsset): string | undefined {
  if (!asset.thumbhash) return undefined
  const cached = placeholders.get(asset.id)
  if (cached) return cached
  const url = thumbhashToDataURL(asset.thumbhash)
  if (!url) return undefined
  placeholders.set(asset.id, url)
  return url
}

function stackTypeOf(asset: TimelineAsset): StackType {
  if (asset.raw_kind === 'raw+jpeg') return 'raw_jpeg'
  if (asset.raw_kind === 'raw') return 'raw_only'
  return 'jpeg'
}

function dateLabelOf(asset: TimelineAsset): string {
  if (!asset.taken_at_utc) return ''
  return new Intl.DateTimeFormat(locale.value, { day: 'numeric', month: 'long', year: 'numeric' }).format(
    new Date(asset.taken_at_utc)
  )
}

async function clearMapFilter() {
  const q = { ...route.query }
  delete q.bbox
  await router.replace({ path: '/', query: q })
  await refreshTimeline()
}

/**
 * No screen should assume the data is already there. A failure here
 * replaces the entire grid with `ErrorState` — it's the view's main
 * content, not a fragment — and "Retry" calls this same function again,
 * putting the data set back into a loading state and requesting it from
 * scratch.
 */
function resetGridForNewGeometry() {
  pageCache.clear()
  loadingMonths.clear()
  cacheTick.value++
  if (gridEl.value) gridEl.value.scrollTop = 0
  scrollTop.value = 0
}

async function refreshTimeline() {
  loadError.value = null
  try {
    if (hasLoadedGeometryOnce.value) {
      const [bucketList, geo] = await Promise.all([
        bbox.value ? fetchBuckets(bbox.value) : fetchBuckets(),
        fetchGeometry(bbox.value, geometryEtag.value ?? undefined)
      ])
      buckets.value = bucketList
      empty.value = bucketList.length === 0
      if (geo.buffer) {
        geometry.value = new TimelineGeometry(geo.buffer)
      }
      geometryEtag.value = geo.etag
      resetGridForNewGeometry()
      return
    }

    // First load of this mount: paginates the geometry to render without
    // waiting for the whole payload. No `ETag` here — a paginated request
    // doesn't carry one (this path is meant for a cold screen, not a
    // return visit), so the *next* refresh will do a full fetch instead
    // of a 304. Deliberate tradeoff: losing that 304 once per session
    // costs far less than the load-time block this pagination removes.
    const [bucketList, first] = await Promise.all([
      bbox.value ? fetchBuckets(bbox.value) : fetchBuckets(),
      fetchGeometry(bbox.value, undefined, { limit: FIRST_GEOMETRY_PAGE_LIMIT })
    ])
    buckets.value = bucketList
    empty.value = bucketList.length === 0
    // A paginated request never sends `If-None-Match` (see above), so the
    // real backend never responds with `304`/`buffer: null` here — an
    // empty library still returns a real buffer, just with `count: 0`.
    // The fallback below is defensive, not for an expected case.
    const firstBuffer = first.buffer ?? EMPTY_GEOMETRY_BUFFER
    geometry.value = new TimelineGeometry(firstBuffer)
    hasLoadedGeometryOnce.value = true
    resetGridForNewGeometry()

    // The rest arrives after the first render, in the background: a
    // failure here shouldn't clear a screen that's already showing
    // something correct — only the tail of the view stays incomplete
    // until a real refresh arrives (live event, filter change).
    if (first.nextCursor) {
      try {
        const chunks = [firstBuffer]
        let cursor: string | null = first.nextCursor
        while (cursor) {
          const page = await fetchGeometry(bbox.value, undefined, {
            limit: FIRST_GEOMETRY_PAGE_LIMIT,
            cursor
          })
          if (page.buffer) chunks.push(page.buffer)
          cursor = page.nextCursor
        }
        geometry.value = TimelineGeometry.concat(chunks)
      } catch (error) {
        console.warn('geometry: background completion failed', error)
      }
    }
  } catch (error) {
    loadError.value = error
  }
}

// Generation priority (POST /viewport): computed from the same exact
// virtualizer math with overscan 0 (the true visible window, not the
// mounted margin) instead of an IntersectionObserver — the geometry
// already gives the exact position and size of every row, so watching
// the DOM for the same information would be a longer route to the same
// result, not a more precise one.
const trueVisibleHashes = computed(() => {
  const { start, end } = virtualizer.value.visibleRange(scrollTop.value, viewportHeight.value, 0)
  const hashes = new Set<string>()
  for (let i = start; i < end; i++) {
    const row = plan.value.rows[i]
    if (row?.type !== 'grid') continue
    for (const cell of row.cells) {
      const hash = assetFor(row.month, cell.offsetInMonth)?.content_hash
      if (hash) hashes.add(hash)
    }
  }
  return hashes
})

watch(trueVisibleHashes, (hashes) => {
  if (promoteTimer) clearTimeout(promoteTimer)
  promoteTimer = setTimeout(() => {
    void promoteViewport([...hashes].slice(0, 200))
  }, 250)
})

function measure() {
  if (!gridEl.value) return
  viewportHeight.value = gridEl.value.clientHeight
  if (contentEl.value) gridWidth.value = contentEl.value.clientWidth
}

function onScroll() {
  if (scrollRaf) return
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0
    if (gridEl.value) scrollTop.value = gridEl.value.scrollTop
  })
}

/** A tile unmounted while it held focus would lose it, and keyboard
 * navigation would "fall back" to the top of the page. Moves focus to
 * the scroll container (`tabindex="-1"`, never in the normal tab order)
 * before Vue removes from the DOM the row that's about to leave the
 * mounted range. */
watch(mountedRange, (next) => {
  const active = document.activeElement
  if (!gridEl.value || !active || !gridEl.value.contains(active)) return
  const rowEl = active.closest('[data-row-index]')
  if (!rowEl) return
  const index = Number((rowEl as HTMLElement).dataset.rowIndex)
  if (index < next.start || index >= next.end) {
    gridEl.value.focus()
  }
})

// --- Month scrubber -----------------------------------------------
// Clicking anywhere on the bar jumps straight to the month (no need to
// drag), a label tag shows only while dragging, the jump is instant
// (never behavior:'smooth'), and it syncs back from scroll position.
// Made keyboard-reachable: arrows/Home/End with role="slider".
const dragging = ref(false)

const monthTop = computed(() => {
  const map = new Map<string, number>()
  plan.value.rows.forEach((row, i) => {
    if (row.type === 'header' && !map.has(row.month)) map.set(row.month, virtualizer.value.rowTop(i))
  })
  return map
})

function jumpToMonth(month: string) {
  const top = monthTop.value.get(month)
  if (top === undefined || !gridEl.value) return
  gridEl.value.scrollTop = top
  scrollTop.value = top
}

// The "current" month is the one actually scrolled to (the last one
// whose header has already passed the top of the viewport), not an
// estimate based on the ratio over the total scroll range: that estimate
// would always zero out the index when the loaded content is shorter
// than the viewport (a short library, or only a few pages loaded so
// far) — a real case, not just a test one — and it isn't the exact
// inverse of `jumpToMonth`, which scrolls to a precise pixel position.
const currentIndex = computed(() => {
  if (buckets.value.length === 0) return 0
  let index = 0
  for (let i = 0; i < buckets.value.length; i++) {
    const top = monthTop.value.get(buckets.value[i].month)
    if (top !== undefined && top <= scrollTop.value) index = i
  }
  return index
})

const dragMonth = ref<string | undefined>()

function updateScrub(clientY: number) {
  const track = scrubberEl.value
  if (!track) return
  const rect = track.getBoundingClientRect()
  const month = monthAtOffset(buckets.value, clientY - rect.top, rect.height)
  if (month) {
    dragMonth.value = month
    jumpToMonth(month)
  }
}

function onScrubberDown(event: MouseEvent) {
  dragging.value = true
  updateScrub(event.clientY)
  window.addEventListener('mousemove', onScrubberMove)
  window.addEventListener('mouseup', onScrubberUp)
}
function onScrubberMove(event: MouseEvent) {
  if (dragging.value) updateScrub(event.clientY)
}
function onScrubberUp() {
  dragging.value = false
  dragMonth.value = undefined
  window.removeEventListener('mousemove', onScrubberMove)
  window.removeEventListener('mouseup', onScrubberUp)
}

function onScrubberKeydown(event: KeyboardEvent) {
  if (buckets.value.length === 0) return
  let next: number | undefined
  if (event.key === 'ArrowUp' || event.key === 'ArrowLeft') next = Math.max(0, currentIndex.value - 1)
  else if (event.key === 'ArrowDown' || event.key === 'ArrowRight') {
    next = Math.min(buckets.value.length - 1, currentIndex.value + 1)
  } else if (event.key === 'Home') next = 0
  else if (event.key === 'End') next = buckets.value.length - 1
  if (next === undefined) return
  event.preventDefault()
  const month = buckets.value[next]?.month
  if (month) jumpToMonth(month)
}

const thumbRatio = computed(() => {
  if (buckets.value.length <= 1) return 0
  if (dragging.value && dragMonth.value) {
    const idx = buckets.value.findIndex((b) => b.month === dragMonth.value)
    return idx < 0 ? 0 : idx / (buckets.value.length - 1)
  }
  return currentIndex.value / (buckets.value.length - 1)
})

// `gridEl` is no longer guaranteed to stay mounted for the component's
// whole lifetime — an active quick filter unmounts this container in
// favor of `FlatAssetGrid` (its own, independent one). A `watch` instead
// of a one-time `addEventListener` in `onMounted` follows every
// appearance/disappearance of the node, not just the first.
watch(gridEl, (el, prevEl) => {
  prevEl?.removeEventListener('scroll', onScroll)
  resizeObserver?.disconnect()
  resizeObserver = undefined
  if (el) {
    measure()
    el.addEventListener('scroll', onScroll, { passive: true })
    if (typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(() => measure())
      resizeObserver.observe(el)
    }
  }
}, { immediate: true, flush: 'post' })

onMounted(async () => {
  await refreshTimeline()
  await nextTick()
  measure()
  live = startLiveEvents((msg) => {
    if (msg.type === 'resync' || msg.type === 'assets.upserted' || msg.type === 'assets.deleted') {
      scheduleLiveRefresh()
    }
  })
})

onUnmounted(() => {
  live?.close()
  resizeObserver?.disconnect()
  gridEl.value?.removeEventListener('scroll', onScroll)
  window.removeEventListener('mousemove', onScrubberMove)
  window.removeEventListener('mouseup', onScrubberUp)
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
  if (promoteTimer) clearTimeout(promoteTimer)
})

function cellProps(month: string, cell: GridCell, priority: 'high' | 'auto') {
  const asset = assetFor(month, cell.offsetInMonth)
  if (!asset) return undefined
  return {
    asset,
    thumbnailUrl: asset.content_hash ? mediaThumbSrc(asset.content_hash) : '',
    placeholderUrl: placeholderFor(asset),
    filename: asset.filename,
    dateLabel: dateLabelOf(asset),
    isFavorite: favorites.isFavorite(asset),
    stackType: stackTypeOf(asset),
    priority
  }
}

/** Only cells whose asset is already in cache: if the month is shown but
 * not yet loaded (rare — `mountedMonths` already starts loading it), the
 * row is simply emptier for a moment — the height is already reserved
 * by the geometry, so there's no layout shift.
 *
 * `rowTop` decides download priority: a row that falls within the first
 * screen (`rowTop < viewportHeight`) downloads its thumbnail immediately
 * and with `fetchpriority="high"`, others stay lazy — this isn't an
 * approximation of the current scroll position, it's literally what's
 * already on screen at first paint.
 */
function resolvedTiles(row: GridRow, rowTop: number) {
  const priority: 'high' | 'auto' = rowTop < viewportHeight.value ? 'high' : 'auto'
  const out: { cell: GridCell; props: NonNullable<ReturnType<typeof cellProps>> }[] = []
  for (const cell of row.cells) {
    const props = cellProps(row.month, cell, priority)
    if (props) out.push({ cell, props })
  }
  return out
}

// A single selection pool, shared by Photos/Favorites/Search/Album/
// Person (`stores/selection.ts`) — you enter it by selecting the first
// photo and leave it by deselecting the last, implicitly, with no
// dedicated "Select" button. `selectionMode` is derived from the count,
// not a separate flag that needs to be kept in sync.
const selectionMode = computed(() => selection.library.selectedIds.size > 0)
function isSelected(id: string): boolean {
  return selection.library.selectedIds.has(id)
}
const selectedAssets = computed(() =>
  loadedAssets.value.filter((asset) => selection.library.selectedIds.has(asset.id))
)

/** "Whatever is visible at that moment", never the whole underlying
 * library — `displayedAssets`, which narrows itself down when a quick
 * filter is active. */
function selectAllVisible() {
  selection.library.selectAllVisible(displayedAssets.value.map((asset) => asset.id))
}
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Global navigation (greeting, search, Folders/Map/Trash/Albums/
         Shares/Users/Groups/Problems, Log out) now lives in
         AppSidebar/AppTopbar (App.vue) — removed from here, not
         duplicated. Culling no longer has an entry point from this view:
         it's a separate, per-folder area (nav "Culling"), no longer a
         session layered on top of "however much has been loaded into the
         timeline". Only density remains (a stand-in until Settings
         exists, see the comment on DENSITY_KEY). "Rename folder…" (shown
         on the left when a folder is open) doesn't apply yet: this view
         has no concept of an "open folder" (a known limitation). -->
    <div
      v-if="!selectionMode"
      class="flex items-center gap-3 border-b border-border px-4 py-3"
    >
      <div class="ml-auto flex items-center gap-2">
        <SelectAllVisible
          :visible-count="displayedAssets.length"
          @select-all="selectAllVisible"
        />
        <QuickFilter
          v-model:selection="filterSelection"
          :dimensions="filterDimensions"
          :result-count="displayedAssets.length"
        />
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
      </div>
    </div>
    <!-- Replaces the whole toolbar row when selection is active. The five
         action buttons (Favorite/Album/Share/Edit/Delete) and the dialogs
         they open live in SelectionBar.vue, along with × / count /
         "Select all". Never `v-if`/`v-else` on `<SelectionBar>` itself
         (a binding comment in its own file): its screen-reader
         announcement region must stay mounted even at the exact instant
         the selection clears, otherwise "Selection cleared" could never
         fire — only the container's visual padding is conditional. -->
    <div :class="selectionMode && 'border-b border-border px-4 py-3'">
      <SelectionBar
        :count="selection.library.selectedIds.size"
        :ariaLabel="t('ui.selectionBar.ariaLabel')"
        @clear="selection.library.clear()"
        @select-all="selectAllVisible"
      >
        <LibrarySelectionActions :assets="selectedAssets" />
      </SelectionBar>
    </div>

    <div
      v-if="bbox"
      class="flex items-center gap-2 border-b border-border px-4 py-2 text-sm"
    >
      <span>{{ t('timeline.mapFilter') }}</span>
      <button
        type="button"
        class="rounded border border-border px-2 py-1"
        @click="clearMapFilter"
      >
        {{ t('timeline.clearMapFilter') }}
      </button>
    </div>

    <ErrorState
      v-if="errorNature"
      :nature="errorNature"
      :technical-detail="errorDetail"
      @retry="refreshTimeline"
    />

    <p
      v-else-if="empty"
      class="p-6 text-content-muted"
    >
      {{ t('timeline.empty') }}
    </p>

    <!-- An active quick filter leaves the geometry blob's territory
         (which assumes a whole month) and switches to the justified flat
         grid already used by Favorites (`FlatAssetGrid.vue`) over the
         filtered set — never carving cells out of rows designed for a
         whole month. -->
    <div
      v-else-if="filterActive && displayedAssets.length === 0"
      class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
    >
      <p class="text-sm font-semibold">
        {{ t('ui.filteredEmpty.title') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('ui.filteredEmpty.subtitle') }}
      </p>
    </div>
    <FlatAssetGrid
      v-else-if="filterActive"
      :assets="displayedAssets"
      :density="density"
      @open="lightbox.open"
    />

    <div
      v-else
      class="flex min-h-0 flex-1"
    >
      <div
        ref="gridEl"
        class="relative min-h-0 flex-1 overflow-auto"
        tabindex="-1"
      >
        <div class="px-4 py-3">
          <div
            ref="contentEl"
            :style="{ position: 'relative', height: `${plan.totalHeight}px` }"
          >
            <div
              v-for="entry in mountedRows"
              :key="entry.index"
              class="stream-row absolute left-0 right-0"
              :data-row-index="entry.index"
              :style="{ transform: `translateY(${entry.top}px)`, height: `${entry.row.height}px` }"
            >
              <h2
                v-if="entry.row.type === 'header'"
                class="flex items-baseline gap-2 text-sm font-medium"
              >
                <span>{{ monthFull(entry.row.month, locale) }}</span>
                <span class="text-xs text-content-muted">
                  {{ t('timeline.monthCount', { n: entry.row.count }, { plural: entry.row.count }) }}
                </span>
              </h2>
              <template v-else>
                <PhotoTile
                  v-for="{ cell, props } in resolvedTiles(entry.row, entry.top)"
                  :key="cell.offsetInMonth"
                  v-bind="props"
                  :selected="isSelected(props.asset.id)"
                  :selection-mode="selectionMode"
                  :enable-long-press="isMobile"
                  :style="{ position: 'absolute', left: `${cell.x}px`, top: 0, width: `${cell.w}px`, height: `${cell.h}px` }"
                  @open="lightbox.open(props.asset)"
                  @toggle-select="selection.library.toggle(props.asset.id)"
                  @toggle-favorite="favorites.toggleOne(props.asset)"
                />
              </template>
            </div>
          </div>
        </div>
      </div>
      <aside
        ref="scrubberEl"
        class="relative w-9 shrink-0 cursor-ns-resize border-l border-border py-2"
        role="slider"
        tabindex="0"
        aria-orientation="vertical"
        :aria-valuemin="0"
        :aria-valuemax="Math.max(0, buckets.length - 1)"
        :aria-valuenow="currentIndex"
        :aria-valuetext="buckets[currentIndex] ? monthFull(buckets[currentIndex].month, locale) : undefined"
        :aria-label="t('timeline.scrubber')"
        @mousedown="onScrubberDown"
        @keydown="onScrubberKeydown"
      >
        <div
          v-for="bucket in buckets"
          :key="bucket.month"
          class="px-1 text-center text-[10px] text-content-muted"
          style="writing-mode: vertical-rl"
        >
          {{ monthAbbrev(bucket.month, locale) }}
        </div>
        <div
          class="absolute right-0.5 h-6 w-2.5 rounded-full bg-accent"
          :style="{ top: `calc(${thumbRatio * 100}% - 12px)` }"
        />
        <p
          v-if="dragging && dragMonth"
          class="absolute right-full mr-1 rounded bg-content px-2 py-1 text-xs text-surface"
          :style="{ top: `${thumbRatio * 100}%` }"
        >
          {{ monthFull(dragMonth, locale) }}
        </p>
      </aside>
    </div>
    <AssetViewer
      v-if="lightbox.viewing.value"
      :asset="lightbox.viewing.value"
      :neighbors="displayedAssets"
      :is-favorite="favorites.isFavorite(lightbox.viewing.value)"
      @close="lightbox.close"
      @step="stepViewer"
      @open-asset="openViewerAsset"
      @toggle-favorite="favorites.toggleOne(lightbox.viewing.value)"
    />
  </div>
</template>
