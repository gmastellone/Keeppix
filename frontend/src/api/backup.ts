import { apiFetch } from './client'

export interface BackupPreferences {
  include_database: boolean
  include_config: boolean
  include_maps: boolean
  include_sidecars: boolean
  include_derivatives: boolean
  include_originals: boolean
  schedule: string
  retention: string
  retention_n: number | null
  verify_after_write: boolean
  monthly_restore_proof: boolean
  passphrase_set: boolean
  originals_warning: boolean
  originals_warning_message: string | null
}

export interface BackupDestination {
  id: string
  kind: string
  label: string
  config: Record<string, unknown>
  enabled: boolean
}

export interface BackupRun {
  id: string
  destination_id: string | null
  started_at: string
  completed_at: string | null
  size_bytes: number | null
  verified_at: string | null
  status: string
  error: string | null
  path: string | null
  keeppix_version: string | null
}

export function getBackupPreferences(): Promise<BackupPreferences> {
  return apiFetch('/api/v1/backup/preferences')
}

export function putBackupPreferences(
  body: Record<string, unknown>
): Promise<BackupPreferences> {
  return apiFetch('/api/v1/backup/preferences', {
    method: 'PUT',
    body: JSON.stringify(body)
  })
}

export function listBackupDestinations(): Promise<BackupDestination[]> {
  return apiFetch('/api/v1/backup/destinations')
}

export function createBackupDestination(body: {
  kind: string
  label: string
  config: Record<string, unknown>
}): Promise<BackupDestination> {
  return apiFetch('/api/v1/backup/destinations', {
    method: 'POST',
    body: JSON.stringify(body)
  })
}

export function deleteBackupDestination(id: string): Promise<null> {
  return apiFetch(`/api/v1/backup/destinations/${id}`, { method: 'DELETE' })
}

export function testBackupDestination(id: string): Promise<null> {
  return apiFetch(`/api/v1/backup/destinations/${id}/test`, {
    method: 'POST',
    body: '{}'
  })
}

export function listBackupRuns(): Promise<BackupRun[]> {
  return apiFetch('/api/v1/backup/runs')
}

export function runBackupNow(): Promise<null> {
  return apiFetch('/api/v1/backup/run', { method: 'POST', body: '{}' })
}

export interface RestoreInspect {
  manifest: Record<string, unknown>
  rejected: boolean
  reject_reason: string | null
}

export function inspectBackup(path: string, passphrase: string): Promise<RestoreInspect> {
  return apiFetch('/api/v1/restore/inspect', {
    method: 'POST',
    body: JSON.stringify({ path, passphrase })
  })
}

export function restoreBackup(body: {
  path: string
  passphrase: string
  components?: { database?: boolean; config?: boolean; maps?: boolean }
  dry_run?: boolean
}): Promise<Record<string, unknown>> {
  return apiFetch('/api/v1/restore', {
    method: 'POST',
    body: JSON.stringify(body)
  })
}
