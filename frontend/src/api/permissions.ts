import { apiFetch } from './client'

export interface PermissionGrant {
  id: string
  subject_type: string
  subject_id: string
  role: string
  inherit: boolean
  inherited: boolean
}

export function fetchPermissions(object_type: string, object_id: string): Promise<PermissionGrant[]> {
  const q = new URLSearchParams({ object_type, object_id })
  return apiFetch(`/api/v1/permissions?${q}`)
}

export function explainPermission(
  object_type: string,
  object_id: string,
  user_id: string
): Promise<{ granted: boolean; chain: unknown[] }> {
  const q = new URLSearchParams({ object_type, object_id, user_id })
  return apiFetch(`/api/v1/permissions/explain?${q}`)
}

export function grantPermission(body: {
  subject_type: string
  subject_id: string
  object_type: string
  object_id: string
  role: string
  inherit?: boolean
}): Promise<{ id: string }> {
  return apiFetch('/api/v1/permissions', {
    method: 'POST',
    body: JSON.stringify(body)
  })
}

export function revokePermission(id: string): Promise<null> {
  return apiFetch(`/api/v1/permissions/${id}`, { method: 'DELETE' })
}
