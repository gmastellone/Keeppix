<script setup lang="ts">
// Fase 11 Task 8 (2/N poi 3/N) — documento funzionale §18 ("Lightbox —
// struttura e barra superiore"), §19 ("Pannello informazioni") e §20
// ("Menu 'altre azioni' ⋯"). La 2/N ha riscritto il segnaposto precedente
// (151 righe) con barra superiore, stage con frecce, filmino e menu ⋯. La
// 3/N (questa) ha aggiunto la prima metà reale del pannello: titolo
// modificabile, valutazione a stelle, sezione SCATTO (exif completo). Le
// sezioni POSIZIONE (dialog di modifica), PERSONE, TAG, ALBUM e AZIONI
// restano le prossime unità di questo stesso Task.
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
// - Il link verso la cartella/il lotto di provenienza nella riga
//   data/ora (§19.2 riga 2) è omesso: non esiste una rotta per risolvere
//   il nome di una cartella dal solo `folder_id` (`GET /folders/{id}`
//   non esiste, solo `tree`/`{id}/children`) — costruirla per una sola
//   riga di sottotitolo non è nello scopo di questa unità.
// - Il commutatore RAW/JPEG (§19.2 riga 5) non è ancora costruito: serve
//   `GET /assets/{id}/stack` (esiste lato backend, nessun wrapper
//   frontend ancora) — prossima unità.
//
// **Corretto qui, non solo aggiunto**: il click sullo sfondo nero
// *non* deve chiudere il lightbox (§18.4, esplicito — a differenza
// dello scrim dei dialog modali, SP-5) — la versione precedente aveva
// `@click.self="emit('close')"` sul contenitore radice, un
// comportamento mai documentato per questa vista. Rimosso.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { deleteAsset, fetchFlags, setFlags, unvotedFlags } from '@/api/culling'
import type { AssetFlags, DiskAction } from '@/api/culling'
import { fetchMetadata, patchMetadata, type AssetMetadata } from '@/api/metadata'
import { originalSrc, previewSrc as mediaPreviewSrc, thumbSrc as mediaThumbSrc } from '@/api/media'
import { fetchAsset, type TimelineAsset } from '@/api/timeline'
import AlbumPickerDialog from '@/components/AlbumPickerDialog.vue'
import RatingStars from '@/components/RatingStars.vue'
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
const { t, locale } = useI18n()
const maps = useMapsStore()
const toast = useToastStore()

const info = ref(false)
const moreOpen = ref(false)
const albumDialogOpen = ref(false)
const renameDialogOpen = ref(false)
const deleteDialogOpen = ref(false)
const metadata = ref<AssetMetadata>()
/** §19.2 sezione "SCATTO": `full_exif` non arriva mai col prop `asset`
 * (le griglie che passano l'asset al lightbox usano `/timeline`/`/search`,
 * che non lo calcolano) — solo `GET /assets/{id}` lo porta. */
const detail = ref<TimelineAsset>()
const flags = ref<AssetFlags>()
const placeName = ref<string | null>(null)
const titleDraft = ref('')
let panelRequestSequence = 0

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
    if (info.value) void loadPanelData()
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

/** Un solo giro per apertura pannello/cambio foto: metadati effettivi
 * (titolo, posizione), dettaglio con `full_exif` (§19.2 "SCATTO", assente
 * dal prop `asset`) e voti (per la valutazione a stelle) — tre chiamate in
 * parallelo, ciascuna con il proprio esito indipendente: se una fallisce
 * (es. pgvector assente per i voti) le altre due restano valide. */
async function loadPanelData() {
  const sequence = ++panelRequestSequence
  const assetId = props.asset.id
  const [metadataResult, detailResult, flagsResult] = await Promise.allSettled([
    fetchMetadata(assetId),
    fetchAsset(assetId),
    fetchFlags(assetId)
  ])
  if (sequence !== panelRequestSequence || assetId !== props.asset.id) return
  metadata.value = metadataResult.status === 'fulfilled' ? metadataResult.value : undefined
  detail.value = detailResult.status === 'fulfilled' ? detailResult.value : undefined
  flags.value = flagsResult.status === 'fulfilled' ? flagsResult.value : unvotedFlags
  titleDraft.value = metadata.value?.title ?? ''
  placeName.value = null
  const location = metadata.value?.location
  if (location) {
    maps.reverseGeocode(location.lat, location.lon)
      .then((place) => {
        if (sequence === panelRequestSequence) placeName.value = place?.name ?? null
      })
      .catch(() => { /* best-effort */ })
  }
}

async function saveTitle() {
  const assetId = props.asset.id
  const trimmed = titleDraft.value.trim()
  titleDraft.value = trimmed
  try {
    await patchMetadata(assetId, { title: trimmed === '' ? null : trimmed })
    if (metadata.value && assetId === props.asset.id) {
      metadata.value.title = trimmed === '' ? null : trimmed
    }
  } catch {
    toast.showError(t('viewer.panel.titleError'))
  }
}

/** SP-9: click sulla stella *n* imposta la valutazione a *n*, riclick
 * sulla stessa stella l'azzera — `RatingStars` emette solo `rate(n)`, il
 * toggle è responsabilità del chiamante (stessa cosa già vera in
 * `CullingView.vue`, che però non lo implementa: qui sì, per rispettare
 * §19.3 alla lettera). `setFlags` sostituisce l'intero oggetto voti, quindi
 * si parte sempre da `flags.value` già caricato, mai da un valore vuoto. */
async function rate(n: number) {
  const assetId = props.asset.id
  const current = flags.value ?? unvotedFlags
  const next = current.rating === n ? 0 : n
  try {
    await setFlags(assetId, { ...current, rating: next })
    if (assetId === props.asset.id) flags.value = { ...current, rating: next }
  } catch {
    toast.showError(t('viewer.panel.ratingError'))
  }
}

onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))
watch(
  () => props.asset.id,
  () => {
    panelRequestSequence += 1
    metadata.value = undefined
    detail.value = undefined
    flags.value = undefined
    placeName.value = null
    titleDraft.value = ''
    if (info.value) void loadPanelData()
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

/** §19.2 riga 2: "{giorno} {mese} {anno}, ore {H:MM}" — il link verso la
 * cartella/il lotto di provenienza che condivide questa riga nel documento
 * resta debito dichiarato (nessuna rotta per risolvere un nome di cartella
 * dal solo `folder_id` esiste ancora: `GET /folders/{id}` non c'è, solo
 * `tree`/`{id}/children`). */
const dateTimeLabel = computed(() => {
  const iso = props.asset.taken_at_utc
  if (!iso) return ''
  const when = new Date(iso)
  const date = new Intl.DateTimeFormat(locale.value, { day: 'numeric', month: 'long', year: 'numeric' }).format(when)
  const time = new Intl.DateTimeFormat(locale.value, { hour: '2-digit', minute: '2-digit', hour12: false }).format(when)
  return t('viewer.panel.dateTime', { date, time })
})

/** §19.2 riga 8: "{diaframma} · {tempo}s · ISO {iso}" — solo le parti
 * effettivamente presenti nell'exif, unite da " · " (un file senza
 * diaframma noto non deve mostrare "f/undefined"). */
const exposureLine = computed(() => {
  const exif = detail.value?.full_exif
  if (!exif) return ''
  const parts: string[] = []
  if (exif.f_number != null) parts.push(`f/${formatFNumber(exif.f_number)}`)
  if (exif.exposure) parts.push(`${exif.exposure}s`)
  if (exif.iso != null) parts.push(`ISO ${exif.iso}`)
  return parts.join(' · ')
})

function formatFNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1)
}

const cameraLine = computed(() => {
  const exif = detail.value?.full_exif
  if (!exif) return ''
  return [exif.camera_make, exif.camera_model].filter(Boolean).join(' ')
})

const dimensionsLine = computed(() => {
  if (!props.asset.width || !props.asset.height) return ''
  return `${props.asset.width}×${props.asset.height}`
})
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
          @click="info = !info; if (info) void loadPanelData()"
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
        <h3 class="truncate text-[14.5px] font-bold">
          {{ asset.filename }}
        </h3>
        <p
          v-if="dateTimeLabel"
          class="mt-1 text-xs text-[#8f8f92]"
        >
          {{ dateTimeLabel }}
        </p>

        <div class="mt-3.5">
          <label
            for="lbTitleInput"
            class="mb-1 block text-xs text-[#d8d8d8]"
          >
            {{ t('viewer.panel.titleLabel') }}
            <span class="font-normal text-[#7a7a7d]">{{ t('viewer.panel.titleOptional') }}</span>
          </label>
          <input
            id="lbTitleInput"
            v-model="titleDraft"
            type="text"
            :placeholder="t('viewer.panel.titlePlaceholder')"
            class="w-full rounded-md border border-[#262626] bg-[#161616] px-2.5 py-2 text-sm
                   text-[#f0f0f0] placeholder:text-[#7a7a7d] focus-visible:outline-2
                   focus-visible:outline-offset-2 focus-visible:outline-accent"
            @change="saveTitle"
          >
        </div>

        <RatingStars
          class="mt-3"
          :rating="flags?.rating ?? null"
          @rate="rate"
        />

        <section
          v-if="cameraLine || detail?.full_exif?.lens || exposureLine || dimensionsLine"
          class="mt-4"
        >
          <h2 class="mb-2 text-[10.5px] tracking-[.06em] text-[#7a7a7d] uppercase">
            {{ t('viewer.panel.shot') }}
          </h2>
          <dl class="space-y-1 text-[13px]">
            <div
              v-if="cameraLine"
              class="flex justify-between gap-2"
            >
              <dt class="text-[#8f8f92]">
                {{ t('viewer.panel.camera') }}
              </dt>
              <dd class="truncate text-right">
                {{ cameraLine }}
              </dd>
            </div>
            <div
              v-if="detail?.full_exif?.lens"
              class="flex justify-between gap-2"
            >
              <dt class="text-[#8f8f92]">
                {{ t('viewer.panel.lens') }}
              </dt>
              <dd class="truncate text-right">
                {{ detail.full_exif.lens }}
              </dd>
            </div>
            <div
              v-if="exposureLine"
              class="flex justify-between gap-2"
            >
              <dt class="text-[#8f8f92]">
                {{ t('viewer.panel.exposure') }}
              </dt>
              <dd class="truncate text-right">
                {{ exposureLine }}
              </dd>
            </div>
            <div
              v-if="dimensionsLine"
              class="flex justify-between gap-2"
            >
              <dt class="text-[#8f8f92]">
                {{ t('viewer.panel.dimensions') }}
              </dt>
              <dd class="truncate text-right">
                {{ dimensionsLine }}
              </dd>
            </div>
          </dl>
        </section>

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
