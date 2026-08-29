<script setup lang="ts">
// The quick chip filter. Built on top of `Popover.vue`, not a
// reimplementation — open/close on click, click-outside closes, Esc
// closes, the same contract reka-ui already guarantees for the popover.
// The funnel button **has no** tooltip ("unlike 'Select all', the
// filter button has no data-tip, only aria-label") — no `Tooltip` here,
// unlike `SelectAllVisible`.
//
// Declared scope: the component is **generic with respect to
// dimensions**. The real dimensions used by the app (Type/People/Tags/
// Categories/Camera/Location) depend on stores that don't exist yet in
// this codebase (people, tags, cameras, folders) — the screen that
// actually uses them will build them from its own data sources. The
// OR-within/AND-across-dimensions combination logic already lives, pure
// and tested, in `design/quickFilter.ts`.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import Popover from './Popover.vue'

export interface QuickFilterOption {
  value: string
  label: string
  color?: string
}

export interface QuickFilterDimension {
  id: string
  label: string
  options: QuickFilterOption[]
}

const props = defineProps<{
  dimensions: QuickFilterDimension[]
  resultCount: number
  hint?: string
}>()

const selection = defineModel<Record<string, Set<string>>>('selection', { required: true })

const { t } = useI18n()

const open = ref(false)
const searchTerms = ref<Record<string, string>>({})

// Sum of chosen values across all dimensions, not the number of active
// dimensions: two tags and one camera show 3.
const activeCount = computed(() =>
  Object.values(selection.value).reduce((sum, set) => sum + set.size, 0)
)

function isActive(dimensionId: string, value: string): boolean {
  return selection.value[dimensionId]?.has(value) ?? false
}

function toggle(dimensionId: string, value: string) {
  const current = new Set(selection.value[dimensionId] ?? [])
  if (current.has(value)) current.delete(value)
  else current.add(value)
  selection.value = { ...selection.value, [dimensionId]: current }
}

function clearAll() {
  const cleared: Record<string, Set<string>> = {}
  for (const dimension of props.dimensions) cleared[dimension.id] = new Set()
  selection.value = cleared
}

// Only Tags and People can realistically grow large enough to need one
// (BROWSE_FILTER_SEARCH_THRESHOLD = 8) — not a per-dimension threshold,
// the same one applies to all.
const SEARCH_THRESHOLD = 8

function needsSearch(dimension: QuickFilterDimension): boolean {
  return dimension.options.length > SEARCH_THRESHOLD
}

// "Already-selected options always stay on top and are never filtered
// away" — this applies to search behavior: with no term typed, the
// original order is preserved.
function visibleOptions(dimension: QuickFilterDimension): QuickFilterOption[] {
  const term = (searchTerms.value[dimension.id] ?? '').trim().toLowerCase()
  if (!term) return dimension.options
  const matches = dimension.options.filter(
    (option) => isActive(dimension.id, option.value) || option.label.toLowerCase().includes(term)
  )
  return [...matches].sort((a, b) => {
    const aActive = isActive(dimension.id, a.value)
    const bActive = isActive(dimension.id, b.value)
    return aActive === bActive ? 0 : aActive ? -1 : 1
  })
}
</script>

<template>
  <Popover
    v-model:open="open"
    side="bottom"
    align="end"
  >
    <template #trigger>
      <button
        type="button"
        :aria-label="t('ui.quickFilter.trigger')"
        aria-haspopup="true"
        :aria-expanded="open"
        class="relative flex h-[26px] w-[26px] items-center justify-center rounded-[7px]
               text-content-muted hover:bg-border/40"
        :class="activeCount > 0 && 'bg-accent/15 text-accent'"
      >
        <svg
          viewBox="0 0 24 24"
          width="14"
          height="14"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M4 5h16l-6.5 7.8V19l-3 1.6v-7.8z" />
        </svg>
        <span
          v-if="activeCount > 0"
          class="absolute -top-1.5 -right-1.5 flex h-3.5 min-w-3.5 items-center justify-center
                 rounded-full bg-accent px-1 text-[9.5px] font-bold text-accent-text"
        >
          {{ activeCount }}
        </span>
      </button>
    </template>

    <div class="flex w-[280px] max-w-[calc(100vw-40px)] flex-col">
      <div class="flex items-center justify-between border-b border-border px-1 pb-2 text-[12.5px] font-bold">
        <span>{{ t('ui.quickFilter.title') }}</span>
        <button
          v-if="activeCount > 0"
          type="button"
          class="text-[11.5px] font-semibold text-accent hover:underline"
          @click="clearAll"
        >
          {{ t('ui.quickFilter.clearAll') }}
        </button>
      </div>

      <div class="flex max-h-[min(50vh,420px)] flex-col gap-3 overflow-y-auto py-2">
        <div
          v-for="dimension in dimensions"
          :key="dimension.id"
        >
          <p class="mb-1.5 px-1 text-[10px] font-semibold tracking-wide text-content-muted uppercase">
            {{ dimension.label }}
          </p>
          <input
            v-if="needsSearch(dimension)"
            v-model="searchTerms[dimension.id]"
            type="text"
            :placeholder="t('ui.quickFilter.searchPlaceholder', { n: dimension.options.length })"
            class="mb-1.5 w-full rounded-md border border-border px-2 py-1 text-[12px]
                   focus:border-accent focus:outline-none"
          >
          <p
            v-if="needsSearch(dimension) && visibleOptions(dimension).length === 0"
            class="px-1 text-[11.5px] text-content-muted"
          >
            {{ t('ui.quickFilter.noResults', { term: searchTerms[dimension.id] ?? '' }) }}
          </p>
          <div
            class="flex flex-wrap gap-1.5"
            :class="needsSearch(dimension) && 'max-h-[132px] overflow-y-auto'"
          >
            <button
              v-for="option in visibleOptions(dimension)"
              :key="option.value"
              type="button"
              class="flex items-center gap-1.5 rounded-full border border-transparent bg-border/30
                     px-2.5 py-1 text-[12px] text-content-muted hover:bg-border/50"
              :class="isActive(dimension.id, option.value) && 'border-accent bg-accent/15 font-semibold text-accent'"
              @click="toggle(dimension.id, option.value)"
            >
              <span
                v-if="option.color"
                class="h-2 w-2 rounded-full"
                :style="{ background: option.color }"
              />
              {{ option.label }}
            </button>
          </div>
        </div>
        <p
          v-if="hint"
          class="px-1 text-[11.5px] text-content-muted"
        >
          {{ hint }}
        </p>
      </div>

      <div class="border-t border-border bg-border/20 px-1 pt-2 text-[11.5px] text-content-muted">
        {{
          activeCount > 0
            ? t('ui.quickFilter.resultCountFiltered', { n: resultCount })
            : t('ui.quickFilter.resultCountTotal', { n: resultCount })
        }}
      </div>
    </div>
  </Popover>
</template>
