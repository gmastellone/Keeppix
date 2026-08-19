<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { isUnauthenticated } from '@/api/client'
import { fetchLibraries, type Library } from '@/api/libraries'
import { fetchDuplicates, fetchProblems, type DuplicateGroup, type Problems } from '@/api/library'
import {
  applyTimezones,
  previewTimezones,
  type TimezoneApplyResult,
  type TimezonePreview
} from '@/api/metadata'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()
const problems = ref<Problems | null>(null)
const duplicates = ref<DuplicateGroup[]>([])
const libraries = ref<Library[]>([])
const loadError = ref(false)

const tzLibraryId = ref('')
const tzPreview = ref<TimezonePreview | null>(null)
const tzResult = ref<TimezoneApplyResult | null>(null)
const tzBusy = ref(false)

async function load() {
  loadError.value = false
  try {
    problems.value = await fetchProblems()
    duplicates.value = await fetchDuplicates()
    libraries.value = await fetchLibraries()
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = true
  }
}

async function tzPreviewAction() {
  if (!tzLibraryId.value || tzBusy.value) return
  tzBusy.value = true
  tzPreview.value = null
  tzResult.value = null
  try {
    tzPreview.value = await previewTimezones(tzLibraryId.value)
  } finally {
    tzBusy.value = false
  }
}

async function tzApplyAction() {
  if (!tzPreview.value || !tzLibraryId.value || tzBusy.value) return
  tzBusy.value = true
  try {
    tzResult.value = await applyTimezones(tzLibraryId.value, tzPreview.value.preview_token)
    tzPreview.value = null
  } finally {
    tzBusy.value = false
  }
}

onMounted(() => {
  void load()
})
</script>

<template>
  <main class="mx-auto max-w-3xl p-6">
    <h1 class="text-2xl font-semibold">
      {{ t('problems.title') }}
    </h1>
    <p
      v-if="loadError"
      class="mt-6 text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>
    <button
      v-if="loadError"
      class="mt-3 rounded-lg border border-border px-3 py-1"
      type="button"
      @click="load"
    >
      {{ t('common.retry') }}
    </button>
    <section
      v-if="problems"
      class="mt-6 space-y-6"
    >
      <div>
        <h2 class="font-medium">
          {{ t('problems.offlineLibraries') }}
        </h2>
        <p
          v-if="problems.offline_libraries.length === 0"
          class="text-content-muted"
        >
          {{ t('problems.none') }}
        </p>
        <ul>
          <li
            v-for="lib in problems.offline_libraries"
            :key="lib.id"
          >
            {{ lib.name }}
          </li>
        </ul>
      </div>
      <div>
        <h2 class="font-medium">
          {{ t('problems.failedJobs') }}
        </h2>
        <p
          v-if="problems.failed_jobs.length === 0"
          class="text-content-muted"
        >
          {{ t('problems.none') }}
        </p>
        <ul>
          <li
            v-for="job in problems.failed_jobs"
            :key="job.id"
          >
            {{ job.kind }} — {{ job.last_error }}
          </li>
        </ul>
      </div>
      <div>
        <h2 class="font-medium">
          {{ t('problems.errorAssets') }}
        </h2>
        <p
          v-if="problems.error_assets.length === 0"
          class="text-content-muted"
        >
          {{ t('problems.none') }}
        </p>
        <ul>
          <li
            v-for="asset in problems.error_assets"
            :key="asset.id"
          >
            {{ asset.filename }}
          </li>
        </ul>
      </div>
      <div>
        <h2 class="font-medium">
          {{ t('problems.duplicates') }}
        </h2>
        <p
          v-if="duplicates.length === 0"
          class="text-content-muted"
        >
          {{ t('problems.none') }}
        </p>
        <ul>
          <li
            v-for="group in duplicates"
            :key="group.content_hash"
          >
            {{ t('problems.reclaimable', { count: group.count, bytes: group.reclaimable_bytes }) }}
          </li>
        </ul>
      </div>
      <div>
        <h2 class="font-medium">
          {{ t('problems.timezones') }}
        </h2>
        <div class="mt-2 space-y-3">
          <div class="flex items-end gap-2">
            <label class="block flex-1 text-sm">
              {{ t('problems.tzLibrary') }}
              <select
                v-model="tzLibraryId"
                class="mt-1 block w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
              >
                <option
                  v-for="lib in libraries"
                  :key="lib.id"
                  :value="lib.id"
                >
                  {{ lib.name }}
                </option>
              </select>
            </label>
            <button
              type="button"
              class="rounded-lg border border-border px-3 py-2 text-sm"
              :disabled="!tzLibraryId || tzBusy"
              @click="tzPreviewAction"
            >
              {{ t('problems.tzPreview') }}
            </button>
          </div>
          <div
            v-if="tzPreview"
            class="rounded-lg border border-border bg-surface-elevated p-3 text-sm"
          >
            <p>{{ t('problems.tzCount', { count: tzPreview.count }) }}</p>
            <p
              v-if="tzPreview.example"
              class="mt-1 text-content-muted"
            >
              {{ tzPreview.example.filename }}:
              {{ tzPreview.example.before }} → {{ tzPreview.example.after }}
              ({{ tzPreview.example.timezone }})
            </p>
            <button
              v-if="tzPreview.count > 0"
              type="button"
              class="mt-2 rounded-lg bg-accent px-3 py-2 text-white"
              :disabled="tzBusy"
              @click="tzApplyAction"
            >
              {{ t('problems.tzApply') }}
            </button>
          </div>
          <p
            v-if="tzResult"
            class="text-sm text-content-muted"
          >
            {{ t('problems.tzDone', { count: tzResult.changed_count }) }}
          </p>
        </div>
      </div>
    </section>
  </main>
</template>
