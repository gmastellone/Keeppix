<script setup lang="ts">
// Upload subsystem's drop veil ("Dragging"), verified line by line against
// the markup **and** the prototype's real handlers: the mockup document's
// text ("dedicated message" for Culling) describes a detail that in the
// mockup **isn't** in the veil — it's a toast on drop. The veil always
// shows the same message, the rejection happens on `drop`. Verified by
// reading the prototype's code, not assumed from the document's prose
// alone.
//
// The "inside a folder" variant (`Drop to upload into <name>`) is omitted:
// no view currently exposes an observable `currentFolder` (the same gap
// already declared multiple times elsewhere) — building that branch now
// would mean dead code for a condition that's never true.
//
// `dragenter`/`dragover` both call `preventDefault()` in the prototype:
// without the first one, some browsers open the file on drop anyway
// before `dragover` gets a chance to intervene.
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'

import { useToastStore } from '@/stores/toast'
import { useUploadStore } from '@/stores/upload'

const { t } = useI18n()
const route = useRoute()
const upload = useUploadStore()
const toast = useToastStore()

const dragging = ref(false)
let depth = 0

function hasFiles(event: DragEvent): boolean {
  return Array.from(event.dataTransfer?.types ?? []).includes('Files')
}

function onDragEnter(event: DragEvent): void {
  if (!hasFiles(event)) return
  event.preventDefault()
  depth += 1
  dragging.value = true
}

function onDragOver(event: DragEvent): void {
  if (!hasFiles(event)) return
  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy'
}

function onDragLeave(event: DragEvent): void {
  if (!hasFiles(event)) return
  depth = Math.max(0, depth - 1)
  if (depth === 0) dragging.value = false
}

async function onDrop(event: DragEvent): Promise<void> {
  if (!hasFiles(event)) return
  event.preventDefault()
  depth = 0
  dragging.value = false

  // "Culling doesn't accept drops — it's a separate area with its own
  // flow" (prototype comment). Exact toast text from the prototype.
  if (route.path === '/culling') {
    toast.showError(t('upload.dropRejectedInCulling'))
    return
  }

  const files = Array.from(event.dataTransfer?.files ?? [])
  if (files.length > 0) await upload.addFilesFromPicker(files)
}

onMounted(() => {
  window.addEventListener('dragenter', onDragEnter)
  window.addEventListener('dragover', onDragOver)
  window.addEventListener('dragleave', onDragLeave)
  window.addEventListener('drop', onDrop)
})

onBeforeUnmount(() => {
  window.removeEventListener('dragenter', onDragEnter)
  window.removeEventListener('dragover', onDragOver)
  window.removeEventListener('dragleave', onDragLeave)
  window.removeEventListener('drop', onDrop)
})
</script>

<template>
  <div
    v-if="dragging"
    class="pointer-events-none absolute inset-0 z-40 flex items-center justify-center
           rounded-xl border-2 border-dashed border-accent bg-accent-tint"
  >
    <div class="flex max-w-[340px] flex-col items-center gap-1 text-center">
      <p class="text-base font-bold text-content">
        {{ t('upload.drop.title') }}
      </p>
      <p class="text-[12.5px] text-content-muted">
        {{ t('upload.drop.subtitle') }}
      </p>
    </div>
  </div>
</template>
