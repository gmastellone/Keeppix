//! Annullamento delle operazioni lunghe (Fase 10 Task 16). `operations` è la
//! fonte di verità già letta dal poll del WebSocket (`ws::drain_operations`);
//! questa rotta si limita a chiedere l'annullamento e a restituire l'esito
//! parziale come `BulkOutcome`, senza inventare un secondo canale di stato.

use axum::extract::State;
use keeppix_db::OperationsRepo;
use keeppix_domain::OperationId;

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

/// **Ruling (Task 16): annullare a metà produce una riuscita parziale, non
/// un rollback.** La risposta elenca in `succeeded` esattamente ciò che è
/// già stato applicato al momento della richiesta — il worker può ancora
/// scrivere qualche elemento in più prima di notare la richiesta di
/// annullamento (la controlla fra un elemento e il successivo, non a
/// grana più fine), ma non disfa nulla di ciò che ha già scritto.
///
/// # Errors
/// `401` senza sessione; `403` — non `404` — se l'operazione non è visibile
/// al chiamante (non owner, non admin), altrimenti l'id diventerebbe un
/// oracolo di esistenza.
#[utoipa::path(
    post,
    path = "/api/v1/operations/{id}/cancel",
    tag = "operations",
    operation_id = "operations_cancel",
    summary = "Cancel a long-running operation",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'operazione")),
    responses(
        (status = 200, description = "Esito parziale al momento dell'annullamento", body = BulkOutcome),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Non consentito", body = Problem)
    )
)]
pub async fn cancel(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    axum::extract::Path(id): axum::extract::Path<OperationId>,
) -> Result<Json<BulkOutcome>, Problem> {
    let ops = OperationsRepo::new(&state.db);
    ops.request_cancel(&ctx, id).await?;
    let operation = ops.find(&ctx, id).await?;
    Ok(Json(BulkOutcome::from_partition(
        operation.succeeded,
        &[],
        None,
    )))
}
