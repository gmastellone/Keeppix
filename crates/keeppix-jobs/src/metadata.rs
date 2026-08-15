use keeppix_db::{AssetRepo, Db, FolderRepo, JobRepo};
use keeppix_domain::{AssetId, JobKind, JobPriority};
use keeppix_media::read_exif;
use uuid::Uuid;

use crate::JobError;

/// # Errors
/// I/O sul file, o database.
pub async fn run(db: &Db, asset_id: AssetId) -> Result<(), JobError> {
    let assets = AssetRepo::new(db);
    let asset = assets.get_for_scan(asset_id).await?;
    let folder_path = FolderRepo::new(db)
        .absolute_path_for_scan(asset.folder_id)
        .await?;
    let path = folder_path.join(asset.filename.as_str());
    let exif = read_exif(&path, asset.mtime).map_err(|e| JobError::Worker(e.to_string()))?;
    assets.insert_exif(asset_id, &exif).await?;
    assets
        .set_indexed(
            asset_id,
            exif.taken_at_utc,
            exif.width.unwrap_or(0),
            exif.height.unwrap_or(0),
        )
        .await?;
    JobRepo::new(db)
        .enqueue(
            JobKind::HashAsset,
            serde_json::json!({ "asset_id": asset_id.to_string() }),
            JobPriority::Background,
            Some(&format!("hash:{asset_id}")),
        )
        .await?;
    Ok(())
}

/// # Errors
/// `JobError::Worker` se manca `asset_id` o non è un UUID.
pub fn asset_id_from_payload(payload: &serde_json::Value) -> Result<AssetId, JobError> {
    let raw = payload
        .get("asset_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("payload.asset_id missing".to_owned()))?;
    let uuid =
        Uuid::parse_str(raw).map_err(|e| JobError::Worker(format!("payload.asset_id: {e}")))?;
    Ok(AssetId::from_uuid(uuid))
}
