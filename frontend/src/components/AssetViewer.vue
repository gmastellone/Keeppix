<script setup lang="ts">
// Fase 11 Task 8 (2/N) — documento funzionale §18 ("Lightbox — struttura
// e barra superiore") e §20 ("Menu 'altre azioni' ⋯"), riscrittura del
// segnaposto precedente (151 righe, solo apri/chiudi/frecce/un campo di
// posizione). Ambito di questa unità: barra superiore, stage con frecce,
// filmino, menu ⋯ con le cinque azioni reali. Il pannello informazioni
// resta il contenuto minimo già esistente (nome file/data/dimensioni/
// mini-mappa) — la riscrittura piena di §19 (titolo modificabile,
// stelle, sezioni PERSONE/TAG/ALBUM, riquadri volto) sono le prossime
// unità di questo stesso Task.
//
// **Debito dichiarato, verificato e non taciuto**:
// - "Condividi" (§18.3 riga 3) omesso: apre un dialog che non esiste
//   ancora (Task 11 "Condivisioni"), stessa motivazione già usata per
//   la barra di selezione in Task 7 (2/N).
// - Riquadri volto sull'immagine (§18.2): la loro **visibilità** è
//   guidata dall'hover sui chip persona del pannello (§19, animazioni:
//   "restano invisibili... compaiono solo passando sopra il nome
//   corrispondente") — costruirli ora, senza quei chip, produrrebbe
//   riquadri per sempre invisibili. Rimandati all'unità che costruisce
//   la sezione PERSONE.
// - "Ruota" resta un toast (nessuna pipeline di rotazione reale esiste
//   ancora — dichiarato nel Task 8 1/N: `orientation` è scrivibile ma
//   mai consumato da `keeppix-media`).
//
// **Corretto qui, non solo aggiunto**: il click sullo sfondo nero
// *non* deve chiudere il lightbox (§18.4, esplicito — a differenza
// dello scrim dei dialog modali, SP-5) — la versione precedente aveva
// `@click.self="emit('close')"` sul contenitore radice, un
// comportamento mai documentato per questa vista. Rimosso.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { apiFetch } from '@/api/client'
import { deleteAsset, type DiskAction } from '@/api/culling'
import { originalSrc, previewSrc as mediaPreviewSrc, thumbSrc as mediaThumbSrc } from '@/api/media'
import type { TimelineAsset } from '@/api/timeline'
import AlbumPickerDialog from '@/components/AlbumPickerDialog.vue'
import RenameFormulaDialog from '@/components/RenameFormulaDialog.vue'
import DeleteDialog, { type DeleteChoice } from '@/components/ui/DeleteDialog.vue'
import Popover from '@/components/ui/Popover.vue'
import MapClusterLayer from '@/components/MapClusterLayer.vue'
import { useMapsStore } from '@/stores/maps'
import { useToastStore } from '@/stores/toast'

const props = withDefaults(
  defineProps<{
    asset: TimelineAsset
    /** L'insieme di navigazione (frecce + filmino), nell'ordine di
     * visualizzazione — §18.2/§18.8: "tutte le foto della stessa
     * cartella e dello stesso mese" per la libreria, già calcolato dal
     * chiamante (ogni vista sa qual è il proprio "vicinato": `loadedAssets`
     * per Timeline, `filteredAssets` per Preferiti/Cerca). Vuoto di
     * default: nessuna freccia, nessun filmino — il popover della mappa
     * non ha un concetto di vicinato e continua a funzionare senza
     * modifiche. */
    neighbors?: TimelineAsset[]
    isFavorite: boolean
  }>(),
  { neighbors: () => [] }
)
const emit = defineEmits<{
  close: []
  /** Sostituisce i due emit separati `prev`/`next` del segnaposto
   * precedente: frecce, filmino e tastiera risolvono già l'asset di
   * destinazione da `neighbors`, il chiamante non deve più rifare la
   * stessa ricerca (`viewingNeighbour`) che il vecchio contratto gli
   * imponeva. */
  step: [asset: TimelineAsset]
  'open-asset': [id: string]
  'toggle-favorite': []
}>()
const { t } = useI18n()
const maps = useMapsStore()
const toast = useToastStore()

const info = ref(false)
const moreOpen = ref(false)
const albumDialogOpen = ref(false)
const renameDialogOpen = ref(false)
const deleteDialogOpen = ref(false)
const metadata = ref<{
  location: { lat: number; lon: number } | null
}>()
const placeName = ref<string | null>(null)
let metadataRequestSequence = 0

function previewSrc(asset: TimelineAsset): string {
  return asset.content_hash
    ? mediaPreviewSrc(asset.content_hash)
    : `/media/original/${asset.id}`
}

const src = computed(() => previewSrc(props.asset))

const currentIndex = computed(() => props.neighbors.findIndex((n) => n.id === props.asset.id))
const prevAsset = computed(() =>
  currentIndex.value > 0 ? props.neighbors[currentIndex.value - 1] : undefined
)
const nextAsset = computed(() =>
  currentIndex.value >= 0 && currentIndex.value < props.neighbors.length - 1
    ? props.neighbors[currentIndex.value + 1]
    : undefined
)
const prevSrc = computed(() => (prevAsset.value ? previewSrc(prevAsset.value) : undefined))
const nextSrc = computed(() => (nextAsset.value ? previewSrc(nextAsset.value) : undefined))

function stepTo(target: TimelineAsset | undefined) {
  if (target) emit('step', target)
}

/** §18.5: `Esc` a due livelli — un menu ⋯ aperto ne assorbe la prima
 * pressione. Controllato qui, non lasciato al layering di reka-ui (il
 * lightbox stesso non è un `DialogRoot`/`PopoverRoot`, solo il menu ⋯
 * lo è): un singolo `keydown` globale deve sapere qual è il primo
 * livello da chiudere prima di arrivare al secondo. */
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    if (moreOpen.value) {
      moreOpen.value = false
      return
    }
    emit('close')
    return
  }
  if (e.key === 'i' || e.key === 'I') {
    info.value = !info.value
    if (info.value) void loadMetadata()
    return
  }
  if (e.key === 'f' || e.key === 'F') {
    emit('toggle-favorite')
    return
  }
  if (e.key === 'ArrowLeft') {
    stepTo(prevAsset.value)
    return
  }
  if (e.key === 'ArrowRight') {
    stepTo(nextAsset.value)
  }
}

async function loadMetadata() {
  const sequence = ++metadataRequestSequence
  const assetId = props.asset.id
  try {
    const response = await apiFetch<{
      location: { lat: number; lon: number } | null
    }>(`/api/v1/assets/${assetId}/metadata`)
    if (sequence === metadataRequestSequence && assetId === props.asset.id) {
      metadata.value = response
      placeName.value = null
      if (response.location) {
        maps.reverseGeocode(response.location.lat, response.location.lon)
          .then((place) => {
            if (sequence === metadataRequestSequence) {
              placeName.value = place?.name ?? null
            }
          })
          .catch(() => { /* best-effort */ })
      }
    }
  } catch {
    if (sequence === metadataRequestSequence && assetId === props.asset.id) {
      metadata.value = undefined
    }
  }
}

onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))
watch(
  () => props.asset.id,
  () => {
    metadataRequestSequence += 1
    metadata.value = undefined
    placeName.value = null
    if (info.value) void loadMetadata()
  }
)

/** §20.3: `closeMoreAnd(fn)` — il menu chiude e ridisegna **prima**
 * dell'azione, così il dialog che si apre non trova il menu ancora
 * sopra di sé. */
function closeMoreThen(fn: () => void) {
  moreOpen.value = false
  void nextTick(fn)
}

function rotateStub() {
  toast.show(t('viewer.menu.rotateToast'))
}

const DISK_ACTION: Record<DeleteChoice, DiskAction> = {
  index: 'kept',
  trash: 'moved_to_trash',
  disk: 'purged'
}

async function confirmDelete(choice: DeleteChoice) {
  try {
    await deleteAsset(props.asset.id, DISK_ACTION[choice])
    toast.show(t('librarySelectionActions.deleted', { n: 1 }, { plural: 1 }))
    emit('close')
  } catch {
    toast.showError(t('librarySelectionActions.deleteError'))
  }
}

const renameSubtitle = computed(() => t('renameFormula.subtitleSingle', { filename: props.asset.filename }))
</script>

<template>
  <div
    class="fixed inset-0 z-50 flex flex-col bg-black text-[#f2f2f2]"
    role="dialog"
    :aria-label="t('viewer.title')"
  >
    <img
      v-if="prevSrc"
      :src="prevSrc"
      alt=""
      class="hidden"
    >
    <img
      v-if="nextSrc"
      :src="nextSrc"
      alt=""
      class="hidden"
    >

    <div class="flex flex-none items-center justify-between gap-2 px-4 py-3">
      <div class="flex min-w-0 items-center gap-1.5">
        <button
          type="button"
          class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-[#f2f2f2] hover:bg-white/10"
          :aria-label="t('viewer.close')"
          @click="emit('close')"
        >
          <svg
            viewBox="0 0 24 24"
            width="18"
            height="18"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            aria-hidden="true"
          >
            <path d="M5 5l14 14M19 5L5 19" />
          </svg>
        </button>
        <span class="truncate text-[13px] text-[#d8d8d8]">{{ asset.filename }}</span>
      </div>

      <div class="flex shrink-0 items-center gap-1.5">
        <button
          type="button"
          class="flex h-8 w-8 items-center justify-center rounded-md hover:bg-white/10"
          :class="isFavorite ? 'text-accent' : 'text-[#f2f2f2]'"
          :aria-label="t(isFavorite ? 'viewer.favoriteOn' : 'viewer.favoriteOff')"
          @click="emit('toggle-favorite')"
        >
          <svg
            viewBox="0 0 24 24"
            width="17"
            height="17"
            :fill="isFavorite ? 'currentColor' : 'none'"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path
              d="M12 21s-7.5-4.6-10-9C.3 8.3 2 4 6 4c2.2 0 3.7 1.2 6 3.6C14.3 5.2 15.8 4 18 4c4 0 5.7 4.3 4 8-2.5 4.4-10 9-10 9z"
            />
          </svg>
        </button>
        <button
          type="button"
          class="flex h-8 w-8 items-center justify-center rounded-md hover:bg-white/10"
          :class="info ? 'text-accent' : 'text-[#f2f2f2]'"
          :aria-label="t('viewer.info')"
          @click="info = !info; if (info) void loadMetadata()"
        >
          <svg
            viewBox="0 0 24 24"
            width="17"
            height="17"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <circle
              cx="12"
              cy="12"
              r="9"
            />
            <path d="M12 11v5.5M12 8v.01" />
          </svg>
        </button>
        <Popover
          v-model:open="moreOpen"
          side="bottom"
          align="end"
        >
          <template #trigger>
            <button
              type="button"
              role="button"
              tabindex="0"
              aria-haspopup="true"
              :aria-expanded="moreOpen"
              :aria-label="t('viewer.moreActions')"
              class="relative flex h-8 w-8 items-center justify-center rounded-md text-[#f2f2f2]
                     hover:bg-white/10 focus-visible:outline-2 focus-visible:outline-offset-2
                     focus-visible:outline-accent"
            >
              <svg
                viewBox="0 0 24 24"
                width="17"
                height="17"
                fill="currentColor"
                aria-hidden="true"
              >
                <circle
                  cx="5"
                  cy="12"
                  r="1.8"
                />
                <circle
                  cx="12"
                  cy="12"
                  r="1.8"
                />
                <circle
                  cx="19"
                  cy="12"
                  r="1.8"
                />
              </svg>
            </button>
          </template>
          <div class="flex w-[188px] flex-col gap-0.5 py-0.5 text-[13px] text-[var(--color-content)]">
            <a
              :href="originalSrc(asset.id)"
              :download="asset.filename"
              class="rounded-md px-2.5 py-2 hover:bg-[var(--color-chip-bg)]"
              @click="moreOpen = false"
            >
              {{ t('viewer.menu.download') }}
            </a>
            <button
              type="button"
              class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
              @click="closeMoreThen(rotateStub)"
            >
              {{ t('viewer.menu.rotate') }}
            </button>
            <button
              type="button"
              class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
              @click="closeMoreThen(() => (albumDialogOpen = true))"
            >
              {{ t('viewer.menu.addToAlbum') }}
            </button>
            <button
              type="button"
              class="rounded-md px-2.5 py-2 text-left hover:bg-[var(--color-chip-bg)]"
              @click="closeMoreThen(() => (renameDialogOpen = true))"
            >
              {{ t('viewer.menu.rename') }}
            </button>
            <div class="my-0.5 h-px bg-[var(--color-border)]" />
            <button
              type="button"
              class="rounded-md px-2.5 py-2 text-left text-danger hover:bg-[var(--color-chip-bg)]"
              @click="closeMoreThen(() => (deleteDialogOpen = true))"
            >
              {{ t('viewer.menu.delete') }}
            </button>
          </div>
        </Popover>
      </div>
    </div>

    <div class="flex min-h-0 flex-1">
      <div class="relative min-w-0 flex-1 px-[60px] py-2.5">
        <button
          v-if="prevAsset"
          type="button"
          :aria-label="t('viewer.prev')"
          class="absolute top-1/2 left-2 z-[1] flex h-[38px] w-[38px] -translate-y-1/2 items-center
                 justify-center rounded-full bg-white/[.08] text-[#f2f2f2] hover:bg-white/[.18]"
          @click="stepTo(prevAsset)"
        >
          <svg
            viewBox="0 0 24 24"
            width="20"
            height="20"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M15 5l-7 7 7 7" />
          </svg>
        </button>
        <div class="relative h-full w-full">
          <img
            :src="src"
            :alt="asset.filename"
            class="m-auto h-full max-h-full w-full max-w-full rounded-md object-contain"
          >
        </div>
        <button
          v-if="nextAsset"
          type="button"
          :aria-label="t('viewer.next')"
          class="absolute top-1/2 right-2 z-[1] flex h-[38px] w-[38px] -translate-y-1/2 items-center
                 justify-center rounded-full bg-white/[.08] text-[#f2f2f2] hover:bg-white/[.18]"
          @click="stepTo(nextAsset)"
        >
          <svg
            viewBox="0 0 24 24"
            width="20"
            height="20"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </div>
      <aside
        v-if="info"
        class="w-[296px] shrink-0 overflow-y-auto border-l border-[#232323] bg-[#0c0c0c] p-[18px] text-sm"
      >
        <p>{{ asset.filename }}</p>
        <p v-if="asset.taken_at_utc">
          {{ asset.taken_at_utc }}
        </p>
        <p v-if="asset.width && asset.height">
          {{ asset.width }} × {{ asset.height }}
        </p>
        <section
          v-if="metadata?.location"
          class="mt-4"
        >
          <p
            v-if="placeName"
            class="mb-2 text-content-muted"
          >
            {{ placeName }}
          </p>
          <h2 class="mb-2 font-medium">
            {{ t('maps.nearbyPhotos') }}
          </h2>
          <MapClusterLayer
            compact
            :center="metadata.location"
            scope="folder"
            :scope-id="asset.folder_id"
            :region-ids="maps.availableRegionIds"
            @asset-click="emit('open-asset', $event)"
          />
        </section>
      </aside>
    </div>

    <div
      v-if="neighbors.length > 0"
      class="flex flex-none gap-1.5 overflow-x-auto border-t border-[#1c1c1c] px-4 py-2.5"
    >
      <button
        v-for="n in neighbors"
        :key="n.id"
        type="button"
        class="h-[52px] w-[52px] shrink-0 overflow-hidden rounded-[5px]"
        :class="n.id === asset.id ? 'opacity-100 ring-2 ring-accent' : 'opacity-60 hover:opacity-100'"
        @click="stepTo(n)"
      >
        <img
          v-if="n.content_hash"
          :src="mediaThumbSrc(n.content_hash)"
          :alt="n.filename"
          class="h-full w-full object-cover"
        >
      </button>
    </div>

    <DeleteDialog
      v-model:open="deleteDialogOpen"
      :title="t('librarySelectionActions.deleteDialogTitle', { n: 1 })"
      @choose="confirmDelete"
    />
    <AlbumPickerDialog
      v-model:open="albumDialogOpen"
      :assets="[asset]"
    />
    <RenameFormulaDialog
      v-model:open="renameDialogOpen"
      :assets="[asset]"
      :subtitle="renameSubtitle"
    />
  </div>
</template>
