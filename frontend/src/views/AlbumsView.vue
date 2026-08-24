<script setup lang="ts">
// Fase 11 Task 12 (1/N) — documento funzionale §41 "Album — la griglia",
// verificato riga per riga (righe 6226-6353). Sostituisce interamente la
// vecchia vista CRUD (lista + form inline + "Elimina" per riga): il
// mockup non prevede né rinomina, né eliminazione, né alcun controllo
// oltre "Crea album" e il click sulla scheda (§3, "sono solo due i tipi
// di controllo") — la vecchia `remove()`/`deleteAlbum` sparisce di
// conseguenza, non viene commentata.
//
// Copertina a gradiente (`albumCoverGradient`, Task 12 dati) invece di
// `a.mono`/`a.hue` scritti a mano nel mockup: nessuna rotta scrive mai
// `cover_tint`/`monochrome` (vedi commento in `api/albums.ts`), quindi
// l'unica differenza dal mockup è che qui non esiste un vero album
// "Bianco e nero" — ogni copertina è comunque deterministica sull'id.
//
// `<N> foto · <intervallo>` (§2) non può leggere `a.range` (mai esistito
// sul backend): N e intervallo vengono da `fetchAlbumAssets(id)` per
// ciascun album (pattern N+1 già usato per cartelle/condivisioni ai
// Task 9/11 — pochi album per istanza, accettabile) più `albumMonthRange`
// (Task 12, `@/albums/range`).
//
// "Crea album" (§3.1) nel mockup porta a una pagina di creazione a sé
// (§43) con nome, condivisione, e un editor di filtro a 9 campi. Quella
// pagina è rimandata alla prossima unità (Task 12 2/N): qui c'è un
// dialog minimo, solo nome, che copre comunque il caso "Manuale" del
// §43 (nome pulito dagli spazi, validato con un toast se vuoto, atterra
// nel dettaglio del nuovo album) senza lasciare il pulsante inerte.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { albumMonthRange } from '@/albums/range'
import { createAlbum, fetchAlbumAssets, fetchAlbums, type Album, type AlbumAsset } from '@/api/albums'
import { ApiProblem, isUnauthenticated } from '@/api/client'
import { classifyError } from '@/errors/classify'
import { albumCoverGradient } from '@/design/albumCover'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

import Dialog from '@/components/ui/Dialog.vue'
import ErrorState from '@/components/ui/ErrorState.vue'

const { t, locale } = useI18n()
const router = useRouter()
const session = useSessionStore()
const toast = useToastStore()

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

const createOpen = ref(false)
const newName = ref('')
const nameInputEl = ref<HTMLInputElement | null>(null)
const creating = ref(false)

function openCreateDialog() {
  newName.value = ''
  createOpen.value = true
}

async function confirmCreate() {
  const name = newName.value.trim()
  if (!name) {
    toast.showError(t('albums.createDialog.error'))
    nameInputEl.value?.focus()
    return
  }
  if (creating.value) return
  creating.value = true
  try {
    const album = await createAlbum(name)
    createOpen.value = false
    await router.push(`/albums/${album.id}`)
  } catch {
    toast.showError(t('albums.createDialog.error'))
  } finally {
    creating.value = false
  }
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
          @click="openCreateDialog"
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

    <Dialog
      v-model:open="createOpen"
      :title="t('albums.createDialog.title')"
      :initial-focus="nameInputEl"
    >
      <form
        class="flex flex-col gap-3"
        @submit.prevent="confirmCreate"
      >
        <input
          ref="nameInputEl"
          v-model="newName"
          class="rounded-lg border border-border bg-surface px-3 py-2 text-sm"
          :placeholder="t('albums.createDialog.namePlaceholderHint')"
          :aria-label="t('albums.namePlaceholder')"
        >
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold"
            @click="createOpen = false"
          >
            {{ t('albums.createDialog.cancel') }}
          </button>
          <button
            type="submit"
            class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-white"
            :disabled="creating"
          >
            {{ t('albums.createDialog.confirm') }}
          </button>
        </div>
      </form>
    </Dialog>
  </main>
</template>
