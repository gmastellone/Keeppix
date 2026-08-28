import { computed, onMounted, ref, type Ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { fetchPersons, type Person } from '@/api/persons'
import { fetchTags, type Tag } from '@/api/tags'
import type { TimelineAsset } from '@/api/timeline'
import { matchesFilters, type FilterSelection, type MatchDimension } from '@/design/quickFilter'
import type { QuickFilterDimension } from '@/components/ui/QuickFilter.vue'
import { useShellStore } from '@/stores/shell'

// The six real filter dimensions. `QuickFilter.vue` and `design/quickFilter.ts`
// are generic by design: this module supplies the real data and the six
// `getValues` implementations. Extracted as a composable because both
// Timeline and Favorites consume it identically ("the same six timeline
// dimensions") — same approach as `useDensity`/`useIsMobile`.
//
// "Categories" and "Tags" are two **separate** dimensions, not one with a
// special case: `matchesFilters` already does AND-across-dimensions /
// OR-within-a-dimension, so treating them as two entries in `dimensions[]`
// produces the required combination ("Type = RAW AND People = Marta AND
// Location = Urbino") on its own, with no dedicated logic needed. Same
// reason "People" **doesn't need an explicit check** when face recognition
// is off: the section below already hides it (empty list), and if a People
// filter were somehow left selected with an empty list, `getValues` would
// never return that value — `matchesFilters` zeroes it out on its own, a
// hard `return false` obtained for free from the generic combination logic
// rather than an extra if-branch.
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

  // "Non-hidden people with at least one photo": this filter naturally
  // covers the "face recognition never enabled" case, and with an empty
  // list the section below isn't rendered at all — matching the prototype
  // behavior without needing an explicit toggle to read (none exists yet
  // in the frontend).
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
