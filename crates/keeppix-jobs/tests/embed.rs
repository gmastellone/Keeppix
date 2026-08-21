#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Fase 7 Task 5: job `embed_assets` — miniature 240px, esclusione culling,
//! inferenza a lotto, niente ricalcolo sullo stesso `model_version`.

mod harness;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use harness::TestDb;
use keeppix_db::{EmbeddingRepo, LibraryRepo};
use keeppix_domain::{AssetId, AuthContext, JobKind, JobPriority, NewLibrary, SystemRole, UserId};
use keeppix_jobs::derive as derive_job;
use keeppix_jobs::discover;
use keeppix_jobs::embed as embed_job;
use keeppix_jobs::hash as hash_job;
use keeppix_jobs::metadata;
use keeppix_media::{MODEL_VERSION, derivative_paths};

fn tiny_jpeg() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.jpg")
}

fn copy_fixture(dest_dir: &Path, name: &str) {
    fs::create_dir_all(dest_dir).unwrap();
    fs::copy(tiny_jpeg(), dest_dir.join(name)).unwrap();
}

async fn ingest_until_thumb(
    test: &TestDb,
    admin: UserId,
    root: &Path,
    data_dir: &Path,
    name: &str,
) -> AssetId {
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: format!("Lib-{name}"),
                owner_id: admin,
                root_path: root.to_path_buf(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    copy_fixture(root, name);
    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM assets WHERE filename = $1")
        .bind(name)
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let asset_id = AssetId::from_uuid(id);
    metadata::run(test.db(), asset_id).await.unwrap();
    hash_job::run(test.db(), asset_id).await.unwrap();
    let hash: Vec<u8> = sqlx::query_scalar("SELECT content_hash FROM assets WHERE id = $1")
        .bind(id)
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let hash: [u8; 32] = hash.as_slice().try_into().unwrap();
    derive_job::run(test.db(), data_dir, hash).await.unwrap();
    let (thumb, _) = derivative_paths(data_dir, &hash);
    assert!(
        thumb.is_file(),
        "thumb must exist before embed: {}",
        thumb.display()
    );
    asset_id
}

#[tokio::test]
async fn embed_job_writes_embeddings_from_thumbs_in_one_batch() {
    let test = TestDb::start_with_vector().await;
    let admin = harness::seed_admin(&test).await;
    let root = std::env::temp_dir().join(format!("kpx-emb-{}", uuid::Uuid::now_v7()));
    let data_dir = root.join("data");
    fs::create_dir_all(&data_dir).unwrap();
    let lib_a = root.join("lib-a");
    let lib_b = root.join("lib-b");

    let a = ingest_until_thumb(&test, admin, &lib_a, &data_dir, "a.jpg").await;
    let b = ingest_until_thumb(&test, admin, &lib_b, &data_dir, "b.jpg").await;

    // Corrompe gli originali: se il job li decodificasse fallirebbe.
    fs::write(lib_a.join("a.jpg"), b"not-a-jpeg").unwrap();
    fs::write(lib_b.join("b.jpg"), b"not-a-jpeg").unwrap();

    let outcome = embed_job::run(test.db(), &data_dir, 16).await.unwrap();
    assert!(
        outcome.embedded >= 2,
        "expected a batch of both assets, got {outcome:?}"
    );

    let repo = EmbeddingRepo::new(test.db());
    for id in [a, b] {
        let row = repo.get(id).await.unwrap().expect("embedding row");
        assert_eq!(row.model_version, MODEL_VERSION);
        assert_eq!(row.embedding.len(), 512);
    }

    let again = embed_job::run(test.db(), &data_dir, 16).await.unwrap();
    assert_eq!(
        again.embedded, 0,
        "stesso model_version non deve ricalcolare: {again:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn embed_job_skips_culling_subtree_entirely() {
    let test = TestDb::start_with_vector().await;
    let root = std::env::temp_dir().join(format!("kpx-emb-cull-{}", uuid::Uuid::now_v7()));
    let data_dir = root.join("data");
    fs::create_dir_all(&data_dir).unwrap();

    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Cull".to_owned(),
                owner_id: admin,
                root_path: root.join("lib"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();

    fs::create_dir_all(root.join("lib/album")).unwrap();
    fs::create_dir_all(root.join("lib/Culling/_taken")).unwrap();
    fs::copy(tiny_jpeg(), root.join("lib/album/keep.jpg")).unwrap();
    fs::copy(tiny_jpeg(), root.join("lib/Culling/cull.jpg")).unwrap();
    fs::copy(tiny_jpeg(), root.join("lib/Culling/_taken/taken.jpg")).unwrap();

    discover::run(test.db(), library.id, Duration::ZERO)
        .await
        .unwrap();

    let cull_root: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM folders WHERE library_id = $1 AND name = 'Culling'")
            .bind(library.id.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    sqlx::query("UPDATE libraries SET culling_root_folder_id = $1 WHERE id = $2")
        .bind(cull_root)
        .bind(library.id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let ids: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT a.id FROM assets a JOIN folders f ON f.id = a.folder_id WHERE f.library_id = $1",
    )
    .bind(library.id.as_uuid())
    .fetch_all(test.db().pool())
    .await
    .unwrap();

    for id in ids {
        let asset_id = AssetId::from_uuid(id);
        metadata::run(test.db(), asset_id).await.unwrap();
        hash_job::run(test.db(), asset_id).await.unwrap();
        let hash: Vec<u8> = sqlx::query_scalar("SELECT content_hash FROM assets WHERE id = $1")
            .bind(id)
            .fetch_one(test.db().pool())
            .await
            .unwrap();
        let hash: [u8; 32] = hash.as_slice().try_into().unwrap();
        derive_job::run(test.db(), &data_dir, hash).await.unwrap();
    }

    let outcome = embed_job::run(test.db(), &data_dir, 32).await.unwrap();
    assert_eq!(
        outcome.embedded, 1,
        "solo keep.jpg fuori dal culling: {outcome:?}"
    );

    let keep_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM assets WHERE filename = 'keep.jpg'")
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert!(
        EmbeddingRepo::new(test.db())
            .get(AssetId::from_uuid(keep_id))
            .await
            .unwrap()
            .is_some()
    );
    for name in ["cull.jpg", "taken.jpg"] {
        let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM assets WHERE filename = $1")
            .bind(name)
            .fetch_one(test.db().pool())
            .await
            .unwrap();
        assert!(
            EmbeddingRepo::new(test.db())
                .get(AssetId::from_uuid(id))
                .await
                .unwrap()
                .is_none(),
            "{name} nel culling non deve avere embedding"
        );
    }

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn embed_assets_job_kind_is_dispatched() {
    assert_eq!(JobKind::EmbedAssets.as_str(), "embed_assets");
    assert_eq!(JobPriority::Background.as_i16(), 3);
}
