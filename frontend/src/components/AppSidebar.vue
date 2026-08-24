<script setup lang="ts">
// Fase 11 Task 6 (documento funzionale §2, "Sidebar di navigazione").
// Verificato riga per riga contro il mockup (righe 1393-1401 per il
// marchio, §2.2 per l'elenco completo) — non solo il riassunto del
// piano.
//
// Ambito dichiarato: **non** tutte le voci canoniche. Costruite solo
// quelle con una destinazione reale in questa sessione — Foto, Cerca,
// Culling, Mappa, Condivisioni, Preferiti (Task 7, 3/N), Album, Cartelle,
// Cestino, Problemi (più Utenti/Gruppi per un amministratore). Tolte,
// debito verso le Tranche che le costruiranno per davvero:
// - "Persone" (Task 16, Tranche D) — nessuna vista Persone esiste.
// - Il gruppo "IA" intero — Tag e categorie/Revisione/Analisi libreria
//   sono Task 15, Tranche C.
// - "Duplicati" dentro "Manutenzione" — Task 13, Tranche B, non ancora
//   fatto (Cestino e Problemi sì, esistono già).
// Ogni voce qui presente è un vero <RouterLink>, quindi raggiungibile
// da tastiera per costruzione — il prototipo non lo è (§2.5, "nessuna
// voce della sidebar è raggiungibile da tastiera").
//
// Task 6 (4/N): "Cartelle" qui **non** è il gruppo del mockup (§2,
// lettera d — una riga `.folder-item` per ogni cartella, con salto
// diretto a una timeline filtrata): quella timeline filtrata non
// esiste ancora, stesso debito dichiarato per le rotte di dettaglio
// nel Task 3. È invece un unico collegamento a `/folders`
// (`FoldersView`), l'albero cartelle reale dell'app — una funzione
// di organizzazione (spostare foto fra cartelle) diversa da quella
// del mockup, non modellata lì. Aggiunta qui perché rimuovere
// l'intestazione improvvisata di `TimelineView` (Task 6, prossimo
// sotto-passo) senza prima darle una destinazione reale in
// `AppSidebar` la renderebbe irraggiungibile — vicolo cieco trovato
// scrivendo quel passo, non un'aggiunta pianificata.
//
// "Amministrazione" (Utenti/Gruppi, solo per `role==='admin'`) non è
// nel documento funzionale: il mockup è a singolo utente, non
// modella l'amministrazione multiutente che il backend reale invece
// ha. Stesso motivo di "Cartelle": erano raggiungibili solo
// dall'intestazione improvvisata di `TimelineView`, altrimenti un
// vicolo cieco dopo averla tolta.
//
// `UploadQueueStrip` (sottosistema di caricamento, §6.1): "nel piede
// della sidebar, sopra 'Spazio libero'" — posizione esatta, non solo
// "da qualche parte nel piede".
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import Avatar from '@/components/ui/Avatar.vue'
import NavGroup from '@/components/ui/NavGroup.vue'
import Popover from '@/components/ui/Popover.vue'
import UploadQueueStrip from '@/components/UploadQueueStrip.vue'
import { useSessionStore } from '@/stores/session'
import { useShellStore } from '@/stores/shell'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const session = useSessionStore()
const shell = useShellStore()

onMounted(() => {
  if (!shell.loaded) void shell.load()
})

const NAV_TOP = [
  { to: '/', labelKey: 'nav.foto', badge: false },
  { to: '/search', labelKey: 'nav.cerca', badge: false },
  { to: '/culling', labelKey: 'culling.entry', badge: true },
  { to: '/map', labelKey: 'maps.entry', badge: false },
  { to: '/shares', labelKey: 'shares.entry', badge: false }
] as const

const MAINT_ITEMS = [
  { to: '/trash', labelKey: 'trash.entry' },
  { to: '/problems', labelKey: 'problems.title' }
] as const

const ADMIN_ITEMS = [
  { to: '/users', labelKey: 'users.entry' },
  { to: '/groups', labelKey: 'groups.entry' }
] as const

// `/albums` resta evidenziata anche dentro il dettaglio di un album
// (Task 12 1/N, prima rotta con figli reali: `/albums/:id`) — stessa
// idea del mockup ("il click su Album azzera anche `state.openAlbum`",
// §41.8), qui capovolta: si è ancora dentro la sezione Album anche col
// dettaglio aperto.
function isActive(to: string): boolean {
  if (to === '/albums') return route.path === to || route.path.startsWith('/albums/')
  return route.path === to
}

const maintActive = computed(() => MAINT_ITEMS.some((item) => isActive(item.to)))
const adminActive = computed(() => ADMIN_ITEMS.some((item) => isActive(item.to)))

const accountMenuOpen = ref(false)

async function signOut() {
  await session.logout()
  await router.push('/login')
}

/** Terza copia locale della stessa formattazione già duplicata (e
 * divergente: 1024 in `UploadPanel.vue`, 1000 in
 * `MapsOfflineView.vue`) — debito noto, non risolto qui: unificarle
 * tocca due viste che questo task non sta toccando. */
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
          <Avatar :name="session.user.display_name" />
          <span class="text-[13px] font-semibold">{{ session.user.display_name }}</span>
        </button>
      </template>
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
