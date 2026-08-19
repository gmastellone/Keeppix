//! Task 6 (Fase 5): `PROPFIND` risponde dal database (mai da `stat()` sul
//! filesystem), `GET` supporta `Range`. `Depth: infinity` è rifiutato con
//! `403` per non esplodere la RAM; una cartella senza permesso restituisce
//! `403`, mai una lista vuota o `404` — non deve diventare un oracolo di
//! esistenza.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use harness::TestServer;
use journey::{
    build_fixture_archive, create_library, create_user, folder_id_by_name, login_as, scan_and_wait,
    setup_admin, tiny_fixture_path,
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

async fn propfind(server: &TestServer, path: &str, auth: &str, depth: &str) -> reqwest::Response {
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

/// Estrae il primo id `/dav/asset/{id}` dal corpo del `multistatus`: un
/// UUID è sempre 36 caratteri, che il PROPFIND appena eseguito ha appena
/// prodotto lui stesso.
fn first_asset_id(body: &str) -> String {
    let marker = "/dav/asset/";
    let start = body.find(marker).expect("an asset href in the body") + marker.len();
    body[start..start + 36].to_owned()
}

/// Libreria con due cartelle (`album-a`, `album-b`), tre `tiny.jpg` ciascuna
/// (vedi `journey::build_fixture_archive`), scansionata e indicizzata.
/// Restituisce l'id di `album-a` e il segreto di un'app-password
/// dell'amministratore autenticato da `setup_admin`.
async fn setup_fixture(server: &TestServer) -> (String, String) {
    setup_admin(server).await;
    let secret = create_app_password(&server.client, server, "Finder").await;
    let archive = build_fixture_archive(server);
    let library_id = create_library(server, "WebDavLib", &archive.root).await;
    let deadline = Instant::now() + Duration::from_secs(90);
    scan_and_wait(server, &library_id, archive.photo_count, deadline).await;
    let folder_id = folder_id_by_name(server, "album-a").await;
    (folder_id, secret)
}

#[tokio::test]
async fn propfind_depth_1_on_a_folder_returns_207_with_child_resources() {
    let server = TestServer::start().await;
    let (folder_id, secret) = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &secret);

    let response = propfind(&server, &format!("/dav/folder/{folder_id}"), &auth, "1").await;

    assert_eq!(response.status(), 207);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/xml; charset=\"utf-8\""
    );
    let body = response.text().await.unwrap();
    assert!(
        body.contains(&format!("/dav/folder/{folder_id}/")),
        "must include the requested folder's own href: {body}"
    );
    assert!(
        body.contains("/dav/asset/"),
        "must include a child asset href: {body}"
    );
    assert!(
        body.contains("photo-0.jpg"),
        "must include the child's display name: {body}"
    );
}

#[tokio::test]
async fn propfind_depth_infinity_is_rejected_with_403() {
    let server = TestServer::start().await;
    let (folder_id, secret) = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &secret);

    let response = propfind(
        &server,
        &format!("/dav/folder/{folder_id}"),
        &auth,
        "infinity",
    )
    .await;

    assert_eq!(response.status(), 403);
}

#[tokio::test]
async fn propfind_on_a_folder_without_permission_returns_403() {
    let server = TestServer::start().await;
    let (folder_id, _admin_secret) = setup_fixture(&server).await;

    create_user(&server, "outsider", "outsider-password-ok").await;
    let outsider = login_as(&server, "outsider", "outsider-password-ok").await;
    let outsider_secret = create_app_password(&outsider, &server, "Cyberduck").await;
    let auth = basic_auth_header("outsider", &outsider_secret);

    let response = propfind(&server, &format!("/dav/folder/{folder_id}"), &auth, "1").await;

    assert_eq!(response.status(), 403);
    let body = response.text().await.unwrap();
    assert!(
        !body.contains("multistatus"),
        "a folder the caller cannot see must never leak a listing, not even an empty one: {body}"
    );
}

#[tokio::test]
async fn get_asset_without_range_returns_200_full_content() {
    let server = TestServer::start().await;
    let (folder_id, secret) = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &secret);

    let listing = propfind(&server, &format!("/dav/folder/{folder_id}"), &auth, "1").await;
    let asset_id = first_asset_id(&listing.text().await.unwrap());

    let response = server
        .client
        .get(server.url(&format!("/dav/asset/{asset_id}")))
        .header("authorization", &auth)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let expected = std::fs::read(tiny_fixture_path()).unwrap();
    let actual = response.bytes().await.unwrap();
    assert_eq!(actual.as_ref(), expected.as_slice());
}

#[tokio::test]
async fn get_asset_with_range_returns_206() {
    let server = TestServer::start().await;
    let (folder_id, secret) = setup_fixture(&server).await;
    let auth = basic_auth_header("giovanni", &secret);

    let listing = propfind(&server, &format!("/dav/folder/{folder_id}"), &auth, "1").await;
    let asset_id = first_asset_id(&listing.text().await.unwrap());

    let response = server
        .client
        .get(server.url(&format!("/dav/asset/{asset_id}")))
        .header("authorization", &auth)
        .header("range", "bytes=0-9")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 206);
    let content_range = response
        .headers()
        .get("content-range")
        .expect("content-range header")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        content_range.starts_with("bytes 0-9/"),
        "unexpected Content-Range: {content_range}"
    );
    let actual = response.bytes().await.unwrap();
    assert_eq!(actual.len(), 10);
    let expected = std::fs::read(tiny_fixture_path()).unwrap();
    assert_eq!(actual.as_ref(), &expected[0..10]);
}
