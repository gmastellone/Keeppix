import { apiFetch } from './client'

export interface AuditEntry {
  id: number
  actor_kind: string
  action: string
  at: string
}

export function fetchAuditLog(limit = 50): Promise<AuditEntry[]> {
  return apiFetch(`/api/v1/audit?limit=${limit}`)
}
