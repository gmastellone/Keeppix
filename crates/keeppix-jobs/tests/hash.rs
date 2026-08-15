#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::LibraryRepo;
use keeppix_domain::{AssetId, AuthContext, NewLibrary, SystemRole};
use keeppix_jobs::discover;
use keeppix_jobs::hash as hash_job;
use keeppix_jobs::metadata;

#[tokio::test]
async fn hashing_twice_does_not_duplicate_derive_jobs() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-hjob-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("foto.jpg"), b"same-bytes").unwrap();
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
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let asset_id = AssetId::from_uuid(id);
    metadata::run(test.db(), asset_id).await.unwrap();
    hash_job::run(test.db(), asset_id).await.unwrap();
    hash_job::run(test.db(), asset_id).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'derive_asset' AND status IN ('pending','running')",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(n, 1);
}
