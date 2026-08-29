<script setup lang="ts">
// The pill-based search bar. `frontend/src/search/parse.ts` (the old text
// parser) has been retired: the AST (`frontend/src/search/ast.ts`, same
// type, without the tokenizer) is now built only from pills plus a
// `text` node for free-text description.
//
// The suggestion categories come from two real sources, not a single
// list:
//  - "Tag" is built here, not from `GET /search/suggest`: that endpoint
//    (`crates/keeppix-db/src/search.rs:396-460`) never produces rows of
//    kind `tag` — the code comment there says so explicitly — so we
//    filter `fetchTags()` client-side instead.
//  - "Camera"/"Folder"/"ISO"/"Year"/"Country" come from the real
//    `GET /search/suggest?q=`, which computes over real library data.
//  - "Location" (GPS) has no real source (no `SuggestionKind` for this):
//    a single pseudo-generated row shown when the text is a substring of
//    "gps".
//  - "Country" shows the raw ISO code (`p.country_code`, e.g. "IT"): the
//    backend has no code→name table and no other view in the app has one
//    either.
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

// --- pills ---
type PillType = 'tag' | 'camera' | 'folder' | 'iso' | 'year' | 'gps' | 'country'

interface SearchPill {
  type: PillType
  /** Raw value used to build the `SearchNode` (tag id, folder id, camera
   * text, ISO/year number as a string, "gps", country code). */
  value: string
  /** "Bare name" to display — `pillText()` adds the prefix where
   * expected (Tag/ISO/Year/GPS). */
  label: string
  color?: string | null
}

const pills = ref<SearchPill[]>([])

// A type chip alone doesn't count as a search. Declared here, before
// `lightbox`/`visibleAssets`: `useLightboxRoute`'s immediate watcher
// (below) reads `visibleAssets.value` synchronously during setup, and
// `visibleAssets` depends on `hasSearch` — so it must already exist at
// that point, not further down in the file (temporal dead zone
// otherwise).
const hasSearch = computed(() => pills.value.length > 0 || q.value.trim().length > 0)

// --- results vs discovery: two different sources feed the same grid,
// never together — `assets` (a real search, every page) when `hasSearch`,
// `recentAssets` (a single page) when there is no search. See
// `refresh()`/`loadRecent()`.
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

// --- suggestion panel ---
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
    // Empty field but focused: encourage discovery with a few folders
    // alongside tags — only Tag (first 5) and Folder.
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

// --- file type chips ---
// Mutually exclusive, "All types" is the default and there's no way back
// to "none" — you only move from one chip to another. "RAW" and "JPEG"
// filter on the `kind` of the stack's primary row (`SearchNode::Type`,
// `crates/keeppix-db/src/search.rs:911-914` — "raw" is an alias for
// "raw_image"): the real system also has `video`/`unknown`, so "JPEG"
// here filters exactly `kind==='image'`, not generically "not RAW",
// otherwise it would also include videos.
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

// --- results ---
// A file type chip alone does **not** count as a search — the discovery
// state stays active even with "RAW" selected, but the "Recently added"
// grid that state shows is still filtered (same `buildAst()`) — the 32
// photos shown are still RAW-only filtered, silently, with nothing
// saying so. `hasSearch` itself is declared much higher up in the file —
// see the comment there.

// Each piece (pill or text) is bolded on its own, joined by a
// non-bold " + ".
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

// At most 32 photos — the backend already orders by `taken_at_utc DESC`
// (`crates/keeppix-db/src/search.rs:246`), so a single page (200 rows by
// default, no cursor ever needed) is enough. This uses real recency
// rather than an approximation, and it's cheaper too (no exhaustive
// pagination of the whole library just for a discovery widget).
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

// --- folders ---
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

// There is no "photos scoped to a folder" view to reach in the real app
// (no route/parameter to open the timeline already filtered to a folder
// — verified: `TimelineView.vue` has no concept of a current folder,
// `FoldersView.vue` doesn't read route parameters). The closest real
// destination is the Folders view itself, from which the user enters the
// chosen folder — not a direct jump, but not a dead link either.
function openFolders() {
  void router.push('/folders')
}

// --- saved searches ---
const savedSearches = ref<SavedSearch[]>([])
const savedJustNow = ref(false)

function quoteIfNeeded(value: string): string {
  return /\s/.test(value) ? `"${value.replace(/"/g, '')}"` : value
}

// The text grammar the backend can still parse (`crates/keeppix-db/src/
// search.rs:696-798`, `parse_query_text`/`value_node`) predates this
// pill UI: it only understands `type:`/`camera:`/`lens:`/`iso:`/
// `folder:`/`has:gps`/a bare 4-digit year/free text (quoted if it
// contains spaces) — it has **never** learned `tag:`, `country:`, or a
// keyword for "favorites". A tag/country pill or the "Favorites" chip
// therefore can't be serialized into `query_text`: `null` here disables
// "Save this search" instead of writing a saved search that, reloaded,
// would behave differently from the current one — silently wrong would
// be worse than unavailable.
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

    <!-- "Searched" state: "Results" title, recap, "Save this search".
         The discovery sections disappear. -->
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

    <!-- Initial state: saved searches + folders, only when no search is
         active. -->
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

    <!-- "No results" — only in the "searched" state. -->
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
