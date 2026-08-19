<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'

import { REGION_CATALOG, type RegionCatalogEntry, useMapsStore } from '@/stores/maps'
import { useSessionStore } from '@/stores/session'

const { t } = useI18n()
const maps = useMapsStore()
const session = useSessionStore()
let pollTimer: ReturnType<typeof setInterval> | undefined

const continents = computed(() => {
  const grouped = new Map<string, RegionCatalogEntry[]>()
  for (const region of REGION_CATALOG) {
    const entries = grouped.get(region.continent) ?? []
    entries.push(region)
    grouped.set(region.continent, entries)
  }
  return [...grouped.entries()].map(([id, entries]) => ({
    id,
    entries,
    size: entries.reduce((total, entry) => total + entry.size_bytes, 0)
  }))
})

function formatBytes(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let size = Math.max(0, bytes)
  let unit = 0
  while (size >= 1000 && unit < units.length - 1) {
    size /= 1000
    unit += 1
  }
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(size)} ${units[unit]}`
}

function progress(downloaded: number, total: number): number {
  if (total <= 0) return 0
  return Math.min(100, Math.round((downloaded / total) * 100))
}

async function cancel(id: string) {
  await maps.cancelRegion(id)
}

async function remove(id: string) {
  await maps.deleteRegion(id)
}

onMounted(async () => {
  try {
    await maps.loadRegions()
  } finally {
    if (maps.regions.some((region) => region.status === 'downloading')) {
      pollTimer = setInterval(() => void maps.loadRegions(), 2000)
    }
  }
})

onBeforeUnmount(() => {
  if (pollTimer) clearInterval(pollTimer)
})
</script>

<template>
  <section class="space-y-6">
    <header>
      <h2 class="text-xl font-semibold">
        {{ t('maps.offline.title') }}
      </h2>
      <p class="mt-1 text-sm text-content-muted">
        {{ t('maps.offline.subtitle') }}
      </p>
    </header>

    <p
      v-if="maps.error"
      class="text-sm text-danger"
      role="alert"
    >
      {{ t('common.unexpectedError') }}
    </p>

    <section v-if="maps.regions.length > 0">
      <h3 class="mb-2 font-medium">
        {{ t('maps.offline.downloaded') }}
      </h3>
      <ul class="space-y-2">
        <li
          v-for="region in maps.regions"
          :key="region.id"
          class="rounded-lg border border-border bg-surface-elevated p-3"
        >
          <div class="flex flex-wrap items-center gap-2">
            <strong>{{ t(`maps.regions.${region.id}`, region.label) }}</strong>
            <span class="text-sm text-content-muted">{{ formatBytes(region.size_bytes) }}</span>
            <span class="text-xs text-content-muted">{{ region.version }}</span>
            <div
              v-if="session.user?.role === 'admin'"
              class="ml-auto flex gap-2"
            >
              <button
                v-if="region.status === 'downloading'"
                type="button"
                class="rounded border border-border px-2 py-1 text-xs"
                :data-action="`cancel-${region.id}`"
                @click="cancel(region.id)"
              >
                {{ t('maps.offline.cancel') }}
              </button>
              <button
                v-else
                type="button"
                class="rounded border border-danger px-2 py-1 text-xs text-danger"
                :data-action="`delete-${region.id}`"
                @click="remove(region.id)"
              >
                {{ t('maps.offline.delete') }}
              </button>
            </div>
          </div>
          <div
            v-if="region.status === 'downloading'"
            class="mt-2 flex items-center gap-2"
          >
            <progress
              class="h-2 flex-1"
              max="100"
              :value="progress(region.downloaded_bytes, region.size_bytes)"
            />
            <span class="text-xs">
              {{ progress(region.downloaded_bytes, region.size_bytes) }}%
            </span>
          </div>
          <p
            v-if="region.last_error"
            class="mt-2 text-sm text-danger"
          >
            {{ region.last_error }}
          </p>
        </li>
      </ul>
    </section>

    <section>
      <h3 class="mb-2 font-medium">
        {{ t('maps.offline.catalog') }}
      </h3>
      <details
        v-for="continent in continents"
        :key="continent.id"
        class="mb-2 rounded-lg border border-border bg-surface-elevated"
        open
      >
        <summary class="cursor-pointer px-3 py-2 font-medium">
          {{ t(`maps.continents.${continent.id}`) }}
          <span class="ml-2 text-sm font-normal text-content-muted">
            {{ formatBytes(continent.size) }}
          </span>
        </summary>
        <ul class="divide-y divide-border">
          <li
            v-for="region in continent.entries"
            :key="region.id"
            class="flex items-center gap-3 px-3 py-2 text-sm"
          >
            <span>{{ t(`maps.regions.${region.id}`) }}</span>
            <span class="text-content-muted">{{ formatBytes(region.size_bytes) }}</span>
            <button
              v-if="session.user?.role === 'admin' && !maps.regions.some((item) => item.id === region.id)"
              type="button"
              class="ml-auto rounded border border-border px-2 py-1"
              :disabled="maps.loading"
              @click="maps.downloadRegion(region)"
            >
              {{ t('maps.offline.download') }}
            </button>
          </li>
        </ul>
      </details>
    </section>
  </section>
</template>
