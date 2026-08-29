<script setup lang="ts">
// The "inline" form of the error pattern: used when only one piece is
// missing and the rest of the page loaded fine — a horizontal box, not
// the whole view, but not silent either. Verified against the
// prototype's `errorInlineHTML` (keeppix-mockup.html lines 3164-3173):
// 17px "alert" icon, message that grows to fill the space, compact ghost
// "Retry" button — no reload icon here, text only, unlike the full-view
// form.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { canRetry, type ErrorNature } from '@/errors/classify'

const props = defineProps<{ nature: ErrorNature }>()

const emit = defineEmits<{ retry: [] }>()

const { t } = useI18n()
const showRetry = computed(() => canRetry(props.nature))
</script>

<template>
  <div
    role="alert"
    class="flex items-center gap-2 rounded-lg bg-danger/10 px-3 py-2 text-sm text-danger"
  >
    <svg
      class="h-[17px] w-[17px] shrink-0"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d="M12 9v4M12 17h.01" />
      <path
        d="M10.3 3.9L1.8 18a2 2 0 001.7 3h17a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0z"
      />
    </svg>
    <span class="flex-1">{{ t(`errors.${nature}.title`) }}</span>
    <button
      v-if="showRetry"
      type="button"
      class="shrink-0 rounded px-2 py-1 text-sm font-medium hover:bg-danger/10"
      @click="emit('retry')"
    >
      {{ t('ui.toast.retry') }}
    </button>
  </div>
</template>
