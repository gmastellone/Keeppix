#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, JobRepo, LibraryRepo};
use keeppix_domain::{AssetKind, AssetName, AuthContext, NewAsset, NewLibrary, SystemRole};
use keeppix_media::transcode::{TranscodeProfile, cache_is_ready, transcode_cache_dir};

#[tokio::test]
async fn transcode_job_dedup_key_collides_while_pending() {
    let test = TestDb::start().await;
    let asset_id = keeppix_domain::AssetId::from_uuid(uuid::Uuid::now_v7());
    keeppix_jobs::transcode::enqueue(test.db(), asset_id, false)
        .await
        .unwrap();
    keeppix_jobs::transcode::enqueue(test.db(), asset_id, false)
        .await
        .unwrap();
    let count = JobRepo::new(test.db())
        .count_for_dedup_key(&format!("transcode:{asset_id}:0"))
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn transcode_job_writes_hls_cache_for_mkv() {
    if !keeppix_media::video::ffprobe_available() {
        eprintln!("skipping: ffprobe not in PATH");
        return;
    }
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let data_dir = tempfile::tempdir().expect("tempdir");
    let root = data_dir.path().join("library");
    std::fs::create_dir_all(&root).unwrap();
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Videos".to_owned(),
                owner_id: admin,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    let src = root.join("clip.mkv");
    write_mkv_clip(&src);
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse("clip.mkv").unwrap(),
            size_bytes: i64::try_from(src.metadata().unwrap().len()).unwrap_or(1),
            mtime: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            inode: Some(2),
            kind: AssetKind::Video,
        })
        .await
        .unwrap()
        .unwrap();
    let hash = [0x33_u8; 32];
    AssetRepo::new(test.db())
        .set_hash(asset.id, hash)
        .await
        .unwrap();
    keeppix_jobs::transcode::run(test.db(), data_dir.path(), asset.id, false)
        .await
        .unwrap();
    let cache = transcode_cache_dir(data_dir.path(), &hash, TranscodeProfile::SaveBandwidth720p);
    assert!(cache_is_ready(&cache));
}

fn write_mkv_clip(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=320x240:d=2",
            "-t",
            "2",
            "-pix_fmt",
            "yuv420p",
            "-f",
            "matroska",
        ])
        .arg(path)
        .status()
        .expect("ffmpeg");
    assert!(status.success(), "ffmpeg must write a test clip");
}
