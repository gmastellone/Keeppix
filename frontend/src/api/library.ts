import { apiFetch } from './client'
import type { DiskAction } from './culling'
import type { SearchNode } from '@/search/ast'
import type { TimelineAsset, TimelinePage } from './timeline'

/** `limit` (Task 16 1/N): campo reale di `SearchRequest` (`crates/
 * keeppix-api/src/routes/search.rs`), mai passato dal frontend finora —
 * la griglia Persone lo usa per prendere una sola foto per persona come
 * copertina, senza scaricare un'intera pagina da scartare. */
export function runSearch(ast: SearchNode, cursor?: string, limit?: number): Promise<TimelinePage> {
  return apiFetch('/api/v1/search', {
    method: 'POST',
    body: JSON.stringify({ ast, ...(cursor ? { cursor } : {}), ...(limit ? { limit } : {}) })
  })
}

/** Un'azione proposta da un problema (§47.3) — l'etichetta è già pronta
 * per un bottone, la chiave (`view-files`/`ignore`/`retry-connection`/
 * `details`) sceglie il gestore. */
export interface ProblemAction {
  action: string
  label: string
}

/** Una riga dell'elenco composto (Task 13, `crates/keeppix-api/src/
 * routes/problems.rs::ProblemView`) — già in linguaggio naturale nella
 * lingua richiesta (`?lang=`), con titolo/descrizione/azioni pronti:
 * quello che manca al terzetto grezzo sotto (`offline_libraries`/
 * `failed_jobs`/`error_assets`) per essere `.problem-row` del documento
 * senza logica di composizione nel frontend. */
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
  /** Bug reale corretto qui (Task 13 3/N): questo campo esiste sul
   * backend da quando `ProblemView` è stato scritto, ma il tipo qui non
   * lo dichiarava — `ProblemsView.vue` leggeva solo i tre secchi grezzi
   * sopra, ricostruendo a mano un'interfaccia molto più povera di
   * quella già pronta. */
  problems: ProblemView[]
}

export interface DuplicateGroup {
  content_hash: string
  count: number
  size_bytes: number
  reclaimable_bytes: number
}

/** `lang` sceglie la lingua delle descrizioni composte lato server
 * (`?lang=it|en`, default dall'`Accept-Language` del browser, poi
 * italiano) — passare la lingua dell'interfaccia evita un disallineamento
 * fra un titolo in inglese e un'interfaccia in italiano. */
export function fetchProblems(lang?: string): Promise<Problems> {
  return apiFetch(`/api/v1/problems${lang ? `?lang=${encodeURIComponent(lang)}` : ''}`)
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
