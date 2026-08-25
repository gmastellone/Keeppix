import { computed, onMounted, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchPersons, type Person } from '@/api/persons'
import { fetchTags, type Tag } from '@/api/tags'
import type { TimelineAsset } from '@/api/timeline'
import { matchesFilters, type FilterSelection, type MatchDimension } from '@/design/quickFilter'
import type { QuickFilterDimension } from '@/components/ui/QuickFilter.vue'
import { useShellStore } from '@/stores/shell'

// Fase 11 Task 7 (5/N) — SP-3 (§11, definizione canonica), le sei
// dimensioni reali. `QuickFilter.vue` e `design/quickFilter.ts` sono
// generici da subito (Task 2, dichiaratamente in attesa di questo
// momento): qui solo i dati veri e le sei `getValues`. Estratto come
// composable perché Timeline e Preferiti (§9.3: "le stesse sei
// dimensioni della timeline") lo consumano identico — secondo
// consumatore già noto, stesso principio di `useDensity`/`useIsMobile`.
//
// "Categorie" e "Tag" sono due dimensioni **separate**, non una con un
// caso speciale: `matchesFilters` fa già AND-fra-dimensioni/OR-dentro,
// quindi trattarle come due voci del `dimensions[]` produce da sola
// l'esempio del documento ("Tipo = RAW E Persone = Marta E Luogo =
// Urbino") senza bisogno di logica dedicata. Stesso motivo per cui
// "Persone" quando il riconoscimento volti è spento **non serve un
// controllo esplicito**: la sezione qui sotto la nasconde già
// (elenco vuoto), e se per qualche motivo un filtro Persone restasse
// selezionato con l'elenco svuotato, `getValues` non restituirebbe mai
// quel valore — `matchesFilters` lo azzera da sola, `return false`
// secco del documento (§11.3) ottenuto gratis dalla combinazione
// generica, non da un ramo if in più.
export const BROWSE_FILE_TYPE_OPTIONS = ['raw+jpeg', 'raw', 'jpeg'] as const

export function useBrowseFilters(assets: Ref<TimelineAsset[]>) {
  const { t } = useI18n()
  const shell = useShellStore()

  const selection = ref<FilterSelection>({})
  const tags = ref<Tag[]>([])
  const persons = ref<Person[]>([])

  onMounted(async () => {
    ;[tags.value, persons.value] = await Promise.all([
      fetchTags().catch(() => []),
      fetchPersons().catch(() => [])
    ])
  })

  // "Persone non nascoste con almeno una foto" (§11.2, `visiblePeople()`):
  // il conteggio filtra da solo il caso "riconoscimento volti mai attivo",
  // e con l'elenco vuoto la sezione sotto non viene disegnata affatto —
  // stesso comportamento del prototipo anche senza un interruttore
  // esplicito da leggere (nessuno esiste ancora nel frontend, Task 14).
  const visiblePersons = computed(() => persons.value.filter((p) => !p.hidden && (p.face_count ?? 0) > 0))
  const tagOptions = computed(() => tags.value.filter((tg) => tg.kind === 'tag'))
  const categoryOptions = computed(() => tags.value.filter((tg) => tg.kind === 'category'))
  const cameraModels = computed(() => {
    const models = new Set<string>()
    for (const asset of assets.value) if (asset.camera_model) models.add(asset.camera_model)
    return [...models].sort()
  })

  const matchDimensions = computed<MatchDimension<TimelineAsset>[]>(() => [
    { id: 'type', getValues: (a) => (a.raw_kind ? [a.raw_kind] : []) },
    { id: 'person', getValues: (a) => a.faces.map((f) => f.person_id) },
    { id: 'tag', getValues: (a) => a.tags.map((tg) => tg.id) },
    { id: 'category', getValues: (a) => a.tags.map((tg) => tg.category_id).filter((id) => id !== null) },
    { id: 'camera', getValues: (a) => (a.camera_model ? [a.camera_model] : []) },
    { id: 'folder', getValues: (a) => [a.folder_id] }
  ])

  const dimensions = computed<QuickFilterDimension[]>(() => {
    const out: QuickFilterDimension[] = [
      {
        id: 'type',
        label: t('browseFilter.type'),
        options: BROWSE_FILE_TYPE_OPTIONS.map((value) => ({ value, label: t(`browseFilter.typeOption.${value}`) }))
      }
    ]
    if (visiblePersons.value.length > 0) {
      out.push({
        id: 'person',
        label: t('browseFilter.person'),
        options: visiblePersons.value.map((p) => ({
          value: p.id,
          label: p.name ?? t('browseFilter.unnamedPerson')
        }))
      })
    }
    out.push({
      id: 'tag',
      label: t('browseFilter.tag'),
      options: tagOptions.value.map((tg) => ({ value: tg.id, label: tg.name, color: tg.color ?? undefined }))
    })
    out.push({
      id: 'category',
      label: t('browseFilter.category'),
      options: categoryOptions.value.map((tg) => ({ value: tg.id, label: tg.name }))
    })
    out.push({
      id: 'camera',
      label: t('browseFilter.camera'),
      options: cameraModels.value.map((model) => ({ value: model, label: model }))
    })
    out.push({
      id: 'folder',
      label: t('browseFilter.folder'),
      options: shell.folders.map((f) => ({ value: f.id, label: f.name }))
    })
    return out
  })

  const filteredAssets = computed(() => assets.value.filter((a) => matchesFilters(a, matchDimensions.value, selection.value)))

  return { selection, dimensions, filteredAssets }
}
