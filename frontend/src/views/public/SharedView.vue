<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import { authenticateShare, fetchPublicShareInfo, fetchSharedContent, type SharedContent } from '@/api/shares'
import { thumbSrc } from '@/api/media'

const { t } = useI18n()
const route = useRoute()

const content = ref<SharedContent | null>(null)
const loadError = ref(false)
const needsPassword = ref(false)
const password = ref('')

onMounted(async () => {
  const token = route.params.token as string
  try {
    const info = await fetchPublicShareInfo(token)
    if (info.has_password) {
      needsPassword.value = true
      return
    }
    content.value = await fetchSharedContent(token)
  } catch {
    loadError.value = true
  }
})

async function submitPassword() {
  const token = route.params.token as string
  loadError.value = false
  try {
    await authenticateShare(token, password.value)
    content.value = await fetchSharedContent(token)
    needsPassword.value = false
  } catch {
    loadError.value = true
  }
}
</script>

<template>
  <main class="mx-auto max-w-4xl p-6">
    <h1 class="text-2xl font-semibold">
      {{ content?.object_type ?? t('shared.title') }}
    </h1>
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('shared.notFound') }}
    </p>
    <form
      v-else-if="needsPassword"
      class="mt-6 flex max-w-sm gap-2"
      @submit.prevent="submitPassword"
    >
      <input
        v-model="password"
        class="flex-1 rounded border border-border bg-surface px-2 py-1"
        type="password"
        :placeholder="t('shared.password')"
      >
      <button
        class="rounded bg-accent px-4 py-1 text-white"
        type="submit"
      >
        {{ t('shared.unlock') }}
      </button>
    </form>
    <ul
      v-else-if="content"
      class="mt-4 grid grid-cols-3 gap-2 sm:grid-cols-4"
    >
      <li
        v-for="asset in content.assets"
        :key="asset.id"
      >
        <img
          v-if="asset.content_hash"
          :src="thumbSrc(asset.content_hash)"
          :alt="asset.filename"
          class="h-32 w-full rounded object-cover"
        >
        <span
          v-else
          class="block truncate text-sm"
        >{{ asset.filename }}</span>
      </li>
    </ul>
    <p
      v-else
      class="mt-6 text-content-muted"
    >
      {{ t('common.loading') }}
    </p>
  </main>
</template>
