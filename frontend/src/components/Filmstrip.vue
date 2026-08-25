<script setup lang="ts">
import { useI18n } from 'vue-i18n'

import { thumbSrc as mediaThumbSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'

// Nome multi-parola esplicito: il file si chiama `Filmstrip.vue` per
// combaciare col piano, ma `vue/multi-word-component-names` vuole un nome
// di componente di più parole per non confondersi con futuri elementi HTML.
defineOptions({ name: 'CullingFilmstrip' })

// Corpo della miniatura e checkbox sono due `<button>` **fratelli**, non
// annidati: il documento funzionale (§15.5) descrive solo la checkbox
// come raggiungibile da Tab nel mockup, ma quella è una descrizione del
// prototipo — e l'eccezione dichiarata all'inizio del documento stesso
// ("l'accessibilità da tastiera del prototipo è rotta e non va
// replicata") vince sul Ruling generale del piano ("ogni cosa cliccabile
// è un pulsante vero"). Annidare la checkbox dentro il pulsante di
// navigazione sarebbe comunque HTML non valido (controlli interattivi
// annidati); da fratelli, un click sulla checkbox non deve nemmeno
// fermare la propagazione — non ha un pulsante genitore da cui risalire.
const props = defineProps<{
  assets: TimelineAsset[]
  currentId?: string
  selectedIds?: Set<string>
}>()
// Quattro eventi distinti (§15.4, "Sul filmino"): il corpo della miniatura
// naviga (con/senza shift), la checkbox seleziona (con/senza shift). La
// decisione "se non c'è ancora un'ancora lo shift+click sulla checkbox
// vale come click semplice" vive nello store (ha bisogno di `order`), non
// qui: questo componente resta solo presentazione, come già `currentId`.
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
