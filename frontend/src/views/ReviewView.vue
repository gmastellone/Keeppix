<script setup lang="ts">
// Fase 11 Task 15 (2/N) + Task 16 (5/N) — documento funzionale §56
// "Revisione — tag" e §39 "Revisione — volti" (SP-10, righe 8216-8397 e
// 6051-6173), entrambe verificate riga per riga.
//
// **Selettore di tab condiviso (§39.3 controllo 1)**: riusa
// `SegmentedControl.vue` (SP-24) — già `role="radiogroup"` con roving
// tabindex, esattamente il pattern richiesto ("la linguetta attiva ha
// `tabindex=0`, l'altra `-1`"), le frecce sono un'aggiunta reale già
// presente lì (Task 2), non nuova qui.
//
// **Le due code restano modelli separati, non unificati**: `Proposal`
// (tag, `api/tags.ts`) e `Face` (volti, `api/faces.ts`) sono forme
// diverse — stato/computed/azioni duplicati apposta invece di
// un'astrazione prematura su due domini che condividono solo il
// "pattern coda", non i campi.
//
// **§39: tre azioni, non due, e le due negazioni sono concettualmente
// diverse** (§38, stesso punto già annotato in `AssetViewer.vue`):
// - **Conferma** — `POST /faces/{id}/confirm`.
// - **"Rifiuta" (la ✕, ex "non è <nome persona>")** — il volto è vero,
//   l'attribuzione no: diventa una **persona nuova senza nome**
//   (`createPerson('')` + `assignFace`, composto qui — nessuna rotta fa
//   questo in un colpo solo, stesso schema già di "Correggi persona…"/
//   "+ aggiungi" altrove in questa fase).
// - **"Non è un volto" (il cestino)** — falso positivo permanente,
//   `POST /faces/{id}/reject` (`api/faces.ts#rejectFace`, già reale dal
//   Task 8).
//
// **"Rifiuta tutte" NON usa la rotta bulk reale** (`POST /persons/{id}/
// proposals/reject`): letta la sua implementazione
// (`crates/keeppix-db/src/faces.rs::reject_all_proposed_for_person`,
// righe 631-664) applica la stessa semantica permanente di
// `FaceRepo::reject`, non "ogni volto diventa una persona nuova senza
// nome" come vuole il documento (§39, righe 6149-6154: "rifiutare in
// blocco 14 proposte crea 14 persone nuove senza nome" — un effetto
// collaterale che il documento stesso segnala di verificare col
// committente). Vero disallineamento fra documento e backend, non
// un'invenzione: composto qui invece, un `createPerson('')`+`assignFace`
// per ciascun volto del gruppo — stessa semantica del "Rifiuta" singolo,
// scalata. Annotato per l'architetto, non "corretto" nel backend (fuori
// scope per un task di sola interfaccia).
//
// **Nessuna azione "Non è un volto" in blocco** (§39.3: "è deliberato:
// è l'azione permanente") — non costruita.
//
// **Miniature reali**: `FaceView` non porta `content_hash`/`thumbhash`
// (solo `id`/`asset_id`/`bbox`/`person_id`/`proposed_person_id`/
// `proposed_score`/`assigned_by_human`) — stesso `fetchAsset(id)` per
// asset unico già usato per i tag.
//
// **Nome della persona suggerita**: `Face` porta solo `proposed_person_id`
// (un id), non il nome — risolto da un `fetchPersons()` a parte
// (nessuna seconda rotta dedicata, stesso principio del colore dei tag).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import {
  assignFace,
  confirmAllFaceProposals,
  confirmFaceProposal,
  fetchFaceProposals,
  rejectFace,
  type Face
} from '@/api/faces'
import { thumbSrc } from '@/api/media'
import { createPerson, fetchPersons } from '@/api/persons'
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
import SegmentedControl from '@/components/ui/SegmentedControl.vue'
import { thumbhashToDataURL } from '@/timeline/thumbhash'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const toast = useToastStore()

const activeTab = ref<'tag' | 'faces'>('tag')

const proposals = ref<Proposal[]>([])
const tagColors = ref<Record<string, string | null>>({})
const thumbs = ref<Record<string, { contentHash: string | null; thumbhash: string | null }>>({})
const loading = ref(true)
const busyGroups = ref<Set<string>>(new Set())
const busyItems = ref<Set<string>>(new Set())

const faceProposals = ref<Face[]>([])
const personNames = ref<Record<string, string | null>>({})
const faceThumbs = ref<Record<string, { contentHash: string | null; thumbhash: string | null }>>({})
const faceBusyGroups = ref<Set<string>>(new Set())
const faceBusyItems = ref<Set<string>>(new Set())

function itemKey(p: Proposal): string {
  return `${p.tag_id}:${p.asset_id}`
}

async function loadThumbs(assetIds: string[]): Promise<Record<string, { contentHash: string | null; thumbhash: string | null }>> {
  const unique = [...new Set(assetIds)]
  const assets = await Promise.all(unique.map((id) => fetchAsset(id).catch(() => null)))
  return Object.fromEntries(
    assets
      .filter((a): a is NonNullable<typeof a> => a !== null)
      .map((a) => [a.id, { contentHash: a.content_hash, thumbhash: a.thumbhash }])
  )
}

async function load() {
  loading.value = true
  try {
    const [proposalRows, tags, faceRows, persons] = await Promise.all([
      fetchTagProposals(),
      fetchTags(),
      fetchFaceProposals(),
      fetchPersons(true)
    ])
    proposals.value = proposalRows
    tagColors.value = Object.fromEntries(tags.filter((tg) => tg.kind === 'tag').map((tg) => [tg.id, tg.color]))
    faceProposals.value = faceRows
    personNames.value = Object.fromEntries(persons.map((p) => [p.id, p.name]))

    const [tagThumbs, faceAssetThumbs] = await Promise.all([
      loadThumbs(proposalRows.map((p) => p.asset_id)),
      loadThumbs(faceRows.map((f) => f.asset_id))
    ])
    thumbs.value = tagThumbs
    faceThumbs.value = faceAssetThumbs
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

interface FaceGroup {
  personId: string
  personName: string | null
  items: Face[]
}

const faceGroups = computed<FaceGroup[]>(() => {
  const byPerson = new Map<string, FaceGroup>()
  for (const f of faceProposals.value) {
    if (!f.proposed_person_id) continue
    let group = byPerson.get(f.proposed_person_id)
    if (!group) {
      group = { personId: f.proposed_person_id, personName: personNames.value[f.proposed_person_id] ?? null, items: [] }
      byPerson.set(f.proposed_person_id, group)
    }
    group.items.push(f)
  }
  return [...byPerson.values()]
})

const faceTotalCount = computed(() => faceProposals.value.length)

function personDisplayName(name: string | null): string {
  return name?.trim() || t('persons.unnamed')
}

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

function faceThumbStyle(assetId: string): Record<string, string> {
  const entry = faceThumbs.value[assetId]
  if (entry?.contentHash) return {}
  const placeholder = entry?.thumbhash ? thumbhashToDataURL(entry.thumbhash) : undefined
  return placeholder ? { background: `center / cover url(${placeholder})` } : {}
}

function faceThumbImgSrc(assetId: string): string | undefined {
  const hash = faceThumbs.value[assetId]?.contentHash
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

async function confirmOneFace(f: Face) {
  if (faceBusyItems.value.has(f.id)) return
  faceBusyItems.value = new Set(faceBusyItems.value).add(f.id)
  try {
    await confirmFaceProposal(f.id)
    faceProposals.value = faceProposals.value.filter((x) => x.id !== f.id)
    toast.show(t('review.faceConfirmedOne'))
  } catch {
    toast.showError(t('review.actionError'))
  } finally {
    const next = new Set(faceBusyItems.value)
    next.delete(f.id)
    faceBusyItems.value = next
  }
}

/** "Rifiuta" (§39.3 controllo 5): l'attribuzione è sbagliata, il volto
 * resta un volto — nasce una persona nuova senza nome. */
async function rejectOneFace(f: Face) {
  if (faceBusyItems.value.has(f.id)) return
  faceBusyItems.value = new Set(faceBusyItems.value).add(f.id)
  try {
    const newPerson = await createPerson('')
    await assignFace(f.id, newPerson.id)
    faceProposals.value = faceProposals.value.filter((x) => x.id !== f.id)
    toast.show(t('review.faceRejectedOne'))
  } catch {
    toast.showError(t('review.actionError'))
  } finally {
    const next = new Set(faceBusyItems.value)
    next.delete(f.id)
    faceBusyItems.value = next
  }
}

async function notAFace(f: Face) {
  if (faceBusyItems.value.has(f.id)) return
  faceBusyItems.value = new Set(faceBusyItems.value).add(f.id)
  try {
    await rejectFace(f.id)
    faceProposals.value = faceProposals.value.filter((x) => x.id !== f.id)
    toast.show(t('review.notAFaceToast'))
  } catch {
    toast.showError(t('review.actionError'))
  } finally {
    const next = new Set(faceBusyItems.value)
    next.delete(f.id)
    faceBusyItems.value = next
  }
}

async function confirmFaceGroup(group: FaceGroup) {
  if (faceBusyGroups.value.has(group.personId)) return
  faceBusyGroups.value = new Set(faceBusyGroups.value).add(group.personId)
  try {
    await confirmAllFaceProposals(group.personId)
    faceProposals.value = faceProposals.value.filter((f) => f.proposed_person_id !== group.personId)
    toast.show(t('review.faceConfirmedAll', { n: group.items.length }, { plural: group.items.length }))
  } catch {
    toast.showError(t('review.actionError'))
  } finally {
    const next = new Set(faceBusyGroups.value)
    next.delete(group.personId)
    faceBusyGroups.value = next
  }
}

/** "Rifiuta tutte" (§39.3 controllo 3): vedi il commento di testa del
 * file — composto, non la rotta bulk reale (semantica diversa lì). Ogni
 * volto del gruppo diventa una **propria** persona nuova senza nome. */
async function rejectFaceGroup(group: FaceGroup) {
  if (faceBusyGroups.value.has(group.personId)) return
  faceBusyGroups.value = new Set(faceBusyGroups.value).add(group.personId)
  try {
    await Promise.all(
      group.items.map(async (f) => {
        const newPerson = await createPerson('')
        await assignFace(f.id, newPerson.id)
      })
    )
    faceProposals.value = faceProposals.value.filter((f) => f.proposed_person_id !== group.personId)
    toast.show(t('review.faceRejectedAll', { n: group.items.length }, { plural: group.items.length }))
  } catch {
    toast.showError(t('review.actionError'))
  } finally {
    const next = new Set(faceBusyGroups.value)
    next.delete(group.personId)
    faceBusyGroups.value = next
  }
}
</script>

<template>
  <main class="mx-auto max-w-[720px] p-6">
    <SegmentedControl
      v-model="activeTab"
      :options="[
        { value: 'tag', label: t('review.tabTag') },
        { value: 'faces', label: faceTotalCount > 0 ? t('review.tabFacesCount', { n: faceTotalCount }) : t('review.tabFaces') }
      ]"
      :aria-label="t('review.tabsAriaLabel')"
    />

    <p class="mt-3 text-[15px] font-bold">
      {{ activeTab === 'tag' ? t('review.title') : t('review.facesTitle') }}
    </p>

    <template v-if="!loading">
      <template v-if="activeTab === 'tag'">
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

      <template v-else>
        <p
          v-if="faceTotalCount > 0"
          class="mt-1 text-[12.5px] text-content-muted"
        >
          {{ t('review.facesSubtitle', { n: faceTotalCount }, { plural: faceTotalCount }) }}
        </p>

        <section
          v-for="group in faceGroups"
          :key="group.personId"
          class="mt-5 rounded-xl border border-border p-3.5"
        >
          <div class="flex items-center gap-2">
            <p class="text-[13.5px]">
              {{ t('review.facesGroupPrefix') }} <b class="font-bold">{{ personDisplayName(group.personName) }}</b>
            </p>
            <span class="text-[12px] text-content-muted">{{ t('review.proposalCount', { n: group.items.length }, { plural: group.items.length }) }}</span>
            <div class="ml-auto flex gap-1.5">
              <button
                type="button"
                class="rounded-lg border border-border px-2.5 py-1 text-[12px] font-semibold hover:bg-border/20 disabled:opacity-60"
                :disabled="faceBusyGroups.has(group.personId)"
                @click="confirmFaceGroup(group)"
              >
                {{ t('review.confirmAll') }}
              </button>
              <button
                type="button"
                class="rounded-lg px-2.5 py-1 text-[12px] font-semibold text-content-muted hover:bg-border/20 disabled:opacity-60"
                :disabled="faceBusyGroups.has(group.personId)"
                @click="rejectFaceGroup(group)"
              >
                {{ t('review.rejectAll') }}
              </button>
            </div>
          </div>
          <div class="mt-3 flex flex-wrap gap-2">
            <div
              v-for="f in group.items"
              :key="f.id"
              class="group/thumb relative h-[86px] w-[86px] shrink-0 overflow-hidden rounded-lg border-[1.5px] border-dashed border-accent opacity-90"
              :style="faceThumbStyle(f.asset_id)"
            >
              <img
                v-if="faceThumbImgSrc(f.asset_id)"
                :src="faceThumbImgSrc(f.asset_id)"
                alt=""
                class="h-full w-full object-cover"
              >
              <span class="absolute left-1 top-1 rounded bg-accent/20 px-1 py-0.5 text-[8.5px] font-bold text-accent">{{ t('review.aiBadge') }}</span>
              <div
                class="absolute inset-0 flex items-center justify-center gap-1 bg-black/50 opacity-0 transition-opacity
                       group-hover/thumb:opacity-100 group-focus-within/thumb:opacity-100"
                :style="{ transitionDuration: 'var(--duration-fast)' }"
              >
                <button
                  type="button"
                  class="flex h-[24px] w-[24px] items-center justify-center rounded-full bg-white text-[12px] font-bold text-[#111] disabled:opacity-60"
                  :disabled="faceBusyItems.has(f.id)"
                  :aria-label="t('review.confirmOne')"
                  @click="confirmOneFace(f)"
                >
                  ✓
                </button>
                <button
                  type="button"
                  class="flex h-[24px] w-[24px] items-center justify-center rounded-full bg-white text-[12px] font-bold text-danger disabled:opacity-60"
                  :disabled="faceBusyItems.has(f.id)"
                  :aria-label="t('review.rejectFaceOne', { name: personDisplayName(group.personName) })"
                  @click="rejectOneFace(f)"
                >
                  ✕
                </button>
                <button
                  type="button"
                  class="flex h-[24px] w-[24px] items-center justify-center rounded-full bg-danger text-[12px] font-bold text-white disabled:opacity-60"
                  :disabled="faceBusyItems.has(f.id)"
                  :aria-label="t('review.notAFaceOne')"
                  @click="notAFace(f)"
                >
                  🗑
                </button>
              </div>
            </div>
          </div>
        </section>

        <div
          v-if="faceTotalCount === 0"
          class="mt-10 flex flex-col items-center gap-1.5 text-center"
        >
          <p class="text-[13.5px] font-semibold">
            {{ t('review.facesEmptyTitle') }}
          </p>
          <p class="max-w-[380px] text-[12.5px] text-content-muted">
            {{ t('review.facesEmptyText') }}
          </p>
        </div>
      </template>
    </template>
  </main>
</template>
