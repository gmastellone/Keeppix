<script setup lang="ts">
// Fase 11 Task 6 (2/N) — documento funzionale §4 ("Barra superiore /
// breadcrumb"), verificato riga per riga (righe 830-929) e contro il
// markup reale del mockup (righe 1434-1439, 3212-3247).
//
// Ambito dichiarato:
// - Briciole di pane: solo il segmento "corrente", per le sole rotte
//   con una destinazione reale oggi (stesso elenco di AppSidebar). Il
//   segmento "genitore" del mockup (`Cartelle / <nome>`, `Album /
//   <nome>`, `Culling / <nome lotto>`) non è mai raggiungibile: nessuna
//   di queste rotte porta oggi uno stato "aperto" osservabile
//   dall'esterno della vista (stesso debito già dichiarato in
//   AppSidebar per il gruppo "Cartelle" — Task 13/15/16).
//
// Task 6 (6/N): `/folders`, `/users`, `/groups` sono entrate in questa
// mappa — inizialmente (Task 6 2/N) restavano a briciola vuota,
// "comportamento letterale del prototipo per le viste non mappate".
// Quella scelta presumeva che il proprio `<h1>` di ciascuna vista
// facesse comunque da titolo — ma spogliare quelle intestazioni
// (Task 6, questo stesso sotto-passo) toglie anche quell'`<h1>`: senza
// una voce qui, quelle tre pagine resterebbero **senza alcun titolo**,
// non fedeli al comportamento del prototipo che le ignora perché non
// esistono lì. Sono destinazioni reali di questa app (aggiunte a
// `AppSidebar` nel Task 6 4/N): meritano una briciola reale, riusando
// `folders.title`/`users.title`/`groups.title`, non un'invenzione.
// - Il comando "Carica" (`#uploadTopBtn`, righe 1438 e 3236-3247 del
//   mockup; documento `caricamento-nuove-foto.md` §3.2) — sempre
//   "Carica", mai "Carica qui": nessuna vista porta oggi un
//   `currentFolder` osservabile, stesso debito già dichiarato più
//   volte in questo sottosistema (`UploadDropVeil.vue`, `stores/
//   upload.ts`).
// - L'interruttore di tema, già rimosso nel mockup stesso (commento del
//   codice sorgente, §4.2), non esiste qui per lo stesso motivo: vive
//   in Impostazioni (Task 14).
//
// Correzione di accessibilità rispetto al prototipo (stessa politica
// già applicata in AppSidebar): il campo di ricerca è `readonly` e nel
// mockup risponde solo al click del mouse ("Deviazione da SP-8", §4.5).
// Qui Invio e Spazio attivano lo stesso comportamento del click.
import { computed, nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'

import Tooltip from '@/components/ui/Tooltip.vue'
import { useUploadPicker, UPLOAD_ACCEPT } from '@/composables/useUploadPicker'
import { activeAlbumName, activePersonName, ROUTE_TITLE_KEYS } from '@/nav/routeTitles'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const inputEl = ref<HTMLInputElement | null>(null)
const { open: openPicker, onChange } = useUploadPicker(inputEl)

// §42.8: la sola briciola con un vero segmento genitore ("Album /
// <nome>") — le altre rotte restano a un unico segmento (vedi il
// commento in testa al file). `/albums/new` (Task 12 2/N) non è un
// album aperto: resta sulla mappa piatta sotto, non su questo ramo.
const albumBreadcrumbName = computed(() =>
  route.path.startsWith('/albums/') && route.path !== '/albums/new' ? activeAlbumName.value : null
)

// §32.8: "Persone / <b>Nome</b> quando è aperto un dettaglio" — ma solo
// se la persona **ha** un nome: senza nome resta la sola briciola piatta
// "Persone" (nessun secondo segmento "Persona senza nome" inventato).
const personBreadcrumbName = computed(() =>
  route.path.startsWith('/persons/') ? activePersonName.value : null
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
