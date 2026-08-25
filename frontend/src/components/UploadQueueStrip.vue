<script setup lang="ts">
// Fase 11, sottosistema di caricamento — documento §6.1 ("Dove vive, e
// perché lì"), verificato riga per riga **e** contro `renderUploadDock`/
// `uploadCounts` del prototipo (righe 2829-2937) per i dettagli che il
// documento non specifica (l'etichetta esatta, l'ordine di priorità,
// cosa conta come "finito").
//
// A riposo non esiste (§6.1: "a coda vuota la striscia non esiste, il
// costo in pixel è zero") — solo `sessions`, mai i rifiuti: verificato
// contro `renderUploadDock()` del prototipo (riga 2913,
// `if(!u.items.length){ host.innerHTML=''; return; }`), che non guarda
// mai `state.upload.rejected`. Un lotto di soli rifiuti non fa comparire
// la striscia — ma il blocco di rifiuto resta comunque visibile,
// perché aggiungere file apre sempre il pannello da solo
// (`stores/upload.ts::addFilesFromPicker`, `panelOpen.value = true`),
// non serve passare dalla striscia per vederlo.
//
// Un solo componente per le due ancore del documento (piede sidebar
// desktop, fascia sopra la tab bar mobile — "solo una delle due esiste
// per volta"): la differenza è dove il chiamante lo monta, non nel suo
// markup — stesso principio già applicato a `useUploadPicker.ts`.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { useUploadStore } from '@/stores/upload'

const { t } = useI18n()
const upload = useUploadStore()

const visible = computed(() => upload.sessions.length > 0)

const counts = computed(() => {
  const finished = upload.sessions.filter(
    (s) => s.status === 'done' || s.status === 'skipped' || s.status === 'error'
  ).length
  const pending = upload.sessions.filter(
    (s) => s.status === 'queued' || s.status === 'uploading' || s.status === 'paused'
  ).length
  return { total: upload.sessions.length, finished, pending }
})

/** Stessa priorità del prototipo (riga 2917-2920): manca la
 * destinazione prima di tutto, poi in pausa, poi in corso, poi finito.
 * "In pausa" qui significa "tutto ciò che è ancora in sospeso è in
 * pausa" — lo stesso effetto del `u.paused` globale del prototipo,
 * che ferma tutte le sessioni in sospeso insieme (`pauseAll`). */
const labelKey = computed(() => {
  if (upload.needsDestination) return 'upload.dock.needsDestination'
  const pendingSessions = upload.sessions.filter(
    (s) => s.status === 'queued' || s.status === 'uploading' || s.status === 'paused'
  )
  if (pendingSessions.length > 0 && pendingSessions.every((s) => s.status === 'paused')) {
    return 'upload.dock.paused'
  }
  if (counts.value.pending > 0) return 'upload.dock.uploading'
  return 'upload.dock.done'
})

const progressPercent = computed(() => {
  if (counts.value.total === 0) return 0
  return Math.round((counts.value.finished / counts.value.total) * 100)
})
</script>

<template>
  <button
    v-if="visible"
    type="button"
    class="block w-full rounded-[10px] bg-chip-bg px-3 py-2.5 text-left hover:bg-border/40
           focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
    :class="upload.needsDestination && 'shadow-[inset_0_0_0_1.5px_var(--color-accent)]'"
    :aria-expanded="upload.panelOpen"
    :aria-label="
      t('upload.dock.ariaLabel', { label: t(labelKey), finished: counts.finished, total: counts.total })
    "
    @click="upload.togglePanel()"
  >
    <div class="mb-1.5 flex items-center gap-1.5">
      <span class="min-w-0 flex-1 truncate text-[11.5px] font-semibold">{{ t(labelKey) }}</span>
      <span class="text-[11px] text-content-muted">{{ counts.finished }}/{{ counts.total }}</span>
    </div>
    <div class="h-[5px] overflow-hidden rounded-[3px] bg-border-strong">
      <div
        class="h-full rounded-[3px] bg-accent transition-[width] duration-[250ms] ease-in-out"
        :style="{ width: `${progressPercent}%` }"
      />
    </div>
  </button>
</template>
