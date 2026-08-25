<script setup lang="ts">
// Fase 11 Task 9 (§23-25 del documento funzionale): la barra di ricerca a
// pillole. Sostituisce interamente la vecchia UI (campo + pulsante
// "Cerca" + sintassi digitata `type:.../camera:...`, mai prevista dal
// documento): §23.5 è esplicito — "non esiste alcun altro modo di creare
// un filtro strutturato — né digitando e premendo Invio". Il vecchio
// parser (`frontend/src/search/parse.ts`) è stato ritirato: l'AST
// (`frontend/src/search/ast.ts`, stesso tipo, senza il tokenizer) ora
// nasce solo da pillole + un nodo `text` per la descrizione libera.
//
// Le sette categorie di suggerimento (§23.2) vengono da due fonti reali,
// non da un'unica lista come nel mockup:
//  - "Tag" è costruito qui, non da `GET /search/suggest`: quell'endpoint
//    (`crates/keeppix-db/src/search.rs:396-460`) non produce mai righe di
//    genere `tag` — il commento a codice lì lo dice esplicitamente
//    ("la tabella dei tag non esiste ancora", scritto in Fase 10, prima
//    che la Fase 7 la creasse) — quindi filtriamo `fetchTags()` lato
//    client, come farebbe il mockup con la sua lista precaricata.
//  - "Fotocamera"/"Cartella"/"ISO"/"Anno"/"Paese" vengono dal vero
//    `GET /search/suggest?q=`, che a differenza del mockup calcola su
//    dati reali di libreria (non costanti cablate) — usato così com'è.
//  - "Posizione" (GPS) non ha alcuna fonte reale (nessun `SuggestionKind`
//    per questo): riprodotta pari al mockup, un'unica riga pseudo-
//    generata quando il testo è sottostringa di "gps".
//  - "Paese" mostra il codice ISO grezzo (`p.country_code`, es. "IT"): il
//    backend non ha una tabella codice→nome e nessun'altra vista
//    dell'app ne ha una — deviazione dichiarata dal mockup, che mostrava
//    "Italia" cablato.
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { runSearch } from '@/api/library'
import { fetchAllFolders, fetchTree, type FolderView } from '@/api/folders'
import { createSavedSearch, fetchSavedSearches, fetchSuggestions, type SavedSearch, type Suggestion } from '@/api/search'
import { fetchTags, type Tag } from '@/api/tags'
import type { TimelineAsset } from '@/api/timeline'
import { thumbSrc } from '@/api/media'
import type { SearchNode } from '@/search/ast'

import AssetViewer from '@/components/AssetViewer.vue'
import FlatAssetGrid from '@/components/FlatAssetGrid.vue'
import LibrarySelectionActions from '@/components/LibrarySelectionActions.vue'
import SelectionBar from '@/components/ui/SelectionBar.vue'
import { useDensity } from '@/composables/useDensity'
import { useLightboxRoute } from '@/composables/useLightboxRoute'
import { useFavoritesStore } from '@/stores/favorites'
import { useMapsStore } from '@/stores/maps'
import { useSelectionStore } from '@/stores/selection'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const maps = useMapsStore()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const { density } = useDensity()

const q = ref(typeof route.query.q === 'string' ? route.query.q : '')
const assets = ref<TimelineAsset[]>([])
const error = ref('')

// --- pillole (§24) ---
type PillType = 'tag' | 'camera' | 'folder' | 'iso' | 'year' | 'gps' | 'country'

interface SearchPill {
  type: PillType
  /** valore grezzo usato per costruire il nodo `SearchNode` (id tag, id
   * cartella, testo fotocamera, numero ISO/anno come stringa, "gps",
   * codice paese). */
  value: string
  /** "nome nudo" da mostrare — `pillText()` aggiunge il prefisso dove
   * previsto (§24.2: Tag/ISO/Anno/GPS). */
  label: string
  color?: string | null
}

const pills = ref<SearchPill[]>([])

// Il chip da solo non conta come ricerca (§25.2). Dichiarato qui, prima di
// `lightbox`/`visibleAssets`: il watcher immediato di `useLightboxRoute`
// (sotto) legge `visibleAssets.value` in modo sincrono durante il setup,
// e `visibleAssets` dipende da `hasSearch` — deve quindi esistere già a
// quel punto, non più in basso nel file (temporal dead zone altrimenti).
const hasSearch = computed(() => pills.value.length > 0 || q.value.trim().length > 0)

// --- risultati vs scoperta (§25.2): due fonti diverse alimentano la
// stessa griglia, mai insieme — `assets` (ricerca vera, tutte le pagine)
// quando `hasSearch`, `recentAssets` (una sola pagina, §25.2 punto 3)
// quando non c'è alcuna ricerca. Vedi `refresh()`/`loadRecent()`.
const recentAssets = ref<TimelineAsset[]>([])
const visibleAssets = computed(() => (hasSearch.value ? assets.value : recentAssets.value))

const lightbox = useLightboxRoute<TimelineAsset>(
  (id) => visibleAssets.value.find((asset) => asset.id === id),
  (id) => maps.loadAsset(id)
)

function stepViewer(asset: TimelineAsset) {
  void lightbox.step(asset)
}

const selectionMode = computed(() => selection.library.selectedIds.size > 0)
const selectedAssets = computed(() => visibleAssets.value.filter((asset) => selection.library.selectedIds.has(asset.id)))

function hasPill(type: PillType, value: string): boolean {
  return pills.value.some((p) => p.type === type && p.value === value)
}

function pillText(p: SearchPill): string {
  switch (p.type) {
    case 'tag':
      return t('search.pill.tag', { name: p.label })
    case 'iso':
      return t('search.pill.iso', { n: p.label })
    case 'year':
      return t('search.pill.year', { n: p.label })
    case 'gps':
      return t('search.pill.gps')
    case 'camera':
    case 'folder':
    case 'country':
      return p.label
  }
}

function removePill(index: number) {
  pills.value.splice(index, 1)
  void refresh()
}

function clearAll() {
  pills.value = []
  q.value = ''
  remoteSuggestions.value = []
  suggestOpen.value = false
  void refresh()
}

// --- fonti dei suggerimenti, caricate una volta al montaggio ---
const allTags = ref<Tag[]>([])
const allFolders = ref<FolderView[]>([])
const remoteSuggestions = ref<Suggestion[]>([])

async function loadRemoteSuggestions() {
  const text = q.value.trim()
  if (!text) {
    remoteSuggestions.value = []
    return
  }
  try {
    const res = await fetchSuggestions(text)
    remoteSuggestions.value = res.suggestions
  } catch {
    remoteSuggestions.value = []
  }
}

// --- pannello suggerimenti (§23.2-23.5) ---
const suggestOpen = ref(false)
const focused = ref(false)
const inputEl = ref<HTMLInputElement | null>(null)
const composerEl = ref<HTMLElement | null>(null)
const panelEl = ref<HTMLElement | null>(null)

interface SuggestGroup {
  key: PillType
  groupLabel: string
  rows: SearchPill[]
}

const suggestGroups = computed<SuggestGroup[]>(() => {
  const text = q.value.trim()
  const groups: SuggestGroup[] = []

  if (!text) {
    // Campo vuoto ma con focus (§23.2): "incoraggia la scoperta con
    // qualche cartella oltre ai tag" — solo Tag (primi 5) e Cartella.
    const tagRows: SearchPill[] = allTags.value
      .filter((tag) => tag.kind === 'tag' && !hasPill('tag', tag.id))
      .slice(0, 5)
      .map((tag) => ({ type: 'tag', value: tag.id, label: tag.name, color: tag.color }))
    if (tagRows.length) groups.push({ key: 'tag', groupLabel: t('search.group.tag'), rows: tagRows })

    const folderRows: SearchPill[] = allFolders.value
      .filter((folder) => !hasPill('folder', folder.id))
      .map((folder) => ({ type: 'folder', value: folder.id, label: folder.name }))
    if (folderRows.length) groups.push({ key: 'folder', groupLabel: t('search.group.folder'), rows: folderRows })

    return groups
  }

  const lower = text.toLowerCase()

  const tagRows: SearchPill[] = allTags.value
    .filter((tag) => tag.kind === 'tag' && tag.name.toLowerCase().includes(lower) && !hasPill('tag', tag.id))
    .slice(0, 5)
    .map((tag) => ({ type: 'tag', value: tag.id, label: tag.name, color: tag.color }))
  if (tagRows.length) groups.push({ key: 'tag', groupLabel: t('search.group.tag'), rows: tagRows })

  const cameraRows: SearchPill[] = remoteSuggestions.value
    .filter((s) => s.kind === 'camera' && !hasPill('camera', s.value))
    .slice(0, 4)
    .map((s) => ({ type: 'camera', value: s.value, label: s.label }))
  if (cameraRows.length) groups.push({ key: 'camera', groupLabel: t('search.group.camera'), rows: cameraRows })

  const folderRows: SearchPill[] = allFolders.value
    .filter((folder) => folder.name.toLowerCase().includes(lower) && !hasPill('folder', folder.id))
    .map((folder) => ({ type: 'folder', value: folder.id, label: folder.name }))
  if (folderRows.length) groups.push({ key: 'folder', groupLabel: t('search.group.folder'), rows: folderRows })

  const isoRows: SearchPill[] = remoteSuggestions.value
    .filter((s) => s.kind === 'iso' && !hasPill('iso', s.value))
    .map((s) => ({ type: 'iso', value: s.value, label: s.label }))
  if (isoRows.length) groups.push({ key: 'iso', groupLabel: t('search.group.iso'), rows: isoRows })

  const yearRows: SearchPill[] = remoteSuggestions.value
    .filter((s) => s.kind === 'year' && !hasPill('year', s.value))
    .map((s) => ({ type: 'year', value: s.value, label: s.label }))
  if (yearRows.length) groups.push({ key: 'year', groupLabel: t('search.group.year'), rows: yearRows })

  if ('gps'.includes(lower) && !hasPill('gps', 'gps')) {
    groups.push({
      key: 'gps',
      groupLabel: t('search.group.gps'),
      rows: [{ type: 'gps', value: 'gps', label: t('search.suggest.gps') }]
    })
  }

  const countryRows: SearchPill[] = remoteSuggestions.value
    .filter((s) => s.kind === 'country' && !hasPill('country', s.value))
    .map((s) => ({ type: 'country', value: s.value, label: s.label }))
  if (countryRows.length) groups.push({ key: 'country', groupLabel: t('search.group.country'), rows: countryRows })

  return groups
})

const showFreeTextRow = computed(() => q.value.trim().length > 0)
const showEmptyMessage = computed(() => !q.value.trim() && suggestGroups.value.length === 0)

function pickSuggestion(row: SearchPill) {
  if (hasPill(row.type, row.value)) return
  pills.value.push(row)
  q.value = ''
  remoteSuggestions.value = []
  suggestOpen.value = false
  void refresh()
  inputEl.value?.focus()
}

function onInput() {
  suggestOpen.value = true
  void loadRemoteSuggestions()
  void refresh()
}

function onFocus() {
  focused.value = true
  suggestOpen.value = true
}

function onEscape() {
  suggestOpen.value = false
}

function onDocumentMouseDown(e: MouseEvent) {
  if (composerEl.value && !composerEl.value.contains(e.target as Node)) {
    suggestOpen.value = false
  }
}

function rowButtons(): HTMLButtonElement[] {
  return panelEl.value ? Array.from(panelEl.value.querySelectorAll<HTMLButtonElement>('.search-suggest-row')) : []
}

function focusFirstRow() {
  rowButtons()[0]?.focus()
}

function focusNextRow(e: KeyboardEvent) {
  const rows = rowButtons()
  const i = rows.indexOf(e.target as HTMLButtonElement)
  if (i === -1) return
  rows[(i + 1) % rows.length]?.focus()
}

function focusPrevRow(e: KeyboardEvent) {
  const rows = rowButtons()
  const i = rows.indexOf(e.target as HTMLButtonElement)
  if (i <= 0) {
    inputEl.value?.focus()
    return
  }
  rows[i - 1]?.focus()
}

// --- chip del tipo file (§23.3 controlli 5-9) ---
// Mutuamente esclusivi, "Tutti i tipi" è il default e nel mockup non si
// può tornare a "nessuno" — si passa solo da un chip all'altro. "RAW" e
// "JPEG" filtrano sul `kind` della riga primaria dello stack (`SearchNode
// ::Type`, `crates/keeppix-db/src/search.rs:911-914` — "raw" è un alias
// di "raw_image"): a differenza del mockup, che aveva un solo booleano
// `isRaw`/non-`isRaw` binario, il sistema reale ha anche `video`/
// `unknown` — "JPEG" qui filtra esattamente `kind==='image'`, non
// genericamente "non RAW", altrimenti includerebbe anche i video.
type TypeFilter = 'all' | 'raw' | 'jpeg' | 'favorite'
const typeFilter = ref<TypeFilter>('all')

function setTypeFilter(value: TypeFilter) {
  typeFilter.value = value
  void refresh()
}

function typeFilterNode(): SearchNode | null {
  switch (typeFilter.value) {
    case 'raw':
      return { op: 'type', value: 'raw_image' }
    case 'jpeg':
      return { op: 'type', value: 'image' }
    case 'favorite':
      return { op: 'favorite' }
    case 'all':
      return null
  }
}

// --- risultati (§25) ---
// Il chip del tipo file da solo **non** conta come ricerca (§25.2, nota:
// "i chip del tipo file non contano come 'ricerca'") — resta lo stato di
// scoperta anche con "RAW" attivo, ma la griglia "Aggiunti di recente"
// che quello stato mostra è comunque filtrata (stesso `buildAst()`,
// stessa ambiguità silenziosa del documento: "le 32 foto mostrate sono
// comunque filtrate solo RAW, senza che nulla lo dica"). `hasSearch`
// stesso è dichiarato molto più in alto nel file — vedi il commento lì.

// §25.2 punto 3: "Ricerca: <b>Tag: Tramonti</b> + descrizione libera
// <b>«tramonto»</b> — 12 risultati" — ogni pezzo (pillola o testo) è in
// grassetto per conto proprio, uniti da " + " non in grassetto.
const recapParts = computed<string[]>(() => {
  const parts = pills.value.map((p) => pillText(p))
  const text = q.value.trim()
  if (text) parts.push(t('search.recap.freeText', { text }))
  return parts
})

function buildAst(): SearchNode | null {
  const nodes: SearchNode[] = []
  const tn = typeFilterNode()
  if (tn) nodes.push(tn)
  for (const p of pills.value) {
    if (p.type === 'tag') nodes.push({ op: 'tag', id: p.value })
    else if (p.type === 'camera') nodes.push({ op: 'camera', value: p.value })
    else if (p.type === 'folder') nodes.push({ op: 'folder', id: p.value })
    else if (p.type === 'iso') nodes.push({ op: 'iso', cmp: 'eq', value: Number(p.value) })
    else if (p.type === 'year') nodes.push({ op: 'year', value: Number(p.value) })
    else if (p.type === 'gps') nodes.push({ op: 'has_gps' })
    else if (p.type === 'country') nodes.push({ op: 'country', value: p.value })
  }
  const text = q.value.trim()
  if (text) nodes.push({ op: 'text', value: text })
  if (nodes.length === 0) return null
  if (nodes.length === 1) return nodes[0]
  return { op: 'and', args: nodes }
}

async function runFullSearch() {
  const ast = buildAst()
  if (!ast) {
    assets.value = []
    return
  }
  try {
    const collected: TimelineAsset[] = []
    let cursor: string | undefined
    do {
      const page = await runSearch(ast, cursor)
      collected.push(...page.assets)
      cursor = page.next_cursor
    } while (cursor)
    assets.value = collected
  } catch {
    error.value = t('search.error')
  }
}

// §25.2 punto 3: "al massimo 32 foto" — il backend ordina già per
// `taken_at_utc DESC` (`crates/keeppix-db/src/search.rs:246`), quindi
// una sola pagina (200 righe di default, mai serve un cursore) basta.
// Deviazione deliberata dal mockup: lì l'ordine era `monthDistance`
// crescente dal "mese corrente della demo" (luglio, cablato) — un
// surrogato che nella demo coincide con la vera recenza solo perché il
// catalogo dimostrativo copre un solo anno. Con dati reali su più anni
// "il mese più vicino a ora" e "le foto più recenti" divergono, e il
// titolo della sezione ("Aggiunti di recente") promette la seconda: usata
// quella, più corretta **e** più economica (niente paginazione esaustiva
// dell'intera libreria solo per un widget di scoperta).
async function loadRecent() {
  try {
    const ast = buildAst() ?? { op: 'and', args: [] }
    const page = await runSearch(ast)
    recentAssets.value = page.assets.slice(0, 32)
  } catch {
    recentAssets.value = []
  }
}

async function refresh() {
  error.value = ''
  savedJustNow.value = false
  await router.replace({ query: { ...route.query, q: q.value || undefined } })
  if (hasSearch.value) {
    await runFullSearch()
  } else {
    await loadRecent()
  }
}

// --- cartelle (§25.2 punto 2) ---
interface FolderCard {
  folder: FolderView
  count: number
  coverHash: string | null
}
const folderCards = ref<FolderCard[]>([])

async function loadFolderCards() {
  try {
    const roots = await fetchTree()
    folderCards.value = await Promise.all(
      roots.map(async (folder) => {
        const collected: TimelineAsset[] = []
        let cursor: string | undefined
        do {
          const page = await runSearch({ op: 'folder', id: folder.id }, cursor)
          collected.push(...page.assets)
          cursor = page.next_cursor
        } while (cursor)
        return { folder, count: collected.length, coverHash: collected[0]?.content_hash ?? null }
      })
    )
  } catch {
    folderCards.value = []
  }
}

// Non c'è, nell'app reale, una "vista Foto scoperta su una cartella" da
// raggiungere (nessuna rotta/parametro per aprire la timeline già
// filtrata su una cartella — verificato: `TimelineView.vue` non ha alcun
// concetto di cartella corrente, `FoldersView.vue` non legge parametri
// di rotta). La destinazione reale più vicina è la vista Cartelle stessa,
// da cui l'utente entra nella cartella scelta — non un salto diretto
// come richiede il documento, ma non un link morto.
function openFolders() {
  void router.push('/folders')
}

// --- ricerche salvate (§25.2 punto 1, §25.3 riga 3 "Salva questa
// ricerca") ---
const savedSearches = ref<SavedSearch[]>([])
const savedJustNow = ref(false)

function quoteIfNeeded(value: string): string {
  return /\s/.test(value) ? `"${value.replace(/"/g, '')}"` : value
}

// La grammatica testuale che il backend sa ancora interpretare
// (`crates/keeppix-db/src/search.rs:696-798`, `parse_query_text`/
// `value_node`) precede la Fase 7/9: capisce solo `type:`/`camera:`/
// `lens:`/`iso:`/`folder:`/`has:gps`/un anno nudo a 4 cifre/testo libero
// (fra virgolette se contiene spazi) — non ha **mai** imparato `tag:`,
// `country:` né una parola chiave per "preferiti". Una pillola tag/paese
// o il chip "Preferiti" non sono quindi serializzabili in `query_text`:
// `null` qui disabilita "Salva questa ricerca" invece di scrivere una
// ricerca salvata che, ricaricata, si comporterebbe in modo diverso da
// quella corrente — silenziosamente sbagliata sarebbe peggio che assente.
const serializedQuery = computed<string | null>(() => {
  if (pills.value.some((p) => p.type === 'tag' || p.type === 'country')) return null
  if (typeFilter.value === 'favorite') return null
  const tokens: string[] = []
  if (typeFilter.value === 'raw') tokens.push('type:raw_image')
  if (typeFilter.value === 'jpeg') tokens.push('type:image')
  for (const p of pills.value) {
    if (p.type === 'camera') tokens.push(`camera:${quoteIfNeeded(p.value)}`)
    else if (p.type === 'folder') tokens.push(`folder:${p.value}`)
    else if (p.type === 'iso') tokens.push(`iso:${p.value}`)
    else if (p.type === 'year') tokens.push(p.value)
    else if (p.type === 'gps') tokens.push('has:gps')
  }
  const text = q.value.trim()
  if (text) tokens.push(`"${text.replace(/"/g, '')}"`)
  return tokens.join(' ')
})

const canSaveSearch = computed(() => serializedQuery.value !== null)

async function saveSearch() {
  const query = serializedQuery.value
  if (query === null || savedJustNow.value) return
  const name = [...pills.value.map((p) => pillText(p)), ...(q.value.trim() ? [q.value.trim()] : [])].join(' + ')
  try {
    const created = await createSavedSearch(name, query)
    savedSearches.value = [...savedSearches.value, created]
    savedJustNow.value = true
  } catch {
    error.value = t('search.error')
  }
}

function selectAllVisible() {
  selection.library.selectAllVisible(visibleAssets.value.map((asset) => asset.id))
}

onMounted(() => {
  document.addEventListener('mousedown', onDocumentMouseDown)
  void fetchTags()
    .then((list) => {
      allTags.value = list
    })
    .catch(() => undefined)
  void fetchAllFolders()
    .then((list) => {
      allFolders.value = list
    })
    .catch(() => undefined)
  void fetchSavedSearches()
    .then((list) => {
      savedSearches.value = list
    })
    .catch(() => undefined)
  void loadFolderCards()
  void refresh()
})

onUnmounted(() => {
  document.removeEventListener('mousedown', onDocumentMouseDown)
})
</script>

<template>
  <main class="flex h-full flex-col p-4">
    <div
      ref="composerEl"
      class="relative flex flex-wrap items-center gap-1.5 rounded-[10px] border border-border bg-chip-bg px-2.5 py-2"
    >
      <svg
        viewBox="0 0 24 24"
        width="16"
        height="16"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
        class="shrink-0 text-content-muted"
      >
        <circle
          cx="11"
          cy="11"
          r="8"
        />
        <line
          x1="21"
          y1="21"
          x2="16.65"
          y2="16.65"
        />
      </svg>

      <span
        v-for="(p, i) in pills"
        :key="`${p.type}:${p.value}`"
        class="search-pill flex items-center gap-1 rounded-full bg-accent-tint px-2.5 py-1 text-[12.5px] font-semibold text-accent"
      >
        <span
          v-if="p.type === 'tag'"
          class="h-2 w-2 rounded-full"
          :style="{ backgroundColor: p.color ?? '#6b6b6e' }"
        />
        {{ pillText(p) }}
        <button
          type="button"
          role="button"
          tabindex="0"
          class="search-pill-x opacity-65 hover:opacity-100"
          :aria-label="t('search.removePill', { label: pillText(p) })"
          @click="removePill(i)"
        >
          ✕
        </button>
      </span>

      <input
        id="search-query-input"
        ref="inputEl"
        v-model="q"
        type="text"
        autocomplete="off"
        class="min-w-[160px] flex-1 bg-transparent text-[14px] outline-none"
        :placeholder="t('search.placeholder')"
        @input="onInput"
        @focus="onFocus"
        @keydown.esc="onEscape"
        @keydown.down.prevent="focusFirstRow"
      >

      <button
        id="searchClearAll"
        type="button"
        class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-content-muted hover:bg-border/40 hover:text-content"
        :aria-label="t('search.clearAll')"
        @click="clearAll"
      >
        ✕
      </button>

      <div
        v-if="suggestOpen"
        ref="panelEl"
        role="listbox"
        class="absolute left-0 right-0 top-full z-10 mt-1 max-h-[320px] overflow-y-auto rounded-lg
               border border-border bg-surface-elevated p-1.5 shadow-lg"
      >
        <template v-if="suggestGroups.length">
          <div
            v-for="group in suggestGroups"
            :key="group.key"
            class="mb-1"
          >
            <div class="search-suggest-group-label px-2 py-1 text-[10.5px] font-semibold uppercase tracking-wider text-content-muted">
              {{ group.groupLabel }}
            </div>
            <button
              v-for="row in group.rows"
              :key="`${row.type}:${row.value}`"
              type="button"
              class="search-suggest-row flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-chip-bg"
              @click="pickSuggestion(row)"
              @keydown.down.prevent="focusNextRow($event)"
              @keydown.up.prevent="focusPrevRow($event)"
              @keydown.esc="onEscape"
            >
              <span
                v-if="row.type === 'tag'"
                class="h-2 w-2 shrink-0 rounded-full"
                :style="{ backgroundColor: row.color ?? '#6b6b6e' }"
              />
              {{ row.label }}
            </button>
          </div>
        </template>
        <p
          v-else-if="showEmptyMessage"
          class="search-suggest-empty px-2 py-3 text-center text-[12.5px] text-content-muted"
        >
          {{ t('search.suggest.empty') }}
        </p>
        <p
          v-if="showFreeTextRow"
          class="search-suggest-free mt-1 border-t border-border px-2 pt-1.5 text-[12.5px] text-content-muted"
        >
          {{ t('search.suggest.freeTextLabel') }} «<b>{{ q.trim() }}</b>»
        </p>
      </div>
    </div>

    <div class="mt-2 flex flex-wrap items-center gap-1.5">
      <button
        v-for="chip in (['all', 'raw', 'jpeg', 'favorite'] as const)"
        :key="chip"
        type="button"
        class="rounded-full border px-3 py-1 text-[12.5px]"
        :class="
          typeFilter === chip
            ? 'border-accent bg-accent-tint font-semibold text-accent'
            : 'border-border bg-chip-bg text-content hover:bg-border/40'
        "
        @click="setTypeFilter(chip)"
      >
        {{ t(`search.type.${chip}`) }}
      </button>
      <span
        class="cursor-default rounded-full border border-border bg-chip-bg px-3 py-1 text-[12.5px] text-content-muted opacity-50"
        :title="t('search.type.personTitle')"
      >
        {{ t('search.type.person') }}
      </span>
    </div>

    <p
      v-if="error"
      class="mt-4 text-danger"
    >
      {{ error }}
    </p>

    <!-- §25.2, stato "ho cercato": titolo "Risultati", riepilogo, "Salva
         questa ricerca". Le sezioni di scoperta spariscono. -->
    <template v-if="hasSearch">
      <div class="mt-4 flex items-center justify-between">
        <p class="text-[15px] font-bold">
          {{ t('search.results.title') }}
        </p>
        <button
          type="button"
          class="rounded-lg border border-border px-2.5 py-1 text-[12.5px]"
          :class="savedJustNow ? 'pointer-events-none opacity-70' : ''"
          :disabled="!canSaveSearch"
          :title="!canSaveSearch ? t('search.recap.saveDisabledTitle') : undefined"
          @click="saveSearch"
        >
          {{ savedJustNow ? t('search.recap.saved') : t('search.recap.save') }}
        </button>
      </div>
      <p class="mt-1 text-[12.5px] text-content-muted">
        {{ t('search.recap.prefix') }}
        <template
          v-for="(part, i) in recapParts"
          :key="i"
        ><b>{{ part }}</b><span v-if="i < recapParts.length - 1"> + </span></template>
        — {{ t('search.recap.count', { n: assets.length }, { plural: assets.length }) }}
      </p>
    </template>

    <!-- §25.2, stato iniziale: ricerche salvate + cartelle, solo quando
         non c'è alcuna ricerca in corso. -->
    <template v-else>
      <div
        v-if="savedSearches.length"
        class="mt-4"
      >
        <p class="text-[15px] font-bold">
          {{ t('search.results.savedTitle') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('search.results.savedSubtitle') }}
        </p>
        <div class="mt-2 flex flex-wrap gap-1.5">
          <span
            v-for="saved in savedSearches"
            :key="saved.id"
            class="flex items-center gap-1 rounded-full border border-border bg-chip-bg px-2.5 py-1 text-[12.5px] text-content-muted"
          >
            <svg
              viewBox="0 0 24 24"
              width="11"
              height="11"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              aria-hidden="true"
            >
              <circle
                cx="11"
                cy="11"
                r="8"
              />
              <line
                x1="21"
                y1="21"
                x2="16.65"
                y2="16.65"
              />
            </svg>
            {{ saved.name }}
          </span>
        </div>
      </div>

      <div
        v-if="folderCards.length"
        class="mt-4"
      >
        <p class="text-[15px] font-bold">
          {{ t('search.results.foldersTitle') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('search.results.foldersSubtitle') }}
        </p>
        <div class="mt-2 grid grid-cols-3 gap-3">
          <button
            v-for="card in folderCards"
            :key="card.folder.id"
            type="button"
            class="overflow-hidden rounded-lg border border-border text-left"
            @click="openFolders"
          >
            <img
              v-if="card.coverHash"
              :src="thumbSrc(card.coverHash)"
              alt=""
              class="h-[88px] w-full object-cover"
            >
            <div
              v-else
              class="h-[88px] w-full bg-chip-bg"
            />
            <div class="px-2 py-1.5">
              <p class="truncate text-[13.5px] font-bold">
                {{ card.folder.name }}
              </p>
              <p class="text-[11.5px] text-content-muted">
                {{ t('search.results.folderCount', { n: card.count }, { plural: card.count }) }}
              </p>
            </div>
          </button>
        </div>
      </div>

      <p class="mt-4 text-[15px] font-bold">
        {{ t('search.results.recentTitle') }}
      </p>
    </template>

    <SelectionBar
      v-if="selectionMode"
      :count="selection.library.selectedIds.size"
      :ariaLabel="t('ui.selectionBar.ariaLabel')"
      class="mt-2"
      @clear="selection.library.clear()"
      @select-all="selectAllVisible"
    >
      <LibrarySelectionActions :assets="selectedAssets" />
    </SelectionBar>

    <!-- §25.2, "Nessun risultato" — solo nello stato "ho cercato". -->
    <div
      v-if="hasSearch && assets.length === 0"
      class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
    >
      <p class="text-sm font-semibold">
        {{ t('search.results.emptyTitle') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('search.results.emptySubtitle') }}
      </p>
    </div>
    <FlatAssetGrid
      v-else
      class="mt-2"
      :assets="visibleAssets"
      :density="density"
      @open="lightbox.open"
    />

    <AssetViewer
      v-if="lightbox.viewing.value"
      :asset="lightbox.viewing.value"
      :neighbors="visibleAssets"
      :is-favorite="favorites.isFavorite(lightbox.viewing.value)"
      @close="lightbox.close"
      @step="stepViewer"
      @toggle-favorite="favorites.toggleOne(lightbox.viewing.value)"
    />
  </main>
</template>
