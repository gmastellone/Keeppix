#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, JobRepo, LibraryRepo, ProblemsRepo};
use keeppix_domain::{
    AssetKind, AssetName, AssetStatus, AuthContext, JobKind, JobPriority, NewAsset, NewLibrary,
    SystemRole, UserId,
};

async fn seed(test: &TestDb) -> (UserId, keeppix_domain::FolderId) {
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    (admin, folder.id)
}

fn hex_of(hash: &[u8; 32]) -> String {
    hash.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

async fn error_raw(test: &TestDb, folder: keeppix_domain::FolderId, name: &str) -> [u8; 32] {
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(name).unwrap(),
            size_bytes: 10,
            mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
            inode: Some(1),
            kind: AssetKind::RawImage,
        })
        .await
        .unwrap()
        .unwrap();
    let mut hash = [0u8; 32];
    hash[0] = name.as_bytes()[0];
    AssetRepo::new(test.db())
        .set_hash(asset.id, hash)
        .await
        .unwrap();
    AssetRepo::new(test.db())
        .set_error(asset.id, "demosaic killed")
        .await
        .unwrap();
    hash
}

async fn finish_derive_raw(test: &TestDb, hash: &[u8; 32]) {
    let hex = hex_of(hash);
    let jobs = JobRepo::new(test.db());
    let job = jobs
        .enqueue(
            JobKind::DeriveRaw,
            serde_json::json!({ "content_hash": hex }),
            JobPriority::Background,
            Some(&format!("derive_raw:{hex}")),
        )
        .await
        .unwrap();
    let claimed = jobs
        .claim(uuid::Uuid::now_v7(), JobPriority::Background)
        .await
        .unwrap()
        .expect("claim");
    assert_eq!(claimed.id, job.id);
    jobs.complete(claimed.id).await.unwrap();
}

async fn pending_derive_raw(test: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM jobs WHERE kind = 'derive_raw' AND status = 'pending'")
        .fetch_one(test.db().pool())
        .await
        .unwrap()
}

/// A derive that went into `error` via `Ok` (not `JobRepo::fail`) must be
/// re-enqueued without going through a rescan.
#[tokio::test]
async fn a_failed_derive_is_retried_by_the_maintenance_job_until_the_attempt_cap() {
    let test = TestDb::start().await;
    let (admin, folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let hash = error_raw(&test, folder, "a.arw").await;
    finish_derive_raw(&test, &hash).await;
    assert_eq!(pending_derive_raw(&test).await, 0);

    keeppix_jobs::retry_derives::run(test.db()).await.unwrap();
    assert_eq!(pending_derive_raw(&test).await, 1, "first retry");

    keeppix_jobs::retry_derives::run(test.db()).await.unwrap();
    assert_eq!(
        pending_derive_raw(&test).await,
        1,
        "does not enqueue a second job while the retry is still pending"
    );

    finish_derive_raw(&test, &hash).await;
    keeppix_jobs::retry_derives::run(test.db()).await.unwrap();
    finish_derive_raw(&test, &hash).await;
    assert_eq!(pending_derive_raw(&test).await, 0);

    keeppix_jobs::retry_derives::run(test.db()).await.unwrap();
    assert_eq!(
        pending_derive_raw(&test).await,
        0,
        "once the cap is exceeded it does not retry forever"
    );

    let set = ProblemsRepo::new(test.db()).list(&ctx).await.unwrap();
    assert_eq!(set.error_assets.len(), 1);
    let asset = AssetRepo::new(test.db())
        .get_for_scan(set.error_assets[0].id)
        .await
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Error);
}

/// A successful derive must remove the asset from `/problems`.
#[tokio::test]
async fn a_successful_retry_clears_the_asset_error_status() {
    let test = TestDb::start().await;
    let (admin, folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let hash = error_raw(&test, folder, "b.arw").await;
    AssetRepo::new(test.db())
        .set_indexed(
            AssetRepo::new(test.db())
                .ids_with_hash(&hash)
                .await
                .unwrap()[0],
            Utc.with_ymd_and_hms(2024, 7, 1, 12, 0, 0).unwrap(),
            100,
            80,
        )
        .await
        .unwrap();
    AssetRepo::new(test.db())
        .set_error(
            AssetRepo::new(test.db())
                .ids_with_hash(&hash)
                .await
                .unwrap()[0],
            "demosaic killed",
        )
        .await
        .unwrap();

    AssetRepo::new(test.db())
        .set_thumbhash_for_hash(&hash, &[1, 2, 3, 4])
        .await
        .unwrap();

    let asset = AssetRepo::new(test.db())
        .get_for_scan(
            AssetRepo::new(test.db())
                .ids_with_hash(&hash)
                .await
                .unwrap()[0],
        )
        .await
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Indexed);
    let set = ProblemsRepo::new(test.db()).list(&ctx).await.unwrap();
    assert!(set.error_assets.is_empty());
}
