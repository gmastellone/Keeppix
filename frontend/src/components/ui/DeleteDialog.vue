<script setup lang="ts">
// SP-18: la scelta a tre vie per l'eliminazione (`openDeleteDialogGeneric`
// nel prototipo, righe 4135-4164 di keeppix-mockup.html) — rimuovi
// dall'indice, sposta nel cestino, elimina dal disco. Il fuoco iniziale va
// sulla **prima** opzione, la meno distruttiva (rimuovi dall'indice): chi
// preme Invio d'istinto fa la cosa innocua, non l'eliminazione definitiva
// — l'altra delle due eccezioni deliberate che il piano chiede di
// preservare, insieme a `ConfirmDialog`.
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import Dialog from './Dialog.vue'

export type DeleteChoice = 'index' | 'trash' | 'disk'

const open = defineModel<boolean>('open', { required: true })
defineProps<{ title: string }>()
const emit = defineEmits<{ choose: [choice: DeleteChoice] }>()
const { t } = useI18n()

const firstOption = ref<HTMLButtonElement | null>(null)

function choose(choice: DeleteChoice) {
  open.value = false
  emit('choose', choice)
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="title"
    :description="t('ui.deleteDialog.description')"
    :initial-focus="firstOption"
  >
    <div class="flex flex-col gap-2">
      <button
        ref="firstOption"
        type="button"
        class="rounded-lg border border-border px-3 py-2.5 text-left hover:bg-border/20"
        @click="choose('index')"
      >
        <span class="block text-[13px] font-semibold text-content">
          {{ t('ui.deleteDialog.index.label') }}
        </span>
        <span class="block text-xs text-content-muted">{{ t('ui.deleteDialog.index.hint') }}</span>
      </button>
      <button
        type="button"
        class="rounded-lg border border-border px-3 py-2.5 text-left hover:bg-border/20"
        @click="choose('trash')"
      >
        <span class="block text-[13px] font-semibold text-content">
          {{ t('ui.deleteDialog.trash.label') }}
        </span>
        <span class="block text-xs text-content-muted">{{ t('ui.deleteDialog.trash.hint') }}</span>
      </button>
      <button
        type="button"
        class="rounded-lg border border-danger px-3 py-2.5 text-left hover:bg-danger/10"
        @click="choose('disk')"
      >
        <span class="block text-[13px] font-semibold text-danger">
          {{ t('ui.deleteDialog.disk.label') }}
        </span>
        <span class="block text-xs text-content-muted">{{ t('ui.deleteDialog.disk.hint') }}</span>
      </button>
      <button
        type="button"
        class="mt-1 self-start rounded-lg px-3.5 py-2 text-[13px] font-semibold text-content
               hover:bg-border/40"
        @click="open = false"
      >
        {{ t('ui.dialog.cancel') }}
      </button>
    </div>
  </Dialog>
</template>
