import { apiFetch } from './client'

/** Seven possible kinds (`SuggestionKind` in `crates/keeppix-db/src/
 * search.rs`, a closed set): `tag` never has a real row — the tags table
 * didn't exist when this endpoint was written and no `UNION` query
 * produces it yet — so `SearchView.vue` builds the "Tag" group itself
 * from `fetchTags()`, not from here. `filename` has no matching pill (no
 * `SearchNode` for an exact filename match, only free-text `text`): it is
 * ignored by the suggestions panel. */
export type SuggestionKind = 'tag' | 'camera' | 'folder' | 'iso' | 'year' | 'country' | 'filename'

export interface Suggestion {
  kind: SuggestionKind
  value: string
  label: string
  color: string | null
}

export function fetchSuggestions(q: string): Promise<{ suggestions: Suggestion[] }> {
  return apiFetch(`/api/v1/search/suggest?q=${encodeURIComponent(q)}`)
}

export interface SavedSearch {
  id: string
  name: string
  query_text: string
}

export function fetchSavedSearches(): Promise<SavedSearch[]> {
  return apiFetch('/api/v1/saved-searches')
}

export function createSavedSearch(name: string, query_text: string): Promise<SavedSearch> {
  return apiFetch('/api/v1/saved-searches', {
    method: 'POST',
    body: JSON.stringify({ name, query_text })
  })
}
