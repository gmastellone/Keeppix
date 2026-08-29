<script setup lang="ts">
// The justified, virtualized "flat" grid (no grouping by month), extracted
// from `FavoritesView.vue` once it became the second real consumer:
// Favorites always uses it, Timeline uses it when a quick filter is active
// — a filtered list no longer makes sense laid out from the server-side
// geometry blob, which assumes the whole month. Same principle already
// followed for `useDensity`/`useIsMobile`: extract on the second real use,
// not before.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { thumbSrc as mediaThumbSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'
import PhotoTile, { type StackType } from '@/components/ui/PhotoTile.vue'
import { useIsMobile } from '@/composables/useIsMobile'
import { useScrollRestoration } from '@/composables/useScrollRestoration'
import { useFavoritesStore } from '@/stores/favorites'
import { useSelectionStore } from '@/stores/selection'
import { justify } from '@/timeline/justify'
import { targetRowHeight, STREAM_OVERSCAN } from '@/timeline/stream'
import { thumbhashToDataURL } from '@/timeline/thumbhash'
import { RowVirtualizer } from '@/timeline/virtualize'

const props = defineProps<{
  assets: TimelineAsset[]
  density: number
}>()

const emit = defineEmits<{ open: [asset: TimelineAsset] }>()

const { locale } = useI18n()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const { isMobile } = useIsMobile()

const placeholders = new Map<string, string>()
const gridEl = ref<HTMLElement | null>(null)
const contentEl = ref<HTMLElement | null>(null)
const gridWidth = ref(800)
const viewportHeight = ref(600)
const scrollTop = ref(0)
let resizeObserver: ResizeObserver | undefined
let scrollRaf = 0

useScrollRestoration(gridEl)

const rows = computed(() =>
  justify(
    props.assets.map((asset) => ({ id: asset.id, width: asset.width ?? 1, height: asset.height ?? 1 })),
    gridWidth.value,
    targetRowHeight(gridWidth.value, props.density)
  )
)
const rowHeights = computed(() => rows.value.map((row) => row.height))
const virtualizer = computed(() => new RowVirtualizer(rowHeights.value))
const overscanPx = computed(() => viewportHeight.value * STREAM_OVERSCAN)
const mountedRange = computed(() => virtualizer.value.visibleRange(scrollTop.value, viewportHeight.value, overscanPx.value))
const mountedRows = computed(() => {
  const { start, end } = mountedRange.value
  const out: { index: number; top: number; row: (typeof rows.value)[number] }[] = []
  for (let i = start; i < end; i++) {
    const row = rows.value[i]
    if (row) out.push({ index: i, top: virtualizer.value.rowTop(i), row })
  }
  return out
})

const assetsById = computed(() => new Map(props.assets.map((asset) => [asset.id, asset])))

function isSelected(id: string): boolean {
  return selection.library.selectedIds.has(id)
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

function cellProps(id: string, priority: 'high' | 'auto') {
  const asset = assetsById.value.get(id)
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

function resolvedTiles(row: (typeof rows.value)[number], rowTop: number) {
  const priority: 'high' | 'auto' = rowTop < viewportHeight.value ? 'high' : 'auto'
  const out: { cell: (typeof row.cells)[number]; props: NonNullable<ReturnType<typeof cellProps>> }[] = []
  for (const cell of row.cells) {
    const props = cellProps(cell.id, priority)
    if (props) out.push({ cell, props })
  }
  return out
}

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

onMounted(async () => {
  measure()
  await nextTick()
  measure()
  gridEl.value?.addEventListener('scroll', onScroll, { passive: true })
  if (typeof ResizeObserver !== 'undefined' && gridEl.value) {
    resizeObserver = new ResizeObserver(() => measure())
    resizeObserver.observe(gridEl.value)
  }
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  gridEl.value?.removeEventListener('scroll', onScroll)
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
})

watch(rowHeights, () => {
  // The row count changes with density/width/filtered set: without a
  // re-measure pass, the mounted window would stay pinned to a
  // `mountedRange` computed from stale geometry for a moment.
  void nextTick(measure)
})
</script>

<template>
  <div
    ref="gridEl"
    class="relative min-h-0 flex-1 overflow-auto"
    tabindex="-1"
  >
    <div class="px-4 py-3">
      <div
        ref="contentEl"
        :style="{ position: 'relative', height: `${virtualizer.totalHeight}px` }"
      >
        <div
          v-for="entry in mountedRows"
          :key="entry.index"
          class="stream-row absolute left-0 right-0"
          :style="{ transform: `translateY(${entry.top}px)`, height: `${entry.row.height}px` }"
        >
          <PhotoTile
            v-for="{ cell, props: tileProps } in resolvedTiles(entry.row, entry.top)"
            :key="cell.id"
            v-bind="tileProps"
            :selected="isSelected(tileProps.asset.id)"
            :selection-mode="selection.library.selectedIds.size > 0"
            :enable-long-press="isMobile"
            :style="{ position: 'absolute', left: `${cell.x}px`, top: 0, width: `${cell.w}px`, height: `${cell.h}px` }"
            @open="emit('open', tileProps.asset)"
            @toggle-select="selection.library.toggle(tileProps.asset.id)"
            @toggle-favorite="favorites.toggleOne(tileProps.asset)"
          />
        </div>
      </div>
    </div>
  </div>
</template>
