<script setup lang="ts">
// "Add to album" dialog: per-album group toggle — if *all* selected
// photos are already in the album it removes it from all of them,
// otherwise it adds it to all of them. "The effect is immediate: there's
// no 'Undo', every click is already applied."
//
// The prototype's dynamic albums ("N dynamic albums not shown here") don't
// exist in this backend — verified by reading
// crates/keeppix-api/src/routes/albums.rs in full: no `kind`/`is_dynamic`
// field anywhere, and the plan itself confirms it ("Dynamic albums don't
// exist"). Every album from `fetchAlbums()` is therefore "manual" by
// construction: nothing to filter, no note to show.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { addAssets, fetchAlbumAssets, fetchAlbums, removeAsset, type Album } from '@/api/albums'
import type { TimelineAsset } from '@/api/timeline'
import { useToastStore } from '@/stores/toast'

import Dialog from './ui/Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ assets: TimelineAsset[] }>()

const { t } = useI18n()
const toast = useToastStore()

const albums = ref<Album[]>([])
/** Album id → set of photo ids already members, to decide the direction of
 * the group toggle. No dedicated "membership across multiple albums"
 * endpoint: it's derived from `GET /albums/{id}/assets`
 * (`fetchAlbumAssets`), one call per album when the dialog opens — the
 * number of albums is typically small, not worth a dedicated endpoint
 * just for this dialog. Real bug fixed here: it used to call
 * `fetchAlbum(id).assets`, a field `GET /albums/{id}` never actually
 * returned — the membership shown here was always empty in production,
 * never clearly wrong only because the tests mocked `fetchAlbum` with a
 * synthetic shape that included `assets`. */
const membership = ref<Record<string, Set<string>>>({})
const pending = ref<Set<string>>(new Set())

async function load() {
  const list = await fetchAlbums().catch(() => [])
  albums.value = list
  const entries = await Promise.all(
    list.map(async (album) => {
      const members = await fetchAlbumAssets(album.id).catch(() => [])
      return [album.id, new Set(members.map((a) => a.id))] as const
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
