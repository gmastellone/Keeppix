<script setup lang="ts">
// SP-28 (documento funzionale, Parte X "Errore" — definizione canonica
// §68), forma "a piena vista": quando è mancato il contenuto principale
// della schermata. Verificato contro `errorStateHTML` del prototipo
// (keeppix-mockup.html righe 3150-3163, icona "alert" 34px + titolo +
// spiegazione + azione + dettaglio facoltativo) — le altre due forme di
// SP-28 (in riga, messaggio temporaneo) sono componenti a sé
// (`InlineError.vue`; il messaggio temporaneo è già `ToastHost`, Task 2).
//
// "Riprova" **non ha stato disabilitato** (§68.7, testuale: "riprovare è
// sempre permesso") — a differenza di `BusyButton`, pensato apposta per
// impedire il doppio invio su un'azione di massa. Qui è l'opposto:
// un pulsante bloccato durante un ritentativo che si impalla
// trasformerebbe l'errore in un vicolo cieco, la cosa che questo pattern
// esiste per evitare. Per questo è un `<button>` semplice, non `BusyButton`.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { canRetry, type ErrorNature } from '@/errors/classify'

const props = defineProps<{
  nature: ErrorNature
  /** Riga tecnica monospaziata facoltativa, per chi amministra il
   * server (§68.3) — tipicamente `ApiProblem.detail`. */
  technicalDetail?: string
}>()

const emit = defineEmits<{ retry: [] }>()

const { t } = useI18n()
const showRetry = computed(() => canRetry(props.nature))
</script>

<template>
  <div
    role="alert"
    class="flex flex-col items-center justify-center gap-1 px-5 py-16 text-center"
  >
    <svg
      class="mb-2.5 h-[34px] w-[34px] text-danger opacity-90"
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
    <p class="text-[14px] font-bold">
      {{ t(`errors.${nature}.title`) }}
    </p>
    <p class="mb-1.5 max-w-[380px] text-[12.5px] leading-[1.55] text-content-muted">
      {{ t(`errors.${nature}.reassurance`) }}
    </p>
    <div
      v-if="showRetry"
      class="mt-2.5 flex gap-2"
    >
      <button
        type="button"
        class="flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-sm"
        @click="emit('retry')"
      >
        <svg
          class="h-3.5 w-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path d="M3 12a9 9 0 0115.5-6.2L21 8" />
          <path d="M21 3v5h-5" />
          <path d="M21 12a9 9 0 01-15.5 6.2L3 16" />
          <path d="M3 21v-5h5" />
        </svg>
        {{ t('ui.toast.retry') }}
      </button>
    </div>
    <p
      v-if="technicalDetail"
      class="mt-2 font-mono text-[11.5px] text-content-muted opacity-80"
    >
      {{ technicalDetail }}
    </p>
  </div>
</template>
