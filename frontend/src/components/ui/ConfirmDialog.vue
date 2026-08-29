<script setup lang="ts">
// Generic yes/no confirmation (`openConfirmDialog` in the prototype,
// lines 6361-6385 of keeppix-mockup.html) — used for a single
// destructive action with one clear-cut alternative (e.g. deleting a
// group). Built on top of `Dialog.vue`, not a reimplementation: initial
// focus goes to "Cancel" via its `initialFocus` prop, the same
// deliberate exception this project wants preserved — someone hitting
// Enter on instinct should not trigger the destructive action.
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
