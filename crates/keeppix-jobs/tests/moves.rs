#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::time::Duration;

use harness::TestDb;
use keeppix_db::LibraryRepo;
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};
use keeppix_jobs::discover;
use keeppix_jobs::hash as hash_job;
use keeppix_jobs::metadata;

fn tiny() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.jpg")
}

async fn seed_tree(test: &TestDb, root: &std::path::Path) -> keeppix_domain::Library {
    fs::create_dir_all(root).unwrap();
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: root.to_path_buf(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap()
}

async fn ingest_file(test: &TestDb, library_id: keeppix_domain::LibraryId, name: &str) {
    discover::run(test.db(), library_id, Duration::ZERO)
        .await
        .unwrap();
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM assets WHERE filename = $1")
        .bind(name)
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let asset_id = keeppix_domain::AssetId::from_uuid(id);
    metadata::run(test.db(), asset_id).await.unwrap();
    hash_job::run(test.db(), asset_id).await.unwrap();
}

#[tokio::test]
async fn two_copies_on_disk_are_duplicates_not_a_move() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-dup-{}", uuid::Uuid::now_v7()));
    let bytes = fs::read(tiny()).unwrap();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.jpg"), &bytes).unwrap();
    fs::write(root.join("b.jpg"), &bytes).unwrap();
    let library = seed_tree(&test, &root).await;
    ingest_file(&test, library.id, "a.jpg").await;
    ingest_file(&test, library.id, "b.jpg").await;
    let indexed: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE status = 'indexed'")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(indexed, 2);
}

#[tokio::test]
async fn hashing_after_a_rename_marks_the_old_asset_offline() {
    let test = TestDb::start().await;
    let root = std::env::temp_dir().join(format!("kpx-mv-{}", uuid::Uuid::now_v7()));
    let bytes = fs::read(tiny()).unwrap();
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.jpg"), &bytes).unwrap();
    let library = seed_tree(&test, &root).await;
    ingest_file(&test, library.id, "a.jpg").await;
    fs::rename(root.join("a.jpg"), root.join("b.jpg")).unwrap();
    ingest_file(&test, library.id, "b.jpg").await;

    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT filename, status FROM assets ORDER BY filename")
            .fetch_all(test.db().pool())
            .await
            .unwrap();
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        rows,
        vec![
            ("a.jpg".to_owned(), "offline".to_owned()),
            ("b.jpg".to_owned(), "indexed".to_owned()),
        ]
    );
}
