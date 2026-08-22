#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::{LibraryRepo, SettingsRepo};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};
use keeppix_jobs::discover;
use keeppix_jobs::watch::{self, WatcherMode};

#[tokio::test]
async fn persist_capabilities_writes_the_measured_probe_result() {
    let test = TestDb::start().await;
    let expected = keeppix_media::probe();
    watch::persist_capabilities(test.db()).await.unwrap();
    let value = SettingsRepo::new(test.db())
        .get_json("capabilities")
        .await
        .unwrap()
        .expect("capabilities");
    assert_eq!(value["backend"], serde_json::json!(expected.backend));
    let status = value["extra"]["ai"]["inference_status"]
        .as_str()
        .expect("inference_status");
    assert!(
        matches!(status, "ok" | "model_missing" | "failed"),
        "persisted capabilities must keep a concrete AI probe status, got {status}"
    );
    assert_eq!(
        value["extra"]["ai"]["inference_status"], expected.extra["ai"]["inference_status"],
        "persisted status must match the live probe"
    );
    assert!(
        value["extra"]["ai"]["free_ram_bytes"].as_u64().unwrap() > 0,
        "persist_capabilities must store the AI host probe, not an empty extra"
    );
    assert_eq!(
        value["extra"]["ai"]["cpu_cores"], expected.extra["ai"]["cpu_cores"],
        "cpu_cores is stable across two probe calls in the same process"
    );
    assert_eq!(
        value["decode_fps"].is_null(),
        expected.decode_fps.is_none(),
        "persisted capabilities must preserve whether the probe measured fps"
    );

    let loaded = watch::load_ai_host_facts(test.db())
        .await
        .unwrap()
        .expect("ai facts via get_json");
    assert!(loaded.free_ram_bytes > 0);
    assert!(loaded.cpu_cores >= 1);
    assert_eq!(loaded.inference_status, status);
}

#[tokio::test]
async fn writing_a_jpeg_after_debounce_creates_an_asset() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-wch-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();

    assert_eq!(watch::mode_for(&root), WatcherMode::Native);
    let handle = watch::spawn(
        test.db().clone(),
        library.id,
        root.clone(),
        Duration::from_millis(80),
        WatcherMode::Native,
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    sqlx::query("UPDATE jobs SET status = 'done' WHERE kind = 'discover_library'")
        .execute(test.db().pool())
        .await
        .unwrap();

    let tiny = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.jpg");
    fs::copy(&tiny, root.join("foto.jpg")).unwrap();

    let start = std::time::Instant::now();
    loop {
        assert!(
            start.elapsed() < Duration::from_secs(8),
            "watcher did not enqueue discover"
        );
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM jobs WHERE kind = 'discover_library' AND status = 'pending'",
        )
        .fetch_one(test.db().pool())
        .await
        .unwrap();
        if n > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    handle.abort();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(n, 1);
}
