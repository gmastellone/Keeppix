<script setup lang="ts">
// "Select everything you see": an icon command in the grid views'
// toolbar. Selects exactly what's currently visible — if a quick filter
// or search is active, only what falls within it — never the entire
// underlying library. This component knows nothing about filters or
// data: the caller passes it how many items are visible and listens for
// the event; the actual set to select goes through the same
// `store.selection.*.selectAllVisible(visibleIds)` used by the selection
// bar, which implements the real toggle semantics.
//
// "Disappears when there's nothing, never disables": no disabled variant
// to design — at zero visible items the component simply doesn't mount.
import { useI18n } from 'vue-i18n'

import Tooltip from './Tooltip.vue'

const { t } = useI18n()

defineProps<{ visibleCount: number }>()
const emit = defineEmits<{ 'select-all': [] }>()
</script>

<template>
  <Tooltip
    v-if="visibleCount > 0"
    :label="t('ui.selectAllVisible.tooltip')"
  >
    <button
      type="button"
      :aria-label="t('ui.selectAllVisible.ariaLabel')"
      class="flex h-8 w-8 items-center justify-center rounded-lg text-content-muted
             hover:bg-border/40"
      @click="emit('select-all')"
    >
      <svg
        viewBox="0 0 24 24"
        width="15"
        height="15"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <rect
          x="3"
          y="3"
          width="18"
          height="18"
          rx="4"
        />
        <path d="M8 12.5l2.5 2.5L16 9" />
      </svg>
    </button>
  </Tooltip>
</template>
