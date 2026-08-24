<script setup lang="ts">
// Fase 11 Task 7 (§13, "Modifica in blocco" — definizione canonica) —
// riscrittura, non affiancamento (PROSEGUI.md, stessa Ruling già applicata
// a TimelineView nel Task 4): la vista precedente (selettore di posizione,
// copia posizione, importazione GPX) non corrisponde a nessuna schermata
// documentata in questa pagina — verificato con una ricerca sull'intero
// documento, nessun risultato. `PlacePicker.vue`/`copyLocation`/`importGpx`
// restano intatti: appartengono al dialog "Imposta posizione" (§28,
// Lightbox, Task 8), solo scollegati da qui, non eliminati.
//
// Otto sezioni nell'ordine esatto del documento (§13.2): Valutazione,
// Pick/Scarta, Preferiti, Album, Tag, Titolo, Rinomina file, Sposta in
// cartella. "Applica" scrive in un colpo solo i campi 1/2/3/6/8 (§13.3
// punto 9); Album/Tag/Rinomina agiscono subito, fuori da "Applica" — i
// loro dialog sono già costruiti (AlbumPickerDialog Task 7 2/N,
// TagPickerDialog/RenameFormulaDialog qui).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { moveAssetsBatch } from '@/api/assets'
import { fetchFlags, setFlags, unvotedFlags, type Pick as PickValue } from '@/api/culling'
import { thumbSrc as mediaThumbSrc } from '@/api/media'
import { applyMetadataBatch } from '@/api/metadata'
import type { TimelineAsset } from '@/api/timeline'
import AlbumPickerDialog from '@/components/AlbumPickerDialog.vue'
import RenameFormulaDialog from '@/components/RenameFormulaDialog.vue'
import TagPickerDialog from '@/components/TagPickerDialog.vue'
import SegmentedControl, { type SegmentedOption } from '@/components/ui/SegmentedControl.vue'
import { useMapsStore } from '@/stores/maps'
import { useSelectionStore } from '@/stores/selection'
import { useShellStore } from '@/stores/shell'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const maps = useMapsStore()
const selection = useSelectionStore()
const shell = useShellStore()
const toast = useToastStore()

const requestedIds = typeof route.query.ids === 'string' && route.query.ids.length > 0
  ? route.query.ids.split(',')
  : []

const assets = ref<TimelineAsset[]>([])
const loaded = ref(false)
const applying = ref(false)

// "Non modificare" è sempre l'opzione iniziale a ogni ingresso (§13.3,
// "Stati per ogni controllo") — nessuna lettura dei flag attuali delle
// foto selezionate, a differenza del cuoricino singolo: qui la bozza
// parte sempre azzerata.
const rating = ref(0)
const pickChoice = ref<'unchanged' | PickValue>('unchanged')
const favoriteChoice = ref<'unchanged' | 'add' | 'remove'>('unchanged')
const titleValue = ref('')
const folderId = ref('')

const albumDialogOpen = ref(false)
const tagDialogOpen = ref(false)
const renameDialogOpen = ref(false)

const PREVIEW_LIMIT = 30

const pickOptions = computed<SegmentedOption[]>(() => [
  { value: 'unchanged', label: t('batchEdit.unchanged') },
  { value: 'pick', label: t('batchEdit.pickReject.pick') },
  { value: 'reject', label: t('batchEdit.pickReject.reject') },
  { value: 'none', label: t('batchEdit.pickReject.none') }
])
const favoriteOptions = computed<SegmentedOption[]>(() => [
  { value: 'unchanged', label: t('batchEdit.unchanged') },
  { value: 'add', label: t('batchEdit.favorites.add') },
  { value: 'remove', label: t('batchEdit.favorites.remove') }
])

const previewAssets = computed(() => assets.value.slice(0, PREVIEW_LIMIT))
const previewOverflow = computed(() => Math.max(0, assets.value.length - PREVIEW_LIMIT))

onMounted(async () => {
  const loadedAssets = await Promise.all(requestedIds.map((id) => maps.loadAsset(id).catch(() => null)))
  assets.value = loadedAssets.filter((asset): asset is TimelineAsset => asset !== null)
  loaded.value = true
  if (!shell.loaded) void shell.load()
})

/** Rating/Pick/Preferiti condividono lo stesso endpoint di rimpiazzo
 * completo (`AssetFlagsBody`, non una patch) — a differenza di un singolo
 * asset (`stores/favorites.ts`), qui ogni foto della selezione può avere
 * un valore corrente diverso: non esiste un corpo condiviso valido per
 * tutte, quindi si legge e riscrive **una alla volta**, come `setMany`
 * già fa per il solo cuoricino. Nessun endpoint batch "parziale" esiste
 * per questi tre campi insieme (verificato: `POST /flags/batch` è anch'esso
 * un rimpiazzo completo, scriverebbe pick/preferiti non toccati a
 * "nessuno"/falso su ogni foto — sbagliato per "lasciane uno invariato").
 */
async function applyFlags() {
  for (const asset of assets.value) {
    const current = await fetchFlags(asset.id).catch(() => unvotedFlags)
    await setFlags(asset.id, {
      rating: rating.value > 0 ? rating.value : current.rating,
      pick: pickChoice.value === 'unchanged' ? current.pick : pickChoice.value,
      color_label: current.color_label,
      favorite: favoriteChoice.value === 'unchanged' ? current.favorite : favoriteChoice.value === 'add'
    }).catch(() => undefined)
  }
}

/** §13.3 punto 9: mai disabilitato, nemmeno a bozza intatta — in quel caso
 * azzera comunque la selezione, mostra il toast e torna indietro senza
 * aver cambiato nulla. */
async function apply() {
  if (applying.value) return
  applying.value = true
  try {
    const ids = assets.value.map((asset) => asset.id)
    const touchedFlags = rating.value > 0 || pickChoice.value !== 'unchanged' || favoriteChoice.value !== 'unchanged'
    const trimmedTitle = titleValue.value.trim()
    await Promise.all([
      touchedFlags ? applyFlags() : Promise.resolve(),
      trimmedTitle ? applyMetadataBatch(ids, { title: trimmedTitle }).catch(() => undefined) : Promise.resolve(),
      folderId.value ? moveAssetsBatch(ids, folderId.value).catch(() => undefined) : Promise.resolve()
    ])
    selection.library.clear()
    toast.show(t('batchEdit.appliedToast', { n: assets.value.length }))
    await router.push('/')
  } finally {
    applying.value = false
  }
}

/** §13.3 punto 10: torna alla timeline senza applicare i campi 1-2-3-6-8 e
 * senza azzerare la selezione — ciò che Album/Tag/Rinomina hanno già
 * fatto resta comunque fatto. */
function cancel() {
  void router.push('/')
}
</script>

<template>
  <main class="mx-auto max-w-2xl p-6">
    <button
      type="button"
      class="mb-3 flex items-center gap-1 text-[13px] text-content-muted hover:text-content"
      @click="cancel"
    >
      <span aria-hidden="true">‹</span>
      {{ t('batchEdit.cancel') }}
    </button>

    <div
      v-if="loaded && requestedIds.length === 0"
      class="flex flex-col items-center gap-1 py-16 text-center"
    >
      <p class="text-sm font-semibold">
        {{ t('batchEdit.emptyTitle') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('batchEdit.emptySubtitle') }}
      </p>
    </div>

    <template v-else-if="loaded">
      <h1 class="text-lg font-bold">
        {{ t('batchEdit.title') }}
      </h1>
      <p class="mt-1 text-sm text-content-muted">
        {{ t('batchEdit.subtitle', { n: assets.length }, { plural: assets.length }) }}
      </p>

      <div
        v-if="assets.length > 0"
        class="mt-4 flex gap-1.5 overflow-x-auto pb-1"
      >
        <span
          v-for="asset in previewAssets"
          :key="asset.id"
          class="h-[52px] w-[52px] shrink-0 overflow-hidden rounded-[6px] bg-border"
        >
          <img
            v-if="asset.content_hash"
            :src="mediaThumbSrc(asset.content_hash)"
            :alt="asset.filename"
            class="h-full w-full object-cover"
          >
        </span>
        <span
          v-if="previewOverflow > 0"
          class="flex h-[52px] w-[52px] shrink-0 items-center justify-center rounded-[6px] bg-chip-bg text-[11px] font-bold"
        >
          +{{ previewOverflow }}
        </span>
      </div>

      <section class="mt-6 space-y-6">
        <div>
          <p class="text-[13px] font-semibold">
            {{ t('batchEdit.rating.label') }}
          </p>
          <p class="mb-1.5 text-[12px] text-content-muted">
            {{ t('batchEdit.rating.hint') }}
          </p>
          <div
            role="radiogroup"
            :aria-label="t('batchEdit.rating.label')"
            class="flex items-center gap-1"
          >
            <button
              v-for="n in 5"
              :key="n"
              type="button"
              role="radio"
              tabindex="0"
              :aria-checked="rating === n"
              :aria-label="t('batchEdit.rating.star', { n })"
              class="text-xl leading-none"
              :class="rating >= n ? 'text-accent' : 'text-content-muted'"
              @click="rating = rating === n ? 0 : n"
            >
              ★
            </button>
          </div>
        </div>

        <div>
          <p class="text-[13px] font-semibold">
            {{ t('batchEdit.pickReject.label') }}
          </p>
          <p class="mb-1.5 text-[12px] text-content-muted">
            {{ t('batchEdit.pickReject.hint') }}
          </p>
          <SegmentedControl
            v-model="pickChoice"
            :options="pickOptions"
            :aria-label="t('batchEdit.pickReject.label')"
          />
        </div>

        <div>
          <p class="text-[13px] font-semibold">
            {{ t('batchEdit.favorites.label') }}
          </p>
          <SegmentedControl
            v-model="favoriteChoice"
            :options="favoriteOptions"
            :aria-label="t('batchEdit.favorites.label')"
          />
        </div>

        <div>
          <p class="text-[13px] font-semibold">
            {{ t('batchEdit.album.label') }}
          </p>
          <p class="mb-1.5 text-[12px] text-content-muted">
            {{ t('batchEdit.album.hint') }}
          </p>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] hover:bg-border/20"
            @click="albumDialogOpen = true"
          >
            {{ t('batchEdit.album.button') }}
          </button>
        </div>

        <div>
          <p class="text-[13px] font-semibold">
            {{ t('batchEdit.tag.label') }}
          </p>
          <p class="mb-1.5 text-[12px] text-content-muted">
            {{ t('batchEdit.tag.hint') }}
          </p>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] hover:bg-border/20"
            @click="tagDialogOpen = true"
          >
            {{ t('batchEdit.tag.button') }}
          </button>
        </div>

        <div>
          <label class="block text-[13px] font-semibold">
            {{ t('batchEdit.titleField.label') }}
          </label>
          <p class="mb-1.5 text-[12px] text-content-muted">
            {{ t('batchEdit.titleField.hint') }}
          </p>
          <input
            v-model="titleValue"
            type="text"
            class="w-full max-w-[320px] rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
            :placeholder="t('batchEdit.unchanged')"
          >
        </div>

        <div>
          <p class="text-[13px] font-semibold">
            {{ t('batchEdit.rename.label') }}
          </p>
          <p class="mb-1.5 text-[12px] text-content-muted">
            {{ t('batchEdit.rename.hint') }}
          </p>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] hover:bg-border/20"
            @click="renameDialogOpen = true"
          >
            {{ t('batchEdit.rename.button') }}
          </button>
        </div>

        <div>
          <label class="block text-[13px] font-semibold">
            {{ t('batchEdit.folder.label') }}
          </label>
          <select
            v-model="folderId"
            :aria-label="t('batchEdit.folder.ariaLabel')"
            class="w-full max-w-[260px] rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
          >
            <option value="">
              {{ t('batchEdit.unchanged') }}
            </option>
            <option
              v-for="folder in shell.folders"
              :key="folder.id"
              :value="folder.id"
            >
              {{ folder.name }}
            </option>
          </select>
        </div>
      </section>

      <div class="mt-8 flex gap-2 border-t border-border pt-6">
        <button
          type="button"
          class="rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-accent-text disabled:opacity-60"
          :disabled="applying"
          @click="apply"
        >
          {{ t('batchEdit.apply', { n: assets.length }) }}
        </button>
        <button
          type="button"
          class="rounded-lg px-4 py-2 text-sm font-medium text-content-muted hover:bg-border/40"
          @click="cancel"
        >
          {{ t('batchEdit.cancel') }}
        </button>
      </div>
    </template>

    <AlbumPickerDialog
      v-model:open="albumDialogOpen"
      :assets="assets"
    />
    <TagPickerDialog
      v-model:open="tagDialogOpen"
      :assets="assets"
    />
    <RenameFormulaDialog
      v-model:open="renameDialogOpen"
      :assets="assets"
    />
  </main>
</template>
