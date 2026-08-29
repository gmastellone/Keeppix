import { apiFetch } from './client'

// "Active sessions" — `crates/keeppix-api/src/routes/sessions.rs`, real
// and complete: list, single revoke, revoke-all-others. `device_label`
// comes from the user-agent at login time
// (`device_label_from_user_agent`, always in English — "Chrome on
// macOS" — no server-side localization of a value derived from the
// client's user-agent).
export interface SessionView {
  id: string
  device_label: string | null
  last_seen_at: string
  current: boolean
}

export function fetchSessions(): Promise<SessionView[]> {
  return apiFetch('/api/v1/users/me/sessions')
}

/** "Sign out" on a non-current row. The only session **not** revocable
 * here is the current one — no button on that row at all, not a
 * disabled button (`routes/sessions.rs::revoke` still responds `400` if
 * you try). */
export function revokeSession(id: string): Promise<null> {
  return apiFetch(`/api/v1/users/me/sessions/${id}`, { method: 'DELETE' })
}

/** "Sign out of all other devices". */
export function revokeOtherSessions(): Promise<null> {
  return apiFetch('/api/v1/users/me/sessions/revoke-others', { method: 'POST' })
}
