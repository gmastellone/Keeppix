<script setup lang="ts">
// Fase 11 Task 16 (4/N), §36 "Dialog 'separa persona'" — documento
// funzionale verificato riga per riga (righe 5753-5871).
//
// **Nessun suggerimento IA da preselezionare**: `subCluster` è un
// campo del mockup senza alcuna colonna corrispondente sul backend
// reale (`Face`/`FaceRow`, `crates/keeppix-domain/src/face.rs`,
// `crates/keeppix-db/src/faces.rs` — verificato in questa unità,
// nessun sotto-cluster/secondo candidato esiste). La banda di
// suggerimento (§36.2) e la preselezione automatica non sono quindi
// costruibili: si apre sempre a zero volti selezionati, come il
// documento stesso descrive per "tutte le persone tranne Chiara" (il
// solo caso demo con `subCluster`).
//
// **Una miniatura per volto confermato, non per foto** (§36.2): stesso
// `fetchPersonFaceTiles` di `ChooseCoverDialog.vue`.
//
// **Il controllo "meno di due volti" resta a monte** (§32.3 controllo
// 5, nel chiamante `PersonDetailView.vue`): questo dialog stesso non
// duplica quel toast.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchPersonFaceTiles, type PersonFaceTile } from '@/api/faces'
import { thumbSrc } from '@/api/media'
import { separatePerson, type Person } from '@/api/persons'
import type { TimelineAsset } from '@/api/timeline'
import Dialog from '@/components/ui/Dialog.vue'
import TextField from '@/components/ui/TextField.vue'
import { useToastStore } from '@/stores/toast'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ person: Person; assets: TimelineAsset[] }>()
const emit = defineEmits<{ split: [] }>()

const { t } = useI18n()
const toast = useToastStore()

const tiles = ref<PersonFaceTile[]>([])
const selected = ref<string[]>([])
const name = ref('')
const saving = ref(false)

watch(
  open,
  async (isOpen) => {
    if (!isOpen) return
    selected.value = []
    name.value = ''
    tiles.value = await fetchPersonFaceTiles(props.person.id, props.assets)
  },
  { immediate: true }
)

function tileStyle(asset: TimelineAsset) {
  if (asset.content_hash) return { backgroundImage: `url(${thumbSrc(asset.content_hash)})` }
  if (asset.thumbhash) {
    const url = thumbhashToDataURL(asset.thumbhash)
    if (url) return { backgroundImage: `url(${url})` }
  }
  return {}
}

function toggle(faceId: string) {
  const i = selected.value.indexOf(faceId)
  if (i === -1) selected.value = [...selected.value, faceId]
  else selected.value = selected.value.filter((id) => id !== faceId)
}

const remaining = computed(() => tiles.value.length - selected.value.length)
const allSelected = computed(() => tiles.value.length > 0 && selected.value.length === tiles.value.length)
const canConfirm = computed(() => selected.value.length > 0 && !allSelected.value)

const displayName = computed(() => props.person.name?.trim() || t('persons.unnamed'))

async function confirmSplit() {
  if (!canConfirm.value || saving.value) return
  saving.value = true
  try {
    await separatePerson(props.person.id, selected.value, name.value.trim())
    toast.show(t('splitPerson.splitToast', { n: selected.value.length }, { plural: selected.value.length }))
    open.value = false
    emit('split')
  } catch {
    toast.showError(t('splitPerson.error'))
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('splitPerson.title', { name: displayName })"
    :description="t('splitPerson.subtitle')"
  >
    <div
      class="grid max-h-[280px] grid-cols-8 gap-1.5 overflow-y-auto"
    >
      <button
        v-for="tile in tiles"
        :key="tile.face.id"
        type="button"
        role="checkbox"
        :aria-checked="selected.includes(tile.face.id)"
        class="relative aspect-square rounded-md border-2 bg-cover bg-center bg-surface-elevated"
        :class="selected.includes(tile.face.id) ? 'border-accent' : 'border-transparent'"
        :style="tileStyle(tile.asset)"
        @click="toggle(tile.face.id)"
      >
        <span
          class="absolute right-1 top-1 flex h-[17px] w-[17px] items-center justify-center rounded-sm border border-white
                 bg-black/40 text-[10px] font-bold text-white"
          :class="selected.includes(tile.face.id) && 'bg-accent'"
        >
          <template v-if="selected.includes(tile.face.id)">✓</template>
        </span>
      </button>
    </div>

    <p class="mt-3 text-[12.5px]">
      {{ t('splitPerson.countLine', { n: selected.length, name: displayName, remaining }, { plural: selected.length }) }}
    </p>
    <p
      v-if="allSelected"
      class="mt-1.5 rounded-md border border-danger/30 bg-danger/10 px-2.5 py-2 text-[12.5px] text-danger"
    >
      {{ t('splitPerson.allSelectedWarning') }}
    </p>

    <div class="mt-3">
      <TextField
        v-model="name"
        :label="`${t('splitPerson.name')} (${t('splitPerson.optional')})`"
      />
    </div>

    <div class="mt-3 flex items-center gap-2">
      <button
        type="button"
        class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-text disabled:opacity-40"
        :disabled="!canConfirm || saving"
        @click="confirmSplit"
      >
        {{ t('splitPerson.confirmButton') }}
      </button>
      <button
        type="button"
        class="rounded-lg border border-transparent px-3.5 py-2 text-[13px] font-semibold hover:bg-border/30"
        @click="open = false"
      >
        {{ t('ui.dialog.cancel') }}
      </button>
    </div>
  </Dialog>
</template>
