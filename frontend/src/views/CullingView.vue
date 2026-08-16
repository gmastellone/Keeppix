<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import type { DiskAction } from '@/api/culling'
import type { TimelineAsset } from '@/api/timeline'
import Filmstrip from '@/components/Filmstrip.vue'
import RatingStars from '@/components/RatingStars.vue'
import { useCullingStore } from '@/stores/culling'

const { t } = useI18n()
const router = useRouter()
const store = useCullingStore()

const FILTERS = ['all', 'pending', 'picks', 'rejects'] as const
const DELETE_OPTIONS = ['kept', 'moved_to_trash', 'purged'] as const

const deleteDialog = ref<{ assetIds: string[] } | null>(null)
const selectedAction = ref<DiskAction>('moved_to_trash')

const assetsById = computed(() => new Map(store.assets.map((a) => [a.id, a])))

const orderedAssets = computed<TimelineAsset[]>(() =>
  store.order.flatMap((id) => {
    const asset = assetsById.value.get(id)
    return asset ? [asset] : []
  })
)

/** Confronto affiancato (`c`): la foto corrente più le successive nell'ordine filtrato, fino a 4. */
const compareAssets = computed<TimelineAsset[]>(() => orderedAssets.value.slice(store.position, store.position + 4))

function previewSrc(id: string): string {
  const asset = assetsById.value.get(id)
  return asset?.content_hash ? `/media/preview/${asset.content_hash}` : originalSrc(id)
}

function originalSrc(id: string): string {
  return `/media/original/${id}`
}

/**
 * L'unico posto in cui il culling legge l'originale in modo aggressivo (spec
 * §4.2): non esiste un endpoint di ritaglio lato server in questa fase, quindi
 * "il ritaglio centrale a piena risoluzione" è una tecnica di sola
 * presentazione — si precarica l'intero originale nella cache del browser e
 * lo si mostra ingrandito e centrato dentro un contenitore con
 * `overflow: hidden` (vedi il template, ramo `store.zoomed`). Il precaricamento
 * è ciò che rende lo zoom istantaneo: il file è già in cache quando l'utente
 * preme `z`.
 */
const preloaded = new Set<string>()
function preloadOriginal(id: string) {
  if (preloaded.has(id)) return
  preloaded.add(id)
  const img = new Image()
  img.src = originalSrc(id)
}

// `radio`/`checkbox` & co. non sono "scrivere testo": il dialogo di
// cancellazione ha un gruppo di radio, e bloccare anche `Escape` lì dentro
// impedirebbe di chiuderlo da tastiera.
const NON_TEXT_INPUT_TYPES = new Set([
  'checkbox',
  'radio',
  'button',
  'submit',
  'reset',
  'range',
  'color',
  'file',
  'image'
])

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  // `isContentEditable` non è implementato da jsdom (resta `undefined`), quindi
  // si controlla anche l'attributo direttamente perché i test lo esercitino.
  const editableAttr = target.getAttribute('contenteditable')
  if (target.isContentEditable || editableAttr === '' || editableAttr === 'true') return true
  if (target.tagName === 'TEXTAREA' || target.tagName === 'SELECT') return true
  if (target.tagName === 'INPUT') {
    const type = (target as HTMLInputElement).type.toLowerCase()
    return !NON_TEXT_INPUT_TYPES.has(type)
  }
  return false
}

function close() {
  void router.push('/')
}

function selectAsset(id: string) {
  store.goToId(id)
}

function openDeleteDialog(assetIds: string[]) {
  if (assetIds.length === 0) return
  selectedAction.value = 'moved_to_trash'
  deleteDialog.value = { assetIds }
}

function closeDeleteDialog() {
  deleteDialog.value = null
}

async function confirmDelete() {
  const pending = deleteDialog.value
  if (!pending) return
  deleteDialog.value = null
  await store.removeMany(pending.assetIds, selectedAction.value)
}

function deleteCurrent() {
  const id = store.currentAsset?.id
  if (id) openDeleteDialog([id])
}

function deleteRejects() {
  const ids = store.assets.filter((a) => store.flagsFor(a.id).pick === 'reject').map((a) => a.id)
  openDeleteDialog(ids)
}

/**
 * Il visualizzatore normale (`AssetViewer.vue`) non diventa una modalità
 * (spec §10.1): qui, e solo qui, vivono `p`/`x`/`z`/`c`/`Canc`. `1-5` e
 * `f`/preferito restano gli unici due che il visualizzatore normale conosce
 * — non duplicati qui perché non esiste un secondo posto dove intercettarli.
 */
function onKey(e: KeyboardEvent) {
  if (isTypingTarget(e.target)) return
  if (deleteDialog.value) {
    if (e.key === 'Escape') closeDeleteDialog()
    return
  }
  const id = store.currentAsset?.id
  if (e.key === 'Escape') {
    close()
    return
  }
  if (!id) return

  if (e.key >= '1' && e.key <= '5') {
    store.rate(id, Number(e.key))
    return
  }
  switch (e.key) {
    case 'p':
      store.pick(id)
      break
    case 'x':
      store.reject(id)
      break
    case 'ArrowLeft':
      store.goTo(-1)
      break
    case 'ArrowRight':
      store.goTo(1)
      break
    case 'z':
      store.toggleZoom()
      break
    case 'c':
      store.toggleCompare()
      break
    case 'Delete':
    case 'Backspace':
      deleteCurrent()
      break
    default:
      break
  }
}

watch(
  () => `${store.position}:${store.order.length}`,
  () => {
    const upcoming = store.order.slice(store.position + 1, store.position + 4)
    upcoming.forEach(preloadOriginal)
    const id = store.currentAsset?.id
    if (id) void store.ensureFlagsLoaded(id)
  },
  { immediate: true }
)

onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => {
  window.removeEventListener('keydown', onKey)
  store.stop()
})
</script>

<template>
  <div class="fixed inset-0 z-50 flex flex-col bg-black text-white">
    <header class="flex items-center gap-3 border-b border-white/10 px-4 py-2 text-sm">
      <h1 class="font-semibold">
        {{ t('culling.title') }}
      </h1>
      <span v-if="store.total > 0">
        {{ store.position + 1 }}/{{ store.order.length }}
        <span
          v-if="store.order.length !== store.total"
          class="text-white/60"
        >({{ t('culling.ofTotal', { total: store.total }) }})</span>
      </span>
      <button
        type="button"
        class="ml-auto rounded bg-white/10 px-3 py-1 hover:bg-white/20"
        @click="close"
      >
        {{ t('culling.close') }}
      </button>
    </header>

    <p
      v-if="store.total === 0"
      class="m-auto text-white/70"
    >
      {{ t('culling.empty') }}
    </p>

    <template v-else>
      <div class="relative min-h-0 flex-1 overflow-hidden bg-black">
        <div
          v-if="store.compareMode"
          class="flex h-full w-full gap-1"
        >
          <img
            v-for="asset in compareAssets"
            :key="asset.id"
            :src="previewSrc(asset.id)"
            :alt="asset.filename"
            class="h-full min-w-0 flex-1 object-contain"
          >
        </div>
        <div
          v-else-if="store.zoomed && store.currentAsset"
          class="relative h-full w-full overflow-hidden"
        >
          <img
            :key="`zoom-${store.currentAsset.id}`"
            :src="originalSrc(store.currentAsset.id)"
            :alt="store.currentAsset.filename"
            class="absolute left-1/2 top-1/2 max-w-none -translate-x-1/2 -translate-y-1/2"
          >
        </div>
        <img
          v-else-if="store.currentAsset"
          :key="store.currentAsset.id"
          :src="previewSrc(store.currentAsset.id)"
          :alt="store.currentAsset.filename"
          class="m-auto h-full max-h-full w-full max-w-full object-contain"
        >
      </div>

      <div class="flex flex-wrap items-center gap-3 border-t border-white/10 bg-black/60 px-4 py-2 text-sm">
        <RatingStars
          :rating="store.currentAsset ? store.flagsFor(store.currentAsset.id).rating : null"
          @rate="(n) => store.currentAsset && store.rate(store.currentAsset.id, n)"
        />
        <span>{{ t('culling.stats.picks') }}: {{ store.pickCount }}</span>
        <span>{{ t('culling.stats.rejects') }}: {{ store.rejectCount }}</span>
        <span>{{ t('culling.stats.pending') }}: {{ store.pendingCount }}</span>
        <div
          class="ml-auto flex gap-1"
          role="group"
          :aria-label="t('culling.filters.label')"
        >
          <button
            v-for="f in FILTERS"
            :key="f"
            type="button"
            class="rounded px-2 py-1"
            :class="store.filter === f ? 'bg-accent' : 'bg-white/10'"
            @click="store.setFilter(f)"
          >
            {{ t(`culling.filters.${f}`) }}
          </button>
        </div>
        <button
          v-if="store.rejectCount > 0"
          type="button"
          class="rounded bg-danger px-3 py-1"
          @click="deleteRejects"
        >
          {{ t('culling.deleteRejects', { count: store.rejectCount }) }}
        </button>
      </div>

      <Filmstrip
        :assets="orderedAssets"
        :current-id="store.currentAsset?.id"
        :flags-for="store.flagsFor"
        @select="selectAsset"
      />
    </template>

    <div
      v-if="deleteDialog"
      class="fixed inset-0 z-[60] flex items-center justify-center bg-black/70"
      role="dialog"
      :aria-label="t('culling.deleteDialog.title', { count: deleteDialog.assetIds.length })"
    >
      <div class="w-96 max-w-[90vw] rounded-lg bg-surface p-4 text-content">
        <h2 class="text-lg font-semibold">
          {{ t('culling.deleteDialog.title', { count: deleteDialog.assetIds.length }) }}
        </h2>
        <div class="mt-3 space-y-3">
          <label
            v-for="option in DELETE_OPTIONS"
            :key="option"
            class="flex items-start gap-2"
          >
            <input
              v-model="selectedAction"
              type="radio"
              name="disk-action"
              :value="option"
              class="mt-1"
            >
            <span>
              <span class="block font-medium">{{ t(`culling.deleteDialog.options.${option}.label`) }}</span>
              <span class="block text-xs text-content-muted">{{ t(`culling.deleteDialog.options.${option}.hint`) }}</span>
            </span>
          </label>
        </div>
        <div class="mt-4 flex justify-end gap-2">
          <button
            type="button"
            class="rounded border border-border px-3 py-1.5"
            @click="closeDeleteDialog"
          >
            {{ t('culling.deleteDialog.cancel') }}
          </button>
          <button
            type="button"
            class="rounded bg-danger px-3 py-1.5 text-white"
            @click="confirmDelete"
          >
            {{ t('culling.deleteDialog.confirm') }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
