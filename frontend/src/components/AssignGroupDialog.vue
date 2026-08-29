<script setup lang="ts">
// "Assign to group" dialog.
//
// **"A person belongs to at most one group"**: the real backend allows a
// person to belong to multiple groups (`person_group_members` is
// many-to-many, no uniqueness constraint) — here the mockup's constraint
// is enforced client-side: before adding the new membership, it removes
// the old one if different (`currentGroupId` passed by the caller, which
// already knows it from the data loaded for the grid). Not an invention:
// it's the exact same interpretation that makes the mockup's copy
// ("Removed from group."/"Group assigned.") make sense.
//
// **No creation from here**: "there's no way to create a group from
// here… you have to leave and use 'New group'" — behavior reproduced, no
// link to `GroupEditorDialog.vue` in here.
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
