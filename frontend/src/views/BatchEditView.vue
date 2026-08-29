<script setup lang="ts">
// The previous version of this view (a location picker, copy location,
// GPX import) did not match batch editing's real purpose:
// `PlacePicker.vue`/`copyLocation`/`importGpx` remain intact — they
// belong to the "Set location" dialog (Lightbox), just unlinked from
// here, not deleted.
//
// Eight sections in a fixed order: Rating, Pick/Reject, Favorites, Album,
// Tag, Title, Rename file, Move to folder. "Apply" writes fields
// 1/2/3/6/8 in one shot; Album/Tag/Rename act immediately, outside of
// "Apply" — their dialogs are already built (AlbumPickerDialog,
// TagPickerDialog/RenameFormulaDialog here).
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

// "Unchanged" is always the initial option on every entry — no reading
// of the selected photos' current flags, unlike the single-photo
// favorite toggle: here the draft always starts blank.
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

/** Rating/Pick/Favorite share the same full-replacement endpoint
 * (`AssetFlagsBody`, not a patch) — unlike a single asset
 * (`stores/favorites.ts`), here each photo in the selection can have a
 * different current value: there is no shared body valid for all of
 * them, so each one is read and rewritten **one at a time**, the same
 * way `setMany` already does for the favorite toggle alone. No "partial"
 * batch endpoint exists for these three fields together (verified:
 * `POST /flags/batch` is also a full replacement, it would write
 * untouched pick/favorite fields to "none"/false on every photo — wrong
 * for "leave this one unchanged").
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

/** Never disabled, even with an untouched draft — in that case it still
 * clears the selection, shows the toast, and navigates back without
 * having changed anything. */
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

/** Returns to the timeline without applying fields 1-2-3-6-8 and without
 * clearing the selection — whatever Album/Tag/Rename already did stays
 * done. */
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
