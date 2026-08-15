#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::{AssetRepo, LibraryRepo};
use keeppix_domain::{AssetStatus, AuthContext, NewLibrary, SystemRole};
use keeppix_jobs::discover;
use keeppix_jobs::metadata;

#[tokio::test]
async fn metadata_indexes_and_enqueues_hash() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-meta-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("foto.jpg"), b"not a jpeg").unwrap();
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
    let asset_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    metadata::run(test.db(), keeppix_domain::AssetId::from_uuid(asset_id))
        .await
        .unwrap();
    let asset = AssetRepo::new(test.db())
        .get_for_scan(keeppix_domain::AssetId::from_uuid(asset_id))
        .await
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Indexed);
    assert!(asset.taken_at_utc.is_some());
    let hashes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'hash_asset' AND status = 'pending'",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(hashes, 1);
}
