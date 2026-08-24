<script setup lang="ts">
// Fase 11 Task 13 (3/N) — documento funzionale §48 'Dialog "file con
// problemi"', verificato riga per riga (righe 7315-7386). Aperto
// dall'azione `view-files` di un problema in `ProblemsView.vue` (§47).
//
// **Fedele alla franchezza del commento del mockup**: "elenco reale dei
// file coinvolti (prime N foto della cartella)... in pratica i file
// elencati sono le prime tre foto della cartella coinvolta, non i tre
// file che hanno davvero il problema" — qui `folderId` viene da
// `ProblemView.folder_id` (reale, non sempre presente: solo quando il
// problema riguarda esattamente una cartella, vedi il commento su
// `sidecar_permission_problem_view` nel backend) e le foto da
// `fetchChildren(folderId).assets`, fino a 3 — stesso comportamento
// "primi N, non i file davvero coinvolti" del documento, non un
// miglioramento silenzioso.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { fetchChildren } from '@/api/folders'
import { thumbSrc as mediaThumbSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

import Dialog from './ui/Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{
  title: string
  description: string
  folderId?: string
}>()

const { t, locale } = useI18n()
const router = useRouter()

const files = ref<TimelineAsset[]>([])
const firstRowEl = ref<HTMLButtonElement | null>(null)
const closeEl = ref<HTMLButtonElement | null>(null)

watch(
  open,
  async (isOpen) => {
    if (!isOpen) return
    files.value = []
    if (!props.folderId) return
    const children = await fetchChildren(props.folderId).catch(() => ({ folders: [], assets: [] }))
    files.value = children.assets.slice(0, 3)
  },
  { immediate: true }
)

const initialFocus = computed(() => firstRowEl.value ?? closeEl.value ?? null)

function thumbnailUrl(asset: TimelineAsset): string | undefined {
  return asset.content_hash ? mediaThumbSrc(asset.content_hash) : undefined
}

function placeholderUrl(asset: TimelineAsset): string | undefined {
  return asset.thumbhash ? (thumbhashToDataURL(asset.thumbhash) ?? undefined) : undefined
}

function dateLabel(asset: TimelineAsset): string {
  if (!asset.taken_at_utc) return ''
  return new Intl.DateTimeFormat(locale.value, { day: 'numeric', month: 'long', year: 'numeric' }).format(
    new Date(asset.taken_at_utc)
  )
}

function openFile(asset: TimelineAsset) {
  open.value = false
  void router.push({ path: '/', query: { photo: asset.id } })
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="title"
    :description="description"
    :initial-focus="initialFocus"
  >
    <div class="max-h-[260px] space-y-1 overflow-y-auto">
      <button
        v-for="(asset, index) in files"
        :key="asset.id"
        :ref="(el) => { if (index === 0) firstRowEl = el as HTMLButtonElement }"
        type="button"
        :aria-label="t('problemFiles.open', { name: asset.filename })"
        class="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-border/20
               focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        @click="openFile(asset)"
      >
        <span class="h-9 w-9 shrink-0 overflow-hidden rounded-md bg-border/30">
          <img
            v-if="thumbnailUrl(asset)"
            :src="thumbnailUrl(asset)"
            alt=""
            class="h-full w-full object-cover"
          >
          <img
            v-else-if="placeholderUrl(asset)"
            :src="placeholderUrl(asset)"
            alt=""
            class="h-full w-full object-cover"
          >
        </span>
        <span class="min-w-0 flex-1">
          <span class="block truncate text-[13px] font-semibold text-content">{{ asset.filename }}</span>
          <span class="block text-[11.5px] text-content-muted">{{ dateLabel(asset) }}</span>
        </span>
        <span class="shrink-0 text-[12.5px] font-semibold text-accent">{{ t('problemFiles.openLabel') }}</span>
      </button>
    </div>
    <div class="mt-4 flex justify-end">
      <button
        ref="closeEl"
        type="button"
        class="rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold"
        @click="open = false"
      >
        {{ t('problemFiles.close') }}
      </button>
    </div>
  </Dialog>
</template>
