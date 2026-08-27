//! `JobKind::DeriveRaw`: embedded preview first, demosaic only if needed.
//!
//! Cascade (XMP sidecar handling excluded):
//! 1. `extract_embedded_preview`: nearly free (1-6 ms), zero demosaic.
//! 2. If the preview's long side is ≥1440 px, it's the final preview. Done.
//! 3. Otherwise (small, missing, or a file not decodable as a recognized
//!    RAW) half-size demosaic is attempted in a sandbox.
//! 4. If that also fails, the asset goes into error state: it does not
//!    block the queue.
//!
//! The demosaic step is injected through [`Demosaic`] precisely to make it
//! verifiable — by counting calls, not timing — that step 2 really avoids
//! libraw when the preview is good enough.

use std::path::Path;

use keeppix_db::{AssetRepo, Db, FolderRepo};
use keeppix_media::{
    DeriveResult, PreviewSource, RawPreview, derivative_paths, derive_from_bytes, derive_from_rgb,
    extract_embedded_preview,
};

use crate::JobError;

/// Minimum long side, in pixels, for the embedded preview to be sufficient
/// on its own. Stays at 1440, not 2048: raising it would force demosaic on
/// fixtures (and on machines) whose embedded JPEG sits between 1440 and
/// 2047, which is already usable.
const MIN_PREVIEW_LONG_SIDE: u32 = 1440;

/// `dcraw_emu` runs on ARM in ~1.5-4 s on a real RAW file; 30 s of CPU is
/// generous but finite. The memory ceiling is 1 GiB (not 512): the same bug
/// found on distro ffmpeg — a too-low `RLIMIT_AS` fails to map the shared
/// libraries before ever touching the Bayer data — and `dcraw_emu` (libraw
/// + lcms/jpeg) has the same binary profile. The floor hasn't been
/// remeasured byte-for-byte on every host; 1 GiB aligns with the
/// `video::MEM` ceiling.
const DEMOSAIC_MEMORY_BYTES: u64 = 1024 * 1024 * 1024;
const DEMOSAIC_CPU_SECS: u64 = 30;

/// Injection point for demosaic. In production it launches `dcraw_emu` in a
/// sandbox; tests substitute it with a counter that never spawns an
/// external process.
pub trait Demosaic: Send + Sync {
    /// # Errors
    /// The RAW file isn't demosaicable: corrupt file, unsupported format,
    /// or sandbox timeout.
    fn demosaic(&self, path: &Path) -> Result<RawPreview, JobError>;
}

/// Production implementation: `dcraw_emu` in a sandbox, half-size, camera
/// white balance. Never in-process: see `keeppix_media::raw::demosaic_half`.
pub struct SandboxDemosaic;

impl Demosaic for SandboxDemosaic {
    fn demosaic(&self, path: &Path) -> Result<RawPreview, JobError> {
        keeppix_media::demosaic_half(path, DEMOSAIC_MEMORY_BYTES, DEMOSAIC_CPU_SECS)
            .map_err(|e| JobError::Worker(e.to_string()))
    }
}

/// # Errors
/// No file for the hash, or database failure.
pub async fn run(db: &Db, data_dir: &Path, hash: [u8; 32]) -> Result<(), JobError> {
    run_with(db, data_dir, hash, &SandboxDemosaic).await
}

/// Same pipeline as [`run`], with the demosaic step injected. Tests call it
/// directly to count invocations without ever touching libraw.
///
/// # Errors
/// No file for the hash, or database failure. A RAW file that produces no
/// derivatives (unusable preview *and* failed demosaic) is not a job error:
/// the assets with that hash go to `error` and the function returns `Ok`,
/// because the next job in the queue still needs to run.
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

    // Idempotency: if the derivative already exists, there's nothing to
    // redo — most importantly, no demosaic, which is the only genuinely
    // expensive step here.
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
            if keeppix_media::openclip_xlmr::first_complete_model_dir().is_some() {
                crate::embed::enqueue_after_ingest(db).await?;
            }
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
