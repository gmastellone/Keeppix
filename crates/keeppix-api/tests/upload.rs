//! Resumable upload sessions in tus style: pre-check, creation, per-chunk
//! checksums, finalization with end-to-end verification, and filename
//! collision resolution.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    create_library, create_share_link_from, scan_and_wait, setup_admin, share_client,
    tiny_fixture_path,
};
use keeppix_db::FolderRepo;
use keeppix_domain::LibraryId;
use serde_json::json;

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn blake3_hex(bytes: &[u8]) -> String {
    hex(blake3::hash(bytes).as_bytes())
}

/// An empty library with a single root folder, ready to receive uploads.
///
/// A root folder doesn't exist until the walker discovers at least one
/// file inside it (`FolderRepo::ensure_path` in `discover.rs`): for a
/// still-empty library it's created directly here, instead of waiting for
/// a scan that wouldn't find anything to index.
async fn library_with_folder(server: &TestServer, name_hint: &str) -> (String, PathBuf) {
    setup_admin(server).await;
    let root = server
        .photos_root
        .join(format!("{name_hint}-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    let library_id = create_library(server, name_hint, &root).await;
    let library: LibraryId = library_id.parse().unwrap();
    FolderRepo::new(&server.db)
        .ensure_path(library, &[])
        .await
        .unwrap();
    (library_id, root)
}

async fn root_folder_id(server: &TestServer, library_id: &str) -> String {
    let folder: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM folders WHERE library_id = $1 AND parent_id IS NULL")
            .bind(uuid::Uuid::parse_str(library_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    folder.0.to_string()
}

async fn create_session(
    server: &TestServer,
    folder_id: &str,
    filename: &str,
    expected_size: i64,
    expected_hash: Option<&str>,
) -> String {
    let mut body = json!({
        "target_folder_id": folder_id,
        "filename": filename,
        "expected_size": expected_size,
    });
    if let Some(hash) = expected_hash {
        body["expected_hash"] = json!(hash);
    }
    let response = server
        .client
        .post(server.url("/api/v1/upload"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let location = response
        .headers()
        .get("location")
        .expect("Location header")
        .to_str()
        .unwrap()
        .to_owned();
    let body: serde_json::Value = response.json().await.unwrap();
    let id = body["id"].as_str().expect("session id").to_owned();
    assert_eq!(location, format!("/api/v1/upload/{id}"));
    id
}

async fn patch_chunk(
    server: &TestServer,
    session_id: &str,
    offset: i64,
    checksum_hex: &str,
    bytes: &[u8],
) -> reqwest::Response {
    server
        .client
        .patch(server.url(&format!("/api/v1/upload/{session_id}")))
        .header("upload-offset", offset.to_string())
        .header("upload-checksum", format!("blake3 {checksum_hex}"))
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap()
}

async fn upload_offset(server: &TestServer, session_id: &str) -> String {
    let response = server
        .client
        .head(server.url(&format!("/api/v1/upload/{session_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response
        .headers()
        .get("upload-offset")
        .expect("Upload-Offset header")
        .to_str()
        .unwrap()
        .to_owned()
}

/// A set of hashes, some unknown — here two, one known and one not — the
/// response lists exactly the unknown ones, zero false positives/negatives.
#[tokio::test]
async fn precheck_returns_only_the_unknown_hashes() {
    let server = TestServer::start().await;
    let (library_id, root) = library_with_folder(&server, "precheck").await;
    fs::copy(tiny_fixture_path(), root.join("known.jpg")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    scan_and_wait(&server, &library_id, 1, deadline).await;

    let known_hash: Vec<u8> =
        sqlx::query_scalar("SELECT content_hash FROM assets WHERE filename = 'known.jpg'")
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    let known_hex = hex(&known_hash);
    let unknown_hex = "ab".repeat(32);

    let response = server
        .client
        .post(server.url("/api/v1/upload/check"))
        .json(&json!({ "hashes": [known_hex, unknown_hex.clone()] }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let unknowns: Vec<&str> = body["unknown_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(unknowns, vec![unknown_hex.as_str()]);
}

/// `Upload-Offset` different from the actual `received_bytes` → `409`,
/// not a silent acceptance that corrupts the file.
#[tokio::test]
async fn patch_with_wrong_offset_is_rejected_with_409() {
    let server = TestServer::start().await;
    let (library_id, _root) = library_with_folder(&server, "offset").await;
    let folder_id = root_folder_id(&server, &library_id).await;
    let bytes = fs::read(tiny_fixture_path()).unwrap();
    let session_id = create_session(
        &server,
        &folder_id,
        "photo.jpg",
        i64::try_from(bytes.len()).unwrap(),
        None,
    )
    .await;

    let response = patch_chunk(&server, &session_id, 999, &blake3_hex(&bytes), &bytes).await;
    assert_eq!(response.status(), 409);
    assert_eq!(upload_offset(&server, &session_id).await, "0");
}

/// A chunk checksum that doesn't match → `460`, the chunk is **not**
/// written, the client can resend it without losing the previous offset.
#[tokio::test]
async fn patch_with_wrong_chunk_checksum_is_rejected_and_does_not_advance() {
    let server = TestServer::start().await;
    let (library_id, _root) = library_with_folder(&server, "checksum").await;
    let folder_id = root_folder_id(&server, &library_id).await;
    let bytes = fs::read(tiny_fixture_path()).unwrap();
    let session_id = create_session(
        &server,
        &folder_id,
        "photo.jpg",
        i64::try_from(bytes.len()).unwrap(),
        None,
    )
    .await;

    let wrong_checksum = "00".repeat(32);
    let response = patch_chunk(&server, &session_id, 0, &wrong_checksum, &bytes).await;
    assert_eq!(response.status(), 460);
    assert_eq!(upload_offset(&server, &session_id).await, "0");
}

/// With a complete file, the total blake3 hash doesn't match
/// `expected_hash` → the file never enters the library, the temp file is
/// deleted, the session is marked failed.
#[tokio::test]
async fn completing_with_a_wrong_expected_hash_never_enters_the_library() {
    let server = TestServer::start().await;
    let (library_id, root) = library_with_folder(&server, "badhash").await;
    let folder_id = root_folder_id(&server, &library_id).await;
    let bytes = fs::read(tiny_fixture_path()).unwrap();
    let wrong_hash = "11".repeat(32);
    let session_id = create_session(
        &server,
        &folder_id,
        "photo.jpg",
        i64::try_from(bytes.len()).unwrap(),
        Some(&wrong_hash),
    )
    .await;

    let temp_path: String =
        sqlx::query_scalar("SELECT temp_path FROM upload_sessions WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&session_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .unwrap();

    let response = patch_chunk(&server, &session_id, 0, &blake3_hex(&bytes), &bytes).await;
    assert_eq!(response.status(), 422);

    assert!(
        !Path::new(&temp_path).exists(),
        "the temp file for a wrong hash must be cleaned up"
    );
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&session_id).unwrap())
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0, "the failed session must be deleted");
    let assets: i64 =
        sqlx::query_scalar("SELECT count(*) FROM assets WHERE filename = 'photo.jpg'")
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    assert_eq!(assets, 0, "a wrong hash must never create an asset");
    assert!(!root.join("photo.jpg").exists());
}

/// Two `PATCH`es in sequence complete the upload: verifies that
/// `write_chunk_checked` really writes in append mode (streaming, never
/// buffered entirely in RAM) and doesn't overwrite what was already
/// received — the end-to-end hash that finalizes the session is that of
/// the whole file, not just the last chunk.
#[tokio::test]
async fn a_multi_chunk_upload_completes_across_two_patches() {
    let server = TestServer::start().await;
    let (library_id, root) = library_with_folder(&server, "multichunk").await;
    let folder_id = root_folder_id(&server, &library_id).await;
    let bytes = fs::read(tiny_fixture_path()).unwrap();
    assert!(
        bytes.len() > 4,
        "the fixture must be splittable into two chunks"
    );
    let (first, second) = bytes.split_at(bytes.len() / 2);

    let session_id = create_session(
        &server,
        &folder_id,
        "multi.jpg",
        i64::try_from(bytes.len()).unwrap(),
        Some(&blake3_hex(&bytes)),
    )
    .await;

    let response = patch_chunk(&server, &session_id, 0, &blake3_hex(first), first).await;
    assert_eq!(response.status(), 204);
    assert_eq!(
        upload_offset(&server, &session_id).await,
        first.len().to_string()
    );

    let response = patch_chunk(
        &server,
        &session_id,
        i64::try_from(first.len()).unwrap(),
        &blake3_hex(second),
        second,
    )
    .await;
    assert_eq!(response.status(), 201);

    assert_eq!(fs::read(root.join("multi.jpg")).unwrap(), bytes);
}

/// A failed decodability check (corrupted header) even with a correct
/// hash → the same outcome as above: never enters the library, reported,
/// temp file and session cleaned up.
#[tokio::test]
async fn completing_with_undecodable_content_never_enters_the_library() {
    let server = TestServer::start().await;
    let (library_id, root) = library_with_folder(&server, "undecodable").await;
    let folder_id = root_folder_id(&server, &library_id).await;
    // No magic number recognized by `keeppix_media::detect_kind`: not
    // JPEG, PNG, GIF, WEBP/AVI (RIFF), TIFF, ISOBMFF (`ftyp`), or EBML/MKV.
    let garbage =
        b"garbage payload, not a real image or video, long enough to fill the sniffed header"
            .to_vec();
    let hash = blake3_hex(&garbage);
    let session_id = create_session(
        &server,
        &folder_id,
        "garbage.jpg",
        i64::try_from(garbage.len()).unwrap(),
        Some(&hash),
    )
    .await;

    let temp_path: String =
        sqlx::query_scalar("SELECT temp_path FROM upload_sessions WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&session_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .unwrap();

    // The chunk's (per-chunk) hash and the `expected_hash` (end-to-end)
    // coincide here because the chunk is the whole file: the hash is
    // correct, only the content isn't decodable.
    let response = patch_chunk(&server, &session_id, 0, &hash, &garbage).await;
    assert_eq!(response.status(), 422);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/upload-undecodable");

    assert!(
        !Path::new(&temp_path).exists(),
        "the temp file for an undecodable file must be cleaned up"
    );
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&session_id).unwrap())
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0, "the failed session must be deleted");
    let assets: i64 =
        sqlx::query_scalar("SELECT count(*) FROM assets WHERE filename = 'garbage.jpg'")
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    assert_eq!(assets, 0, "an undecodable file must never create an asset");
    assert!(!root.join("garbage.jpg").exists());
}

/// The `client_mtime` the client declares when opening the session ends
/// up on the finalized asset's `assets.mtime` (a fallback for `taken_at`
/// when EXIF doesn't have it).
#[tokio::test]
async fn client_mtime_is_preserved_on_the_finalized_asset() {
    let server = TestServer::start().await;
    let (library_id, _root) = library_with_folder(&server, "mtime").await;
    let folder_id = root_folder_id(&server, &library_id).await;
    let bytes = fs::read(tiny_fixture_path()).unwrap();
    let client_mtime = "2019-05-01T12:30:00Z";

    let response = server
        .client
        .post(server.url("/api/v1/upload"))
        .json(&json!({
            "target_folder_id": folder_id,
            "filename": "photo.jpg",
            "expected_size": i64::try_from(bytes.len()).unwrap(),
            "client_mtime": client_mtime,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let body: serde_json::Value = response.json().await.unwrap();
    let session_id = body["id"].as_str().unwrap().to_owned();

    let response = patch_chunk(&server, &session_id, 0, &blake3_hex(&bytes), &bytes).await;
    assert_eq!(response.status(), 201);
    let body: serde_json::Value = response.json().await.unwrap();
    let asset_id = body["asset_id"].as_str().unwrap().to_owned();

    let mtime: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT mtime FROM assets WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&asset_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    assert_eq!(
        mtime,
        client_mtime
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap()
    );
}

/// Same name and same hash as an asset already present → it's skipped,
/// reported, no second file.
#[tokio::test]
async fn same_name_and_hash_is_skipped_as_a_duplicate() {
    let server = TestServer::start().await;
    let (library_id, root) = library_with_folder(&server, "dup").await;
    let bytes = fs::read(tiny_fixture_path()).unwrap();
    fs::write(root.join("dup.jpg"), &bytes).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    scan_and_wait(&server, &library_id, 1, deadline).await;
    let folder_id = root_folder_id(&server, &library_id).await;

    let session_id = create_session(
        &server,
        &folder_id,
        "dup.jpg",
        i64::try_from(bytes.len()).unwrap(),
        None,
    )
    .await;
    let response = patch_chunk(&server, &session_id, 0, &blake3_hex(&bytes), &bytes).await;
    assert_eq!(response.status(), 201);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["collision"], "skipped_duplicate");
    assert!(body["existing_asset_id"].is_string());

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE filename = 'dup.jpg'")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 1, "no second file for an exact duplicate");
}

/// Same name, different hash → saved as `name_1.ext`, never a silent
/// overwrite.
#[tokio::test]
async fn same_name_different_hash_is_saved_with_a_numeric_suffix() {
    let server = TestServer::start().await;
    let (library_id, root) = library_with_folder(&server, "rename").await;
    let existing_bytes = fs::read(tiny_fixture_path()).unwrap();
    fs::write(root.join("photo.jpg"), &existing_bytes).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    scan_and_wait(&server, &library_id, 1, deadline).await;
    let folder_id = root_folder_id(&server, &library_id).await;

    let mut new_bytes = existing_bytes.clone();
    new_bytes.push(0);

    let session_id = create_session(
        &server,
        &folder_id,
        "photo.jpg",
        i64::try_from(new_bytes.len()).unwrap(),
        None,
    )
    .await;
    let response = patch_chunk(&server, &session_id, 0, &blake3_hex(&new_bytes), &new_bytes).await;
    assert_eq!(response.status(), 201);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["collision"], "renamed");
    assert_eq!(body["filename"], "photo_1.jpg");

    assert!(root.join("photo_1.jpg").is_file());
    assert_eq!(
        fs::read(root.join("photo.jpg")).unwrap(),
        existing_bytes,
        "the existing file must never be overwritten"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM assets a JOIN folders f ON f.id = a.folder_id \
         WHERE f.library_id = $1",
    )
    .bind(uuid::Uuid::parse_str(&library_id).unwrap())
    .fetch_one(server.db.pool())
    .await
    .unwrap();
    assert_eq!(count, 2);
}

/// A share link without `allow_upload` gets `403` when creating the
/// session, before accepting any bytes.
#[tokio::test]
async fn opening_a_session_from_a_share_link_without_allow_upload_is_forbidden() {
    let server = TestServer::start().await;
    let (library_id, _root) = library_with_folder(&server, "noupload").await;
    let folder_id = root_folder_id(&server, &library_id).await;

    let token = create_share_link_from(
        &server,
        json!({
            "object_type": "folder",
            "object_id": folder_id,
        }),
    )
    .await;

    let guest = share_client(&token);
    let response = guest
        .post(server.url("/api/v1/upload"))
        .json(&json!({
            "target_folder_id": folder_id,
            "filename": "guest.jpg",
            "expected_size": 10,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM upload_sessions")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0);
}

/// `expires_at` in the past → the session has already been cleaned up,
/// and `PATCH` finds it gone, not merely inaccessible.
#[tokio::test]
async fn patch_on_an_expired_session_reports_it_is_gone() {
    let server = TestServer::start().await;
    let (library_id, _root) = library_with_folder(&server, "expired").await;
    let folder_id = root_folder_id(&server, &library_id).await;
    let bytes = fs::read(tiny_fixture_path()).unwrap();
    let session_id = create_session(
        &server,
        &folder_id,
        "photo.jpg",
        i64::try_from(bytes.len()).unwrap(),
        None,
    )
    .await;

    sqlx::query(
        "UPDATE upload_sessions SET expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&session_id).unwrap())
    .execute(server.db.pool())
    .await
    .unwrap();

    let response = patch_chunk(&server, &session_id, 0, &blake3_hex(&bytes), &bytes).await;
    assert_eq!(response.status(), 410);
}

/// An uploaded file must not have to wait for the watcher's 15-minute
/// cycle. On finalization, an `extract_metadata` job at
/// `JobPriority::High` (value 1) must be queued with the correct
/// `asset_id` payload.
#[tokio::test]
async fn finalizing_an_upload_enqueues_a_high_priority_extract_metadata_job() {
    let server = TestServer::start().await;
    let (library_id, _root) = library_with_folder(&server, "index-now").await;
    let folder_id = root_folder_id(&server, &library_id).await;
    let bytes = fs::read(tiny_fixture_path()).unwrap();
    let session_id = create_session(
        &server,
        &folder_id,
        "photo.jpg",
        i64::try_from(bytes.len()).unwrap(),
        None,
    )
    .await;

    let response = patch_chunk(&server, &session_id, 0, &blake3_hex(&bytes), &bytes).await;
    assert_eq!(response.status(), 201);
    let body: serde_json::Value = response.json().await.unwrap();
    let asset_id = body["asset_id"].as_str().unwrap().to_owned();

    let job: (String, i16, serde_json::Value) =
        sqlx::query_as("SELECT kind, priority, payload FROM jobs WHERE dedup_key = $1")
            .bind(format!("meta:{asset_id}"))
            .fetch_one(server.db.pool())
            .await
            .expect("an extract_metadata job must be queued after finalization");
    assert_eq!(job.0, "extract_metadata");
    assert_eq!(job.1, 1, "JobPriority::High is worth 1");
    assert_eq!(job.2["asset_id"], asset_id);
}

/// Two uploads finalized concurrently in the same folder enqueue two
/// separate jobs: the dedup key is per-asset (not per-library like
/// rescan), so they don't collide with each other.
#[tokio::test]
async fn two_concurrent_finalizations_enqueue_two_separate_high_priority_jobs() {
    let server = TestServer::start().await;
    let (library_id, _root) = library_with_folder(&server, "concurrent-index").await;
    let folder_id = root_folder_id(&server, &library_id).await;
    let bytes_a = fs::read(tiny_fixture_path()).unwrap();
    let mut bytes_b = bytes_a.clone();
    bytes_b.push(0);

    let session_a = create_session(
        &server,
        &folder_id,
        "a.jpg",
        i64::try_from(bytes_a.len()).unwrap(),
        None,
    )
    .await;
    let session_b = create_session(
        &server,
        &folder_id,
        "b.jpg",
        i64::try_from(bytes_b.len()).unwrap(),
        None,
    )
    .await;

    let checksum_a = blake3_hex(&bytes_a);
    let checksum_b = blake3_hex(&bytes_b);
    let (response_a, response_b) = tokio::join!(
        patch_chunk(&server, &session_a, 0, &checksum_a, &bytes_a),
        patch_chunk(&server, &session_b, 0, &checksum_b, &bytes_b),
    );
    assert_eq!(response_a.status(), 201);
    assert_eq!(response_b.status(), 201);
    let asset_a = response_a.json::<serde_json::Value>().await.unwrap()["asset_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let asset_b = response_b.json::<serde_json::Value>().await.unwrap()["asset_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(asset_a, asset_b);

    let dedup_keys: Vec<String> = sqlx::query_scalar(
        "SELECT dedup_key FROM jobs WHERE kind = 'extract_metadata' AND priority = 1 \
         AND dedup_key = ANY($1) ORDER BY dedup_key",
    )
    .bind(vec![format!("meta:{asset_a}"), format!("meta:{asset_b}")])
    .fetch_all(server.db.pool())
    .await
    .unwrap();
    let mut expected = vec![format!("meta:{asset_a}"), format!("meta:{asset_b}")];
    expected.sort();
    assert_eq!(
        dedup_keys, expected,
        "two concurrent finalizations must enqueue two distinct jobs, not collide"
    );
}

/// Insufficient disk space for `expected_size` → rejected **at session
/// creation**, not discovered mid-upload.
#[tokio::test]
async fn insufficient_disk_space_is_rejected_at_session_creation() {
    let server = TestServer::start().await;
    let (library_id, _root) = library_with_folder(&server, "space").await;
    let folder_id = root_folder_id(&server, &library_id).await;

    let huge = 1_000_000_000_000_000_000_i64;
    let response = server
        .client
        .post(server.url("/api/v1/upload"))
        .json(&json!({
            "target_folder_id": folder_id,
            "filename": "gigante.mov",
            "expected_size": huge,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 507);
}
