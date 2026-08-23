<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import { applyMetadataBatch, copyLocation, importGpx } from '@/api/metadata'
import PlacePicker from '@/components/PlacePicker.vue'
import { useMapsStore } from '@/stores/maps'

const { t } = useI18n()
const route = useRoute()
const maps = useMapsStore()

const assetIds = ref<string[]>(
  typeof route.query.ids === 'string' ? route.query.ids.split(',') : []
)
const field = ref('')
const value = ref('')
const done = ref(false)
const copySourceId = ref('')
const gpxText = ref('')
const gpxTolerance = ref('5')

async function submit() {
  if (!field.value.trim() || assetIds.value.length === 0) return
  await applyMetadataBatch(assetIds.value, { [field.value.trim()]: value.value || null })
  done.value = true
}

async function submitCopyLocation() {
  if (!copySourceId.value.trim() || assetIds.value.length === 0) return
  await copyLocation(copySourceId.value.trim(), assetIds.value)
  done.value = true
}

async function submitGpx() {
  if (!gpxText.value.trim() || assetIds.value.length === 0) return
  const tolerance = parseInt(gpxTolerance.value, 10) || undefined
  await importGpx(assetIds.value, gpxText.value, tolerance)
  done.value = true
}

onMounted(() => {
  void maps.loadRegions().catch(() => undefined)
})
</script>

<template>
  <main class="mx-auto max-w-3xl p-6">
    <!-- Fase 11 Task 6 (6/N): link "indietro" e titolo tolti — copre
         AppTopbar (briciola "Modifica in blocco", riusa batchEdit.title). -->
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
    <section
      v-if="!done && assetIds.length > 0"
      class="mt-8 border-t border-border pt-6"
    >
      <h2 class="mb-3 text-lg font-medium">
        {{ t('batchEdit.place') }}
      </h2>
      <PlacePicker
        :asset-ids="assetIds"
        :available-region-ids="maps.availableRegionIds"
        :all-regions="maps.regions"
        @applied="done = true"
      />
    </section>
    <section
      v-if="!done && assetIds.length > 0"
      class="mt-8 border-t border-border pt-6"
    >
      <h2 class="mb-3 text-lg font-medium">
        {{ t('batchEdit.copyLocation') }}
      </h2>
      <div class="flex gap-2">
        <input
          v-model="copySourceId"
          class="min-w-0 flex-1 rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
          :placeholder="t('batchEdit.copySourcePlaceholder')"
        >
        <button
          type="button"
          class="rounded-lg bg-accent px-3 py-2 text-sm text-white"
          @click="submitCopyLocation"
        >
          {{ t('batchEdit.apply') }}
        </button>
      </div>
    </section>
    <section
      v-if="!done && assetIds.length > 0"
      class="mt-8 border-t border-border pt-6"
    >
      <h2 class="mb-3 text-lg font-medium">
        {{ t('batchEdit.importGpx') }}
      </h2>
      <div class="space-y-2">
        <textarea
          v-model="gpxText"
          class="h-32 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm font-mono"
          :placeholder="t('batchEdit.gpxPlaceholder')"
        />
        <div class="flex items-end gap-2">
          <label class="text-sm">
            {{ t('batchEdit.gpxTolerance') }}
            <input
              v-model="gpxTolerance"
              type="number"
              min="1"
              class="mt-1 w-20 rounded-lg border border-border bg-surface-elevated px-2 py-1"
            >
          </label>
          <button
            type="button"
            class="rounded-lg bg-accent px-3 py-2 text-sm text-white"
            @click="submitGpx"
          >
            {{ t('batchEdit.apply') }}
          </button>
        </div>
      </div>
    </section>
    <p
      v-if="done"
      class="mt-6 text-content-muted"
    >
      {{ t('batchEdit.done') }}
    </p>
  </main>
</template>
