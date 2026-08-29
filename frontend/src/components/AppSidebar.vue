<script setup lang="ts">
// Navigation sidebar. Verified line by line against the mockup (brand mark
// markup, full item list) — not just the plan's summary.
//
// Declared scope: **not** every canonical item. Only items with a real
// destination in this app are built — Photos, Search, Culling, Map,
// Shares, People (grid + detail, without groups/merge/cover yet),
// Favorites, Albums, Folders, Trash, Problems (plus Users/Groups for an
// administrator).
//
// The "AI" group has **two** real items, not three: "Tags and categories"
// and "Review" (badge `shell.badges.revision`, tags+faces combined —
// real, not new here). "Library analysis" stays out:
// `AnalysisLevel::ms_per_photo()`
// (`crates/keeppix-jobs/src/profile.rs`) is real but no route reads it —
// the same gap already declared for "Artificial intelligence" in
// Settings, re-verified here before building this unit: building a page
// with no real data to show would be the exact opposite of the discipline
// followed in every other deviation.
// "Duplicates" inside "Maintenance" arrived after Trash/Problems, not at
// the same time — no residual gap here.
// Every item here is a real <RouterLink>, therefore keyboard-reachable by
// construction — the prototype is not ("no sidebar item is
// keyboard-reachable").
//
// "Folders" here is **not** the mockup's group (a `.folder-item` row per
// folder, jumping straight to a filtered timeline): that filtered timeline
// doesn't exist yet, the same gap already declared for detail routes.
// Instead it's a single link to `/folders` (`FoldersView`), the app's real
// folder tree — an organization feature (moving photos between folders)
// different from the mockup's, not modeled there. Added here because
// removing `TimelineView`'s improvised header without first giving it a
// real destination in `AppSidebar` would make it unreachable — a dead end
// found while writing that step, not a planned addition.
//
// "Administration" (Users/Groups, only for `role==='admin'`) is not in the
// mockup: the mockup is single-user, it doesn't model the multi-user
// administration the real backend has instead. Same reason as "Folders":
// they were only reachable from `TimelineView`'s improvised header,
// otherwise a dead end after removing it.
//
// `UploadQueueStrip` (upload subsystem): "in the sidebar footer, above
// 'Free space'" — an exact position, not just "somewhere in the footer".
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import Avatar from '@/components/ui/Avatar.vue'
import NavGroup from '@/components/ui/NavGroup.vue'
import Popover from '@/components/ui/Popover.vue'
import UploadQueueStrip from '@/components/UploadQueueStrip.vue'
import { useAvatarColorStore } from '@/stores/avatarColor'
import { useSessionStore } from '@/stores/session'
import { useShellStore } from '@/stores/shell'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const shell = useShellStore()
const avatarColor = useAvatarColorStore()

onMounted(() => {
  if (!shell.loaded) void shell.load()
})

// "People" here, not in the "Library" group below — the mockup explicitly
// puts it in `NAV_TOP` ("'People' sidebar item (NAV_TOP, user icon)"),
// unlike Folders/Favorites/Albums which the mockup doesn't list in
// `NAV_TOP` at all. On mobile it instead lives under "Library"
// (`MoreView.vue`) — a different placement per platform, declared by the
// same mockup note.
const NAV_TOP = [
  { to: '/', labelKey: 'nav.foto', badge: false },
  { to: '/search', labelKey: 'nav.cerca', badge: false },
  { to: '/culling', labelKey: 'culling.entry', badge: true },
  { to: '/map', labelKey: 'maps.entry', badge: false },
  { to: '/shares', labelKey: 'shares.entry', badge: false },
  { to: '/persons', labelKey: 'persons.entry', badge: false }
] as const

const MAINT_ITEMS = [
  { to: '/trash', labelKey: 'trash.entry' },
  { to: '/duplicates', labelKey: 'duplicates.entry' },
  { to: '/problems', labelKey: 'problems.title' }
] as const

const ADMIN_ITEMS = [
  { to: '/users', labelKey: 'users.entry' },
  { to: '/groups', labelKey: 'groups.entry' }
] as const

// "Review" with badge `shell.badges.revision` — real, combining tags and
// faces, not just tags (the extended comment is in
// `App.vue`/`bootstrap.rs`), not a new count here.
const IA_ITEMS = [
  { to: '/tags', labelKey: 'tags.entry', badge: false },
  { to: '/review', labelKey: 'review.entry', badge: true }
] as const

// `/albums` stays highlighted even inside an album's detail view (the
// first route with real children: `/albums/:id`) — the same idea as the
// mockup ("clicking Albums also clears `state.openAlbum`"), flipped here:
// you're still inside the Albums section even with a detail open.
function isActive(to: string): boolean {
  if (to === '/albums') return route.path === to || route.path.startsWith('/albums/')
  if (to === '/persons') return route.path === to || route.path.startsWith('/persons/')
  return route.path === to
}

const maintActive = computed(() => MAINT_ITEMS.some((item) => isActive(item.to)))
const iaActive = computed(() => IA_ITEMS.some((item) => isActive(item.to)))
const adminActive = computed(() => ADMIN_ITEMS.some((item) => isActive(item.to)))

const accountMenuOpen = ref(false)

async function signOut() {
  await session.logout()
  await router.push('/login')
}

/** Third local copy of the same formatting logic already duplicated (and
 * diverging: 1024 in `UploadPanel.vue`, 1000 in `MapsOfflineView.vue`) — a
 * known gap, not resolved here: unifying them touches two views this
 * change isn't touching. */
function formatBytes(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let size = Math.max(0, bytes)
  let unit = 0
  while (size >= 1000 && unit < units.length - 1) {
    size /= 1000
    unit += 1
  }
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(size)} ${units[unit]}`
}

const storageTotals = computed(() => {
  const values = Object.values(shell.storage)
  if (values.length === 0) return null
  const free = values.reduce((sum, v) => sum + v.free_bytes, 0)
  const total = values.reduce((sum, v) => sum + v.total_bytes, 0)
  return { free, total }
})
</script>

<template>
  <div class="flex h-full w-[216px] flex-col overflow-y-auto border-r border-border bg-surface p-3.5 pt-[18px]">
    <div class="mb-[18px] flex items-center gap-2.5 px-1.5">
      <span
        class="brand-mark inline-block h-[26px] w-[26px] shrink-0"
        aria-hidden="true"
      >
        <svg viewBox="0 0 200 200">
          <circle
            cx="100"
            cy="100"
            r="62"
            fill="none"
            stroke="currentColor"
            stroke-width="22"
          />
          <circle
            class="brand-mark-dot"
            cx="100"
            cy="100"
            r="24"
            fill="var(--color-accent)"
          />
        </svg>
      </span>
      <span class="text-[15.5px] font-bold tracking-[-0.01em]">Keeppix</span>
    </div>

    <nav class="flex flex-col gap-px">
      <RouterLink
        v-for="item in NAV_TOP"
        :key="item.to"
        :to="item.to"
        class="flex items-center justify-between rounded-lg border-l-[2.5px] border-transparent px-2.5 py-2 text-sm hover:bg-border/30"
        :class="isActive(item.to) && 'border-l-accent bg-border/30 font-semibold'"
      >
        <span>{{ t(item.labelKey) }}</span>
        <span
          v-if="item.badge"
          class="min-w-[16px] rounded-full bg-danger px-1.5 text-center text-[10.5px] font-bold text-white"
        >
          {{ shell.badges.culling }}
        </span>
      </RouterLink>
    </nav>

    <p class="mb-2 mt-[18px] px-1.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
      {{ t('nav.libraryGroup') }}
    </p>
    <nav class="flex flex-col gap-px">
      <RouterLink
        to="/folders"
        class="flex items-center rounded-lg border-l-[2.5px] border-transparent px-2.5 py-2 text-sm hover:bg-border/30"
        :class="isActive('/folders') && 'border-l-accent bg-border/30 font-semibold'"
      >
        {{ t('folders.entry') }}
      </RouterLink>
      <RouterLink
        to="/favorites"
        class="flex items-center rounded-lg border-l-[2.5px] border-transparent px-2.5 py-2 text-sm hover:bg-border/30"
        :class="isActive('/favorites') && 'border-l-accent bg-border/30 font-semibold'"
      >
        {{ t('favorites.entry') }}
      </RouterLink>
      <RouterLink
        to="/albums"
        class="flex items-center rounded-lg border-l-[2.5px] border-transparent px-2.5 py-2 text-sm hover:bg-border/30"
        :class="isActive('/albums') && 'border-l-accent bg-border/30 font-semibold'"
      >
        {{ t('albums.entry') }}
      </RouterLink>
      <NavGroup
        :label="t('nav.manutenzione')"
        :active="maintActive"
      >
        <RouterLink
          v-for="item in MAINT_ITEMS"
          :key="item.to"
          :to="item.to"
          class="block rounded-lg border-l-[2.5px] border-transparent px-2.5 py-1.5 text-[13px] hover:bg-border/30"
          :class="isActive(item.to) && 'border-l-accent bg-border/30 font-semibold'"
        >
          {{ t(item.labelKey) }}
        </RouterLink>
      </NavGroup>
      <NavGroup
        :label="t('nav.ia')"
        :active="iaActive"
      >
        <RouterLink
          v-for="item in IA_ITEMS"
          :key="item.to"
          :to="item.to"
          class="flex items-center justify-between rounded-lg border-l-[2.5px] border-transparent px-2.5 py-1.5 text-[13px] hover:bg-border/30"
          :class="isActive(item.to) && 'border-l-accent bg-border/30 font-semibold'"
        >
          <span>{{ t(item.labelKey) }}</span>
          <span
            v-if="item.badge && shell.badges.revision > 0"
            class="min-w-[16px] rounded-full bg-danger px-1.5 text-center text-[10.5px] font-bold text-white"
          >
            {{ shell.badges.revision }}
          </span>
        </RouterLink>
      </NavGroup>
      <NavGroup
        v-if="session.user?.role === 'admin'"
        :label="t('nav.amministrazione')"
        :active="adminActive"
      >
        <RouterLink
          v-for="item in ADMIN_ITEMS"
          :key="item.to"
          :to="item.to"
          class="block rounded-lg border-l-[2.5px] border-transparent px-2.5 py-1.5 text-[13px] hover:bg-border/30"
          :class="isActive(item.to) && 'border-l-accent bg-border/30 font-semibold'"
        >
          {{ t(item.labelKey) }}
        </RouterLink>
      </NavGroup>
    </nav>

    <div class="flex-1" />

    <UploadQueueStrip />

    <div
      v-if="storageTotals"
      class="mb-3 mt-1.5 rounded-[10px] bg-surface-elevated px-3 py-[11px]"
    >
      <p class="text-[11px] text-content-muted">
        {{ t('sidebar.storageLabel') }}
      </p>
      <p class="text-[12.5px] font-semibold">
        {{ t('sidebar.storageValue', { free: formatBytes(storageTotals.free), total: formatBytes(storageTotals.total) }) }}
      </p>
      <div class="mt-1.5 h-[5px] rounded-[3px] bg-border">
        <div
          class="h-full rounded-[3px] bg-accent"
          :style="{ width: `${storageTotals.total > 0 ? Math.min(100, (100 * (storageTotals.total - storageTotals.free)) / storageTotals.total) : 0}%` }"
        />
      </div>
    </div>

    <Popover
      v-if="session.user"
      v-model:open="accountMenuOpen"
      side="top"
      align="start"
    >
      <template #trigger>
        <button
          type="button"
          class="flex items-center gap-2.5 rounded-lg p-1.5 pr-2 text-left hover:bg-border/30"
        >
          <Avatar
            :name="session.user.display_name"
            :color="avatarColor.hex"
          />
          <span class="text-[13px] font-semibold">{{ session.user.display_name }}</span>
        </button>
      </template>
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-[13px] hover:bg-border/30"
        @click="accountMenuOpen = false; router.push('/profile')"
      >
        {{ t('profile.entry') }}
      </button>
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-[13px] hover:bg-border/30"
        @click="accountMenuOpen = false; router.push('/settings')"
      >
        {{ t('settings.entry') }}
      </button>
      <button
        type="button"
        class="flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-[13px] text-danger hover:bg-border/30"
        @click="signOut"
      >
        {{ t('home.logout') }}
      </button>
    </Popover>
  </div>
</template>

<style scoped>
.brand-mark {
  color: var(--color-content);
}
.brand-mark-dot {
  stroke: #3a3a3a;
  stroke-width: 3;
}
@media (prefers-color-scheme: dark) {
  .brand-mark-dot {
    stroke: none;
  }
}
</style>
