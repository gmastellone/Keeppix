<script setup lang="ts">
// "Edit position…" / "Set position…" dialog: the only entry point from the
// lightbox to the info panel's LOCATION section. The mockup describes a
// static list of "places known to the library" with an explicit caveat
// ("no real map in this mockup") — not reproduced here: the real backend
// has a real GeoNames search (`GET /places/suggest`), already behind
// `PlacePicker.vue` (until now never wired to any view — an orphan,
// confirmed via grep before writing this file), which is strictly better
// than the prototype's fake list. This component is just the `Dialog.vue`
// wrapper around `PlacePicker` plus the one thing it's missing to cover
// the spec to the letter: "No location", to explicitly clear
// `location`/`place_id` — an action `PlacePicker.apply()` cannot express
// (it always requires a chosen place).
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
