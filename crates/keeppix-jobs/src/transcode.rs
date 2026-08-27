//! On-demand HLS transcode jobs.

use std::path::Path;

use keeppix_db::{AssetRepo, Db, FolderRepo, SettingsRepo};
use keeppix_domain::{AssetId, JobKind, JobPriority};
use keeppix_media::{
    PlaybackMode, VideoBackend, cache_is_ready, ensure_poster, extract_animated_preview,
    playback_mode, probe_stream, transcode_cache_dir, transcode_to_hls, video_poster_path,
    video_preview_anim_path,
};
use serde_json::json;
use uuid::Uuid;

use crate::JobError;

/// # Errors
/// Database or worker failure.
pub async fn run(
    db: &Db,
    data_dir: &Path,
    asset_id: AssetId,
    save_bandwidth: bool,
) -> Result<(), JobError> {
    let assets = AssetRepo::new(db);
    let asset = assets.get_for_scan(asset_id).await?;
    let folder_path = FolderRepo::new(db)
        .absolute_path_for_scan(asset.folder_id)
        .await?;
    let src = folder_path.join(asset.filename.as_str());
    if !src.is_file() {
        return Err(JobError::Worker("video source missing".to_owned()));
    }
    let hash = asset
        .content_hash
        .ok_or_else(|| JobError::Worker("video not hashed yet".to_owned()))?;
    let info = probe_stream(&src).map_err(|e| JobError::Worker(e.to_string()))?;
    let mode = playback_mode(&info, save_bandwidth);
    let profile = match mode {
        PlaybackMode::DirectPlay => return Ok(()),
        PlaybackMode::Transcode(profile) => profile,
    };
    let cache_dir = transcode_cache_dir(data_dir, &hash, profile);
    if cache_is_ready(&cache_dir) {
        return Ok(());
    }
    let backend = load_video_backend(db).await?;
    transcode_to_hls(&src, &cache_dir, profile, backend)
        .map_err(|e| JobError::Worker(e.to_string()))?;
    let poster = video_poster_path(data_dir, &hash);
    ensure_poster(&src, &poster).map_err(|e| JobError::Worker(e.to_string()))?;
    let preview = video_preview_anim_path(data_dir, &hash);
    extract_animated_preview(&src, &preview).map_err(|e| JobError::Worker(e.to_string()))?;
    Ok(())
}

async fn load_video_backend(db: &Db) -> Result<VideoBackend, JobError> {
    let value = SettingsRepo::new(db)
        .get_json("capabilities")
        .await?
        .unwrap_or_else(|| json!({}));
    let backend = value
        .get("backend")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("software");
    match backend {
        "rkmpp" => Ok(VideoBackend::Rkmpp),
        "nvenc" => Ok(VideoBackend::Nvenc),
        "v4l2m2m" => Ok(VideoBackend::V4l2m2m),
        "videotoolbox" => Ok(VideoBackend::Videotoolbox),
        "vaapi" => Ok(VideoBackend::Vaapi),
        "qsv" => Ok(VideoBackend::Qsv),
        "amf" => Ok(VideoBackend::Amf),
        _ => Ok(VideoBackend::Software),
    }
}

/// Enqueue on-demand transcoding at interactive priority.
///
/// # Errors
/// Database.
pub async fn enqueue(db: &Db, asset_id: AssetId, save_bandwidth: bool) -> Result<(), JobError> {
    keeppix_db::JobRepo::new(db)
        .enqueue(
            JobKind::TranscodeVideo,
            json!({
                "asset_id": asset_id.to_string(),
                "save_bandwidth": save_bandwidth,
            }),
            JobPriority::Interactive,
            Some(&format!(
                "transcode:{asset_id}:{}",
                u8::from(save_bandwidth)
            )),
        )
        .await?;
    Ok(())
}

/// # Errors
/// `JobError::Worker` if `asset_id` is missing or not a UUID.
pub fn asset_id_from_payload(payload: &serde_json::Value) -> Result<AssetId, JobError> {
    let raw = payload
        .get("asset_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Worker("payload.asset_id missing".to_owned()))?;
    let uuid =
        Uuid::parse_str(raw).map_err(|e| JobError::Worker(format!("payload.asset_id: {e}")))?;
    Ok(AssetId::from_uuid(uuid))
}

/// # Errors
/// `JobError::Worker` if `save_bandwidth` is missing.
pub fn save_bandwidth_from_payload(payload: &serde_json::Value) -> Result<bool, JobError> {
    payload
        .get("save_bandwidth")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| JobError::Worker("payload.save_bandwidth missing".to_owned()))
}
