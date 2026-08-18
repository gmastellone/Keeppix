<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import {
  authenticateShare,
  fetchPublicShareInfo,
  fetchSharedContent,
  uploadGuestFile,
  type SharedContent
} from '@/api/shares'
import { thumbSrc } from '@/api/media'

const { t } = useI18n()
const route = useRoute()

const content = ref<SharedContent | null>(null)
const loadError = ref(false)
const needsPassword = ref(false)
const password = ref('')
const allowUpload = ref(false)
const uploaded = ref(false)

onMounted(async () => {
  const token = route.params.token as string
  try {
    const info = await fetchPublicShareInfo(token)
    allowUpload.value = Boolean(info.allow_upload)
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

async function onFile(event: Event) {
  const token = route.params.token as string
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  uploaded.value = false
  loadError.value = false
  try {
    await uploadGuestFile(token, file.name, file)
    uploaded.value = true
  } catch {
    loadError.value = true
  }
  input.value = ''
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
    <template v-else-if="content">
      <label
        v-if="allowUpload"
        class="mt-4 block text-sm"
      >
        {{ t('shared.upload') }}
        <input
          class="mt-1 block"
          type="file"
          @change="onFile"
        >
      </label>
      <p
        v-if="uploaded"
        class="mt-2 text-sm text-content-muted"
      >
        {{ t('shared.uploaded') }}
      </p>
      <ul class="mt-4 grid grid-cols-3 gap-2 sm:grid-cols-4">
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
    </template>
    <p
      v-else
      class="mt-6 text-content-muted"
    >
      {{ t('common.loading') }}
    </p>
  </main>
</template>
