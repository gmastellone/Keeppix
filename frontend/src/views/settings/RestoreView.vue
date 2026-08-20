<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { inspectBackup, restoreBackup } from '@/api/backup'
import { ApiProblem } from '@/api/client'

const { t } = useI18n()

const path = ref('')
const passphrase = ref('')
const includeDatabase = ref(true)
const includeConfig = ref(true)
const includeMaps = ref(true)
const preview = ref<Record<string, unknown> | null>(null)
const rejectReason = ref('')
const error = ref('')
const loading = ref(false)
const done = ref('')

async function inspect(): Promise<void> {
  loading.value = true
  error.value = ''
  rejectReason.value = ''
  preview.value = null
  try {
    const result = await inspectBackup(path.value.trim(), passphrase.value)
    if (result.rejected) {
      rejectReason.value = result.reject_reason || t('restore.tooNew')
      return
    }
    preview.value = result.manifest
  } catch (e) {
    error.value = e instanceof ApiProblem ? e.title : t('restore.errors.inspectFailed')
  } finally {
    loading.value = false
  }
}

async function dryRun(): Promise<void> {
  loading.value = true
  error.value = ''
  done.value = ''
  try {
    preview.value = await restoreBackup({
      path: path.value.trim(),
      passphrase: passphrase.value,
      components: {
        database: includeDatabase.value,
        config: includeConfig.value,
        maps: includeMaps.value
      },
      dry_run: true
    })
  } catch (e) {
    error.value = e instanceof ApiProblem ? e.title : t('restore.errors.restoreFailed')
  } finally {
    loading.value = false
  }
}

async function runRestore(): Promise<void> {
  loading.value = true
  error.value = ''
  done.value = ''
  try {
    const result = await restoreBackup({
      path: path.value.trim(),
      passphrase: passphrase.value,
      components: {
        database: includeDatabase.value,
        config: includeConfig.value,
        maps: includeMaps.value
      },
      dry_run: false
    })
    preview.value = result
    done.value = t('restore.done')
  } catch (e) {
    error.value = e instanceof ApiProblem ? e.title : t('restore.errors.restoreFailed')
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <main class="mx-auto max-w-2xl px-4 py-8">
    <h1 class="text-2xl font-semibold tracking-tight">{{ t('restore.title') }}</h1>
    <p class="mt-2 text-sm opacity-80">{{ t('restore.subtitle') }}</p>

    <p v-if="error" class="mt-4 text-sm text-red-700" role="alert">{{ error }}</p>
    <p v-if="rejectReason" class="mt-4 text-sm font-semibold text-red-800" role="alert">
      {{ rejectReason }}
    </p>
    <p v-if="done" class="mt-4 text-sm text-green-800">{{ done }}</p>

    <section class="mt-8 space-y-3 text-sm">
      <label class="block">
        <span>{{ t('restore.path') }}</span>
        <input v-model="path" class="mt-1 w-full border px-2 py-1" type="text" />
      </label>
      <label class="block">
        <span>{{ t('restore.passphrase') }}</span>
        <input v-model="passphrase" class="mt-1 w-full border px-2 py-1" type="password" />
      </label>
      <label class="flex items-center gap-2">
        <input v-model="includeDatabase" type="checkbox" />
        {{ t('restore.components.database') }}
      </label>
      <label class="flex items-center gap-2">
        <input v-model="includeConfig" type="checkbox" />
        {{ t('restore.components.config') }}
      </label>
      <label class="flex items-center gap-2">
        <input v-model="includeMaps" type="checkbox" />
        {{ t('restore.components.maps') }}
      </label>

      <div class="flex flex-wrap gap-2 pt-2">
        <button type="button" class="border px-3 py-1" :disabled="loading" @click="inspect">
          {{ t('restore.inspect') }}
        </button>
        <button type="button" class="border px-3 py-1" :disabled="loading" @click="dryRun">
          {{ t('restore.preview') }}
        </button>
        <button type="button" class="border px-3 py-1" :disabled="loading" @click="runRestore">
          {{ t('restore.run') }}
        </button>
      </div>

      <pre v-if="preview" class="mt-4 overflow-auto border p-2 text-xs">{{
        JSON.stringify(preview, null, 2)
      }}</pre>
    </section>
  </main>
</template>
