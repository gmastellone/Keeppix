<script setup lang="ts">
// This view follows the same pattern as `FavoritesView.vue`
// (`FlatAssetGrid`, `useBrowseFilters`, `useLightboxRoute`): showing the
// photos contained in an album with the same grid tools as the rest of
// the app is the same shape as Favorites being "the timeline with a
// single section", just scoped to the album's members instead of
// favorites.
//
// The three distinct empty states stay distinct: filter too narrow,
// dynamic with no matches, manual and genuinely empty — three different
// situations even though the final rendering looks similar, a deliberate
// choice to avoid flattening them into one generic message.
//
// "Update album": a `rule`-based album on the real backend does not
// recompute itself automatically, it only updates on request
// (`POST /albums/{id}/refresh`, see `api/albums.ts`). Without this button
// a dynamic album would have no way to refresh its membership after
// creation.
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { albumMonthRange } from '@/albums/range'
import { fetchAlbum, fetchAlbumAssets, refreshAlbum, type Album, type AlbumAsset } from '@/api/albums'
import { ApiProblem, isUnauthenticated } from '@/api/client'
import type { TimelineAsset } from '@/api/timeline'
import AssetViewer from '@/components/AssetViewer.vue'
import FlatAssetGrid from '@/components/FlatAssetGrid.vue'
import LibrarySelectionActions from '@/components/LibrarySelectionActions.vue'
import ErrorState from '@/components/ui/ErrorState.vue'
import QuickFilter from '@/components/ui/QuickFilter.vue'
import SelectAllVisible from '@/components/ui/SelectAllVisible.vue'
import SelectionBar from '@/components/ui/SelectionBar.vue'
import { useBrowseFilters } from '@/composables/useBrowseFilters'
import { useDensity } from '@/composables/useDensity'
import { useLightboxRoute } from '@/composables/useLightboxRoute'
import { classifyError } from '@/errors/classify'
import { activeAlbumName } from '@/nav/routeTitles'
import { useFavoritesStore } from '@/stores/favorites'
import { useMapsStore } from '@/stores/maps'
import { useSelectionStore } from '@/stores/selection'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

const { t, locale } = useI18n()
const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const toast = useToastStore()
const maps = useMapsStore()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const { density, setDensity } = useDensity()

const album = ref<Album | null>(null)
const members = ref<AlbumAsset[]>([])
const loaded = ref(false)
const loadError = ref<unknown>(null)
const refreshing = ref(false)

const errorNature = computed(() => (loadError.value ? classifyError(loadError.value) : null))
const errorDetail = computed(() =>
  loadError.value instanceof ApiProblem ? `${loadError.value.type} · ${loadError.value.status}` : undefined
)

const totalCount = computed(() => members.value.length)

const { selection: filterSelection, dimensions: filterDimensions, filteredAssets } = useBrowseFilters(members)

const lightbox = useLightboxRoute<TimelineAsset>(
  (id) => filteredAssets.value.find((asset) => asset.id === id),
  (id) => maps.loadAsset(id)
)

function stepViewer(asset: TimelineAsset) {
  void lightbox.step(asset)
}

function openViewerAsset(id: string) {
  void lightbox.openById(id)
}

const selectionMode = computed(() => selection.library.selectedIds.size > 0)
const selectedAssets = computed(() => members.value.filter((asset) => selection.library.selectedIds.has(asset.id)))

function selectAllVisible() {
  selection.library.selectAllVisible(filteredAssets.value.map((asset) => asset.id))
}

const subtitle = computed(() => {
  if (!album.value) return ''
  const range =
    albumMonthRange(members.value, locale.value) ?? t(album.value.rule ? 'albums.noMatch' : 'albums.noPhotosYet')
  let text = t('albums.cardSubtitle', { count: totalCount.value, range })
  if (album.value.is_shared) text += t('albums.detail.sharedSuffix')
  if (album.value.rule) text += t('albums.detail.dynamicSuffix')
  return text
})

// The three empty states — mutually exclusive, in this order.
const emptyState = computed<'filtered' | 'dynamic' | 'manual' | null>(() => {
  if (!loaded.value) return null
  if (totalCount.value === 0) return album.value?.rule ? 'dynamic' : 'manual'
  if (filteredAssets.value.length === 0) return 'filtered'
  return null
})

async function load() {
  loadError.value = null
  loaded.value = false
  const id = route.params.id as string
  try {
    const [detail, assets] = await Promise.all([fetchAlbum(id), fetchAlbumAssets(id)])
    album.value = detail
    members.value = assets
    loaded.value = true
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = error
  }
}

async function refresh() {
  if (!album.value || refreshing.value) return
  refreshing.value = true
  try {
    await refreshAlbum(album.value.id)
    await load()
    toast.show(t('albums.detail.refreshDone'))
  } catch {
    toast.showError(t('albums.detail.refreshError'))
  } finally {
    refreshing.value = false
  }
}

function goBack() {
  void router.push('/albums')
}

watch(album, (current) => {
  activeAlbumName.value = current?.name ?? null
})

onMounted(load)
onUnmounted(() => {
  activeAlbumName.value = null
})
</script>

<template>
  <div class="flex h-full flex-col">
    <ErrorState
      v-if="errorNature"
      :nature="errorNature"
      :technical-detail="errorDetail"
      @retry="load"
    />
    <template v-else>
      <div class="border-b border-border px-4 py-3">
        <button
          type="button"
          class="mb-1 flex items-center gap-1 text-[13px] text-content-muted hover:text-content"
          @click="goBack"
        >
          {{ t('albums.backLink') }}
        </button>
        <div class="flex items-center gap-2">
          <p class="text-[15px] font-bold">
            {{ album?.name }}
          </p>
          <span
            v-if="album?.rule"
            class="rounded-full bg-accent-tint px-2 py-0.5 text-[10.5px] font-bold text-accent"
          >
            {{ t('albums.dynamicBadge') }}
          </span>
          <button
            v-if="album?.rule"
            type="button"
            class="ml-auto rounded-lg border border-border px-2.5 py-1 text-[12px] font-semibold disabled:opacity-50"
            :disabled="refreshing"
            @click="refresh"
          >
            {{ t('albums.detail.refresh') }}
          </button>
        </div>
        <p class="text-sm text-content-muted">
          {{ subtitle }}
        </p>
      </div>

      <div
        v-if="!selectionMode"
        class="flex items-center gap-3 border-b border-border px-4 py-3"
      >
        <div class="ml-auto flex items-center gap-2">
          <SelectAllVisible
            v-if="filteredAssets.length > 0"
            :visible-count="filteredAssets.length"
            @select-all="selectAllVisible"
          />
          <QuickFilter
            v-model:selection="filterSelection"
            :dimensions="filterDimensions"
            :result-count="filteredAssets.length"
          />
          <button
            class="rounded-lg border border-border px-2 py-1"
            :aria-label="t('timeline.densityDown')"
            @click="setDensity(density - 1)"
          >
            −
          </button>
          <span class="w-4 text-center text-sm">{{ density }}</span>
          <button
            class="rounded-lg border border-border px-2 py-1"
            :aria-label="t('timeline.densityUp')"
            @click="setDensity(density + 1)"
          >
            +
          </button>
        </div>
      </div>
      <div :class="selectionMode && 'border-b border-border px-4 py-3'">
        <SelectionBar
          :count="selection.library.selectedIds.size"
          :ariaLabel="t('ui.selectionBar.ariaLabel')"
          @clear="selection.library.clear()"
          @select-all="selectAllVisible"
        >
          <LibrarySelectionActions :assets="selectedAssets" />
        </SelectionBar>
      </div>

      <div
        v-if="emptyState === 'filtered'"
        class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
      >
        <p class="text-sm font-semibold">
          {{ t('ui.filteredEmpty.title') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('ui.filteredEmpty.subtitle') }}
        </p>
      </div>
      <div
        v-else-if="emptyState === 'dynamic'"
        class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
      >
        <p class="text-sm font-semibold">
          {{ t('albums.detail.emptyDynamicTitle') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('albums.detail.emptyDynamicSubtitle') }}
        </p>
      </div>
      <div
        v-else-if="emptyState === 'manual'"
        class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
      >
        <p class="text-sm font-semibold">
          {{ t('albums.detail.emptyManualTitle') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('albums.detail.emptyManualSubtitle') }}
        </p>
      </div>
      <FlatAssetGrid
        v-else
        :assets="filteredAssets"
        :density="density"
        @open="lightbox.open"
      />
    </template>
    <AssetViewer
      v-if="lightbox.viewing.value"
      :asset="lightbox.viewing.value"
      :neighbors="filteredAssets"
      :is-favorite="favorites.isFavorite(lightbox.viewing.value)"
      @close="lightbox.close"
      @step="stepViewer"
      @open-asset="openViewerAsset"
      @toggle-favorite="favorites.toggleOne(lightbox.viewing.value)"
    />
  </div>
</template>
