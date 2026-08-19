<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { mapErrorKey, type MapRegion, type Place, useMapsStore } from '@/stores/maps'

const props = defineProps<{
  assetIds: string[]
  availableRegionIds: string[]
  allRegions?: MapRegion[]
}>()

const emit = defineEmits<{ applied: [place: Place] }>()
const { t } = useI18n()
const maps = useMapsStore()
const query = ref('')
const results = ref<Place[]>([])
const selected = ref<Place>()
const searching = ref(false)
const applying = ref(false)
const error = ref<unknown>()

const mapUnavailable = computed(
  () =>
    selected.value?.country_code != null &&
    !props.availableRegionIds.includes(selected.value.country_code)
)
const errorMessage = computed(() => error.value ? t(mapErrorKey(error.value)) : '')

function placeContext(place: Place): string {
  return [place.admin1, place.country_code].filter(Boolean).join(', ')
}

async function search() {
  const normalized = query.value.trim()
  if (normalized.length < 2) return
  searching.value = true
  error.value = undefined
  try {
    results.value = await maps.suggestPlaces(normalized)
  } catch (cause) {
    error.value = cause
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
  error.value = undefined
  try {
    await maps.applyPlace(props.assetIds, selected.value)
    emit('applied', selected.value)
  } catch (cause) {
    error.value = cause
  } finally {
    applying.value = false
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
        <RouterLink
          to="/settings/maps/offline"
          class="rounded-lg border border-current px-3 py-2"
          data-action="download-region"
        >
          {{ t('maps.places.downloadRegionAction') }}
        </RouterLink>
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
      v-if="errorMessage"
      class="text-sm text-danger"
      role="alert"
    >
      {{ errorMessage }}
    </p>
  </section>
</template>
