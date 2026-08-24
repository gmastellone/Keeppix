<script setup lang="ts">
// Fase 11 Task 8 (4/N) — documento funzionale §19.3, "Modifica posizione…"
// / "Imposta posizione…" (`#lbEditPlaceBtn`): unico ingresso dal lightbox
// alla sezione POSIZIONE del pannello informazioni. Il mockup descrive un
// elenco statico di "luoghi noti alla libreria" con un caveat esplicito
// ("Nessuna mappa reale in questo mockup") — non riprodotto: il backend
// reale ha una ricerca GeoNames vera (`GET /places/suggest`), già dietro
// `PlacePicker.vue` (fin qui mai collegato a nessuna vista — orfano,
// confermato via grep prima di scrivere questo file), che è strettamente
// migliore del finto elenco del prototipo. Questo componente è solo
// l'involucro SP-5 (`Dialog.vue`) attorno a `PlacePicker` più l'unica cosa
// che gli manca per coprire §19.3 alla lettera: "Nessuna posizione", per
// azzerare esplicitamente `location`/`place_id` — un'azione che
// `PlacePicker.apply()` non può esprimere (richiede sempre un luogo
// scelto).
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { patchMetadata } from '@/api/metadata'
import type { TimelineAsset } from '@/api/timeline'
import { useMapsStore } from '@/stores/maps'
import { useToastStore } from '@/stores/toast'

import PlacePicker from './PlacePicker.vue'
import Dialog from './ui/Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ asset: TimelineAsset }>()
const emit = defineEmits<{ applied: [] }>()

const { t } = useI18n()
const maps = useMapsStore()
const toast = useToastStore()
const clearing = ref(false)

function onApplied() {
  open.value = false
  emit('applied')
}

async function clearPosition() {
  clearing.value = true
  try {
    await patchMetadata(props.asset.id, { location: null, place_id: null })
    open.value = false
    emit('applied')
  } catch {
    toast.showError(t('maps.places.clearError'))
  } finally {
    clearing.value = false
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('maps.places.dialogTitle')"
    :description="t('maps.places.dialogSubtitle')"
  >
    <PlacePicker
      :asset-ids="[asset.id]"
      :available-region-ids="maps.availableRegionIds"
      :all-regions="maps.regions"
      @applied="onApplied"
    />
    <button
      type="button"
      class="mt-4 text-sm text-content-muted underline underline-offset-2 hover:text-content"
      :disabled="clearing"
      @click="clearPosition"
    >
      {{ t('maps.places.clear') }}
    </button>
  </Dialog>
</template>
