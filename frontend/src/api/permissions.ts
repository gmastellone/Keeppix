import { apiFetch } from './client'

export interface PermissionGrant {
  id: string
  subject_type: string
  subject_id: string
  role: string
  inherit: boolean
  inherited: boolean
}

export interface ExplainChainLink {
  subject_type: string
  subject_name: string
  role: string
  granted_on_type: string
  granted_on_name: string
  inherited_in?: string | null
}

export interface ExplainResult {
  granted: boolean
  chain: ExplainChainLink[]
}

export function fetchPermissions(object_type: string, object_id: string): Promise<PermissionGrant[]> {
  const q = new URLSearchParams({ object_type, object_id })
  return apiFetch(`/api/v1/permissions?${q}`)
}

export function explainPermission(
  object_type: string,
  object_id: string,
  user_id: string
): Promise<ExplainResult> {
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

/** The "Shared with me" tab — `GET /shared-with-me` (`crates/keeppix-api/
 * src/routes/permissions.rs`). Merges direct and group grants on the
 * current user's folders/albums, with name/owner/count already resolved
 * server-side. */
export interface SharedWithMe {
  object_type: string
  object_id: string
  name: string
  owner_name: string
  role: string
  via_group?: string
  item_count: number
}

export function fetchSharedWithMe(): Promise<SharedWithMe[]> {
  return apiFetch('/api/v1/shared-with-me')
}
