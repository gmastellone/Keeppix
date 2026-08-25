import { apiFetch } from './client'

/** Sette generi possibili (`SuggestionKind` in `crates/keeppix-db/src/
 * search.rs`, insieme chiuso): `tag` non ha mai una riga reale — la
 * tabella dei tag non esisteva quando questo endpoint fu scritto (Fase
 * 10) e nessuna query `UNION` la produce ancora — quindi `SearchView.vue`
 * costruisce il gruppo "Tag" per conto proprio da `fetchTags()`, non da
 * qui. `filename` non ha una pillola corrispondente (nessun `SearchNode`
 * di corrispondenza esatta sul nome file, solo `text` libero): ignorato
 * dal pannello dei suggerimenti. */
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
