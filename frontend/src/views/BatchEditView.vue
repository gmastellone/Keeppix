<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import { applyMetadataBatch } from '@/api/metadata'

const { t } = useI18n()
const route = useRoute()

const assetIds = ref<string[]>(
  typeof route.query.ids === 'string' ? route.query.ids.split(',') : []
)
const field = ref('')
const value = ref('')
const done = ref(false)

async function submit() {
  if (!field.value.trim() || assetIds.value.length === 0) return
  await applyMetadataBatch(assetIds.value, { [field.value.trim()]: value.value || null })
  done.value = true
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
      {{ t('batchEdit.title') }}
    </h1>
    <p class="mt-2 text-sm text-content-muted">
      {{ t('batchEdit.count', { count: assetIds.length }) }}
    </p>
    <form
      v-if="!done"
      class="mt-4 space-y-3"
      @submit.prevent="submit"
    >
      <input
        v-model="field"
        class="w-full rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
        :placeholder="t('batchEdit.fieldPlaceholder')"
      >
      <input
        v-model="value"
        class="w-full rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
        :placeholder="t('batchEdit.valuePlaceholder')"
      >
      <button
        class="rounded-lg bg-accent px-4 py-2 text-sm text-white"
        type="submit"
      >
        {{ t('batchEdit.apply') }}
      </button>
    </form>
    <p
      v-else
      class="mt-6 text-content-muted"
    >
      {{ t('batchEdit.done') }}
    </p>
  </main>
</template>
