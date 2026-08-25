<script setup lang="ts">
// SP-2/§13.3 campo 5 (dialog di scelta tag) — verificato sul prototipo
// reale, `openTagPickerDialog` in `docs/ui/keeppix-mockup.html`: "attiva/
// disattiva un tag per aggiungerlo o toglierlo da tutti", stesso
// interruttore di gruppo di `AlbumPickerDialog.vue`. A differenza
// dell'album, l'appartenenza non richiede un fetch per tag: i tag
// confermati sono già dentro ogni `TimelineAsset.tags` (Task 7 4/N,
// `AssetView` esteso) — nessun endpoint di "appartenenza" da inventare.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { assignTagBatch, fetchTags, unassignTagBatch, type Tag } from '@/api/tags'
import type { TimelineAsset } from '@/api/timeline'
import { useToastStore } from '@/stores/toast'

import Dialog from './ui/Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ assets: TimelineAsset[] }>()

const { t } = useI18n()
const toast = useToastStore()

const tags = ref<Tag[]>([])
/** Sovrascritture ottimistiche per id-tag: assente = deduci da
 * `props.assets[*].tags`, presente = l'esito dell'ultimo toggle in questa
 * apertura (evita di dover ri-fetchare gli asset per riflettere un
 * cambiamento appena scritto). */
const overlay = ref<Record<string, boolean>>({})
const pending = ref<Set<string>>(new Set())

const tagOptions = computed(() => tags.value.filter((tg) => tg.kind === 'tag'))

async function load() {
  tags.value = await fetchTags().catch(() => [])
}

watch(
  open,
  (isOpen) => {
    if (isOpen) {
      overlay.value = {}
      void load()
    }
  },
  { immediate: true }
)

function isFullyIn(tagId: string): boolean {
  if (tagId in overlay.value) return overlay.value[tagId]
  if (props.assets.length === 0) return false
  return props.assets.every((asset) => asset.tags.some((tg) => tg.id === tagId))
}

async function toggle(tag: Tag) {
  if (pending.value.has(tag.id)) return
  pending.value = new Set(pending.value).add(tag.id)
  const add = !isFullyIn(tag.id)
  const ids = props.assets.map((asset) => asset.id)
  try {
    if (add) {
      await assignTagBatch(tag.id, ids)
    } else {
      await unassignTagBatch(tag.id, ids)
    }
    overlay.value = { ...overlay.value, [tag.id]: add }
  } catch {
    toast.showError(t('tagPicker.error'))
  } finally {
    const rest = new Set(pending.value)
    rest.delete(tag.id)
    pending.value = rest
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('tagPicker.title')"
    :description="t('tagPicker.subtitle', { n: assets.length }, { plural: assets.length })"
  >
    <div class="max-h-[260px] space-y-1 overflow-y-auto">
      <button
        v-for="tag in tagOptions"
        :key="tag.id"
        type="button"
        role="switch"
        :aria-checked="isFullyIn(tag.id)"
        :disabled="pending.has(tag.id)"
        class="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-border/20
               focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        @click="toggle(tag)"
      >
        <span
          class="h-[22px] w-[22px] shrink-0 rounded-[7px]"
          :style="{ background: tag.color ?? 'var(--color-border)' }"
        />
        <span class="min-w-0 flex-1 truncate text-[13px] font-medium text-content">{{ tag.name }}</span>
        <span
          class="relative h-5 w-9 shrink-0 rounded-full transition-colors"
          :style="{ transitionDuration: 'var(--duration-arrow)' }"
          :class="isFullyIn(tag.id) ? 'bg-accent' : 'bg-border'"
        >
          <span
            class="absolute top-0.5 h-4 w-4 rounded-full bg-white transition-[left]"
            :style="{ left: isFullyIn(tag.id) ? '18px' : '2px', transitionDuration: 'var(--duration-arrow)' }"
          />
        </span>
      </button>
    </div>
    <div class="mt-4 flex justify-end">
      <button
        type="button"
        class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-text"
        @click="open = false"
      >
        {{ t('tagPicker.done') }}
      </button>
    </div>
  </Dialog>
</template>
