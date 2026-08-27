//! `BulkRename` in the background, moved off the HTTP request path — see
//! the comment at the top of `keeppix-db::rename`. The operation is already
//! created by the HTTP caller (`routes/rename.rs::apply_batch`) before this
//! job is enqueued: here only the `AuthContext` of the user who started the
//! request is reconstructed, so `RenameRepo::apply` runs with the same
//! permission it had at call time.

use keeppix_db::{Db, OperationsRepo, RenameRepo, UserRepo};
use keeppix_domain::{AssetId, AuthContext, OperationId, UserId};
use uuid::Uuid;

use crate::JobError;

/// # Errors
/// `JobError::Worker` if the payload is malformed; `JobError::Db` if
/// `RenameRepo::apply` fails (the operation is still closed as failed
/// before the error propagates, never left `running`).
pub async fn run(db: &Db, payload: &serde_json::Value) -> Result<(), JobError> {
    let operation_id = operation_id_from_payload(payload)?;
    let actor_id = actor_id_from_payload(payload)?;
    let asset_ids = asset_ids_from_payload(payload)?;
    let schema = schema_from_payload(payload)?;

    let ops = OperationsRepo::new(db);
    // **Current** role, not the one at the time the HTTP request enqueued
    // the job — consistent with `UserRepo::role_for`.
    let role = match UserRepo::new(db).role_for(actor_id).await {
        Ok(role) => role,
        Err(err) => {
            ops.finish_failed(operation_id).await?;
            return Err(JobError::Db(err));
        }
    };
    let ctx = AuthContext::user(actor_id, role);

    match RenameRepo::new(db)
        .apply(&ctx, &asset_ids, &schema, Some(operation_id))
        .await
    {
        Ok(_outcome) => Ok(()),
        Err(err) => {
            // `apply` closes the operation itself on every path that
            // reaches the rename pass (Done/Cancelled) — an `Err` here
            // means it failed **earlier**, inside `compute` (permission
            // changed in the meantime, or a connection error): that path
            // never touches the operation, so it's done here instead,
            // otherwise it would stay `running` forever (the same gap
            // documented on `OperationsRepo::finish_failed`).
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

fn asset_ids_from_payload(payload: &serde_json::Value) -> Result<Vec<AssetId>, JobError> {
    let raw = payload
        .get("asset_ids")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| JobError::Worker("payload.asset_ids missing".to_owned()))?;
    raw.iter()
        .map(|v| {
            let s = v
                .as_str()
                .ok_or_else(|| JobError::Worker("payload.asset_ids: not a string".to_owned()))?;
            let uuid = Uuid::parse_str(s)
                .map_err(|e| JobError::Worker(format!("payload.asset_ids: {e}")))?;
            Ok(AssetId::from_uuid(uuid))
        })
        .collect()
}

fn schema_from_payload(payload: &serde_json::Value) -> Result<String, JobError> {
    payload
        .get("schema")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| JobError::Worker("payload.schema missing".to_owned()))
}
