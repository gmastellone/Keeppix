<script setup lang="ts">
// Fase 11 Task 15 (1/N), §54 "Dialog 'modifica categoria'" — documento
// funzionale verificato riga per riga (righe 8061-8119). A differenza
// dell'editor tag, non ha un pulsante "Elimina" al proprio interno: solo
// il nome, coerente col modello dati reale (`kind:'category'` — `{id,
// name}`, nient'altro). L'eliminazione resta solo dal cestino sulla
// testata del blocco categoria in `TagsView.vue`.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { createTag, patchTag, type Tag } from '@/api/tags'
import Dialog from '@/components/ui/Dialog.vue'
import TextField from '@/components/ui/TextField.vue'
import { useToastStore } from '@/stores/toast'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ category: Tag | null }>()
const emit = defineEmits<{ saved: [] }>()

const { t } = useI18n()
const toast = useToastStore()

const name = ref('')
const nameError = ref('')
const saving = ref(false)

// `immediate: true`: stesso bug reale già trovato in `TagEditorDialog.vue`
// e in `ProblemFilesDialog.vue` (Task 13 3/N) — un dialog che nasce già
// aperto va precompilato subito, non solo alla prossima transizione.
watch(
  open,
  (isOpen) => {
    if (isOpen) {
      name.value = props.category?.name ?? ''
      nameError.value = ''
    }
  },
  { immediate: true }
)

async function save() {
  const trimmed = name.value.trim()
  if (!trimmed) {
    nameError.value = t('categoryEditor.nameRequired')
    return
  }
  saving.value = true
  nameError.value = ''
  try {
    if (props.category) {
      await patchTag(props.category.id, { name: trimmed })
      toast.show(t('categoryEditor.renamedToast', { name: trimmed }))
    } else {
      await createTag({ name: trimmed, kind: 'category' })
      toast.show(t('categoryEditor.createdToast', { name: trimmed }))
    }
    open.value = false
    emit('saved')
  } catch {
    nameError.value = t('categoryEditor.nameConflict')
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="category ? t('categoryEditor.renameTitle') : t('categoryEditor.newTitle')"
    :description="t('categoryEditor.subtitle')"
  >
    <div class="flex flex-col gap-3.5">
      <TextField
        v-model="name"
        :label="t('categoryEditor.name')"
      />
      <p
        v-if="nameError"
        class="-mt-2 text-xs text-danger"
      >
        {{ nameError }}
      </p>
      <div class="mt-1 flex items-center gap-2">
        <button
          type="button"
          class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-text disabled:opacity-60"
          :disabled="saving"
          @click="save"
        >
          {{ category ? t('categoryEditor.save') : t('categoryEditor.create') }}
        </button>
        <button
          type="button"
          class="rounded-lg border border-transparent px-3.5 py-2 text-[13px] font-semibold hover:bg-border/30"
          @click="open = false"
        >
          {{ t('ui.dialog.cancel') }}
        </button>
      </div>
    </div>
  </Dialog>
</template>
