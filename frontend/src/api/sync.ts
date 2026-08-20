import { apiFetch } from './client'

export interface SyncDelta {
  cursor: number
  upserted: string[]
  deleted: string[]
  has_more: boolean
}

export function fetchSyncDelta(cursor = 0): Promise<SyncDelta> {
  return apiFetch(`/api/v1/sync/delta?cursor=${cursor}`)
}
