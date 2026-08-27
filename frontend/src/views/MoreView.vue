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
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { startLiveEvents, type LiveSocket } from '@/api/events'
import { useSessionStore } from '@/stores/session'
import { useShellStore } from '@/stores/shell'

const { t } = useI18n()
const session = useSessionStore()
const shell = useShellStore()

// Aggiunto, non nel documento funzionale (§6 non lo prevede): indicatore
// di sola visualizzazione per le due operazioni automatiche senza alcun
// innesco utente — `AiAnalysis`/`FaceDetection` (finestre in background,
// mai una rotta HTTP a differenza di `LibraryScan`/`BulkRename`, vedi
// `scripts/wired-exceptions.txt`, Task 16 Fase 10). Nessun pulsante
// pausa/annulla di proposito: solo visibilità di cosa sta succedendo in
// background, non un controllo. `operation.progress` non porta `kind`
// (Task 16, spec originale) — il tipo si legge dalla stringa `phase`,
// l'unica che i due job impostano davvero (`embed.rs`: "embedding",
// `detect_faces.rs`: "detecting"); `LibraryScan` resta a `''` per tutta
// la corsa e `BulkRename` usa "renaming"/"undoing" — entrambi ignorati
// qui, hanno già una superficie propria altrove (`ProblemsView.vue`,
// `RenameFormulaDialog.vue`).
type BackgroundKind = 'ai_analysis' | 'face_detection'
const PHASE_TO_KIND: Record<string, BackgroundKind> = {
  embedding: 'ai_analysis',
  detecting: 'face_detection'
}
interface BackgroundOp {
  operationId: string
  kind: BackgroundKind
  done: number
  total: number | null
}
const backgroundOps = ref<Map<string, BackgroundOp>>(new Map())
const backgroundOpList = computed(() => Array.from(backgroundOps.value.values()))
let live: LiveSocket | undefined

interface OperationProgressPayload {
  operation_id: string
  done: number
  total: number | null
  phase: string
}

onMounted(() => {
  live = startLiveEvents((msg) => {
    if (msg.type !== 'operation.progress') return
    const payload = msg.payload as OperationProgressPayload
    const kind = PHASE_TO_KIND[payload.phase]
    if (kind) {
      backgroundOps.value = new Map(backgroundOps.value).set(payload.operation_id, {
        operationId: payload.operation_id,
        kind,
        done: payload.done,
        total: payload.total
      })
      return
    }
    // Fase terminale ("done"/"cancelled"/"failed") o di un altro tipo di
    // operazione: se la conoscevamo, esce dal riquadro.
    if (backgroundOps.value.has(payload.operation_id)) {
      const next = new Map(backgroundOps.value)
      next.delete(payload.operation_id)
      backgroundOps.value = next
    }
  })
})

onUnmounted(() => {
  live?.close()
})

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
    <template v-if="backgroundOpList.length > 0">
      <p class="mb-2 mt-0.5 px-0.5 text-[11px] font-bold uppercase tracking-wide text-content-muted">
        {{ t('nav.backgroundActivity') }}
      </p>
      <ul class="mb-[18px] overflow-hidden rounded-xl border border-border">
        <li
          v-for="op in backgroundOpList"
          :key="op.operationId"
          class="border-b border-border px-3.5 py-3 last:border-b-0"
        >
          <p class="text-[13px] font-semibold">
            {{
              op.total !== null
                ? t(`backgroundOps.${op.kind === 'ai_analysis' ? 'aiAnalysisKnown' : 'faceDetectionKnown'}`, {
                  done: op.done,
                  total: op.total
                })
                : t(`backgroundOps.${op.kind === 'ai_analysis' ? 'aiAnalysisUnknown' : 'faceDetectionUnknown'}`, {
                  done: op.done
                })
            }}
          </p>
          <div class="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-border/40">
            <div
              class="h-full rounded-full bg-accent transition-[width]"
              :class="{ 'animate-pulse': op.total === null }"
              :style="{ width: op.total ? `${Math.min(100, (op.done / op.total) * 100)}%` : '30%' }"
            />
          </div>
        </li>
      </ul>
    </template>
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
