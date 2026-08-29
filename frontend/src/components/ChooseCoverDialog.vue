<script setup lang="ts">
// "Choose cover" dialog.
//
// **One thumbnail per confirmed face, not per photo**: if a person has two
// confirmed faces in the same photo, two identical thumbnails appear —
// `fetchPersonFaceTiles` (`api/faces.ts`) builds them that way on purpose.
//
// **The thumbnail shows the whole photo, not a crop of the face** — same
// principle already followed for the People grid cover: no route crops a
// face out of its photo.
//
// **No "revert to automatic cover"** option is provided here.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchPersonFaceTiles, type PersonFaceTile } from '@/api/faces'
import { thumbSrc } from '@/api/media'
import { patchPerson, type Person } from '@/api/persons'
import type { TimelineAsset } from '@/api/timeline'
import Dialog from '@/components/ui/Dialog.vue'
import { useToastStore } from '@/stores/toast'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ person: Person; assets: TimelineAsset[] }>()
const emit = defineEmits<{ updated: [person: Person] }>()

const { t } = useI18n()
const toast = useToastStore()

const tiles = ref<PersonFaceTile[]>([])
const loading = ref(true)

watch(
  open,
  async (isOpen) => {
    if (!isOpen) return
    loading.value = true
    try {
      tiles.value = await fetchPersonFaceTiles(props.person.id, props.assets)
    } finally {
      loading.value = false
    }
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

const displayName = computed(() => props.person.name?.trim() || t('persons.unnamed'))

async function setCover(tile: PersonFaceTile) {
  try {
    const updated = await patchPerson(props.person.id, { cover_face_id: tile.face.id })
    toast.show(t('chooseCover.updatedToast'))
    open.value = false
    emit('updated', updated)
  } catch {
    toast.showError(t('chooseCover.error'))
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('chooseCover.title')"
    :description="t('chooseCover.subtitle', { name: displayName })"
  >
    <div
      v-if="!loading && tiles.length === 0"
      class="py-4 text-[12.5px] text-content-muted"
    >
      {{ t('chooseCover.empty') }}
    </div>
    <div
      v-else
      class="grid max-h-[280px] grid-cols-5 gap-1.5 overflow-y-auto"
    >
      <button
        v-for="tile in tiles"
        :key="tile.face.id"
        type="button"
        class="aspect-square rounded-md border-2 bg-cover bg-center bg-surface-elevated hover:border-border-strong"
        :class="tile.face.id === person.cover_face_id ? 'border-accent' : 'border-transparent'"
        :style="tileStyle(tile.asset)"
        :aria-label="t('chooseCover.setCover')"
        @click="setCover(tile)"
      />
    </div>
    <button
      type="button"
      class="mt-3 rounded-lg border border-transparent px-3.5 py-2 text-[13px] font-semibold hover:bg-border/30"
      @click="open = false"
    >
      {{ t('chooseCover.close') }}
    </button>
  </Dialog>
</template>
