<script setup lang="ts">
// SP-2 (documento funzionale §12.2-§12.3): i cinque pulsanti d'azione della
// barra di selezione — condiviso da Foto/Timeline e Preferiti (§9.3: "SP-2
// completa, tutti e cinque i pulsanti"), quindi estratto come componente
// proprio invece di duplicato, con Preferiti come secondo consumatore già
// noto al momento di scriverlo (stesso principio già seguito per
// `nav/routeTitles.ts` e `composables/useIsMobile.ts`).
//
// **"Condividi" volutamente omesso** — non uno stub, un'assenza dichiarata:
// verificato leggendo per intero `crates/keeppix-db/src/share_links.rs` e
// `crates/keeppix-db/src/permissions.rs`. Un link di condivisione esiste
// solo per `object_type` `folder`/`album`/`asset` — e "asset" **conta
// sempre 1** (`item_counts`, commento del codice stesso: "un link asset
// conta sempre 1, senza query"), mai una selezione multi-foto ad hoc; le
// concessioni di permesso a persone già invitate coprono solo `folder`/
// `album` (`permissions.rs`, riga 502). Non esiste nel backend nessun
// concetto di "condividi questa selezione arbitraria di N foto", né come
// link pubblico né come concessione a una persona — lo stesso genere di
// lacuna già dichiarata altrove in questa sessione (il badge video "in
// preparazione", "Nuova cartella…") invece di un pulsante che non
// farebbe nulla di reale.
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { deleteAsset, type DiskAction } from '@/api/culling'
import type { TimelineAsset } from '@/api/timeline'
import { useFavoritesStore } from '@/stores/favorites'
import { useSelectionStore } from '@/stores/selection'
import { useToastStore } from '@/stores/toast'

import AlbumPickerDialog from './AlbumPickerDialog.vue'
import DeleteDialog, { type DeleteChoice } from './ui/DeleteDialog.vue'
import Tooltip from './ui/Tooltip.vue'

const props = defineProps<{ assets: TimelineAsset[] }>()

const { t } = useI18n()
const router = useRouter()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const toast = useToastStore()

const albumOpen = ref(false)
const deleteOpen = ref(false)

// §12.3: "se tutte le selezionate sono già preferite, le toglie tutte;
// altrimenti le mette tutte" — il verso del gruppo dipende dallo stato
// dell'intera selezione, non da una singola foto.
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
 * §12.3: "ogni foto selezionata riceve pick='reject' e la scelta di
 * smaltimento". Il voto `pick='reject'` prima della cancellazione è solo
 * contabilità del prototipo (uno stato client-side che sopravvive nella
 * demo dopo la rimozione dalla lista visibile) — qui l'asset smette di
 * esistere nell'indice (o va in cestino/su disco), quindi non c'è alcun
 * voto da preservare: stesso comportamento già in uso da
 * `CullingStore.removeMany`, che chiama `deleteAsset` da solo, senza un
 * voto separato prima.
 */
async function confirmDelete(choice: DeleteChoice) {
  const diskAction = DISK_ACTION[choice]
  const ids = props.assets.map((asset) => asset.id)
  let failed = 0
  for (const id of ids) {
    try {
      await deleteAsset(id, diskAction)
    } catch {
      failed++
    }
  }
  selection.library.clear()
  const okCount = ids.length - failed
  if (failed === 0) {
    toast.show(t('librarySelectionActions.deleted', { n: okCount }, { plural: okCount }))
  } else if (okCount > 0) {
    toast.showPartial(okCount, failed)
  } else {
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
  <DeleteDialog
    v-model:open="deleteOpen"
    :title="t('librarySelectionActions.deleteDialogTitle', { n: assets.length })"
    @choose="confirmDelete"
  />
</template>
