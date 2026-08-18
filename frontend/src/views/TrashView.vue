<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { isUnauthenticated } from '@/api/client'
import {
  emptyTrash,
  fetchTrash,
  restoreAsset,
  type TrashedAsset
} from '@/api/trash'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()

const items = ref<TrashedAsset[]>([])
const loadError = ref(false)

onMounted(() => {
  void load()
})

async function load() {
  loadError.value = false
  try {
    const page = await fetchTrash()
    items.value = page.items
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = true
  }
}

async function restore(id: string) {
  await restoreAsset(id)
  await load()
}

async function empty() {
  await emptyTrash()
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
      {{ t('trash.title') }}
    </h1>
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>
    <template v-else>
      <div
        v-if="items.length > 0"
        class="mt-4 flex justify-end"
      >
        <button
          class="rounded-lg bg-danger px-4 py-2 text-sm text-white"
          @click="empty"
        >
          {{ t('trash.emptyAll') }}
        </button>
      </div>
      <p
        v-if="items.length === 0"
        class="mt-6 text-content-muted"
      >
        {{ t('trash.empty') }}
      </p>
      <ul
        v-else
        class="mt-4 space-y-2"
      >
        <li
          v-for="item in items"
          :key="item.id"
          class="flex items-center justify-between rounded-lg border border-border px-4 py-3"
        >
          <div>
            <span class="font-medium">{{ item.filename }}</span>
            <span class="ml-2 text-xs text-content-muted">
              {{ t('trash.expires', { date: item.expires_at }) }}
            </span>
          </div>
          <div class="flex gap-2">
            <button
              class="text-sm underline"
              @click="restore(item.id)"
            >
              {{ t('trash.restore') }}
            </button>
          </div>
        </li>
      </ul>
    </template>
  </main>
</template>
