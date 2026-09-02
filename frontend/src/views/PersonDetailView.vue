<script setup lang="ts">
// **All five action buttons are real**: "Assign/Change group" reuses
// `AssignGroupDialog.vue`; "Choose cover" (`ChooseCoverDialog.vue`) and
// "Split…" (`SplitPersonDialog.vue`) both build their own thumbnails from
// `fetchPersonFaceTiles` on the photos already loaded here (`assets`),
// one confirmed face per thumbnail.
//
// **"Fewer than two faces" is approximated with `assets.length`**: the
// exact count of *faces* (not distinct photos) would require loading all
// the split dialog's thumbnails just for a preliminary check —
// disproportionate cost for the rare case where they differ (two
// confirmed faces in the same photo). `assets.length` is the same number
// already shown in this view's summary line, honest even if not
// identical to "N faces" in that rare case.
//
// **Photo grid = `photosForPerson()`**: `runSearch({op: 'person', id})`,
// paginated like Favorites (same `do…while(cursor)` scheme — no person's
// photo set is ever large enough to justify incremental loading, same
// reasoning already applied there). `useBrowseFilters` operates over the
// person's own photo set, not the whole library — the "Persons" quick
// filter dimension is still present and can further narrow down to
// co-appearances, a behavior this scoping gives for free since the
// Persons quick-filter dimension already exists.
//
// **"Show again" stays effectively unreachable**: hiding returns to the
// grid, and hidden people are excluded from `visiblePeople()` — there is
// no real path to reopen this detail view on a hidden person. Not
// "solved" here, same behavior as intended.
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import { startLiveEvents, type LiveSocket } from '@/api/events'
import { runSearch } from '@/api/library'
import { thumbSrc } from '@/api/media'
import { fetchGroupMembers, fetchPerson, fetchPersonGroups, patchPerson, type Person, type PersonGroup } from '@/api/persons'
import type { TimelineAsset } from '@/api/timeline'
import AssetViewer from '@/components/AssetViewer.vue'
import AssignGroupDialog from '@/components/AssignGroupDialog.vue'
import ChooseCoverDialog from '@/components/ChooseCoverDialog.vue'
import FlatAssetGrid from '@/components/FlatAssetGrid.vue'
import LibrarySelectionActions from '@/components/LibrarySelectionActions.vue'
import MergePeopleDialog from '@/components/MergePeopleDialog.vue'
import PersonPickerDialog from '@/components/PersonPickerDialog.vue'
import SplitPersonDialog from '@/components/SplitPersonDialog.vue'
import Dialog from '@/components/ui/Dialog.vue'
import QuickFilter from '@/components/ui/QuickFilter.vue'
import SelectAllVisible from '@/components/ui/SelectAllVisible.vue'
import SelectionBar from '@/components/ui/SelectionBar.vue'
import TextField from '@/components/ui/TextField.vue'
import { useBrowseFilters } from '@/composables/useBrowseFilters'
import { useDebouncedCallback } from '@/composables/useDebouncedCallback'
import { useDensity } from '@/composables/useDensity'
import { useLightboxRoute } from '@/composables/useLightboxRoute'
import { activePersonName } from '@/nav/routeTitles'
import { useFavoritesStore } from '@/stores/favorites'
import { useMapsStore } from '@/stores/maps'
import { useSelectionStore } from '@/stores/selection'
import { useToastStore } from '@/stores/toast'
import { thumbhashToDataURL } from '@/timeline/thumbhash'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const toast = useToastStore()
const maps = useMapsStore()
const favorites = useFavoritesStore()
const selection = useSelectionStore()
const { density, setDensity } = useDensity()

const personId = computed(() => route.params.id as string)
const person = ref<Person | null>(null)
const notFound = ref(false)
const assets = ref<TimelineAsset[]>([])
const loaded = ref(false)
const groups = ref<PersonGroup[]>([])
const currentGroup = ref<PersonGroup | null>(null)

let live: LiveSocket | undefined

async function loadGroup() {
  groups.value = await fetchPersonGroups()
  const memberships = await Promise.all(groups.value.map((g) => fetchGroupMembers(g.id)))
  const i = memberships.findIndex((ids) => ids.includes(personId.value))
  currentGroup.value = i === -1 ? null : groups.value[i]
}

async function loadPhotos() {
  const collected: TimelineAsset[] = []
  let cursor: string | undefined
  do {
    const page = await runSearch({ op: 'person', id: personId.value }, cursor)
    collected.push(...page.assets)
    cursor = page.next_cursor
  } while (cursor)
  assets.value = collected
}

async function load() {
  loaded.value = false
  notFound.value = false
  try {
    person.value = await fetchPerson(personId.value)
    activePersonName.value = person.value.name?.trim() || null
    await Promise.all([loadPhotos(), loadGroup()])
    loaded.value = true
  } catch {
    // If the person disappears, we automatically fall back to the grid —
    // same behavior for a nonexistent or no-longer-visible id (403, never
    // a 404 — see the backend's comment).
    notFound.value = true
    await router.replace('/persons')
  }
}

watch(personId, load)

const { selection: filterSelection, dimensions: filterDimensions, filteredAssets } = useBrowseFilters(assets)

const lightbox = useLightboxRoute<TimelineAsset>(
  (id) => filteredAssets.value.find((asset) => asset.id === id),
  (id) => maps.loadAsset(id)
)

function stepViewer(asset: TimelineAsset) {
  void lightbox.step(asset)
}

function openViewerAsset(id: string) {
  void lightbox.openById(id)
}

const selectionMode = computed(() => selection.library.selectedIds.size > 0)
const selectedAssets = computed(() => assets.value.filter((asset) => selection.library.selectedIds.has(asset.id)))

function selectAllVisible() {
  selection.library.selectAllVisible(filteredAssets.value.map((asset) => asset.id))
}

function displayName(p: Person): string {
  return p.name?.trim() || t('persons.unnamed')
}

const coverStyle = computed(() => {
  const first = assets.value[0]
  if (!first) return {}
  if (first.content_hash) return { backgroundImage: `url(${thumbSrc(first.content_hash)})` }
  if (first.thumbhash) {
    const url = thumbhashToDataURL(first.thumbhash)
    if (url) return { backgroundImage: `url(${url})` }
  }
  return {}
})

const renameOpen = ref(false)
const renameValue = ref('')

function openRename() {
  renameValue.value = person.value?.name ?? ''
  renameOpen.value = true
}

async function saveRename() {
  if (!person.value) return
  try {
    person.value = await patchPerson(person.value.id, { name: renameValue.value.trim() })
    activePersonName.value = person.value.name?.trim() || null
    renameOpen.value = false
  } catch {
    toast.showError(t('persons.renameError'))
  }
}

const assignGroupOpen = ref(false)

function currentGroupOf(id: string): string | null {
  return id === personId.value ? (currentGroup.value?.id ?? null) : null
}

async function onAssigned() {
  await loadGroup()
}

const coverOpen = ref(false)

function onCoverUpdated(updated: Person) {
  person.value = updated
}

const splitOpen = ref(false)

function askSplit() {
  // If the person has fewer than two faces, the dialog does not open —
  // approximated with `assets.length` (see the file's header comment).
  if (assets.value.length < 2) {
    toast.showError(t('splitPerson.tooFewFaces'))
    return
  }
  splitOpen.value = true
}

async function onSplit() {
  await load()
}

// "Merge with…" — the same MergePeopleDialog the overview grid's
// multi-select already uses, reached differently: pick the *other*
// person first (PersonPickerDialog, excluding this one), then open the
// merge dialog with exactly the two of them. `mergeTarget` is the full
// `Person`, not just the id PersonPickerDialog emits — MergePeopleDialog
// needs face_count/name for both rows in its survivor picker.
const personPickerOpen = ref(false)
const mergeOpen = ref(false)
const mergeTarget = ref<Person | null>(null)
const mergeTotalPhotos = ref(0)

async function onMergeTargetPicked(targetId: string) {
  if (!person.value) return
  try {
    const [target, page] = await Promise.all([
      fetchPerson(targetId),
      runSearch({
        op: 'or',
        args: [
          { op: 'person', id: person.value.id },
          { op: 'person', id: targetId }
        ]
      })
    ])
    mergeTarget.value = target
    mergeTotalPhotos.value = page.assets.length
  } catch {
    toast.showError(t('mergePeople.error'))
    return
  }
  mergeOpen.value = true
}

// Whichever of the two survives, `load()` already does the right thing:
// `fetchPerson(personId.value)` either returns the updated survivor (this
// page just refreshes) or fails because this person no longer exists
// (absorbed into the other one) — the existing catch in `load()` already
// redirects to `/persons` for exactly that case, same as a person that
// became invisible or was deleted outright.
async function onMerged() {
  mergeTarget.value = null
  await load()
}

async function toggleHidden() {
  if (!person.value) return
  const next = !person.value.hidden
  try {
    await patchPerson(person.value.id, { hidden: next })
    if (next) {
      toast.show(t('persons.hiddenToast'))
      await router.push('/persons')
    } else {
      person.value.hidden = false
      toast.show(t('persons.shownToast'))
    }
  } catch {
    toast.showError(t('persons.hideError'))
  }
}

// During a large import, `assets.upserted` arrives once per finished
// background job — tens per second (see TimelineView.vue for the full
// story). Debounced so a burst collapses into one reload instead of
// re-rendering the grid on every single file.
const scheduleReload = useDebouncedCallback(() => void loadPhotos(), 800)

onMounted(async () => {
  await load()
  live = startLiveEvents((msg) => {
    if (msg.type === 'resync' || msg.type === 'assets.upserted' || msg.type === 'assets.deleted') {
      scheduleReload()
    }
  })
})

onUnmounted(() => {
  live?.close()
  activePersonName.value = null
})
</script>

<template>
  <div class="flex h-full flex-col">
    <template v-if="person">
      <div class="border-b border-border px-4 py-3">
        <RouterLink
          to="/persons"
          class="text-[13px] text-content-muted hover:text-content"
        >
          ← {{ t('persons.backLink') }}
        </RouterLink>

        <div class="mt-2 flex items-center gap-3.5">
          <span
            class="h-[78px] w-[78px] shrink-0 rounded-full border border-border bg-cover bg-center bg-surface-elevated"
            :style="coverStyle"
            aria-hidden="true"
          />
          <div>
            <p class="text-[16px] font-bold">
              {{ displayName(person) }}
            </p>
            <p class="text-[12.5px] text-content-muted">
              {{ t('persons.photoCount', { n: assets.length }, { plural: assets.length }) }}
              <template v-if="currentGroup"> · {{ t('persons.groupLabel', { name: currentGroup.name }) }}</template>
              <template v-else> · {{ t('persons.noGroupLabel') }}</template>
              <template v-if="person.hidden"> · {{ t('persons.hiddenLabel') }}</template>
            </p>
          </div>
        </div>

        <div class="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
            @click="openRename"
          >
            {{ t('persons.rename') }}
          </button>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
            @click="coverOpen = true"
          >
            {{ t('persons.chooseCover') }}
          </button>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
            @click="assignGroupOpen = true"
          >
            {{ currentGroup ? t('persons.changeGroup') : t('persons.assignGroup') }}
          </button>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
            @click="askSplit"
          >
            {{ t('persons.split') }}
          </button>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
            @click="personPickerOpen = true"
          >
            {{ t('persons.mergeWith') }}
          </button>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-1.5 text-[13px] font-semibold hover:bg-border/20"
            @click="toggleHidden"
          >
            {{ person.hidden ? t('persons.show') : t('persons.hide') }}
          </button>
        </div>
      </div>

      <div
        v-if="!selectionMode"
        class="flex items-center gap-3 border-b border-border px-4 py-3"
      >
        <div class="ml-auto flex items-center gap-2">
          <SelectAllVisible
            :visible-count="filteredAssets.length"
            @select-all="selectAllVisible"
          />
          <QuickFilter
            v-model:selection="filterSelection"
            :dimensions="filterDimensions"
            :result-count="filteredAssets.length"
          />
          <button
            class="rounded-lg border border-border px-2 py-1"
            :aria-label="t('timeline.densityDown')"
            @click="setDensity(density - 1)"
          >
            −
          </button>
          <span class="w-4 text-center text-sm">{{ density }}</span>
          <button
            class="rounded-lg border border-border px-2 py-1"
            :aria-label="t('timeline.densityUp')"
            @click="setDensity(density + 1)"
          >
            +
          </button>
        </div>
      </div>
      <div :class="selectionMode && 'border-b border-border px-4 py-3'">
        <SelectionBar
          :count="selection.library.selectedIds.size"
          :ariaLabel="t('ui.selectionBar.ariaLabel')"
          @clear="selection.library.clear()"
          @select-all="selectAllVisible"
        >
          <LibrarySelectionActions :assets="selectedAssets" />
        </SelectionBar>
      </div>

      <div
        v-if="loaded && assets.length === 0"
        class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
      >
        <p class="text-sm font-semibold">
          {{ t('persons.emptyPhotosTitle') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('persons.emptyPhotosSubtitle') }}
        </p>
      </div>
      <div
        v-else-if="filteredAssets.length === 0"
        class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
      >
        <p class="text-sm font-semibold">
          {{ t('ui.filteredEmpty.title') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('ui.filteredEmpty.subtitle') }}
        </p>
      </div>
      <FlatAssetGrid
        v-else
        :assets="filteredAssets"
        :density="density"
        @open="lightbox.open"
      />
    </template>

    <AssetViewer
      v-if="lightbox.viewing.value"
      :asset="lightbox.viewing.value"
      :neighbors="filteredAssets"
      :is-favorite="favorites.isFavorite(lightbox.viewing.value)"
      @close="lightbox.close"
      @step="stepViewer"
      @open-asset="openViewerAsset"
      @toggle-favorite="favorites.toggleOne(lightbox.viewing.value)"
    />

    <Dialog
      v-model:open="renameOpen"
      :title="t('persons.renameTitle')"
    >
      <div class="flex flex-col gap-3.5">
        <TextField
          v-model="renameValue"
          :label="t('persons.name')"
        />
        <div class="mt-1 flex items-center gap-2">
          <button
            type="button"
            class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-text"
            @click="saveRename"
          >
            {{ t('persons.save') }}
          </button>
          <button
            type="button"
            class="rounded-lg border border-transparent px-3.5 py-2 text-[13px] font-semibold hover:bg-border/30"
            @click="renameOpen = false"
          >
            {{ t('ui.dialog.cancel') }}
          </button>
        </div>
      </div>
    </Dialog>

    <AssignGroupDialog
      v-if="person"
      v-model:open="assignGroupOpen"
      :person-ids="[person.id]"
      :person-label="displayName(person)"
      :current-group-id="currentGroupOf"
      :groups="groups"
      @assigned="onAssigned"
    />
    <ChooseCoverDialog
      v-if="person"
      v-model:open="coverOpen"
      :person="person"
      :assets="assets"
      @updated="onCoverUpdated"
    />
    <SplitPersonDialog
      v-if="person"
      v-model:open="splitOpen"
      :person="person"
      :assets="assets"
      @split="onSplit"
    />
    <PersonPickerDialog
      v-if="person"
      v-model:open="personPickerOpen"
      :exclude-id="person.id"
      :title="t('persons.mergePickerTitle')"
      @picked="onMergeTargetPicked"
    />
    <MergePeopleDialog
      v-if="person && mergeTarget"
      v-model:open="mergeOpen"
      :people="[person, mergeTarget]"
      :total-photo-count="mergeTotalPhotos"
      @merged="onMerged"
    />
  </div>
</template>
