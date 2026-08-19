<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import {
  REGION_CATALOG,
  type Place,
  useMapsStore
} from '@/stores/maps'

const props = withDefaults(
  defineProps<{
    assetIds: string[]
    availableRegionIds: string[]
    canDownload?: boolean
  }>(),
  { canDownload: false }
)

const emit = defineEmits<{ applied: [place: Place] }>()
const { t } = useI18n()
const maps = useMapsStore()
const query = ref('')
const results = ref<Place[]>([])
const selected = ref<Place>()
const searching = ref(false)
const applying = ref(false)
const downloading = ref(false)
const error = ref(false)

const selectedRegion = computed(() =>
  REGION_CATALOG.find((region) => region.id === selected.value?.country_code)
)
const mapUnavailable = computed(
  () =>
    selected.value?.country_code != null &&
    !props.availableRegionIds.includes(selected.value.country_code)
)

function placeContext(place: Place): string {
  return [place.admin1, place.country_code].filter(Boolean).join(', ')
}

async function search() {
  const normalized = query.value.trim()
  if (normalized.length < 2) return
  searching.value = true
  error.value = false
  try {
    results.value = await maps.suggestPlaces(normalized)
  } catch {
    error.value = true
  } finally {
    searching.value = false
  }
}

function choose(place: Place) {
  selected.value = place
  query.value = place.name
  results.value = []
}

async function apply() {
  if (!selected.value || props.assetIds.length === 0) return
  applying.value = true
  error.value = false
  try {
    await maps.applyPlace(props.assetIds, selected.value)
    emit('applied', selected.value)
  } catch {
    error.value = true
  } finally {
    applying.value = false
  }
}

async function download() {
  if (!selectedRegion.value) return
  downloading.value = true
  error.value = false
  try {
    await maps.downloadRegion(selectedRegion.value)
  } catch {
    error.value = true
  } finally {
    downloading.value = false
  }
}
</script>

<template>
  <section class="space-y-3">
    <form
      class="flex gap-2"
      @submit.prevent="search"
    >
      <input
        v-model="query"
        type="search"
        class="min-w-0 flex-1 rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
        :placeholder="t('maps.places.placeholder')"
        :aria-label="t('maps.places.label')"
      >
      <button
        type="submit"
        class="rounded-lg border border-border px-3 py-2 text-sm"
        :disabled="searching || query.trim().length < 2"
      >
        {{ searching ? t('common.loading') : t('maps.places.search') }}
      </button>
    </form>

    <ul
      v-if="results.length > 0"
      class="divide-y divide-border rounded-lg border border-border bg-surface-elevated"
    >
      <li
        v-for="place in results"
        :key="place.id"
      >
        <button
          type="button"
          class="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm"
          :data-place-id="place.id"
          @click="choose(place)"
        >
          <span>
            <strong>{{ place.name }}</strong>
            <span class="ml-2 text-content-muted">{{ placeContext(place) }}</span>
          </span>
          <span class="text-xs text-content-muted">
            {{ new Intl.NumberFormat().format(place.population) }}
          </span>
        </button>
      </li>
    </ul>

    <div
      v-if="selected && mapUnavailable"
      class="rounded-lg border border-amber-500 bg-amber-50 p-3 text-sm text-amber-950 dark:bg-amber-950 dark:text-amber-50"
      role="status"
    >
      <p class="font-medium">
        {{ t('maps.places.mapUnavailable') }}
      </p>
      <p class="mt-1">
        {{ t('maps.places.applyAnyway', { count: assetIds.length }) }}
      </p>
      <div class="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          class="rounded-lg bg-accent px-3 py-2 text-white"
          data-action="apply"
          :disabled="applying"
          @click="apply"
        >
          {{ t('maps.places.apply') }}
        </button>
        <button
          v-if="canDownload && selectedRegion"
          type="button"
          class="rounded-lg border border-current px-3 py-2"
          data-action="download"
          :disabled="downloading"
          @click="download"
        >
          {{ t('maps.places.downloadRegion', { region: t(`maps.regions.${selectedRegion.id}`) }) }}
        </button>
      </div>
    </div>

    <button
      v-else-if="selected"
      type="button"
      class="rounded-lg bg-accent px-3 py-2 text-sm text-white"
      data-action="apply"
      :disabled="applying"
      @click="apply"
    >
      {{ t('maps.places.apply') }}
    </button>

    <p
      v-if="error"
      class="text-sm text-danger"
      role="alert"
    >
      {{ t('common.unexpectedError') }}
    </p>
  </section>
</template>
