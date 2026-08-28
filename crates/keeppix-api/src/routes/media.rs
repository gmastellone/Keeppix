use std::io::SeekFrom;
use std::path::Path;

use axum::body::Body;
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use keeppix_db::{AssetRepo, FolderRepo};
use keeppix_domain::AssetId;
use keeppix_media::derivative_paths;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::extract::SessionOrShare;
use crate::problem::Problem;
use crate::state::AppState;

// Authenticated media: browser cache only. `public` waits for share URLs.
const IMMUTABLE: &str = "private, max-age=31536000, immutable";

/// # Errors
/// `401` if not authenticated; `403` if the hash is not visible or does not
/// exist.
#[utoipa::path(
    get,
    path = "/media/thumb/{hash}",
    tag = "media",
    operation_id = "media_thumb",
    summary = "Serve a thumbnail",
    security(("session_cookie" = [])),
    params(("hash" = String, Path, description = "blake3 hex, 64 characters")),
    responses(
        (status = 200, description = "WebP thumbnail"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Hash not visible", body = Problem),
        (status = 404, description = "Derivative missing on disk", body = Problem)
    )
)]
pub async fn thumb(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    AxumPath(hash): AxumPath<String>,
) -> Result<Response, Problem> {
    serve_derivative(&state, &ctx, &hash, true).await
}

/// # Errors
/// Same as `thumb`.
#[utoipa::path(
    get,
    path = "/media/preview/{hash}",
    tag = "media",
    operation_id = "media_preview",
    summary = "Serve a preview derivative",
    security(("session_cookie" = [])),
    params(("hash" = String, Path, description = "blake3 hex, 64 characters")),
    responses(
        (status = 200, description = "WebP preview"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Hash not visible", body = Problem),
        (status = 404, description = "Derivative missing on disk", body = Problem)
    )
)]
pub async fn preview(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    AxumPath(hash): AxumPath<String>,
) -> Result<Response, Problem> {
    serve_derivative(&state, &ctx, &hash, false).await
}

/// # Errors
/// Same as `thumb`. Generates the `full` tier on first request.
#[utoipa::path(
    get,
    path = "/media/full/{hash}",
    tag = "media",
    operation_id = "media_full",
    summary = "Serve a full-size derivative",
    security(("session_cookie" = [])),
    params(("hash" = String, Path, description = "blake3 hex, 64 characters")),
    responses(
        (status = 200, description = "Full-size WebP at native resolution"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Hash not visible", body = Problem),
        (status = 404, description = "Original missing or not derivable", body = Problem),
        (status = 503, description = "Full not available (demosaic unavailable)", body = Problem)
    )
)]
pub async fn full(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    AxumPath(hash_hex): AxumPath<String>,
) -> Result<Response, Problem> {
    let hash = parse_hash(&hash_hex).ok_or_else(Problem::forbidden)?;
    let visible = AssetRepo::new(&state.db).find_by_hash(&ctx, &hash).await?;
    let Some(asset) = visible.into_iter().next() else {
        return Err(Problem::forbidden());
    };
    let folder_path = FolderRepo::new(&state.db)
        .absolute_path(&ctx, asset.folder_id)
        .await?;
    let src = folder_path.join(asset.filename.as_str());
    let data_dir = state.data_dir.clone();
    let cap = state.full_cache_bytes;
    let kind = asset.kind;
    let demosaic = state.demosaic.clone();
    let path = tokio::task::spawn_blocking(move || -> Result<_, keeppix_media::DeriveError> {
        let path = keeppix_media::full_derivative_path(&data_dir, &hash);
        if path.is_file() {
            keeppix_media::ensure_full_from_bytes(&[], &data_dir, &hash)?;
        } else {
            build_full(&src, kind, &data_dir, &hash, demosaic.as_ref())?;
        }
        keeppix_media::enforce_full_cache_cap(&data_dir, cap)?;
        Ok(keeppix_media::full_derivative_path(&data_dir, &hash))
    })
    .await
    .map_err(|_| Problem::internal())?
    .map_err(|err| match err {
        keeppix_media::DeriveError::FullUnavailable => Problem::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "full-unavailable",
            "Full resolution is not available",
        ),
        _ => Problem::not_found(),
    })?;

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| Problem::not_found())?;
    Ok(immutable_webp(bytes))
}

fn build_full(
    src: &Path,
    kind: keeppix_domain::AssetKind,
    data_dir: &Path,
    hash: &[u8; 32],
    demosaic: &dyn keeppix_jobs::raw::Demosaic,
) -> Result<std::path::PathBuf, keeppix_media::DeriveError> {
    match kind {
        keeppix_domain::AssetKind::RawImage => {
            let embedded = keeppix_media::extract_embedded_preview(src)
                .map_err(|e| keeppix_media::DeriveError::Decode(e.to_string()))?;
            if let Some(preview) = embedded.as_ref()
                && keeppix_media::embedded_usable_as_full(preview.width.max(preview.height))
            {
                return keeppix_media::ensure_full_from_bytes(&preview.bytes, data_dir, hash);
            }
            match demosaic.demosaic(src) {
                Ok(rgb) => keeppix_media::ensure_full_from_rgb(
                    &rgb.bytes, rgb.width, rgb.height, data_dir, hash,
                ),
                Err(_) => Err(keeppix_media::DeriveError::FullUnavailable),
            }
        }
        keeppix_domain::AssetKind::Video => Err(keeppix_media::DeriveError::Decode(
            "video has no full still".to_owned(),
        )),
        keeppix_domain::AssetKind::Image | keeppix_domain::AssetKind::Unknown => {
            let bytes = std::fs::read(src).map_err(keeppix_media::DeriveError::from)?;
            keeppix_media::ensure_full_from_bytes(&bytes, data_dir, hash)
        }
    }
}

/// # Errors
/// `401` / `403` same as other assets; `404` if the file is not on disk.
#[utoipa::path(
    get,
    path = "/media/original/{id}",
    tag = "media",
    operation_id = "media_original",
    summary = "Serve an original media file",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Asset id")),
    responses(
        (status = 200, description = "Original file"),
        (status = 206, description = "Byte range"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Asset not visible", body = Problem),
        (status = 404, description = "File missing", body = Problem)
    )
)]
pub async fn original(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    AxumPath(id): AxumPath<AssetId>,
    headers: HeaderMap,
) -> Result<Response, Problem> {
    let asset = AssetRepo::new(&state.db).find_by_id(&ctx, id).await?;
    let folder_path = FolderRepo::new(&state.db)
        .absolute_path(&ctx, asset.folder_id)
        .await?;
    let path = folder_path.join(asset.filename.as_str());
    stream_file(
        &path,
        headers.get(header::RANGE).and_then(|v| v.to_str().ok()),
        mime_for_name(asset.filename.as_str()),
        false,
    )
    .await
}

async fn serve_derivative(
    state: &AppState,
    ctx: &keeppix_domain::AuthContext,
    hash_hex: &str,
    thumb: bool,
) -> Result<Response, Problem> {
    let hash = parse_hash(hash_hex).ok_or_else(Problem::forbidden)?;
    let visible = AssetRepo::new(&state.db).find_by_hash(ctx, &hash).await?;
    if visible.is_empty() {
        return Err(Problem::forbidden());
    }
    let (thumb_path, preview_path) = derivative_paths(&state.data_dir, &hash);
    let path = if thumb { thumb_path } else { preview_path };
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| Problem::not_found())?;
    Ok(immutable_webp(bytes))
}

fn immutable_webp(bytes: Vec<u8>) -> Response {
    let mut response = bytes.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/webp"));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(IMMUTABLE));
    response
}

pub(crate) async fn stream_file(
    path: &Path,
    range: Option<&str>,
    content_type: &'static str,
    immutable: bool,
) -> Result<Response, Problem> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| Problem::not_found())?;
    let len = file
        .metadata()
        .await
        .map_err(|_| Problem::not_found())?
        .len();
    let (status, start, take, content_range) = match range {
        None => (StatusCode::OK, 0, len, None),
        Some(header) => {
            let Some((start, end)) = parse_byte_range(header, len) else {
                return Err(Problem::new(
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    "range-not-satisfiable",
                    "Range not satisfiable",
                ));
            };
            (
                StatusCode::PARTIAL_CONTENT,
                start,
                end.saturating_sub(start).saturating_add(1),
                Some(format!("bytes {start}-{end}/{len}")),
            )
        }
    };
    if start > 0 {
        file.seek(SeekFrom::Start(start))
            .await
            .map_err(|_| Problem::not_found())?;
    }
    let stream = ReaderStream::new(file.take(take));
    let mut response = Body::from_stream(stream).into_response();
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
        .headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Some(cr) = content_range {
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&cr).unwrap_or_else(|_| HeaderValue::from_static("bytes */0")),
        );
    }
    if immutable {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static(IMMUTABLE));
    }
    Ok(response)
}

fn parse_byte_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let rest = header.strip_prefix("bytes=")?;
    let (start, end) = rest.split_once('-')?;
    let start: u64 = start.parse().ok()?;
    let end: u64 = if end.is_empty() {
        len.saturating_sub(1)
    } else {
        end.parse().ok()?
    };
    if len == 0 || start > end || start >= len {
        return None;
    }
    Some((start, end.min(len - 1)))
}

fn parse_hash(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = core::str::from_utf8(chunk).ok()?;
        out[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(out)
}

pub(crate) fn mime_for_name(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    }
}
