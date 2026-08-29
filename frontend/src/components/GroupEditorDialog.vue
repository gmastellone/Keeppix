<script setup lang="ts">
// Two text dialogs share the same form ("New group"/"Rename group") — a
// single component, like `CategoryEditorDialog.vue` for tags. **No
// duplicate check here**, unlike tags: the mockup explicitly allows "two
// groups with the same name", but the real backend applies `UNIQUE(name)`
// for real (`PersonGroupRepo`, 409 Conflict) — a 409 therefore becomes a
// real error under the field, not just the "allowed" duplicate name from
// the prototype.
//
// **"If left empty: the dialog closes and nothing happens"** in the
// prototype — here instead an empty name shows the required-field error:
// closing without feedback on a required field is not a behavior worth
// reproducing, same discipline already applied to
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
