<script setup lang="ts">
// The selection bar's five action buttons — shared by Photos/Timeline and
// Favorites, therefore extracted as its own component instead of
// duplicated, with Favorites already known as the second consumer at the
// time of writing (same principle already followed for
// `nav/routeTitles.ts` and `composables/useIsMobile.ts`).
//
// "Share" opens `ShareSelectionDialog.vue` — see there for why no
// "selection" `object_type` is needed in the backend (it never existed,
// verified in `crates/keeppix-db/src/share_links.rs`/`permissions.rs`: an
// auto-generated album stands in for it, with permissions and public
// links already fully built).
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { deleteAssetsBatch, type DiskAction } from '@/api/culling'
import type { TimelineAsset } from '@/api/timeline'
import { useFavoritesStore } from '@/stores/favorites'
import { useSelectionStore } from '@/stores/selection'
import { useToastStore } from '@/stores/toast'

import AlbumPickerDialog from './AlbumPickerDialog.vue'
import ShareSelectionDialog from './ShareSelectionDialog.vue'
import DeleteDialog, { type DeleteChoice } from './ui/DeleteDialog.vue'
import Tooltip from './ui/Tooltip.vue'

const props = defineProps<{ assets: TimelineAsset[] }>()

const { t } = useI18n()
const router = useRouter()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const toast = useToastStore()

const albumOpen = ref(false)
const shareOpen = ref(false)
const deleteOpen = ref(false)

// "If all the selected photos are already favorites, unfavorite all of
// them; otherwise favorite all of them" — the group's direction depends on
// the state of the whole selection, not a single photo.
const allFavorite = computed(
  () => props.assets.length > 0 && props.assets.every((asset) => favorites.isFavorite(asset))
)

function toggleFavorites() {
  void favorites.setMany(props.assets, !allFavorite.value)
}

function editSelection() {
  const ids = props.assets.map((asset) => asset.id).join(',')
  void router.push({ path: '/batch-edit', query: { ids } })
}

const DISK_ACTION: Record<DeleteChoice, DiskAction> = {
  index: 'kept',
  trash: 'moved_to_trash',
  disk: 'purged'
}

/**
 * The prototype's `pick='reject'` vote before deletion is just prototype
 * bookkeeping (a client-side state that survives in the demo after
 * removal from the visible list) — here the asset stops existing in the
 * index (or goes to trash/disk), so there's no vote to preserve.
 *
 * A single call to `deleteAssetsBatch`, not a loop over `deleteAsset`: for
 * `purged` the server checks authorization **for the whole batch before
 * touching a single file** (`routes::trash::batch_delete`) — a per-asset
 * loop would lose that guarantee, potentially deleting some files for
 * real before hitting a 403 on an unauthorized one. `purged` is the app's
 * only destructive, irreversible action: not the place for an
 * optimization that can wait.
 */
async function confirmDelete(choice: DeleteChoice) {
  const diskAction = DISK_ACTION[choice]
  const ids = props.assets.map((asset) => asset.id)
  try {
    const outcome = await deleteAssetsBatch(ids, diskAction)
    selection.library.clear()
    const okCount = outcome.succeeded.length
    const failedCount = outcome.failed.length
    if (failedCount === 0) {
      toast.show(t('librarySelectionActions.deleted', { n: okCount }, { plural: okCount }))
    } else if (okCount > 0) {
      toast.showPartial(okCount, failedCount)
    } else {
      toast.showError(t('librarySelectionActions.deleteError'))
    }
  } catch {
    // `purged` rejected for the whole batch (all-or-nothing authorization):
    // no file was touched, same message as a total failure.
    selection.library.clear()
    toast.showError(t('librarySelectionActions.deleteError'))
  }
}
</script>

<template>
  <Tooltip :label="t('librarySelectionActions.favoritesTip')">
    <button
      type="button"
      :aria-label="t('librarySelectionActions.favoritesLabel')"
      class="flex h-8 w-8 items-center justify-center rounded-lg text-content-muted hover:bg-border/40"
      @click="toggleFavorites"
    >
      <svg
        viewBox="0 0 24 24"
        width="15"
        height="15"
        fill="none"
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
  </Tooltip>
  <Tooltip :label="t('librarySelectionActions.albumTip')">
    <button
      type="button"
      :aria-label="t('librarySelectionActions.albumLabel')"
      class="flex h-8 w-8 items-center justify-center rounded-lg text-content-muted hover:bg-border/40"
      @click="albumOpen = true"
    >
      <svg
        viewBox="0 0 24 24"
        width="15"
        height="15"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <rect
          x="3"
          y="5"
          width="18"
          height="14"
          rx="2"
        />
        <path d="M3 15l5-5 4 4 3-3 6 6" />
      </svg>
    </button>
  </Tooltip>
  <Tooltip :label="t('librarySelectionActions.shareTip')">
    <button
      type="button"
      :aria-label="t('librarySelectionActions.shareLabel')"
      class="flex h-8 w-8 items-center justify-center rounded-lg text-content-muted hover:bg-border/40"
      @click="shareOpen = true"
    >
      <svg
        viewBox="0 0 24 24"
        width="15"
        height="15"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <circle
          cx="18"
          cy="5"
          r="3"
        />
        <circle
          cx="6"
          cy="12"
          r="3"
        />
        <circle
          cx="18"
          cy="19"
          r="3"
        />
        <path d="M8.6 10.5l6.8-3.9M8.6 13.5l6.8 3.9" />
      </svg>
    </button>
  </Tooltip>
  <Tooltip :label="t('librarySelectionActions.editTip')">
    <button
      type="button"
      :aria-label="t('librarySelectionActions.editLabel')"
      class="flex h-8 w-8 items-center justify-center rounded-lg border border-border text-content
             hover:bg-border/40"
      @click="editSelection"
    >
      <svg
        viewBox="0 0 24 24"
        width="14"
        height="14"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M12 20h9" />
        <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
      </svg>
    </button>
  </Tooltip>
  <Tooltip :label="t('librarySelectionActions.deleteTip')">
    <button
      type="button"
      :aria-label="t('librarySelectionActions.deleteLabel')"
      class="flex h-8 w-8 items-center justify-center rounded-lg border border-danger text-danger
             hover:bg-danger/10"
      @click="deleteOpen = true"
    >
      <svg
        viewBox="0 0 24 24"
        width="15"
        height="15"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M4 7h16" />
        <path d="M9 7V4h6v3" />
        <path d="M6 7l1 13h10l1-13" />
      </svg>
    </button>
  </Tooltip>

  <AlbumPickerDialog
    v-model:open="albumOpen"
    :assets="assets"
  />
  <ShareSelectionDialog
    v-model:open="shareOpen"
    :asset-ids="assets.map((asset) => asset.id)"
  />
  <DeleteDialog
    v-model:open="deleteOpen"
    :title="t('librarySelectionActions.deleteDialogTitle', { n: assets.length })"
    @choose="confirmDelete"
  />
</template>
