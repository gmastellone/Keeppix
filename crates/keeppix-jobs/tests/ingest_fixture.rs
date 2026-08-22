#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::{AssetRepo, JobRepo, LibraryRepo};
use keeppix_domain::{AuthContext, JobKind, JobPriority, NewLibrary, SystemRole};
use keeppix_jobs::{ActivityTracker, IngestHandler, WorkerPool};

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn ingest_fixture_indexes_three_jpegs() {
    let test = TestDb::start().await;
    let tiny = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.jpg");
    let root = std::env::temp_dir().join(format!("kpx-fix-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(root.join("album")).unwrap();
    let bytes = fs::read(&tiny).unwrap();
    fs::write(root.join("a.jpg"), &bytes).unwrap();
    write_whatsapp_or(root.join("b.jpg"), &bytes);
    fs::write(root.join("album").join("c.jpg"), &bytes).unwrap();
    fs::write(root.join(".DS_Store"), b"junk").unwrap();
    let orig_mtime = fs::metadata(root.join("a.jpg"))
        .unwrap()
        .modified()
        .unwrap();

    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Fixture".to_owned(),
                owner_id: admin,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();

    let data_dir = root.join(".keeppix-data");
    JobRepo::new(test.db())
        .enqueue(
            JobKind::DiscoverLibrary,
            serde_json::json!({ "library_id": library.id.to_string() }),
            JobPriority::Background,
            Some(&format!("discover:{}", library.id)),
        )
        .await
        .unwrap();

    let tracker = std::sync::Arc::new(ActivityTracker::new());
    let handler = IngestHandler {
        db: test.db().clone(),
        data_dir: data_dir.clone(),
        stability_wait: Duration::ZERO,
        trash_retention_days: keeppix_db::TRASH_RETENTION_DAYS,
        database_url: test.database_url().to_owned(),
        config_path: None,
        activity: tracker.clone(),
    };
    let night = keeppix_jobs::default_night_window();
    let pool = WorkerPool::new(
        test.db().clone(),
        handler,
        tracker,
        512 * 1024 * 1024,
        night,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );

    let start = std::time::Instant::now();
    loop {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "ingest fixture timed out"
        );
        if !pool.step().await.unwrap() {
            let pending: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM jobs WHERE status IN ('pending','running')",
            )
            .fetch_one(test.db().pool())
            .await
            .unwrap();
            if pending == 0 {
                break;
            }
        }
    }

    let n = AssetRepo::new(test.db())
        .count_in_library(library.id)
        .await
        .unwrap();
    assert_eq!(n, 3);
    let indexed: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE status = 'indexed'")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(indexed, 3);
    let thumbs: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE thumbhash IS NOT NULL")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(thumbs, 3);
    assert!(data_dir.join("derivatives").exists());
    assert_eq!(
        fs::metadata(root.join("a.jpg"))
            .unwrap()
            .modified()
            .unwrap(),
        orig_mtime
    );
    let names: Vec<String> = sqlx::query_scalar("SELECT filename FROM assets ORDER BY filename")
        .fetch_all(test.db().pool())
        .await
        .unwrap();
    assert!(!names.iter().any(|n| n == ".DS_Store"));

    let meta_ms = {
        let t = std::time::Instant::now();
        keeppix_media::read_exif(
            &root.join("a.jpg"),
            chrono::DateTime::<chrono::Utc>::from(orig_mtime),
        )
        .unwrap();
        t.elapsed().as_millis()
    };
    let hash: Vec<u8> =
        sqlx::query_scalar("SELECT content_hash FROM assets WHERE filename = 'a.jpg'")
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    let mut hash_arr = [0u8; 32];
    hash_arr.copy_from_slice(&hash);
    let derive_dir = root.join("timing-data");
    let derive_ms = {
        let t = std::time::Instant::now();
        keeppix_media::derive_jpeg(&root.join("a.jpg"), &derive_dir, &hash_arr).unwrap();
        t.elapsed().as_millis()
    };
    eprintln!(
        "ingest_fixture elapsed_ms={} per_file_ms={} metadata_ms={} derive_ms={}",
        start.elapsed().as_millis(),
        start.elapsed().as_millis() / 3,
        meta_ms,
        derive_ms
    );
    let _ = fs::remove_dir_all(&root);
}

fn write_whatsapp_or(path: std::path::PathBuf, fallback: &[u8]) {
    let ok = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=1600x1200",
            "-frames:v",
            "1",
            "-q:v",
            "5",
        ])
        .arg(&path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        fs::write(path, fallback).unwrap();
    }
}
