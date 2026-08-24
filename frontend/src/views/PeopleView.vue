<script setup lang="ts">
// Fase 11 Task 16 (1/N) — documento funzionale §31 "Persone — la
// griglia" (righe 5192-5417), verificato riga per riga.
//
// **Scheda ridotta rispetto al documento**: niente blocchi di gruppo
// (§31.2, "un blocco per ogni gruppo… e in coda sempre 'Senza gruppo'"),
// niente pulsante "Nuovo gruppo", niente selezione multipla con
// Unisci/Assegna a gruppo (§31.3, controlli 7-10), niente banner della
// coda di revisione. Nessuna di queste è raggiungibile senza i dialog
// dedicati (§34 "Assegna a gruppo", §35 "Unisci") o senza una linguetta
// "Volti" reale in `ReviewView.vue` — entrambi ancora da costruire
// (prossime sotto-unità). Costruire qui i loro trigger sarebbe un
// vicolo cieco, stessa disciplina di "Duplicati" (Task 13 2/N) e del
// gruppo "IA" a comparsa incrementale (Task 15). Per ora: un unico
// elenco piatto di tutte le persone visibili, nell'ordine restituito da
// `GET /persons`.
//
// **`visiblePeople()`** (§31.2): persone non nascoste (`fetchPersons()`
// senza `include_hidden`, già il comportamento della rotta) *e* con
// almeno un volto confermato (`face_count > 0` — filtro lato client,
// la rotta non lo applica, commento originale di `persons.ts`).
//
// **Foto di copertina reale, ma non quella scelta**: `PersonView` porta
// `cover_face_id` (l'id di un volto), non l'id di un asset — nessuna
// rotta risolve un id di volto al suo asset/riquadro (`GET /faces/{id}`
// non esiste, solo `GET /assets/{id}/faces`). Costruirla sarebbe una
// rotta nuova per comodità di interfaccia, fuori scope. La copertina
// mostrata qui è quindi sempre **una foto reale e recente della
// persona** (`runSearch({op:'person',id}, undefined, 1)`, un giro per
// scheda — stesso costo accettato di N richieste per N elementi già di
// `ReviewView.vue`), non necessariamente quella impostata con "Scegli
// copertina" (§33, prossima sotto-unità: quel dialog resta comunque
// pienamente reale — il difetto è solo nel non poter *mostrare* la
// scelta qui senza quella rotta mancante).
//
// **Nessun `autoNum` per le persone senza nome**: `_personAutoSeq` è un
// contatore in memoria del mockup, senza alcuna colonna corrispondente
// sul backend reale (`Person.name: Option<String>`, nient'altro). Al
// posto di "Persona 12" inventato, l'etichetta è `persons.unnamed`
// ("Persona senza nome") — onesto, non un numero fabbricato.
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { runSearch } from '@/api/library'
import { fetchPersons, type Person } from '@/api/persons'
import { thumbSrc } from '@/api/media'
import { thumbhashToDataURL } from '@/timeline/thumbhash'
import { useToastStore } from '@/stores/toast'

const { t } = useI18n()
const router = useRouter()
const toast = useToastStore()

const loading = ref(true)
const people = ref<Person[]>([])
const hiddenCount = ref(0)
const covers = ref<Record<string, { hash: string | null; thumbhash: string | null } | null>>({})

async function loadCover(person: Person) {
  try {
    const page = await runSearch({ op: 'person', id: person.id }, undefined, 1)
    const asset = page.assets[0]
    covers.value[person.id] = asset ? { hash: asset.content_hash, thumbhash: asset.thumbhash } : null
  } catch {
    covers.value[person.id] = null
  }
}

async function load() {
  loading.value = true
  try {
    const [visible, all] = await Promise.all([fetchPersons(), fetchPersons(true)])
    people.value = visible.filter((p) => (p.face_count ?? 0) > 0)
    hiddenCount.value = all.filter((p) => p.hidden).length
    await Promise.all(people.value.map(loadCover))
  } catch {
    toast.showError(t('persons.loadError'))
  } finally {
    loading.value = false
  }
}

onMounted(load)

function displayName(person: Person): string {
  return person.name?.trim() || t('persons.unnamed')
}

function coverStyle(person: Person) {
  const cover = covers.value[person.id]
  if (cover?.hash) return { backgroundImage: `url(${thumbSrc(cover.hash)})` }
  if (cover?.thumbhash) {
    const url = thumbhashToDataURL(cover.thumbhash)
    if (url) return { backgroundImage: `url(${url})` }
  }
  return {}
}

function open(person: Person) {
  void router.push(`/persons/${person.id}`)
}
</script>

<template>
  <main class="mx-auto max-w-[860px] p-6">
    <p class="text-[15px] font-bold">
      {{ t('persons.title') }}
    </p>
    <p class="mt-1 text-[12.5px] text-content-muted">
      {{ t('persons.subtitle') }}
    </p>

    <p
      v-if="!loading && people.length === 0"
      class="mt-6 text-[13px] text-content-muted"
    >
      {{ t('persons.emptyText') }}
    </p>

    <div
      v-else
      class="mt-5 grid grid-cols-[repeat(auto-fill,minmax(110px,1fr))] gap-4"
    >
      <button
        v-for="person in people"
        :key="person.id"
        type="button"
        class="flex flex-col items-center gap-2 rounded-lg p-2 text-center hover:bg-border/20
               focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
        @click="open(person)"
      >
        <span
          class="h-[78px] w-[78px] rounded-full border border-border bg-cover bg-center bg-surface-elevated"
          :style="coverStyle(person)"
          aria-hidden="true"
        />
        <span class="w-full truncate text-[12.5px] font-semibold">
          {{ displayName(person) }}
          <span
            v-if="!person.name"
            class="font-semibold text-accent"
          > · {{ t('persons.unnamedHint') }}</span>
        </span>
        <span class="text-[11px] text-content-muted">
          {{ t('persons.photoCount', { n: person.face_count ?? 0 }, { plural: person.face_count ?? 0 }) }}
        </span>
      </button>
    </div>

    <p
      v-if="hiddenCount > 0"
      class="mt-6 text-[12px] text-content-muted"
    >
      {{ t('persons.hiddenFooter', { n: hiddenCount }, { plural: hiddenCount }) }}
    </p>
  </main>
</template>
