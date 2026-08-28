//! `PUT`, `MKCOL`, `MOVE` — the `WebDAV` write operations, reusing the same
//! repositories as the rest of the API: no raw SQL here, only
//! `FolderRepo`/`AssetRepo`/`JobRepo` as everywhere else in the crate.
//!
//! **`COPY` is not implemented** (stays `501` in `dav::handler`'s dispatch):
//! copying an entire subtree — new ids for every folder and asset,
//! potentially across different libraries (which requires checking free
//! space on the *destination* library, which can differ from the source
//! one) — is complex enough that it ships as a `501` stub for now.
//!
//! **Never a silent overwrite on `PUT`**: same name and same hash is a
//! duplicate, skipped (`204`); same name and different hash gets a numeric
//! suffix (`201`, final name different from the one requested) — the same
//! rule as `UploadSessionRepo::finalize`, applied here by
//! `AssetRepo::ingest_direct`.

use std::path::Path;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use keeppix_db::{AssetRepo, FolderRepo, UPLOAD_TMP_DIR_NAME, ensure_disk_space};
use keeppix_domain::{AssetId, AuthContext, CollisionOutcome, FolderId};

use crate::problem::Problem;
use crate::routes::share::peek_header;
use crate::routes::upload::enqueue_indexing;
use crate::state::AppState;

/// Absolute ceiling for a `WebDAV` `PUT` body, regardless of what
/// `Content-Length` declares (or its absence, with `Transfer-Encoding:
/// chunked`) — without this, a client that sends no `Content-Length` at all
/// would bypass both this check and `ensure_disk_space` (which needs a
/// declared size to run), and could fill the disk while streaming without
/// any check catching it. 10 GiB is well beyond any real photo or video file
/// that Keeppix indexes; cost if wrong: a client legitimately uploading a
/// larger file gets a `413` instead of a successful upload — revisit if
/// huge RAW/video files are ever needed.
const MAX_BODY_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// Validates the size declared by the client against [`MAX_BODY_BYTES`] and
/// returns the cap to actually enforce on the stream: the declared size if
/// present and within the limit, otherwise `MAX_BODY_BYTES` (body with no
/// `Content-Length`, typically `Transfer-Encoding: chunked`).
///
/// # Errors
/// `413` if `Content-Length` exceeds `MAX_BODY_BYTES`.
fn check_declared_size(content_length: Option<u64>) -> Result<u64, Problem> {
    match content_length {
        Some(len) if len > MAX_BODY_BYTES => Err(Problem::payload_too_large()),
        Some(len) => Ok(len),
        None => Ok(MAX_BODY_BYTES),
    }
}

/// `PUT /dav/folder/{folder_id}/{filename}` — same idea as the `tus` upload
/// path, without a session: the body arrives already whole in a stream,
/// gets written to a temp file in `.keeppix-tmp/` inside the library, then
/// an atomic `rename()` moves it to its final place on the same filesystem.
///
/// A name starting with `.` (`.DS_Store`, `._photo.jpg`, macOS's hidden
/// sidecar files) is **accepted and saved to disk** — a `WebDAV` client
/// shouldn't get an unexpected error for a file it didn't ask to upload —
/// but **not indexed**: no `assets` row, no job (see [`put_dotfile`]).
///
/// # Errors
/// `400` if the name isn't a valid path component, or if the body is
/// empty. `403` if the caller isn't an editor on the folder. `413` if
/// `Content-Length` (or the actual body, for a client without
/// `Content-Length`) exceeds [`MAX_BODY_BYTES`]. `507` if the free space on
/// the library is below the declared size. `500` for an I/O error.
pub async fn put(
    state: &AppState,
    ctx: &AuthContext,
    folder_id: FolderId,
    filename: &str,
    content_length: Option<u64>,
    body: Body,
) -> Result<Response, Problem> {
    let (_, library) = FolderRepo::new(&state.db)
        .assert_editor(ctx, folder_id)
        .await?;

    // Guard before touching the disk, not after — a `Content-Length`
    // declared larger than the free space is rejected without writing a
    // single byte of the temp file, on the same principle as
    // `UploadSessionRepo::create` for the `tus` session
    // (`crates/keeppix-db/src/uploads.rs`).
    let cap = check_declared_size(content_length)?;
    if let Some(len) = content_length {
        let expected = i64::try_from(len).unwrap_or(i64::MAX);
        ensure_disk_space(&library.root_path, expected)?;
    }

    let tmp_dir = library.root_path.join(UPLOAD_TMP_DIR_NAME);
    tokio::fs::create_dir_all(&tmp_dir).await.map_err(|err| {
        tracing::error!(error = %err, "webdav PUT: could not create the temp dir");
        Problem::internal()
    })?;
    let temp_path = tmp_dir.join(format!("{}_{filename}", uuid::Uuid::now_v7()));

    let written = write_body_to_file(&temp_path, body, cap).await?;
    if written == 0 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(Problem::bad_request("empty-body", "PUT body was empty"));
    }

    if filename.starts_with('.') {
        return put_dotfile(&state.db, ctx, folder_id, filename, &temp_path).await;
    }

    let header = peek_header(&temp_path).await?;
    let kind = keeppix_media::detect_kind(&header);
    let hash = hash_temp_file(&temp_path).await?;
    let size_bytes = i64::try_from(written).unwrap_or(i64::MAX);

    let outcome = AssetRepo::new(&state.db)
        .ingest_direct(
            ctx,
            folder_id,
            &temp_path,
            filename,
            hash,
            size_bytes,
            Utc::now(),
            kind,
        )
        .await?;

    enqueue_indexing(&state.db, outcome.asset_id, &outcome.collision).await;

    Ok(put_response(outcome.asset_id, &outcome.collision))
}

/// Saves the dotfile to its final place without going through the asset
/// collision check — see [`put`]'s doc. Overwrites an existing same-named
/// file: that's exactly the behavior the client OS's cache file expects
/// (`.DS_Store` gets rewritten continuously by Finder), and the "never a
/// silent overwrite" invariant protects the user's photos, not the client's
/// cache.
async fn put_dotfile(
    db: &keeppix_db::Db,
    ctx: &AuthContext,
    folder_id: FolderId,
    filename: &str,
    temp_path: &Path,
) -> Result<Response, Problem> {
    let folder_dir = FolderRepo::new(db).absolute_path(ctx, folder_id).await?;
    let target = folder_dir.join(filename);
    tokio::fs::rename(temp_path, &target).await.map_err(|err| {
        tracing::error!(error = %err, "webdav PUT: could not move the dotfile into place");
        Problem::internal()
    })?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// `201 Created` + `Location: /dav/asset/{id}` for a new file (created or
/// renamed on collision); `204 No Content` for an exact duplicate — never a
/// response that would suggest a silent overwrite.
fn put_response(asset_id: AssetId, collision: &CollisionOutcome) -> Response {
    match collision {
        CollisionOutcome::SkippedDuplicate { .. } => StatusCode::NO_CONTENT.into_response(),
        CollisionOutcome::Created | CollisionOutcome::RenamedTo(_) => {
            let mut headers = HeaderMap::new();
            if let Ok(value) = HeaderValue::from_str(&format!("/dav/asset/{asset_id}")) {
                headers.insert(header::LOCATION, value);
            }
            (StatusCode::CREATED, headers).into_response()
        }
    }
}

/// `MKCOL /dav/folder/{parent_id}/{new_name}` — creates the directory on
/// disk first, then the row in the database (`FolderRepo::ensure_child`,
/// idempotent by construction).
///
/// Order matters: write to disk first, then to the database, never the
/// other way around — if the `INSERT` went first and `create_dir_all`
/// failed afterward (permissions, disk full, mount dropped), a `folders`
/// row would be left with no corresponding directory, a ghost folder that
/// no client would ever see disappear on its own. With the right order, a
/// disk failure never touches the database at all; an `INSERT` failure
/// **after** the directory was created by this same call removes it on a
/// best-effort basis (`remove_dir`, silent if non-empty or already gone) —
/// but not if the directory already existed beforehand (`MKCOL` idempotent
/// on a second attempt, or a directory left by a scanner): that one isn't
/// ours to delete.
///
/// Simplification: a repeated `MKCOL` on the same name doesn't fail with
/// `405` (RFC 4918 §9.3) — it returns `201` again on the already-existing
/// folder, because `ensure_child` is idempotent for the same reason the
/// scanner calls it repeatedly without duplicating anything. No real client
/// in use here depends on the `405`.
///
/// # Errors
/// `403` if the caller isn't an editor on the parent folder. `500` for an
/// I/O error creating the directory or a database error after the
/// directory has already been created.
pub async fn mkcol(
    state: &AppState,
    ctx: &AuthContext,
    parent_id: FolderId,
    new_name: &str,
) -> Result<Response, Problem> {
    let folder_repo = FolderRepo::new(&state.db);
    let (parent, _library) = folder_repo.assert_editor(ctx, parent_id).await?;

    let parent_path = folder_repo.absolute_path(ctx, parent_id).await?;
    let target_dir = parent_path.join(new_name);
    let already_on_disk = tokio::fs::metadata(&target_dir).await.is_ok();

    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "webdav MKCOL: could not create the directory on disk");
            Problem::internal()
        })?;

    let child = match folder_repo.ensure_child(&parent, new_name).await {
        Ok(child) => child,
        Err(err) => {
            if !already_on_disk {
                let _ = tokio::fs::remove_dir(&target_dir).await;
            }
            return Err(err.into());
        }
    };

    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&format!("/dav/folder/{}", child.id)) {
        headers.insert(header::LOCATION, value);
    }
    Ok((StatusCode::CREATED, headers).into_response())
}

/// `MOVE /dav/folder/{src_id}` with `Destination: /dav/folder/{dst_parent_id}/{name}`
/// — checks editor permission on **both** folders involved
/// (`FolderRepo::move_subtree` on its own only checks the source folder),
/// then reuses `move_subtree`, which preserves rating/albums/descriptions
/// by construction, and finally moves the directory on disk too.
///
/// Note: `move_subtree` does **not** call `rename()` — it only rewrites
/// `folders.path` in the database (verified by reading the code). The
/// physical move below happens **after** `move_subtree`'s commit: if the
/// `rename()` were to fail (only a genuine I/O error, since `move_subtree`
/// has already validated cycle/library/name-collision before getting here),
/// the folder would already be moved in the database but not on disk — an
/// inconsistency to fix by hand, the same gap already present and untouched
/// in `PATCH /api/v1/folders/{id}` (which today doesn't move the directory
/// at all). The `WebDAV` `MOVE` here is therefore already more correct than
/// the existing REST endpoint, not more fragile.
///
/// # Errors
/// `403` if the caller isn't an editor on `src_id` or on `dst_parent_id`.
/// `409` if `dst_parent_id` is in `src_id`'s subtree (including the folder
/// itself), if the two folders are in different libraries, or if the new
/// parent already has a child with the same name. `500` if the physical
/// `rename()` fails after the database has already been updated.
pub async fn move_folder(
    state: &AppState,
    ctx: &AuthContext,
    src_id: FolderId,
    dst_parent_id: FolderId,
) -> Result<Response, Problem> {
    let folder_repo = FolderRepo::new(&state.db);
    folder_repo.assert_editor(ctx, src_id).await?;
    folder_repo.assert_editor(ctx, dst_parent_id).await?;

    let old_path = folder_repo.absolute_path(ctx, src_id).await?;

    folder_repo.move_subtree(ctx, src_id, dst_parent_id).await?;

    let new_path = folder_repo.absolute_path(ctx, src_id).await?;
    if old_path != new_path {
        tokio::fs::rename(&old_path, &new_path).await.map_err(|err| {
            tracing::error!(
                error = %err,
                old = %old_path.display(),
                new = %new_path.display(),
                "webdav MOVE: the folder moved in the database but the directory rename failed on disk"
            );
            Problem::internal()
        })?;
    }

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// Writes the request body to `path` in a stream — never buffered whole in
/// RAM, in the style of `crate::routes::upload::write_chunk_checked`, but
/// without a checksum: there's no `Upload-Checksum` to compare against
/// here, the hash is computed over the whole file (see [`hash_temp_file`]),
/// exactly like `crate::routes::upload::finalize_upload`.
///
/// `max_len` is enforced byte by byte during streaming, not just checked
/// once against `Content-Length`: a client that declares a small body but
/// then sends a larger one (or that declares no size at all, `chunked`) is
/// truncated the same way, in the style of
/// `crate::routes::share::write_body_capped`.
///
/// # Errors
/// `400` if the body can't be read. `413` if the written body exceeds
/// `max_len`. `500` for an I/O error on the temp file.
async fn write_body_to_file(path: &Path, body: Body, max_len: u64) -> Result<u64, Problem> {
    use http_body::Body as _;
    use tokio::io::AsyncWriteExt as _;

    let mut file = tokio::fs::File::create(path).await.map_err(|err| {
        tracing::error!(error = %err, "webdav PUT: could not create the temp file");
        Problem::internal()
    })?;

    let mut written = 0_u64;
    let mut body = std::pin::pin!(body);
    loop {
        let frame = std::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await;
        match frame {
            None => break,
            Some(Err(_)) => {
                let _ = tokio::fs::remove_file(path).await;
                return Err(Problem::bad_request(
                    "invalid-body",
                    "Could not read the request body",
                ));
            }
            Some(Ok(frame)) => {
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                let len = u64::try_from(data.len()).unwrap_or(u64::MAX);
                if written.saturating_add(len) > max_len {
                    let _ = tokio::fs::remove_file(path).await;
                    return Err(Problem::payload_too_large());
                }
                if let Err(err) = file.write_all(&data).await {
                    tracing::error!(error = %err, "webdav PUT: write failed");
                    let _ = tokio::fs::remove_file(path).await;
                    return Err(Problem::internal());
                }
                written = written.saturating_add(len);
            }
        }
    }
    file.flush().await.map_err(|err| {
        tracing::error!(error = %err, "webdav PUT: flush failed");
        Problem::internal()
    })?;
    Ok(written)
}

/// Blake3 hash of the whole temp file, off the async thread — same as
/// `crate::routes::upload::finalize_upload`.
async fn hash_temp_file(path: &Path) -> Result<[u8; 32], Problem> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || keeppix_media::hash_file(&path))
        .await
        .map_err(|_| Problem::internal())?
        .map_err(|err| {
            tracing::error!(error = %err, "webdav PUT: could not hash the temp file");
            Problem::internal()
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_size_within_the_limit_becomes_the_streaming_cap() {
        assert_eq!(check_declared_size(Some(1024)).unwrap(), 1024);
    }

    #[test]
    fn a_declared_size_over_the_limit_is_rejected_with_413() {
        let err = check_declared_size(Some(MAX_BODY_BYTES + 1)).unwrap_err();
        assert_eq!(err.status, StatusCode::PAYLOAD_TOO_LARGE.as_u16());
    }

    #[test]
    fn a_declared_size_exactly_at_the_limit_is_accepted() {
        assert_eq!(
            check_declared_size(Some(MAX_BODY_BYTES)).unwrap(),
            MAX_BODY_BYTES
        );
    }

    #[test]
    fn no_content_length_caps_streaming_at_max_body_bytes() {
        assert_eq!(check_declared_size(None).unwrap(), MAX_BODY_BYTES);
    }
}
