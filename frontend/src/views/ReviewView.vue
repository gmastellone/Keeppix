<script setup lang="ts">
// Fase 11 Task 15 (2/N) — documento funzionale §56 "Revisione — tag" e
// SP-10 (righe 8216-8397), verificato riga per riga.
//
// **Solo la coda Tag, non ancora il selettore di tab**: il documento
// descrive un'unica pagina a due schede (Tag/Volti), ma "Volti" è
// esplicitamente Task 16 (Tranche D, Persone) — costruire qui una scheda
// che porta a una coda volti ancora inesistente sarebbe un collegamento
// morto. La scheda arriva con Task 16, quando SP-10 avrà davvero due code
// da condividere: fino ad allora questa è solo la coda Tag.
//
// **Raggruppamento client-side**: `GET /tags/proposals` (senza `tag_id`)
// torna un elenco piatto ordinato per punteggio — nessuna rotta raggruppa
// per tag con un conteggio pronto (verificato: `TagView.assignment_count`
// conta ogni stato, non solo le proposte). Raggruppato qui da
// `Proposal.tag_id`, colore preso da un secondo `fetchTags()` (la
// proposta non porta il colore del tag, solo il nome).
//
// **Miniature reali, non finte**: `ProposalView` non porta `content_hash`/
// `thumbhash` (solo `asset_id`/`tag_id`/`tag_name`/`score`/`filename`/
// `taken_at_utc`) — nessuna rotta di card per una coda di revisione
// esiste. Un `fetchAsset(id)` per ogni asset **unico** coinvolto (non per
// riga: una foto può comparire in più gruppi se ha più proposte) recupera
// `content_hash`/`thumbhash` reali, stesso identico dato che alimenta
// `FlatAssetGrid.vue`. Nessuna paginazione: il documento stesso non ne
// ha ("non esiste paginazione o 'mostra altre N'").
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { thumbSrc } from '@/api/media'
import {
  confirmAllTagProposals,
  confirmTagProposal,
  fetchTagProposals,
  fetchTags,
  rejectAllTagProposals,
  rejectTagProposal,
  type Proposal
} from '@/api/tags'
import { fetchAsset } from '@/api/timeline'
import { thumbhashToDataURL } from '@/timeline/thumbhash'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const toast = useToastStore()

const proposals = ref<Proposal[]>([])
const tagColors = ref<Record<string, string | null>>({})
const thumbs = ref<Record<string, { contentHash: string | null; thumbhash: string | null }>>({})
const loading = ref(true)
const busyGroups = ref<Set<string>>(new Set())
const busyItems = ref<Set<string>>(new Set())

function itemKey(p: Proposal): string {
  return `${p.tag_id}:${p.asset_id}`
}

async function load() {
  loading.value = true
  try {
    const [proposalRows, tags] = await Promise.all([fetchTagProposals(), fetchTags()])
    proposals.value = proposalRows
    tagColors.value = Object.fromEntries(tags.filter((tg) => tg.kind === 'tag').map((tg) => [tg.id, tg.color]))

    const uniqueAssetIds = [...new Set(proposalRows.map((p) => p.asset_id))]
    const assets = await Promise.all(uniqueAssetIds.map((id) => fetchAsset(id).catch(() => null)))
    thumbs.value = Object.fromEntries(
      assets
        .filter((a): a is NonNullable<typeof a> => a !== null)
        .map((a) => [a.id, { contentHash: a.content_hash, thumbhash: a.thumbhash }])
    )
  } catch {
    toast.showError(t('review.loadError'))
  } finally {
    loading.value = false
  }
}

onMounted(load)

interface Group {
  tagId: string
  tagName: string
  color: string | null
  items: Proposal[]
}

const groups = computed<Group[]>(() => {
  const byTag = new Map<string, Group>()
  for (const p of proposals.value) {
    let group = byTag.get(p.tag_id)
    if (!group) {
      group = { tagId: p.tag_id, tagName: p.tag_name, color: tagColors.value[p.tag_id] ?? null, items: [] }
      byTag.set(p.tag_id, group)
    }
    group.items.push(p)
  }
  return [...byTag.values()]
})

const totalCount = computed(() => proposals.value.length)

function thumbStyle(assetId: string): Record<string, string> {
  const entry = thumbs.value[assetId]
  if (entry?.contentHash) return {}
  const placeholder = entry?.thumbhash ? thumbhashToDataURL(entry.thumbhash) : undefined
  return placeholder ? { background: `center / cover url(${placeholder})` } : {}
}

function thumbImgSrc(assetId: string): string | undefined {
  const hash = thumbs.value[assetId]?.contentHash
  return hash ? thumbSrc(hash) : undefined
}

async function confirmOne(p: Proposal) {
  const key = itemKey(p)
  if (busyItems.value.has(key)) return
  busyItems.value = new Set(busyItems.value).add(key)
  try {
    await confirmTagProposal(p.tag_id, p.asset_id)
    proposals.value = proposals.value.filter((x) => itemKey(x) !== key)
    toast.show(t('review.confirmedOne'))
  } catch {
    toast.showError(t('review.actionError'))
  } finally {
    const next = new Set(busyItems.value)
    next.delete(key)
    busyItems.value = next
  }
}

async function rejectOne(p: Proposal) {
  const key = itemKey(p)
  if (busyItems.value.has(key)) return
  busyItems.value = new Set(busyItems.value).add(key)
  try {
    await rejectTagProposal(p.tag_id, p.asset_id)
    proposals.value = proposals.value.filter((x) => itemKey(x) !== key)
    toast.show(t('review.rejectedOne'))
  } catch {
    toast.showError(t('review.actionError'))
  } finally {
    const next = new Set(busyItems.value)
    next.delete(key)
    busyItems.value = next
  }
}

async function confirmGroup(group: Group) {
  if (busyGroups.value.has(group.tagId)) return
  busyGroups.value = new Set(busyGroups.value).add(group.tagId)
  try {
    await confirmAllTagProposals(group.tagId)
    proposals.value = proposals.value.filter((p) => p.tag_id !== group.tagId)
    toast.show(t('review.confirmedAll', { n: group.items.length }, { plural: group.items.length }))
  } catch {
    toast.showError(t('review.actionError'))
  } finally {
    const next = new Set(busyGroups.value)
    next.delete(group.tagId)
    busyGroups.value = next
  }
}

async function rejectGroup(group: Group) {
  if (busyGroups.value.has(group.tagId)) return
  busyGroups.value = new Set(busyGroups.value).add(group.tagId)
  try {
    await rejectAllTagProposals(group.tagId)
    proposals.value = proposals.value.filter((p) => p.tag_id !== group.tagId)
    toast.show(t('review.rejectedAll', { n: group.items.length }, { plural: group.items.length }))
  } catch {
    toast.showError(t('review.actionError'))
  } finally {
    const next = new Set(busyGroups.value)
    next.delete(group.tagId)
    busyGroups.value = next
  }
}
</script>

<template>
  <main class="mx-auto max-w-[720px] p-6">
    <p class="text-[15px] font-bold">
      {{ t('review.title') }}
    </p>

    <template v-if="!loading">
      <p
        v-if="totalCount > 0"
        class="mt-1 text-[12.5px] text-content-muted"
      >
        {{ t('review.subtitle', { n: totalCount }, { plural: totalCount }) }}
      </p>

      <section
        v-for="group in groups"
        :key="group.tagId"
        class="mt-5 rounded-xl border border-border p-3.5"
      >
        <div class="flex items-center gap-2">
          <span
            class="h-[9px] w-[9px] shrink-0 rounded-full"
            aria-hidden="true"
            :style="{ background: group.color ?? 'var(--color-border-strong)' }"
          />
          <p class="text-[13.5px] font-bold">
            «{{ group.tagName }}»
          </p>
          <span class="text-[12px] text-content-muted">{{ t('review.proposalCount', { n: group.items.length }, { plural: group.items.length }) }}</span>
          <div class="ml-auto flex gap-1.5">
            <button
              type="button"
              class="rounded-lg border border-border px-2.5 py-1 text-[12px] font-semibold hover:bg-border/20 disabled:opacity-60"
              :disabled="busyGroups.has(group.tagId)"
              @click="confirmGroup(group)"
            >
              {{ t('review.confirmAll') }}
            </button>
            <button
              type="button"
              class="rounded-lg px-2.5 py-1 text-[12px] font-semibold text-content-muted hover:bg-border/20 disabled:opacity-60"
              :disabled="busyGroups.has(group.tagId)"
              @click="rejectGroup(group)"
            >
              {{ t('review.rejectAll') }}
            </button>
          </div>
        </div>
        <div class="mt-3 flex flex-wrap gap-2">
          <div
            v-for="p in group.items"
            :key="itemKey(p)"
            class="group/thumb relative h-[74px] w-[74px] shrink-0 overflow-hidden rounded-lg border-[1.5px] border-dashed border-accent opacity-90"
            :style="thumbStyle(p.asset_id)"
          >
            <img
              v-if="thumbImgSrc(p.asset_id)"
              :src="thumbImgSrc(p.asset_id)"
              :alt="p.filename"
              class="h-full w-full object-cover"
            >
            <span class="absolute left-1 top-1 rounded bg-accent/20 px-1 py-0.5 text-[8.5px] font-bold text-accent">{{ t('review.aiBadge') }}</span>
            <div
              class="absolute inset-0 flex items-center justify-center gap-1.5 bg-black/50 opacity-0 transition-opacity
                     group-hover/thumb:opacity-100 group-focus-within/thumb:opacity-100"
              :style="{ transitionDuration: 'var(--duration-fast)' }"
            >
              <button
                type="button"
                class="flex h-[26px] w-[26px] items-center justify-center rounded-full bg-white text-[13px] font-bold text-[#111] disabled:opacity-60"
                :disabled="busyItems.has(itemKey(p))"
                :aria-label="t('review.confirmOne')"
                @click="confirmOne(p)"
              >
                ✓
              </button>
              <button
                type="button"
                class="flex h-[26px] w-[26px] items-center justify-center rounded-full bg-white text-[13px] font-bold text-danger disabled:opacity-60"
                :disabled="busyItems.has(itemKey(p))"
                :aria-label="t('review.rejectOne')"
                @click="rejectOne(p)"
              >
                ✕
              </button>
            </div>
          </div>
        </div>
      </section>

      <div
        v-if="totalCount === 0"
        class="mt-10 flex flex-col items-center gap-1.5 text-center"
      >
        <p class="text-[13.5px] font-semibold">
          {{ t('review.emptyTitle') }}
        </p>
        <p class="max-w-[380px] text-[12.5px] text-content-muted">
          {{ t('review.emptyText') }}
        </p>
      </div>
    </template>
  </main>
</template>
