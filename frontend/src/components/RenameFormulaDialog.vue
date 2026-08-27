<script setup lang="ts">
// SP-62 (documento funzionale §62, "Dialog 'Rinomina con formula'"). Quattro
// dei cinque punti d'ingresso del documento sono coperti: "selection"
// (§13.3 campo 7, "Modifica in blocco", Task 7 — e §15.3 controllo 25, la
// barra di selezione del lotto di culling, Task 17 4/N), "single" (§18/§20,
// il pulsante "Rinomina…" del lightbox, Task 8 — sottotitolo distinto, "1
// foto — {nome file}", non "1 foto selezionata": per questo `subtitle` è
// sovrascrivibile dal chiamante invece di dedurlo dal solo conteggio,
// §62.8 lega il testo al **punto d'ingresso**, non al numero di foto) e
// "lotto intero" (§15.3 controllo 19, "Rinomina lotto…", Task 17 4/N —
// l'unico ambito con `hasSubfolders`, §62.3e: `restrictedAssets` è il
// sottoinsieme `cullState==='pending'`, l'ambito di partenza con
// l'interruttore spento; `assets` resta l'intero lotto, usato quando
// l'interruttore è acceso). Resta debito dichiarato solo il quinto
// (cartella aperta in Timeline).
//
// Nessuna logica di token/slug/sanificazione duplicata qui: il motore è
// server-side (`keeppix-domain::rename::render_base`, Fase 9), verificato
// leggendo il codice reale prima di scrivere questo dialog — l'anteprima
// chiama `previewRename` a ogni cambio di schema, non ricalcola nulla in
// locale.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { startLiveEvents, type LiveSocket } from '@/api/events'
import { cancelOperation } from '@/api/operations'
import { applyRenameBatch, previewRename, type RenamePreviewItem } from '@/api/rename'
import type { TimelineAsset } from '@/api/timeline'
import { useToastStore } from '@/stores/toast'

import Dialog from './ui/Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{
  assets: TimelineAsset[]
  subtitle?: string
  hasSubfolders?: boolean
  restrictedAssets?: TimelineAsset[]
}>()

const { t } = useI18n()
const toast = useToastStore()

// §62.3e: spento di partenza, tocca solo `restrictedAssets` (le foto
// "da vedere"); acceso, tocca l'intero `assets` (presi e scartati inclusi).
// Mai ricordato fra un'apertura e l'altra, stessa regola dello schema.
const includeSubfolders = ref(false)
const scopedAssets = computed(() =>
  props.hasSubfolders && !includeSubfolders.value ? (props.restrictedAssets ?? []) : props.assets
)

// Valore iniziale sempre lo stesso a ogni apertura (§62.2 punto 4): non è
// ricordato fra un'apertura e l'altra.
const DEFAULT_SCHEMA = '{data}_{luogo}_{n:3}'
const PLACEHOLDERS = ['data', 'fotocamera', 'obiettivo', 'luogo', 'titolo'] as const

// `applied` (Task 17 4/N): la barra di selezione del lotto vuole svuotare
// la selezione a rinomina riuscita (§15.3 controllo 25) — un evento
// generico invece di far indovinare al chiamante quando l'apply è andato
// a buon fine dal solo v-model:open che torna a `false` (si chiude anche
// su "Annulla", che non deve svuotare nulla).
const emit = defineEmits<{ applied: [] }>()

const schema = ref(DEFAULT_SCHEMA)
const preview = ref<RenamePreviewItem[]>([])
const applying = ref(false)
const schemaInput = ref<HTMLInputElement | null>(null)
let debounceTimer: ReturnType<typeof setTimeout> | undefined

const visiblePreview = computed(() => preview.value.slice(0, 5))
const collisionCount = computed(() => preview.value.filter((item) => item.collides).length)

async function runPreview() {
  const ids = scopedAssets.value.map((asset) => asset.id)
  preview.value = await previewRename(ids, schema.value).catch(() => [])
}

function scheduleRunPreview() {
  if (debounceTimer) clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => void runPreview(), 200)
}

watch(schema, scheduleRunPreview)
watch(includeSubfolders, scheduleRunPreview)

// Debito chiuso il 27 agosto: `POST /assets/batch/rename` non rinomina più
// dentro la richiesta — accoda un job (`keeppix-jobs::rename_batch`,
// `keeppix_db::rename` per il perché) e torna subito `operation_id`. Questo
// dialog ne segue l'avanzamento reale sul WebSocket (`operation.progress`),
// stesso pattern di `ProblemsView.vue` per la ri-scansione di libreria —
// annulla vero, non solo uno spinner indeterminato. Dichiarato **prima**
// del `watch(open, ..., { immediate: true })` sotto: quel watcher chiama
// `resetApplyState` fin dalla prima esecuzione, in `<script setup>` un
// `const` non è sollevato come una funzione — un ordine diverso lancia
// `ReferenceError` all'apertura del componente (visto per davvero).
const activeOperationId = ref<string | null>(null)
const progressDone = ref(0)
const progressTotal = ref<number | null>(null)
const cancelling = ref(false)
let live: LiveSocket | undefined

interface OperationProgressPayload {
  operation_id: string
  done: number
  total: number | null
  phase: string
}

const TERMINAL_PHASES = new Set(['done', 'cancelled', 'failed'])

function resetApplyState() {
  activeOperationId.value = null
  applying.value = false
  cancelling.value = false
  progressDone.value = 0
  progressTotal.value = null
}

function handleOperationProgress(payload: OperationProgressPayload) {
  if (payload.operation_id !== activeOperationId.value) return
  if (!TERMINAL_PHASES.has(payload.phase)) {
    progressDone.value = payload.done
    progressTotal.value = payload.total
    return
  }
  // `cancelRename` sta già aspettando la sua stessa risposta HTTP e mostrerà
  // il proprio toast quando arriva: se questo evento "cancelled" è una
  // corsa con quella richiesta, lascia fare a lei, non duplicare qui.
  if (cancelling.value) return
  if (payload.phase === 'done') {
    const n = payload.done
    toast.show(t('renameFormula.renamedToast', { n }, { plural: n }))
  } else if (payload.phase === 'failed') {
    toast.showError(t('renameFormula.failedToast'))
  }
  // "cancelled" arriva qui senza toast proprio se annullato da un'altra
  // scheda/sessione (non da questo dialog, coperto dalla guardia sopra).
  resetApplyState()
  open.value = false
  emit('applied')
}

onMounted(() => {
  live = startLiveEvents((msg) => {
    if (msg.type === 'operation.progress') {
      handleOperationProgress(msg.payload as OperationProgressPayload)
    }
  })
})

onUnmounted(() => {
  live?.close()
})

watch(
  open,
  (isOpen) => {
    if (!isOpen) return
    // Difensivo: se il dialog viene chiuso a mano (velo/Esc/`X`, reka-ui)
    // mentre una rinomina è in corso, il job in background continua per
    // conto suo — solo il *tracciamento* di questo dialog si perde (nessun
    // "riprendi da dove eri" qui, debito minore non affrontato). Una
    // riapertura successiva non deve però ripartire da uno stato a metà.
    resetApplyState()
    schema.value = DEFAULT_SCHEMA
    includeSubfolders.value = false
    void runPreview()
    void nextTick(() => {
      // Fuoco sul campo schema con il cursore in fondo, non sul testo
      // selezionato (§62.5, "Focus all'apertura").
      const len = schema.value.length
      schemaInput.value?.focus()
      schemaInput.value?.setSelectionRange(len, len)
    })
  },
  { immediate: true }
)

/** Inserisce un segnaposto alla posizione del cursore, sostituendo
 * l'eventuale testo selezionato — poi riposiziona il cursore subito dopo
 * e riporta il focus nel campo (§62.3b). */
function insertToken(token: string) {
  const el = schemaInput.value
  const start = el?.selectionStart ?? schema.value.length
  const end = el?.selectionEnd ?? schema.value.length
  const inserted = `{${token}}`
  schema.value = schema.value.slice(0, start) + inserted + schema.value.slice(end)
  const cursor = start + inserted.length
  void nextTick(() => {
    el?.focus()
    el?.setSelectionRange(cursor, cursor)
  })
}

async function apply() {
  // §62.3f: ricalcola implicito (la preview è già corrente, debounced),
  // poi esce senza fare nulla su collisioni o ambito vuoto — lo stesso
  // guard silenzioso del prototipo, non un errore visibile.
  if (collisionCount.value > 0 || preview.value.length === 0 || applying.value) return
  applying.value = true
  try {
    const accepted = await applyRenameBatch(
      scopedAssets.value.map((asset) => asset.id),
      schema.value
    )
    activeOperationId.value = accepted.operation_id
  } catch {
    toast.showError(t('renameFormula.error'))
    applying.value = false
  }
}

async function cancelRename() {
  if (!activeOperationId.value || cancelling.value) return
  cancelling.value = true
  try {
    const outcome = await cancelOperation(activeOperationId.value)
    const n = outcome.succeeded.length
    toast.show(t('renameFormula.cancelledToast', { n }, { plural: n }))
    resetApplyState()
    open.value = false
    emit('applied')
  } catch {
    cancelling.value = false
    toast.showError(t('common.unexpectedError'))
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('renameFormula.title')"
    :description="subtitle ?? t('renameFormula.subtitle', { n: assets.length }, { plural: assets.length })"
  >
    <div
      v-if="activeOperationId"
      class="space-y-3"
    >
      <p class="text-[13px] font-semibold">
        {{ t('renameFormula.applyingTitle') }}
      </p>
      <p class="text-[12.5px] text-content-muted">
        {{
          progressTotal !== null
            ? t('renameFormula.progressKnown', { done: progressDone, total: progressTotal })
            : t('renameFormula.progressUnknown', { done: progressDone })
        }}
      </p>
      <div class="h-1.5 w-full overflow-hidden rounded-full bg-border/40">
        <div
          class="h-full rounded-full bg-accent transition-[width]"
          :class="{ 'animate-pulse': progressTotal === null }"
          :style="{
            width: progressTotal ? `${Math.min(100, (progressDone / progressTotal) * 100)}%` : '30%'
          }"
        />
      </div>
      <div class="mt-2 flex justify-end">
        <button
          type="button"
          class="rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold text-content-muted
                 hover:bg-border/20 disabled:opacity-60"
          :disabled="cancelling"
          @click="cancelRename"
        >
          {{ cancelling ? t('renameFormula.cancellingOperation') : t('renameFormula.cancelOperation') }}
        </button>
      </div>
    </div>
    <div
      v-else
      class="space-y-3"
    >
      <label class="block text-[12.5px] font-medium text-content-muted">
        {{ t('renameFormula.schemaLabel') }}
        <input
          ref="schemaInput"
          v-model="schema"
          type="text"
          autocomplete="off"
          class="mt-1 w-full rounded-lg border border-border bg-surface-elevated px-3 py-2 text-[13px]
                 text-content focus:border-accent focus:outline-none"
        >
      </label>

      <label
        v-if="hasSubfolders"
        class="flex cursor-pointer items-center justify-between gap-2"
      >
        <span class="text-[12.5px] text-content-muted">{{ t('renameFormula.includeSubfolders') }}</span>
        <button
          type="button"
          role="switch"
          :aria-checked="includeSubfolders"
          class="relative h-5 w-9 shrink-0 rounded-full transition-colors"
          :class="includeSubfolders ? 'bg-accent' : 'bg-border'"
          @click="includeSubfolders = !includeSubfolders"
        >
          <span
            class="absolute top-0.5 h-4 w-4 rounded-full bg-white transition-[left]"
            :style="{ left: includeSubfolders ? '18px' : '2px' }"
          />
        </button>
      </label>

      <div class="flex flex-wrap gap-1.5">
        <button
          v-for="placeholder in PLACEHOLDERS"
          :key="placeholder"
          type="button"
          class="rounded-full border border-transparent bg-border/30 px-2.5 py-1 text-[12px]
                 text-content-muted hover:border-border-strong hover:text-content"
          @click="insertToken(placeholder)"
        >
          {{ t(`renameFormula.token.${placeholder}`) }}
        </button>
        <button
          type="button"
          class="rounded-full border border-transparent bg-border/30 px-2.5 py-1 text-[12px]
                 text-content-muted hover:border-border-strong hover:text-content"
          @click="insertToken('n:3')"
        >
          {{ t('renameFormula.token.counter') }}
        </button>
      </div>

      <div>
        <p class="mb-1.5 text-[12.5px] font-medium text-content-muted">
          {{ t('renameFormula.previewLabel') }}
          <span class="font-normal text-content-muted/70">{{ t('renameFormula.previewHint') }}</span>
        </p>
        <ul class="max-h-[150px] space-y-1 overflow-y-auto">
          <li
            v-if="visiblePreview.length === 0"
            class="text-[12.5px] text-content-muted"
          >
            {{ t('renameFormula.emptyScope') }}
          </li>
          <li
            v-for="item in visiblePreview"
            :key="item.asset_id"
            class="flex items-center gap-1.5 truncate text-[12.5px]"
          >
            <span class="truncate text-content-muted">{{ item.current_name }}</span>
            <span
              class="shrink-0 text-content-muted"
              aria-hidden="true"
            >→</span>
            <span class="truncate font-semibold text-content">{{ item.new_name }}</span>
          </li>
        </ul>
      </div>

      <p
        v-if="collisionCount > 0"
        class="rounded-lg border border-[rgba(214,80,52,.3)] bg-[rgba(214,80,52,.1)] px-3 py-2 text-[12.5px]"
        style="color: var(--color-danger)"
      >
        {{ t('renameFormula.collisionWarning', { n: collisionCount }) }}
      </p>

      <div class="mt-2 flex justify-end gap-2">
        <button
          type="button"
          class="rounded-lg px-3.5 py-2 text-[13px] font-medium text-content-muted hover:bg-border/40"
          @click="open = false"
        >
          {{ t('renameFormula.cancel') }}
        </button>
        <button
          type="button"
          class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-text disabled:opacity-40"
          :disabled="collisionCount > 0"
          @click="apply"
        >
          {{ t('renameFormula.apply') }}
        </button>
      </div>
    </div>
  </Dialog>
</template>
