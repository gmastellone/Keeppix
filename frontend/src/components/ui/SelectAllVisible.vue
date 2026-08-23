<script setup lang="ts">
// SP-4 ("Seleziona tutto quello che vedi", documento funzionale,
// definizione canonica): un comando icona nella toolbar delle viste a
// griglia. Seleziona esattamente ciò che è visibile in quel momento —
// se un filtro rapido o una ricerca sono attivi, solo ciò che ci ricade
// dentro — mai l'intera libreria sottostante. Questo componente non sa
// nulla di filtri o dati: il chiamante gli passa quanti elementi sono
// visibili e ascolta l'evento; l'insieme vero e proprio da selezionare
// va allo stesso `store.selection.*.selectAllVisible(visibleIds)` già
// costruito per SP-2, che implementa la vera semantica di toggle.
//
// "Scompare quando non c'è nulla, non si disabilita" (nota vincolante
// del piano): nessuna variante disabilitata da disegnare — a zero
// elementi visibili il componente semplicemente non si monta.
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
