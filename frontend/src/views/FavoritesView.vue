<script setup lang="ts">
// Fase 11 Task 7 (3/N) — documento funzionale §9 ("Preferiti"),
// verificato riga per riga (righe 1755-1833). "È la timeline con una
// sola sezione e senza titolo" (§9.2): stessa tessera (SP-1), stessa
// barra di selezione (SP-2), stessa griglia giustificata e la stessa
// virtualizzazione (SP-22) — ma senza raggruppamento per mese, senza
// intestazioni di mese e **senza scrubber**.
//
// Niente endpoint "sola geometria" per una lista piatta come esiste per
// la timeline (`fetchGeometry`, Task 4): non serve. `runSearch({op:
// 'favorite'})` (verificato in crates/keeppix-db/src/search.rs,
// `SearchNode::Favorite`, già pronto dal Task 6/10 del backend — mai
// consumato dal frontend finora) restituisce gli stessi `TimelineAsset`
// con `width`/`height` già inclusi: `justify()` (già scritta nel Task 4,
// pura, indipendente dal blob di geometria) basta da sola per il
// layout, senza bisogno di un secondo endpoint.
//
// La griglia visibile è `assets` filtrati per `favorites.isFavorite` —
// non solo lo snapshot caricato: un cuoricino tolto (singolo o di
// gruppo dalla barra di selezione, stesso store condiviso con Timeline)
// fa sparire la tessera da qui **subito**, per costruzione, senza
// bisogno di un handler dedicato "rimuovi dalla vista" (§9.3: "il
// cuoricino qui toglie la foto dalla vista... senza conferma, senza
// toast, senza annulla" — è esattamente il comportamento che questa
// derivazione dà gratis).
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { runSearch } from '@/api/library'
import { thumbSrc as mediaThumbSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'
import { startLiveEvents, type LiveSocket } from '@/api/events'
import AssetViewer from '@/components/AssetViewer.vue'
import LibrarySelectionActions from '@/components/LibrarySelectionActions.vue'
import ErrorState from '@/components/ui/ErrorState.vue'
import PhotoTile, { type StackType } from '@/components/ui/PhotoTile.vue'
import SelectAllVisible from '@/components/ui/SelectAllVisible.vue'
import SelectionBar from '@/components/ui/SelectionBar.vue'
import { useDensity } from '@/composables/useDensity'
import { useIsMobile } from '@/composables/useIsMobile'
import { useLightboxRoute } from '@/composables/useLightboxRoute'
import { useScrollRestoration } from '@/composables/useScrollRestoration'
import { classifyError } from '@/errors/classify'
import { ApiProblem } from '@/api/client'
import { useFavoritesStore } from '@/stores/favorites'
import { useMapsStore } from '@/stores/maps'
import { useSelectionStore } from '@/stores/selection'
import { justify } from '@/timeline/justify'
import { targetRowHeight, STREAM_OVERSCAN } from '@/timeline/stream'
import { thumbhashToDataURL } from '@/timeline/thumbhash'
import { RowVirtualizer } from '@/timeline/virtualize'

const { t, locale } = useI18n()
const maps = useMapsStore()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const { density, setDensity } = useDensity()
const { isMobile } = useIsMobile()

const assets = ref<TimelineAsset[]>([])
const loaded = ref(false)
const loadError = ref<unknown>(null)
const placeholders = new Map<string, string>()

const gridEl = ref<HTMLElement | null>(null)
const contentEl = ref<HTMLElement | null>(null)
const gridWidth = ref(800)
const viewportHeight = ref(600)
const scrollTop = ref(0)

let live: LiveSocket | undefined
let resizeObserver: ResizeObserver | undefined
let scrollRaf = 0

const errorNature = computed(() => (loadError.value ? classifyError(loadError.value) : null))
const errorDetail = computed(() =>
  loadError.value instanceof ApiProblem ? `${loadError.value.type} · ${loadError.value.status}` : undefined
)

// §9.2: il sottotitolo conta i preferiti **prima** dei filtri — qui
// coincide con "tutti quelli caricati", visto che non c'è ancora un
// filtro rapido cablato su questa vista (stesso stato di fatto di
// TimelineView, non un debito nuovo di questa unità).
const totalCount = computed(() => assets.value.length)
const visibleAssets = computed(() => assets.value.filter((asset) => favorites.isFavorite(asset)))

const rows = computed(() =>
  justify(
    visibleAssets.value.map((asset) => ({ id: asset.id, width: asset.width ?? 1, height: asset.height ?? 1 })),
    gridWidth.value,
    targetRowHeight(gridWidth.value, density.value)
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

const assetsById = computed(() => new Map(visibleAssets.value.map((asset) => [asset.id, asset])))

const lightbox = useLightboxRoute<TimelineAsset>(
  (id) => assetsById.value.get(id),
  (id) => maps.loadAsset(id)
)

useScrollRestoration(gridEl)

function viewingNeighbour(delta: number): TimelineAsset | undefined {
  const list = visibleAssets.value
  const i = list.findIndex((a) => a.id === lightbox.viewing.value?.id)
  if (i < 0) return undefined
  return list[i + delta]
}

function stepViewer(delta: number) {
  void lightbox.step(viewingNeighbour(delta))
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

const selectionMode = computed(() => selection.library.selectedIds.size > 0)
function isSelected(id: string): boolean {
  return selection.library.selectedIds.has(id)
}
const selectedAssets = computed(() => visibleAssets.value.filter((asset) => selection.library.selectedIds.has(asset.id)))

function selectAllVisible() {
  selection.library.selectAllVisible(visibleAssets.value.map((asset) => asset.id))
}

async function loadFavorites() {
  loadError.value = null
  loaded.value = false
  try {
    const collected: TimelineAsset[] = []
    let cursor: string | undefined
    do {
      const page = await runSearch({ op: 'favorite' }, cursor)
      collected.push(...page.assets)
      cursor = page.next_cursor
    } while (cursor)
    assets.value = collected
    loaded.value = true
    if (gridEl.value) gridEl.value.scrollTop = 0
    scrollTop.value = 0
  } catch (error) {
    loadError.value = error
  }
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
  await loadFavorites()
  await nextTick()
  measure()
  gridEl.value?.addEventListener('scroll', onScroll, { passive: true })
  if (typeof ResizeObserver !== 'undefined' && gridEl.value) {
    resizeObserver = new ResizeObserver(() => measure())
    resizeObserver.observe(gridEl.value)
  }
  live = startLiveEvents((msg) => {
    if (msg.type === 'resync' || msg.type === 'assets.upserted' || msg.type === 'assets.deleted') {
      void loadFavorites()
    }
  })
})

onUnmounted(() => {
  live?.close()
  resizeObserver?.disconnect()
  gridEl.value?.removeEventListener('scroll', onScroll)
  if (scrollRaf) cancelAnimationFrame(scrollRaf)
})

watch(rowHeights, () => {
  // Il numero di righe cambia con densità/larghezza/preferiti: senza un
  // giro di misura la finestra montata resterebbe agganciata a un
  // `mountedRange` calcolato sulla geometria vecchia per un istante.
  void nextTick(measure)
})
</script>

<template>
  <div class="flex h-full flex-col">
    <ErrorState
      v-if="errorNature"
      :nature="errorNature"
      :technical-detail="errorDetail"
      @retry="loadFavorites"
    />

    <!-- §9.2, primo stato vuoto: nessun preferito in assoluto — "in
         questo caso non viene disegnata nemmeno la barra strumenti". -->
    <div
      v-else-if="loaded && totalCount === 0"
      class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
    >
      <p class="text-sm font-semibold">
        {{ t('favorites.emptyTitle') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('favorites.emptySubtitle') }}
      </p>
    </div>

    <template v-else>
      <div class="border-b border-border px-4 py-3">
        <p class="text-[15px] font-bold">
          {{ t('favorites.title') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('favorites.subtitle', { n: totalCount }) }}
        </p>
      </div>

      <div
        v-if="!selectionMode"
        class="flex items-center gap-3 border-b border-border px-4 py-3"
      >
        <div class="ml-auto flex items-center gap-2">
          <SelectAllVisible
            :visible-count="visibleAssets.length"
            @select-all="selectAllVisible"
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

      <!-- §9.2, secondo stato vuoto: ci sono preferiti ma nessuno è
           attualmente visibile — qui raggiungibile togliendo il cuoricino
           dall'ultima tessera visibile in sessione (senza un filtro
           rapido ancora cablato, la sola causa possibile oggi), stessa
           dicitura del pannello filtri perché la situazione visiva è
           identica: "avevi delle foto, ora non ne vedi nessuna". -->
      <div
        v-if="visibleAssets.length === 0"
        class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
      >
        <p class="text-sm font-semibold">
          {{ t('ui.filteredEmpty.title') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('ui.filteredEmpty.subtitle') }}
        </p>
      </div>
      <div
        v-else
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
                v-for="{ cell, props } in resolvedTiles(entry.row, entry.top)"
                :key="cell.id"
                v-bind="props"
                :selected="isSelected(props.asset.id)"
                :selection-mode="selectionMode"
                :enable-long-press="isMobile"
                :style="{ position: 'absolute', left: `${cell.x}px`, top: 0, width: `${cell.w}px`, height: `${cell.h}px` }"
                @open="lightbox.open(props.asset)"
                @toggle-select="selection.library.toggle(props.asset.id)"
                @toggle-favorite="favorites.toggleOne(props.asset)"
              />
            </div>
          </div>
        </div>
      </div>
    </template>
    <AssetViewer
      v-if="lightbox.viewing.value"
      :asset="lightbox.viewing.value"
      :prev="viewingNeighbour(-1)"
      :next="viewingNeighbour(1)"
      @close="lightbox.close"
      @prev="stepViewer(-1)"
      @next="stepViewer(1)"
      @open-asset="openViewerAsset"
    />
  </div>
</template>
