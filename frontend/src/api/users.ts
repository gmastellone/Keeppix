import { apiFetch } from './client'

export interface UserSummary {
  id: string
  username: string
  display_name: string
  role: string
  locale: string | null
  disabled_at: string | null
}

export function fetchUsers(): Promise<UserSummary[]> {
  return apiFetch('/api/v1/users')
}

export function createUser(body: {
  username: string
  display_name: string
  password: string
  role?: string
}): Promise<UserSummary> {
  return apiFetch('/api/v1/users', {
    method: 'POST',
    body: JSON.stringify(body)
  })
}

export function updateUser(
  id: string,
  patch: { display_name?: string; locale?: string; role?: string }
): Promise<UserSummary> {
  return apiFetch(`/api/v1/users/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(patch)
  })
}

export function disableUser(id: string): Promise<null> {
  return apiFetch(`/api/v1/users/${id}/disable`, { method: 'POST' })
}

export function enableUser(id: string): Promise<null> {
  return apiFetch(`/api/v1/users/${id}/enable`, { method: 'POST' })
}

export function changePassword(current_password: string, new_password: string): Promise<null> {
  return apiFetch('/api/v1/users/me/password', {
    method: 'POST',
    body: JSON.stringify({ current_password, new_password })
  })
}
