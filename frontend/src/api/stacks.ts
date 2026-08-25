import { apiFetch } from './client'
import type { TimelineAsset } from './timeline'

/** Fase 11 Task 8 (8/N), §19.2 riga 5 (commutatore RAW/JPEG): primo
 * consumatore frontend di `GET /assets/{id}/stack` (Fase 10) — costruita
 * per lo stack RAW+JPEG, mai chiamata da questa app finora. Ogni membro è
 * un `TimelineAsset` completo (stesso `raw_kind` per-file usato altrove
 * per distinguere RAW da JPEG), più `is_primary`. */
export interface StackMember extends TimelineAsset {
  is_primary: boolean
}

export interface Stack {
  stack_id: string | null
  primary_asset_id: string | null
  members: StackMember[]
}

export function fetchStack(assetId: string): Promise<Stack> {
  return apiFetch(`/api/v1/assets/${assetId}/stack`)
}
