use keeppix_db::{AssetRepo, Db, FolderRepo};
use keeppix_media::derive_jpeg;

use crate::JobError;

/// # Errors
/// File assente, decode, o database.
pub async fn run(db: &Db, data_dir: &std::path::Path, hash: [u8; 32]) -> Result<(), JobError> {
    let assets = AssetRepo::new(db);
    let ids = assets.ids_with_hash(&hash).await?;
    let mut src = None;
    for id in ids {
        let asset = assets.get_for_scan(id).await?;
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
    let result = derive_jpeg(&src, data_dir, &hash).map_err(|e| JobError::Worker(e.to_string()))?;
    if result.skipped {
        return Ok(());
    }
    assets
        .set_thumbhash_for_hash(&hash, &result.thumbhash)
        .await?;
    // Foto nuova (o ricalcolo derive): accoda un lotto AI ad alta priorità,
    // solo se i pesi CLIP sono presenti — altrimenti retry inutili in coda.
    if keeppix_media::openclip_xlmr::first_complete_model_dir().is_some() {
        crate::embed::enqueue_after_ingest(db).await?;
    }
    if keeppix_media::face::first_complete_model_dir().is_some() {
        crate::detect_faces::enqueue_after_ingest(db).await?;
    }
    Ok(())
}

/// # Errors
/// `JobError::Worker` se `content_hash` manca o non è hex da 32 byte.
pub fn hash_from_payload(payload: &serde_json::Value) -> Result<[u8; 32], JobError> {
    let hex = payload
        .get("content_hash")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("payload.content_hash missing".to_owned()))?;
    if hex.len() != 64 {
        return Err(JobError::Worker(
            "content_hash must be 64 hex chars".to_owned(),
        ));
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        let s = std::str::from_utf8(chunk).map_err(|e| JobError::Worker(e.to_string()))?;
        out[i] = u8::from_str_radix(s, 16).map_err(|e| JobError::Worker(e.to_string()))?;
    }
    Ok(out)
}
