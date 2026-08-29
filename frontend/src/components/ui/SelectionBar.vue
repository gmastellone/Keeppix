<script setup lang="ts">
// The multi-select bar. Two parallel, independent pools feed this same
// component (library and culling batch, see stores/selection.ts) — it
// doesn't matter here which of the two: the component just receives a
// count and a label, the real separation lives in the store, not in the
// rendering.
//
// Scope is deliberately limited to the presentation-only part the spec
// constrains: correct singular/plural count, "Select all" that **never
// changes label** even when it's currently deselecting (no conditional
// text logic here), and the screen-reader announcement
// (`aria-live="polite" aria-atomic="true"`) — the latter guaranteed by
// the component itself, not left for every caller to remember.
//
// The visible bar disappears entirely at zero selected ("at zero the
// mode turns itself off") — but the spec describes the announcement
// region as its **own** node, off-screen, not nested inside
// `.selection-bar`. If the region only lived inside the markup that
// disappears, the "Selection cleared" announcement could never fire: the
// region would vanish at the exact instant it should be announcing.
// That's why the component root always stays mounted (the caller should
// never `v-if` it) and only the bar's visible content hides itself
// internally based on `count`.
//
// Action buttons stay outside the component: the library needs five
// (Favorite/Album/Share/Edit/Delete, per the mockup), culling needs
// three (Pick/Reject/Rename…) — completely different icons and labels,
// and some open dialogs that don't exist yet as shared components (album
// picker, sharing). The caller composes them in the default slot with
// Tooltip+BusyButton.
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{ count: number; ariaLabel: string }>()
const emit = defineEmits<{ clear: []; 'select-all': [] }>()
</script>

<template>
  <div>
    <div
      v-if="count > 0"
      role="toolbar"
      :aria-label="ariaLabel"
      class="flex flex-wrap items-center justify-between gap-3 rounded-[10px] bg-border/30 p-2"
    >
      <div class="flex items-center gap-3">
        <button
          type="button"
          :aria-label="t('ui.selectionBar.cancel')"
          class="flex h-7 w-7 items-center justify-center rounded-lg text-content-muted
                 hover:bg-border/50"
          @click="emit('clear')"
        >
          <svg
            viewBox="0 0 20 20"
            class="h-4 w-4"
            fill="none"
            aria-hidden="true"
          >
            <path
              d="M5 5l10 10M15 5L5 15"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
            />
          </svg>
        </button>
        <b class="text-[13.5px]">{{ t('ui.selectionBar.count', { n: count }, { plural: count }) }}</b>
        <button
          type="button"
          class="text-[12.5px] font-semibold text-accent"
          @click="emit('select-all')"
        >
          {{ t('ui.selectionBar.selectAll') }}
        </button>
      </div>
      <div class="flex flex-wrap gap-1.5">
        <slot />
      </div>
    </div>
    <span
      role="status"
      aria-live="polite"
      aria-atomic="true"
      class="sr-only"
    >
      {{
        count > 0
          ? t('ui.selectionBar.announceSelected', { n: count }, { plural: count })
          : t('ui.selectionBar.announceCleared')
      }}
    </span>
  </div>
</template>
