import { apiFetch } from './client'
import type { TimelineAsset } from './timeline'

/** Frontend client for `GET /assets/{id}/stack`, used by the RAW/JPEG
 * switcher. Each member is a full `TimelineAsset` (same per-file
 * `raw_kind` used elsewhere to distinguish RAW from JPEG) plus
 * `is_primary`. */
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
