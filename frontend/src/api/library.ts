import { apiFetch } from './client'
import type { SearchNode } from '@/search/ast'
import type { TimelinePage } from './timeline'

export function runSearch(ast: SearchNode, cursor?: string): Promise<TimelinePage> {
  return apiFetch('/api/v1/search', {
    method: 'POST',
    body: JSON.stringify(cursor ? { ast, cursor } : { ast })
  })
}

export interface Problems {
  offline_libraries: { id: string; name: string }[]
  failed_jobs: { id: number; kind: string; last_error: string | null }[]
  error_assets: { id: string; filename: string }[]
}

export interface DuplicateGroup {
  content_hash: string
  count: number
  size_bytes: number
  reclaimable_bytes: number
}

export function fetchProblems(): Promise<Problems> {
  return apiFetch('/api/v1/problems')
}

export function fetchDuplicates(): Promise<DuplicateGroup[]> {
  return apiFetch('/api/v1/duplicates')
}
