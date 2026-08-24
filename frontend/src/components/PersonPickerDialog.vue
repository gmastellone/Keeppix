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
//
// Task 16 (3/N): riverificato riga per riga contro §37 (righe 5874-5955)
// con occhi freschi, non solo riportato dal Task 8 — quattro lacune
// reali trovate e chiuse: titolo/placeholder non combaciavano col
// documento ("Assegna persona"/"Cerca o crea una persona…", non "Scegli
// una persona"/"Cerca o digita un nome…"); mancava il conteggio "N foto"
// per riga (§37.2); mancava il fuoco automatico sul campo di ricerca
// all'apertura (§37.5, "l'unico dialog di questo blocco a farlo");
// mancava lo stato vuoto "Nessuna persona trovata." (§37.7); mancava il
// pulsante "Annulla" (§37.2 punto 4).
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
      // §37.5: "il campo di ricerca riceve il focus all'apertura" — il
      // solo dialog di questo blocco a farlo, `nextTick` perché reka-ui
      // porta già il proprio focus iniziale sul contenuto del dialog.
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
