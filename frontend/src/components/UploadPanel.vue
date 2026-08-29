<script setup lang="ts">
// Upload subsystem's queue panel (`docs/ui/caricamento-nuove-foto.md`,
// "The queue"), verified line by line **and** against the prototype's
// `renderUploadPanel`/`uploadRowHTML`/`uploadCounts` for details the
// document doesn't fully specify (the exact title priority, each row's
// text, the truncated file list).
//
// Entirely replaces the generic floating panel in the bottom-right corner:
// that pattern is exactly the floating button the mockup document says was
// dropped, not just a different style.
//
// Single-word filename (matches the plan), but the component still has a
// two-word name: avoids the `multi-word-component-names` lint warning
// without having to disable it for a component outside `ui/`.
//
// "Layered Esc": the first press closes the destination menu, the second
// closes the panel. The menu (`Popover`, inside `DestinationChip`) already
// closes itself on Esc and stops propagation — library behavior
// (`reka-ui`, `DismissableLayer`), not hand-orchestrated (same principle
// already documented in `Popover.vue`). A single `@keydown.escape` on the
// panel's root is enough for the second level: if the menu is open, the
// event never reaches this far.
//
// Declared gap: the panel doesn't move focus onto itself when it opens (no
// element is "active" until the user first presses Tab) — `@keydown.escape`
// only works after that, because a keypress needs to reach an element that
// already has focus. More complete focus management (moving it here on
// open, returning it to the strip on close) is out of scope for this step.
defineOptions({ name: 'UploadPanel' })

import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import DestinationChip from '@/components/DestinationChip.vue'
import { useUploadStore, type UploadSessionState } from '@/stores/upload'

const { t } = useI18n()
const router = useRouter()
const upload = useUploadStore()

// Declared deviation from the prototype, not a mirroring of it: the
// mockup's logic (`if(!u.open || !u.items.length){...return}`) hides the
// panel even for a batch that was entirely rejected — a drop of only RAW
// files would give **no** visible feedback at all, contradicting the same
// document's own principle ("rejecting RAW files isn't an error, it's an
// explanation" — and an explanation needs to be seen). Fixed here: the
// panel stays reachable even with empty `sessions`, as long as there's
// something to explain.
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

/** Same effect as the prototype's global `u.paused`: everything still
 * pending is paused together (`pauseAll`/`resumeAll`). */
const allPaused = computed(() => {
  const pending = upload.sessions.filter((s) => s.status === 'queued' || s.status === 'uploading' || s.status === 'paused')
  return pending.length > 0 && pending.every((s) => s.status === 'paused')
})

/** Exact title priority order, matching the mockup. */
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
 * `useLightboxRoute` (TimelineView) resolves `?photo=<id>` even for an
 * asset not yet loaded on the page — it loads it remotely if not found
 * locally (the composable's `*loadRemote*` path). Not a workaround: the
 * same mechanism meant for "send a colleague the link to this photo"
 * genuinely opens the copy that already exists.
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

/** A "done" with `collision==='skipped_duplicate'` (the server, not the
 * pre-check, found the duplicate at the end of the upload) presents itself
 * as "skipped" for all practical purposes — same treatment as the
 * previous component. */
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

/** The prototype's example text ("the server didn't respond") is just the
 * demo simulation — here the row shows the real reason the store already
 * carries (`session.error`, always an i18n key). */
function subLine(session: UploadSessionState): string | null {
  const state = displayState(session)
  if (state === 'skipped') return t('upload.collision.skipped_duplicate')
  if (state === 'error') return t(session.error ?? 'upload.errors.unknown')
  if (session.status === 'done' && session.collision === 'renamed') {
    return t('upload.collision.renamed', { filename: session.savedFilename ?? session.filename })
  }
  return null
}

/** Up to four names, then "and one more"/"and N more" — verified against
 * the prototype's `uploadAndOthers()`. */
function fileListText(names: string[]): string {
  const shown = names.slice(0, 4).join(', ')
  if (names.length <= 4) return shown
  // Explicit space in the code, not in the i18n key (fragile to lose in a
  // translation): the prototype keeps it inside `uploadAndOthers()` itself
  // (`' and one more'`), here it's separated to keep the key free of
  // implicit spaces.
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
      tabindex="-1"
      class="fixed z-50 flex flex-col overflow-hidden border border-border-strong bg-card-bg
             shadow-[0_18px_44px_rgba(0,0,0,0.28)]
             max-md:inset-x-0 max-md:bottom-0 max-md:max-h-[72vh] max-md:rounded-t-2xl
             md:bottom-3 md:left-3 md:w-[344px] md:max-h-[460px] md:rounded-xl"
      @keydown.escape="close"
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
