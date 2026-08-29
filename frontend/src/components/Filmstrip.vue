<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { thumbSrc as mediaThumbSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'

// Explicit multi-word name: the file is called `Filmstrip.vue`, but
// `vue/multi-word-component-names` wants a multi-word component name so it
// doesn't get confused with future HTML elements.
defineOptions({ name: 'CullingFilmstrip' })

// The thumbnail body and the checkbox are two **sibling** `<button>`s, not
// nested: nesting the checkbox inside the navigation button would be
// invalid HTML (nested interactive controls) regardless. As siblings, a
// click on the checkbox doesn't even need to stop propagation — it has no
// parent button to bubble up to.
const props = defineProps<{
  assets: TimelineAsset[]
  currentId?: string
  selectedIds?: Set<string>
}>()
// Four distinct events: the thumbnail body navigates (with/without shift),
// the checkbox selects (with/without shift). The decision "if there's no
// anchor yet, shift+click on the checkbox counts as a plain click" lives in
// the store (it needs `order`), not here — this component stays purely
// presentational, same as `currentId` already is.
const emit = defineEmits<{
  select: [id: string]
  'shift-select': [id: string]
  toggle: [id: string]
  'shift-toggle': [id: string]
}>()
const { t } = useI18n()

function thumbSrc(asset: TimelineAsset): string | undefined {
  return asset.content_hash ? mediaThumbSrc(asset.content_hash) : undefined
}

function isSelected(id: string): boolean {
  return props.selectedIds?.has(id) ?? false
}

function onThumbClick(event: MouseEvent, id: string) {
  if (event.shiftKey) emit('shift-select', id)
  else emit('select', id)
}

function onCheckboxClick(event: MouseEvent, id: string) {
  if (event.shiftKey) emit('shift-toggle', id)
  else emit('toggle', id)
}
</script>

<template>
  <div
    class="flex gap-1.5 overflow-x-auto border-t border-b border-border bg-black/40 px-10 py-2.5"
    role="listbox"
    :aria-label="t('culling.filmstrip.label')"
  >
    <div
      v-for="(asset, i) in assets"
      :key="asset.id"
      class="group/thumb relative h-[58px] w-[58px] shrink-0"
    >
      <button
        type="button"
        role="option"
        class="block h-full w-full overflow-hidden rounded-md border-2"
        :class="[
          asset.id === currentId ? 'border-accent' : 'border-transparent',
          isSelected(asset.id) ? 'shadow-[0_0_0_2px_var(--color-accent)]' : ''
        ]"
        :aria-selected="asset.id === currentId"
        :aria-label="asset.filename"
        @click="onThumbClick($event, asset.id)"
      >
        <img
          v-if="thumbSrc(asset)"
          :src="thumbSrc(asset)"
          :alt="asset.filename"
          class="h-full w-full object-cover"
        >
      </button>
      <button
        type="button"
        role="checkbox"
        :aria-checked="isSelected(asset.id)"
        :aria-label="t('culling.filmstrip.selectPhoto', { n: i + 1 })"
        class="absolute top-1 right-1 flex h-4 w-4 items-center justify-center rounded border transition-opacity"
        :class="
          isSelected(asset.id)
            ? 'border-accent bg-accent opacity-100'
            : 'border-white bg-black/40 opacity-0 group-hover/thumb:opacity-100 focus-visible:opacity-100'
        "
        @click="onCheckboxClick($event, asset.id)"
      >
        <svg
          v-if="isSelected(asset.id)"
          viewBox="0 0 20 20"
          class="h-2.5 w-2.5"
          fill="none"
          aria-hidden="true"
        >
          <path
            d="M4 10l4 4 8-8"
            stroke="white"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    </div>
  </div>
</template>
