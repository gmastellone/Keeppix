<script setup lang="ts">
// "Edit position…" / "Set position…" dialog: reachable from the lightbox
// (one photo) and from bulk selection actions / a folder's own location
// action (many photos at once — `LibrarySelectionActions.vue`,
// `FoldersView.vue`). The mockup describes a static list of "places known
// to the library" with an explicit caveat ("no real map in this mockup")
// — not reproduced here: the real backend has a real GeoNames search
// (`GET /places/suggest`), already behind `PlacePicker.vue`, which is
// strictly better than the prototype's fake list and is itself already
// batch-capable (`assetIds: string[]`). This component is just the
// `Dialog.vue` wrapper around `PlacePicker` plus the one thing it's
// missing to cover the spec to the letter: "No location", to explicitly
// clear `location`/`place_id` for every selected asset — an action
// `PlacePicker.apply()` cannot express (it always requires a chosen
// place).
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { applyMetadataBatch } from '@/api/metadata'
import type { TimelineAsset } from '@/api/timeline'
import { useToastStore } from '@/stores/toast'

import PlacePicker from './PlacePicker.vue'
import Dialog from './ui/Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ assets: TimelineAsset[] }>()
const emit = defineEmits<{ applied: [] }>()

const { t } = useI18n()
const toast = useToastStore()
const clearing = ref(false)
const assetIds = computed(() => props.assets.map((asset) => asset.id))

function onApplied() {
  open.value = false
  emit('applied')
}

async function clearPosition() {
  clearing.value = true
  try {
    await applyMetadataBatch(assetIds.value, { location: null, place_id: null })
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
      :asset-ids="assetIds"
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
