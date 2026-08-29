import { apiFetch } from './client'
import type { DiskAction } from './culling'
import type { SearchNode } from '@/search/ast'
import type { TimelineAsset, TimelinePage } from './timeline'

/** `limit`: a real field of `SearchRequest` (`crates/keeppix-api/src/
 * routes/search.rs`) — the People grid uses it to fetch a single photo
 * per person as a cover, without downloading an entire page just to
 * discard most of it. */
export function runSearch(ast: SearchNode, cursor?: string, limit?: number): Promise<TimelinePage> {
  return apiFetch('/api/v1/search', {
    method: 'POST',
    body: JSON.stringify({ ast, ...(cursor ? { cursor } : {}), ...(limit ? { limit } : {}) })
  })
}

/** An action proposed by a problem — the label is ready for a button,
 * the key (`view-files`/`ignore`/`retry-connection`/`details`) picks the
 * handler. */
export interface ProblemAction {
  action: string
  label: string
}

/** A row of the composed list (`crates/keeppix-api/src/routes/
 * problems.rs::ProblemView`) — already natural-language text in the
 * requested language (`?lang=`), with title/description/actions ready to
 * go: what the raw trio below (`offline_libraries`/`failed_jobs`/
 * `error_assets`) is missing to become a rendered problem row without
 * any composition logic in the frontend. */
export interface ProblemView {
  id: string
  severity: 'warning' | 'error'
  title: string
  description: string
  library_id?: string
  library_name?: string
  folder_id?: string
  folder_name?: string
  actions: ProblemAction[]
}

export interface Problems {
  offline_libraries: { id: string; name: string }[]
  failed_jobs: { id: number; kind: string; last_error: string | null }[]
  error_assets: { id: string; filename: string }[]
  /** A real bug fixed here: this field has existed on the backend since
   * `ProblemView` was written, but the type here didn't declare it —
   * `ProblemsView.vue` only read the three raw buckets above, manually
   * rebuilding an interface much poorer than the one already
   * available. */
  problems: ProblemView[]
}

export interface DuplicateGroup {
  content_hash: string
  count: number
  size_bytes: number
  reclaimable_bytes: number
}

/** `lang` picks the language of the server-composed descriptions
 * (`?lang=it|en`, defaults to the browser's `Accept-Language`, then
 * Italian) — passing the UI's language avoids a mismatch between an
 * English title and an Italian interface. */
export function fetchProblems(lang?: string): Promise<Problems> {
  return apiFetch(`/api/v1/problems${lang ? `?lang=${encodeURIComponent(lang)}` : ''}`)
}

export function fetchDuplicates(): Promise<DuplicateGroup[]> {
  return apiFetch('/api/v1/duplicates')
}

/** The actual members of a group — `AssetView`
 * (`routes/duplicates.rs::members`), the same shape as `TimelineAsset`:
 * these are real catalog photos with real thumbnails, not lightweight
 * records independent of the catalog. Trashed members are never
 * included (already excluded by the backend). */
export function fetchDuplicateMembers(contentHash: string): Promise<TimelineAsset[]> {
  return apiFetch(`/api/v1/duplicates/${contentHash}`)
}

/** "Resolve group": applies `diskAction` to every member of the group
 * **except** `keep`, in a single call — the choice actually gets applied
 * server-side, as promised by the three-option deletion dialog that
 * produces it. */
export function resolveDuplicateGroup(
  contentHash: string,
  keep: string,
  diskAction: DiskAction
): Promise<{ resolved: number }> {
  return apiFetch(`/api/v1/duplicates/${contentHash}/resolve`, {
    method: 'POST',
    body: JSON.stringify({ keep, disk_action: diskAction })
  })
}
