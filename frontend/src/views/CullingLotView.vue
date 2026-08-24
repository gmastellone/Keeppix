<script setup lang="ts">
// Fase 11 Task 17 (3/N) — documento funzionale §15 "Culling — lotto
// aperto". Nucleo reale: filtri, palco, filmino, Scelta/Scarta con
// spostamento fisico vero, valutazione, tastiera con la guardia sui campi
// di testo (Ruling del piano). **Deliberatamente fuori da questa
// sotto-unità** (Task 17 4/N): selezione multipla (shift+click/
// shift+freccia, barra di selezione), "Rinomina lotto…", selettore rapido
// di lotto (§16) — nessuno di questi è finto qui, semplicemente non
// ancora costruito.
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { previewSrc as mediaPreviewSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'
import AssetViewer from '@/components/AssetViewer.vue'
import Filmstrip from '@/components/Filmstrip.vue'
import RatingStars from '@/components/RatingStars.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import { activeCullingLotName } from '@/nav/routeTitles'
import { useCullingLotStore, type CullingLotFilter } from '@/stores/cullingLot'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const store = useCullingLotStore()
const toast = useToastStore()

const FILTERS: CullingLotFilter[] = ['all', 'todo', 'taken', 'skipped']

const lotId = computed(() => route.params.lotId as string)
const lotNameFromQuery = computed(() => (typeof route.query.name === 'string' ? route.query.name : ''))

onMounted(() => {
  void store.open(lotId.value, lotNameFromQuery.value)
})
watch(lotId, (id) => {
  void store.open(id, lotNameFromQuery.value)
})
watch(() => store.lotName, (name) => {
  activeCullingLotName.value = name || null
}, { immediate: true })
onUnmounted(() => {
  activeCullingLotName.value = null
})

function backToLots() {
  void router.push('/culling')
}

function previewSrc(asset: TimelineAsset): string | undefined {
  return asset.content_hash ? mediaPreviewSrc(asset.content_hash) : undefined
}

function stateLabel(asset: { cullState: string }): string {
  return t(`culling.state.${asset.cullState}`)
}

async function rate(n: number) {
  const asset = store.currentAsset
  if (!asset) return
  await store.setRating(asset.id, n)
}

const emptySkippedOpen = ref(false)

async function confirmEmptySkipped() {
  const { succeeded, failed } = await store.emptySkippedFolder()
  if (failed > 0) {
    toast.showError(t('culling.emptySkipped.partialError', { n: failed }))
  }
  if (succeeded > 0) {
    toast.show(t('culling.emptySkipped.done'))
  }
}

// §15.6 Ambiguità del prototipo, corretta dal Ruling del piano: le
// scorciatoie non si attivano scrivendo in un campo di testo o con un
// dialog aperto.
function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable
}

function onKeydown(event: KeyboardEvent) {
  if (isTypingTarget(event.target) || emptySkippedOpen.value || viewingId.value) return
  const asset = store.currentAsset
  switch (event.key) {
    case 'ArrowLeft':
      store.goTo(-1)
      break
    case 'ArrowRight':
      store.goTo(1)
      break
    case 'p':
    case 'P':
      if (asset) void store.decide(asset.id, 'taken')
      break
    case 'x':
    case 'X':
    case 'Delete':
      if (asset) void store.decide(asset.id, 'skipped')
      break
    case '1':
    case '2':
    case '3':
    case '4':
    case '5':
      void rate(Number(event.key))
      break
    default:
      return
  }
  event.preventDefault()
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))

// §15.2/§15.3.11: il pulsante info apre il lightbox sulla foto corrente,
// con la coda di culling corrente (filtro compreso) come vicinato.
const viewingId = ref<string | null>(null)
const orderedAssets = computed(() => store.order.map((id) => store.assets.find((a) => a.id === id)).filter((a): a is NonNullable<typeof a> => !!a))
const viewingAsset = computed(() => (viewingId.value ? store.assets.find((a) => a.id === viewingId.value) : undefined))

watch(viewingId, (id) => {
  if (id) void store.ensureFlagsLoaded(id)
})

function openInfo() {
  const asset = store.currentAsset
  if (!asset) return
  viewingId.value = asset.id
}

function onViewerStep(asset: TimelineAsset) {
  viewingId.value = asset.id
  store.goToId(asset.id)
}

async function onToggleFavorite() {
  const id = viewingId.value
  if (!id) return
  await store.toggleFavorite(id)
}
</script>

<template>
  <div class="no-pad flex h-full flex-col">
    <div
      v-if="store.loadError"
      class="p-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </div>

    <template v-else>
      <div class="flex flex-col gap-2 border-b border-border px-4 pt-3.5 pb-2">
        <div class="flex flex-wrap items-center gap-3.5">
          <button
            type="button"
            class="flex items-center gap-1 text-sm text-content-muted hover:text-content"
            @click="backToLots"
          >
            ‹ {{ t('culling.backToLots') }}
          </button>
          <span class="rounded-lg bg-[var(--color-border)]/30 px-2.5 py-1 text-[13px]">
            {{ store.lotName || lotNameFromQuery }}
          </span>
          <div class="flex items-center gap-3 text-[13px]">
            <span>✓ <strong>{{ store.counts.taken }}</strong> {{ t('culling.counters.taken') }}</span>
            <span>✕ <strong>{{ store.counts.skipped }}</strong> {{ t('culling.counters.skipped') }}</span>
            <span>○ <strong>{{ store.counts.pending }}</strong> {{ t('culling.counters.pending') }}</span>
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-1.5">
          <button
            v-for="f in FILTERS"
            :key="f"
            type="button"
            class="rounded-full px-3 py-1 text-[12.5px]"
            :class="store.filter === f ? 'bg-[color-mix(in_srgb,var(--color-accent)_16%,transparent)] font-semibold text-accent' : 'bg-[var(--color-border)]/30 text-content-muted'"
            @click="store.setFilter(f)"
          >
            {{ t(`culling.filters.${f}`) }}
          </button>
          <button
            v-if="store.filter === 'skipped' && store.counts.skipped > 0"
            type="button"
            class="ml-auto rounded-full border border-danger px-3 py-1 text-[12.5px] text-danger hover:bg-danger/10"
            @click="emptySkippedOpen = true"
          >
            {{ t('culling.emptySkipped.button', { n: store.counts.skipped }) }}
          </button>
        </div>
      </div>

      <div
        v-if="!store.loading && store.order.length === 0"
        class="flex flex-1 flex-col items-center justify-center gap-1 text-center"
      >
        <p class="font-semibold">
          {{ t('culling.emptyFilter.title') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('culling.emptyFilter.subtitle') }}
        </p>
      </div>

      <template v-else-if="store.currentAsset">
        <div class="relative flex flex-1 items-center justify-center gap-4 p-4">
          <button
            type="button"
            class="flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-full border border-border"
            :style="{ opacity: store.position === 0 ? 0.35 : 1, pointerEvents: store.position === 0 ? 'none' : 'auto' }"
            :aria-label="t('culling.prev')"
            @click="store.goTo(-1)"
          >
            ‹
          </button>

          <div class="relative max-h-[340px] max-w-[520px] overflow-hidden rounded-[10px]">
            <img
              :src="previewSrc(store.currentAsset)"
              :alt="store.currentAsset.filename"
              class="max-h-[340px] max-w-[520px] object-contain"
            >
            <span
              v-if="store.currentAsset.raw_kind === 'raw' || store.currentAsset.raw_kind === 'raw+jpeg'"
              class="absolute top-2 left-2 rounded bg-black/60 px-1.5 py-0.5 text-[10px] font-semibold text-white"
            >{{ store.currentAsset.raw_kind === 'raw+jpeg' ? 'RAW+JPEG' : 'RAW' }}</span>
            <button
              type="button"
              class="absolute top-2 right-2 flex h-7 w-7 items-center justify-center rounded-full bg-black/45 text-white transition-colors hover:bg-black/65"
              :aria-label="t('culling.infoButton')"
              @click="openInfo"
            >
              i
            </button>
            <span class="absolute right-2 bottom-2 rounded bg-black/35 px-1.5 py-0.5 text-[10.5px] text-white/85">
              {{ t('culling.keyhint') }}
            </span>
          </div>

          <button
            type="button"
            class="flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-full border border-border"
            :style="{ opacity: store.position === store.order.length - 1 ? 0.35 : 1, pointerEvents: store.position === store.order.length - 1 ? 'none' : 'auto' }"
            :aria-label="t('culling.next')"
            @click="store.goTo(1)"
          >
            ›
          </button>
        </div>

        <Filmstrip
          :assets="orderedAssets"
          :current-id="store.currentAsset.id"
          @select="store.goToId"
        />

        <div class="flex items-center justify-between gap-3 px-6 py-3.5">
          <div>
            <p class="font-semibold">
              {{ store.currentAsset.filename }}
            </p>
            <p class="text-[12px] text-content-muted">
              <button
                type="button"
                class="underline hover:text-content"
                @click="backToLots"
              >{{ store.lotName || lotNameFromQuery }}</button>
              › {{ stateLabel(store.currentAsset) }}
            </p>
            <RatingStars
              class="mt-1"
              :rating="store.flagsFor(store.currentAsset.id).rating"
              @rate="rate"
            />
          </div>
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="flex items-center gap-1.5 rounded-lg border border-[#2E9E5B] px-3.5 py-2 text-[13px] font-semibold transition-colors"
              :class="store.currentAsset.cullState === 'taken' ? 'bg-[#2E9E5B] text-white' : 'text-[#2E9E5B] hover:bg-[#2E9E5B]/10'"
              @click="store.decide(store.currentAsset.id, 'taken')"
            >
              {{ t('culling.pick') }}
            </button>
            <button
              type="button"
              class="rounded-lg border border-danger px-3.5 py-2 text-[13px] font-semibold text-danger hover:bg-danger/10"
              @click="store.decide(store.currentAsset.id, 'skipped')"
            >
              {{ t('culling.reject') }}
            </button>
          </div>
        </div>
      </template>
    </template>

    <ConfirmDialog
      v-model:open="emptySkippedOpen"
      :title="t('culling.emptySkipped.title', { name: store.lotName || lotNameFromQuery })"
      :description="t('culling.emptySkipped.description', { n: store.counts.skipped })"
      :confirm-label="t('culling.emptySkipped.button', { n: store.counts.skipped })"
      @confirm="confirmEmptySkipped"
    />

    <AssetViewer
      v-if="viewingAsset"
      :asset="viewingAsset"
      :neighbors="orderedAssets"
      :is-favorite="store.flagsFor(viewingAsset.id).favorite"
      is-culling
      @close="viewingId = null"
      @step="onViewerStep"
      @open-asset="viewingId = $event"
      @toggle-favorite="onToggleFavorite"
    />
  </div>
</template>
