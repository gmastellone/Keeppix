import { apiFetch } from './client'
import type { User } from './auth'
import type { FolderView } from './folders'

export interface LibraryStorageView {
  free_bytes: number
  total_bytes: number
}

/** SP-25 badge del Culling: sempre presente, anche a 0 (documento
 * funzionale §2.7 — "renderizzato sempre, anche con valore 0"). Vale 0
 * finché i lotti di culling non sono esposti da una rotta reale (Task
 * 17, Tranche D) — il backend lo dichiara esplicitamente nel proprio
 * commento (`BadgeCountsView.culling`, "Zero finché i lotti non esistono
 * nel backend"), non un valore finto lato frontend. */
export interface BadgeCountsView {
  culling: number
  /** Proposte tag + volti in attesa (`AssetTagRepo`/`FaceRepo`
   * `count_proposed_visible`, già al sicuro se il riconoscimento volti è
   * spento) — badge "Revisione" (sidebar §2.2, mobile §6.2), mostrato
   * solo se > 0. */
  revision: number
}

export interface BootstrapResponse {
  user: User
  folders: FolderView[]
  /** Per id libreria — stesso payload di `GET /libraries/{id}/storage`. */
  storage: Record<string, LibraryStorageView>
  badges: BadgeCountsView
}

/**
 * Un'unica chiamata per i dati della shell (sidebar desktop + header/
 * "Altro" mobile, Task 6): cartelle, spazio libero per libreria, badge
 * di navigazione. Distinto da `session.bootstrap()` (stato di setup +
 * autenticazione, `stores/session.ts`) nonostante il nome uguale — due
 * concetti diversi che il backend chiama entrambi "bootstrap".
 */
export function fetchBootstrap(): Promise<BootstrapResponse> {
  return apiFetch('/api/v1/bootstrap')
}
