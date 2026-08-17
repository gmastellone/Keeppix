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

use crate::extract::Auth;
use crate::problem::Problem;
use crate::state::AppState;

// Authenticated media: browser cache only. `public` waits for Fase 3 share URLs
// (design §CDN; spec 1c said public — see STATO ruling).
const IMMUTABLE: &str = "private, max-age=31536000, immutable";

/// # Errors
/// `401` se non autenticato; `403` se l'hash non è visibile o non esiste.
#[utoipa::path(
    get,
    path = "/media/thumb/{hash}",
    tag = "media",
    operation_id = "media_thumb",
    security(("session_cookie" = [])),
    params(("hash" = String, Path, description = "blake3 hex da 64 caratteri")),
    responses(
        (status = 200, description = "Thumbnail WebP"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Hash non visibile", body = Problem),
        (status = 404, description = "Derivato assente su disco", body = Problem)
    )
)]
pub async fn thumb(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(hash): AxumPath<String>,
) -> Result<Response, Problem> {
    serve_derivative(&state, &ctx, &hash, true).await
}

/// # Errors
/// Come `thumb`.
#[utoipa::path(
    get,
    path = "/media/preview/{hash}",
    tag = "media",
    operation_id = "media_preview",
    security(("session_cookie" = [])),
    params(("hash" = String, Path, description = "blake3 hex da 64 caratteri")),
    responses(
        (status = 200, description = "Preview WebP"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Hash non visibile", body = Problem),
        (status = 404, description = "Derivato assente su disco", body = Problem)
    )
)]
pub async fn preview(
    State(state): State<AppState>,
    Auth(ctx): Auth,
    AxumPath(hash): AxumPath<String>,
) -> Result<Response, Problem> {
    serve_derivative(&state, &ctx, &hash, false).await
}

/// # Errors
/// Come `thumb`. Genera il livello `full` alla prima richiesta.
#[utoipa::path(
    get,
    path = "/media/full/{hash}",
    tag = "media",
    operation_id = "media_full",
    security(("session_cookie" = [])),
    params(("hash" = String, Path, description = "blake3 hex da 64 caratteri")),
    responses(
        (status = 200, description = "Full WebP a risoluzione nativa"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Hash non visibile", body = Problem),
        (status = 404, description = "Originale assente o non derivabile", body = Problem)
    )
)]
pub async fn full(
    State(state): State<AppState>,
    Auth(ctx): Auth,
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
    let path = tokio::task::spawn_blocking(move || -> Result<_, keeppix_media::DeriveError> {
        let path = keeppix_media::full_derivative_path(&data_dir, &hash);
        if path.is_file() {
            keeppix_media::ensure_full_from_bytes(&[], &data_dir, &hash)?;
        } else {
            let bytes = jpeg_bytes_for_full(&src, kind)?;
            keeppix_media::ensure_full_from_bytes(&bytes, &data_dir, &hash)?;
        }
        keeppix_media::enforce_full_cache_cap(&data_dir, cap)?;
        Ok(keeppix_media::full_derivative_path(&data_dir, &hash))
    })
    .await
    .map_err(|_| Problem::internal())?
    .map_err(|_| Problem::not_found())?;

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| Problem::not_found())?;
    Ok(immutable_webp(bytes))
}

fn jpeg_bytes_for_full(
    src: &Path,
    kind: keeppix_domain::AssetKind,
) -> Result<Vec<u8>, keeppix_media::DeriveError> {
    match kind {
        keeppix_domain::AssetKind::RawImage => {
            let preview = keeppix_media::extract_embedded_preview(src)
                .map_err(|e| keeppix_media::DeriveError::Decode(e.to_string()))?
                .ok_or_else(|| {
                    keeppix_media::DeriveError::Decode("no embedded preview".to_owned())
                })?;
            Ok(preview.bytes)
        }
        keeppix_domain::AssetKind::Video => Err(keeppix_media::DeriveError::Decode(
            "video has no full still".to_owned(),
        )),
        keeppix_domain::AssetKind::Image | keeppix_domain::AssetKind::Unknown => {
            std::fs::read(src).map_err(Into::into)
        }
    }
}

/// # Errors
/// `401` / `403` come gli altri asset; `404` se il file non è sul disco.
#[utoipa::path(
    get,
    path = "/media/original/{id}",
    tag = "media",
    operation_id = "media_original",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Id dell'asset")),
    responses(
        (status = 200, description = "File originale"),
        (status = 206, description = "Byte range"),
        (status = 401, description = "Non autenticato", body = Problem),
        (status = 403, description = "Asset non visibile", body = Problem),
        (status = 404, description = "File assente", body = Problem)
    )
)]
pub async fn original(
    State(state): State<AppState>,
    Auth(ctx): Auth,
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

async fn stream_file(
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

fn mime_for_name(name: &str) -> &'static str {
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
