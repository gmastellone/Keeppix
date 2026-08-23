<script setup lang="ts">
// Fase 11 Task 6 (8/N) — documento funzionale §5.2-§5.3 (header
// mobile), verificato riga per riga (righe 962-1052).
//
// Titolo per rotta: `ROUTE_TITLE_KEYS` (`src/nav/routeTitles.ts`),
// condivisa con AppTopbar (Task 6 2/N, 6/N) — stesso identico testo
// per ogni rotta oggi coperta, non una seconda copia divergente.
// `/more` **non** è in quella mappa: il documento (§5.8) dice
// esplicitamente che le viste `libreria`/`cartelle` "esistono solo
// nella shell mobile" e restano a briciola vuota tornando a desktop —
// qui, mobile-only, il titolo "Altro" è aggiunto a parte.
//
// Freccia indietro (§5.3.1): il mockup ha tre rami di priorità
// (dettaglio album aperto → griglia Album; culling/bulkEdit → Foto;
// altrimenti → Altro). Il primo ramo non è raggiungibile qui: nessuna
// rotta espone oggi un "dettaglio album aperto" osservabile
// dall'esterno della vista (stesso debito già dichiarato per
// AppSidebar e AppTopbar, Task 3/13/15/16). Restano gli altri due.
//
// Pulsante imbuto culling: badge dal dato reale già usato da
// AppSidebar (`shell.badges.culling`), non un segnaposto.
//
// Menu account: il documento (§5.2.c) elenca Profilo/Impostazioni/
// Esci — qui solo Esci, stesso ambito già dichiarato per il menu
// account desktop di AppSidebar (Profilo e Impostazioni non hanno
// ancora una vista, Task 14).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import Avatar from '@/components/ui/Avatar.vue'
import Popover from '@/components/ui/Popover.vue'
import { ROUTE_TITLE_KEYS } from '@/nav/routeTitles'
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

const ROOT_ROUTES = new Set(['/', '/search', '/albums', '/more'])
const BACK_TO_FOTO = new Set(['/culling', '/batch-edit'])

const showBack = computed(() => !ROOT_ROUTES.has(route.path))

const title = computed(() => {
  const key = ROUTE_TITLE_KEYS[route.path]
  if (key) return t(key)
  if (route.path === '/more') return t('nav.more')
  return t('app.name')
})

function goBack() {
  void router.push(BACK_TO_FOTO.has(route.path) ? '/' : '/more')
}

const accountMenuOpen = ref(false)

async function signOut() {
  await session.logout()
  await router.push('/login')
}
</script>

<template>
  <div class="flex h-[52px] shrink-0 items-center justify-between border-b border-border px-3.5">
    <div class="flex min-w-0 items-center gap-1">
      <button
        v-if="showBack"
        type="button"
        class="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-content-muted hover:bg-border/40"
        :aria-label="t('common.back')"
        @click="goBack"
      >
        ←
      </button>
      <span class="truncate text-[15.5px] font-bold">{{ title }}</span>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      <RouterLink
        v-if="route.path === '/'"
        to="/culling"
        class="relative rounded-lg px-2 py-1.5 text-sm font-semibold text-content-muted hover:bg-border/40"
        :aria-label="t('culling.entry')"
      >
        {{ t('culling.entry') }}
        <span
          v-if="shell.badges.culling > 0"
          class="absolute -right-1.5 -top-1.5 min-w-[16px] rounded-full bg-danger px-1 text-center text-[10px] font-bold text-white"
        >
          {{ shell.badges.culling }}
        </span>
      </RouterLink>
      <Popover
        v-if="session.user"
        v-model:open="accountMenuOpen"
        side="bottom"
        align="end"
      >
        <template #trigger>
          <button
            type="button"
            class="flex h-7 w-7 items-center justify-center rounded-full"
          >
            <Avatar :name="session.user.display_name" />
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
  </div>
</template>
