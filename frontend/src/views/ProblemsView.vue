<script setup lang="ts">
// Fase 11 Task 13 (3/N) — documento funzionale §47 "Problemi", verificato
// riga per riga (righe 7157-7311). Riscrittura completa: la vista
// precedente ignorava `problems.problems` (l'elenco già composto in
// linguaggio naturale dal backend, `crates/keeppix-api/src/routes/
// problems.rs::ProblemView` — severità/titolo/descrizione/azioni pronti),
// mostrando invece tre elenchi grezzi di nomi file/librerie senza alcuna
// azione collegata. Il commento del mockup a `attachProblemHandlers`
// dice esplicitamente l'intento: "prima erano pulsanti senza alcun
// comportamento collegato... ognuna ora fa qualcosa di reale" — la
// vecchia vista aveva silenziosamente perso quella promessa.
//
// La sezione "Duplicati" della vecchia vista è rimossa (non commentata):
// da questa stessa tranche esiste `/duplicates` (Task 13 2/N), una vista
// reale e completa — ripeterne un riassunto qui dentro sarebbe
// ridondante e disallineato dal documento, che tratta Duplicati come
// pagina a sé (§46), mai annidata in Problemi.
//
// **Deviazione reale dal documento**: "Riprova connessione" nel mockup
// "riesce sempre" (nessun ramo "ancora offline"). Sul backend reale
// `POST /libraries/{id}/probe` (`api/libraries.ts::probeLibrary`)
// verifica per davvero se il percorso torna raggiungibile e può
// rispondere `status:'offline'` invariato — qui c'è quindi un vero
// ramo di fallimento con un toast dedicato, assente nel mockup solo
// perché la sua base dati non può fallire.
//
// **Seconda deviazione**: il dialog "Dettagli" del mockup racconta un
// percorso di rete NAS/SMB immaginario con un contatore finto. Qui
// mostra dati reali della libreria (`root_path`, `last_scan_at` come
// "ultimo contatto riuscito") — niente affermazioni su NAS/SMB che il
// backend non può verificare (`root_path` può essere qualunque percorso
// locale, montato in rete o no, il backend non lo distingue).
//
// La sezione "Ricalcolo fusi orari" (`tzPreview`/`tzApply`) resta
// invariata in fondo alla pagina: uno strumento reale, funzionante, che
// non ha alcuna controparte nel documento (§47 non lo menziona) — non
// è oggetto di questa riscrittura, solo riposizionato sotto ai problemi
// veri e propri invece di essere frammisto ai vecchi elenchi grezzi.
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'

import { isUnauthenticated } from '@/api/client'
import { fetchLibraries, probeLibrary, type Library } from '@/api/libraries'
import { fetchProblems, type ProblemView } from '@/api/library'
import {
  applyTimezones,
  previewTimezones,
  type TimezoneApplyResult,
  type TimezonePreview
} from '@/api/metadata'
import Dialog from '@/components/ui/Dialog.vue'
import ProblemFilesDialog from '@/components/ProblemFilesDialog.vue'
import { useSessionStore } from '@/stores/session'
import { useToastStore } from '@/stores/toast'

const { t, locale } = useI18n()
const router = useRouter()
const session = useSessionStore()
const toast = useToastStore()

const problems = ref<ProblemView[]>([])
const libraries = ref<Library[]>([])
const loaded = ref(false)
const loadError = ref(false)
const retrying = ref<Set<string>>(new Set())

const tzLibraryId = ref('')
const tzPreview = ref<TimezonePreview | null>(null)
const tzResult = ref<TimezoneApplyResult | null>(null)
const tzBusy = ref(false)

async function load() {
  loadError.value = false
  loaded.value = false
  try {
    const [result, libs] = await Promise.all([fetchProblems(locale.value), fetchLibraries()])
    problems.value = result.problems
    libraries.value = libs
    loaded.value = true
  } catch (error) {
    if (isUnauthenticated(error)) {
      session.user = null
      await router.push('/login')
      return
    }
    loadError.value = true
  }
}

onMounted(load)

function removeProblem(id: string) {
  problems.value = problems.value.filter((p) => p.id !== id)
}

function ignore(problem: ProblemView) {
  removeProblem(problem.id)
  toast.show(t('problems.ignored'))
}

async function retryConnection(problem: ProblemView) {
  const libraryId = problem.library_id
  if (!libraryId) return
  retrying.value = new Set(retrying.value).add(libraryId)
  toast.show(t('problems.verifying'))
  try {
    const library = await probeLibrary(libraryId)
    if (library.status === 'active') {
      removeProblem(problem.id)
      toast.show(t('problems.reconnected', { name: library.name }))
    } else {
      toast.showError(t('problems.stillOffline', { name: library.name }))
    }
  } catch {
    toast.showError(t('problems.stillOffline', { name: problem.library_name ?? '' }))
  } finally {
    const next = new Set(retrying.value)
    next.delete(libraryId)
    retrying.value = next
  }
}

const filesDialogProblem = ref<ProblemView | null>(null)
const filesDialogOpen = ref(false)

function viewFiles(problem: ProblemView) {
  filesDialogProblem.value = problem
  filesDialogOpen.value = true
}

const detailsProblem = ref<ProblemView | null>(null)
const detailsOpen = ref(false)

function openDetails(problem: ProblemView) {
  detailsProblem.value = problem
  detailsOpen.value = true
}

const detailsLibrary = computed(() =>
  detailsProblem.value?.library_id ? libraries.value.find((l) => l.id === detailsProblem.value?.library_id) : undefined
)

function relativeDays(iso: string | null | undefined): string {
  if (!iso) return t('problems.details.never')
  const days = Math.floor((Date.now() - new Date(iso).getTime()) / 86_400_000)
  return new Intl.RelativeTimeFormat(locale.value, { numeric: 'auto' }).format(-days, 'day')
}

const ACTION_HANDLERS: Record<string, (problem: ProblemView) => void> = {
  'view-files': viewFiles,
  ignore,
  'retry-connection': (problem) => void retryConnection(problem),
  details: openDetails
}

function runAction(problem: ProblemView, action: string) {
  ACTION_HANDLERS[action]?.(problem)
}

async function tzPreviewAction() {
  if (!tzLibraryId.value || tzBusy.value) return
  tzBusy.value = true
  tzPreview.value = null
  tzResult.value = null
  try {
    tzPreview.value = await previewTimezones(tzLibraryId.value)
  } finally {
    tzBusy.value = false
  }
}

async function tzApplyAction() {
  if (!tzPreview.value || !tzLibraryId.value || tzBusy.value) return
  tzBusy.value = true
  try {
    tzResult.value = await applyTimezones(tzLibraryId.value, tzPreview.value.preview_token)
    tzPreview.value = null
  } finally {
    tzBusy.value = false
  }
}
</script>

<template>
  <main class="flex h-full flex-col">
    <div
      v-if="loadError"
      class="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center"
    >
      <p class="text-content-muted">
        {{ t('common.unexpectedError') }}
      </p>
      <button
        type="button"
        class="rounded-lg border border-border px-3 py-1"
        @click="load"
      >
        {{ t('common.retry') }}
      </button>
    </div>
    <div
      v-else-if="loaded && problems.length === 0"
      class="flex flex-1 flex-col items-center justify-center gap-1 p-6 text-center"
    >
      <p class="text-sm font-semibold">
        {{ t('problems.emptyTitle') }}
      </p>
      <p class="max-w-[380px] text-sm text-content-muted">
        {{ t('problems.emptySubtitle') }}
      </p>
    </div>
    <template v-else-if="loaded">
      <div class="border-b border-border px-4 py-3">
        <p class="text-[15px] font-bold">
          {{ t('problems.title') }}
        </p>
        <p class="text-sm text-content-muted">
          {{ t('problems.subtitle', { n: problems.length }, { plural: problems.length }) }}
        </p>
      </div>

      <div class="flex flex-col gap-2.5 p-4">
        <div
          v-for="problem in problems"
          :key="problem.id"
          class="flex items-start gap-3 rounded-[10px] border border-border p-3.5"
        >
          <div
            class="flex h-[34px] w-[34px] shrink-0 items-center justify-center rounded-[9px]"
            :class="problem.severity === 'error' ? 'bg-danger-tint text-danger' : 'bg-accent-tint text-accent'"
          >
            {{ problem.severity === 'error' ? '✕' : '!' }}
          </div>
          <div class="min-w-0 flex-1">
            <p class="text-[13.5px] font-bold">
              {{ problem.title }}
            </p>
            <p class="text-[12px] text-content-muted">
              {{ problem.description }}
            </p>
            <div class="mt-2.5 flex gap-2">
              <button
                v-for="(action, index) in problem.actions"
                :key="action.action"
                type="button"
                class="rounded-lg px-2.5 py-1.5 text-[12px] font-semibold"
                :class="[
                  index === 0
                    ? 'border border-border-strong bg-surface-elevated hover:bg-border/20'
                    : 'text-content-muted hover:bg-border/20',
                  action.action === 'retry-connection' && problem.library_id && retrying.has(problem.library_id) && 'opacity-60'
                ]"
                @click="runAction(problem, action.action)"
              >
                {{ action.label }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </template>

    <div
      v-if="loaded"
      class="border-t border-border p-4"
    >
      <h2 class="font-medium">
        {{ t('problems.timezones') }}
      </h2>
      <div class="mt-2 space-y-3">
        <div class="flex items-end gap-2">
          <label class="block flex-1 text-sm">
            {{ t('problems.tzLibrary') }}
            <select
              v-model="tzLibraryId"
              class="mt-1 block w-full rounded-lg border border-border bg-surface-elevated px-3 py-2"
            >
              <option
                v-for="lib in libraries"
                :key="lib.id"
                :value="lib.id"
              >
                {{ lib.name }}
              </option>
            </select>
          </label>
          <button
            type="button"
            class="rounded-lg border border-border px-3 py-2 text-sm"
            :disabled="!tzLibraryId || tzBusy"
            @click="tzPreviewAction"
          >
            {{ t('problems.tzPreview') }}
          </button>
        </div>
        <div
          v-if="tzPreview"
          class="rounded-lg border border-border bg-surface-elevated p-3 text-sm"
        >
          <p>{{ t('problems.tzCount', { count: tzPreview.count }) }}</p>
          <p
            v-if="tzPreview.example"
            class="mt-1 text-content-muted"
          >
            {{ tzPreview.example.filename }}:
            {{ tzPreview.example.before }} → {{ tzPreview.example.after }}
            ({{ tzPreview.example.timezone }})
          </p>
          <button
            v-if="tzPreview.count > 0"
            type="button"
            class="mt-2 rounded-lg bg-accent px-3 py-2 text-white"
            :disabled="tzBusy"
            @click="tzApplyAction"
          >
            {{ t('problems.tzApply') }}
          </button>
        </div>
        <p
          v-if="tzResult"
          class="text-sm text-content-muted"
        >
          {{ t('problems.tzDone', { count: tzResult.changed_count }) }}
        </p>
      </div>
    </div>

    <ProblemFilesDialog
      v-model:open="filesDialogOpen"
      :title="filesDialogProblem?.title ?? ''"
      :description="filesDialogProblem?.description ?? ''"
      :folder-id="filesDialogProblem?.folder_id"
    />

    <Dialog
      v-model:open="detailsOpen"
      :title="t('problems.details.title')"
    >
      <p class="text-sm text-content-muted">
        {{ t('problems.details.intro') }}
      </p>
      <ul class="mt-2 list-disc space-y-1 pl-5 text-sm">
        <li>{{ t('problems.details.lastContact', { when: relativeDays(detailsLibrary?.last_scan_at) }) }}</li>
        <li>
          {{ t('problems.details.path') }}
          <code class="font-mono text-[12px]">{{ detailsLibrary?.root_path }}</code>
        </li>
        <li>{{ t('problems.details.explain') }}</li>
      </ul>
      <div class="mt-4 flex justify-end">
        <button
          type="button"
          class="rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold"
          @click="detailsOpen = false"
        >
          {{ t('problems.details.close') }}
        </button>
      </div>
    </Dialog>
  </main>
</template>
