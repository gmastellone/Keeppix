import { apiFetch } from './client'

export function fetchSuggestions(q: string): Promise<{ suggestions: string[] }> {
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
