use std::time::Duration;

use chrono::{TimeDelta, Utc};
use keeppix_db::{AssetRepo, Db, FolderRepo, JobRepo, LibraryRepo, OperationsRepo};
use keeppix_domain::{
    AssetKind, AssetName, JobKind, JobPriority, LibraryId, LibraryStatus, NewAsset, OperationId,
};
use keeppix_media::{Freshness, WalkedFile, freshness, iter_entries};
use uuid::Uuid;

use crate::{JobError, PRODUCTION_BATCH_SIZE, PRODUCTION_STABILITY_WAIT, maintenance};

/// Scansiona l'albero. Non apre i file: solo `stat` e insert a lotti.
///
/// `settled_after`: età minima di `mtime` perché un file conti come fermo.
/// I file più recenti sono `InFlight`: non bloccano il ciclo; si accoda un
/// ricontrollo con `run_after = now() + PRODUCTION_STABILITY_WAIT`.
///
/// # Errors
/// `MassDisappearance` se è sparito più del 20% dei file noti; errori di I/O
/// o di database.
pub async fn run(db: &Db, library_id: LibraryId, settled_after: Duration) -> Result<(), JobError> {
    run_with_operation(db, library_id, settled_after, None).await
}

/// Come [`run`], ma segue un'operazione (Fase 10 Task 16): avanzamento e
/// annullamento (`operations`, letta dal poll del WebSocket) invece di una
/// scansione muta. La rinomina di massa della Fase 9 non esiste ancora nel
/// codice — questa è l'infrastruttura generica agganciata all'unico long-op
/// reale già presente, la scansione di libreria (vedi Ruling nel ledger).
///
/// **Ruling (Task 16): annullare a metà produce una riuscita parziale, non
/// un rollback.** Gli asset già scritti in questo giro restano; l'operazione
/// si chiude come `Cancelled` elencandoli, non tenta di disfarli.
///
/// # Errors
/// Come [`run`].
pub async fn run_with_operation(
    db: &Db,
    library_id: LibraryId,
    settled_after: Duration,
    operation_id: Option<OperationId>,
) -> Result<(), JobError> {
    let libraries = LibraryRepo::new(db);
    let library = libraries.load_for_scan(library_id).await?;
    let assets = AssetRepo::new(db);
    let jobs = JobRepo::new(db);
    let ops = OperationsRepo::new(db);

    let existing = assets.count_in_library(library_id).await?;
    if let Some(op_id) = operation_id {
        // Onesto, non inventato: una prima importazione non sa quanti file
        // troverà finché non li ha trovati, quindi resta `None` — il
        // frontend disegna un avanzamento indeterminato invece di un
        // rapporto falso (stessa scelta di `FailureReason::Unknown`).
        ops.set_total(op_id, (existing > 0).then_some(existing))
            .await?;
    }
    let root = &library.root_path;

    if !root.is_dir() {
        if existing > 0 {
            libraries
                .set_status_for_scan(library_id, LibraryStatus::Offline)
                .await?;
        }
        finish_done_if_tracked(&ops, operation_id).await?;
        return Ok(());
    }

    let mut batch: Vec<WalkedFile> = Vec::with_capacity(PRODUCTION_BATCH_SIZE);
    let mut present: i64 = 0;
    let mut had_inflight = false;
    let mut cancelled = false;

    for walked in iter_entries(root, &library.exclude_patterns) {
        match freshness(&walked.path, settled_after)
            .map_err(|e| JobError::Worker(format!("stat {}: {e}", walked.path.display())))?
        {
            Freshness::InFlight => {
                present = present.saturating_add(1);
                had_inflight = true;
            }
            Freshness::Settled(meta) => {
                present = present.saturating_add(1);
                let mut file = walked;
                file.size_bytes = i64::try_from(meta.len()).unwrap_or(file.size_bytes);
                if let Ok(mtime) = meta.modified() {
                    file.mtime = chrono::DateTime::<chrono::Utc>::from(mtime);
                }
                batch.push(file);
                if batch.len() >= PRODUCTION_BATCH_SIZE {
                    cancelled = flush_batch(db, library_id, &mut batch, operation_id).await?;
                    if cancelled {
                        break;
                    }
                }
            }
        }
    }

    if cancelled {
        finish_cancelled_if_tracked(&ops, operation_id).await?;
        return Ok(());
    }

    if existing > 0 && present == 0 {
        libraries
            .set_status_for_scan(library_id, LibraryStatus::Offline)
            .await?;
        finish_done_if_tracked(&ops, operation_id).await?;
        return Ok(());
    }

    if existing > 0 && present.saturating_mul(5) < existing.saturating_mul(4) {
        // Errore, non completamento: l'operazione resta `running` — la
        // riprova del job la riprenderà con lo stesso `operation_id`.
        return Err(JobError::MassDisappearance);
    }

    libraries
        .set_status_for_scan(library_id, LibraryStatus::Active)
        .await?;

    if flush_batch(db, library_id, &mut batch, operation_id).await? {
        finish_cancelled_if_tracked(&ops, operation_id).await?;
        return Ok(());
    }

    if had_inflight {
        let run_after = Utc::now()
            + TimeDelta::from_std(PRODUCTION_STABILITY_WAIT)
                .unwrap_or_else(|_| TimeDelta::seconds(5));
        let mut payload = serde_json::json!({ "library_id": library_id.to_string() });
        if let Some(op_id) = operation_id {
            payload["operation_id"] = serde_json::Value::String(op_id.to_string());
        }
        jobs.enqueue_after(
            JobKind::DiscoverLibrary,
            payload,
            JobPriority::Background,
            Some(&format!("discover-retry:{library_id}")),
            run_after,
        )
        .await?;
    }

    libraries.mark_scanned(library_id).await?;
    maintenance::schedule_vacuum_analyze(db).await?;
    // Con `had_inflight` c'è ancora lavoro in arrivo con questo stesso
    // `operation_id`: chiuderla `Done` ora mentirebbe sull'avanzamento.
    if !had_inflight {
        finish_done_if_tracked(&ops, operation_id).await?;
    }
    Ok(())
}

async fn finish_done_if_tracked(
    ops: &OperationsRepo<'_>,
    operation_id: Option<OperationId>,
) -> Result<(), JobError> {
    if let Some(op_id) = operation_id {
        ops.finish_done(op_id).await?;
    }
    Ok(())
}

async fn finish_cancelled_if_tracked(
    ops: &OperationsRepo<'_>,
    operation_id: Option<OperationId>,
) -> Result<(), JobError> {
    if let Some(op_id) = operation_id {
        ops.finish_cancelled(op_id).await?;
    }
    Ok(())
}

/// Scrive il lotto. Ritorna `true` se l'operazione tracciata è stata
/// annullata a metà lotto: il chiamante deve fermarsi subito, senza
/// proseguire con altri file.
async fn flush_batch(
    db: &Db,
    library_id: LibraryId,
    batch: &mut Vec<WalkedFile>,
    operation_id: Option<OperationId>,
) -> Result<bool, JobError> {
    if batch.is_empty() {
        return Ok(false);
    }
    let assets = AssetRepo::new(db);
    let folders = FolderRepo::new(db);
    let jobs = JobRepo::new(db);
    let ops = OperationsRepo::new(db);
    for file in batch.drain(..) {
        if let Some(op_id) = operation_id
            && ops.is_cancel_requested(op_id).await?
        {
            return Ok(true);
        }
        let relative: Vec<&str> = file.relative_dir.iter().map(String::as_str).collect();
        let folder = folders.ensure_path(library_id, &relative).await?;
        let filename = AssetName::parse(&file.filename)
            .map_err(|e| JobError::Worker(format!("filename: {e}")))?;
        let Some(asset) = assets
            .upsert_discovered(NewAsset {
                folder_id: folder.id,
                filename,
                size_bytes: file.size_bytes,
                mtime: file.mtime,
                inode: file.inode,
                kind: AssetKind::Unknown,
            })
            .await?
        else {
            continue;
        };
        jobs.enqueue(
            JobKind::ExtractMetadata,
            serde_json::json!({ "asset_id": asset.id.to_string() }),
            JobPriority::Background,
            Some(&format!("meta:{}", asset.id)),
        )
        .await?;
        if let Some(op_id) = operation_id {
            ops.record_success(op_id, asset.id).await?;
        }
    }
    Ok(false)
}

/// # Errors
/// `JobError::Worker` se manca `library_id` o non è un UUID.
pub fn library_id_from_payload(payload: &serde_json::Value) -> Result<LibraryId, JobError> {
    let raw = payload
        .get("library_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("payload.library_id missing".to_owned()))?;
    let uuid =
        Uuid::parse_str(raw).map_err(|e| JobError::Worker(format!("payload.library_id: {e}")))?;
    Ok(LibraryId::from_uuid(uuid))
}

/// `None` quando il payload non porta un `operation_id` — il caso normale
/// per una riscansione partita dal watcher, senza un utente in attesa.
#[must_use]
pub fn operation_id_from_payload(payload: &serde_json::Value) -> Option<OperationId> {
    payload
        .get("operation_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .map(OperationId::from_uuid)
}
