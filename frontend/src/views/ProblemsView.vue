<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchDuplicates, fetchProblems, type DuplicateGroup, type Problems } from '@/api/library'

const { t } = useI18n()
const problems = ref<Problems | null>(null)
const duplicates = ref<DuplicateGroup[]>([])

onMounted(async () => {
  problems.value = await fetchProblems()
  duplicates.value = await fetchDuplicates()
})
</script>

<template>
  <main class="mx-auto max-w-3xl p-6">
    <h1 class="text-2xl font-semibold">
      {{ t('problems.title') }}
    </h1>
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
    </section>
  </main>
</template>
