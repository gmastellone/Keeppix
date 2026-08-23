<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, shallowRef, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { fetchBuckets, fetchGeometry, fetchPage, promoteViewport, type MonthBucket, type TimelineAsset } from '@/api/timeline'
import { ApiProblem } from '@/api/client'
import { startLiveEvents, type LiveSocket } from '@/api/events'
import { thumbSrc as mediaThumbSrc } from '@/api/media'
import ErrorState from '@/components/ui/ErrorState.vue'
import PhotoTile, { type StackType } from '@/components/ui/PhotoTile.vue'
import AssetViewer from '@/components/AssetViewer.vue'
import { useLightboxRoute } from '@/composables/useLightboxRoute'
import { useScrollRestoration } from '@/composables/useScrollRestoration'
import { useCullingStore } from '@/stores/culling'
import { useMapsStore } from '@/stores/maps'
import { classifyError } from '@/errors/classify'
import { TimelineGeometry } from '@/timeline/geometry'
import { clampDensity } from '@/timeline/justify'
import { LruPageCache } from '@/timeline/pageCache'
import { monthAbbrev, monthAtOffset, monthFull } from '@/timeline/scrubber'
import { planStream, STREAM_OVERSCAN, type GridCell, type GridRow, type StreamRow } from '@/timeline/stream'
import { thumbhashToDataURL } from '@/timeline/thumbhash'
import { RowVirtualizer } from '@/timeline/virtualize'

// Fase 11 Task 4 (§66/§8 del documento funzionale): la timeline a scala
// reale. Riscrittura, non affiancamento — la Ruling della spec
// fase-11-interfaccia.md §1 lo impone esplicitamente. Due comportamenti
// dell'implementazione precedente non sono nella definizione canonica e
// non sono stati portati avanti: il raggruppamento per giorno dentro il
// mese ("Nessun raggruppamento per giorno", §8, testuale) e
// l'intestazione di mese appiccicata durante lo scroll ("le .month-head
// scorrono via normalmente", §8). Il filtro "Tutti/Foto/Video" è stato
// tolto insieme a loro: non è nel documento per questa vista, non è una
// delle sei dimensioni di SP-3 (che usa "Tipo" per RAW/JPEG, un asse
// diverso — §11), e non ha senso strutturale qui: la geometria che guida
// il layout non porta il kind dello scatto, solo w/h/mese.

const DENSITY_KEY = 'keeppix.density'
// Il documento funzionale (riga 1745) mette la densità in Impostazioni
// (Task 14, non ancora costruito), non in un controllo di vista. Il +/-
// qui è un ripiego temporaneo pre-Task-14: rimosso quando la vista
// Impostazioni reale esisterà, non prima — toglierlo ora senza
// sostituto lascerebbe la densità fissa a 6 per chiunque.
/** Fino a 50 mesi residenti, ~10.000 asset attesi (piano §4.8) — solo le
 * pagine, mai la geometria stessa, che vive fuori da questa cache e non
 * si sfratta mai. */
const PAGE_CACHE_CAPACITY = 50

const { t, locale } = useI18n()
const route = useRoute()
const router = useRouter()
const culling = useCullingStore()
const maps = useMapsStore()

const buckets = ref<MonthBucket[]>([])
// shallowRef, non ref: TimelineGeometry incapsula un DataView e non deve
// mai passare per UnwrapRef (che smonterebbe l'istanza campo per campo,
// perdendo il campo privato `view`) — è anche semanticamente corretto,
// un blob binario immutabile una volta caricato non ha bisogno di
// reattività profonda.
const geometry = shallowRef<TimelineGeometry | null>(null)
const geometryEtag = ref<string | null>(null)
const pageCache = new LruPageCache<string, TimelineAsset[]>(PAGE_CACHE_CAPACITY)
const loadingMonths = new Set<string>()
/** Bump esplicito: `pageCache` è una classe imperativa, non reattiva —
 * questo ref è l'unico modo per dire a Vue "qualcosa dentro è cambiato". */
const cacheTick = ref(0)

const density = ref(clampDensity(Number(localStorage.getItem(DENSITY_KEY) ?? 6)))
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
/** Riga tecnica facoltativa (§68.3, "per chi amministra il server") —
 * solo quando l'errore porta davvero un `Problem` RFC 9457, non per un
 * `TypeError` di rete generico che non ha nulla di più preciso da dire. */
const errorDetail = computed(() =>
  loadError.value instanceof ApiProblem ? `${loadError.value.type} · ${loadError.value.status}` : undefined
)

let promoteTimer: ReturnType<typeof setTimeout> | undefined
let live: LiveSocket | undefined
let resizeObserver: ResizeObserver | undefined
let scrollRaf = 0

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

// Concatenazione dei mesi attualmente residenti in cache, nello stesso
// ordine di `buckets` (dal più recente): l'equivalente di un "tutte le
// foto caricate finora", non l'intera libreria — stesso limite già
// dichiarato prima del Task 4 per l'avvio del culling, ora esteso anche
// alla navigazione prev/next del lightbox (debito dichiarato nel ledger:
// ai bordi di un mese caricato, "successiva" può non trovare nulla anche
// se esiste, semplicemente non è ancora in cache).
const loadedAssets = computed(() => {
  void cacheTick.value
  const out: TimelineAsset[] = []
  for (const bucket of buckets.value) {
    const cached = pageCache.get(bucket.month)
    if (cached) out.push(...cached)
  }
  return out
})

const lightbox = useLightboxRoute<TimelineAsset>(
  (id) => loadedAssets.value.find((asset) => asset.id === id),
  (id) => maps.loadAsset(id)
)

useScrollRestoration(gridEl)

function viewingNeighbour(delta: number): TimelineAsset | undefined {
  const list = loadedAssets.value
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

async function clearMapFilter() {
  const q = { ...route.query }
  delete q.bbox
  await router.replace({ path: '/', query: q })
  await refreshTimeline()
}

/**
 * Unico punto d'ingresso al culling (spec §4.2, hard rule del piano): un
 * pulsante, non scorciatoie sparse. L'insieme di lavoro resta "quanto già
 * caricato in timeline" (nessuna vista per cartella o selezione multipla
 * in questa fase del frontend — Task 7 la introdurrà), invariato dal
 * comportamento pre-Task-4.
 */
async function startCulling() {
  culling.start(loadedAssets.value)
  await router.push('/culling')
}

function setDensity(next: number) {
  density.value = clampDensity(next)
  localStorage.setItem(DENSITY_KEY, String(density.value))
}

/**
 * Fase 11 Task 5 (SP-28, forma a piena vista): "nessuna schermata assume
 * che i dati ci siano". Un fallimento qui sostituisce l'intera griglia
 * con `ErrorState` — è il contenuto principale della vista, non un
 * pezzo — e "Riprova" richiama di nuovo questa stessa funzione (§68.4:
 * "rimette l'insieme di dati in caricamento e lo richiede da capo").
 */
async function refreshTimeline() {
  loadError.value = null
  try {
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
    pageCache.clear()
    loadingMonths.clear()
    cacheTick.value++
    if (gridEl.value) gridEl.value.scrollTop = 0
    scrollTop.value = 0
  } catch (error) {
    loadError.value = error
  }
}

// Priorità di generazione (POST /viewport, piano Task 4.6): calcolata
// dalla stessa matematica esatta del virtualizzatore con overscan 0
// (la vera finestra visibile, non il margine montato) invece di un
// IntersectionObserver — la geometria dà già posizione e dimensione
// esatte di ogni riga, osservare il DOM per la stessa informazione
// sarebbe un giro più lungo per lo stesso risultato, non più preciso.
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

/** Punto di attenzione esplicito del documento funzionale (§66.5): una
 * tessera smontata mentre aveva il fuoco lo perderebbe, e la navigazione
 * da tastiera "cadrebbe" a inizio pagina — non coperto dal prototipo.
 * Sposta il fuoco sul contenitore di scorrimento (`tabindex="-1"`, mai
 * nell'ordine di tabulazione normale) prima che Vue rimuova dal DOM la
 * riga che sta per uscire dall'intervallo montato. */
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

// --- Scrubber dei mesi -----------------------------------------------
// Documento funzionale §8.3: click ovunque sulla barra salta subito al
// mese (non serve trascinare), targhetta solo durante il trascinamento,
// salto istantaneo (mai behavior:'smooth'), sincronizzazione inversa
// allo scroll. "Da rendere raggiungibile da tastiera, cosa che il
// prototipo non fa" (piano, Task 4.5): frecce/Home/End con
// role="slider", assente nel prototipo.
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

// Il mese "corrente" è quello in cui si è effettivamente scrollati (l'ultimo
// la cui intestazione ha già superato la cima del viewport), non una
// stima per rapporto sull'intervallo di scroll totale: quella stima
// azzererebbe sempre l'indice quando il contenuto caricato è più basso
// del viewport (una libreria corta, o poche pagine ancora caricate) — un
// caso reale, non solo di test — e non è l'inverso esatto di
// `jumpToMonth`, che scrolla a una posizione in pixel precisa.
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

onMounted(async () => {
  measure()
  await refreshTimeline()
  await nextTick()
  measure()
  gridEl.value?.addEventListener('scroll', onScroll, { passive: true })
  if (typeof ResizeObserver !== 'undefined' && gridEl.value) {
    resizeObserver = new ResizeObserver(() => measure())
    resizeObserver.observe(gridEl.value)
  }
  live = startLiveEvents((msg) => {
    if (msg.type === 'resync' || msg.type === 'assets.upserted' || msg.type === 'assets.deleted') {
      void refreshTimeline()
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
    isFavorite: asset.favorite,
    stackType: stackTypeOf(asset),
    priority
  }
}

/** Solo le celle il cui asset è già in cache: se il mese è mostrato ma
 * non ancora caricato (raro — `mountedMonths` avvia già il caricamento),
 * la riga resta semplicemente più vuota per un istante — l'altezza è già
 * riservata dalla geometria, nessuno spostamento di layout.
 *
 * `rowTop` decide la priorità di scaricamento (Task 5bis, tabella §5bis):
 * una riga che ricade nella prima schermata (`rowTop < viewportHeight`)
 * scarica la propria miniatura subito e con `fetchpriority="high"`, le
 * altre restano pigre — non è un'approssimazione dello scroll corrente,
 * è letteralmente cosa c'è già a schermo al primo paint.
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
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Fase 11 Task 6 (5/N): la navigazione globale (saluto, ricerca,
         Cartelle/Mappa/Cestino/Album/Condivisioni/Utenti/Gruppi/Problemi,
         Esci) è ora in AppSidebar/AppTopbar (App.vue) — tolta da qui, non
         duplicata. Restano solo i due controlli reali di questa vista che
         non hanno un'altra sede: l'ingresso al culling (unico pulsante,
         spec §4.2) e la densità (ripiego pre-Impostazioni, v. commento
         su DENSITY_KEY). -->
    <div class="flex items-center gap-3 border-b border-border px-4 py-3">
      <button
        v-if="loadedAssets.length > 0"
        type="button"
        class="rounded-lg border border-border px-3 py-1 text-sm"
        @click="startCulling"
      >
        {{ t('culling.entry') }}
      </button>
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
      </div>
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
                  :selected="false"
                  :selection-mode="false"
                  :style="{ position: 'absolute', left: `${cell.x}px`, top: 0, width: `${cell.w}px`, height: `${cell.h}px` }"
                  @open="lightbox.open(props.asset)"
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
      :prev="viewingNeighbour(-1)"
      :next="viewingNeighbour(1)"
      @close="lightbox.close"
      @prev="stepViewer(-1)"
      @next="stepViewer(1)"
      @open-asset="openViewerAsset"
    />
  </div>
</template>
