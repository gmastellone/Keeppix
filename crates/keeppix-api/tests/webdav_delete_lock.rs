//! Task 8 (Fase 5): `DELETE`, `LOCK`, `UNLOCK` — completano `WebDAV`.
//!
//! `DELETE` passa sempre da `TrashRepo::choose(ctx, asset_id,
//! DiskAction::MovedToTrash)` — mai una cancellazione diretta sul
//! filesystem — e un editor senza il ruolo giusto riceve `403`, non un
//! successo silenzioso su una porta diversa da quella web (invariante di
//! `AGENTS.md`).
//!
//! `LOCK`/`UNLOCK` (Class 2) sono richiesti da Finder e Windows Explorer:
//! senza risposta a questi due metodi i client nativi si rifiutano di
//! scrivere, non solo "vanno più lento".
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use harness::TestServer;
use journey::{
    build_fixture_archive, create_library, create_user, folder_id_by_name, grant_folder_editor,
    login_as, scan_and_wait, setup_admin,
};
use serde_json::json;

fn basic_auth_header(username: &str, secret: &str) -> String {
    let raw = format!("{username}:{secret}");
    format!("Basic {}", STANDARD.encode(raw))
}

async fn create_app_password(client: &reqwest::Client, server: &TestServer, label: &str) -> String {
    let created = client
        .post(server.url("/api/v1/users/me/app-passwords"))
        .json(&json!({ "label": label }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let body: serde_json::Value = created.json().await.unwrap();
    body["secret"].as_str().unwrap().to_owned()
}

struct Fixture {
    album_a: String,
    secret: String,
}

/// Libreria con due cartelle (`album-a`, `album-b`), tre `tiny.jpg` ciascuna,
/// scansionata e indicizzata.
async fn setup_fixture(server: &TestServer) -> Fixture {
    setup_admin(server).await;
    let secret = create_app_password(&server.client, server, "Finder").await;
    let archive = build_fixture_archive(server);
    let library_id = create_library(server, "WebDavDeleteLockLib", &archive.root).await;
    let deadline = Instant::now() + Duration::from_secs(90);
    scan_and_wait(server, &library_id, archive.photo_count, deadline).await;
    let album_a = folder_id_by_name(server, "album-a").await;
    Fixture { album_a, secret }
}

async fn asset_id_by_name(server: &TestServer, folder_id: &str, filename: &str) -> String {
    let row: (uuid::Uuid,) =
        sqlx::query_as("SELECT id FROM assets WHERE folder_id = $1 AND filename = $2")
            .bind(uuid::Uuid::parse_str(folder_id).unwrap())
            .bind(filename)
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    row.0.to_string()
}

async fn dav_delete(server: &TestServer, path: &str, auth: &str) -> reqwest::Response {
    server
        .client
        .delete(server.url(path))
        .header("authorization", auth)
        .send()
        .await
        .unwrap()
}

async fn dav_lock(
    server: &TestServer,
    path: &str,
    auth: &str,
    depth: Option<&str>,
) -> reqwest::Response {
    let mut req = server
        .client
        .request(
            reqwest::Method::from_bytes(b"LOCK").expect("LOCK is a valid method token"),
            server.url(path),
        )
        .header("authorization", auth);
    if let Some(depth) = depth {
        req = req.header("depth", depth);
    }
    req.send().await.unwrap()
}

async fn dav_unlock(
    server: &TestServer,
    path: &str,
    auth: &str,
    lock_token_header: Option<&str>,
) -> reqwest::Response {
    let mut req = server
        .client
        .request(
            reqwest::Method::from_bytes(b"UNLOCK").expect("UNLOCK is a valid method token"),
            server.url(path),
        )
        .header("authorization", auth);
    if let Some(token) = lock_token_header {
        req = req.header("lock-token", token);
    }
    req.send().await.unwrap()
}

/// Estrae il token grezzo (senza `<`/`>`) dall'header `Lock-Token`.
fn token_from_header(response: &reqwest::Response) -> String {
    response
        .headers()
        .get("lock-token")
        .expect("Lock-Token header")
        .to_str()
        .unwrap()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_owned()
}

#[tokio::test]
async fn delete_asset_moves_it_to_trash_not_file_system_removal() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    let asset_id = asset_id_by_name(&server, &fixture.album_a, "photo-0.jpg").await;

    let original_path: String =
        sqlx::query_scalar("SELECT original_path FROM trash_entries WHERE asset_id = $1")
            .bind(uuid::Uuid::parse_str(&asset_id).unwrap())
            .fetch_optional(server.db.pool())
            .await
            .unwrap()
            .unwrap_or_default();
    assert!(
        original_path.is_empty(),
        "no trash_entries row must exist before the DELETE"
    );

    let response = dav_delete(&server, &format!("/dav/asset/{asset_id}"), &auth).await;
    assert_eq!(response.status(), 204);

    let entry: (String, Option<String>) =
        sqlx::query_as("SELECT disk_action, trash_path FROM trash_entries WHERE asset_id = $1")
            .bind(uuid::Uuid::parse_str(&asset_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .expect("a trash_entries row must exist after DELETE");
    assert_eq!(entry.0, "moved_to_trash");
    let trash_path = entry.1.expect("moved_to_trash always has a trash_path");
    assert!(
        std::path::Path::new(&trash_path).exists(),
        "the file must actually be on disk under .keeppix-trash, not just recorded in the database"
    );

    let status: String = sqlx::query_scalar("SELECT status FROM assets WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&asset_id).unwrap())
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(status, "trashed");
}

/// Il test più importante della spec: un editor (non owner/admin) che prova
/// `DELETE` riceve `403`, esattamente come `may_purge` in `trash.rs` — non
/// un successo silenzioso su una porta diversa dalla web app.
#[tokio::test]
async fn delete_by_editor_returns_403() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;

    let editor_id = create_user(&server, "editor", "editor-password-ok").await;
    grant_folder_editor(&server, &editor_id, &fixture.album_a).await;
    let editor_client = login_as(&server, "editor", "editor-password-ok").await;
    let editor_secret = create_app_password(&editor_client, &server, "Cyberduck").await;
    let auth = basic_auth_header("editor", &editor_secret);

    let asset_id = asset_id_by_name(&server, &fixture.album_a, "photo-0.jpg").await;

    let response = dav_delete(&server, &format!("/dav/asset/{asset_id}"), &auth).await;
    assert_eq!(response.status(), 403);

    let trashed_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM trash_entries WHERE asset_id = $1")
            .bind(uuid::Uuid::parse_str(&asset_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    assert_eq!(
        trashed_count, 0,
        "an editor's DELETE must never actually trash the asset"
    );

    let status: String = sqlx::query_scalar("SELECT status FROM assets WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&asset_id).unwrap())
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(status, "indexed", "the asset must remain untouched");
}

#[tokio::test]
async fn lock_and_unlock_work_and_unlock_rejects_expired_token() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    let path = format!("/dav/folder/{}", fixture.album_a);

    let first = dav_lock(&server, &path, &auth, Some("0")).await;
    assert_eq!(first.status(), 200);
    let first_token = token_from_header(&first);

    sqlx::query("UPDATE dav_locks SET timeout_at = now() - interval '1 hour' WHERE token = $1")
        .bind(&first_token)
        .execute(server.db.pool())
        .await
        .unwrap();

    // Un token scaduto è come se il lock non esistesse: un secondo LOCK
    // sullo stesso path deve riuscire di nuovo con un token nuovo.
    let second = dav_lock(&server, &path, &auth, Some("0")).await;
    assert_eq!(second.status(), 200);
    let second_token = token_from_header(&second);
    assert_ne!(first_token, second_token);

    // UNLOCK con il token scaduto: non trovato (attivo), 404 — non 200 su
    // un lock fantasma.
    let unlock_expired = dav_unlock(&server, &path, &auth, Some(&format!("<{first_token}>"))).await;
    assert_eq!(unlock_expired.status(), 404);

    // Il token valido (il secondo) invece si rilascia correttamente.
    let unlock_valid = dav_unlock(&server, &path, &auth, Some(&format!("<{second_token}>"))).await;
    assert_eq!(unlock_valid.status(), 204);
}

#[tokio::test]
async fn unlock_without_a_lock_token_header_returns_400() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    let path = format!("/dav/folder/{}", fixture.album_a);

    let response = dav_unlock(&server, &path, &auth, None).await;
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn class_2_lock_token_response_has_correct_headers() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);

    let response = dav_lock(
        &server,
        &format!("/dav/folder/{}", fixture.album_a),
        &auth,
        Some("0"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let content_type = response
        .headers()
        .get("content-type")
        .expect("Content-Type header")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        content_type.starts_with("application/xml"),
        "unexpected Content-Type: {content_type}"
    );
    let lock_token = response
        .headers()
        .get("lock-token")
        .expect("Lock-Token header")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        lock_token.starts_with("<opaquelocktoken:") && lock_token.ends_with('>'),
        "unexpected Lock-Token: {lock_token}"
    );

    let body = response.text().await.unwrap();
    assert!(body.contains("<D:lockdiscovery>"));
    assert!(body.contains("opaquelocktoken:"));
}
