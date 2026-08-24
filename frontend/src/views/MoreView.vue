<script setup lang="ts">
// Fase 11 Task 6 (7/N) — documento funzionale §6 ("Pagina 'Altro' /
// Libreria su mobile"), verificato riga per riga (righe 1135-1281).
//
// Elenco piatto SENZA accordion — il documento lo dice esplicitamente
// (§6.6: "nessuna animazione... le righe compaiono già tutte aperte";
// §6.1: "niente più accordion da aprire"), a differenza della sidebar
// desktop che usa `NavGroup`. Non riusato qui apposta.
//
// Ambito dichiarato: stesse destinazioni reali di AppSidebar (Task 6
// 1/N e 4/N; Preferiti aggiunta nel Task 7 3/N; Persone nel Task 16
// 1/N), non i dodici gruppi canonici del mockup.
//
// "Persone" vive qui sotto "Libreria" (§31.8: "da mobile 'Altro' →
// gruppo 'Libreria' → 'Persone'"), **non** in `NAV_TOP` come su
// desktop (`AppSidebar.vue`) — posizionamento diverso per piattaforma,
// dichiarato dallo stesso paragrafo del documento, non una divergenza
// introdotta qui.
//
// Il gruppo "IA" (Task 15) ha due voci reali: "Tag e categorie" e
// "Revisione" (badge `shell.badges.revision`, stesso dato di
// `AppSidebar.vue`). "Analisi libreria" resta fuori per sempre, non solo
// per ora: nessuna rotta la legge (stesso commento esteso in
// `AppSidebar.vue`).
// - "Condivisi con me" / "Le mie condivisioni" come due righe
//   distinte: `SharesView` non ha le due schede `state.shareTab` del
//   mockup, è un'unica vista — collassate in una sola riga
//   "Condivisioni" verso `/shares`.
// - Il valore secondario "N cartelle" della riga "Cartelle": nessun
//   conteggio è disponibile senza una chiamata dedicata solo per
//   questo badge (stesso motivo per cui `FolderView` non porta un
//   conteggio foto, Task 6 1/N).
// - La sotto-pagina "Cartelle" con le card a gradiente (copertina
//   dalla prima foto, conteggio foto): non è `/folders` (l'albero
//   cartelle reale, Task 6 4/N) — quella sotto-pagina non esiste,
//   "Cartelle" porta direttamente a `/folders`.
// Aggiunta, non nel mockup: "Amministrazione" (Utenti/Gruppi, solo
// `role==='admin'`), stesso motivo di AppSidebar — funzione reale del
// backend multiutente che il mockup a singolo utente non modella.
//
// Nessuna icona: questo frontend non ha ancora un sistema di icone
// (stesso stato di fatto di AppSidebar, mai dichiarato esplicitamente
// lì — lo è qui). Ogni riga è un vero <RouterLink>, raggiungibile da
// tastiera per costruzione — il prototipo non lo è (§6.5: le righe
// sono `<div>` senza `tabindex` né `bindActivatable`).
import { useI18n } from 'vue-i18n'

import { useSessionStore } from '@/stores/session'
import { useShellStore } from '@/stores/shell'

const { t } = useI18n()
const session = useSessionStore()
const shell = useShellStore()

const LIBRARY_ITEMS = [
  { to: '/folders', labelKey: 'folders.entry' },
  { to: '/map', labelKey: 'maps.entry' },
  { to: '/shares', labelKey: 'shares.entry' },
  { to: '/favorites', labelKey: 'favorites.entry' },
  { to: '/persons', labelKey: 'persons.entry' }
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

const IA_ITEMS = [
  { to: '/tags', labelKey: 'tags.entry', badge: false },
  { to: '/review', labelKey: 'review.entry', badge: true }
] as const
</script>

<template>
  <main class="p-3.5">
    <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
      {{ t('nav.libraryGroup') }}
    </p>
    <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
      <li
        v-for="item in LIBRARY_ITEMS"
        :key="item.to"
      >
        <RouterLink
          :to="item.to"
          class="flex items-center gap-3 border-b border-border px-3.5 py-3 text-[13.5px] font-semibold
                 last:border-b-0 hover:bg-border/30"
        >
          {{ t(item.labelKey) }}
        </RouterLink>
      </li>
    </ul>

    <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
      {{ t('nav.manutenzione') }}
    </p>
    <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
      <li
        v-for="item in MAINT_ITEMS"
        :key="item.to"
      >
        <RouterLink
          :to="item.to"
          class="flex items-center gap-3 border-b border-border px-3.5 py-3 text-[13.5px] font-semibold
                 last:border-b-0 hover:bg-border/30"
        >
          {{ t(item.labelKey) }}
        </RouterLink>
      </li>
    </ul>

    <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
      {{ t('nav.ia') }}
    </p>
    <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
      <li
        v-for="item in IA_ITEMS"
        :key="item.to"
      >
        <RouterLink
          :to="item.to"
          class="flex items-center justify-between gap-3 border-b border-border px-3.5 py-3 text-[13.5px] font-semibold
                 last:border-b-0 hover:bg-border/30"
        >
          <span>{{ t(item.labelKey) }}</span>
          <span
            v-if="item.badge && shell.badges.revision > 0"
            class="min-w-[18px] rounded-full bg-danger px-1.5 text-center text-[11px] font-bold text-white"
          >
            {{ shell.badges.revision }}
          </span>
        </RouterLink>
      </li>
    </ul>

    <template v-if="session.user?.role === 'admin'">
      <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
        {{ t('nav.amministrazione') }}
      </p>
      <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
        <li
          v-for="item in ADMIN_ITEMS"
          :key="item.to"
        >
          <RouterLink
            :to="item.to"
            class="flex items-center gap-3 border-b border-border px-3.5 py-3 text-[13.5px] font-semibold
                   last:border-b-0 hover:bg-border/30"
          >
            {{ t(item.labelKey) }}
          </RouterLink>
        </li>
      </ul>
    </template>
  </main>
</template>
