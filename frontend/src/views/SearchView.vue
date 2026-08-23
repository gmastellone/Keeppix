<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { runSearch } from '@/api/library'
import { fetchSavedSearches, fetchSuggestions } from '@/api/search'
import type { TimelineAsset } from '@/api/timeline'
import { thumbSrc } from '@/api/media'
import { parseSearch } from '@/search/parse'

import AssetViewer from '@/components/AssetViewer.vue'
import { useLightboxRoute } from '@/composables/useLightboxRoute'
import { useMapsStore } from '@/stores/maps'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const maps = useMapsStore()
const q = ref(typeof route.query.q === 'string' ? route.query.q : '')
const assets = ref<TimelineAsset[]>([])
const error = ref('')
// Fase 11 Task 3: la foto aperta nel visore vive nell'URL (?photo=),
// stesso composable di TimelineView — vedi lì per il ragionamento
// completo su push/replace/back.
const lightbox = useLightboxRoute<TimelineAsset>(
  (id) => assets.value.find((asset) => asset.id === id),
  (id) => maps.loadAsset(id)
)

function neighbour(delta: number): TimelineAsset | undefined {
  const i = assets.value.findIndex((a) => a.id === lightbox.viewing.value?.id)
  if (i < 0) return undefined
  return assets.value[i + delta]
}

const chips = computed(() => {
  try {
    const ast = parseSearch(q.value)
    if (ast.op === 'and') return ast.args.map(label)
    return [label(ast)]
  } catch {
    return []
  }
})

function label(node: ReturnType<typeof parseSearch>): string {
  if (node.op === 'text') return node.value
  if (node.op === 'type') return `type:${node.value}`
  if (node.op === 'camera') return `camera:${node.value}`
  if (node.op === 'year') return `year:${node.value}`
  return node.op
}

async function submit() {
  error.value = ''
  await router.replace({ query: { q: q.value } })
  try {
    const collected: TimelineAsset[] = []
    let cursor: string | undefined
    const ast = parseSearch(q.value)
    do {
      const page = await runSearch(ast, cursor)
      collected.push(...page.assets)
      cursor = page.next_cursor
    } while (cursor)
    assets.value = collected
  } catch {
    error.value = t('search.invalid')
  }
}

function stepViewer(delta: number) {
  void lightbox.step(neighbour(delta))
}

const suggestions = ref<string[]>([])

async function loadSuggestions() {
  if (q.value.length < 2) {
    suggestions.value = []
    return
  }
  try {
    const res = await fetchSuggestions(q.value)
    suggestions.value = res.suggestions
  } catch {
    suggestions.value = []
  }
}

onMounted(() => {
  void fetchSavedSearches().catch(() => undefined)
  if (q.value) void submit()
})
</script>

<template>
  <main class="flex h-full flex-col p-4">
    <form
      class="flex gap-2"
      @submit.prevent="submit"
    >
      <input
        id="search-query-input"
        v-model="q"
        class="flex-1 rounded-lg border border-border bg-surface-elevated px-3 py-2"
        :placeholder="t('search.placeholder')"
        type="search"
        @input="loadSuggestions"
      >
      <ul
        v-if="suggestions.length"
        class="absolute z-10 mt-10 max-h-40 w-full overflow-auto rounded-lg border border-border bg-surface-elevated shadow"
      >
        <li
          v-for="s in suggestions"
          :key="s"
        >
          <button
            class="block w-full px-3 py-2 text-left text-sm hover:bg-surface"
            type="button"
            @click="q = s; suggestions = []; void submit()"
          >
            {{ s }}
          </button>
        </li>
      </ul>
      <button
        class="rounded-lg bg-accent px-4 py-2 text-white"
        type="submit"
      >
        {{ t('search.submit') }}
      </button>
    </form>
    <div class="mt-2 flex flex-wrap gap-1">
      <span
        v-for="chip in chips"
        :key="chip"
        class="rounded-full bg-surface-elevated px-2 py-0.5 text-xs"
      >
        {{ chip }}
      </span>
    </div>
    <p
      v-if="error"
      class="mt-4 text-danger"
    >
      {{ error }}
    </p>
    <ul class="mt-4 grid grid-cols-4 gap-2">
      <li
        v-for="asset in assets"
        :key="asset.id"
      >
        <button
          class="block w-full"
          @click="lightbox.open(asset)"
        >
          <img
            v-if="asset.content_hash"
            :src="thumbSrc(asset.content_hash)"
            :alt="asset.filename"
            class="h-32 w-full object-cover"
          >
          <span
            v-else
            class="block truncate text-sm"
          >{{ asset.filename }}</span>
        </button>
      </li>
    </ul>
    <AssetViewer
      v-if="lightbox.viewing.value"
      :asset="lightbox.viewing.value"
      :prev="neighbour(-1)"
      :next="neighbour(1)"
      @close="lightbox.close"
      @prev="stepViewer(-1)"
      @next="stepViewer(1)"
    />
  </main>
</template>
