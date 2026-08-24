import { apiFetch } from './client'
import type { DiskAction } from './culling'
import type { SearchNode } from '@/search/ast'
import type { TimelineAsset, TimelinePage } from './timeline'

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

/** §46, i membri reali di un gruppo — `AssetView` (`routes/duplicates.rs
 * ::members`), stessa forma di `TimelineAsset`: a differenza del
 * mockup, dove "i file di un gruppo sono record leggeri indipendenti
 * dal catalogo" (nota per l'architetto, §46.9), qui sono vere foto del
 * catalogo, con vera miniatura. Mai i membri cestinati (già esclusi dal
 * backend). */
export function fetchDuplicateMembers(contentHash: string): Promise<TimelineAsset[]> {
  return apiFetch(`/api/v1/duplicates/${contentHash}`)
}

/** "Risolvi gruppo": applica `diskAction` a ogni membro del gruppo
 * **tranne** `keep`, in una sola chiamata — a differenza del mockup, che
 * secondo la nota per l'architetto (§46.9) raccoglie la modalità scelta
 * ma "non applica davvero" nulla: qui la propaga per davvero, come
 * promesso dal dialog di eliminazione a 3 opzioni (§9) che la produce. */
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
