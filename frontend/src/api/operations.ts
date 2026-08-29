import { apiFetch } from './client'
import type { BulkOutcome } from './culling'

/** Cancels a long-running operation (`LibraryScan`, `AiAnalysis`,
 * `FaceDetection`, `BulkRename`) already in progress.
 * `outcome.succeeded` is whatever was already applied at the time of the
 * request — cancelling midway produces a partial success, not a
 * rollback (`crates/keeppix-api/src/routes/operations.rs`). */
export function cancelOperation(operationId: string): Promise<BulkOutcome> {
  return apiFetch(`/api/v1/operations/${operationId}/cancel`, { method: 'POST' })
}
