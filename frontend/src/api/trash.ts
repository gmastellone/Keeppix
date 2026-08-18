import { apiFetch } from './client'

export interface TrashedAsset {
  id: string
  filename: string
  deleted_at: string
  expires_at: string
}

export function fetchTrash(): Promise<{ items: TrashedAsset[] }> {
  return apiFetch('/api/v1/trash')
}

export function restoreAsset(id: string): Promise<null> {
  return apiFetch(`/api/v1/assets/${id}/restore`, { method: 'POST' })
}

export function emptyTrash(): Promise<null> {
  return apiFetch('/api/v1/trash/empty', { method: 'POST' })
}
