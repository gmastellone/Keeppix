<script setup lang="ts">
// "Person picker" dialog — reused by "Correct person…" (the only real
// consumer today — see the header comment of `AssetViewer.vue` for why
// "+ add" remains a declared gap, not here). `GET /persons` has no search
// parameter (`ListPersonsQuery` only carries `include_hidden`,
// crates/keeppix-api/src/routes/persons.rs) — hence client-side filtering
// over the whole list, same principle already used by `TagPickerDialog`/
// `AlbumPickerDialog` (typically small lists, not worth a dedicated search
// route).
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'

import { createPerson, fetchPersons, type Person } from '@/api/persons'
import { useToastStore } from '@/stores/toast'

import Dialog from './ui/Dialog.vue'

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ picked: [personId: string] }>()

const { t } = useI18n()
const toast = useToastStore()
const query = ref('')
const persons = ref<Person[]>([])
const creating = ref(false)
const inputEl = ref<HTMLInputElement | null>(null)

async function load() {
  persons.value = await fetchPersons().catch(() => [])
}

watch(
  open,
  (isOpen) => {
    if (isOpen) {
      query.value = ''
      void load()
      // The search field receives focus on open — the only dialog in this
      // group that does so; `nextTick` because reka-ui already applies its
      // own initial focus to the dialog content.
      void nextTick(() => inputEl.value?.focus())
    }
  },
  { immediate: true }
)

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) return persons.value
  return persons.value.filter((person) => (person.name ?? '').toLowerCase().includes(q))
})

/** No exact duplicate name already in the list: avoids offering "create"
 * when the user has simply typed a name that already exists. */
const canCreate = computed(() => {
  const name = query.value.trim()
  if (!name) return false
  return !persons.value.some((person) => (person.name ?? '').toLowerCase() === name.toLowerCase())
})

function pick(person: Person) {
  emit('picked', person.id)
  open.value = false
}

async function createAndPick() {
  const name = query.value.trim()
  if (!name || creating.value) return
  creating.value = true
  try {
    const person = await createPerson(name)
    emit('picked', person.id)
    open.value = false
  } catch {
    toast.showError(t('personPicker.error'))
  } finally {
    creating.value = false
  }
}
</script>

<template>
  <Dialog
    v-model:open="open"
    :title="t('personPicker.title')"
  >
    <label class="sr-only" for="person-picker-query">{{ t('personPicker.searchLabel') }}</label>
    <input
      id="person-picker-query"
      ref="inputEl"
      v-model="query"
      type="search"
      autocomplete="off"
      :placeholder="t('personPicker.placeholder')"
      class="w-full rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
    >
    <ul class="mt-3 max-h-[260px] space-y-1 overflow-y-auto">
      <li v-if="canCreate">
        <button
          type="button"
          class="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left text-[13px]
                 hover:bg-border/20 focus-visible:outline-2 focus-visible:outline-offset-2
                 focus-visible:outline-accent"
          :disabled="creating"
          @click="createAndPick"
        >
          {{ t('personPicker.create', { name: query.trim() }) }}
        </button>
      </li>
      <li
        v-for="person in filtered"
        :key="person.id"
      >
        <button
          type="button"
          class="flex w-full items-center justify-between gap-3 rounded-lg px-2 py-2 text-left text-[13px]
                 hover:bg-border/20 focus-visible:outline-2 focus-visible:outline-offset-2
                 focus-visible:outline-accent"
          @click="pick(person)"
        >
          <span>{{ person.name || t('personPicker.unnamed') }}</span>
          <span class="text-[11px] text-content-muted">
            {{ t('persons.photoCount', { n: person.face_count ?? 0 }, { plural: person.face_count ?? 0 }) }}
          </span>
        </button>
      </li>
      <li
        v-if="!canCreate && filtered.length === 0"
        class="px-2 py-2 text-[12.5px] text-content-muted"
      >
        {{ t('personPicker.empty') }}
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
