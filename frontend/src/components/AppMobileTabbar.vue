<script setup lang="ts">
// Fase 11 Task 6 (8/N) — documento funzionale §5.2.b, §5.3.8-11,
// §5.7 (tab bar mobile), verificato riga per riga (righe 1004-1014,
// 1048-1052, 1097-1105).
//
// Quattro schede reali, stesso ordine ed etichette esatte del
// documento. Nessuna icona (stesso stato di fatto dichiarato in
// MoreView.vue — questo frontend non ne ha ancora una).
//
// "Attiva" (§5.7): il documento assegna esplicitamente culling e
// bulkEdit alla scheda "Foto" (non "Altro" — si entra in culling SOLO
// dal pulsante imbuto della vista Foto, §4.2 del piano complessivo),
// e un lungo elenco di viste alla scheda "Altro". Qui, con le sole
// rotte reali di questa app: "Altro" per tutto ciò che MoreView.vue
// elenca (`/folders`, `/map`, `/shares`, `/trash`, `/problems`,
// `/users`, `/groups`, `/more` stessa); "Foto" anche per `/culling` e
// `/batch-edit`.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

const { t } = useI18n()
const route = useRoute()

const ALTRO_ROUTES = new Set(['/folders', '/map', '/shares', '/trash', '/problems', '/users', '/groups', '/more'])
const FOTO_ROUTES = new Set(['/', '/culling', '/batch-edit'])

type TabId = 'foto' | 'cerca' | 'album' | 'altro'

const activeTab = computed<TabId | null>(() => {
  // `/albums/:id` (Task 12 1/N): stesso principio di `AppSidebar.isActive`
  // — il dettaglio di un album resta dentro la scheda "Album".
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
