<script setup lang="ts">
// A group of AI suggestions for **one** tag or person, awaiting
// confirmation or rejection — never applied on their own. "Tags and
// faces, same shape": the same component serves both Review queues. The
// only real difference between the two domains is that faces have a
// third per-thumbnail button ("Not a face" — solid `--danger` fill,
// unlike the two normal buttons) that tags don't have: exposed here as a
// scoped slot (`extra-actions`) rather than a prop specific to a domain
// the other doesn't share.
import { useI18n } from 'vue-i18n'

export interface SuggestionThumbnail {
  id: string
  thumbnailUrl: string
}

defineProps<{
  label: string
  count: number
  /** Only tags have a color dot; faces don't. */
  color?: string
  thumbnails: SuggestionThumbnail[]
}>()

const emit = defineEmits<{
  'confirm-all': []
  'reject-all': []
  confirm: [id: string]
  reject: [id: string]
}>()

const { t } = useI18n()
</script>

<template>
  <div class="rounded-xl border border-border p-3.5">
    <div class="mb-2.5 flex flex-wrap items-center justify-between gap-2">
      <div class="flex items-center gap-2">
        <span
          v-if="color"
          class="h-2 w-2 rounded-full"
          :style="{ background: color }"
        />
        <b class="text-[13.5px]">«{{ label }}»</b>
        <span class="text-xs text-content-muted">
          {{ t('ui.suggestionQueue.count', { n: count }, { plural: count }) }}
        </span>
      </div>
      <div class="flex gap-1.5">
        <button
          type="button"
          class="rounded-lg border border-border px-2.5 py-1 text-[12px] font-medium
                 text-content hover:bg-border/40"
          @click="emit('confirm-all')"
        >
          {{ t('ui.suggestionQueue.confirmAll') }}
        </button>
        <button
          type="button"
          class="rounded-lg px-2.5 py-1 text-[12px] font-medium text-content-muted
                 hover:bg-border/40"
          @click="emit('reject-all')"
        >
          {{ t('ui.suggestionQueue.rejectAll') }}
        </button>
      </div>
    </div>

    <div class="flex flex-wrap gap-2">
      <div
        v-for="thumbnail in thumbnails"
        :key="thumbnail.id"
        class="group relative h-[74px] w-[74px]"
      >
        <img
          :src="thumbnail.thumbnailUrl"
          alt=""
          class="h-full w-full rounded-lg border-[1.5px] border-dashed border-accent object-cover opacity-[.92]"
        >
        <span
          aria-hidden="true"
          class="absolute top-1 left-1 rounded-[4px] bg-accent/20 px-1 py-0.5 text-[8.5px]
                 font-bold text-accent"
        >
          {{ t('ui.suggestionQueue.aiBadge') }}
        </span>
        <div
          class="absolute inset-0 flex items-center justify-center gap-1.5 rounded-lg bg-black/50
                 opacity-0 transition-opacity duration-[var(--duration-fast)]
                 ease-[var(--easing-standard)] group-hover:opacity-100 group-focus-within:opacity-100"
        >
          <button
            type="button"
            :aria-label="t('ui.suggestionQueue.confirm')"
            class="flex h-[26px] w-[26px] items-center justify-center rounded-full bg-white text-[#111]"
            @click="emit('confirm', thumbnail.id)"
          >
            <svg
              viewBox="0 0 24 24"
              width="13"
              height="13"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M20 6L9 17l-5-5" />
            </svg>
          </button>
          <slot
            :id="thumbnail.id"
            name="extra-actions"
          />
          <button
            type="button"
            :aria-label="t('ui.suggestionQueue.reject')"
            class="flex h-[26px] w-[26px] items-center justify-center rounded-full bg-white text-danger"
            @click="emit('reject', thumbnail.id)"
          >
            <svg
              viewBox="0 0 24 24"
              width="13"
              height="13"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
