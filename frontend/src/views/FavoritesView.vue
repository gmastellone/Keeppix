<script setup lang="ts">
// This is the timeline with a single section and no title: the same
// tile, the same selection bar, the same justified grid and the same
// virtualization — but with no month grouping, no month headers, and
// **no scrubber**.
//
// No "geometry only" endpoint for a flat list like the one that exists
// for the timeline (`fetchGeometry`): not needed. `runSearch({op:
// 'favorite'})` (`SearchNode::Favorite` in
// crates/keeppix-db/src/search.rs) returns the same `TimelineAsset`
// objects with `width`/`height` already included: `justify()` (pure,
// independent of the geometry blob) is enough on its own for layout,
// with no need for a second endpoint.
//
// The visible grid is `assets` filtered by `favorites.isFavorite` — not
// just the loaded snapshot: removing a heart (single, or as a group from
// the selection bar, same store shared with Timeline) makes the tile
// disappear from here **immediately**, by construction, with no need
// for a dedicated "remove from view" handler — the heart here removes
// the photo from view, with no confirmation, no toast, no undo, which is
// exactly the behavior this derivation gives for free.
//
// The quick filter (`useBrowseFilters`) composes on top of this same
// filter — `favoriteAssets` first, `filteredAssets` after — and the grid
// itself (justified, virtualized) is now `FlatAssetGrid.vue`, extracted
// here and reused by Timeline when a filter is active.
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { runSearch } from '@/api/library'
import type { TimelineAsset } from '@/api/timeline'
import { startLiveEvents, type LiveSocket } from '@/api/events'
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
import { ApiProblem } from '@/api/client'
import { useFavoritesStore } from '@/stores/favorites'
import { useMapsStore } from '@/stores/maps'
import { useSelectionStore } from '@/stores/selection'

const { t } = useI18n()
const maps = useMapsStore()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const { density, setDensity } = useDensity()

const assets = ref<TimelineAsset[]>([])
const loaded = ref(false)
const loadError = ref<unknown>(null)

let live: LiveSocket | undefined

const errorNature = computed(() => (loadError.value ? classifyError(loadError.value) : null))
const errorDetail = computed(() =>
  loadError.value instanceof ApiProblem ? `${loadError.value.type} · ${loadError.value.status}` : undefined
)

// The subtitle counts favorites **before** filters.
const totalCount = computed(() => assets.value.length)
const favoriteAssets = computed(() => assets.value.filter((asset) => favorites.isFavorite(asset)))

// The quick filter on Favorites: the same six dimensions as the
// timeline, scoped to favorites only (`favoriteAssets`, not the whole
// library) — consistent with the result count being computed on this
// view's own list.
const { selection: filterSelection, dimensions: filterDimensions, filteredAssets } = useBrowseFilters(favoriteAssets)

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
const selectedAssets = computed(() => favoriteAssets.value.filter((asset) => selection.library.selectedIds.has(asset.id)))

// "Only what falls within the filter" when a quick filter is active —
// never the whole underlying set of favorites.
function selectAllVisible() {
  selection.library.selectAllVisible(filteredAssets.value.map((asset) => asset.id))
}

async function loadFavorites() {
  loadError.value = null
  loaded.value = false
  try {
    const collected: TimelineAsset[] = []
    let cursor: string | undefined
    do {
      const page = await runSearch({ op: 'favorite' }, cursor)
      collected.push(...page.assets)
      cursor = page.next_cursor
    } while (cursor)
    assets.value = collected
    loaded.value = true
  } catch (error) {
    loadError.value = error
  }
}

onMounted(async () => {
  await loadFavorites()
  live = startLiveEvents((msg) => {
    if (msg.type === 'resync' || msg.type === 'assets.upserted' || msg.type === 'assets.deleted') {
      void loadFavorites()
    }
  })
})

onUnmounted(() => {
  live?.close()
})
</script>

<template>
  <div class="flex h-full flex-col">
    <ErrorState
      v-if="errorNature"
      :nature="errorNature"
      :technical-detail="errorDetail"
      @retry="loadFavorites"
    />

    <!-- First empty state: no favorites at all — in this case not even
         the toolbar is drawn. -->
    <div
      v-else-if="loaded && totalCount === 0"
      class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
    >
      <p class="text-sm font-semibold">
        {{ t('favorites.emptyTitle') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('favorites.emptySubtitle') }}
      </p>
    </div>

    <template v-else>
      <div class="border-b border-border px-4 py-3">
        <p class="text-[15px] font-bold">
          {{ t('favorites.title') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('favorites.subtitle', { n: totalCount }) }}
        </p>
      </div>

      <div
        v-if="!selectionMode"
        class="flex items-center gap-3 border-b border-border px-4 py-3"
      >
        <div class="ml-auto flex items-center gap-2">
          <SelectAllVisible
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

      <!-- Second empty state: there are favorites but none is currently
           visible — either because the last visible heart was removed,
           or because the quick filter finds no matches: same wording for
           both, the visual situation is identical ("you had photos, now
           you see none"). -->
      <div
        v-if="filteredAssets.length === 0"
        class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
      >
        <p class="text-sm font-semibold">
          {{ t('ui.filteredEmpty.title') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('ui.filteredEmpty.subtitle') }}
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
