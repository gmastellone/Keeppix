<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import {
  createBackupDestination,
  getBackupPreferences,
  listBackupDestinations,
  listBackupRuns,
  putBackupPreferences,
  runBackupNow,
  type BackupDestination,
  type BackupPreferences,
  type BackupRun
} from '@/api/backup'
import { ApiProblem } from '@/api/client'

const { t } = useI18n()

const prefs = ref<BackupPreferences | null>(null)
const destinations = ref<BackupDestination[]>([])
const runs = ref<BackupRun[]>([])
const loading = ref(false)
const error = ref('')
const passphrase = ref('')
const newKind = ref('local')
const newLabel = ref('')
const newPath = ref('/data/backups')

const showOriginalsWarning = computed(
  () => prefs.value?.originals_warning === true && !prefs.value.include_originals
)

async function refresh(): Promise<void> {
  error.value = ''
  try {
    prefs.value = await getBackupPreferences()
    destinations.value = await listBackupDestinations()
    runs.value = await listBackupRuns()
  } catch {
    error.value = t('backup.errors.loadFailed')
  }
}

async function savePrefs(): Promise<void> {
  if (!prefs.value) return
  loading.value = true
  error.value = ''
  try {
    const body: Record<string, unknown> = { ...prefs.value }
    if (passphrase.value.trim()) {
      body.passphrase = passphrase.value.trim()
    }
    prefs.value = await putBackupPreferences(body)
    passphrase.value = ''
  } catch {
    error.value = t('backup.errors.saveFailed')
  } finally {
    loading.value = false
  }
}

async function toggleOriginals(value: boolean): Promise<void> {
  if (!prefs.value) return
  prefs.value = { ...prefs.value, include_originals: value }
  await savePrefs()
}

async function addDestination(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    await createBackupDestination({
      kind: newKind.value,
      label: newLabel.value || t('backup.destinationDefault'),
      config: { path: newPath.value }
    })
    newLabel.value = ''
    await refresh()
  } catch (e) {
    error.value =
      e instanceof ApiProblem ? e.title : t('backup.errors.destinationFailed')
  } finally {
    loading.value = false
  }
}

async function runNow(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    await runBackupNow()
    await refresh()
  } catch (e) {
    error.value = e instanceof ApiProblem ? e.title : t('backup.errors.runFailed')
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  void refresh()
})
</script>

<template>
  <main class="mx-auto max-w-2xl px-4 py-8">
    <h1 class="text-2xl font-semibold tracking-tight">{{ t('backup.title') }}</h1>
    <p class="mt-2 text-sm opacity-80">{{ t('backup.subtitle') }}</p>

    <p v-if="error" class="mt-4 text-sm text-red-700" role="alert">{{ error }}</p>

    <section v-if="prefs" class="mt-8 space-y-4">
      <h2 class="text-lg font-medium">{{ t('backup.content') }}</h2>
      <label class="flex items-center gap-2 text-sm">
        <input v-model="prefs.include_database" type="checkbox" @change="savePrefs" />
        {{ t('backup.include.database') }}
      </label>
      <label class="flex items-center gap-2 text-sm">
        <input v-model="prefs.include_config" type="checkbox" @change="savePrefs" />
        {{ t('backup.include.config') }}
      </label>
      <label class="flex items-center gap-2 text-sm">
        <input v-model="prefs.include_maps" type="checkbox" @change="savePrefs" />
        {{ t('backup.include.maps') }}
      </label>
      <label class="flex items-center gap-2 text-sm">
        <input v-model="prefs.include_sidecars" type="checkbox" @change="savePrefs" />
        {{ t('backup.include.sidecars') }}
      </label>
      <label class="flex items-center gap-2 text-sm">
        <input v-model="prefs.include_derivatives" type="checkbox" @change="savePrefs" />
        {{ t('backup.include.derivatives') }}
      </label>
      <label class="flex items-center gap-2 text-sm">
        <input
          :checked="prefs.include_originals"
          type="checkbox"
          @change="toggleOriginals(($event.target as HTMLInputElement).checked)"
        />
        {{ t('backup.include.originals') }}
      </label>

      <p
        v-if="showOriginalsWarning"
        data-testid="originals-warning"
        class="rounded border border-amber-700/40 bg-amber-50 px-3 py-2 text-sm font-semibold text-amber-950"
        role="status"
      >
        {{ prefs.originals_warning_message || t('backup.originalsWarning') }}
      </p>

      <h2 class="pt-4 text-lg font-medium">{{ t('backup.encryption') }}</h2>
      <p class="text-sm opacity-80">{{ t('backup.passphraseHint') }}</p>
      <input
        v-model="passphrase"
        type="password"
        class="w-full border px-2 py-1 text-sm"
        :placeholder="
          prefs.passphrase_set ? t('backup.passphraseChange') : t('backup.passphraseSet')
        "
        @change="savePrefs"
      />

      <h2 class="pt-4 text-lg font-medium">{{ t('backup.destination') }}</h2>
      <ul class="space-y-1 text-sm">
        <li v-for="d in destinations" :key="d.id">{{ d.label }} ({{ d.kind }})</li>
      </ul>
      <div class="flex flex-wrap gap-2 text-sm">
        <select v-model="newKind" class="border px-2 py-1">
          <option value="local">local</option>
          <option value="s3">s3</option>
          <option value="webdav">webdav</option>
          <option value="sftp">sftp</option>
        </select>
        <input v-model="newLabel" class="border px-2 py-1" :placeholder="t('backup.label')" />
        <input v-model="newPath" class="border px-2 py-1" :placeholder="t('backup.path')" />
        <button
          type="button"
          class="border px-3 py-1"
          :disabled="loading"
          @click="addDestination"
        >
          {{ t('backup.addDestination') }}
        </button>
      </div>

      <div class="pt-6">
        <button
          type="button"
          class="border px-4 py-2 text-sm font-medium"
          :disabled="loading"
          @click="runNow"
        >
          {{ t('backup.runNow') }}
        </button>
      </div>

      <h2 class="pt-6 text-lg font-medium">{{ t('backup.history') }}</h2>
      <ul class="space-y-1 text-sm">
        <li v-for="r in runs" :key="r.id">
          {{ r.started_at }} · {{ r.status }}
          <span v-if="r.size_bytes != null">· {{ r.size_bytes }} B</span>
          <span v-if="r.verified_at">· ✓</span>
        </li>
      </ul>
    </section>
  </main>
</template>
