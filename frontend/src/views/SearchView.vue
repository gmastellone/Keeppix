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
import { fetchAllFolders, type FolderView } from '@/api/folders'
import { fetchSavedSearches, fetchSuggestions, type Suggestion } from '@/api/search'
import { fetchTags, type Tag } from '@/api/tags'
import type { TimelineAsset } from '@/api/timeline'
import { thumbSrc } from '@/api/media'
import type { SearchNode } from '@/search/ast'

import AssetViewer from '@/components/AssetViewer.vue'
import { useLightboxRoute } from '@/composables/useLightboxRoute'
import { useFavoritesStore } from '@/stores/favorites'
import { useMapsStore } from '@/stores/maps'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const maps = useMapsStore()
const favorites = useFavoritesStore()

const q = ref(typeof route.query.q === 'string' ? route.query.q : '')
const assets = ref<TimelineAsset[]>([])
const error = ref('')

const lightbox = useLightboxRoute<TimelineAsset>(
  (id) => assets.value.find((asset) => asset.id === id),
  (id) => maps.loadAsset(id)
)

function stepViewer(asset: TimelineAsset) {
  void lightbox.step(asset)
}

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
  void triggerSearch()
}

function clearAll() {
  pills.value = []
  q.value = ''
  remoteSuggestions.value = []
  suggestOpen.value = false
  void triggerSearch()
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
  void triggerSearch()
  inputEl.value?.focus()
}

function onInput() {
  suggestOpen.value = true
  void loadRemoteSuggestions()
  void triggerSearch()
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

// --- risultati (§25, area completa non ancora costruita — resta la
// griglia semplice per questa unità: rifatta nella prossima) ---
const hasSearch = computed(() => pills.value.length > 0 || q.value.trim().length > 0)

function buildAst(): SearchNode | null {
  const nodes: SearchNode[] = []
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

async function triggerSearch() {
  error.value = ''
  await router.replace({ query: { ...route.query, q: q.value || undefined } })
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
  void fetchSavedSearches().catch(() => undefined)
  if (hasSearch.value) void triggerSearch()
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

    <p
      v-if="error"
      class="mt-4 text-danger"
    >
      {{ error }}
    </p>

    <ul class="mt-4 grid grid-cols-4 gap-2">
      <li
        v-for="asset in assets"
        :key="asset.id"
      >
        <button
          class="block w-full"
          @click="lightbox.open(asset)"
        >
          <img
            v-if="asset.content_hash"
            :src="thumbSrc(asset.content_hash)"
            :alt="asset.filename"
            class="h-32 w-full object-cover"
          >
          <span
            v-else
            class="block truncate text-sm"
          >{{ asset.filename }}</span>
        </button>
      </li>
    </ul>

    <AssetViewer
      v-if="lightbox.viewing.value"
      :asset="lightbox.viewing.value"
      :neighbors="assets"
      :is-favorite="favorites.isFavorite(lightbox.viewing.value)"
      @close="lightbox.close"
      @step="stepViewer"
      @toggle-favorite="favorites.toggleOne(lightbox.viewing.value)"
    />
  </main>
</template>
