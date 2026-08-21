//! `JobKind::DeriveRaw`: preview incorporata prima, demosaic solo se serve.
//!
//! Cascata (spec `fase-2-raw-culling.md` §2.1, sidecar XMP escluso — Task 5):
//! 1. `extract_embedded_preview`: quasi gratis (1-6 ms), zero demosaic.
//! 2. Se la preview ha il lato lungo ≥1440 px, è la preview finale. Fine.
//! 3. Altrimenti (piccola, assente, o file non decodificabile come RAW
//!    riconosciuto) si tenta il demosaic half-size in sandbox.
//! 4. Se anche quello fallisce, l'asset va in errore: non blocca la coda.
//!
//! Il demosaic è iniettato tramite [`Demosaic`] proprio per rendere
//! verificabile — contando le chiamate, non misurando i tempi — che il passo
//! 2 evita davvero libraw quando la preview basta.

use std::path::Path;

use keeppix_db::{AssetRepo, Db, FolderRepo};
use keeppix_media::{
    DeriveResult, PreviewSource, RawPreview, derivative_paths, derive_from_bytes, derive_from_rgb,
    extract_embedded_preview,
};

use crate::JobError;

/// Lato lungo minimo, in pixel, perché la preview incorporata basti da sola.
/// Resta 1440, non 2048: alzarlo forzerebbero il demosaic su fixture (e su
/// macchine) la cui JPEG incorporata sta fra 1440 e 2047, che è già usabile.
const MIN_PREVIEW_LONG_SIDE: u32 = 1440;

/// `dcraw_emu` gira su ARM in ~1,5-4 s su un RAW reale (spec §2.1); 30 s di
/// CPU è generoso ma finito. Il tetto di memoria è 1 GiB (non 512): lo stesso
/// bug trovato su ffmpeg distro — `RLIMIT_AS` troppo basso fallisce il
/// mapping delle shared libs prima di toccare il Bayer — e `dcraw_emu`
/// (libraw + lcms/jpeg) è lo stesso profilo di binario. Floor non rimisurato
/// byte-a-byte su ogni host; 1 GiB allinea al tetto di `video::MEM`.
const DEMOSAIC_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;
const DEMOSAIC_CPU_SECS: u64 = 30;

/// Punto di iniezione del demosaic. In produzione avvia `dcraw_emu` in
/// sandbox; nei test si sostituisce con un contatore che non avvia mai un
/// processo esterno.
pub trait Demosaic: Send + Sync {
    /// # Errors
    /// Il RAW non è demosaicabile: file corrotto, formato non supportato, o
    /// timeout della sandbox.
    fn demosaic(&self, path: &Path) -> Result<RawPreview, JobError>;
}

/// Implementazione di produzione: `dcraw_emu` in sandbox, half-size, WB
/// camera. Mai in-process: vedi `keeppix_media::raw::demosaic_half`.
pub struct SandboxDemosaic;

impl Demosaic for SandboxDemosaic {
    fn demosaic(&self, path: &Path) -> Result<RawPreview, JobError> {
        keeppix_media::demosaic_half(path, DEMOSAIC_MEMORY_BYTES, DEMOSAIC_CPU_SECS)
            .map_err(|e| JobError::Worker(e.to_string()))
    }
}

/// # Errors
/// File assente per l'hash, o database.
pub async fn run(db: &Db, data_dir: &Path, hash: [u8; 32]) -> Result<(), JobError> {
    run_with(db, data_dir, hash, &SandboxDemosaic).await
}

/// Stessa pipeline di [`run`], col demosaic iniettato. I test la chiamano
/// direttamente per contare le invocazioni senza mai toccare libraw.
///
/// # Errors
/// File assente per l'hash, o database. Un RAW che non produce derivati
/// (preview inutilizzabile *e* demosaic fallito) non è un errore di job: gli
/// asset con quell'hash vanno in `error` e la funzione ritorna `Ok`, perché
/// il job successivo in coda deve comunque partire.
pub async fn run_with(
    db: &Db,
    data_dir: &Path,
    hash: [u8; 32],
    demosaic: &dyn Demosaic,
) -> Result<(), JobError> {
    let assets = AssetRepo::new(db);
    let ids = assets.ids_with_hash(&hash).await?;
    let mut src = None;
    for id in &ids {
        let asset = assets.get_for_scan(*id).await?;
        let path = FolderRepo::new(db)
            .absolute_path_for_scan(asset.folder_id)
            .await?
            .join(asset.filename.as_str());
        if path.is_file() {
            src = Some(path);
            break;
        }
    }
    let Some(src) = src else {
        return Err(JobError::Worker("no file for content hash".to_owned()));
    };

    // Idempotenza: se il derivato esiste già, niente da rifare — soprattutto
    // niente demosaic, che è l'unico passo davvero costoso qui.
    let (thumb_path, _) = derivative_paths(data_dir, &hash);
    if thumb_path.is_file() {
        assets.propagate_thumbhash_for_hash(&hash).await?;
        return Ok(());
    }

    match derive_raw(&src, data_dir, &hash, demosaic) {
        Ok(result) if !result.skipped => {
            assets
                .set_thumbhash_for_hash(&hash, &result.thumbhash)
                .await?;
            crate::embed::enqueue_after_ingest(db).await?;
        }
        Ok(_) => {}
        Err(detail) => {
            for id in ids {
                assets.set_error(id, &detail).await?;
            }
        }
    }
    Ok(())
}

fn derive_raw(
    src: &Path,
    data_dir: &Path,
    hash: &[u8; 32],
    demosaic: &dyn Demosaic,
) -> Result<DeriveResult, String> {
    let preview = extract_embedded_preview(src).ok().flatten();
    let chosen = match preview {
        Some(p) if p.width.max(p.height) >= MIN_PREVIEW_LONG_SIDE => p,
        _ => demosaic.demosaic(src).map_err(|e| e.to_string())?,
    };

    match chosen.source {
        PreviewSource::Embedded => {
            derive_from_bytes(&chosen.bytes, data_dir, hash).map_err(|e| e.to_string())
        }
        PreviewSource::Demosaic => {
            derive_from_rgb(&chosen.bytes, chosen.width, chosen.height, data_dir, hash)
                .map_err(|e| e.to_string())
        }
    }
}
