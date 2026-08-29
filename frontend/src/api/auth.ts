import { apiFetch } from './client'

/**
 * Public user view returned by the API. Contains neither the password
 * hash nor the TOTP secret: those fields never leave the backend.
 */
export interface User {
  id: string
  username: string
  display_name: string
  email: string | null
  role: 'admin' | 'user'
  locale: string | null
  disabled_at?: string | null
}

export interface SetupPayload {
  username: string
  display_name: string
  email?: string
  password: string
}

export function getSetupStatus(): Promise<{ initialised: boolean }> {
  return apiFetch('/api/v1/setup/status')
}

export function setupAccount(payload: SetupPayload): Promise<{ user: User }> {
  return apiFetch('/api/v1/setup', {
    method: 'POST',
    body: JSON.stringify(payload)
  })
}

export function login(
  username: string,
  password: string,
  totpCode?: string
): Promise<{ user: User }> {
  return apiFetch('/api/v1/auth/login', {
    method: 'POST',
    body: JSON.stringify({
      username,
      password,
      ...(totpCode ? { totp_code: totpCode } : {})
    })
  })
}

export function me(): Promise<{ user: User }> {
  return apiFetch('/api/v1/auth/me')
}

export function logout(): Promise<null> {
  return apiFetch('/api/v1/auth/logout', { method: 'POST' })
}

export function refresh(): Promise<null> {
  return apiFetch('/api/v1/auth/refresh', { method: 'POST' })
}
