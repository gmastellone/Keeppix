//! Resumable upload sessions in a tus-like style. The protocol is our own,
//! not a tus crate: hash pre-check, session creation with space and
//! permission verification, checksummed chunks, finalization with
//! end-to-end verification and filename collision resolution. See
//! `keeppix_db::UploadSessionRepo` for the logic, which this module only
//! translates to HTTP.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use keeppix_db::{AssetRepo, Db, JobRepo, NewUploadSession, UploadSessionRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, ChunkChecksum, CollisionOutcome, FolderId, JobKind,
    JobPriority, UploadSession, UploadSessionId,
};
use serde::{Deserialize, Serialize};

use crate::extract::SessionOrShare;
use crate::json::Json;
use crate::problem::Problem;
use crate::routes::share::peek_header;
use crate::state::AppState;

/// Server-side limit on a single chunk (not fixed by the client, but a
/// client declaring `remaining` in the multi-gigabyte range must not be
/// able to force it all to be buffered in RAM at once): a larger chunk
/// must be split by the client, not accepted here.
const MAX_CHUNK_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CheckRequest {
    /// blake3 hex digests (64 characters) computed locally by the client.
    pub hashes: Vec<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CheckResponse {
    /// The subset of `hashes` that the server **does not** already have:
    /// these are the only ones worth uploading ("I already have these 47,
    /// send me the other 12").
    pub unknown_hashes: Vec<String>,
}

/// `POST /api/v1/upload/check` — batch pre-check before opening sessions.
///
/// # Errors
/// `400` if a hash is not 64 hex characters. `401`/`403` if the caller
/// does not have a valid context.
#[utoipa::path(
    post,
    path = "/api/v1/upload/check",
    tag = "upload",
    operation_id = "upload_check",
    summary = "Check which hashes are already known",
    security(("session_cookie" = [])),
    request_body = CheckRequest,
    responses(
        (status = 200, description = "Hashes not yet present on the server", body = CheckResponse),
        (status = 400, description = "A hash is not 64 hex characters", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Invalid context", body = Problem)
    )
)]
pub async fn check(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    Json(req): Json<CheckRequest>,
) -> Result<Json<CheckResponse>, Problem> {
    let mut parsed = Vec::with_capacity(req.hashes.len());
    for hex in &req.hashes {
        parsed.push(decode_hex32(hex)?);
    }

    let known = AssetRepo::new(&state.db)
        .known_hashes(&ctx, &parsed)
        .await?;

    let unknown_hashes = req
        .hashes
        .into_iter()
        .zip(parsed)
        .filter(|(_, bytes)| !known.contains(bytes))
        .map(|(hex, _)| hex)
        .collect();

    Ok(Json(CheckResponse { unknown_hashes }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSessionRequest {
    #[schema(value_type = String)]
    pub target_folder_id: uuid::Uuid,
    pub filename: String,
    pub expected_size: i64,
    /// blake3 hex digest of the whole file, if the client already knows it.
    pub expected_hash: Option<String>,
    /// Original `mtime` on the client device: preserved as a fallback for
    /// `taken_at` when EXIF doesn't have it.
    #[schema(value_type = Option<String>)]
    pub client_mtime: Option<DateTime<Utc>>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CreateSessionResponse {
    pub id: String,
}

/// `POST /api/v1/upload` — opens a session. `201` with `Location:
/// /api/v1/upload/{id}`.
///
/// # Errors
/// `400` if `filename` or `expected_hash` are invalid. `403` if the caller
/// cannot write to the destination folder, or if a share link does not
/// have `allow_upload` on the exact object. `507` if the library
/// filesystem's free space is below `expected_size`.
#[utoipa::path(
    post,
    path = "/api/v1/upload",
    tag = "upload",
    operation_id = "upload_create_session",
    summary = "Open a resumable upload session",
    security(("session_cookie" = [])),
    request_body = CreateSessionRequest,
    responses(
        (status = 201, description = "Session created, Location: /api/v1/upload/{id}", body = CreateSessionResponse),
        (status = 400, description = "Invalid filename or expected_hash", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Cannot write to the destination folder", body = Problem),
        (status = 507, description = "Insufficient free space", body = Problem)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Response, Problem> {
    let filename = AssetName::parse(&req.filename)
        .map_err(|_| Problem::bad_request("invalid-filename", "Invalid filename"))?;
    let expected_hash = match req.expected_hash {
        Some(hex) => Some(decode_hex32(&hex)?),
        None => None,
    };

    let session = UploadSessionRepo::new(&state.db)
        .create(
            &ctx,
            NewUploadSession {
                target_folder_id: FolderId::from_uuid(req.target_folder_id),
                filename: filename.as_str().to_owned(),
                expected_size: req.expected_size,
                expected_hash,
                client_mtime: req.client_mtime,
            },
        )
        .await?;

    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&format!("/api/v1/upload/{}", session.id)) {
        headers.insert(header::LOCATION, value);
    }

    Ok((
        StatusCode::CREATED,
        headers,
        Json(CreateSessionResponse {
            id: session.id.to_string(),
        }),
    )
        .into_response())
}

/// `HEAD /api/v1/upload/{id}` — the true offset, never what the client
/// believes it has ("the truth always lives on the server").
///
/// # Errors
/// `403` if the caller is not the owner of the session. `410` if it has
/// expired.
#[utoipa::path(
    head,
    path = "/api/v1/upload/{id}",
    tag = "upload",
    operation_id = "upload_session_head",
    summary = "Get the true received-bytes offset of an upload session",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Session id")),
    responses(
        (status = 200, description = "True offset in the Upload-Offset header"),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not the owner of the session", body = Problem),
        (status = 410, description = "Session expired", body = Problem)
    )
)]
pub async fn head(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    Path(id): Path<uuid::Uuid>,
) -> Result<Response, Problem> {
    let session = UploadSessionRepo::new(&state.db)
        .load_owned(&ctx, UploadSessionId::from_uuid(id))
        .await?;
    Ok((StatusCode::OK, offset_header(session.received_bytes)).into_response())
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UploadCompleteResponse {
    pub asset_id: String,
    pub filename: String,
    /// `created`, `skipped_duplicate`, or `renamed`: never a silent
    /// overwrite, always reported to the client.
    pub collision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_asset_id: Option<String>,
}

/// `PATCH /api/v1/upload/{id}` — appends a chunk, verifies its checksum,
/// and finalizes the session when the offset reaches `expected_size`.
///
/// # Errors
/// `403`/`410` as in [`head`]. `409` if `Upload-Offset` does not match the
/// real `received_bytes` — never a silent acceptance that corrupts the
/// file. `400` if a required header is missing or the body is empty.
/// `413` if the chunk exceeds `MAX_CHUNK_BYTES` or the session's remaining
/// bytes: the body is read in streaming, never buffered whole in RAM.
/// `460` if the chunk's checksum does not match: the chunk is not written,
/// the client can resend it without losing the previous offset. `422` at
/// full-file time, if the end-to-end hash does not match `expected_hash`
/// or the file is not decodable — never ends up in the library, the
/// temporary file is deleted, the session is marked failed.
#[utoipa::path(
    patch,
    path = "/api/v1/upload/{id}",
    tag = "upload",
    operation_id = "upload_session_patch",
    summary = "Append a chunk, finalizing the session when complete",
    security(("session_cookie" = [])),
    params(("id" = String, Path, description = "Session id")),
    request_body(content = Vec<u8>, description = "Raw chunk", content_type = "application/octet-stream"),
    responses(
        (status = 204, description = "Chunk accepted, session not yet complete"),
        (status = 201, description = "Session completed", body = UploadCompleteResponse),
        (status = 400, description = "Missing header or empty body", body = Problem),
        (status = 401, description = "Not authenticated", body = Problem),
        (status = 403, description = "Not the owner of the session", body = Problem),
        (status = 409, description = "Upload-Offset does not match received_bytes", body = Problem),
        (status = 410, description = "Session expired", body = Problem),
        (status = 413, description = "Chunk beyond the allowed limit", body = Problem),
        (status = 422, description = "End-to-end hash or decoding failed", body = Problem),
        (status = 460, description = "Chunk checksum does not match", body = Problem)
    )
)]
pub async fn patch(
    State(state): State<AppState>,
    SessionOrShare(ctx): SessionOrShare,
    Path(id): Path<uuid::Uuid>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Problem> {
    let session_id = UploadSessionId::from_uuid(id);
    let repo = UploadSessionRepo::new(&state.db);
    let session = repo.load_owned(&ctx, session_id).await?;

    let offset = parse_offset_header(&headers)?;
    if offset != session.received_bytes {
        return Err(Problem::offset_mismatch(session.received_bytes));
    }

    let remaining = session.expected_size - session.received_bytes;
    if remaining <= 0 {
        return Err(Problem::bad_request(
            "upload-already-complete",
            "This upload session has no bytes left to receive",
        ));
    }
    let max_len = u64::try_from(remaining)
        .unwrap_or(u64::MAX)
        .min(MAX_CHUNK_BYTES);

    let checksum = parse_checksum_header(&headers)?;
    let written = write_chunk_checked(&session.temp_path, body, checksum, max_len).await?;

    let written_i64 = i64::try_from(written).unwrap_or(i64::MAX);
    let new_offset = session.received_bytes.saturating_add(written_i64);
    repo.advance(&ctx, session_id, new_offset).await?;

    if new_offset < session.expected_size {
        return Ok((StatusCode::NO_CONTENT, offset_header(new_offset)).into_response());
    }

    let response = finalize_upload(&state.db, &repo, &ctx, session_id, &session).await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

/// Indexes the just-finalized file right away, without waiting for the
/// watcher's usual delay. An exact duplicate (`SkippedDuplicate`) is left
/// untouched: the existing asset was already indexed the first time, a
/// second job would just be wasted work.
///
/// # Errors
/// `Problem::internal()` if the enqueue fails — an error here must not
/// fail the response to the client: the asset already exists, indexing
/// will still arrive from the watcher's next rescan, just later.
///
/// `pub(crate)`: also reused by `dav::write::put`, which indexes a
/// `WebDAV` `PUT` exactly like a finalized tus chunk.
pub(crate) async fn enqueue_indexing(db: &Db, asset_id: AssetId, collision: &CollisionOutcome) {
    if matches!(collision, CollisionOutcome::SkippedDuplicate { .. }) {
        return;
    }
    if let Err(err) = JobRepo::new(db)
        .enqueue(
            JobKind::ExtractMetadata,
            serde_json::json!({ "asset_id": asset_id.to_string() }),
            JobPriority::High,
            Some(&format!("meta:{asset_id}")),
        )
        .await
    {
        tracing::warn!(
            error = %err,
            asset_id = %asset_id,
            "upload finalize: could not enqueue high-priority indexing, the next rescan will catch it"
        );
    }
}

async fn finalize_upload(
    db: &Db,
    repo: &UploadSessionRepo<'_>,
    ctx: &AuthContext,
    id: UploadSessionId,
    session: &UploadSession,
) -> Result<UploadCompleteResponse, Problem> {
    let path = session.temp_path.clone();
    let computed = tokio::task::spawn_blocking(move || keeppix_media::hash_file(&path))
        .await
        .map_err(|_| Problem::internal())?
        .map_err(|err| {
            tracing::error!(error = %err, "upload finalize: could not hash the temp file");
            Problem::internal()
        })?;

    if let Some(expected) = session.expected_hash
        && expected != computed
    {
        repo.fail(ctx, id).await?;
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "upload-hash-mismatch",
            "The uploaded file's hash does not match the declared expected_hash",
        ));
    }

    let header = peek_header(&session.temp_path).await?;
    let kind = keeppix_media::detect_kind(&header);
    if kind == AssetKind::Unknown {
        repo.fail(ctx, id).await?;
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "upload-undecodable",
            "The uploaded file could not be verified as decodable",
        ));
    }

    let outcome = repo.finalize(ctx, id, kind, computed).await?;
    enqueue_indexing(db, outcome.asset_id, &outcome.collision).await;
    Ok(match outcome.collision {
        CollisionOutcome::Created => UploadCompleteResponse {
            asset_id: outcome.asset_id.to_string(),
            filename: outcome.filename,
            collision: "created".to_owned(),
            existing_asset_id: None,
        },
        CollisionOutcome::SkippedDuplicate { existing_asset_id } => UploadCompleteResponse {
            asset_id: outcome.asset_id.to_string(),
            filename: outcome.filename,
            collision: "skipped_duplicate".to_owned(),
            existing_asset_id: Some(existing_asset_id.to_string()),
        },
        CollisionOutcome::RenamedTo(_) => UploadCompleteResponse {
            asset_id: outcome.asset_id.to_string(),
            filename: outcome.filename,
            collision: "renamed".to_owned(),
            existing_asset_id: None,
        },
    })
}

/// Writes a chunk to the temp file in streaming — never buffered whole in
/// memory, unlike an `axum::body::to_bytes(body, cap)` with `cap` up to
/// `expected_size` — computing the blake3 hash incrementally as bytes
/// arrive, in the style of `crate::routes::share::write_body_capped`.
///
/// The checksum is verified only once the body is exhausted, but the
/// chunk does **not** survive a wrong checksum: the append file is
/// truncated back to its original length (`set_len`), undoing exactly the
/// bytes just written — the client can resend it without losing the
/// offset already accepted.
///
/// # Errors
/// `400`/`413` if the body is empty, unreadable, or exceeds `max_len`
/// (per-chunk limit, never the rest of the whole file at once). `460` if
/// the checksum does not match. `500` for an I/O error on the temp file.
async fn write_chunk_checked(
    path: &std::path::Path,
    body: Body,
    checksum: ChunkChecksum,
    max_len: u64,
) -> Result<u64, Problem> {
    use http_body::Body as _;
    use tokio::io::AsyncWriteExt as _;

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "upload chunk: could not open temp file");
            Problem::internal()
        })?;
    let original_len = file
        .metadata()
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "upload chunk: could not stat temp file");
            Problem::internal()
        })?
        .len();

    let mut hasher = blake3::Hasher::new();
    let mut written = 0_u64;
    let mut body = std::pin::pin!(body);
    loop {
        let frame = std::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await;
        match frame {
            None => break,
            Some(Err(_)) => {
                let _ = file.set_len(original_len).await;
                return Err(Problem::bad_request(
                    "invalid-body",
                    "Could not read the chunk",
                ));
            }
            Some(Ok(frame)) => {
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                let len = u64::try_from(data.len()).unwrap_or(u64::MAX);
                if written.saturating_add(len) > max_len {
                    // Never write past the limit: undo whatever was
                    // already accepted for this chunk and report 413,
                    // not a silent partial advance.
                    let _ = file.set_len(original_len).await;
                    return Err(Problem::payload_too_large());
                }
                hasher.update(&data);
                if let Err(err) = file.write_all(&data).await {
                    tracing::error!(error = %err, "upload chunk: write failed");
                    let _ = file.set_len(original_len).await;
                    return Err(Problem::internal());
                }
                written = written.saturating_add(len);
            }
        }
    }

    if written == 0 {
        return Err(Problem::bad_request("empty-chunk", "Chunk body was empty"));
    }

    let computed = hasher.finalize();
    if !checksum.matches(computed.as_bytes()) {
        // The chunk must not be left on the temp file: the client resends
        // it without losing the offset the server already accepted before
        // this chunk.
        file.set_len(original_len).await.map_err(|err| {
            tracing::error!(error = %err, "upload chunk: truncate after checksum mismatch failed");
            Problem::internal()
        })?;
        return Err(Problem::chunk_checksum_mismatch());
    }

    // One fsync per chunk, not every 16 MB: safer with adaptive chunks up
    // to `MAX_CHUNK_BYTES`, and the extra cost of an fsync on a local file
    // is negligible compared to the network.
    file.sync_all().await.map_err(|err| {
        tracing::error!(error = %err, "upload chunk: fsync failed");
        Problem::internal()
    })?;
    Ok(written)
}

fn offset_header(offset: i64) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&offset.to_string()) {
        headers.insert(HeaderName::from_static("upload-offset"), value);
    }
    headers
}

fn parse_offset_header(headers: &HeaderMap) -> Result<i64, Problem> {
    let value = headers.get("upload-offset").ok_or_else(|| {
        Problem::bad_request("missing-upload-offset", "Upload-Offset header is required")
    })?;
    let text = value.to_str().map_err(|_| {
        Problem::bad_request(
            "invalid-upload-offset",
            "Upload-Offset must be a non-negative integer",
        )
    })?;
    text.parse::<i64>().map_err(|_| {
        Problem::bad_request(
            "invalid-upload-offset",
            "Upload-Offset must be a non-negative integer",
        )
    })
}

/// `Upload-Checksum: blake3 <64 hex characters>` — same style as the tus
/// 1.0 checksum extension (algorithm followed by the digest), hex instead
/// of base64 to stay consistent with `content_hash` throughout the API
/// (`crate::routes::timeline::hex_hash`).
fn parse_checksum_header(headers: &HeaderMap) -> Result<ChunkChecksum, Problem> {
    let value = headers.get("upload-checksum").ok_or_else(|| {
        Problem::bad_request(
            "missing-upload-checksum",
            "Upload-Checksum header is required",
        )
    })?;
    let text = value.to_str().map_err(|_| bad_checksum_header())?;
    let (algo, hex) = text.split_once(' ').ok_or_else(bad_checksum_header)?;
    if algo != "blake3" {
        return Err(bad_checksum_header());
    }
    Ok(ChunkChecksum(decode_hex32(hex)?))
}

fn bad_checksum_header() -> Problem {
    Problem::bad_request(
        "invalid-upload-checksum",
        "Upload-Checksum must be `blake3 <64 hex characters>`",
    )
}

fn decode_hex32(hex: &str) -> Result<[u8; 32], Problem> {
    if hex.len() != 64 {
        return Err(bad_hash());
    }
    let mut out = [0_u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let piece = core::str::from_utf8(chunk).map_err(|_| bad_hash())?;
        out[i] = u8::from_str_radix(piece, 16).map_err(|_| bad_hash())?;
    }
    Ok(out)
}

fn bad_hash() -> Problem {
    Problem::bad_request("invalid-hash", "Expected 64 hex characters")
}
