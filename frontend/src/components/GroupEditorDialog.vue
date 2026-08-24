<script setup lang="ts">
// Fase 11 Task 16 (2/N), §31.3 controlli 1 e 5: due dialog di testo del
// documento condividono lo stesso form ("Nuovo gruppo"/"Rinomina
// gruppo") — un solo componente, come `CategoryEditorDialog.vue` per i
// tag. **Nessun controllo di duplicati qui**, a differenza dei tag: il
// documento lo dice esplicitamente ("due gruppi con lo stesso nome sono
// ammessi"), ma il backend reale applica `UNIQUE(name)` per davvero
// (`PersonGroupRepo`, 409 Conflict) — un 409 diventa quindi un vero
// errore sotto il campo, non la sola omonimia "ammessa" del mockup.
//
// **"Se lasciato vuoto: il dialog si chiude e non succede nulla"** (§31.3
// controllo 1) — qui invece un nome vuoto mostra l'errore del campo
// obbligatorio: chiudere senza feedback su un campo obbligatorio non è
// un comportamento da riprodurre, stessa disciplina già applicata a
// `TagEditorDialog.vue`/`CategoryEditorDialog.vue`.
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { createPersonGroup, renamePersonGroup, type PersonGroup } from '@/api/persons'
import Dialog from '@/components/ui/Dialog.vue'
import TextField from '@/components/ui/TextField.vue'
import { useToastStore } from '@/stores/toast'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ group: PersonGroup | null }>()
const emit = defineEmits<{ saved: [] }>()

const { t } = useI18n()
const toast = useToastStore()

const name = ref('')
const nameError = ref('')
const saving = ref(false)

watch(
  open,
  (isOpen) => {
    if (isOpen) {
      name.value = props.group?.name ?? ''
      nameError.value = ''
    }
  },
  { immediate: true }
)

async function save() {
  const trimmed = name.value.trim()
  if (!trimmed) {
    nameError.value = t('groupEditor.nameRequired')
    return
  }
  saving.value = true
  nameError.value = ''
  try {
    if (props.group) {
      await renamePersonGroup(props.group.id, trimmed)
      toast.show(t('groupEditor.renamedToast'))
    } else {
      await createPersonGroup(trimmed)
      toast.show(t('groupEditor.createdToast', { name: trimmed }))
    }
    open.value = false
    emit('saved')
  } catch {
    nameError.value = t('groupEditor.nameConflict')
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="group ? t('groupEditor.renameTitle') : t('groupEditor.newTitle')"
    :description="group ? undefined : t('groupEditor.subtitle')"
  >
    <div class="flex flex-col gap-3.5">
      <TextField
        v-model="name"
        :label="t('groupEditor.name')"
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
          {{ group ? t('groupEditor.save') : t('groupEditor.create') }}
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
