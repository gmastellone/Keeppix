<script setup lang="ts">
// Fase 11 Task 16 (1/N) — documento funzionale §32 "Persone — dettaglio
// di una persona" (righe 5420-5524), verificato riga per riga.
//
// **Tre dei cinque pulsanti d'azione mancano ancora** (§32.3, controlli
// 3-5: "Scegli copertina", "Assegna/Cambia gruppo", "Dividi…") — i loro
// dialog (§33-34, §36) sono sotto-unità successive; aggiungerli ora
// senza dialog dietro sarebbe un pulsante che non fa nulla. Restano
// "Rinomina" e "Nascondi"/"Mostra di nuovo" (controlli 2 e 6), entrambi
// pienamente reali (`PATCH /persons/{id}`).
//
// **Griglia foto = `photosForPerson()`** (§32.2): `runSearch({op:
// 'person', id})`, paginata come Preferiti (Task 7 3/N, stesso schema
// `do…while(cursor)` — nessuna foto di una persona è mai così numerosa
// da giustificare un caricamento incrementale, stesso ragionamento già
// fatto lì). SP-3 (`useBrowseFilters`) sopra l'insieme delle foto della
// persona, non sopra l'intera libreria — §32.3 controllo 8: "qui è
// particolare che la dimensione 'Persone' sia ancora presente e possa
// restringere ulteriormente alle co-presenze", comportamento che questa
// riduzione dà gratis (la dimensione Persone del filtro rapido esiste
// già, Fase 11 Task 7).
//
// **"Mostra di nuovo" resta di fatto irraggiungibile** (§32.7, nota
// importante): nascondendo si torna alla griglia, e le persone nascoste
// sono escluse da `visiblePeople()` — nessun percorso reale per
// riaprire questo dettaglio su una persona nascosta. Non "risolto" qui,
// stesso comportamento del documento.
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { startLiveEvents, type LiveSocket } from '@/api/events'
import { runSearch } from '@/api/library'
import { thumbSrc } from '@/api/media'
import { fetchPerson, patchPerson, type Person } from '@/api/persons'
import type { TimelineAsset } from '@/api/timeline'
import AssetViewer from '@/components/AssetViewer.vue'
import FlatAssetGrid from '@/components/FlatAssetGrid.vue'
import LibrarySelectionActions from '@/components/LibrarySelectionActions.vue'
import Dialog from '@/components/ui/Dialog.vue'
import QuickFilter from '@/components/ui/QuickFilter.vue'
import SelectAllVisible from '@/components/ui/SelectAllVisible.vue'
import SelectionBar from '@/components/ui/SelectionBar.vue'
import TextField from '@/components/ui/TextField.vue'
import { useBrowseFilters } from '@/composables/useBrowseFilters'
import { useDensity } from '@/composables/useDensity'
import { useLightboxRoute } from '@/composables/useLightboxRoute'
import { activePersonName } from '@/nav/routeTitles'
import { useFavoritesStore } from '@/stores/favorites'
import { useMapsStore } from '@/stores/maps'
import { useSelectionStore } from '@/stores/selection'
import { useToastStore } from '@/stores/toast'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const toast = useToastStore()
const maps = useMapsStore()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const { density, setDensity } = useDensity()

const personId = computed(() => route.params.id as string)
const person = ref<Person | null>(null)
const notFound = ref(false)
const assets = ref<TimelineAsset[]>([])
const loaded = ref(false)

let live: LiveSocket | undefined

async function loadPhotos() {
  const collected: TimelineAsset[] = []
  let cursor: string | undefined
  do {
    const page = await runSearch({ op: 'person', id: personId.value }, cursor)
    collected.push(...page.assets)
    cursor = page.next_cursor
  } while (cursor)
  assets.value = collected
}

async function load() {
  loaded.value = false
  notFound.value = false
  try {
    person.value = await fetchPerson(personId.value)
    activePersonName.value = person.value.name?.trim() || null
    await loadPhotos()
    loaded.value = true
  } catch {
    // §32.8: "se la persona sparisce… si ricade automaticamente sulla
    // griglia" — stesso comportamento per un id inesistente o non più
    // visibile (403, mai un 404 — vedi il commento del backend).
    notFound.value = true
    await router.replace('/persons')
  }
}

watch(personId, load)

const { selection: filterSelection, dimensions: filterDimensions, filteredAssets } = useBrowseFilters(assets)

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
const selectedAssets = computed(() => assets.value.filter((asset) => selection.library.selectedIds.has(asset.id)))

function selectAllVisible() {
  selection.library.selectAllVisible(filteredAssets.value.map((asset) => asset.id))
}

function displayName(p: Person): string {
  return p.name?.trim() || t('persons.unnamed')
}

const coverStyle = computed(() => {
  const first = assets.value[0]
  if (!first) return {}
  if (first.content_hash) return { backgroundImage: `url(${thumbSrc(first.content_hash)})` }
  if (first.thumbhash) {
    const url = thumbhashToDataURL(first.thumbhash)
    if (url) return { backgroundImage: `url(${url})` }
  }
  return {}
})

const renameOpen = ref(false)
const renameValue = ref('')

function openRename() {
  renameValue.value = person.value?.name ?? ''
  renameOpen.value = true
}

async function saveRename() {
  if (!person.value) return
  try {
    person.value = await patchPerson(person.value.id, { name: renameValue.value.trim() })
    activePersonName.value = person.value.name?.trim() || null
    renameOpen.value = false
  } catch {
    toast.showError(t('persons.renameError'))
  }
}

async function toggleHidden() {
  if (!person.value) return
  const next = !person.value.hidden
  try {
    await patchPerson(person.value.id, { hidden: next })
    if (next) {
      toast.show(t('persons.hiddenToast'))
      await router.push('/persons')
    } else {
      person.value.hidden = false
      toast.show(t('persons.shownToast'))
    }
  } catch {
    toast.showError(t('persons.hideError'))
  }
}

onMounted(async () => {
  await load()
  live = startLiveEvents((msg) => {
    if (msg.type === 'resync' || msg.type === 'assets.upserted' || msg.type === 'assets.deleted') {
      void loadPhotos()
    }
  })
})

onUnmounted(() => {
  live?.close()
  activePersonName.value = null
})
</script>

<template>
  <div class="flex h-full flex-col">
    <template v-if="person">
      <div class="border-b border-border px-4 py-3">
        <RouterLink
          to="/persons"
          class="text-[13px] text-content-muted hover:text-content"
        >
          ← {{ t('persons.backLink') }}
        </RouterLink>

        <div class="mt-2 flex items-center gap-3.5">
          <span
            class="h-[78px] w-[78px] shrink-0 rounded-full border border-border bg-cover bg-center bg-surface-elevated"
            :style="coverStyle"
            aria-hidden="true"
          />
          <div>
            <p class="text-[16px] font-bold">
              {{ displayName(person) }}
            </p>
            <p class="text-[12.5px] text-content-muted">
              {{ t('persons.photoCount', { n: assets.length }, { plural: assets.length }) }}
              <template v-if="person.hidden"> · {{ t('persons.hiddenLabel') }}</template>
            </p>
          </div>
        </div>

        <div class="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
            @click="openRename"
          >
            {{ t('persons.rename') }}
          </button>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
            @click="toggleHidden"
          >
            {{ person.hidden ? t('persons.show') : t('persons.hide') }}
          </button>
        </div>
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

      <div
        v-if="loaded && assets.length === 0"
        class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
      >
        <p class="text-sm font-semibold">
          {{ t('persons.emptyPhotosTitle') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('persons.emptyPhotosSubtitle') }}
        </p>
      </div>
      <div
        v-else-if="filteredAssets.length === 0"
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

    <Dialog
      v-model:open="renameOpen"
      :title="t('persons.renameTitle')"
    >
      <div class="flex flex-col gap-3.5">
        <TextField
          v-model="renameValue"
          :label="t('persons.name')"
        />
        <div class="mt-1 flex items-center gap-2">
          <button
            type="button"
            class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-text"
            @click="saveRename"
          >
            {{ t('persons.save') }}
          </button>
          <button
            type="button"
            class="rounded-lg border border-transparent px-3.5 py-2 text-[13px] font-semibold hover:bg-border/30"
            @click="renameOpen = false"
          >
            {{ t('ui.dialog.cancel') }}
          </button>
        </div>
      </div>
    </Dialog>
  </div>
</template>
