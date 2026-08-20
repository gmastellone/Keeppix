//! Task 7 (Fase 5): `PUT`, `MKCOL`, `MOVE` — le operazioni di scrittura
//! `WebDAV`. `COPY` non è implementata (resta `501`, vedi il ledger del
//! Task 7): nessun test qui la esercita, come esplicitamente permesso dal
//! brief.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use harness::TestServer;
use journey::{
    build_fixture_archive, create_library, create_user, folder_id_by_name, grant_folder_viewer,
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
    album_b: String,
    /// Radice della libreria su disco (`root/album-a`, `root/album-b`).
    root: PathBuf,
    secret: String,
}

/// Libreria con due cartelle (`album-a`, `album-b`), tre `tiny.jpg` ciascuna
/// (vedi `journey::build_fixture_archive`), scansionata e indicizzata.
async fn setup_fixture(server: &TestServer) -> Fixture {
    setup_admin(server).await;
    let secret = create_app_password(&server.client, server, "Finder").await;
    let archive = build_fixture_archive(server);
    let library_id = create_library(server, "WebDavWriteLib", &archive.root).await;
    let deadline = Instant::now() + Duration::from_secs(90);
    scan_and_wait(server, &library_id, archive.photo_count, deadline).await;
    let album_a = folder_id_by_name(server, "album-a").await;
    let album_b = folder_id_by_name(server, "album-b").await;
    Fixture {
        album_a,
        album_b,
        root: archive.root,
        secret,
    }
}

async fn mkcol(server: &TestServer, path: &str, auth: &str) -> reqwest::Response {
    server
        .client
        .request(
            reqwest::Method::from_bytes(b"MKCOL").expect("MKCOL is a valid method token"),
            server.url(path),
        )
        .header("authorization", auth)
        .send()
        .await
        .unwrap()
}

async fn put(server: &TestServer, path: &str, auth: &str, bytes: &[u8]) -> reqwest::Response {
    server
        .client
        .put(server.url(path))
        .header("authorization", auth)
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap()
}

/// Come [`put`], ma con un `Content-Length` dichiarato a mano — diverso dal
/// corpo effettivo che `reqwest` manda per davvero (`bytes`). Serve a
/// esercitare la guardia sulla dimensione dichiarata (fix round Task 7,
/// Important #1) senza dover davvero spedire terabyte di corpo: il server
/// deve rispondere guardando solo l'header, prima di leggere un solo byte.
async fn put_with_declared_length(
    server: &TestServer,
    path: &str,
    auth: &str,
    bytes: &[u8],
    declared_length: u64,
) -> reqwest::Response {
    server
        .client
        .put(server.url(path))
        .header("authorization", auth)
        .header("content-length", declared_length.to_string())
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap()
}

async fn move_folder(
    server: &TestServer,
    path: &str,
    auth: &str,
    destination: &str,
) -> reqwest::Response {
    server
        .client
        .request(
            reqwest::Method::from_bytes(b"MOVE").expect("MOVE is a valid method token"),
            server.url(path),
        )
        .header("authorization", auth)
        .header("destination", destination)
        .send()
        .await
        .unwrap()
}

/// `parent_id` della cartella secondo `/api/v1/folders/tree` (sessione
/// admin già nel cookie store di `server.client`, impostata da
/// `setup_admin`).
async fn folder_parent_id(server: &TestServer, folder_id: &str) -> Option<String> {
    let tree: serde_json::Value = server
        .client
        .get(server.url("/api/v1/folders/tree"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    tree.as_array()
        .expect("tree array")
        .iter()
        .find(|f| f["id"].as_str() == Some(folder_id))
        .and_then(|f| f["parent_id"].as_str())
        .map(str::to_owned)
}

async fn asset_count_in_folder(server: &TestServer, folder_id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM assets WHERE folder_id = $1")
        .bind(uuid::Uuid::parse_str(folder_id).unwrap())
        .fetch_one(server.db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn mkcol_creates_a_subfolder_and_returns_201() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);

    let response = mkcol(
        &server,
        &format!("/dav/folder/{}/2024", fixture.album_a),
        &auth,
    )
    .await;

    assert_eq!(response.status(), 201);
    let location = response
        .headers()
        .get("location")
        .expect("Location header")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        location.starts_with("/dav/folder/"),
        "unexpected Location: {location}"
    );
    let new_id = location.trim_start_matches("/dav/folder/");

    assert_eq!(
        folder_parent_id(&server, new_id).await,
        Some(fixture.album_a.clone())
    );
    assert!(
        fixture.root.join("album-a").join("2024").is_dir(),
        "MKCOL must also create the directory on disk"
    );
}

#[tokio::test]
async fn mkcol_by_a_viewer_returns_403() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;

    let outsider_id = create_user(&server, "outsider", "outsider-password-ok").await;
    grant_folder_viewer(&server, &outsider_id, &fixture.album_a).await;
    let outsider = login_as(&server, "outsider", "outsider-password-ok").await;
    let outsider_secret = create_app_password(&outsider, &server, "Cyberduck").await;
    let auth = basic_auth_header("outsider", &outsider_secret);

    let response = mkcol(
        &server,
        &format!("/dav/folder/{}/should-not-exist", fixture.album_a),
        &auth,
    )
    .await;

    assert_eq!(response.status(), 403);
    assert!(
        !fixture
            .root
            .join("album-a")
            .join("should-not-exist")
            .exists()
    );
}

#[tokio::test]
async fn mkcol_disk_failure_leaves_no_phantom_folder_row() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);

    // Un file omonimo già sul disco impedisce a `create_dir_all` di
    // riuscire (errore di I/O reale, non un `NotFound`/permessi che root
    // potrebbe bypassare in CI) — ordine invertito (fix round Task 7,
    // Important #2): il DB non deve mai vedere una riga per una cartella
    // che non esiste sul disco.
    let colliding_path = fixture.root.join("album-a").join("blocked");
    std::fs::write(&colliding_path, b"not a directory").unwrap();

    let response = mkcol(
        &server,
        &format!("/dav/folder/{}/blocked", fixture.album_a),
        &auth,
    )
    .await;

    assert_eq!(response.status(), 500);

    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM folders WHERE parent_id = $1 AND name = 'blocked'",
    )
    .bind(uuid::Uuid::parse_str(&fixture.album_a).unwrap())
    .fetch_one(server.db.pool())
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "a disk failure on MKCOL must never leave a phantom folders row"
    );
}

#[tokio::test]
async fn put_creates_an_asset_and_enqueues_high_priority_indexing() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    let bytes = std::fs::read(tiny_fixture_path()).unwrap();

    let response = put(
        &server,
        &format!("/dav/folder/{}/new-photo.jpg", fixture.album_a),
        &auth,
        &bytes,
    )
    .await;

    assert_eq!(response.status(), 201);
    let location = response
        .headers()
        .get("location")
        .expect("Location header")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        location.starts_with("/dav/asset/"),
        "unexpected Location: {location}"
    );
    let asset_id = location.trim_start_matches("/dav/asset/").to_owned();

    let on_disk = fixture.root.join("album-a").join("new-photo.jpg");
    assert_eq!(std::fs::read(on_disk).unwrap(), bytes);

    // Stesso punto del brief di Task 1: nessuno dei 15 minuti del watcher,
    // un job `extract_metadata` ad alta priorità deve essere già in coda.
    let job: (String, i16, serde_json::Value) =
        sqlx::query_as("SELECT kind, priority, payload FROM jobs WHERE dedup_key = $1")
            .bind(format!("meta:{asset_id}"))
            .fetch_one(server.db.pool())
            .await
            .expect("a high-priority extract_metadata job must be enqueued after PUT");
    assert_eq!(job.0, "extract_metadata");
    assert_eq!(job.1, 1, "JobPriority::High vale 1");
    assert_eq!(job.2["asset_id"], asset_id);
}

#[tokio::test]
async fn put_of_same_content_skips_without_creating_a_second_file() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    // `photo-0.jpg` esiste già in `album-a` con esattamente questo contenuto
    // (vedi `journey::build_fixture_archive`).
    let bytes = std::fs::read(tiny_fixture_path()).unwrap();

    let response = put(
        &server,
        &format!("/dav/folder/{}/photo-0.jpg", fixture.album_a),
        &auth,
        &bytes,
    )
    .await;

    assert_eq!(response.status(), 204);
    assert!(
        response.headers().get("location").is_none(),
        "a skipped duplicate must not advertise a Location"
    );
    assert_eq!(
        asset_count_in_folder(&server, &fixture.album_a).await,
        3,
        "a duplicate PUT must never create a second asset row"
    );
    assert!(
        !fixture.root.join("album-a").join("photo-0_1.jpg").exists(),
        "a duplicate PUT must never write a second file"
    );
}

#[tokio::test]
async fn put_of_same_name_different_content_saves_with_suffix() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    let original = std::fs::read(tiny_fixture_path()).unwrap();
    let mut different = original.clone();
    different.push(0);

    let response = put(
        &server,
        &format!("/dav/folder/{}/photo-0.jpg", fixture.album_a),
        &auth,
        &different,
    )
    .await;

    assert_eq!(response.status(), 201);
    let location = response
        .headers()
        .get("location")
        .expect("Location header")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(location.starts_with("/dav/asset/"));

    // Mai una sovrascrittura silenziosa (AGENTS.md): il file originale resta
    // intatto, il nuovo prende un suffisso numerico.
    assert_eq!(
        std::fs::read(fixture.root.join("album-a").join("photo-0.jpg")).unwrap(),
        original,
        "the pre-existing file must never be silently overwritten"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("album-a").join("photo-0_1.jpg")).unwrap(),
        different
    );
    assert_eq!(asset_count_in_folder(&server, &fixture.album_a).await, 4);
}

#[tokio::test]
async fn put_with_a_declared_content_length_over_the_limit_returns_413() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    let bytes = std::fs::read(tiny_fixture_path()).unwrap();

    // Il corpo vero è minuscolo (`tiny.jpg`); l'header dichiara 100 TiB,
    // ben oltre `MAX_BODY_BYTES` (10 GiB) — il server deve respingerlo
    // guardando solo `Content-Length`, senza mai scrivere un file.
    let response = put_with_declared_length(
        &server,
        &format!("/dav/folder/{}/too-big.jpg", fixture.album_a),
        &auth,
        &bytes,
        100 * 1024 * 1024 * 1024 * 1024,
    )
    .await;

    assert_eq!(response.status(), 413);
    assert!(
        !fixture.root.join("album-a").join("too-big.jpg").exists(),
        "a PUT rejected for its declared size must never write a file to disk"
    );
    assert_eq!(
        asset_count_in_folder(&server, &fixture.album_a).await,
        3,
        "a rejected PUT must never create an asset row"
    );
}

#[tokio::test]
async fn put_of_dotfile_saves_on_disk_but_does_not_index() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);
    let bytes = b"macOS Finder metadata cache, not a photo".to_vec();

    // La scansione iniziale della fixture ha già accodato 6 job
    // `extract_metadata` (uno a foto): si confronta il conteggio prima/dopo,
    // non uno zero assoluto.
    let jobs_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'extract_metadata'")
            .fetch_one(server.db.pool())
            .await
            .unwrap();

    let response = put(
        &server,
        &format!("/dav/folder/{}/.DS_Store", fixture.album_a),
        &auth,
        &bytes,
    )
    .await;

    assert_eq!(response.status(), 204);
    assert_eq!(
        std::fs::read(fixture.root.join("album-a").join(".DS_Store")).unwrap(),
        bytes,
        "a dotfile must still be written to disk"
    );

    let indexed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM assets WHERE folder_id = $1 AND filename = '.DS_Store'",
    )
    .bind(uuid::Uuid::parse_str(&fixture.album_a).unwrap())
    .fetch_one(server.db.pool())
    .await
    .unwrap();
    assert_eq!(indexed, 0, "a dotfile must never become an indexed asset");

    let jobs_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'extract_metadata'")
            .fetch_one(server.db.pool())
            .await
            .unwrap();
    assert_eq!(
        jobs_after, jobs_before,
        "a dotfile must never enqueue indexing"
    );
}

#[tokio::test]
async fn move_folder_changes_its_parent() {
    let server = TestServer::start().await;
    let fixture = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &fixture.secret);

    let response = move_folder(
        &server,
        &format!("/dav/folder/{}", fixture.album_b),
        &auth,
        &format!("/dav/folder/{}/album-b", fixture.album_a),
    )
    .await;

    assert_eq!(response.status(), 204);
    assert_eq!(
        folder_parent_id(&server, &fixture.album_b).await,
        Some(fixture.album_a.clone())
    );
    assert!(
        fixture.root.join("album-a").join("album-b").is_dir(),
        "MOVE must also relocate the directory on disk"
    );
    assert!(!fixture.root.join("album-b").exists());
}
