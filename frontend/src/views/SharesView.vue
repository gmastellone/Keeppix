<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchShareLinks, revokeShareLink, type ShareLink } from '@/api/shares'

const { t } = useI18n()

const links = ref<ShareLink[]>([])
const loadError = ref(false)

onMounted(() => {
  void load()
})

async function load() {
  loadError.value = false
  try {
    links.value = await fetchShareLinks()
  } catch {
    loadError.value = true
  }
}

async function remove(id: string) {
  await revokeShareLink(id)
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
      {{ t('shares.title') }}
    </h1>
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>
    <p
      v-else-if="links.length === 0"
      class="mt-6 text-content-muted"
    >
      {{ t('shares.empty') }}
    </p>
    <ul
      v-else
      class="mt-4 space-y-2"
    >
      <li
        v-for="link in links"
        :key="link.id"
        class="flex items-center justify-between rounded-lg border border-border px-4 py-3"
      >
        <div class="min-w-0 flex-1">
          <p class="truncate text-sm">
            {{ link.object_type }} · {{ link.object_id }}
          </p>
          <p class="text-xs text-content-muted">
            {{ t('shares.views', { count: link.view_count }) }}
          </p>
        </div>
        <button
          class="ml-4 text-sm text-danger underline"
          @click="remove(link.id)"
        >
          {{ t('shares.revoke') }}
        </button>
      </li>
    </ul>
  </main>
</template>
