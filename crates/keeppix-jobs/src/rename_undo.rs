//! `RenameUndo` in the background, moved off the HTTP request path — see
//! the comment at the top of `keeppix-db::rename`. The operation is already
//! created by the HTTP caller (`routes/rename.rs::undo_batch`), which has
//! also already verified batch ownership via `RenameRepo::assert_batch_owned`
//! before enqueueing this job — here only the `AuthContext` of the user who
//! started the request is reconstructed, so `RenameRepo::undo` runs with
//! the same permission it had at call time. Structurally identical to
//! `rename_batch::run`, just calling `undo` instead of `apply`.

use keeppix_db::{Db, OperationsRepo, RenameRepo, UserRepo};
use keeppix_domain::{AuthContext, BatchId, OperationId, UserId};
use uuid::Uuid;

use crate::JobError;

/// # Errors
/// `JobError::Worker` if the payload is malformed; `JobError::Db` if
/// `RenameRepo::undo` fails (the operation is still closed as failed
/// before the error propagates, never left `running`).
pub async fn run(db: &Db, payload: &serde_json::Value) -> Result<(), JobError> {
    let operation_id = operation_id_from_payload(payload)?;
    let actor_id = actor_id_from_payload(payload)?;
    let batch_id = batch_id_from_payload(payload)?;

    let ops = OperationsRepo::new(db);
    // **Current** role, not the one at the time the HTTP request enqueued
    // the job — consistent with `UserRepo::role_for` (same as `rename_batch`).
    let role = match UserRepo::new(db).role_for(actor_id).await {
        Ok(role) => role,
        Err(err) => {
            ops.finish_failed(operation_id).await?;
            return Err(JobError::Db(err));
        }
    };
    let ctx = AuthContext::user(actor_id, role);

    match RenameRepo::new(db)
        .undo(&ctx, batch_id, Some(operation_id))
        .await
    {
        Ok(_outcome) => Ok(()),
        Err(err) => {
            // `undo` closes the operation itself on every path that
            // reaches the restore pass (Done/Cancelled, including the
            // already-undone no-op) — an `Err` here means it failed
            // **before** that, on the ownership re-check under the row
            // lock (e.g. the batch was undone and somehow re-verified
            // differently between the HTTP check and this job — the same
            // rare-but-possible gap `rename_batch::run` documents for
            // `apply`), so it's closed here instead, otherwise it would
            // stay `running` forever.
            ops.finish_failed(operation_id).await?;
            Err(JobError::Db(err))
        }
    }
}

fn operation_id_from_payload(payload: &serde_json::Value) -> Result<OperationId, JobError> {
    let raw = payload
        .get("operation_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("payload.operation_id missing".to_owned()))?;
    let uuid =
        Uuid::parse_str(raw).map_err(|e| JobError::Worker(format!("payload.operation_id: {e}")))?;
    Ok(OperationId::from_uuid(uuid))
}

fn actor_id_from_payload(payload: &serde_json::Value) -> Result<UserId, JobError> {
    let raw = payload
        .get("actor_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("payload.actor_id missing".to_owned()))?;
    let uuid =
        Uuid::parse_str(raw).map_err(|e| JobError::Worker(format!("payload.actor_id: {e}")))?;
    Ok(UserId::from_uuid(uuid))
}

fn batch_id_from_payload(payload: &serde_json::Value) -> Result<BatchId, JobError> {
    let raw = payload
        .get("batch_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("payload.batch_id missing".to_owned()))?;
    let uuid =
        Uuid::parse_str(raw).map_err(|e| JobError::Worker(format!("payload.batch_id: {e}")))?;
    Ok(BatchId::from_uuid(uuid))
}
