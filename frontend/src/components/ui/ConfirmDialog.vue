<script setup lang="ts">
// SP-5 ("stessa filosofia" del piano): conferma generica sì/no
// (`openConfirmDialog` nel prototipo, righe 6361-6385 di
// keeppix-mockup.html) — usata per un'azione distruttiva singola con una
// sola alternativa netta (es. eliminare un gruppo). Sopra `Dialog.vue`, non
// una reimplementazione: il fuoco iniziale va su "Annulla" tramite la sua
// prop `initialFocus`, la stessa eccezione deliberata che il piano chiede
// di preservare — chi preme Invio d'istinto non deve innescare l'azione
// distruttiva.
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import Dialog from './Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
defineProps<{ title: string; description: string; confirmLabel: string }>()
const emit = defineEmits<{ confirm: [] }>()
const { t } = useI18n()

const cancelBtn = ref<HTMLButtonElement | null>(null)

function confirm() {
  open.value = false
  emit('confirm')
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="title"
    :description="description"
    :initial-focus="cancelBtn"
  >
    <div class="flex gap-2">
      <button
        type="button"
        class="rounded-lg border border-danger bg-transparent px-3.5 py-2 text-[13px]
               font-semibold text-danger hover:bg-danger/10"
        @click="confirm"
      >
        {{ confirmLabel }}
      </button>
      <button
        ref="cancelBtn"
        type="button"
        class="rounded-lg border border-transparent bg-transparent px-3.5 py-2 text-[13px]
               font-semibold text-content hover:bg-border/40"
        @click="open = false"
      >
        {{ t('ui.dialog.cancel') }}
      </button>
    </div>
  </Dialog>
</template>
