<script setup lang="ts">
// "Rename with formula" dialog. Four of the five entry points are covered:
// "selection" (bulk edit, and the culling lot selection bar), "single"
// (the lightbox's "Rename…" button — distinct subtitle, "1 photo —
// {filename}", not "1 photo selected": that's why `subtitle` is
// overridable by the caller instead of being derived from the count
// alone — the text is tied to the **entry point**, not the photo count)
// and "whole lot" ("Rename lot…" — the only scope with `hasSubfolders`:
// `restrictedAssets` is the `cullState==='pending'` subset, the starting
// scope with the toggle off; `assets` stays the whole lot, used when the
// toggle is on). The fifth entry point (a folder open in Timeline) remains
// a declared gap.
//
// No token/slug/sanitization logic is duplicated here: the engine is
// server-side (`keeppix-domain::rename::render_base`), verified by reading
// the real code before writing this dialog — the preview calls
// `previewRename` on every schema change, nothing is recomputed locally.
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

// Off by default, touches only `restrictedAssets` (the "to review"
// photos); on, touches the whole `assets` (picks and rejects included).
// Never remembered between openings, same rule as the schema.
const includeSubfolders = ref(false)
const scopedAssets = computed(() =>
  props.hasSubfolders && !includeSubfolders.value ? (props.restrictedAssets ?? []) : props.assets
)

// Initial value is always the same on every opening: it is not remembered
// between openings.
const DEFAULT_SCHEMA = '{data}_{luogo}_{n:3}'
const PLACEHOLDERS = ['data', 'fotocamera', 'obiettivo', 'luogo', 'titolo'] as const

// `applied`: the lot selection bar wants to clear the selection on a
// successful rename — a dedicated event instead of making the caller
// guess whether the apply succeeded from just `v-model:open` going back
// to `false` (it also closes on "Cancel", which must not clear anything).
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

// `POST /assets/batch/rename` no longer renames inside the request itself
// — it queues a job (`keeppix-jobs::rename_batch`, see `keeppix_db::rename`
// for why) and immediately returns `operation_id`. This dialog follows its
// real progress over the WebSocket (`operation.progress`), same pattern as
// `ProblemsView.vue` for the library rescan — a real cancel, not just an
// indeterminate spinner. Declared **before** the `watch(open, ..., {
// immediate: true })` below: that watcher calls `resetApplyState` starting
// from its very first run, and in `<script setup>` a `const` isn't hoisted
// like a function — a different order throws a `ReferenceError` when the
// component opens (seen for real).
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
  // `cancelRename` is already waiting on its own HTTP response and will
  // show its own toast when it arrives: if this "cancelled" event is a
  // race with that request, let it handle it — don't duplicate here.
  if (cancelling.value) return
  if (payload.phase === 'done') {
    const n = payload.done
    toast.show(t('renameFormula.renamedToast', { n }, { plural: n }))
  } else if (payload.phase === 'failed') {
    toast.showError(t('renameFormula.failedToast'))
  }
  // "cancelled" arrives here without a toast precisely when cancelled from
  // another tab/session (not from this dialog, covered by the guard above).
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
    // Defensive: if the dialog is closed manually (scrim/Esc/`X`, reka-ui)
    // while a rename is in progress, the background job keeps going on its
    // own — only this dialog's *tracking* of it is lost (no "resume where
    // you left off" here, a minor gap left unaddressed). A subsequent
    // reopening must not start from a half-finished state, though.
    resetApplyState()
    schema.value = DEFAULT_SCHEMA
    includeSubfolders.value = false
    void runPreview()
    void nextTick(() => {
      // Focus the schema field with the cursor at the end, not with the
      // text selected.
      const len = schema.value.length
      schemaInput.value?.focus()
      schemaInput.value?.setSelectionRange(len, len)
    })
  },
  { immediate: true }
)

/** Inserts a placeholder at the cursor position, replacing any selected
 * text — then repositions the cursor right after it and returns focus to
 * the field. */
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
  // Implicit recalculation (the preview is already current, debounced),
  // then bails out silently on collisions or an empty scope — same
  // silent guard as the prototype, not a visible error.
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
