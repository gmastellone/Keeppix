<script setup lang="ts">
// The three-way deletion choice (`openDeleteDialogGeneric` in the
// prototype, lines 4135-4164 of keeppix-mockup.html) — remove from
// index, move to trash, delete from disk. Initial focus goes to the
// **first** option, the least destructive one (remove from index):
// someone hitting Enter on instinct does the harmless thing, not the
// permanent deletion — the other of the two deliberate exceptions we
// want to preserve, alongside `ConfirmDialog`.
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
