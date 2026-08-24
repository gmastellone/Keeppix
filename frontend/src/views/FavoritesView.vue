<script setup lang="ts">
// Fase 11 Task 7 (3/N) — documento funzionale §9 ("Preferiti"),
// verificato riga per riga (righe 1755-1833). "È la timeline con una
// sola sezione e senza titolo" (§9.2): stessa tessera (SP-1), stessa
// barra di selezione (SP-2), stessa griglia giustificata e la stessa
// virtualizzazione (SP-22) — ma senza raggruppamento per mese, senza
// intestazioni di mese e **senza scrubber**.
//
// Niente endpoint "sola geometria" per una lista piatta come esiste per
// la timeline (`fetchGeometry`, Task 4): non serve. `runSearch({op:
// 'favorite'})` (verificato in crates/keeppix-db/src/search.rs,
// `SearchNode::Favorite`, già pronto dal Task 6/10 del backend — mai
// consumato dal frontend finora) restituisce gli stessi `TimelineAsset`
// con `width`/`height` già inclusi: `justify()` (già scritta nel Task 4,
// pura, indipendente dal blob di geometria) basta da sola per il
// layout, senza bisogno di un secondo endpoint.
//
// La griglia visibile è `assets` filtrati per `favorites.isFavorite` —
// non solo lo snapshot caricato: un cuoricino tolto (singolo o di
// gruppo dalla barra di selezione, stesso store condiviso con Timeline)
// fa sparire la tessera da qui **subito**, per costruzione, senza
// bisogno di un handler dedicato "rimuovi dalla vista" (§9.3: "il
// cuoricino qui toglie la foto dalla vista... senza conferma, senza
// toast, senza annulla" — è esattamente il comportamento che questa
// derivazione dà gratis).
//
// Task 7 (6/N): il filtro rapido SP-3 (`useBrowseFilters`) si compone
// sopra questo stesso filtro — `favoriteAssets` prima, `filteredAssets`
// dopo — e la griglia stessa (giustificata, virtualizzata) è ora
// `FlatAssetGrid.vue`, estratta qui e riusata da Timeline quando un
// filtro è attivo (Task 7, 7/N).
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

// §9.2: il sottotitolo conta i preferiti **prima** dei filtri.
const totalCount = computed(() => assets.value.length)
const favoriteAssets = computed(() => assets.value.filter((asset) => favorites.isFavorite(asset)))

// Task 7 (6/N) — SP-3 sui Preferiti: le stesse sei dimensioni della
// timeline (§9.3), scoped ai soli preferiti (`favoriteAssets`, non
// all'intera libreria) — coerente con "N è calcolato sulla lista di
// questa vista" (§11, piede del pannello).
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

// SP-4: "solo ciò che ricade nel filtro" quando un filtro rapido è
// attivo — mai l'intero insieme dei preferiti sottostante.
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

    <!-- §9.2, primo stato vuoto: nessun preferito in assoluto — "in
         questo caso non viene disegnata nemmeno la barra strumenti". -->
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

      <!-- §9.2, secondo stato vuoto: ci sono preferiti ma nessuno è
           attualmente visibile — sia perché l'ultimo cuoricino visibile è
           stato tolto, sia perché il filtro rapido (SP-3) non trova
           corrispondenze: stessa dicitura per entrambe, la situazione
           visiva è identica ("avevi delle foto, ora non ne vedi
           nessuna"). -->
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
