import { ApiProblem } from '@/api/client'

/**
 * Fase 11 Task 5 (documento funzionale §68.9 + spec fase-10-api-
 * interfaccia.md §7): quattro nature di fallimento, più un ripiego onesto.
 * `§7` fissa questa stessa tassonomia lato backend per `BulkFailure.reason`
 * (le operazioni di massa) — qui è la stessa idea applicata al
 * caricamento di un'intera schermata, dove il backend non porta un
 * `reason` dedicato: la natura si deduce dallo `status`/`type` del
 * `Problem` (RFC 9457) o da un fallimento di rete puro, non da un campo
 * che non esiste per le richieste singole.
 */
export type ErrorNature = 'unreachable' | 'permission-denied' | 'file-missing' | 'timeout' | 'unknown'

/** "Riprova" ha senso solo per `unreachable` e `permission-denied`
 * (spec §7: gli altri due richiedono un'azione diversa — una scansione,
 * o frazionare la richiesta — non un nuovo tentativo identico). */
const RETRYABLE = new Set<ErrorNature>(['unreachable', 'permission-denied'])

export function canRetry(nature: ErrorNature): boolean {
  return RETRYABLE.has(nature)
}

/**
 * Deduce la natura da un errore reale, non da un campo che il backend non
 * invia per le richieste singole:
 * - `ApiProblem` con `type: 'service-unavailable'` (il `Problem` che
 *   `DbError::Connection` produce, `crates/keeppix-api/src/problem.rs`) →
 *   `unreachable`, la stessa natura testuale ("il server o la libreria di
 *   rete non risponde").
 * - `type: 'forbidden'` → `permission-denied`.
 * - `type: 'not-found'` → `file-missing`.
 * - un `TypeError` non è un `ApiProblem`: è `fetch()` stesso che non è
 *   riuscito a raggiungere la rete (DNS, offline, connessione rifiutata) —
 *   comportamento noto della Fetch API, non un'invenzione — anche questo
 *   `unreachable`.
 * - un `AbortError` (richiesta interrotta per tempo scaduto) → `timeout`.
 * - tutto il resto → `unknown`, onesto invece di forzare una delle
 *   quattro nature note (stesso principio del quinto valore `Unknown`
 *   lato backend).
 */
export function classifyError(error: unknown): ErrorNature {
  if (error instanceof ApiProblem) {
    switch (error.type) {
      case 'service-unavailable':
        return 'unreachable'
      case 'forbidden':
        return 'permission-denied'
      case 'not-found':
        return 'file-missing'
      default:
        return 'unknown'
    }
  }
  if (error instanceof DOMException && error.name === 'AbortError') return 'timeout'
  if (error instanceof TypeError) return 'unreachable'
  return 'unknown'
}
