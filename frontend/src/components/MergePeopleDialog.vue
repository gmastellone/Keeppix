<script setup lang="ts">
// "Merge people" dialog.
//
// **Who survives by default**: "the first selected person with a real
// name; if none has a name, the very first one" — here `people` already
// arrives in selection order (guaranteed by the caller, `PeopleView.vue`,
// which appends to an array via `personSelectedIds`), not grid order.
//
// **M = distinct photo count of the union**: computed by the caller (via
// `runSearch({op:'or',...})` over the set of people) and passed as a
// prop — this component does not run its own searches.
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { mergePersons, type Person } from '@/api/persons'
import Dialog from '@/components/ui/Dialog.vue'
import { useToastStore } from '@/stores/toast'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ people: Person[]; totalPhotoCount: number }>()
const emit = defineEmits<{ merged: [] }>()

const { t } = useI18n()
const toast = useToastStore()

function displayName(p: Person): string {
  return p.name?.trim() || t('persons.unnamed')
}

const survivorId = ref('')

watch(
  open,
  (isOpen) => {
    if (isOpen) {
      const named = props.people.find((p) => p.name?.trim())
      survivorId.value = (named ?? props.people[0])?.id ?? ''
    }
  },
  { immediate: true }
)

const merging = ref(false)

async function merge() {
  const absorbed = props.people.filter((p) => p.id !== survivorId.value).map((p) => p.id)
  if (!survivorId.value || absorbed.length === 0) return
  merging.value = true
  try {
    await mergePersons(survivorId.value, absorbed)
    open.value = false
    toast.show(t('mergePeople.mergedToast', { n: props.people.length }, { plural: props.people.length }))
    emit('merged')
  } catch {
    toast.showError(t('mergePeople.error'))
  } finally {
    merging.value = false
  }
}

const title = computed(() => t('mergePeople.title', { n: props.people.length }))
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="title"
    :description="t('mergePeople.subtitle', { n: totalPhotoCount })"
  >
    <div
      role="radiogroup"
      :aria-label="t('mergePeople.radiogroupLabel')"
      class="flex flex-col gap-1"
    >
      <button
        v-for="person in people"
        :key="person.id"
        type="button"
        role="radio"
        :aria-checked="survivorId === person.id"
        class="flex items-center justify-between gap-2 rounded-lg px-2.5 py-2 text-left text-[13px]
               hover:bg-border/20 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        @click="survivorId = person.id"
      >
        <span class="flex items-center gap-2">
          <span
            class="h-[15px] w-[15px] shrink-0 rounded-full border-2"
            :class="survivorId === person.id ? 'border-accent bg-accent' : 'border-border-strong'"
            aria-hidden="true"
          />
          {{ displayName(person) }}
        </span>
        <span class="text-[11px] text-content-muted">
          {{ t('persons.photoCount', { n: person.face_count ?? 0 }, { plural: person.face_count ?? 0 }) }}
        </span>
      </button>
    </div>

    <div class="mt-3 flex items-center gap-2">
      <button
        type="button"
        class="rounded-lg bg-accent px-3.5 py-2 text-[13px] font-semibold text-accent-text disabled:opacity-60"
        :disabled="merging"
        @click="merge"
      >
        {{ t('mergePeople.confirm') }}
      </button>
      <button
        type="button"
        class="rounded-lg border border-transparent px-3.5 py-2 text-[13px] font-semibold hover:bg-border/30"
        @click="open = false"
      >
        {{ t('ui.dialog.cancel') }}
      </button>
    </div>
  </Dialog>
</template>
