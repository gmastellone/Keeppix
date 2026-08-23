<script setup lang="ts">
// Fase 11, sottosistema di caricamento (`docs/ui/caricamento-nuove-foto.md`
// §6, "La coda"), verificato riga per riga **e** contro `renderUploadPanel`/
// `uploadRowHTML`/`uploadCounts` del prototipo (righe 2939-3078) per i
// dettagli che il documento non specifica per esteso (l'ordine esatto del
// titolo, il testo di ogni riga, la lista di file troncata).
//
// Sostituisce interamente il pannello generico fluttuante in basso a
// destra (upload 1/N-6/N lo dicevano già): quel pattern è esattamente il
// pulsante flottante che il documento §2 dice scartato, non solo un altro
// stile.
//
// Nome a una parola per il file (combacia col piano), ma il componente ha
// comunque un nome di due parole: evita l'avviso `multi-word-component-names`
// senza doverlo disabilitare per un componente fuori da `ui/`.
defineOptions({ name: 'UploadPanel' })

import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import DestinationChip from '@/components/DestinationChip.vue'
import { useUploadStore, type UploadSessionState } from '@/stores/upload'

const { t } = useI18n()
const router = useRouter()
const upload = useUploadStore()

// Deviazione dichiarata dal prototipo, non un suo rispecchiamento: la
// riga 2979 del mockup (`if(!u.open || !u.items.length){...return}`)
// nasconde il pannello anche per un lotto tutto scartato — un rilascio
// di soli RAW non darebbe **nessun** riscontro visibile, in contraddizione
// con il principio dichiarato dallo stesso documento (§4.1: "il rifiuto
// dei RAW non è un errore, è una spiegazione" — un'spiegazione che va
// vista). Corretto qui: il pannello resta raggiungibile anche a
// `sessions` vuoto, finché c'è qualcosa da spiegare.
const visible = computed(
  () => upload.panelOpen && (upload.sessions.length > 0 || upload.rejectedRaw.length > 0 || upload.rejectedUnsupported.length > 0)
)

const counts = computed(() => {
  const s = upload.sessions
  const isPending = (x: UploadSessionState) => x.status === 'queued' || x.status === 'uploading' || x.status === 'paused'
  const isFinished = (x: UploadSessionState) => x.status === 'done' || x.status === 'skipped' || x.status === 'error'
  return {
    total: s.length,
    done: s.filter((x) => x.status === 'done').length,
    skipped: s.filter((x) => x.status === 'skipped' || (x.status === 'done' && x.collision === 'skipped_duplicate')).length,
    error: s.filter((x) => x.status === 'error').length,
    pending: s.filter(isPending).length,
    finished: s.filter(isFinished).length
  }
})

/** Stesso effetto del `u.paused` globale del prototipo: tutto ciò che è
 * ancora in sospeso è in pausa insieme (`pauseAll`/`resumeAll`). */
const allPaused = computed(() => {
  const pending = upload.sessions.filter((s) => s.status === 'queued' || s.status === 'uploading' || s.status === 'paused')
  return pending.length > 0 && pending.every((s) => s.status === 'paused')
})

/** Priorità esatta della riga 3001 del mockup. */
const panelTitle = computed(() => {
  if (counts.value.pending === 0) return t('upload.panel.titleDone')
  if (upload.needsDestination) return t('upload.panel.titleNeedsDestination')
  if (allPaused.value) return t('upload.panel.titlePaused')
  return t('upload.panel.titleUploading')
})

function togglePause(): void {
  if (allPaused.value) upload.resumeAll()
  else upload.pauseAll()
}

function close(): void {
  upload.panelOpen = false
}

function goCulling(): void {
  upload.panelOpen = false
  void router.push('/culling')
}

/**
 * `useLightboxRoute` (TimelineView) risolve `?photo=<id>` anche per un
 * asset non ancora caricato in pagina — carica da remoto se non lo
 * trova in locale (righe 42-45 del composable: "*loadRemote*"). Non un
 * ripiego: lo stesso meccanismo pensato per "mando a un collega il
 * link a questa foto" (Task 3) apre per davvero la copia già presente.
 */
function seeExisting(session: UploadSessionState): void {
  if (!session.existingAssetId) return
  upload.panelOpen = false
  void router.push({ path: '/', query: { photo: session.existingAssetId } })
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let value = bytes / 1024
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unitIndex]}`
}

function progressPercent(session: UploadSessionState): number {
  if (session.expectedSize <= 0) return 0
  return Math.min(100, Math.round((session.receivedBytes / session.expectedSize) * 100))
}

/** Un "done" con `collision==='skipped_duplicate'` (il server, non il
 * pre-check, ha trovato il duplicato a fine caricamento) si presenta
 * come "saltato" a tutti gli effetti — stesso trattamento già nel
 * componente precedente. */
function displayState(session: UploadSessionState): 'queued' | 'uploading' | 'paused' | 'done' | 'skipped' | 'error' {
  if (session.status === 'done' && session.collision === 'skipped_duplicate') return 'skipped'
  return session.status
}

function metaText(session: UploadSessionState): string {
  const size = formatBytes(session.expectedSize)
  const state = displayState(session)
  if (state === 'uploading') return `${size} · ${progressPercent(session)}%`
  if (state === 'queued') return `${size} · ${t('upload.row.queued')}`
  if (state === 'paused') return `${size} · ${t('upload.row.paused')}`
  return size
}

const BADGE_KEYS: Partial<Record<ReturnType<typeof displayState>, string>> = {
  done: 'upload.row.badgeDone',
  skipped: 'upload.row.badgeSkipped',
  error: 'upload.row.badgeError'
}

function badgeKey(session: UploadSessionState): string | null {
  return BADGE_KEYS[displayState(session)] ?? null
}

const BADGE_CLASSES: Partial<Record<ReturnType<typeof displayState>, string>> = {
  done: 'bg-border text-content-muted',
  skipped: 'border border-warn-border bg-warn-tint text-warn',
  error: 'bg-danger-tint text-danger'
}

function badgeClass(session: UploadSessionState): string {
  return BADGE_CLASSES[displayState(session)] ?? ''
}

/** Il testo di esempio del prototipo ("il server non ha risposto") è
 * solo la simulazione demo — qui la riga mostra la ragione reale che lo
 * store porta già (`session.error`, sempre una chiave i18n). */
function subLine(session: UploadSessionState): string | null {
  const state = displayState(session)
  if (state === 'skipped') return t('upload.collision.skipped_duplicate')
  if (state === 'error') return t(session.error ?? 'upload.errors.unknown')
  if (session.status === 'done' && session.collision === 'renamed') {
    return t('upload.collision.renamed', { filename: session.savedFilename ?? session.filename })
  }
  return null
}

/** §4: fino a quattro nomi, poi "e un altro"/"e altri N" — verificato
 * contro `uploadAndOthers()` del prototipo (riga 2892). */
function fileListText(names: string[]): string {
  const shown = names.slice(0, 4).join(', ')
  if (names.length <= 4) return shown
  // Spazio esplicito nel codice, non nella chiave i18n (fragile da
  // perdere in una traduzione): il prototipo lo tiene dentro
  // `uploadAndOthers()` stessa (riga 2892, `' e un altro'`), qui è
  // separato per lasciare la chiave senza spazi impliciti.
  return `${shown} ${t('upload.rejectedRaw.andMore', { n: names.length - 4 }, { plural: names.length - 4 })}`
}

const footerParts = computed(() => {
  const parts = [t('upload.footer.done', { n: counts.value.done }, { plural: counts.value.done })]
  if (counts.value.skipped > 0) {
    parts.push(t('upload.footer.skipped', { n: counts.value.skipped }, { plural: counts.value.skipped }))
  }
  if (counts.value.error > 0) {
    parts.push(t('upload.footer.error', { n: counts.value.error }, { plural: counts.value.error }))
  }
  return parts.join(' · ')
})
</script>

<template>
  <template v-if="visible">
    <div
      class="fixed inset-0 z-40 hidden bg-black/40 max-md:block"
      @click="close"
    />
    <div
      role="dialog"
      :aria-label="t('upload.title')"
      class="fixed z-50 flex flex-col overflow-hidden border border-border-strong bg-card-bg
             shadow-[0_18px_44px_rgba(0,0,0,0.28)]
             max-md:inset-x-0 max-md:bottom-0 max-md:max-h-[72vh] max-md:rounded-t-2xl
             md:bottom-3 md:left-3 md:w-[344px] md:max-h-[460px] md:rounded-xl"
    >
      <div class="flex items-center gap-2 border-b border-border px-3.5 py-2.5">
        <p class="flex-1 text-[13px] font-bold">
          {{ panelTitle }}
        </p>
        <button
          v-if="counts.pending > 0 && !upload.needsDestination"
          type="button"
          class="rounded-md p-1.5 text-content-muted hover:bg-border/40"
          :aria-label="allPaused ? t('upload.resume') : t('upload.pause')"
          @click="togglePause"
        >
          {{ allPaused ? '▷' : '❙❙' }}
        </button>
        <button
          type="button"
          class="rounded-md p-1.5 text-content-muted hover:bg-border/40"
          :aria-label="t('upload.close')"
          @click="close"
        >
          ✕
        </button>
      </div>

      <div class="border-b border-border px-3.5 py-2.5">
        <DestinationChip />
      </div>

      <div class="flex-1 overflow-y-auto px-3.5 py-2.5">
        <div
          v-if="upload.rejectedRaw.length > 0"
          role="note"
          class="mb-2.5 rounded-lg border border-warn-border bg-warn-tint px-3 py-2.5 text-[12px]"
        >
          <p class="font-bold text-warn">
            {{ t('upload.rejectedRaw.title', { n: upload.rejectedRaw.length }, { plural: upload.rejectedRaw.length }) }}
          </p>
          <p class="mt-1">
            {{ t('upload.rejectedRaw.explanation') }}
          </p>
          <p class="mt-1 text-content-muted">
            {{ fileListText(upload.rejectedRaw) }}
          </p>
          <button
            type="button"
            class="mt-2 rounded-md border border-border px-2.5 py-1 text-[12px] font-semibold"
            @click="goCulling"
          >
            {{ t('upload.rejectedRaw.openCulling') }}
          </button>
        </div>

        <div
          v-if="upload.rejectedUnsupported.length > 0"
          role="note"
          class="mb-2.5 rounded-lg border border-warn-border bg-warn-tint px-3 py-2.5 text-[12px]"
        >
          <p class="font-bold text-warn">
            {{
              t(
                'upload.rejectedUnsupported.title',
                { n: upload.rejectedUnsupported.length },
                { plural: upload.rejectedUnsupported.length }
              )
            }}
          </p>
          <p class="mt-1">
            {{ t('upload.rejectedUnsupported.explanation') }}
          </p>
          <p class="mt-1 text-content-muted">
            {{ fileListText(upload.rejectedUnsupported) }}
          </p>
        </div>

        <div
          v-for="session in upload.sessions"
          :key="session.id"
          class="flex items-start gap-2.5 border-b border-border py-2 last:border-b-0"
        >
          <div class="min-w-0 flex-1">
            <p class="truncate text-[12px] font-semibold">
              {{ session.filename }}
            </p>
            <p class="text-[11px] text-content-muted">
              {{ metaText(session) }}
            </p>
            <div
              v-if="session.status === 'uploading'"
              class="mt-1 h-[3px] overflow-hidden rounded-full bg-border-strong"
            >
              <div
                class="h-full rounded-full bg-accent transition-[width] duration-200 ease-linear"
                :style="{ width: `${progressPercent(session)}%` }"
              />
            </div>
            <p
              v-if="subLine(session)"
              class="mt-0.5 text-[11px] text-content-muted"
            >
              {{ subLine(session) }}
              <button
                v-if="displayState(session) === 'error'"
                type="button"
                class="text-accent underline"
                @click="upload.retry(session.id)"
              >
                {{ t('upload.retry') }}
              </button>
              <button
                v-else-if="displayState(session) === 'skipped' && session.existingAssetId"
                type="button"
                class="text-accent underline"
                @click="seeExisting(session)"
              >
                {{ t('upload.seeExisting') }}
              </button>
            </p>
          </div>
          <span
            v-if="badgeKey(session)"
            class="shrink-0 rounded-[5px] px-1.5 py-0.5 text-[10px] font-bold uppercase tracking-[0.02em]"
            :class="badgeClass(session)"
          >
            {{ t(badgeKey(session)!) }}
          </span>
        </div>
      </div>

      <div class="flex items-center gap-3 border-t border-border px-3.5 py-2.5 text-[12px]">
        <span class="flex-1 text-content-muted">{{ footerParts }}</span>
        <button
          v-if="counts.finished > 0"
          type="button"
          class="text-accent underline"
          @click="upload.removeCompleted()"
        >
          {{ t('upload.clearCompleted') }}
        </button>
        <button
          v-if="counts.pending > 0"
          type="button"
          class="text-accent underline"
          @click="upload.cancelAll()"
        >
          {{ t('upload.cancelAll') }}
        </button>
      </div>
    </div>
  </template>
</template>
