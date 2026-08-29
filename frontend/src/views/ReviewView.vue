<script setup lang="ts">
// Covers both the tag review queue and the faces review queue.
//
// **Shared tab selector**: reuses `SegmentedControl.vue` — already
// `role="radiogroup"` with roving tabindex (the active tab has
// `tabindex=0`, the other `-1`), the arrow-key navigation is a real
// feature already present there, not new here.
//
// **The two queues remain separate models, not unified**: `Proposal`
// (tags, `api/tags.ts`) and `Face` (faces, `api/faces.ts`) are different
// shapes — state/computed/actions are duplicated on purpose instead of
// a premature abstraction over two domains that only share the "queue
// pattern", not the fields.
//
// **Three actions, not two, and the two negative ones are conceptually
// different**:
// - **Confirm** — `POST /faces/{id}/confirm`.
// - **"Reject" (the ✕, formerly "not <person name>")** — the face is
//   real, the attribution isn't: it becomes a **new, unnamed person**
//   (`createPerson('')` + `assignFace`, composed here — no route does
//   this in one call, the same pattern already used by "Correct
//   person…"/"+ add" elsewhere).
// - **"Not a face" (the trash icon)** — a permanent false positive,
//   `POST /faces/{id}/reject` (`api/faces.ts#rejectFace`).
//
// **"Reject all" does NOT use the real bulk route**
// (`POST /persons/{id}/proposals/reject`): reading its implementation
// (`crates/keeppix-db/src/faces.rs::reject_all_proposed_for_person`)
// shows it applies the same permanent semantics as `FaceRepo::reject`,
// not "every face becomes a new unnamed person" as intended — a real
// mismatch between intent and backend, not an invention here: composed
// instead, one `createPerson('')`+`assignFace` per face in the group —
// the same semantics as the single "Reject", scaled up. Noted for
// whoever maintains the backend, not "fixed" there (out of scope for a
// UI-only task).
//
// **No bulk "Not a face" action** — deliberately not built, since it's
// the permanent action.
//
// **Real thumbnails**: `FaceView` carries no `content_hash`/`thumbhash`
// (only `id`/`asset_id`/`bbox`/`person_id`/`proposed_person_id`/
// `proposed_score`/`assigned_by_human`) — same per-asset `fetchAsset(id)`
// already used for tags.
//
// **Suggested person's name**: `Face` only carries `proposed_person_id`
// (an id), not the name — resolved via a separate `fetchPersons()` call
// (no dedicated second route, same principle as tag color).
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

/** "Reject": the attribution is wrong, the face stays a face — a new
 * unnamed person is created. */
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

/** "Reject all": see the file's header comment — composed, not the real
 * bulk route (different semantics there). Every face in the group
 * becomes its **own** new unnamed person. */
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
