<script setup lang="ts">
// SP-2 (documento funzionale §12.3, "Dialog 'Aggiungi ad album'"):
// interruttore di gruppo per album — se *tutte* le foto selezionate sono
// già nell'album lo toglie da tutte, altrimenti le aggiunge tutte.
// "L'effetto è immediato: non c'è 'Annulla', ogni click è già applicato."
//
// Gli album dinamici del prototipo ("N album dinamici non mostrati qui")
// non esistono in questo backend — verificato leggendo
// crates/keeppix-api/src/routes/albums.rs per intero: nessun campo
// `kind`/`is_dynamic` da nessuna parte, e il piano stesso lo conferma
// ("Gli album dinamici non esistono", decisione del 20 agosto, Task 12).
// Ogni album da `fetchAlbums()` è quindi "manuale" per costruzione: niente
// da filtrare, niente nota da mostrare.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { addAssets, fetchAlbum, fetchAlbums, removeAsset, type Album } from '@/api/albums'
import type { TimelineAsset } from '@/api/timeline'
import { useToastStore } from '@/stores/toast'

import Dialog from './ui/Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ assets: TimelineAsset[] }>()

const { t } = useI18n()
const toast = useToastStore()

const albums = ref<Album[]>([])
/** Id album → insieme degli id foto già membri, per decidere il verso del
 * toggle di gruppo. Niente endpoint di "appartenenza" dedicato: si deduce
 * da `AlbumDetail.assets`, un `fetchAlbum` per album al momento
 * dell'apertura — il numero di album è tipicamente piccolo, non merita
 * un endpoint apposito solo per questo dialog. */
const membership = ref<Record<string, Set<string>>>({})
const pending = ref<Set<string>>(new Set())

async function load() {
  const list = await fetchAlbums().catch(() => [])
  albums.value = list
  const entries = await Promise.all(
    list.map(async (album) => {
      const detail = await fetchAlbum(album.id).catch(() => null)
      return [album.id, new Set((detail?.assets ?? []).map((a) => a.id))] as const
    })
  )
  membership.value = Object.fromEntries(entries)
}

watch(
  open,
  (isOpen) => {
    if (isOpen) void load()
  },
  { immediate: true }
)

function isFullyIn(albumId: string): boolean {
  const set = membership.value[albumId]
  if (!set || props.assets.length === 0) return false
  return props.assets.every((asset) => set.has(asset.id))
}

async function toggle(album: Album) {
  if (pending.value.has(album.id)) return
  pending.value = new Set(pending.value).add(album.id)
  const add = !isFullyIn(album.id)
  const ids = props.assets.map((asset) => asset.id)
  try {
    if (add) {
      await addAssets(album.id, ids)
    } else {
      for (const id of ids) await removeAsset(album.id, id)
    }
    const next = new Set(membership.value[album.id] ?? [])
    ids.forEach((id) => (add ? next.add(id) : next.delete(id)))
    membership.value = { ...membership.value, [album.id]: next }
  } catch {
    toast.showError(t('albumPicker.error'))
  } finally {
    const rest = new Set(pending.value)
    rest.delete(album.id)
    pending.value = rest
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('albumPicker.title')"
    :description="t('albumPicker.subtitle', { n: assets.length }, { plural: assets.length })"
  >
    <div class="max-h-[260px] space-y-1 overflow-y-auto">
      <button
        v-for="album in albums"
        :key="album.id"
        type="button"
        role="switch"
        :aria-checked="isFullyIn(album.id)"
        :disabled="pending.has(album.id)"
        class="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left hover:bg-border/20
               focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        @click="toggle(album)"
      >
        <span class="h-9 w-9 shrink-0 rounded-md bg-border" />
        <span class="min-w-0 flex-1 truncate text-[13px] font-medium text-content">{{ album.name }}</span>
        <span
          class="relative h-5 w-9 shrink-0 rounded-full transition-colors"
          :style="{ transitionDuration: 'var(--duration-arrow)' }"
          :class="isFullyIn(album.id) ? 'bg-accent' : 'bg-border'"
        >
          <span
            class="absolute top-0.5 h-4 w-4 rounded-full bg-white transition-[left]"
            :style="{ left: isFullyIn(album.id) ? '18px' : '2px', transitionDuration: 'var(--duration-arrow)' }"
          />
        </span>
      </button>
    </div>
    <div class="mt-4 flex justify-end">
      <button
        type="button"
        class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-text"
        @click="open = false"
      >
        {{ t('albumPicker.done') }}
      </button>
    </div>
  </Dialog>
</template>
