<script setup lang="ts">
// This is a pure browse/navigate grid: no rename, no delete, no control
// beyond "Create album" and clicking a card.
//
// Gradient cover (`albumCoverGradient`) instead of any hand-picked
// tint/hue: no route ever writes `cover_tint`/`monochrome` (see the
// comment in `api/albums.ts`), so the cover is always deterministic on
// the album id rather than a stored value.
//
// `<N> photos · <range>` can't read a stored range field (never existed
// on the backend): N and the range come from `fetchAlbumAssets(id)` for
// each album (the same N+1 pattern already used for folders/shares
// elsewhere — few albums per instance, acceptable) plus `albumMonthRange`
// (`@/albums/range`).
//
// "Create album" leads to the dedicated creation page (`AlbumCreateView.vue`)
// rather than a name-only dialog.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { albumMonthRange } from '@/albums/range'
import { fetchAlbumAssets, fetchAlbums, type Album, type AlbumAsset } from '@/api/albums'
import { ApiProblem, isUnauthenticated } from '@/api/client'
import { classifyError } from '@/errors/classify'
import { albumCoverGradient } from '@/design/albumCover'
import { useSessionStore } from '@/stores/session'

import ErrorState from '@/components/ui/ErrorState.vue'

const { t, locale } = useI18n()
const router = useRouter()
const session = useSessionStore()

const albums = ref<Album[]>([])
const members = ref<Record<string, AlbumAsset[]>>({})
const loaded = ref(false)
const loadError = ref<unknown>(null)

const errorNature = computed(() => (loadError.value ? classifyError(loadError.value) : null))
const errorDetail = computed(() =>
  loadError.value instanceof ApiProblem ? `${loadError.value.type} · ${loadError.value.status}` : undefined
)

async function load() {
  loadError.value = null
  loaded.value = false
  try {
    const list = await fetchAlbums()
    albums.value = list
    const pairs = await Promise.all(
      list.map(async (album) => [album.id, await fetchAlbumAssets(album.id).catch(() => [])] as const)
    )
    members.value = Object.fromEntries(pairs)
    loaded.value = true
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = error
  }
}

onMounted(load)

function cardSubtitle(album: Album): string {
  const assets = members.value[album.id] ?? []
  const range = albumMonthRange(assets, locale.value) ?? t(album.rule ? 'albums.noMatch' : 'albums.noPhotosYet')
  return t('albums.cardSubtitle', { count: assets.length, range })
}

function openAlbum(id: string) {
  void router.push(`/albums/${id}`)
}

function openCreate() {
  void router.push('/albums/new')
}
</script>

<template>
  <main class="flex h-full flex-col">
    <ErrorState
      v-if="errorNature"
      :nature="errorNature"
      :technical-detail="errorDetail"
      @retry="load"
    />
    <template v-else>
      <div class="flex items-center justify-between border-b border-border px-4 py-3">
        <div>
          <p class="text-[15px] font-bold">
            {{ t('albums.title') }}
          </p>
          <p class="text-sm text-content-muted">
            {{ t('albums.subtitle') }}
          </p>
        </div>
        <button
          type="button"
          class="flex items-center gap-1.5 rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-white"
          @click="openCreate"
        >
          {{ t('albums.createButton') }}
        </button>
      </div>

      <div
        v-if="loaded && albums.length === 0"
        class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
      >
        <p class="text-sm font-semibold">
          {{ t('albums.emptyTitle') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('albums.emptySubtitle') }}
        </p>
      </div>

      <div
        v-else
        class="grid gap-4 p-4"
        style="grid-template-columns: repeat(auto-fill, minmax(190px, 1fr))"
      >
        <div
          v-for="album in albums"
          :key="album.id"
          role="button"
          tabindex="0"
          class="cursor-pointer overflow-hidden rounded-xl border border-border bg-surface-elevated"
          @click="openAlbum(album.id)"
          @keydown.enter="openAlbum(album.id)"
          @keydown.space.prevent="openAlbum(album.id)"
        >
          <div
            class="relative h-[120px]"
            :style="{ background: albumCoverGradient(album.id) }"
          >
            <span
              v-if="album.is_shared"
              class="absolute top-2 right-2 flex items-center gap-1 rounded-full bg-black/55 px-1.5 py-0.5 text-[10px] font-bold text-white"
            >
              {{ t('albums.sharedBadge') }}
            </span>
            <span
              v-if="album.rule"
              class="absolute top-2 left-2 flex items-center gap-1 rounded-full bg-black/55 px-1.5 py-0.5 text-[10px] font-bold text-white"
              :title="t('albums.dynamicBadgeTooltip')"
            >
              {{ t('albums.dynamicBadge') }}
            </span>
          </div>
          <div class="p-2.5">
            <p class="truncate text-[13.5px] font-bold">
              {{ album.name }}
            </p>
            <p class="mt-0.5 truncate text-[11.5px] text-content-muted">
              {{ cardSubtitle(album) }}
            </p>
          </div>
        </div>
      </div>
    </template>
  </main>
</template>
