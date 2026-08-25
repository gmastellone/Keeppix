<script setup lang="ts">
// Fase 11 Task 16 (2/N), §31.2-§31.3 — la scheda persona, estratta da
// `PeopleView.vue` nel momento in cui deve ospitare anche la casella di
// spunta di selezione (comparsa in questa stessa unità), non annidabile
// dentro un vero `<button>` (HTML non valido) — stesso motivo per cui
// `TagRow.vue` (Task 15 1/N) è un `<div role="button">` con un
// `<button>` nidificato, non un vero `<button>`.
//
// **Corretto rispetto al documento, non riprodotto**: §31.5 segnala
// esplicitamente "la scheda persona non è raggiungibile da tastiera…
// è una lacuna di accessibilità, non una scelta documentata" — qui ha
// `role="button" tabindex="0"` + Invio/Spazio (SP-8), stesso principio
// già seguito per i difetti dichiarati altrove in questa fase (pastiglie
// colore di `TagEditorDialog.vue`, dialog `ProblemFilesDialog.vue`).
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { thumbSrc } from '@/api/media'
import type { Person } from '@/api/persons'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

const props = defineProps<{
  person: Person
  cover: { hash: string | null; thumbhash: string | null } | null
  selected: boolean
}>()
const emit = defineEmits<{ open: []; toggleSelect: [] }>()

const { t } = useI18n()

const displayName = computed(() => props.person.name?.trim() || t('persons.unnamed'))

const coverStyle = computed(() => {
  if (props.cover?.hash) return { backgroundImage: `url(${thumbSrc(props.cover.hash)})` }
  if (props.cover?.thumbhash) {
    const url = thumbhashToDataURL(props.cover.thumbhash)
    if (url) return { backgroundImage: `url(${url})` }
  }
  return {}
})

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    emit('open')
  }
}
</script>

<template>
  <div
    role="button"
    tabindex="0"
    class="group flex flex-col items-center gap-2 rounded-lg p-2 text-center hover:bg-border/20
           focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
    @click="emit('open')"
    @keydown="onKeydown"
  >
    <span class="relative">
      <span
        class="h-[78px] w-[78px] rounded-full border border-border bg-cover bg-center bg-surface-elevated"
        :class="selected && 'ring-[3px] ring-accent'"
        :style="coverStyle"
        aria-hidden="true"
      />
      <button
        type="button"
        role="checkbox"
        :aria-checked="selected"
        :aria-label="t('persons.selectAriaLabel', { name: displayName })"
        class="absolute -left-1 -top-1 flex h-5 w-5 items-center justify-center rounded-full border-2 border-white
               text-[11px] font-bold text-white transition-opacity
               focus-visible:opacity-100 group-hover:opacity-100 group-focus-within:opacity-100"
        :class="selected ? 'bg-accent opacity-100' : 'bg-black/40 opacity-0'"
        @click.stop="emit('toggleSelect')"
      >
        <template v-if="selected">✓</template>
      </button>
    </span>
    <span class="w-full truncate text-[12.5px] font-semibold">
      {{ displayName }}
      <span
        v-if="!person.name"
        class="font-semibold text-accent"
      > · {{ t('persons.unnamedHint') }}</span>
    </span>
    <span class="text-[11px] text-content-muted">
      {{ t('persons.photoCount', { n: person.face_count ?? 0 }, { plural: person.face_count ?? 0 }) }}
    </span>
  </div>
</template>
