import { apiFetch } from './client'
import type { BulkOutcome } from './culling'

/** `Task 16` (Fase 10): annullare un'operazione lunga (`LibraryScan`,
 * `AiAnalysis`, `FaceDetection`, `BulkRename`) già in corso.
 * `outcome.succeeded` è ciò che è già stato applicato al momento della
 * richiesta — un annullamento a metà produce una riuscita parziale, non
 * un rollback (`crates/keeppix-api/src/routes/operations.rs`). */
export function cancelOperation(operationId: string): Promise<BulkOutcome> {
  return apiFetch(`/api/v1/operations/${operationId}/cancel`, { method: 'POST' })
}
