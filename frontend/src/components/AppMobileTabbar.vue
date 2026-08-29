<script setup lang="ts">
// Mobile tab bar. Four real tabs, same order and exact labels as the
// mockup. No icon (same gap already declared in MoreView.vue — this
// frontend doesn't have one yet).
//
// "Active" state: the mockup explicitly assigns culling and bulkEdit to
// the "Photos" tab (not "More" — culling is entered ONLY from the funnel
// button on the Photos view), and a long list of views to the "More" tab.
// Here, with only this app's real routes: "More" for everything
// MoreView.vue lists (`/folders`, `/map`, `/shares`, `/trash`,
// `/problems`, `/users`, `/groups`, `/more` itself); "Photos" also for
// `/culling` and `/batch-edit`.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

const { t } = useI18n()
const route = useRoute()

const ALTRO_ROUTES = new Set([
  '/folders',
  '/map',
  '/shares',
  '/trash',
  '/duplicates',
  '/problems',
  '/users',
  '/groups',
  '/more'
])
const FOTO_ROUTES = new Set(['/', '/culling', '/batch-edit'])

type TabId = 'foto' | 'cerca' | 'album' | 'altro'

const activeTab = computed<TabId | null>(() => {
  // `/albums/:id`: same principle as `AppSidebar.isActive` — an album's
  // detail stays inside the "Album" tab.
  if (route.path === '/albums' || route.path.startsWith('/albums/')) return 'album'
  if (ALTRO_ROUTES.has(route.path)) return 'altro'
  if (FOTO_ROUTES.has(route.path)) return 'foto'
  if (route.path === '/search') return 'cerca'
  return null
})

const TABS: Array<{ id: TabId; to: string; labelKey: string }> = [
  { id: 'foto', to: '/', labelKey: 'nav.foto' },
  { id: 'cerca', to: '/search', labelKey: 'nav.cerca' },
  { id: 'album', to: '/albums', labelKey: 'albums.entry' },
  { id: 'altro', to: '/more', labelKey: 'nav.more' }
]
</script>

<template>
  <nav class="flex shrink-0 border-t border-border bg-surface-elevated px-1 pb-2 pt-1.5">
    <RouterLink
      v-for="tab in TABS"
      :key="tab.id"
      :to="tab.to"
      class="flex flex-1 flex-col items-center gap-0.5 rounded-lg py-1 text-[10.5px] font-semibold text-content-muted"
      :class="activeTab === tab.id && 'text-accent'"
    >
      {{ t(tab.labelKey) }}
    </RouterLink>
  </nav>
</template>
