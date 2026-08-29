<script setup lang="ts">
// Declared scope:
// - Breadcrumbs: only the "current" segment, for the routes that have a
//   real destination today (same list as AppSidebar). The "parent" segment
//   (`Folders / <name>`, `Albums / <name>`, `Culling / <lot name>`) is
//   never reachable: none of these routes currently exposes an "open"
//   state observable from outside the view (same gap already declared in
//   AppSidebar for the "Folders" group).
//
// `/folders`, `/users`, `/groups` are entries in this map — they used to
// fall back to an empty breadcrumb ("literal prototype behavior for
// unmapped views"). That assumption relied on each view's own `<h1>`
// still acting as a title — but stripping those headings also removes
// that `<h1>`: without an entry here, those three pages would be left
// **with no title at all**, unlike the prototype which simply ignores
// them because they don't exist there. These are real destinations of
// this app (added to `AppSidebar`): they deserve a real breadcrumb,
// reusing `folders.title`/`users.title`/`groups.title` rather than
// inventing new copy.
// - The "Upload" command (`#uploadTopBtn`) — always "Upload", never
//   "Upload here": no view currently exposes an observable
//   `currentFolder`, the same gap already declared elsewhere in this
//   subsystem (`UploadDropVeil.vue`, `stores/upload.ts`).
// - The theme switch, already removed in the mockup itself, does not
//   exist here for the same reason: it lives in Settings.
//
// Accessibility fix relative to the prototype (same policy already
// applied in AppSidebar): the search field is `readonly` and in the
// mockup only responds to a mouse click. Here, Enter and Space trigger
// the same behavior as the click.
import { computed, nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import Tooltip from '@/components/ui/Tooltip.vue'
import { useUploadPicker, UPLOAD_ACCEPT } from '@/composables/useUploadPicker'
import { activeAlbumName, activeCullingLotName, activePersonName, ROUTE_TITLE_KEYS } from '@/nav/routeTitles'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const inputEl = ref<HTMLInputElement | null>(null)
const { open: openPicker, onChange } = useUploadPicker(inputEl)

// The only breadcrumb with a real parent segment ("Albums / <name>") —
// other routes stay at a single segment (see the file header comment).
// `/albums/new` is not an open album: it stays on the flat map below,
// not on this branch.
const albumBreadcrumbName = computed(() =>
  route.path.startsWith('/albums/') && route.path !== '/albums/new' ? activeAlbumName.value : null
)

// "People / <b>Name</b> when a detail is open" — but only if the person
// **has** a name: without one, only the flat "People" breadcrumb remains
// (no invented second segment like "Unnamed person").
const personBreadcrumbName = computed(() =>
  route.path.startsWith('/persons/') ? activePersonName.value : null
)

// "Desktop topbar: Culling / <b>Lot name</b>" — closes the gap declared
// above, now that `/culling/:lotId` actually exposes an "open" state.
const cullingLotBreadcrumbName = computed(() =>
  route.path.startsWith('/culling/') ? activeCullingLotName.value : null
)

const breadcrumbLabel = computed(() => {
  const key = ROUTE_TITLE_KEYS[route.path] ?? (route.path.startsWith('/persons/') ? 'persons.title' : undefined)
  return key ? t(key) : null
})

async function openSearch() {
  await router.push('/search')
  await nextTick()
  document.getElementById('search-query-input')?.focus()
}
</script>

<template>
  <div class="flex h-14 shrink-0 items-center justify-between gap-4 border-b border-border px-5">
    <div class="min-w-0 truncate text-[14.5px] text-content-muted">
      <template v-if="albumBreadcrumbName">
        {{ t('albums.entry') }} / <b class="font-semibold text-content">{{ albumBreadcrumbName }}</b>
      </template>
      <template v-else-if="personBreadcrumbName">
        {{ t('persons.title') }} / <b class="font-semibold text-content">{{ personBreadcrumbName }}</b>
      </template>
      <template v-else-if="cullingLotBreadcrumbName">
        {{ t('culling.entry') }} / <b class="font-semibold text-content">{{ cullingLotBreadcrumbName }}</b>
      </template>
      <b
        v-else-if="breadcrumbLabel"
        class="font-semibold text-content"
      >{{ breadcrumbLabel }}</b>
    </div>
    <div class="flex shrink-0 items-center gap-3.5">
      <Tooltip :label="t('upload.uploadTooltip')">
        <button
          type="button"
          class="rounded-lg px-2.5 py-1.5 text-[13px] font-semibold text-content-muted hover:bg-border/40"
          :aria-label="t('upload.uploadTooltip')"
          @click="openPicker"
        >
          {{ t('upload.uploadButton') }}
        </button>
      </Tooltip>
      <input
        id="topSearch"
        readonly
        type="text"
        class="w-[230px] cursor-text rounded-[9px] border border-border bg-surface-elevated px-3 py-2
               text-[13px] text-content-muted hover:bg-border/40
               focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        :placeholder="t('topbar.searchPlaceholder')"
        :aria-label="t('topbar.searchPlaceholder')"
        @click="openSearch"
        @keydown.enter.prevent="openSearch"
        @keydown.space.prevent="openSearch"
      >
      <input
        ref="inputEl"
        type="file"
        multiple
        :accept="UPLOAD_ACCEPT"
        class="hidden"
        :aria-hidden="true"
        tabindex="-1"
        @change="onChange"
      >
    </div>
  </div>
</template>
