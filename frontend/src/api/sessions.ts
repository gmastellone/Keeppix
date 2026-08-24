import { apiFetch } from './client'

// Fase 11 Task 14 (2/N), §61.4 "Sessioni attive" — `crates/keeppix-api/src/
// routes/sessions.rs`, reale e completo: elenco, revoca singola, revoca di
// tutte le altre. A differenza del mockup ("né 'Esci' né 'Esci da tutti gli
// altri dispositivi' sono collegati a un gestore"), qui tutti e tre
// funzionano davvero. `device_label` viene dallo user-agent al login
// (`device_label_from_user_agent`, sempre in inglese — "Chrome on macOS",
// non "Chrome su macOS" del documento: nessuna localizzazione lato server di
// un valore derivato dallo user-agent del client).
export interface SessionView {
  id: string
  device_label: string | null
  last_seen_at: string
  current: boolean
}

export function fetchSessions(): Promise<SessionView[]> {
  return apiFetch('/api/v1/users/me/sessions')
}

/** "Esci" su una riga non corrente (§61, tabella controlli #14). L'unica
 * sessione **non** revocabile qui è quella corrente — niente pulsante su
 * quella riga, non un pulsante disabilitato (`routes/sessions.rs::revoke`
 * risponde comunque `400` se ci si prova). */
export function revokeSession(id: string): Promise<null> {
  return apiFetch(`/api/v1/users/me/sessions/${id}`, { method: 'DELETE' })
}

/** "Esci da tutti gli altri dispositivi" (§61, controllo #15). */
export function revokeOtherSessions(): Promise<null> {
  return apiFetch('/api/v1/users/me/sessions/revoke-others', { method: 'POST' })
}
