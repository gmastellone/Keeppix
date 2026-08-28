import { ApiProblem } from '@/api/client'

/**
 * Four failure natures, plus an honest fallback. The backend applies the
 * same taxonomy for `BulkFailure.reason` (bulk operations); here it's the
 * same idea applied to loading an entire screen, where the backend doesn't
 * carry a dedicated `reason` — the nature is inferred from the
 * `status`/`type` of the `Problem` (RFC 9457) response or from a plain
 * network failure, not from a field that doesn't exist for single
 * requests.
 */
export type ErrorNature = 'unreachable' | 'permission-denied' | 'file-missing' | 'timeout' | 'unknown'

/** "Retry" only makes sense for `unreachable` and `permission-denied` — the
 * other two require a different action (a rescan, or splitting up the
 * request), not an identical retry. */
const RETRYABLE = new Set<ErrorNature>(['unreachable', 'permission-denied'])

export function canRetry(nature: ErrorNature): boolean {
  return RETRYABLE.has(nature)
}

/**
 * Infers the nature from a real error, not from a field the backend
 * doesn't send for single requests:
 * - `ApiProblem` with `type: 'service-unavailable'` (the `Problem` that
 *   `DbError::Connection` produces) → `unreachable`, i.e. "the server or
 *   the network can't be reached".
 * - `type: 'forbidden'` → `permission-denied`.
 * - `type: 'not-found'` → `file-missing`.
 * - a `TypeError` isn't an `ApiProblem`: it's `fetch()` itself failing to
 *   reach the network (DNS, offline, connection refused) — known Fetch API
 *   behavior, not a guess — also treated as `unreachable`.
 * - an `AbortError` (request aborted due to timeout) → `timeout`.
 * - everything else → `unknown`, an honest fallback rather than forcing
 *   one of the four known natures (mirrors the backend's own `Unknown`
 *   fifth value).
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
