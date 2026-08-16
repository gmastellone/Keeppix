#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, JobRepo, LibraryRepo, ProblemsRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, JobKind, JobPriority, LibraryStatus, NewAsset, NewLibrary,
    SystemRole, UserId,
};

async fn seed(test: &TestDb) -> (UserId, keeppix_domain::LibraryId, keeppix_domain::FolderId) {
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
    (admin, library.id, folder.id)
}

fn photo(folder: keeppix_domain::FolderId, name: &str, size: i64) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(name).unwrap(),
        size_bytes: size,
        mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

#[tokio::test]
async fn problems_lists_offline_libraries_and_error_assets() {
    let test = TestDb::start().await;
    let (admin, library, folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .set_status(&ctx, library, LibraryStatus::Offline)
        .await
        .unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(photo(folder, "broken.jpg", 10))
        .await
        .unwrap();
    AssetRepo::new(test.db())
        .set_error(asset.id, "unreadable")
        .await
        .unwrap();

    let set = ProblemsRepo::new(test.db()).list(&ctx).await.unwrap();
    assert_eq!(set.offline_libraries.len(), 1);
    assert_eq!(set.error_assets.len(), 1);
    assert_eq!(set.error_assets[0].filename, "broken.jpg");
}

#[tokio::test]
async fn failed_jobs_are_admin_only() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let user = harness::seed_user(&test, admin, "luca").await;
    let job = JobRepo::new(test.db())
        .enqueue(
            JobKind::HashAsset,
            serde_json::json!({}),
            JobPriority::Background,
            None,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status = 'failed', last_error = 'boom' WHERE id = $1")
        .bind(job.id)
        .execute(test.db().pool())
        .await
        .unwrap();

    let as_admin = ProblemsRepo::new(test.db())
        .list(&AuthContext::user(admin, SystemRole::Admin))
        .await
        .unwrap();
    assert_eq!(as_admin.failed_jobs.len(), 1);

    let as_user = ProblemsRepo::new(test.db())
        .list(&AuthContext::user(user, SystemRole::User))
        .await
        .unwrap();
    assert!(as_user.failed_jobs.is_empty());
}
