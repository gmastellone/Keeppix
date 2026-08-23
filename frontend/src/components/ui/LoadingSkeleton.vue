<script setup lang="ts">
// SP-27: lo scheletro di caricamento (documento funzionale, principio 1 —
// "il caricamento non è mai uno spinner al centro del vuoto: è uno
// scheletro che ha già la FORMA del contenuto che sta arrivando"). Due usi
// reali del prototipo (keeppix-mockup.html righe 3180-3207), non un
// rettangolo grigio generico:
// - `grid`: una griglia fotografica giustificata (`skelGridHTML`) — stessa
//   impaginazione delle tessere vere (larghezza dal rapporto d'aspetto,
//   altezza di riga comune), così quando le foto arrivano prendono il
//   posto dello scheletro senza che nulla si sposti.
// - `stream`: la timeline in caricamento (`streamSkeletonPlaceholderHTML`)
//   — due mesi scheletro, non uno: il ritmo "titolo, griglia, titolo,
//   griglia" è parte di ciò che si sta annunciando.
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

withDefaults(
  defineProps<{
    variant?: 'grid' | 'stream'
    count?: number
    rowHeight?: number
  }>(),
  { variant: 'grid', count: 24, rowHeight: 150 }
)

// Rapporti d'aspetto misurati sul prototipo (riga 3184), non inventati:
// ciclici, non un solo valore ripetuto, perché una griglia scheletro fatta
// di quadrati identici non assomiglia a una griglia fotografica vera.
const SKEL_ASPECTS = [
  1.5, 0.67, 1.5, 1.33, 1.5, 0.75, 1.78, 1.5, 0.67, 1.5, 1.33, 1.5,
  1.5, 0.75, 1.5, 1.78, 1.33, 0.67, 1.5, 1.5, 1.33, 1.5, 0.75, 1.5
]

function aspectFor(index: number): number {
  return SKEL_ASPECTS[index % SKEL_ASPECTS.length]
}

function tileStyle(index: number, rowHeight: number) {
  const ar = aspectFor(index)
  return { height: `${rowHeight}px`, flex: `${ar} 1 ${Math.round(ar * rowHeight)}px` }
}

// Stessa formula del prototipo (riga 3198): fra 8 e 16 tessere per mese,
// proporzionata al conteggio approssimativo passato dal chiamante.
function perMonthCount(approxCount: number): number {
  return Math.max(8, Math.min(16, Math.round(approxCount / 2) || 12))
}
</script>

<template>
  <div
    v-if="variant === 'grid'"
    aria-hidden="true"
    class="flex flex-wrap gap-1.5"
  >
    <div
      v-for="i in count"
      :key="i"
      class="skel"
      :style="tileStyle(i - 1, rowHeight)"
    />
  </div>
  <div
    v-else
    role="status"
    :aria-label="t('ui.loadingSkeleton.streamLabel')"
    class="flex flex-col gap-6"
  >
    <div
      v-for="month in 2"
      :key="month"
      class="flex flex-col gap-2.5"
    >
      <div class="flex items-baseline gap-2.5">
        <div
          class="skel"
          style="width: 118px; height: 15px"
        />
        <div
          class="skel"
          style="width: 56px; height: 10px"
        />
      </div>
      <div
        aria-hidden="true"
        class="flex flex-wrap gap-1.5"
      >
        <div
          v-for="i in perMonthCount(count)"
          :key="i"
          class="skel"
          :style="tileStyle(i - 1, rowHeight)"
        />
      </div>
    </div>
  </div>
</template>
