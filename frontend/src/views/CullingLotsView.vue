<script setup lang="ts">
// Culling is a separate, per-folder area with real physical file moves,
// unrelated to regular photos.
//
// **Multi-library support**: each library has its own culling root
// (`Library.culling_root_folder_id`), so there is one section per
// library — the same approach already used for face recognition and the
// culling root folder setting. With only one library configured, the
// layout collapses to a single section.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { fetchCullingLots, type CullingLot } from '@/api/culling'
import { fetchAllFolders } from '@/api/folders'
import { fetchLibraries, type Library } from '@/api/libraries'
import { folderPathName } from '@/utils/folderPath'

const { t, locale } = useI18n()
const router = useRouter()

interface LibrarySection {
  library: Library
  path: string | null
  lots: CullingLot[]
}

const sections = ref<LibrarySection[]>([])
const loading = ref(true)
const loadError = ref(false)

onMounted(load)

async function load() {
  loading.value = true
  loadError.value = false
  try {
    const [libraries, folders] = await Promise.all([fetchLibraries(), fetchAllFolders()])
    const byId = new Map(folders.map((f) => [f.id, f]))
    const withRoot = libraries.filter((l) => l.culling_root_folder_id)
    const lotsByLibrary = await Promise.all(withRoot.map((l) => fetchCullingLots(l.id)))
    sections.value = withRoot.map((library, i) => ({
      library,
      path: library.culling_root_folder_id ? folderPathName(library.culling_root_folder_id, byId) : null,
      lots: lotsByLibrary[i]
    }))
  } catch {
    loadError.value = true
  } finally {
    loading.value = false
  }
}

const hasAnyRoot = computed(() => sections.value.length > 0)

function openLot(lot: CullingLot, libraryId: string) {
  void router.push({ path: `/culling/${lot.folder_id}`, query: { name: lot.name, library: libraryId } })
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString(locale.value, { day: 'numeric', month: 'short', year: 'numeric' })
}
</script>

<template>
  <main class="mx-auto max-w-4xl p-6">
    <p
      v-if="loadError"
      class="text-content-muted"
    >
      {{ t('common.unexpectedError') }}
    </p>

    <p
      v-else-if="!loading && !hasAnyRoot"
      class="text-content-muted"
    >
      {{ t('culling.noRootConfigured') }}
      <RouterLink
        to="/settings"
        class="text-accent underline"
      >
        {{ t('culling.rootChangeLink') }}
      </RouterLink>
    </p>

    <template
      v-for="section in sections"
      :key="section.library.id"
    >
      <div class="mt-6 flex items-center gap-1.5 text-[13px] first:mt-0">
        <span>{{ t('culling.rootLine', { path: section.path ?? '—' }) }}</span>
        <RouterLink
          to="/settings"
          class="text-accent underline"
        >
          {{ t('culling.rootChangeLink') }}
        </RouterLink>
      </div>

      <p class="mt-4 text-[13.5px] font-semibold">
        {{ t('culling.lotsTitle') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('culling.lotsSubtitle') }}
      </p>

      <div
        v-if="section.lots.length === 0"
        class="mt-4 text-content-muted"
      >
        {{ t('culling.empty') }}
      </div>
      <div
        v-else
        class="mt-4 grid grid-cols-2 gap-3 sm:grid-cols-3"
      >
        <button
          v-for="lot in section.lots"
          :key="lot.folder_id"
          type="button"
          class="rounded-lg border border-border p-3 text-left transition-colors hover:border-[var(--color-border-strong)]"
          @click="openLot(lot, section.library.id)"
        >
          <div class="h-[110px] rounded-md bg-gradient-to-br from-border to-surface-elevated" />
          <p class="mt-2 truncate text-[13.5px] font-semibold">
            {{ lot.name }}
          </p>
          <p class="text-[12px] text-content-muted">
            {{ t('culling.lotSubtitle', { date: formatDate(lot.created_at), n: lot.pending + lot.taken + lot.skipped }) }}
          </p>
          <div class="mt-2 flex items-center gap-3 text-[12px] text-content-muted">
            <span>○ {{ t('culling.lotTodo', { n: lot.pending }) }}</span>
            <span>✓ {{ lot.taken }}</span>
            <span>✕ {{ lot.skipped }}</span>
          </div>
        </button>
      </div>
    </template>
  </main>
</template>
