<script setup lang="ts">
// **Real thumbnail, not a gradient placeholder**: items in the trash ARE
// real catalog photos (`status='trashed'`, not removed from the `assets`
// table until "Delete permanently" is chosen): `GET /assets/{id}` still
// finds them, with valid `content_hash`/`thumbhash`. Showing a gradient
// instead of the real photo would make it impossible to recognize what
// is about to be restored or permanently deleted (no filename is ever
// shown here). Same N+1 pattern used elsewhere: one `fetchAsset` per
// item, few items expected in a trash can.
//
// **"<N> days remaining" is a real countdown**: the backend computes
// `days_remaining` from the real `deleted_at` plus 30 days (see
// `api/trash.ts`).
//
// **"Empty trash" and "Delete permanently" are real, permanent actions
// with no confirmation dialog, no success toast, and no way to undo** —
// this is the intended behavior. A network error is still reported
// (toast), because on the real backend these calls can genuinely fail
// (403/409/500).
//
// Keyboard accessibility: the two per-tile buttons are real
// `<button>` elements, revealed by `:focus-within` as well as `:hover`,
// consistent with the rest of the app.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { deleteAsset } from '@/api/culling'
import { ApiProblem, isUnauthenticated } from '@/api/client'
import { thumbSrc as mediaThumbSrc } from '@/api/media'
import { fetchAsset, type TimelineAsset } from '@/api/timeline'
import { emptyTrash, fetchTrash, restoreAsset, type TrashedItem } from '@/api/trash'
import ErrorState from '@/components/ui/ErrorState.vue'
import { classifyError } from '@/errors/classify'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

const { t } = useI18n()
const router = useRouter()
const session = useSessionStore()
const toast = useToastStore()

const items = ref<TrashedItem[]>([])
const assetsById = ref<Record<string, TimelineAsset>>({})
const loaded = ref(false)
const loadError = ref<unknown>(null)
const emptying = ref(false)
const pending = ref<Set<string>>(new Set())

const errorNature = computed(() => (loadError.value ? classifyError(loadError.value) : null))
const errorDetail = computed(() =>
  loadError.value instanceof ApiProblem ? `${loadError.value.type} · ${loadError.value.status}` : undefined
)

async function load() {
  loadError.value = null
  loaded.value = false
  try {
    const collected: TrashedItem[] = []
    let cursor: string | undefined
    do {
      const page = await fetchTrash(cursor)
      collected.push(...page.items)
      cursor = page.next_cursor
    } while (cursor)
    items.value = collected
    const pairs = await Promise.all(
      collected.map(async (item) => [item.asset_id, await fetchAsset(item.asset_id).catch(() => null)] as const)
    )
    assetsById.value = Object.fromEntries(pairs.filter((pair): pair is [string, TimelineAsset] => pair[1] !== null))
    loaded.value = true
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = error
  }
}

onMounted(load)

function thumbnailUrl(item: TrashedItem): string | undefined {
  const asset = assetsById.value[item.asset_id]
  return asset?.content_hash ? mediaThumbSrc(asset.content_hash) : undefined
}

function placeholderUrl(item: TrashedItem): string | undefined {
  const hash = assetsById.value[item.asset_id]?.thumbhash
  return hash ? (thumbhashToDataURL(hash) ?? undefined) : undefined
}

async function restore(item: TrashedItem) {
  if (pending.value.has(item.asset_id)) return
  pending.value = new Set(pending.value).add(item.asset_id)
  try {
    await restoreAsset(item.asset_id)
    items.value = items.value.filter((i) => i.id !== item.id)
  } catch {
    toast.showError(t('trash.actionError'))
  } finally {
    const next = new Set(pending.value)
    next.delete(item.asset_id)
    pending.value = next
  }
}

async function purge(item: TrashedItem) {
  if (pending.value.has(item.asset_id)) return
  pending.value = new Set(pending.value).add(item.asset_id)
  try {
    await deleteAsset(item.asset_id, 'purged')
    items.value = items.value.filter((i) => i.id !== item.id)
  } catch {
    toast.showError(t('trash.actionError'))
  } finally {
    const next = new Set(pending.value)
    next.delete(item.asset_id)
    pending.value = next
  }
}

async function emptyAll() {
  if (emptying.value) return
  emptying.value = true
  try {
    await emptyTrash()
    items.value = []
  } catch {
    toast.showError(t('trash.actionError'))
  } finally {
    emptying.value = false
  }
}
</script>

<template>
  <main class="flex h-full flex-col">
    <ErrorState
      v-if="errorNature"
      :nature="errorNature"
      :technical-detail="errorDetail"
      @retry="load"
    />
    <div
      v-else-if="loaded && items.length === 0"
      class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
    >
      <p class="text-sm font-semibold">
        {{ t('trash.emptyTitle') }}
      </p>
      <p class="max-w-[380px] text-sm text-content-muted">
        {{ t('trash.emptySubtitle') }}
      </p>
    </div>
    <template v-else>
      <div class="flex items-center justify-between border-b border-border px-4 py-3">
        <div>
          <p class="text-[15px] font-bold">
            {{ t('trash.title') }}
          </p>
          <p class="text-sm text-content-muted">
            {{ t('trash.subtitle', { n: items.length }, { plural: items.length }) }}
          </p>
        </div>
        <button
          type="button"
          class="flex items-center gap-1.5 rounded-lg border border-danger px-3.5 py-2 text-[13px] font-semibold text-danger hover:bg-danger/10 disabled:opacity-50"
          :disabled="emptying"
          @click="emptyAll"
        >
          {{ t('trash.emptyAll') }}
        </button>
      </div>

      <div
        class="grid gap-3 p-4"
        style="grid-template-columns: repeat(auto-fill, minmax(140px, 1fr))"
      >
        <div
          v-for="item in items"
          :key="item.id"
          class="group relative aspect-square overflow-hidden rounded-[5px] border border-border"
        >
          <img
            v-if="thumbnailUrl(item)"
            :src="thumbnailUrl(item)"
            :alt="''"
            class="absolute inset-0 h-full w-full object-cover"
            loading="lazy"
          >
          <img
            v-else-if="placeholderUrl(item)"
            :src="placeholderUrl(item)"
            :alt="''"
            class="absolute inset-0 h-full w-full object-cover"
          >
          <div
            v-else
            class="absolute inset-0 bg-border/30"
          />

          <span
            class="absolute bottom-[2.5px] left-[2.5px] right-[2.5px] rounded-md bg-black/60 py-0.5 text-center text-[9.5px] font-bold text-white"
          >
            {{ t('trash.daysRemaining', { n: item.days_remaining }, { plural: item.days_remaining }) }}
          </span>

          <div
            class="absolute inset-0 flex items-center justify-center gap-1.5 bg-black/45 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
          >
            <button
              type="button"
              :aria-label="t('trash.restore')"
              :title="t('trash.restore')"
              :disabled="pending.has(item.asset_id)"
              class="flex h-[26px] w-[26px] items-center justify-center rounded-full bg-white text-content disabled:opacity-50"
              @click.stop="restore(item)"
            >
              ↻
            </button>
            <button
              type="button"
              :aria-label="t('trash.purge')"
              :title="t('trash.purge')"
              :disabled="pending.has(item.asset_id)"
              class="flex h-[26px] w-[26px] items-center justify-center rounded-full bg-white text-danger disabled:opacity-50"
              @click.stop="purge(item)"
            >
              🗑
            </button>
          </div>
        </div>
      </div>
    </template>
  </main>
</template>
