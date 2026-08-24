<script setup lang="ts">
// Fase 11 Task 8 (5/N) — documento funzionale §19.3, "selettore di
// persona": riusato da "Correggi persona…" (il solo consumatore reale
// oggi — vedi il commento di testa di `AssetViewer.vue` per il perché di
// "+ aggiungi" resta debito dichiarato, non qui). `GET /persons` non ha
// un parametro di ricerca (`ListPersonsQuery` porta solo `include_hidden`,
// crates/keeppix-api/src/routes/persons.rs) — filtro lato client sull'
// intero elenco, stesso principio già di `TagPickerDialog`/
// `AlbumPickerDialog` (elenchi tipicamente piccoli, non meritano una
// rotta di ricerca dedicata).
import { computed, ref, watch } from 'vue'
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

async function load() {
  persons.value = await fetchPersons().catch(() => [])
}

watch(
  open,
  (isOpen) => {
    if (isOpen) {
      query.value = ''
      void load()
    }
  },
  { immediate: true }
)

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) return persons.value
  return persons.value.filter((person) => (person.name ?? '').toLowerCase().includes(q))
})

/** Nessun nome duplicato esatto già in elenco: evita di offrire "crea"
 * quando l'utente ha semplicemente digitato un nome che esiste già. */
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
    <input
      v-model="query"
      type="search"
      :placeholder="t('personPicker.placeholder')"
      :aria-label="t('personPicker.placeholder')"
      class="w-full rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
    >
    <ul class="mt-3 max-h-[240px] space-y-1 overflow-y-auto">
      <li
        v-for="person in filtered"
        :key="person.id"
      >
        <button
          type="button"
          class="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left text-[13px]
                 hover:bg-border/20 focus-visible:outline-2 focus-visible:outline-offset-2
                 focus-visible:outline-accent"
          @click="pick(person)"
        >
          {{ person.name ?? t('personPicker.unnamed') }}
        </button>
      </li>
    </ul>
    <button
      v-if="canCreate"
      type="button"
      class="mt-2 w-full rounded-lg border border-border px-3 py-2 text-left text-[13px]
             hover:bg-border/20"
      :disabled="creating"
      @click="createAndPick"
    >
      {{ t('personPicker.create', { name: query.trim() }) }}
    </button>
  </Dialog>
</template>
