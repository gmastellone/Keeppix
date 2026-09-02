<script setup lang="ts">
// **Still out of scope here**: the review-queue banner (no real "Faces"
// tab in `ReviewView.vue` yet — a link today would be a dead end).
//
// **`visiblePeople()`**: people that are not hidden (`fetchPersons()`
// without `include_hidden`, already the route's behavior) *and* have
// at least one confirmed face (`face_count > 0` — a client-side filter,
// the route does not apply it, per the original comment in `persons.ts`).
//
// **Group blocks, but no `groupId` on the person**: `PersonView` does
// not carry membership — only `GET /person-groups/{id}/members` (a list
// of person ids per group) exposes it. The "No group" block is therefore
// the complement: every person not present in any members list.
//
// **A real cover photo, but not necessarily the chosen one**: `Person`
// carries `cover_hash`/`cover_thumbhash` computed server-side, in the
// same query as `face_count` — not necessarily the asset behind
// `cover_face_id` (the one set via "Choose cover"), for the same reason
// as before this moved server-side: no route resolves a bare face id to
// its asset/tile. This used to be a `runSearch({op:'person',id})` call
// per card instead (one round trip per person — tens to hundreds on a
// real library, enough concurrent load against the connection pool to
// make the whole app feel slow, not just this page) — fixed by computing
// it alongside everything else `GET /persons` already returns.
//
// **No `autoNum` for nameless people**: `_personAutoSeq` would be an
// in-memory mockup counter, with no corresponding column on the real
// backend (`Person.name: Option<String>`, nothing else). Instead of a
// made-up "Person 12", the label is `persons.unnamed`
// ("Unnamed person") — honest, not a fabricated number.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { runSearch } from '@/api/library'
import {
  deletePersonGroup,
  fetchGroupMembers,
  fetchPersonGroups,
  fetchPersons,
  type Person,
  type PersonGroup
} from '@/api/persons'
import AssignGroupDialog from '@/components/AssignGroupDialog.vue'
import GroupEditorDialog from '@/components/GroupEditorDialog.vue'
import MergePeopleDialog from '@/components/MergePeopleDialog.vue'
import PersonCard from '@/components/PersonCard.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const router = useRouter()
const toast = useToastStore()

const loading = ref(true)
const people = ref<Person[]>([])
const groups = ref<PersonGroup[]>([])
const memberOf = ref<Record<string, string>>({})
const hiddenCount = ref(0)

async function loadGroups() {
  groups.value = await fetchPersonGroups()
  const memberships = await Promise.all(groups.value.map((g) => fetchGroupMembers(g.id)))
  const map: Record<string, string> = {}
  groups.value.forEach((g, i) => {
    for (const personId of memberships[i]) map[personId] = g.id
  })
  memberOf.value = map
}

async function load() {
  loading.value = true
  try {
    const [visible, all] = await Promise.all([fetchPersons(), fetchPersons(true)])
    people.value = visible.filter((p) => (p.face_count ?? 0) > 0)
    hiddenCount.value = all.filter((p) => p.hidden).length
    await loadGroups()
  } catch {
    toast.showError(t('persons.loadError'))
  } finally {
    loading.value = false
  }
}

onMounted(load)

const groupBlocks = computed(() =>
  groups.value.map((group) => ({
    group,
    people: people.value.filter((p) => memberOf.value[p.id] === group.id)
  }))
)

const ungrouped = computed(() => people.value.filter((p) => !memberOf.value[p.id]))

function open(person: Person) {
  void router.push(`/persons/${person.id}`)
}

// Selection is kept in click order (needed by the merge dialog for
// the default survivor), not a `Set`, which would lose the order.
const selectedIds = ref<string[]>([])

function toggleSelect(personId: string) {
  const i = selectedIds.value.indexOf(personId)
  if (i === -1) selectedIds.value = [...selectedIds.value, personId]
  else selectedIds.value = selectedIds.value.filter((id) => id !== personId)
}

function clearSelection() {
  selectedIds.value = []
}

const selectedPeople = computed(() =>
  selectedIds.value.map((id) => people.value.find((p) => p.id === id)).filter((p): p is Person => p !== undefined)
)

const groupEditorOpen = ref(false)
const editingGroup = ref<PersonGroup | null>(null)

function openNewGroup() {
  editingGroup.value = null
  groupEditorOpen.value = true
}

function openRenameGroup(group: PersonGroup) {
  editingGroup.value = group
  groupEditorOpen.value = true
}

const deleteGroupTarget = ref<PersonGroup | null>(null)
const deleteGroupConfirmOpen = ref(false)

function askDeleteGroup(group: PersonGroup) {
  deleteGroupTarget.value = group
  deleteGroupConfirmOpen.value = true
}

async function confirmDeleteGroup() {
  const target = deleteGroupTarget.value
  if (!target) return
  try {
    await deletePersonGroup(target.id)
    toast.show(t('persons.groupDeletedToast'))
    await load()
  } catch {
    toast.showError(t('persons.deleteGroupError'))
  }
}

const assignGroupOpen = ref(false)

function currentGroupOf(personId: string): string | null {
  return memberOf.value[personId] ?? null
}

async function onAssigned() {
  clearSelection()
  await loadGroups()
}

const mergeOpen = ref(false)
const mergeTotalPhotos = ref(0)

async function openMerge() {
  const ids = selectedIds.value
  if (ids.length < 2) return
  try {
    const page = await runSearch({ op: 'or', args: ids.map((id) => ({ op: 'person' as const, id })) })
    mergeTotalPhotos.value = page.assets.length
  } catch {
    mergeTotalPhotos.value = 0
  }
  mergeOpen.value = true
}

async function onMerged() {
  clearSelection()
  await load()
}
</script>

<template>
  <main class="mx-auto max-w-[860px] p-6">
    <div class="flex items-center justify-between gap-3">
      <div>
        <p class="text-[15px] font-bold">
          {{ t('persons.title') }}
        </p>
        <p class="mt-1 text-[12.5px] text-content-muted">
          {{ t('persons.subtitle') }}
        </p>
      </div>
      <button
        type="button"
        class="shrink-0 rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
        @click="openNewGroup"
      >
        {{ t('persons.newGroup') }}
      </button>
    </div>

    <div
      v-if="selectedIds.length > 0"
      class="mt-4 flex items-center gap-2 rounded-lg border border-border px-3 py-2"
    >
      <button
        type="button"
        class="rounded-md px-1.5 py-1 text-[12px] text-content-muted hover:bg-border/30"
        :aria-label="t('persons.cancelSelection')"
        @click="clearSelection"
      >
        ✕
      </button>
      <span class="text-[13px] font-semibold">
        {{ t('persons.selectedCount', { n: selectedIds.length }, { plural: selectedIds.length }) }}
      </span>
      <div class="ml-auto flex gap-1">
        <button
          v-if="selectedIds.length >= 2"
          type="button"
          class="rounded-md px-2 py-1 text-[12px] font-semibold text-content-muted hover:bg-border/30"
          :aria-label="t('persons.mergeAction')"
          @click="openMerge"
        >
          {{ t('persons.mergeAction') }}
        </button>
        <button
          type="button"
          class="rounded-md px-2 py-1 text-[12px] font-semibold text-content-muted hover:bg-border/30"
          :aria-label="t('persons.assignGroupAction')"
          @click="assignGroupOpen = true"
        >
          {{ t('persons.assignGroupAction') }}
        </button>
      </div>
    </div>

    <p
      v-if="!loading && people.length === 0"
      class="mt-6 text-[13px] text-content-muted"
    >
      {{ t('persons.emptyText') }}
    </p>

    <template v-else>
      <section
        v-for="block in groupBlocks"
        :key="block.group.id"
        class="mt-6"
      >
        <div class="mb-2 flex items-center justify-between">
          <div class="flex items-baseline gap-2">
            <p class="text-[13.5px] font-bold">
              {{ block.group.name }}
            </p>
            <span class="text-[11.5px] text-content-muted">
              {{ t('persons.groupCount', { n: block.people.length }, { plural: block.people.length }) }}
            </span>
          </div>
          <div class="flex gap-1">
            <button
              type="button"
              class="rounded-md px-2 py-1 text-[12px] text-content-muted hover:bg-border/30 hover:text-content"
              :aria-label="t('persons.renameGroup', { name: block.group.name })"
              @click="openRenameGroup(block.group)"
            >
              {{ t('persons.edit') }}
            </button>
            <button
              type="button"
              class="rounded-md px-2 py-1 text-[12px] text-content-muted hover:bg-danger/10 hover:text-danger"
              :aria-label="t('persons.deleteGroup', { name: block.group.name })"
              @click="askDeleteGroup(block.group)"
            >
              {{ t('persons.delete') }}
            </button>
          </div>
        </div>
        <p
          v-if="block.people.length === 0"
          class="text-[12.5px] text-content-muted"
        >
          {{ t('persons.emptyGroupText') }}
        </p>
        <div
          v-else
          class="grid grid-cols-[repeat(auto-fill,minmax(110px,1fr))] gap-4"
        >
          <PersonCard
            v-for="person in block.people"
            :key="person.id"
            :person="person"
            :cover="{ hash: person.cover_hash ?? null, thumbhash: person.cover_thumbhash ?? null }"
            :selected="selectedIds.includes(person.id)"
            @open="open(person)"
            @toggle-select="toggleSelect(person.id)"
          />
        </div>
      </section>

      <section class="mt-6">
        <p class="mb-2 text-[13.5px] font-bold">
          {{ t('persons.noGroupTitle') }}
        </p>
        <p
          v-if="ungrouped.length === 0"
          class="text-[12.5px] text-content-muted"
        >
          {{ t('persons.emptyGroupText') }}
        </p>
        <div
          v-else
          class="grid grid-cols-[repeat(auto-fill,minmax(110px,1fr))] gap-4"
        >
          <PersonCard
            v-for="person in ungrouped"
            :key="person.id"
            :person="person"
            :cover="{ hash: person.cover_hash ?? null, thumbhash: person.cover_thumbhash ?? null }"
            :selected="selectedIds.includes(person.id)"
            @open="open(person)"
            @toggle-select="toggleSelect(person.id)"
          />
        </div>
      </section>
    </template>

    <p
      v-if="hiddenCount > 0"
      class="mt-6 text-[12px] text-content-muted"
    >
      {{ t('persons.hiddenFooter', { n: hiddenCount }, { plural: hiddenCount }) }}
    </p>

    <GroupEditorDialog
      v-model:open="groupEditorOpen"
      :group="editingGroup"
      @saved="load"
    />
    <ConfirmDialog
      v-if="deleteGroupTarget"
      v-model:open="deleteGroupConfirmOpen"
      :title="t('persons.deleteGroupConfirmTitle', { name: deleteGroupTarget.name })"
      :description="t('persons.deleteGroupConfirmDescription')"
      :confirm-label="t('persons.deleteGroupConfirmButton')"
      @confirm="confirmDeleteGroup"
    />
    <AssignGroupDialog
      v-model:open="assignGroupOpen"
      :person-ids="selectedIds"
      :person-label="selectedIds.length === 1
        ? (selectedPeople[0]?.name?.trim() || t('persons.unnamed'))
        : t('persons.selectedCount', { n: selectedIds.length }, { plural: selectedIds.length })"
      :current-group-id="currentGroupOf"
      :groups="groups"
      @assigned="onAssigned"
    />
    <MergePeopleDialog
      v-model:open="mergeOpen"
      :people="selectedPeople"
      :total-photo-count="mergeTotalPhotos"
      @merged="onMerged"
    />
  </main>
</template>
