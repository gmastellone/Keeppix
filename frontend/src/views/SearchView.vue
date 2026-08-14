<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { runSearch } from '@/api/library'
import type { TimelineAsset } from '@/api/timeline'
import { parseSearch } from '@/search/parse'

import AssetViewer from '@/components/AssetViewer.vue'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const q = ref(typeof route.query.q === 'string' ? route.query.q : '')
const assets = ref<TimelineAsset[]>([])
const error = ref('')
const viewing = ref<TimelineAsset | null>(null)

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
    const page = await runSearch(parseSearch(q.value))
    assets.value = page.assets
  } catch {
    error.value = t('search.invalid')
  }
}

onMounted(() => {
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
        v-model="q"
        class="flex-1 rounded-lg border border-border bg-surface-elevated px-3 py-2"
        :placeholder="t('search.placeholder')"
        type="search"
      >
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
          @click="viewing = asset"
        >
          <img
            v-if="asset.content_hash"
            :src="`/media/thumb/${asset.content_hash}`"
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
      v-if="viewing"
      :asset="viewing"
      @close="viewing = null"
    />
  </main>
</template>
