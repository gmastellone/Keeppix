<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import {
  mapErrorKey,
  regionDisplayLabel,
  type RegionCatalogEntry,
  type RegionDownloadRequest,
  useMapsStore
} from '@/stores/maps'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const maps = useMapsStore()
const session = useSessionStore()
const toast = useToastStore()
let pollTimer: ReturnType<typeof setInterval> | undefined

const operationError = ref<unknown>()
const form = ref<RegionDownloadRequest>({
  id: '',
  label: '',
  size_bytes: 0,
  version: '',
  source_url: '',
  checksum_sha256: ''
})

const searchOpen = ref(false)
const searchQuery = ref('')
const searchInput = ref<HTMLInputElement>()
const adding = ref(false)

const errorMessage = computed(() => {
  const error = operationError.value ?? maps.error
  return error ? t(mapErrorKey(error)) : ''
})

// Substring match on label, excluding regions already tracked (downloading
// or downloaded) — a region already in the list simply doesn't reappear
// in results, rather than showing disabled (matches the spec's "una
// regione già in elenco non compare affatto invece di comparire
// disattivata").
const searchResults = computed<RegionCatalogEntry[]>(() => {
  const query = searchQuery.value.trim().toLowerCase()
  if (!query) return []
  const known = new Set(maps.regions.map((region) => region.id))
  return maps.catalog
    .filter(
      (entry) =>
        !known.has(entry.id) &&
        regionDisplayLabel(entry.id, entry.label).toLowerCase().includes(query)
    )
    .slice(0, 8)
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

async function openSearch() {
  searchOpen.value = true
  searchQuery.value = ''
  await maps.loadCatalog().catch((cause) => {
    operationError.value = cause
  })
  await nextTick()
  searchInput.value?.focus()
  searchInput.value?.setSelectionRange(searchQuery.value.length, searchQuery.value.length)
}

function closeSearch() {
  searchOpen.value = false
  searchQuery.value = ''
}

// Esc closes the search from anywhere it has focus (input, a result row,
// the close button) — handled globally rather than on the input alone, so
// focus can never get "stuck" inside the search box.
function onGlobalKeydown(event: KeyboardEvent) {
  if (searchOpen.value && event.key === 'Escape') closeSearch()
}

async function addFromCatalog(entry: RegionCatalogEntry) {
  if (adding.value) return
  adding.value = true
  operationError.value = undefined
  try {
    await maps.downloadFromCatalog(entry.id)
    closeSearch()
    toast.show(t('maps.offline.search.added', { name: regionDisplayLabel(entry.id, entry.label) }))
    ensurePolling()
  } catch (cause) {
    operationError.value = cause
  } finally {
    adding.value = false
  }
}

function onResultKeydown(event: KeyboardEvent, entry: RegionCatalogEntry) {
  if (event.key !== 'Enter' && event.key !== ' ') return
  event.preventDefault()
  void addFromCatalog(entry)
}

async function cancel(id: string) {
  operationError.value = undefined
  try {
    await maps.cancelRegion(id)
  } catch (error) {
    operationError.value = error
  }
}

async function remove(id: string) {
  operationError.value = undefined
  try {
    await maps.deleteRegion(id)
  } catch (error) {
    operationError.value = error
  }
}

function stopPolling() {
  if (pollTimer) clearInterval(pollTimer)
  pollTimer = undefined
}

async function pollRegions() {
  try {
    await maps.loadRegions()
  } catch {
    // `maps.error` is rendered by the view; keep polling transient failures.
  }
  if (!maps.regions.some((region) => region.status === 'downloading')) {
    stopPolling()
  }
}

function ensurePolling() {
  if (pollTimer || !maps.regions.some((region) => region.status === 'downloading')) return
  pollTimer = setInterval(() => void pollRegions(), 2000)
}

async function download() {
  operationError.value = undefined
  try {
    await maps.downloadRegion({ ...form.value })
    ensurePolling()
  } catch (error) {
    operationError.value = error
  }
}

onMounted(async () => {
  window.addEventListener('keydown', onGlobalKeydown)
  await pollRegions()
  ensurePolling()
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKeydown)
  stopPolling()
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
      v-if="errorMessage"
      class="text-sm text-danger"
      role="alert"
    >
      {{ errorMessage }}
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
            <strong>{{ regionDisplayLabel(region.id, region.label) }}</strong>
            <span class="text-sm text-content-muted">{{ formatBytes(region.size_bytes) }}</span>
            <span class="text-xs text-content-muted">{{ region.version }}</span>
            <span class="text-xs text-content-muted">
              {{ t(`maps.offline.status.${region.status}`) }}
            </span>
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
            <strong>{{ t('maps.offline.downloadFailed') }}</strong>
            <span class="ml-1">{{ region.last_error }}</span>
          </p>
        </li>
      </ul>
    </section>
    <p
      v-else
      class="text-sm text-content-muted"
    >
      {{ t('maps.offline.empty') }}
    </p>

    <section v-if="session.user?.role === 'admin'">
      <h3 class="mb-2 font-medium">
        {{ t('maps.offline.catalog') }}
      </h3>

      <button
        v-if="!searchOpen"
        type="button"
        role="button"
        tabindex="0"
        class="inline-flex items-center gap-1.5 rounded border border-border px-3 py-1.5 text-sm hover:bg-chip-bg"
        data-action="open-region-search"
        @click="openSearch"
      >
        <span aria-hidden="true">+</span>
        {{ t('maps.offline.search.open') }}
      </button>

      <div
        v-else
        class="region-search-box space-y-2"
      >
        <div class="flex items-center gap-2">
          <label
            for="regionSearchInput"
            class="sr-only"
          >
            {{ t('maps.offline.search.srLabel') }}
          </label>
          <input
            id="regionSearchInput"
            ref="searchInput"
            v-model="searchQuery"
            type="text"
            autocomplete="off"
            :placeholder="t('maps.offline.search.placeholder')"
            class="w-full min-w-0 flex-1 rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
          >
          <button
            type="button"
            class="rounded border border-border px-2 py-1.5 text-sm"
            :aria-label="t('maps.offline.search.close')"
            data-action="close-region-search"
            @click="closeSearch"
          >
            ✕
          </button>
        </div>

        <ul
          v-if="searchResults.length > 0"
          role="listbox"
          :aria-label="t('maps.offline.search.resultsLabel')"
          class="region-search-results max-h-[220px] divide-y divide-border overflow-y-auto rounded-lg border border-border"
        >
          <li
            v-for="entry in searchResults"
            :key="entry.id"
            role="option"
            aria-selected="false"
            tabindex="0"
            class="flex cursor-pointer items-center justify-between gap-3 px-3 py-2 text-sm hover:bg-chip-bg"
            :data-region-id="entry.id"
            @click="addFromCatalog(entry)"
            @keydown="onResultKeydown($event, entry)"
          >
            <span>{{ regionDisplayLabel(entry.id, entry.label) }}</span>
            <span class="text-xs text-content-muted">{{ formatBytes(entry.approx_size_bytes) }}</span>
          </li>
        </ul>
        <p
          v-else
          class="region-search-empty text-sm text-content-muted"
        >
          {{ searchQuery.trim() ? t('maps.offline.search.noResults') : t('maps.offline.search.emptyPrompt') }}
        </p>
      </div>

      <details class="mt-4">
        <summary class="cursor-pointer text-sm text-content-muted">
          {{ t('maps.offline.manual.toggle') }}
        </summary>
        <form
          class="mt-3 grid gap-3 rounded-lg border border-border bg-surface-elevated p-3 sm:grid-cols-2"
          data-action="download-region"
          @submit.prevent="download"
        >
          <label class="text-sm">
            <span class="mb-1 block">{{ t('maps.offline.fields.id') }}</span>
            <input
              v-model.trim="form.id"
              name="region-id"
              required
              class="w-full rounded border border-border bg-surface px-2 py-1.5"
            >
          </label>
          <label class="text-sm">
            <span class="mb-1 block">{{ t('maps.offline.fields.label') }}</span>
            <input
              v-model.trim="form.label"
              name="region-label"
              required
              class="w-full rounded border border-border bg-surface px-2 py-1.5"
            >
          </label>
          <label class="text-sm">
            <span class="mb-1 block">{{ t('maps.offline.fields.size') }}</span>
            <input
              v-model.number="form.size_bytes"
              name="region-size"
              type="number"
              min="1"
              required
              class="w-full rounded border border-border bg-surface px-2 py-1.5"
            >
          </label>
          <label class="text-sm">
            <span class="mb-1 block">{{ t('maps.offline.fields.version') }}</span>
            <input
              v-model.trim="form.version"
              name="region-version"
              required
              class="w-full rounded border border-border bg-surface px-2 py-1.5"
            >
          </label>
          <label class="text-sm sm:col-span-2">
            <span class="mb-1 block">{{ t('maps.offline.fields.url') }}</span>
            <input
              v-model.trim="form.source_url"
              name="region-url"
              type="url"
              pattern="https://.*"
              required
              class="w-full rounded border border-border bg-surface px-2 py-1.5"
            >
          </label>
          <label class="text-sm sm:col-span-2">
            <span class="mb-1 block">{{ t('maps.offline.fields.sha256') }}</span>
            <input
              v-model.trim="form.checksum_sha256"
              name="region-sha256"
              minlength="64"
              maxlength="64"
              pattern="[0-9a-fA-F]{64}"
              required
              class="w-full rounded border border-border bg-surface px-2 py-1.5 font-mono"
            >
          </label>
          <button
            type="submit"
            class="rounded border border-border px-3 py-2 text-sm sm:col-span-2"
            :disabled="maps.loading"
          >
            {{ t('maps.offline.download') }}
          </button>
        </form>
      </details>
    </section>
  </section>
</template>
