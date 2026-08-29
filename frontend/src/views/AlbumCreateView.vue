<script setup lang="ts">
// Three real constraints from the backend, verified by reading the code:
//
// 1. **No "Automatic" mode**: on the real backend `PatchAlbumBody`
//    (`crates/keeppix-api/src/routes/albums.rs`) has no `rule` field:
//    once an album is created with a `rule`, that field can never be
//    cleared again — becoming fully manual later is therefore not
//    reachable. The only real mode is "apply now" (creation +
//    an immediate `POST .../refresh`) — the "when to apply" segmented
//    control disappears because there is no real second option to offer;
//    the album can still be refreshed later from `AlbumDetailView`
//    (an honest bonus, a natural consequence of how `rule` actually
//    works).
//
// 2. **No "Shared" switch**: `is_shared` is a real column but no route
//    ever writes it (verified: `CreateAlbumBody` and `PatchAlbumBody`
//    have no such field) — it stays `false`, the same story as
//    `cover_tint`/`monochrome`. Real sharing is the permissions/link
//    flow already built elsewhere, applied after creation.
//
// 3. **"File type" only offers RAW/JPEG, not RAW+JPEG**: `SearchNode::
//    Type` (search.rs) filters on `assets.kind` (image/raw_image/video/
//    unknown) — a per-file concept. "RAW+JPEG" is the client-side
//    `raw_kind` pairing (`useBrowseFilters.ts`, read from
//    `TimelineAsset.raw_kind`, never from a SQL query): there is no
//    `SearchNode` that represents it, so it cannot be represented in a
//    persisted `rule`. "Camera"/"Country" are not dropdowns with a
//    distinct-values list (no "list all values" route exists, only
//    `GET /search/suggest?q=` with a non-empty prefix): they're text
//    inputs with a `<datalist>` fed by the same endpoint as the search
//    bar, not a real dropdown. "Lens" doesn't even have a
//    `SuggestionKind::Lens` on the backend: it stays a free-text field,
//    with no suggestions.
import { onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { createAlbum, refreshAlbum } from '@/api/albums'
import { fetchAllFolders, type FolderView } from '@/api/folders'
import { runSearch } from '@/api/library'
import { fetchSuggestions } from '@/api/search'
import type { IsoCmp, PickValue, SearchNode } from '@/search/ast'
import Popover from '@/components/ui/Popover.vue'
import SegmentedControl from '@/components/ui/SegmentedControl.vue'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const router = useRouter()
const toast = useToastStore()

const name = ref('')
const nameInputEl = ref<HTMLInputElement | null>(null)
const collectionType = ref<'manual' | 'filter'>('manual')
const operator = ref<'and' | 'or'>('and')
const creating = ref(false)

const collectionTypeOptions = [
  { value: 'manual', label: t('albums.create.manual') },
  { value: 'filter', label: t('albums.create.filterBased') }
]
const operatorOptions = [
  { value: 'and', label: t('albums.create.operatorAnd') },
  { value: 'or', label: t('albums.create.operatorOr') }
]

type ConditionField = 'folder' | 'date_range' | 'country' | 'camera' | 'lens' | 'type' | 'favorite' | 'rating' | 'pick'

const FIELD_ORDER: ConditionField[] = [
  'folder',
  'date_range',
  'country',
  'camera',
  'lens',
  'type',
  'favorite',
  'rating',
  'pick'
]

type ConditionValue =
  | Set<string> // folder
  | { from: string; to: string } // date_range
  | string // country / camera / lens
  | '' | 'raw' | 'jpeg' // type
  | '' | 'yes' | 'no' // favorite
  | number // rating (0 = "Scegli…")
  | '' | PickValue // pick

interface ConditionRow {
  id: number
  field: ConditionField
  value: ConditionValue
}

function defaultValueFor(field: ConditionField): ConditionValue {
  switch (field) {
    case 'folder':
      return new Set<string>()
    case 'date_range':
      return { from: '', to: '' }
    case 'country':
    case 'camera':
    case 'lens':
      return ''
    case 'type':
    case 'favorite':
    case 'pick':
      return ''
    case 'rating':
      return 0
  }
}

let nextRowId = 1
const conditions = ref<ConditionRow[]>([{ id: nextRowId++, field: 'folder', value: defaultValueFor('folder') }])

function addCondition() {
  conditions.value.push({ id: nextRowId++, field: 'folder', value: defaultValueFor('folder') })
}

function removeCondition(id: number) {
  conditions.value = conditions.value.filter((row) => row.id !== id)
}

function onFieldChange(row: ConditionRow) {
  row.value = defaultValueFor(row.field)
}

const folders = ref<FolderView[]>([])
onMounted(async () => {
  folders.value = await fetchAllFolders().catch(() => [])
})

function folderName(id: string): string {
  return folders.value.find((f) => f.id === id)?.name ?? id
}

function folderPicklistSummary(selected: Set<string>): string {
  if (selected.size === 0) return t('albums.create.folderPicklistAll')
  if (selected.size === 1) return folderName([...selected][0]!)
  return t('albums.create.folderPicklistN', { n: selected.size })
}

function toggleFolder(row: ConditionRow, id: string) {
  const current = row.value as Set<string>
  const next = new Set(current)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  row.value = next
}

// Datalist for Camera/Country: the same `fetchSuggestions` used by the
// search bar, not an exhaustive list — see the file's header comment,
// deviation 3.
const suggestionsByRow = ref<Record<number, string[]>>({})
let suggestTimer: ReturnType<typeof setTimeout> | undefined

function onSuggestInput(row: ConditionRow, kind: 'camera' | 'country') {
  const text = (row.value as string).trim()
  clearTimeout(suggestTimer)
  if (!text) {
    suggestionsByRow.value = { ...suggestionsByRow.value, [row.id]: [] }
    return
  }
  suggestTimer = setTimeout(async () => {
    const { suggestions } = await fetchSuggestions(text).catch(() => ({ suggestions: [] }))
    suggestionsByRow.value = {
      ...suggestionsByRow.value,
      [row.id]: suggestions.filter((s) => s.kind === kind).map((s) => s.value)
    }
  }, 250)
}

const RATING_OPTIONS = [1, 2, 3, 4, 5].map((n) => ({ value: n, label: '★'.repeat(n) + '☆'.repeat(5 - n) }))

function nodeFor(row: ConditionRow): SearchNode | null {
  switch (row.field) {
    case 'folder': {
      const ids = [...(row.value as Set<string>)]
      if (ids.length === 0) return null
      if (ids.length === 1) return { op: 'folder', id: ids[0]! }
      return { op: 'or', args: ids.map((id) => ({ op: 'folder', id }) as SearchNode) }
    }
    case 'date_range': {
      const { from, to } = row.value as { from: string; to: string }
      if (!from && !to) return null
      return {
        op: 'date_range',
        from: from ? `${from}T00:00:00Z` : '0001-01-01T00:00:00Z',
        to: to ? `${to}T23:59:59Z` : '9999-12-31T23:59:59Z'
      }
    }
    case 'country': {
      const value = (row.value as string).trim()
      return value ? { op: 'country', value } : null
    }
    case 'camera': {
      const value = (row.value as string).trim()
      return value ? { op: 'camera', value } : null
    }
    case 'lens': {
      const value = (row.value as string).trim()
      return value ? { op: 'lens', value } : null
    }
    case 'type': {
      const value = row.value as '' | 'raw' | 'jpeg'
      if (!value) return null
      return { op: 'type', value: value === 'raw' ? 'raw_image' : 'image' }
    }
    case 'favorite': {
      const value = row.value as '' | 'yes' | 'no'
      if (!value) return null
      return value === 'yes' ? { op: 'favorite' } : { op: 'not', arg: { op: 'favorite' } }
    }
    case 'rating': {
      const value = row.value as number
      if (!value) return null
      const cmp: IsoCmp = 'gte'
      return { op: 'rating', cmp, value }
    }
    case 'pick': {
      const value = row.value as '' | PickValue
      return value ? { op: 'pick', value } : null
    }
  }
}

function buildRule(): SearchNode | null {
  const nodes = conditions.value.map(nodeFor).filter((n): n is SearchNode => n !== null)
  if (nodes.length === 0) return null
  if (nodes.length === 1) return nodes[0]!
  return { op: operator.value, args: nodes }
}

// "Live preview": counts real matches via an exhaustive `runSearch` (the
// same paging loop as `FavoritesView.loadFavorites`) — this count is
// entirely real, unlike the N/range shown on the albums grid: the AST is
// evaluated live by the backend, not read from a materialized
// membership. Debounced: changing a value shouldn't relaunch a search on
// every keystroke.
const previewCount = ref(0)
const previewLoading = ref(false)
let previewTimer: ReturnType<typeof setTimeout> | undefined

async function countMatches(rule: SearchNode): Promise<number> {
  let count = 0
  let cursor: string | undefined
  do {
    const page = await runSearch(rule, cursor)
    count += page.assets.length
    cursor = page.next_cursor
  } while (cursor)
  return count
}

watch(
  [collectionType, operator, conditions],
  () => {
    clearTimeout(previewTimer)
    if (collectionType.value !== 'filter') {
      previewCount.value = 0
      return
    }
    previewTimer = setTimeout(async () => {
      const rule = buildRule()
      if (!rule) {
        previewCount.value = 0
        return
      }
      previewLoading.value = true
      try {
        previewCount.value = await countMatches(rule)
      } catch {
        previewCount.value = 0
      } finally {
        previewLoading.value = false
      }
    }, 300)
  },
  { deep: true }
)

function goBack() {
  void router.push('/albums')
}

async function submit() {
  const trimmed = name.value.trim()
  if (!trimmed) {
    toast.showError(t('albums.create.errorName'))
    nameInputEl.value?.focus()
    return
  }
  let rule: SearchNode | undefined
  if (collectionType.value === 'filter') {
    const built = buildRule()
    if (!built) {
      toast.showError(t('albums.create.errorCondition'))
      return
    }
    rule = built
  }
  if (creating.value) return
  creating.value = true
  try {
    const album = await createAlbum(trimmed, rule)
    if (rule) {
      // "One-time": applies the filter NOW, once — see deviation 1 in the
      // file's header comment for why there's no second "Automatic" mode
      // to offer here.
      await refreshAlbum(album.id).catch(() => {})
    }
    toast.show(t('albums.create.success', { name: trimmed }))
    await router.push(`/albums/${album.id}`)
  } catch {
    toast.showError(t('albums.create.error'))
  } finally {
    creating.value = false
  }
}
</script>

<template>
  <main class="mx-auto max-w-[560px] p-6">
    <button
      type="button"
      class="mb-3 flex items-center gap-1 text-[13px] text-content-muted hover:text-content"
      @click="goBack"
    >
      {{ t('albums.backLink') }}
    </button>
    <p class="text-[15px] font-bold">
      {{ t('albums.create.title') }}
    </p>
    <p class="mt-1 text-sm text-content-muted">
      {{ t('albums.create.subtitle') }}
    </p>

    <section class="mt-6">
      <label
        for="album-name"
        class="sr-only"
      >{{ t('albums.create.nameLabel') }}</label>
      <input
        id="album-name"
        ref="nameInputEl"
        v-model="name"
        class="w-full max-w-[360px] rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
        :placeholder="t('albums.create.namePlaceholderHint')"
      >
    </section>

    <section class="mt-6">
      <p class="mb-2 text-[12.5px] font-semibold text-content-muted">
        {{ t('albums.create.collectionTypeLabel') }}
      </p>
      <SegmentedControl
        v-model="collectionType"
        :options="collectionTypeOptions"
        :aria-label="t('albums.create.collectionTypeLabel')"
      />

      <p
        v-if="collectionType === 'manual'"
        class="mt-3 text-sm text-content-muted"
      >
        {{ t('albums.create.manualExplain') }}
      </p>

      <template v-else>
        <p class="mt-3 text-sm text-content-muted">
          {{ t('albums.create.filterExplain') }}
        </p>

        <p class="mt-4 mb-2 text-[12.5px] font-semibold text-content-muted">
          {{ t('albums.create.operatorLabel') }}
        </p>
        <SegmentedControl
          v-model="operator"
          :options="operatorOptions"
          :aria-label="t('albums.create.operatorLabel')"
        />

        <div class="mt-4 space-y-2">
          <div
            v-for="row in conditions"
            :key="row.id"
            class="flex flex-wrap items-center gap-2 rounded-[9px] bg-border/20 p-2.5"
          >
            <select
              v-model="row.field"
              class="min-w-[150px] rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
              @change="onFieldChange(row)"
            >
              <option
                v-for="field in FIELD_ORDER"
                :key="field"
                :value="field"
              >
                {{ t(`albums.create.fieldLabel.${field}`) }}
              </option>
            </select>

            <Popover
              v-if="row.field === 'folder'"
              align="start"
            >
              <template #trigger>
                <button
                  type="button"
                  role="button"
                  aria-haspopup="listbox"
                  class="min-w-[150px] rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-left text-[12.5px]"
                >
                  {{ folderPicklistSummary(row.value as Set<string>) }}
                </button>
              </template>
              <div
                role="listbox"
                aria-multiselectable="true"
                class="max-h-[260px] w-[240px] overflow-y-auto"
              >
                <div
                  v-for="folder in folders"
                  :key="folder.id"
                  role="option"
                  tabindex="0"
                  :aria-selected="(row.value as Set<string>).has(folder.id)"
                  class="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-[12.5px] hover:bg-border/30"
                  @click="toggleFolder(row, folder.id)"
                  @keydown.enter="toggleFolder(row, folder.id)"
                  @keydown.space.prevent="toggleFolder(row, folder.id)"
                >
                  <span
                    class="h-[15px] w-[15px] shrink-0 rounded border border-border-strong"
                    :class="(row.value as Set<string>).has(folder.id) && 'bg-accent'"
                  />
                  {{ folder.name }}
                </div>
              </div>
            </Popover>

            <template v-else-if="row.field === 'date_range'">
              <label class="text-[11.5px] text-content-muted">{{ t('albums.create.dateFrom') }}</label>
              <input
                v-model="(row.value as { from: string; to: string }).from"
                type="date"
                class="rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
              >
              <label class="text-[11.5px] text-content-muted">{{ t('albums.create.dateTo') }}</label>
              <input
                v-model="(row.value as { from: string; to: string }).to"
                type="date"
                class="rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
              >
            </template>

            <template v-else-if="row.field === 'country'">
              <input
                v-model="row.value as string"
                list="album-create-country-suggestions"
                :placeholder="t('albums.create.countryPlaceholder')"
                class="min-w-[150px] rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
                @input="onSuggestInput(row, 'country')"
              >
              <datalist id="album-create-country-suggestions">
                <option
                  v-for="value in suggestionsByRow[row.id] ?? []"
                  :key="value"
                  :value="value"
                />
              </datalist>
            </template>

            <template v-else-if="row.field === 'camera'">
              <input
                v-model="row.value as string"
                list="album-create-camera-suggestions"
                :placeholder="t('albums.create.cameraPlaceholder')"
                class="min-w-[150px] rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
                @input="onSuggestInput(row, 'camera')"
              >
              <datalist id="album-create-camera-suggestions">
                <option
                  v-for="value in suggestionsByRow[row.id] ?? []"
                  :key="value"
                  :value="value"
                />
              </datalist>
            </template>

            <input
              v-else-if="row.field === 'lens'"
              v-model="row.value as string"
              :placeholder="t('albums.create.lensPlaceholder')"
              class="min-w-[150px] rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
            >

            <select
              v-else-if="row.field === 'type'"
              v-model="row.value as string"
              class="min-w-[150px] rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
            >
              <option value="">
                {{ t('albums.create.choose') }}
              </option>
              <option value="raw">
                {{ t('albums.create.typeRaw') }}
              </option>
              <option value="jpeg">
                {{ t('albums.create.typeJpeg') }}
              </option>
            </select>

            <select
              v-else-if="row.field === 'favorite'"
              v-model="row.value as string"
              class="min-w-[150px] rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
            >
              <option value="">
                {{ t('albums.create.choose') }}
              </option>
              <option value="yes">
                {{ t('albums.create.favoriteYes') }}
              </option>
              <option value="no">
                {{ t('albums.create.favoriteNo') }}
              </option>
            </select>

            <select
              v-else-if="row.field === 'rating'"
              v-model.number="row.value as unknown as number"
              class="min-w-[150px] rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
            >
              <option :value="0">
                {{ t('albums.create.choose') }}
              </option>
              <option
                v-for="opt in RATING_OPTIONS"
                :key="opt.value"
                :value="opt.value"
              >
                {{ opt.label }}
              </option>
            </select>

            <select
              v-else-if="row.field === 'pick'"
              v-model="row.value as string"
              class="min-w-[150px] rounded-lg border border-border bg-surface-elevated px-2 py-1.5 text-[12.5px]"
            >
              <option value="">
                {{ t('albums.create.choose') }}
              </option>
              <option value="pick">
                {{ t('albums.create.pickPick') }}
              </option>
              <option value="reject">
                {{ t('albums.create.pickReject') }}
              </option>
              <option value="none">
                {{ t('albums.create.pickNone') }}
              </option>
            </select>

            <button
              type="button"
              :aria-label="t('albums.create.removeCondition')"
              class="ml-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-content-muted hover:bg-border/40 hover:text-danger"
              @click="removeCondition(row.id)"
            >
              ×
            </button>
          </div>
        </div>

        <button
          type="button"
          class="mt-3 flex items-center gap-1.5 rounded-lg border border-border px-2.5 py-1.5 text-[12.5px] font-semibold"
          @click="addCondition"
        >
          + {{ t('albums.create.addCondition') }}
        </button>

        <div class="mt-4 rounded-lg bg-accent-tint px-3 py-2 text-[12.5px] text-accent">
          <b>{{ previewLoading ? '…' : previewCount }}</b> {{ t('albums.create.previewSuffix') }}
        </div>
      </template>
    </section>

    <div class="mt-6 flex gap-2">
      <button
        type="button"
        class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-white disabled:opacity-50"
        :disabled="creating"
        @click="submit"
      >
        {{ t('albums.create.submit') }}
      </button>
      <button
        type="button"
        class="rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold"
        @click="goBack"
      >
        {{ t('albums.create.cancel') }}
      </button>
    </div>
  </main>
</template>
