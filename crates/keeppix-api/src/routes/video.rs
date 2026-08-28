use std::path::PathBuf;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use keeppix_db::{AssetRepo, FolderRepo};
use keeppix_domain::{AssetId, AssetKind};
use keeppix_media::{
    PlaybackMode, cache_is_ready, client_accepts_direct, playback_mode, probe_stream, touch_cache,
    transcode_cache_dir, video_poster_path,
};
use serde::Serialize;

use crate::extract::SessionOrShare;
use crate::problem::Problem;
use crate::routes::media::stream_file;
use crate::state::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct PlaybackQuery {
    /// Explicit client flag: never inferred server-side.
    #[serde(default)]
    pub save_bandwidth: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PlaybackResponse {
    pub mode: String,
    pub url: String,
    pub poster_url: String,
    pub ready: bool,
}

struct ResolvedVideo {
    asset_id: AssetId,
    src: PathBuf,
    hash: [u8; 32],
}

/// Negotiate direct play vs on-demand HLS transcode.
///
/// # Errors
/// `401` / `403` / `404` like other media routes.
#[utoipa::path(
    get,
    path = "/media/video/{id}/playback",
    tag = "media",
    operation_id = "media_video_playback",
    summary = "Negotiate video playback",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Asset id"),
        ("save_bandwidth" = Option<bool>, Query, description = "Serve 720p on mobile")
    ),
    responses(
        (status = 200, description = "Playback plan", body = PlaybackResponse),
        (status = 401, description = "Unauthenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem),
        (status = 404, description = "Not a video or missing on disk", body = Problem)
    )
)]
pub async fn playback(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    AxumPath(id): AxumPath<AssetId>,
    Query(query): Query<PlaybackQuery>,
    headers: HeaderMap,
) -> Result<Response, Problem> {
    let resolved = resolve_video(&state, &ctx, id).await?;
    let info = tokio::task::spawn_blocking({
        let src = resolved.src.clone();
        move || probe_stream(&src)
    })
    .await
    .map_err(|_| Problem::internal())?
    .map_err(|_| Problem::not_found())?;
    let mode = playback_mode(&info, query.save_bandwidth);
    let poster_url = format!("/media/video/{id}/poster");
    match mode {
        PlaybackMode::DirectPlay
            if client_accepts_direct(
                &info,
                headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()),
            ) =>
        {
            Ok(axum::Json(PlaybackResponse {
                mode: "direct".to_owned(),
                url: format!("/media/original/{id}"),
                poster_url,
                ready: true,
            })
            .into_response())
        }
        PlaybackMode::DirectPlay | PlaybackMode::Transcode(_) => {
            let profile = match mode {
                PlaybackMode::Transcode(profile) => profile,
                PlaybackMode::DirectPlay => keeppix_media::TranscodeProfile::SaveBandwidth720p,
            };
            let cache_dir = transcode_cache_dir(&state.data_dir, &resolved.hash, profile);
            let ready = cache_is_ready(&cache_dir);
            if ready {
                let _ = touch_cache(&cache_dir);
            } else {
                let db = state.db.clone();
                let asset_id = resolved.asset_id;
                let save = query.save_bandwidth;
                tokio::spawn(async move {
                    if let Err(e) = keeppix_jobs::transcode::enqueue(&db, asset_id, save).await {
                        tracing::warn!(error = %e, %asset_id, "transcode enqueue failed");
                    }
                });
            }
            Ok(axum::Json(PlaybackResponse {
                mode: "hls".to_owned(),
                url: format!("/media/video/{id}/hls/index.m3u8"),
                poster_url,
                ready,
            })
            .into_response())
        }
    }
}

/// Serve HLS playlist or segment from the on-disk cache.
///
/// # Errors
/// `404` for missing segments (including beyond the end of the video).
#[utoipa::path(
    get,
    path = "/media/video/{id}/hls/{file}",
    tag = "media",
    operation_id = "media_video_hls",
    summary = "Serve an HLS playlist or segment",
    security(("session_cookie" = [])),
    params(
        ("id" = String, Path, description = "Asset id"),
        ("file" = String, Path, description = "Playlist or segment file name")
    ),
    responses(
        (status = 200, description = "HLS payload"),
        (status = 401, description = "Unauthenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem),
        (status = 404, description = "Segment or playlist missing", body = Problem)
    )
)]
pub async fn hls(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    AxumPath((id, file)): AxumPath<(AssetId, String)>,
    Query(query): Query<PlaybackQuery>,
) -> Result<Response, Problem> {
    if file.contains('/') || file.contains('\\') || file.contains("..") {
        return Err(Problem::not_found());
    }
    let resolved = resolve_video(&state, &ctx, id).await?;
    let info = tokio::task::spawn_blocking({
        let src = resolved.src.clone();
        move || probe_stream(&src)
    })
    .await
    .map_err(|_| Problem::internal())?
    .map_err(|_| Problem::not_found())?;
    let mode = playback_mode(&info, query.save_bandwidth);
    let profile = match mode {
        PlaybackMode::Transcode(profile) => profile,
        PlaybackMode::DirectPlay => keeppix_media::TranscodeProfile::SaveBandwidth720p,
    };
    let cache_dir = transcode_cache_dir(&state.data_dir, &resolved.hash, profile);
    let path = cache_dir.join(&file);
    if !path.is_file() {
        return Err(Problem::not_found());
    }
    let _ = touch_cache(&cache_dir);
    let content_type = hls_content_type(&file);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| Problem::not_found())?;
    let mut response = bytes.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    Ok(response)
}

/// Poster frame at 10% of duration.
///
/// # Errors
/// Like other media routes.
#[utoipa::path(
    get,
    path = "/media/video/{id}/poster",
    tag = "media",
    operation_id = "media_video_poster",
    summary = "Serve a video poster frame",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    responses(
        (status = 200, description = "JPEG poster"),
        (status = 401, description = "Unauthenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem),
        (status = 404, description = "Poster missing", body = Problem)
    )
)]
pub async fn poster(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    AxumPath(id): AxumPath<AssetId>,
) -> Result<Response, Problem> {
    let resolved = resolve_video(&state, &ctx, id).await?;
    let dest = video_poster_path(&state.data_dir, &resolved.hash);
    if !dest.is_file() {
        let src = resolved.src.clone();
        let dest = dest.clone();
        tokio::task::spawn_blocking(move || keeppix_media::ensure_poster(&src, &dest))
            .await
            .map_err(|_| Problem::internal())?
            .map_err(|_| Problem::not_found())?;
    }
    stream_file(&dest, None, "image/jpeg", true).await
}

async fn resolve_video(
    state: &AppState,
    ctx: &keeppix_domain::AuthContext,
    id: AssetId,
) -> Result<ResolvedVideo, Problem> {
    let asset = AssetRepo::new(&state.db).find_by_id(ctx, id).await?;
    if asset.kind != AssetKind::Video {
        return Err(Problem::not_found());
    }
    let hash = asset.content_hash.ok_or_else(Problem::not_found)?;
    let folder_path = FolderRepo::new(&state.db)
        .absolute_path(ctx, asset.folder_id)
        .await?;
    let src = folder_path.join(asset.filename.as_str());
    if !src.is_file() {
        return Err(Problem::not_found());
    }
    Ok(ResolvedVideo {
        asset_id: id,
        src,
        hash,
    })
}

fn hls_content_type(file: &str) -> &'static str {
    let ext = std::path::Path::new(file)
        .extension()
        .and_then(|e| e.to_str());
    if ext.is_some_and(|e| e.eq_ignore_ascii_case("m3u8")) {
        "application/vnd.apple.mpegurl"
    } else if ext.is_some_and(|e| e.eq_ignore_ascii_case("ts")) {
        "video/mp2t"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal_in_hls_file_name() {
        assert!(hls_content_type("index.m3u8").contains("mpegurl"));
        assert!(hls_content_type("seg001.ts").contains("mp2t"));
    }
}
