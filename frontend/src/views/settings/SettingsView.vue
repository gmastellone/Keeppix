<script setup lang="ts">
// Fase 11 Task 14 (1/N) — documento funzionale §60 "Impostazioni",
// verificato riga per riga (righe 8813-9127). Sette delle otto sezioni
// hanno una capacità reale da rispecchiare fedelmente; una no, e resta
// fuori da questa pagina, dichiarata esplicitamente qui invece che
// finta con dati a caso:
//
// - **"Intelligenza artificiale"**: i numeri reali esistono
//   (`AnalysisLevel::ms_per_photo()`, `crates/keeppix-jobs/src/
//   profile.rs:50-74`, 45ms Piena/270ms Ridotta, misurati sul vero
//   modello MobileCLIP2-S2) ma **nessuna rotta li legge**: stesso
//   motivo, stessa scelta.
//
// Le altre sette sezioni sono reali:
// - **Cartella di culling** (Task 17, 2/N): `PATCH .../culling-root` e
//   `GET .../culling/lots` non esistevano quando questa pagina fu
//   scritta la prima volta (Task 14) — la sezione restava dichiarata
//   fuori scope. Le rotte sono arrivate col Task 17 (Ruling nel ledger
//   del 24 agosto): una riga per libreria, stesso adattamento già
//   scelto per "Riconoscimento volti" qui sotto (per libreria, non per
//   istanza come lo descrive il documento). Il percorso mostrato è una
//   briciola di **nomi** di cartella (`folders.name`, risalendo
//   `parent_id` con `fetchAllFolders()`), non un percorso su disco: il
//   backend non espone un percorso assoluto per una cartella qualunque
//   (solo `Library.root_path`, la radice della libreria intera).
// - **Aspetto**: `stores/theme.ts` (Task 14, questa unità), preferenze
//   server (`GET/PATCH /users/me/preferences`, Fase 10 Task 9, mai
//   consumate dal frontend prima d'ora).
// - **Densità griglia**: `useDensity` (riscritto in questa unità),
//   stessa fonte, due valori distinti desktop/mobile per davvero.
// - **Mappe offline**: `MapsOfflineView.vue` esiste già, completa —
//   qui solo il collegamento reale.
// - **Notifiche**: tre preferenze reali, mai un effetto visibile altrove
//   nell'app (nessun sottosistema di notifiche esiste ancora) — la
//   stessa scrittura del documento, che le tratta come sole preferenze.
// - **Lingua**: a differenza del mockup ("il select non ha id né alcun
//   gestore: cambiarlo non fa nulla"), qui `session.changeLocale` è già
//   reale e funzionante dalla Fase 10 — un miglioramento reale, non un
//   controllo inerte.
// - **Riconoscimento volti**: reale ma **per libreria**, non per
//   istanza come lo descrive il documento (`LibraryView.faces_enabled`)
//   — una riga per libreria, non un solo interruttore. "Elimina tutti i
//   dati dei volti" è invece davvero globale e admin-only
//   (`DELETE /faces/data`).
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchCullingLots } from '@/api/culling'
import { deleteAllFaceData } from '@/api/faces'
import { fetchAllFolders, type FolderView } from '@/api/folders'
import { fetchLibraries, patchCullingRoot, patchLibrary, type Library } from '@/api/libraries'
import { fetchPreferences, patchPreferences, type NotificationPreferences, type Theme } from '@/api/preferences'
import CullingRootPickerDialog from '@/components/CullingRootPickerDialog.vue'
import ConfirmDialog from '@/components/ui/ConfirmDialog.vue'
import SegmentedControl from '@/components/ui/SegmentedControl.vue'
import { useDensity } from '@/composables/useDensity'
import { useSessionStore } from '@/stores/session'
import { useThemeStore } from '@/stores/theme'
import { useToastStore } from '@/stores/toast'
import { folderPathName } from '@/utils/folderPath'

const { t, locale } = useI18n()
const session = useSessionStore()
const theme = useThemeStore()
const toast = useToastStore()
const { density, setDensity, isMobile } = useDensity()

const isAdmin = computed(() => session.user?.role === 'admin')

const themeOptions = [
  { value: 'chiaro', label: t('settings.appearance.light') },
  { value: 'scuro', label: t('settings.appearance.dark') },
  { value: 'sistema', label: t('settings.appearance.system') }
]

async function onThemeChange(next: string) {
  try {
    await theme.setPreference(next as Theme)
  } catch {
    toast.showError(t('settings.faces.actionError'))
  }
}

const densitySubtitle = computed(() =>
  t(isMobile.value ? 'settings.density.subtitleMobile' : 'settings.density.subtitleDesktop', { n: density.value })
)

const notifications = ref<NotificationPreferences>({ digest: true, condivisioni: true, problemi: true })

const NOTIFICATION_I18N_KEY: Record<keyof NotificationPreferences, string> = {
  digest: 'digest',
  condivisioni: 'shares',
  problemi: 'problems'
}

async function toggleNotification(key: keyof NotificationPreferences) {
  const next = !notifications.value[key]
  notifications.value = { ...notifications.value, [key]: next }
  try {
    await patchPreferences({ notifications: { [key]: next } })
  } catch {
    notifications.value = { ...notifications.value, [key]: !next }
    toast.showError(t('settings.faces.actionError'))
  }
}

async function onLanguageChange(event: Event) {
  const next = (event.target as HTMLSelectElement).value as 'it' | 'en'
  await session.changeLocale(next)
}

const libraries = ref<Library[]>([])
const facesBusy = ref<Set<string>>(new Set())
const deleteAllOpen = ref(false)

async function toggleFaces(library: Library) {
  if (!isAdmin.value || facesBusy.value.has(library.id)) return
  facesBusy.value = new Set(facesBusy.value).add(library.id)
  try {
    const updated = await patchLibrary(library.id, { faces_enabled: !library.faces_enabled })
    libraries.value = libraries.value.map((l) => (l.id === library.id ? updated : l))
  } catch {
    toast.showError(t('settings.faces.actionError'))
  } finally {
    const next = new Set(facesBusy.value)
    next.delete(library.id)
    facesBusy.value = next
  }
}

async function confirmDeleteAllFaces() {
  try {
    await deleteAllFaceData()
    toast.show(t('settings.faces.deleteDone'))
  } catch {
    toast.showError(t('settings.faces.actionError'))
  }
}

const allFolders = ref<FolderView[]>([])
const foldersById = computed(() => new Map(allFolders.value.map((f) => [f.id, f])))
const lotCounts = ref<Record<string, number>>({})
const cullingBusy = ref<Set<string>>(new Set())
const pickerOpen = ref(false)
const pickerLibraryId = ref<string | null>(null)

function canChangeCullingRoot(library: Library): boolean {
  return isAdmin.value || library.owner_id === session.user?.id
}

function libraryRoot(library: Library): FolderView | undefined {
  return allFolders.value.find((f) => f.library_id === library.id && f.parent_id === null)
}

function cullingPathName(library: Library): string | null {
  return library.culling_root_folder_id
    ? folderPathName(library.culling_root_folder_id, foldersById.value)
    : null
}

/** Radice-a-foglia per il dialog (§17.2: "si posiziona sul percorso
 * attualmente configurato... se quel percorso esiste nell'albero;
 * altrimenti riparte dalla radice"). `[]` solo se la libreria non ha
 * ancora una cartella radice propria (scansione non ancora avvenuta). */
function pickerInitialPath(library: Library): FolderView[] {
  const root = libraryRoot(library)
  if (!root) return []
  const id = library.culling_root_folder_id
  if (!id) return [root]
  const chain: FolderView[] = []
  let current = foldersById.value.get(id)
  while (current) {
    chain.unshift(current)
    if (current.id === root.id) return chain
    current = current.parent_id ? foldersById.value.get(current.parent_id) : undefined
  }
  return [root]
}

const pickerLibrary = computed(() => libraries.value.find((l) => l.id === pickerLibraryId.value) ?? null)
const pickerPath = computed(() => (pickerLibrary.value ? pickerInitialPath(pickerLibrary.value) : []))

function openCullingRootPicker(library: Library) {
  pickerLibraryId.value = library.id
  pickerOpen.value = true
}

async function loadLotCount(libraryId: string) {
  try {
    const lots = await fetchCullingLots(libraryId)
    lotCounts.value = { ...lotCounts.value, [libraryId]: lots.length }
  } catch {
    // Conteggio informativo: un fallimento qui non deve bloccare la sezione,
    // resta semplicemente assente (il template ricade su 0).
  }
}

async function onCullingRootConfirm(folderId: string) {
  const library = pickerLibrary.value
  if (!library) return
  cullingBusy.value = new Set(cullingBusy.value).add(library.id)
  try {
    const updated = await patchCullingRoot(library.id, folderId)
    libraries.value = libraries.value.map((l) => (l.id === library.id ? updated : l))
    toast.show(t('settings.cullingRoot.updated'))
    await loadLotCount(library.id)
  } catch {
    toast.showError(t('settings.cullingRoot.actionError'))
  } finally {
    const next = new Set(cullingBusy.value)
    next.delete(library.id)
    cullingBusy.value = next
  }
}

onMounted(async () => {
  const [prefs, libs, folders] = await Promise.all([
    fetchPreferences().catch(() => null),
    fetchLibraries().catch(() => []),
    fetchAllFolders().catch(() => [])
  ])
  if (prefs) notifications.value = prefs.notifications
  libraries.value = libs
  allFolders.value = folders
  await Promise.all(libs.filter((l) => l.culling_root_folder_id).map((l) => loadLotCount(l.id)))
})
</script>

<template>
  <main class="mx-auto max-w-[560px] p-6">
    <p class="text-[15px] font-bold">
      {{ t('settings.title') }}
    </p>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('settings.appearance.title') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('settings.appearance.subtitle') }}
      </p>
      <SegmentedControl
        class="mt-2.5"
        :model-value="theme.preference"
        :options="themeOptions"
        :aria-label="t('settings.appearance.title')"
        @update:model-value="onThemeChange"
      />
    </section>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('settings.density.title') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ densitySubtitle }}
      </p>
      <input
        type="range"
        class="mt-2.5 w-full"
        :min="2"
        :max="isMobile ? 6 : 12"
        :value="density"
        @input="setDensity(Number(($event.target as HTMLInputElement).value))"
      >
    </section>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('settings.maps.title') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('settings.maps.subtitle') }}
      </p>
      <RouterLink
        to="/settings/maps/offline"
        class="mt-2.5 inline-block rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold hover:bg-border/20"
      >
        {{ t('settings.maps.manage') }}
      </RouterLink>
    </section>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('settings.notifications.title') }}
      </p>
      <div class="mt-2.5 flex flex-col gap-2.5">
        <label
          v-for="key in (['digest', 'condivisioni', 'problemi'] as const)"
          :key="key"
          class="flex cursor-pointer items-center justify-between"
        >
          <span class="text-[13px]">{{ t(`settings.notifications.${NOTIFICATION_I18N_KEY[key]}`) }}</span>
          <button
            type="button"
            role="switch"
            :aria-checked="notifications[key]"
            class="relative h-5 w-9 shrink-0 rounded-full transition-colors"
            :style="{ transitionDuration: 'var(--duration-arrow)' }"
            :class="notifications[key] ? 'bg-accent' : 'bg-border'"
            @click="toggleNotification(key)"
          >
            <span
              class="absolute top-0.5 h-4 w-4 rounded-full bg-white transition-[left]"
              :style="{ left: notifications[key] ? '18px' : '2px', transitionDuration: 'var(--duration-arrow)' }"
            />
          </button>
        </label>
      </div>
    </section>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('settings.language.title') }}
      </p>
      <select
        class="mt-2.5 max-w-[220px] rounded-lg border border-border bg-surface-elevated px-3 py-2 text-sm"
        :value="session.user?.locale ?? locale"
        @change="onLanguageChange"
      >
        <option value="it">
          Italiano
        </option>
        <option value="en">
          English
        </option>
      </select>
    </section>

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('settings.faces.title') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('settings.faces.subtitle') }}
      </p>
      <p
        v-if="!isAdmin"
        class="mt-1.5 text-[12px] text-content-muted italic"
      >
        {{ t('settings.faces.adminOnly') }}
      </p>
      <div class="mt-2.5 flex flex-col gap-2.5">
        <label
          v-for="library in libraries"
          :key="library.id"
          class="flex items-center justify-between"
          :class="isAdmin ? 'cursor-pointer' : 'cursor-not-allowed opacity-60'"
        >
          <span class="text-[13px]">{{ t('settings.faces.toggleLabel', { name: library.name }) }}</span>
          <button
            type="button"
            role="switch"
            :aria-checked="library.faces_enabled"
            :disabled="!isAdmin || facesBusy.has(library.id)"
            class="relative h-5 w-9 shrink-0 rounded-full transition-colors disabled:cursor-not-allowed"
            :style="{ transitionDuration: 'var(--duration-arrow)' }"
            :class="library.faces_enabled ? 'bg-accent' : 'bg-border'"
            @click="toggleFaces(library)"
          >
            <span
              class="absolute top-0.5 h-4 w-4 rounded-full bg-white transition-[left]"
              :style="{ left: library.faces_enabled ? '18px' : '2px', transitionDuration: 'var(--duration-arrow)' }"
            />
          </button>
        </label>
      </div>
      <button
        v-if="isAdmin"
        type="button"
        class="mt-3.5 flex items-center gap-1.5 rounded-lg border border-danger px-3.5 py-2 text-[13px] font-semibold text-danger hover:bg-danger/10"
        @click="deleteAllOpen = true"
      >
        {{ t('settings.faces.deleteAll') }}
      </button>
    </section>

    <ConfirmDialog
      v-model:open="deleteAllOpen"
      :title="t('settings.faces.deleteConfirmTitle')"
      :description="t('settings.faces.deleteConfirmDescription')"
      :confirm-label="t('settings.faces.deleteConfirmButton')"
      @confirm="confirmDeleteAllFaces"
    />

    <section class="mt-6">
      <p class="text-[13.5px] font-semibold">
        {{ t('settings.cullingRoot.title') }}
      </p>
      <p class="text-sm text-content-muted">
        {{ t('settings.cullingRoot.subtitle') }}
      </p>
      <div class="mt-2.5 flex flex-col gap-3">
        <div
          v-for="library in libraries"
          :key="library.id"
          class="flex items-center justify-between gap-3"
        >
          <div class="min-w-0">
            <p class="truncate text-[13px] font-medium">
              {{ library.name }}
            </p>
            <p class="truncate text-[12px] text-content-muted">
              {{ cullingPathName(library) ?? t('settings.cullingRoot.notSet') }}
              <template v-if="library.culling_root_folder_id">
                — {{ t('settings.cullingRoot.lotsCount', { n: lotCounts[library.id] ?? 0 }) }}
              </template>
            </p>
          </div>
          <button
            v-if="canChangeCullingRoot(library)"
            type="button"
            :disabled="cullingBusy.has(library.id)"
            class="shrink-0 rounded-lg border border-border px-3.5 py-2 text-[13px] font-semibold
                   hover:bg-border/20 disabled:cursor-not-allowed disabled:opacity-60"
            @click="openCullingRootPicker(library)"
          >
            {{ t('settings.cullingRoot.change') }}
          </button>
        </div>
      </div>
    </section>

    <CullingRootPickerDialog
      v-model:open="pickerOpen"
      :initial-path="pickerPath"
      @confirm="onCullingRootConfirm"
    />
  </main>
</template>
