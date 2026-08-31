<script setup lang="ts">
// Culling — open lot, and the quick lot switcher. Core: filters, stage,
// filmstrip, Pick/Reject with real physical file moves, rating, keyboard
// shortcuts guarded against text fields. Also: multi-selection
// (shift+click/shift+arrow on the filmstrip and from the keyboard, a
// selection bar with its own declared deviations), "Rename lot…"/
// "Rename…" (both extend RenameFormulaDialog with `hasSubfolders`), and
// the quick lot switcher (on a Popover with `escDismisses=false` — the
// one deviation from the standard Popover behavior among this
// component's consumers).
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { fetchCullingLots, type CullingLot } from '@/api/culling'
import { previewSrc as mediaPreviewSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'
import AssetViewer from '@/components/AssetViewer.vue'
import Filmstrip from '@/components/Filmstrip.vue'
import RatingStars from '@/components/RatingStars.vue'
import RenameFormulaDialog from '@/components/RenameFormulaDialog.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import Popover from '@/components/ui/Popover.vue'
import SelectionBar from '@/components/ui/SelectionBar.vue'
import Tooltip from '@/components/ui/Tooltip.vue'
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
const libraryId = computed(() => (typeof route.query.library === 'string' ? route.query.library : null))

// Quick lot switcher: the list is lazily loaded when the panel opens,
// not every time a lot opens (id, name, and pending count per lot are
// only needed while the panel is open).
const switcherOpen = ref(false)
const switcherLots = ref<CullingLot[]>([])

watch(switcherOpen, (isOpen) => {
  if (isOpen) void loadSwitcherLots()
})

async function loadSwitcherLots() {
  if (!libraryId.value) return
  switcherLots.value = await fetchCullingLots(libraryId.value).catch(() => [])
}

function switchToLot(lot: CullingLot) {
  switcherOpen.value = false
  store.clearSelection()
  void router.push({ path: `/culling/${lot.folder_id}`, query: { name: lot.name, library: libraryId.value ?? undefined } })
}

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

// Keyboard shortcuts don't fire while typing in a text field or with a
// dialog open.
function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable
}

function onKeydown(event: KeyboardEvent) {
  if (isTypingTarget(event.target) || emptySkippedOpen.value || viewingId.value) return
  const asset = store.currentAsset
  switch (event.key) {
    case 'ArrowLeft':
      if (event.shiftKey) {
        store.selectRangeByArrow(-1)
      } else {
        // A plain arrow key clears an in-progress multi-selection — the
        // only quick keyboard exit (no Esc here).
        if (store.selectedCount > 0) store.clearSelection()
        store.goTo(-1)
      }
      break
    case 'ArrowRight':
      if (event.shiftKey) {
        store.selectRangeByArrow(1)
      } else {
        if (store.selectedCount > 0) store.clearSelection()
        store.goTo(1)
      }
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

// The info button opens the lightbox on the current photo, with the
// current culling queue (filter included) as its neighbors.
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

const selectedAssets = computed(() =>
  orderedAssets.value.filter((a) => store.selectedIds.has(a.id))
)

/** Selection bar, bulk "Pick"/"Reject": shows a toast and clears the
 * selection, partial success handled the same way as "Empty skipped". */
async function decideSelection(target: 'taken' | 'skipped') {
  const ids = Array.from(store.selectedIds)
  const { succeeded, failed } = await store.decideMany(ids, target)
  store.clearSelection()
  if (failed > 0) {
    toast.showPartial(succeeded, failed)
  } else if (succeeded > 0) {
    toast.show(t(target === 'taken' ? 'culling.bulk.taken' : 'culling.bulk.skipped', { n: succeeded }, { plural: succeeded }))
  }
}

// "Rename lot…" and the selection bar's "Rename…" share the same dialog:
// only the scope it opens with changes — no duplicated logic.
const renameOpen = ref(false)
const renameScope = ref<'lot' | 'selection'>('lot')
const pendingLotAssets = computed(() => store.assets.filter((a) => a.cullState === 'pending'))

function openRenameLot() {
  renameScope.value = 'lot'
  renameOpen.value = true
}

function openRenameSelection() {
  renameScope.value = 'selection'
  renameOpen.value = true
}

const renameAssets = computed(() => (renameScope.value === 'lot' ? store.assets : selectedAssets.value))
const renameSubtitle = computed(() =>
  renameScope.value === 'lot'
    ? t('culling.renameLot.subtitle', { name: store.lotName || lotNameFromQuery.value, n: store.assets.length })
    : undefined
)

function onRenameApplied() {
  if (renameScope.value === 'selection') store.clearSelection()
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
          <Popover
            v-model:open="switcherOpen"
            :esc-dismisses="false"
            align="start"
          >
            <template #trigger>
              <button
                type="button"
                class="rounded-lg bg-[var(--color-border)]/30 px-2.5 py-1 text-[13px]"
              >
                {{ store.lotName || lotNameFromQuery }} ⌄
              </button>
            </template>
            <div
              role="listbox"
              :aria-label="t('culling.switcher.ariaLabel')"
              class="w-[220px]"
            >
              <button
                v-for="lot in switcherLots"
                :key="lot.folder_id"
                type="button"
                role="option"
                :aria-selected="lot.folder_id === lotId"
                class="flex w-full items-center justify-between gap-2 border-b border-border px-2 py-2 text-left text-[12.5px] last:border-b-0 hover:bg-border/30"
                :class="lot.folder_id === lotId ? 'font-semibold text-accent' : ''"
                @click="switchToLot(lot)"
              >
                <span class="truncate">{{ lot.name }}</span>
                <span class="shrink-0 text-content-muted">{{ t('culling.lotTodo', { n: lot.pending }) }}</span>
              </button>
            </div>
          </Popover>
          <div class="flex items-center gap-3 text-[13px]">
            <span>✓ <strong>{{ store.counts.taken }}</strong> {{ t('culling.counters.taken') }}</span>
            <span>✕ <strong>{{ store.counts.skipped }}</strong> {{ t('culling.counters.skipped') }}</span>
            <span>○ <strong>{{ store.counts.pending }}</strong> {{ t('culling.counters.pending') }}</span>
          </div>
        </div>

        <SelectionBar
          v-if="store.selectedCount > 0"
          :count="store.selectedCount"
          :ariaLabel="t('culling.selectionBar.ariaLabel')"
          @clear="store.clearSelection"
          @select-all="store.toggleSelectAllInQueue"
        >
          <Tooltip :label="t('culling.pick')">
            <button
              type="button"
              :aria-label="t('culling.pick')"
              class="flex h-8 w-8 items-center justify-center rounded-lg text-[#2E9E5B] hover:bg-[#2E9E5B]/10"
              @click="decideSelection('taken')"
            >
              ✓
            </button>
          </Tooltip>
          <Tooltip :label="t('culling.reject')">
            <button
              type="button"
              :aria-label="t('culling.reject')"
              class="flex h-8 w-8 items-center justify-center rounded-lg text-danger hover:bg-danger/10"
              @click="decideSelection('skipped')"
            >
              ✕
            </button>
          </Tooltip>
          <Tooltip :label="t('culling.selectionBar.renameTip')">
            <button
              type="button"
              :aria-label="t('culling.selectionBar.renameTip')"
              class="flex h-8 w-8 items-center justify-center rounded-lg text-content-muted hover:bg-border/40"
              @click="openRenameSelection"
            >
              ✎
            </button>
          </Tooltip>
        </SelectionBar>
        <div
          v-else
          class="flex flex-wrap items-center gap-1.5"
        >
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
            class="rounded-full border border-danger px-3 py-1 text-[12.5px] text-danger hover:bg-danger/10"
            @click="emptySkippedOpen = true"
          >
            {{ t('culling.emptySkipped.button', { n: store.counts.skipped }) }}
          </button>
          <Tooltip
            v-if="store.order.length > 0"
            :label="t('culling.selectAll.tip')"
            class="ml-auto"
          >
            <button
              type="button"
              :aria-label="t('culling.selectAll.ariaLabel')"
              class="flex h-[26px] w-[26px] items-center justify-center rounded-lg text-content-muted hover:bg-border/40"
              @click="store.selectAllInQueue"
            >
              ☑
            </button>
          </Tooltip>
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
            >{{
              store.currentAsset.raw_kind === 'raw+jpeg'
                ? t('ui.rawBadge.jpegPair')
                : t('ui.rawBadge.rawOnly')
            }}</span>
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
          :selected-ids="store.selectedIds"
          @select="store.goToId"
          @shift-select="store.selectRangeToThumb"
          @toggle="store.toggleSelect"
          @shift-toggle="store.selectRangeOrToggle"
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
              >
                {{ store.lotName || lotNameFromQuery }}
              </button>
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
            <button
              type="button"
              class="rounded-lg px-3 py-2 text-[13px] font-medium text-content-muted hover:bg-border/40"
              @click="openRenameLot"
            >
              {{ t('culling.renameLot.button') }}
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

    <RenameFormulaDialog
      v-model:open="renameOpen"
      :assets="renameAssets"
      :restricted-assets="pendingLotAssets"
      :has-subfolders="renameScope === 'lot'"
      :subtitle="renameSubtitle"
      @applied="onRenameApplied"
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
