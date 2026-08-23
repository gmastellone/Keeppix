<script setup lang="ts">
// Fase 11, sottosistema di caricamento — documento §3.1 ("Trascinare"),
// verificato riga per riga contro il markup **e** gli handler reali del
// prototipo (righe 3085-3103, 7592-7627): il testo del documento
// ("messaggio dedicato" per il Culling) descrive un dettaglio che nel
// mockup **non** è nel velo — è un toast al rilascio. Il velo mostra
// sempre lo stesso messaggio, il rifiuto avviene al `drop`. Verificato
// leggendo il codice del prototipo, non assunto dalla sola prosa del
// documento.
//
// Variante "dentro una cartella" (`Rilascia per caricare in <nome>`,
// riga 3097) omessa: nessuna vista porta oggi un `currentFolder`
// osservabile (stesso debito già dichiarato più volte in questa
// sessione) — costruire quel ramo ora significherebbe codice morto per
// una condizione mai vera.
//
// `dragenter`/`dragover` chiamano entrambi `preventDefault()` nel
// prototipo (riga 7600, 7606): senza il primo alcuni browser aprono
// comunque il file al rilascio prima che `dragover` abbia la
// possibilità di intervenire.
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

  // "Il Culling non accetta rilasci — è un'area separata con un suo
  // flusso" (commento del prototipo, riga 7621). Testo esatto del
  // toast, riga 7623.
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
