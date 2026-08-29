import { apiFetch } from './client'

export interface AppPassword {
  id: string
  label: string
  created_at: string
  last_used_at: string | null
}

export interface AppPasswordCreated extends AppPassword {
  /** Present only in the creation response: the server does not store it in plaintext. */
  secret: string
}

export function listAppPasswords(): Promise<AppPassword[]> {
  return apiFetch('/api/v1/users/me/app-passwords')
}

export function createAppPassword(label: string): Promise<AppPasswordCreated> {
  return apiFetch('/api/v1/users/me/app-passwords', {
    method: 'POST',
    body: JSON.stringify({ label })
  })
}

export function deleteAppPassword(id: string): Promise<null> {
  return apiFetch(`/api/v1/users/me/app-passwords/${encodeURIComponent(id)}`, {
    method: 'DELETE'
  })
}
