//! Bulk formula-based rename: preview, apply, and undo on
//! [`keeppix_db::RenameRepo`]. `undo` stays synchronous within the request
//! (like `metadata::apply_batch`, `track_operation = true` — a known
//! limitation declared in `keeppix_db::rename`, not addressed here).
//!
//! **`apply_batch` is no longer synchronous**: it runs in the background
//! via `JobKind::BulkRename` (`keeppix-jobs::rename_batch`), the same shape
//! as `LibraryScan` — this route only does the fallible checks up front
//! (batch/permission/visibility), creates the `Operation`, and queues the
//! job, responding `202` with `operation_id` right away. The original
//! design was synchronous because it was "fast, no inference" — but a
//! batch of thousands of photos on slow storage was still a multi-minute
//! block with no way to cancel it, which is why it changed.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use keeppix_db::{AssetRepo, JobRepo, OperationsRepo, PermissionRepo, RenameRepo};
use keeppix_domain::{
    AssetId, BatchId, FolderId, JobKind, JobPriority, OperationId, OperationKind,
};
use serde::{Deserialize, Serialize};

use crate::bulk::BulkOutcome;
use crate::extract::Auth;
use crate::json::Json;
use crate::problem::Problem;
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RenameBatchRequest {
    #[schema(value_type = Vec<String>)]
    pub asset_ids: Vec<AssetId>,
    /// Formula with placeholders: `{date}`, `{camera}`, `{lens}`,
    /// `{place}`, `{title}`, `{n[:D]}`.
    pub schema: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenamePreviewItemView {
    #[schema(value_type = String)]
    pub asset_id: AssetId,
    #[schema(value_type = String)]
    pub folder_id: FolderId,
    pub current_name: String,
    pub new_name: String,
    pub collides: bool,
}

impl From<keeppix_db::RenamePreviewItem> for RenamePreviewItemView {
    fn from(item: keeppix_db::RenamePreviewItem) -> Self {
        Self {
            asset_id: item.asset_id,
            folder_id: item.folder_id,
            current_name: item.current_name,
            new_name: item.new_name,
            collides: item.collides,
        }
    }
}

/// Nested, not flattened onto `BulkOutcome` (`utoipa` limitation: a
/// `#[serde(flatten)]` on a generated schema loses the field names in the
/// `OpenAPI` document). Used only by `undo_batch`, which is still
/// synchronous — `apply_batch` responds with [`RenameAccepted`].
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenameOperationOutcome {
    #[schema(value_type = String)]
    pub operation_id: OperationId,
    pub outcome: BulkOutcome,
}

/// Response of `apply_batch` (`202`): the outcome is not yet known when
/// responding — the caller follows `operation_id` over the `WebSocket`
/// (`operation.progress`), the same pattern as `ScanAccepted`
/// (`routes/libraries.rs`).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenameAccepted {
    #[schema(value_type = String)]
    pub operation_id: OperationId,
}

/// # Errors
/// `400` if the batch exceeds [`crate::batch::MAX_BATCH_ASSETS`]; `401` if
/// not authenticated; `403` if even one asset is not visible or editable.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/rename/preview",
    tag = "rename",
    operation_id = "rename_preview",
    summary = "Preview a bulk rename",
    security(("session_cookie" = [])),
    request_body = RenameBatchRequest,
    responses(
        (status = 200, description = "Names computed, nothing written to disk or database", body = Vec<RenamePreviewItemView>),
        (status = 400, description = "Batch too large", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "An asset is not visible or not editable", body = Problem)
    )
)]
pub async fn preview(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<RenameBatchRequest>,
) -> Result<Json<Vec<RenamePreviewItemView>>, Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let items = RenameRepo::new(&state.db)
        .preview(&ctx, &body.asset_ids, &body.schema)
        .await?;
    Ok(Json(items.into_iter().map(Into::into).collect()))
}

// The fallible checks stay synchronous and up front, exactly as before
// when they lived inside RenameRepo::compute — just moved up one level,
// because now they are the only thing this request actually does: the
// real work (computing again, for real, with fresh names, not the ones
// from this moment) runs inside keeppix-jobs::rename_batch, not here.
/// # Errors
/// `400` if the batch exceeds [`crate::batch::MAX_BATCH_ASSETS`]; `401` if
/// not authenticated; `403` if even one asset is not visible or editable.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/rename",
    tag = "rename",
    operation_id = "rename_apply_batch",
    summary = "Start a bulk rename",
    security(("session_cookie" = [])),
    request_body = RenameBatchRequest,
    responses(
        (status = 202, description = "Queued — follow operation_id over WebSocket (operation.progress)", body = RenameAccepted),
        (status = 400, description = "Batch too large", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "An asset is not visible or not editable", body = Problem)
    )
)]
pub async fn apply_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Json(body): Json<RenameBatchRequest>,
) -> Result<(StatusCode, Json<RenameAccepted>), Problem> {
    crate::batch::reject_oversized_batch(&body.asset_ids)?;
    let actor_id = ctx.user_id().ok_or_else(Problem::unauthenticated)?;
    AssetRepo::new(&state.db)
        .assert_visible(&ctx, &body.asset_ids)
        .await?;
    PermissionRepo::new(&state.db)
        .assert_can_edit_assets(&ctx, &body.asset_ids)
        .await?;

    let operation = OperationsRepo::new(&state.db)
        .create(&ctx, OperationKind::BulkRename)
        .await?;

    JobRepo::new(&state.db)
        .enqueue(
            JobKind::BulkRename,
            serde_json::json!({
                "operation_id": operation.id.to_string(),
                "actor_id": actor_id.to_string(),
                "asset_ids": body.asset_ids.iter().map(AssetId::to_string).collect::<Vec<_>>(),
                "schema": body.schema,
            }),
            JobPriority::Background,
            None,
        )
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(RenameAccepted {
            operation_id: operation.id,
        }),
    ))
}

/// # Errors
/// `401` if not authenticated; `403` if the batch does not belong to the
/// caller (non-admin). A second undo on the same batch is a no-op, not an
/// error — the response comes back with an empty `outcome.succeeded`.
#[utoipa::path(
    post,
    path = "/api/v1/assets/batch/rename/{batch_id}/undo",
    tag = "rename",
    operation_id = "rename_undo_batch",
    summary = "Undo a bulk rename batch",
    security(("session_cookie" = [])),
    params(("batch_id" = String, Path, description = "Id of the batch returned by apply")),
    responses(
        (status = 200, description = "Per-asset outcome of the restore", body = RenameOperationOutcome),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Batch does not belong to the caller", body = Problem)
    )
)]
pub async fn undo_batch(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    Path(batch_id): Path<BatchId>,
) -> Result<Json<RenameOperationOutcome>, Problem> {
    let outcome = RenameRepo::new(&state.db)
        .undo(&ctx, batch_id, true)
        .await?;
    let operation_id = outcome
        .operation_id
        .ok_or_else(|| Problem::internal().with_detail("rename undo did not track an operation"))?;
    let succeeded = outcome.restored.iter().map(|asset| asset.id).collect();
    Ok(Json(RenameOperationOutcome {
        operation_id,
        outcome: BulkOutcome::from_partition(succeeded, &outcome.failed, None),
    }))
}
