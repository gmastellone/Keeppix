<script setup lang="ts">
// The whole row opens the editor; the (nested) trash icon opens the delete
// confirmation without opening the row — it needs `@click.stop`, so the
// row cannot be a real `<button>` (nesting `<button>` inside `<button>` is
// invalid HTML): `role="button"` + `tabindex="0"` + an Enter/Space
// handler, the same pattern already used for non-nestable rows elsewhere
// in the app.
import type { Tag } from '@/api/tags'
import { useI18n } from 'vue-i18n'

defineProps<{ tag: Tag }>()
const emit = defineEmits<{ edit: []; delete: [] }>()

const { t } = useI18n()

function showsPrompt(tag: Tag): boolean {
  const prompt = tag.prompt?.trim()
  if (!prompt) return false
  return prompt.toLowerCase() !== tag.name.trim().toLowerCase()
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    emit('edit')
  }
}
</script>

<template>
  <div
    role="button"
    tabindex="0"
    class="flex w-full items-center gap-2.5 border-b border-border px-3.5 py-2.5 text-left last:border-b-0
           hover:bg-border/20 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
    :aria-label="t('tags.editTag', { name: tag.name })"
    @click="emit('edit')"
    @keydown="onKeydown"
  >
    <span
      class="h-[11px] w-[11px] shrink-0 rounded-full"
      aria-hidden="true"
      :style="{ background: tag.color ?? 'var(--color-border-strong)' }"
    />
    <span class="min-w-0 flex-1">
      <span class="block truncate text-[13.5px] font-semibold">{{ tag.name }}</span>
      <span
        v-if="showsPrompt(tag)"
        class="block truncate text-[11.5px] text-content-muted"
      >{{ t('tags.promptLine', { prompt: tag.prompt }) }}</span>
    </span>
    <span class="shrink-0 text-[12px] text-content-muted">{{ t('tags.photoCount', { n: tag.assignment_count }, { plural: tag.assignment_count }) }}</span>
    <span
      class="shrink-0 rounded-full bg-border/40 px-1.5 py-0.5 text-[10.5px] font-bold"
      :title="t('tags.thresholdTooltip')"
    >{{ Math.round((tag.threshold ?? 0.2) * 100) }}%</span>
    <button
      type="button"
      class="shrink-0 rounded-md px-1.5 py-1 text-[12px] text-content-muted hover:bg-danger/10 hover:text-danger"
      :aria-label="t('tags.deleteTag', { name: tag.name })"
      @click.stop="emit('delete')"
    >
      {{ t('tags.delete') }}
    </button>
  </div>
</template>
