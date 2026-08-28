//! Cancellation of long-running operations. `operations` is the source of
//! truth already read by the WebSocket poll (`ws::drain_operations`); this
//! route only requests the cancellation and returns the partial outcome as
//! `BulkOutcome`, without inventing a second status channel.

use axum::extract::State;
use keeppix_db::OperationsRepo;
use keeppix_domain::OperationId;

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

/// **Cancelling midway produces a partial success, not a rollback.** The
/// response lists in `succeeded` exactly what has already been applied at
/// the time of the request — the worker may still write a few more
/// elements before noticing the cancellation request (it checks between
/// one element and the next, not at finer granularity), but it never
/// undoes anything it has already written.
///
/// # Errors
/// `401` without a session; `403` — not `404` — if the operation is not
/// visible to the caller (not owner, not admin), otherwise the id would
/// become an existence oracle.
#[utoipa::path(
    post,
    path = "/api/v1/operations/{id}/cancel",
    tag = "operations",
    operation_id = "operations_cancel",
    summary = "Cancel a long-running operation",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Operation id")),
    responses(
        (status = 200, description = "Partial outcome at the time of cancellation", body = BulkOutcome),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not allowed", body = Problem)
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
