//! `JobKind::WriteSidecar`: riversa sui file gli override in sospeso
//! (spec `fase-2-raw-culling.md` §3.3-§3.4). «DB prima, file poi»: l'utente
//! ha già visto il cambiamento (l'`UPDATE` su `asset_overrides` è
//! sincrono), questo job è solo la propagazione — asincrona, a priorità 3,
//! ritentabile — verso il sidecar `.xmp` accanto al file.
//!
//! A differenza di `DeriveRaw` (un job per asset, nel payload), questo job
//! non porta nessun asset nel payload: ad ogni esecuzione rilegge
//! `OverrideRepo::pending_sidecars` e processa un batch. Così un
//! `apply_batch` su 500 asset produce **un** job (l'accodamento è deduplicato
//! da `keeppix-db`), non 500.

use std::path::{Path, PathBuf};

use keeppix_db::{AssetRepo, Db, DbError, FolderRepo, JobRepo, OverrideRepo};
use keeppix_domain::{AssetId, JobKind, JobPriority, Pick, Rating};
use keeppix_media::SidecarData;

use crate::JobError;

/// Asset processati per esecuzione. Abbastanza per svuotare in fretta una
/// libreria piccola; su una più grande il job si ri-accoda da solo (vedi
/// [`run`]) invece di tenere il worker occupato per minuti in un colpo solo.
const BATCH_LIMIT: i64 = 200;

/// # Errors
/// Database, o uno o più sidecar non scrivibili (cartella in sola lettura,
/// disco pieno, RAW il cui file è sparito, ...). In quel caso il job
/// fallisce con retry — gli asset scritti con successo restano segnati
/// come tali, quindi un ritentativo processa solo quelli rimasti indietro,
/// non l'intero batch da capo.
pub async fn run(db: &Db) -> Result<(), JobError> {
    let pending = OverrideRepo::new(db).pending_sidecars(BATCH_LIMIT).await?;
    if pending.is_empty() {
        return Ok(());
    }

    let mut failures = Vec::new();
    for asset_id in &pending {
        if let Err(e) = write_one(db, *asset_id).await {
            failures.push(format!("{asset_id}: {e}"));
        }
    }

    let pending_count = i64::try_from(pending.len()).unwrap_or(i64::MAX);
    if pending_count >= BATCH_LIMIT {
        // Il batch era pieno: potrebbe esserci ancora lavoro. Ri-accodarsi
        // invece di continuare qui tiene ogni esecuzione limitata nel tempo
        // anche su una libreria con migliaia di override in sospeso.
        JobRepo::new(db)
            .enqueue(
                JobKind::WriteSidecar,
                serde_json::json!({}),
                JobPriority::Background,
                Some("write_sidecar"),
            )
            .await?;
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(JobError::Worker(failures.join("; ")))
    }
}

async fn write_one(db: &Db, asset_id: AssetId) -> Result<(), JobError> {
    let assets = AssetRepo::new(db);
    let asset = match assets.get_for_scan(asset_id).await {
        Ok(asset) => asset,
        // Cancellato fra l'accodamento del job e la sua esecuzione: la sua
        // riga di override è già sparita con lui (ON DELETE CASCADE su
        // asset_overrides), quindi non ricomparirà nel prossimo giro.
        Err(DbError::NotFound) => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let folder_path = FolderRepo::new(db)
        .absolute_path_for_scan(asset.folder_id)
        .await?;
    let sidecar_path = sidecar_path_for(&folder_path.join(asset.filename.as_str()));

    let overrides = OverrideRepo::new(db);
    let source = overrides.sidecar_source(asset_id).await?;
    let data = SidecarData {
        rating: source.owner_rating.map(Rating::value),
        description: source.effective.description,
        title: source.effective.title,
        tags: Vec::new(),
        gps: source.effective.location,
        taken_at: source.effective.taken_at,
        label: pick_label(source.owner_pick),
    };

    keeppix_media::write_sidecar(&sidecar_path, &data).map_err(|e| map_sidecar_error(&e))?;
    overrides.mark_sidecar_written(asset_id).await?;
    Ok(())
}

/// `permission-denied: ` è un marcatore stabile, non testo libero: è da lì
/// che `keeppix_db::ProblemsRepo::composed` (Fase 10 Task 13) riconosce la
/// natura "sidecar XMP non scrivibile" in `jobs.last_error` senza dover
/// interpretare il messaggio di un `io::Error` che potrebbe cambiare
/// formulazione a seconda della piattaforma.
fn map_sidecar_error(e: &keeppix_media::XmpError) -> JobError {
    if let keeppix_media::XmpError::Io(io_err) = e
        && io_err.kind() == std::io::ErrorKind::PermissionDenied
    {
        return JobError::Worker(format!("permission-denied: {e}"));
    }
    JobError::Worker(e.to_string())
}

/// `IMG_1234.ARW` → `IMG_1234.ARW.xmp`, accanto al file (spec §3.4).
fn sidecar_path_for(asset_path: &Path) -> PathBuf {
    let mut name = asset_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    name.push_str(".xmp");
    asset_path.with_file_name(name)
}

/// `xmp:Label` porta il pick/reject solo se l'utente ha votato: `Pick::None`
/// non scrive nulla, così un asset mai votato non finisce con un'etichetta
/// vuota nel sidecar.
fn pick_label(pick: Pick) -> Option<String> {
    (pick != Pick::None).then(|| pick.as_str().to_owned())
}
