<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchSyncDelta, type SyncDelta } from '@/api/sync'
import { ApiProblem } from '@/api/client'

const { t } = useI18n()

const loading = ref(false)
const error = ref('')
const result = ref<SyncDelta | null>(null)

async function runProbe(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    result.value = await fetchSyncDelta(0)
  } catch (e) {
    result.value = null
    error.value = e instanceof ApiProblem ? e.title : t('sync.errors.failed')
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  void runProbe()
})
</script>

<template>
  <main class="mx-auto max-w-2xl px-4 py-8">
    <h1 class="text-2xl font-semibold tracking-tight">{{ t('sync.title') }}</h1>
    <p class="mt-2 text-sm opacity-80">{{ t('sync.subtitle') }}</p>

    <p v-if="error" class="mt-4 text-sm text-red-700" role="alert">{{ error }}</p>

    <div class="mt-6">
      <button
        type="button"
        class="border px-4 py-2 text-sm font-medium"
        :disabled="loading"
        data-testid="sync-probe"
        @click="runProbe"
      >
        {{ t('sync.probe') }}
      </button>
    </div>

    <dl v-if="result" class="mt-6 space-y-2 text-sm" data-testid="sync-result">
      <div>
        <dt class="opacity-70">{{ t('sync.cursor') }}</dt>
        <dd>{{ result.cursor }}</dd>
      </div>
      <div>
        <dt class="opacity-70">{{ t('sync.upserted') }}</dt>
        <dd>{{ result.upserted.length }}</dd>
      </div>
      <div>
        <dt class="opacity-70">{{ t('sync.deleted') }}</dt>
        <dd>{{ result.deleted.length }}</dd>
      </div>
      <div>
        <dt class="opacity-70">{{ t('sync.hasMore') }}</dt>
        <dd>{{ result.has_more ? t('sync.yes') : t('sync.no') }}</dd>
      </div>
    </dl>
  </main>
</template>
