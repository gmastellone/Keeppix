<script setup lang="ts">
// Upload subsystem's queue strip ("Where it lives, and why there"),
// verified line by line **and** against the prototype's
// `renderUploadDock`/`uploadCounts` for details the mockup document
// doesn't specify (the exact label, the priority order, what counts as
// "finished").
//
// At rest it doesn't exist ("with an empty queue the strip doesn't exist,
// the pixel cost is zero") — only `sessions`, never the rejects: verified
// against the prototype's `renderUploadDock()`
// (`if(!u.items.length){ host.innerHTML=''; return; }`), which never
// looks at `state.upload.rejected`. A batch of only rejects doesn't make
// the strip appear — but the reject block stays visible regardless,
// because adding files always opens the panel by itself
// (`stores/upload.ts::addFilesFromPicker`, `panelOpen.value = true`), so
// there's no need to go through the strip to see it.
//
// A single component for the mockup document's two anchors (desktop
// sidebar footer, mobile tab-bar strip — "only one of the two exists at a
// time"): the difference is where the caller mounts it, not its markup —
// same principle already applied to `useUploadPicker.ts`.
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'

import { useUploadStore } from '@/stores/upload'

const { t } = useI18n()
const upload = useUploadStore()

const visible = computed(() => upload.sessions.length > 0)

const counts = computed(() => {
  const finished = upload.sessions.filter(
    (s) => s.status === 'done' || s.status === 'skipped' || s.status === 'error'
  ).length
  const pending = upload.sessions.filter(
    (s) => s.status === 'queued' || s.status === 'uploading' || s.status === 'paused'
  ).length
  return { total: upload.sessions.length, finished, pending }
})

/** Same priority order as the prototype: missing destination first, then
 * paused, then in progress, then finished. "Paused" here means "everything
 * still pending is paused" — the same effect as the prototype's global
 * `u.paused`, which stops all pending sessions together (`pauseAll`). */
const labelKey = computed(() => {
  if (upload.needsDestination) return 'upload.dock.needsDestination'
  const pendingSessions = upload.sessions.filter(
    (s) => s.status === 'queued' || s.status === 'uploading' || s.status === 'paused'
  )
  if (pendingSessions.length > 0 && pendingSessions.every((s) => s.status === 'paused')) {
    return 'upload.dock.paused'
  }
  if (counts.value.pending > 0) return 'upload.dock.uploading'
  return 'upload.dock.done'
})

const progressPercent = computed(() => {
  if (counts.value.total === 0) return 0
  return Math.round((counts.value.finished / counts.value.total) * 100)
})
</script>

<template>
  <button
    v-if="visible"
    type="button"
    class="block w-full rounded-[10px] bg-chip-bg px-3 py-2.5 text-left hover:bg-border/40
           focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
    :class="upload.needsDestination && 'shadow-[inset_0_0_0_1.5px_var(--color-accent)]'"
    :aria-expanded="upload.panelOpen"
    :aria-label="
      t('upload.dock.ariaLabel', { label: t(labelKey), finished: counts.finished, total: counts.total })
    "
    @click="upload.togglePanel()"
  >
    <div class="mb-1.5 flex items-center gap-1.5">
      <span class="min-w-0 flex-1 truncate text-[11.5px] font-semibold">{{ t(labelKey) }}</span>
      <span class="text-[11px] text-content-muted">{{ counts.finished }}/{{ counts.total }}</span>
    </div>
    <div class="h-[5px] overflow-hidden rounded-[3px] bg-border-strong">
      <div
        class="h-full rounded-[3px] bg-accent transition-[width] duration-[250ms] ease-in-out"
        :style="{ width: `${progressPercent}%` }"
      />
    </div>
  </button>
</template>
