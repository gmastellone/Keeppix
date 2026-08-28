//! `DELETE`, `LOCK`, `UNLOCK` — complete `WebDAV`.
//!
//! `DELETE` always goes through `TrashRepo::choose(ctx, asset_id,
//! DiskAction::MovedToTrash)` — never a direct filesystem deletion — and
//! an editor without the right role gets `403`, not a silent success
//! through a door different from the web one.
//!
//! `LOCK`/`UNLOCK` (Class 2) are required by Finder and Windows Explorer:
//! without a response to these two methods, native clients refuse to
//! write at all, not just "go slower".
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use harness::TestServer;
use journey::{
    build_fixture_archive, create_library, create_user, folder_id_by_name, grant_folder_editor,
    login_as, scan_and_wait, setup_admin, tiny_fixture_path,
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

/// A library with two folders (`album-a`, `album-b`), three `tiny.jpg`
/// each, scanned and indexed.
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

/// `LOCK` with an `If: (<token>)` header — the renewal path (RFC 4918
/// §10.4), not the creation one used by [`dav_lock`].
async fn dav_lock_with_if_token(
    server: &TestServer,
    path: &str,
    auth: &str,
    if_token: &str,
) -> reqwest::Response {
    server
        .client
        .request(
            reqwest::Method::from_bytes(b"LOCK").expect("LOCK is a valid method token"),
            server.url(path),
        )
        .header("authorization", auth)
        .header("if", format!("(<{if_token}>)"))
        .send()
        .await
        .unwrap()
}

async fn dav_propfind(
    server: &TestServer,
    path: &str,
    auth: &str,
    depth: &str,
) -> reqwest::Response {
    server
        .client
        .request(
            reqwest::Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid method token"),
            server.url(path),
        )
        .header("authorization", auth)
        .header("depth", depth)
        .send()
        .await
        .unwrap()
}

/// A library with **one** folder (`only-album`) and two `tiny.jpg` —
/// unlike [`setup_fixture`] (two folders, three assets each): the folder
/// `DELETE` test wants a small, exact asset count to verify one by one in
/// the trash.
async fn setup_two_asset_folder_fixture(server: &TestServer) -> (String, String) {
    setup_admin(server).await;
    let secret = create_app_password(&server.client, server, "Finder").await;

    let root = server
        .photos_root
        .join(format!("delete-archive-{}", uuid::Uuid::now_v7().simple()));
    let dir = root.join("only-album");
    std::fs::create_dir_all(&dir).unwrap();
    let bytes = std::fs::read(tiny_fixture_path()).unwrap();
    for i in 0..2 {
        std::fs::write(dir.join(format!("photo-{i}.jpg")), &bytes).unwrap();
    }

    let library_id = create_library(server, "WebDavFolderDeleteLib", &root).await;
    let deadline = Instant::now() + Duration::from_secs(90);
    scan_and_wait(server, &library_id, 2, deadline).await;
    let folder_id = folder_id_by_name(server, "only-album").await;
    (folder_id, secret)
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

/// Extracts the raw token (without `<`/`>`) from the `Lock-Token` header.
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

/// The most important test here: an editor (not owner/admin) who tries
/// `DELETE` gets `403`, exactly like `may_purge` in `trash.rs` — not a
/// silent success through a door different from the web app.
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

    // An expired token is as if the lock didn't exist: a second LOCK on
    // the same path must succeed again with a new token.
    let second = dav_lock(&server, &path, &auth, Some("0")).await;
    assert_eq!(second.status(), 200);
    let second_token = token_from_header(&second);
    assert_ne!(first_token, second_token);

    // UNLOCK with the expired token: not found (active), 404 — not 200 on
    // a phantom lock.
    let unlock_expired = dav_unlock(&server, &path, &auth, Some(&format!("<{first_token}>"))).await;
    assert_eq!(unlock_expired.status(), 404);

    // The valid token (the second one), on the other hand, releases correctly.
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

/// `DELETE /dav/folder/{id}` is recursive: every asset in the folder must
/// end up in `trash_entries` with its file moved under
/// `.keeppix-trash/` (never a direct `rm`), and the `folders` row must
/// disappear — verified both via SQL and with a subsequent `PROPFIND`,
/// which must answer `403` (a missing id must never become an existence
/// oracle, not even for the admin: see the note further below on why
/// it's not `404`).
#[tokio::test]
async fn folder_delete_moves_all_assets_to_trash_and_removes_folder() {
    let server = TestServer::start().await;
    let (folder_id, secret) = setup_two_asset_folder_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &secret);

    let mut asset_ids = Vec::new();
    for i in 0..2 {
        asset_ids.push(asset_id_by_name(&server, &folder_id, &format!("photo-{i}.jpg")).await);
    }

    let response = dav_delete(&server, &format!("/dav/folder/{folder_id}"), &auth).await;
    assert_eq!(response.status(), 204);

    for asset_id in &asset_ids {
        let entry: (String, Option<String>) =
            sqlx::query_as("SELECT disk_action, trash_path FROM trash_entries WHERE asset_id = $1")
                .bind(uuid::Uuid::parse_str(asset_id).unwrap())
                .fetch_one(server.db.pool())
                .await
                .expect("every asset of the deleted folder must have a trash_entries row");
        assert_eq!(entry.0, "moved_to_trash");
        let trash_path = entry.1.expect("moved_to_trash always has a trash_path");
        assert!(
            trash_path.contains(".keeppix-trash"),
            "the file must be moved under .keeppix-trash, not deleted directly: {trash_path}"
        );
        assert!(
            std::path::Path::new(&trash_path).exists(),
            "the file must actually be on disk under .keeppix-trash: {trash_path}"
        );

        // Discovery: unlike `DELETE /dav/asset/{id}` (where the `assets`
        // row stays with `status = 'trashed'`), here the `assets` row
        // disappears along with the folder — `assets.folder_id
        // REFERENCES folders(id) ON DELETE CASCADE` (migration 0005) also
        // deletes assets that are already "trashed" once
        // `folder_repo.delete_subtree` removes the `folders` row. The
        // only trace left is `trash_entries` (which has no FK to
        // `assets`, migration 0014) — which is why the assertions above
        // are enough to prove "trashed, not deleted": the physical file
        // and the audit trail survive, the `assets` row does not.
        let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE id = $1")
            .bind(uuid::Uuid::parse_str(asset_id).unwrap())
            .fetch_one(server.db.pool())
            .await
            .unwrap();
        assert_eq!(
            remaining, 0,
            "the assets row is cascaded away with its folder — trash_entries above is the only \
             surviving record, and it must have already proven the file was moved, not deleted"
        );
    }

    let folder_count: i64 = sqlx::query_scalar("SELECT count(*) FROM folders WHERE id = $1")
        .bind(uuid::Uuid::parse_str(&folder_id).unwrap())
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    assert_eq!(folder_count, 0, "the folders row must be gone after DELETE");

    // `403`, not `404`: the `WebDAV` dispatcher (`dav::mod::handler`)
    // always builds `AuthContext::user(user_id, SystemRole::User)`, never
    // `SystemRole::Admin`, regardless of the user's actual role
    // (`ctx.is_admin()` is never true for a WebDAV actor — see the
    // comment on this ruling in `dav::mod`). `FolderRepo::visible`
    // answers `NotFound` only for an admin, `Forbidden` for everyone
    // else: here it's always the latter case, even for giovanni.
    let after = dav_propfind(&server, &format!("/dav/folder/{folder_id}"), &auth, "0").await;
    assert_eq!(
        after.status(),
        403,
        "a PROPFIND on the just-deleted folder must not find it — WebDAV auth is never treated \
         as admin, so a missing folder is 403, not 404 (see FolderRepo::visible)"
    );
}

/// A second `LOCK` (without `If:`) on a resource already locked by
/// another active token must answer `423 Locked` — never a second lock
/// that would end up making two clients believe they own the same lock.
#[tokio::test]
async fn locking_an_already_locked_resource_returns_423() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    let path = format!("/dav/folder/{}", fixture.album_a);

    let first = dav_lock(&server, &path, &auth, Some("0")).await;
    assert_eq!(first.status(), 200);

    let second = dav_lock(&server, &path, &auth, Some("0")).await;
    assert_eq!(
        second.status(),
        423,
        "a second LOCK on an already-locked resource must be rejected, not silently granted"
    );
}

/// A renewal `LOCK` (`If: (<token>)`) on a token that's already expired
/// must never answer `200` — it would make the client believe it still
/// owns the lock. The code answers `412 Precondition Failed`
/// (`dav::lock::lock`), but either `412` or `404` is an acceptable
/// contract: this checks the actual behavior without pinning the test to
/// a non-contractual implementation detail.
#[tokio::test]
async fn lock_with_expired_if_token_returns_412_or_404() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    let path = format!("/dav/folder/{}", fixture.album_a);

    let first = dav_lock(&server, &path, &auth, Some("0")).await;
    assert_eq!(first.status(), 200);
    let token = token_from_header(&first);

    sqlx::query("UPDATE dav_locks SET timeout_at = now() - interval '1 hour' WHERE token = $1")
        .bind(&token)
        .execute(server.db.pool())
        .await
        .unwrap();

    let response = dav_lock_with_if_token(&server, &path, &auth, &token).await;
    let status = response.status();
    assert!(
        status == 412 || status == 404,
        "expected 412 or 404 for a LOCK renewal with an expired If: token, got {status}"
    );
}
