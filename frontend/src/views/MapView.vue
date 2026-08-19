<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { fetchLibraries, type Library } from '@/api/libraries'
import type { TimelineAsset } from '@/api/timeline'
import AssetViewer from '@/components/AssetViewer.vue'
import MapClusterLayer from '@/components/MapClusterLayer.vue'
import type { MapBounds } from '@/stores/maps'
import { useMapsStore } from '@/stores/maps'
import MapsOfflineView from '@/views/settings/MapsOfflineView.vue'

const { t } = useI18n()
const router = useRouter()
const maps = useMapsStore()
const libraries = ref<Library[]>([])
const viewing = ref<TimelineAsset>()
const loading = ref(true)
const failed = ref(false)
const managingRegions = ref(false)

const library = computed(() => libraries.value[0])
const availableRegions = computed(() =>
  maps.regions.filter((region) => region.status === 'available')
)

async function load() {
  loading.value = true
  failed.value = false
  try {
    await Promise.all([maps.loadRegions(), fetchLibraries().then((items) => (libraries.value = items))])
  } catch {
    failed.value = true
  } finally {
    loading.value = false
  }
}

async function openAsset(id: string) {
  try {
    viewing.value = await maps.loadAsset(id)
  } catch {
    failed.value = true
  }
}

async function showArea(bounds: MapBounds) {
  await router.push({
    path: '/',
    query: {
      bbox: `${bounds.west},${bounds.south},${bounds.east},${bounds.north}`
    }
  })
}

onMounted(load)
</script>

<template>
  <main class="flex h-full min-h-0 flex-col">
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <RouterLink
        class="text-sm underline"
        to="/"
      >
        {{ t('maps.back') }}
      </RouterLink>
      <h1 class="text-xl font-semibold">
        {{ t('maps.title') }}
      </h1>
      <button
        v-if="availableRegions.length > 0"
        type="button"
        class="ml-auto rounded-lg border border-border px-3 py-1.5 text-sm"
        @click="managingRegions = !managingRegions"
      >
        {{ t('maps.regionManager') }}
      </button>
    </header>

    <p
      v-if="loading"
      class="p-6 text-content-muted"
    >
      {{ t('common.loading') }}
    </p>
    <div
      v-else-if="failed"
      class="p-6"
    >
      <p class="text-danger">
        {{ t('common.unexpectedError') }}
      </p>
      <button
        type="button"
        class="mt-3 rounded-lg border border-border px-3 py-2 text-sm"
        @click="load"
      >
        {{ t('common.retry') }}
      </button>
    </div>
    <div
      v-else-if="availableRegions.length === 0"
      class="overflow-auto p-6"
    >
      <MapsOfflineView />
    </div>
    <p
      v-else-if="!library"
      class="p-6 text-content-muted"
    >
      {{ t('maps.noLibrary') }}
    </p>
    <div
      v-else
      class="relative min-h-0 flex-1 p-3"
    >
      <MapClusterLayer
        :scope-id="library.id"
        scope="library"
        :region-ids="availableRegions.map((region) => region.id)"
        allow-draw
        @asset-click="openAsset"
        @area-selected="showArea"
      />
      <aside
        v-if="managingRegions"
        class="absolute inset-y-3 right-3 z-20 w-full max-w-md overflow-auto border-l border-border bg-surface p-4 shadow-xl"
      >
        <MapsOfflineView />
      </aside>
    </div>

    <AssetViewer
      v-if="viewing"
      :asset="viewing"
      @close="viewing = undefined"
      @open-asset="openAsset"
    />
  </main>
</template>
