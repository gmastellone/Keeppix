import { apiFetch } from './client'

export interface ShareLink {
  id: string
  object_type: string
  object_id: string
  has_password: boolean
  expires_at: string | null
  max_views: number | null
  view_count: number
  allow_download: boolean
  allow_original: boolean
  allow_upload: boolean
  /** Public list omits capture dates when true. Not a location geofence. */
  hide_metadata: boolean
  revoked_at: string | null
  last_accessed_at: string | null
  created_at: string
}

export interface SharedAsset {
  id: string
  folder_id: string
  filename: string
  content_hash: string | null
}

export interface SharedContent {
  object_type: string
  assets: SharedAsset[]
}

export function fetchShareLinks(): Promise<ShareLink[]> {
  return apiFetch('/api/v1/share/links')
}

export function createShareLink(body: {
  object_type: string
  object_id: string
  password?: string
  expires_at?: string
  max_views?: number
  allow_download?: boolean
  allow_original?: boolean
  allow_upload?: boolean
  hide_metadata?: boolean
}): Promise<{ id: string; token: string }> {
  return apiFetch('/api/v1/share/links', {
    method: 'POST',
    body: JSON.stringify(body)
  })
}

export function revokeShareLink(id: string): Promise<null> {
  return apiFetch(`/api/v1/share/links/${id}`, { method: 'DELETE' })
}

export function fetchPublicShareInfo(token: string): Promise<Record<string, unknown>> {
  return apiFetch(`/api/v1/share/${token}`)
}

export function uploadGuestFile(
  token: string,
  filename: string,
  file: Blob
): Promise<{ id: string; filename: string }> {
  return apiFetch(
    `/api/v1/share/${token}/uploads?filename=${encodeURIComponent(filename)}`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/octet-stream' },
      body: file
    }
  )
}

export function authenticateShare(token: string, password?: string): Promise<null> {
  return apiFetch(`/api/v1/share/${token}/auth`, {
    method: 'POST',
    body: JSON.stringify({ password: password ?? null })
  })
}

export function fetchSharedContent(token: string): Promise<SharedContent> {
  return apiFetch(`/api/v1/share/${token}/assets`, {
    headers: { 'X-Share-Token': token }
  })
}

export function approveGuestUpload(id: string): Promise<null> {
  return apiFetch(`/api/v1/guest-uploads/${id}/approve`, { method: 'POST' })
}
