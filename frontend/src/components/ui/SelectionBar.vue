<script setup lang="ts">
// SP-2 (documento funzionale §12, definizione canonica): la barra di
// selezione multipla. Due pool paralleli e indipendenti alimentano
// questo stesso componente (libreria e lotto di culling, vedi
// stores/selection.ts) — qui non importa quale dei due: il componente
// riceve solo un conteggio e un'etichetta, la separazione vera vive nello
// store, non nella resa.
//
// Ambito volutamente limitato alla parte di sola presentazione che il
// documento vincola: conteggio singolare/plurale corretto, "Seleziona
// tutte" che **non cambia mai etichetta** anche quando in quel momento
// deseleziona (§12.7 — nessuna logica di testo condizionale qui), e
// l'annuncio per screen reader (`aria-live="polite" aria-atomic="true"`,
// §12.2) — quest'ultimo garantito dal componente stesso, non lasciato al
// chiamante da ricordare ogni volta.
//
// La barra vera e propria sparisce del tutto a zero selezionate ("a zero
// la modalità si spegne da sola", §12.7) — ma il documento descrive la
// regione d'annuncio come un nodo **a sé**, fuori schermo, non dentro
// `.selection-bar`. Se la regione vivesse solo dentro il markup che
// sparisce, l'annuncio "Selezione annullata" non potrebbe mai
// scattare: la regione sparirebbe nello stesso istante in cui dovrebbe
// annunciare. Per questo la radice del componente resta sempre montata
// (il chiamante non deve mai `v-if`rla) e solo il contenuto visibile
// della barra si nasconde internamente sotto `count`.
//
// I pulsanti di azione restano fuori dal componente: la libreria ne ha
// cinque (Preferiti/Album/Condividi/Modifica/Elimina, righe 2176-2182 del
// mockup), il culling tre (Scelta/Scarta/Rinomina…, righe 5002-5004) —
// icone ed etichette completamente diverse, e alcuni aprono dialog che
// non esistono ancora come componenti condivisi (selettore album,
// condivisione). Il chiamante li compone nello slot di default con
// Tooltip+BusyButton, già costruiti in questo stesso Task.
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
