use keeppix_db::{AssetRepo, Db, FolderRepo};
use keeppix_domain::{AssetId, AssetStatus};

use crate::JobError;

/// Stesso hash e size su un altro asset **il cui file non è più sul disco**:
/// è uno spostamento. I duplicati (entrambi i file presenti) restano due
/// asset. Si trasferiscono gli EXIF e si marca il vecchio `offline`.
/// Rating/album non esistono in 1b.
///
/// # Errors
/// Database.
pub async fn after_hash(
    db: &Db,
    new_id: AssetId,
    hash: [u8; 32],
    size: i64,
) -> Result<(), JobError> {
    let assets = AssetRepo::new(db);
    let folders = FolderRepo::new(db);
    for id in assets.ids_with_hash(&hash).await? {
        if id == new_id {
            continue;
        }
        let old = assets.get_for_scan(id).await?;
        if old.size_bytes != size || old.status == AssetStatus::Offline {
            continue;
        }
        let path = folders
            .absolute_path_for_scan(old.folder_id)
            .await?
            .join(old.filename.as_str());
        if path.is_file() {
            continue;
        }
        tracing::info!(from = %old.id, to = %new_id, "rilevato spostamento");
        assets.copy_exif(old.id, new_id).await?;
        assets.mark_offline(old.id).await?;
    }
    Ok(())
}
