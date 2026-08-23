<script setup lang="ts">
// SP-12: la provenienza automatica vs umana (documento funzionale,
// definizione canonica §59 — "un'etichetta proposta dal riconoscimento e
// una messa da una persona non sono mai indistinguibili nell'interfaccia,
// in nessun punto"). Il marcatore reale (`.lb-tag-chip` attenuata, righe
// 8729-8734 del documento funzionale): un piccolo "IA" (9px, peso 700,
// opacità .8) con un titolo che spiega cosa significa. Visibile per
// qualunque assegnazione ancora di origine `ai` — `confirmed`+`ai` o
// `suggested`+`ai`, il documento tratta entrambe come "non ancora una
// firma umana". L'unico stato che non lo mostra è `confirmed`+`human`:
// qui la decisione è dentro il componente, non lasciata al chiamante da
// ricordare ogni volta — è il modo in cui "mai indistinguibili, in nessun
// punto" resta vero anche quando questo badge finisce dentro componenti
// che non esistono ancora (chip dei tag, riquadro del volto, miniatura
// di Revisione).
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{ origin: 'ai' | 'human' }>()
</script>

<template>
  <span
    v-if="origin === 'ai'"
    class="text-[9px] font-bold opacity-80"
    :title="t('ui.provenanceBadge.description')"
    :aria-label="t('ui.provenanceBadge.description')"
  >
    {{ t('ui.provenanceBadge.label') }}
  </span>
</template>
