<script setup lang="ts">
// Fase 11 Task 16 (2/N), §34 "Dialog 'assegna a gruppo'" — documento
// funzionale verificato riga per riga (righe 5594-5656).
//
// **"Una persona sta in al massimo un gruppo"** (§34, e §31.2): il
// backend reale permette a una persona di stare in più gruppi
// (`person_group_members` molti-a-molti, nessun vincolo di unicità) —
// qui il vincolo del documento è applicato lato client: prima di
// aggiungere l'appartenenza nuova, rimuove quella vecchia se diversa
// (`currentGroupId` passato dal chiamante, che lo conosce già dai dati
// caricati per la griglia). Non è un'invenzione: è la stessa identica
// interpretazione che rende sensato il testo del documento ("Rimosso
// dal gruppo."/"Gruppo assegnato.").
//
// **Nessuna creazione da qui** (§34.2: "non c'è un modo per creare un
// gruppo da qui… bisogna uscire e usare 'Nuovo gruppo'"): comportamento
// riprodotto, nessun collegamento a `GroupEditorDialog.vue` qui dentro.
import { useI18n } from 'vue-i18n'

import { addGroupMember, removeGroupMember, type PersonGroup } from '@/api/persons'
import Dialog from '@/components/ui/Dialog.vue'
import { useToastStore } from '@/stores/toast'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{
  personIds: string[]
  personLabel: string
  currentGroupId: (personId: string) => string | null
  groups: PersonGroup[]
}>()
const emit = defineEmits<{ assigned: [] }>()

const { t } = useI18n()
const toast = useToastStore()

async function apply(groupId: string | null) {
  await Promise.all(
    props.personIds.map(async (personId) => {
      const current = props.currentGroupId(personId)
      if (current && current !== groupId) {
        await removeGroupMember(current, personId)
      }
      if (groupId && current !== groupId) {
        await addGroupMember(groupId, personId)
      }
    })
  )
  open.value = false
  toast.show(groupId ? t('assignGroup.assignedToast') : t('assignGroup.removedToast'))
  emit('assigned')
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('assignGroup.title')"
    :description="personLabel"
  >
    <ul class="max-h-[260px] space-y-1 overflow-y-auto">
      <li>
        <button
          type="button"
          class="w-full rounded-lg px-2.5 py-2 text-left text-[13px] hover:bg-border/20
                 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
          @click="apply(null)"
        >
          {{ t('assignGroup.noGroup') }}
        </button>
      </li>
      <li
        v-for="group in groups"
        :key="group.id"
      >
        <button
          type="button"
          class="w-full rounded-lg px-2.5 py-2 text-left text-[13px] hover:bg-border/20
                 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
          @click="apply(group.id)"
        >
          {{ group.name }}
        </button>
      </li>
    </ul>
    <button
      type="button"
      class="mt-3 rounded-lg border border-transparent px-3.5 py-2 text-[13px] font-semibold hover:bg-border/30"
      @click="open = false"
    >
      {{ t('ui.dialog.cancel') }}
    </button>
  </Dialog>
</template>
