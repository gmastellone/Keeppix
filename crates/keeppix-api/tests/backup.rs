#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use harness::TestServer;
use serde_json::json;

async fn setup_admin(server: &TestServer) {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
}

#[tokio::test]
async fn preferences_include_originals_warning_when_originals_excluded() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let response = server
        .client
        .get(server.url("/api/v1/backup/preferences"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["include_originals"], false);
    assert_eq!(body["originals_warning"], true);
    assert!(
        body["originals_warning_message"]
            .as_str()
            .unwrap()
            .contains("does NOT contain your photos")
    );
}

#[tokio::test]
async fn restore_rejects_backup_newer_than_installed() {
    use keeppix_jobs::backup::{BackupIncludes, BackupManifest, pack_kpxb};
    use std::fs;

    let server = TestServer::start().await;
    setup_admin(&server).await;

    let staging = server.data_dir.join("newer-staging");
    fs::create_dir_all(&staging).unwrap();
    let manifest = BackupManifest {
        format: "kpxb/1".into(),
        keeppix_version: "99.0.0".into(),
        created_at: chrono::Utc::now().to_rfc3339(),
        includes: BackupIncludes {
            database: false,
            config: false,
            maps: true,
            sidecars: false,
            derivatives: false,
            originals: false,
        },
        counts: json!({}),
        checksums: serde_json::Map::new(),
    };
    fs::write(
        staging.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(staging.join("maps.json"), b"[]").unwrap();
    let kpxb = server.data_dir.join("newer.kpxb");
    pack_kpxb(&staging, "secret-pass", &kpxb).unwrap();

    let response = server
        .client
        .post(server.url("/api/v1/restore/inspect"))
        .json(&json!({
            "path": kpxb.to_string_lossy(),
            "passphrase": "secret-pass"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["rejected"], true);

    let restore = server
        .client
        .post(server.url("/api/v1/restore"))
        .json(&json!({
            "path": kpxb.to_string_lossy(),
            "passphrase": "secret-pass",
            "components": { "database": false, "config": false, "maps": true },
            "dry_run": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(restore.status(), 400);
    let err: serde_json::Value = restore.json().await.unwrap();
    assert_eq!(err["type"], "keeppix/backup-too-new");
}
