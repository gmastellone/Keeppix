<script setup lang="ts">
// Fase 11, sottosistema di caricamento — documento §5 ("La
// destinazione"), verificato riga per riga contro il markup e i colori
// esatti del prototipo (righe 128-154, §7.1 per i token).
//
// "Nuova cartella…" (§5, riga 139) è deliberatamente assente: il
// backend non ha una rotta per creare una cartella (verificato in
// `crates/keeppix-api/src/routes/folders.rs` — solo `tree`/`children`/
// `relocate`), a differenza del "in preparazione" dei video (blocco
// analogo già dichiarato nel diario per questo stesso sottosistema).
// Il menu elenca solo le cartelle reali già esistenti.
//
// Stato "manca la destinazione" (§5, "Lo stato che blocca, reso
// visibile"): tre cose cambiano insieme nel documento (chip arancione
// tenue, riga di rassicurazione, striscia sidebar) — questo componente
// copre solo il chip e la riga; la striscia arriva con la coda
// (sotto-passo successivo).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import Popover from '@/components/ui/Popover.vue'
import { useShellStore } from '@/stores/shell'
import { useUploadStore } from '@/stores/upload'

const { t } = useI18n()
const shell = useShellStore()
const upload = useUploadStore()

onMounted(() => {
  if (!shell.loaded) void shell.load()
})

const open = ref(false)

const missing = computed(() => upload.stickyDestination === null)
const destinationName = computed(() => {
  const id = upload.stickyDestination
  if (!id) return null
  return shell.folders.find((f) => f.id === id)?.name ?? null
})

function choose(folderId: string): void {
  upload.setDestination(folderId)
  open.value = false
}
</script>

<template>
  <div>
    <Popover
      v-model:open="open"
      side="bottom"
      align="start"
    >
      <template #trigger>
        <button
          type="button"
          class="flex w-full items-center justify-between rounded-[9px] border px-2.5 py-2 text-xs"
          :class="
            missing
              ? 'border-accent bg-accent-tint text-accent'
              : 'border-border bg-chip-bg text-content'
          "
        >
          <span class="text-content-muted">{{ t('upload.destination.label') }}</span>
          <span
            class="font-bold"
            :class="missing && 'italic'"
          >{{ missing ? t('upload.destination.missing') : destinationName }}</span>
        </button>
      </template>
      <ul
        role="listbox"
        :aria-label="t('upload.destination.label')"
        class="max-h-60 overflow-y-auto"
      >
        <li
          v-for="folder in shell.folders"
          :key="folder.id"
        >
          <button
            type="button"
            role="option"
            :aria-selected="folder.id === upload.stickyDestination"
            class="block w-full rounded-md px-2.5 py-1.5 text-left text-[13px] hover:bg-border/30"
            :class="folder.id === upload.stickyDestination && 'font-semibold'"
            @click="choose(folder.id)"
          >
            {{ folder.name }}
          </button>
        </li>
      </ul>
    </Popover>
    <p
      v-if="missing"
      class="mt-1 text-[11px] text-content-muted"
    >
      {{ t('upload.destination.missingHint') }}
    </p>
  </div>
</template>
