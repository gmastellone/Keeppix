//! `BulkRename` in background (Task 10 + Task 16, spostato dalla richiesta
//! HTTP il 27 agosto — vedi il commento in cima a
//! `keeppix-db::rename`). L'operazione è già creata dal chiamante HTTP
//! (`routes/rename.rs::apply_batch`) prima di accodare questo job: qui si
//! ricostruisce solo l'`AuthContext` dell'utente che ha avviato la
//! richiesta, per far girare `RenameRepo::apply` con lo stesso permesso che
//! aveva al momento della chiamata.

use keeppix_db::{Db, OperationsRepo, RenameRepo, UserRepo};
use keeppix_domain::{AssetId, AuthContext, OperationId, UserId};
use uuid::Uuid;

use crate::JobError;

/// # Errors
/// `JobError::Worker` se il payload è malformato; `JobError::Db` se
/// `RenameRepo::apply` fallisce (l'operazione viene comunque chiusa come
/// fallita prima di propagare, mai lasciata `running`).
pub async fn run(db: &Db, payload: &serde_json::Value) -> Result<(), JobError> {
    let operation_id = operation_id_from_payload(payload)?;
    let actor_id = actor_id_from_payload(payload)?;
    let asset_ids = asset_ids_from_payload(payload)?;
    let schema = schema_from_payload(payload)?;

    let ops = OperationsRepo::new(db);
    // Ruolo **corrente**, non quello del momento in cui la richiesta HTTP
    // ha accodato il job — coerente con `UserRepo::role_for`.
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
            // `apply` chiude da sola l'operazione su ogni percorso che
            // raggiunge il giro di rinomina (Done/Cancelled) — un `Err` qui
            // vuol dire che ha fallito **prima**, dentro `compute` (permesso
            // cambiato nel frattempo, o un errore di connessione): quella
            // strada non tocca mai l'operazione, quindi lo facciamo qui,
            // altrimenti resterebbe `running` per sempre (lo stesso buco
            // documentato su `OperationsRepo::finish_failed`).
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
