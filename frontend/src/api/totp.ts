import { apiFetch } from './client'

export interface TotpStatus {
  enabled: boolean
  pending: boolean
  unused_recovery_codes: number
}

export interface TotpSetup {
  otpauth_uri: string
  secret_base32: string
  qr_svg: string
}

export function getTotpStatus(): Promise<TotpStatus> {
  return apiFetch('/api/v1/auth/totp')
}

export function beginTotpSetup(): Promise<TotpSetup> {
  return apiFetch('/api/v1/auth/totp/setup', { method: 'POST' })
}

export function confirmTotp(code: string): Promise<{ recovery_codes: string[] }> {
  return apiFetch('/api/v1/auth/totp/confirm', {
    method: 'POST',
    body: JSON.stringify({ code })
  })
}

export function regenerateTotpRecoveryCodes(
  code: string
): Promise<{ recovery_codes: string[] }> {
  return apiFetch('/api/v1/auth/totp/recovery-codes', {
    method: 'POST',
    body: JSON.stringify({ code })
  })
}

export function disableTotp(code: string): Promise<null> {
  return apiFetch('/api/v1/auth/totp', {
    method: 'DELETE',
    body: JSON.stringify({ code })
  })
}
