<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { createAlbum, deleteAlbum, fetchAlbums, type Album } from '@/api/albums'
import { isUnauthenticated } from '@/api/client'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const albums = ref<Album[]>([])
const loadError = ref(false)
const newName = ref('')

onMounted(() => {
  void load()
})

async function load() {
  loadError.value = false
  try {
    albums.value = await fetchAlbums()
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = true
  }
}

async function add() {
  if (!newName.value.trim()) return
  await createAlbum(newName.value.trim())
  newName.value = ''
  await load()
}

async function remove(id: string) {
  await deleteAlbum(id)
  await load()
}
</script>

<template>
  <main class="mx-auto max-w-3xl p-6">
    <p class="mb-4 text-sm">
      <RouterLink
        class="underline"
        to="/"
      >
        {{ t('folders.back') }}
      </RouterLink>
    </p>
    <h1 class="text-2xl font-semibold">
      {{ t('albums.title') }}
    </h1>
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>
    <template v-else>
      <form
        class="mt-4 flex gap-2"
        @submit.prevent="add"
      >
        <input
          v-model="newName"
          class="flex-1 rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
          :placeholder="t('albums.namePlaceholder')"
        >
        <button
          class="rounded-lg bg-accent px-4 py-2 text-sm text-white"
          type="submit"
        >
          {{ t('albums.create') }}
        </button>
      </form>
      <p
        v-if="albums.length === 0"
        class="mt-6 text-content-muted"
      >
        {{ t('albums.empty') }}
      </p>
      <ul
        v-else
        class="mt-4 space-y-2"
      >
        <li
          v-for="album in albums"
          :key="album.id"
          class="flex items-center justify-between rounded-lg border border-border px-4 py-3"
        >
          <div>
            <span class="font-medium">{{ album.name }}</span>
            <span class="ml-2 text-sm text-content-muted">
              {{ t('albums.assetCount', { count: album.asset_count }) }}
            </span>
          </div>
          <button
            class="text-sm text-danger underline"
            @click="remove(album.id)"
          >
            {{ t('albums.delete') }}
          </button>
        </li>
      </ul>
    </template>
  </main>
</template>
