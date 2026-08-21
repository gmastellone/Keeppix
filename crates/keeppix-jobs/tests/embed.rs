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
    if keeppix_media::first_complete_model_dir().is_none() {
        eprintln!("skipping: MobileCLIP2-S2 incomplete (run scripts/download-mobileclip2-s2.sh)");
        return;
    }

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

    let outcome = embed_job::run(test.db(), &data_dir, 16, || true)
        .await
        .unwrap();
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

    let again = embed_job::run(test.db(), &data_dir, 16, || true)
        .await
        .unwrap();
    assert_eq!(
        again.embedded, 0,
        "stesso model_version non deve ricalcolare: {again:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn embed_job_skips_culling_subtree_entirely() {
    if keeppix_media::first_complete_model_dir().is_none() {
        eprintln!("skipping: MobileCLIP2-S2 incomplete (run scripts/download-mobileclip2-s2.sh)");
        return;
    }

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

    let outcome = embed_job::run(test.db(), &data_dir, 32, || true)
        .await
        .unwrap();
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
async fn one_run_drains_multiple_batches_without_requeueing() {
    if keeppix_media::first_complete_model_dir().is_none() {
        eprintln!("skipping: MobileCLIP2-S2 incomplete (run scripts/download-mobileclip2-s2.sh)");
        return;
    }

    let test = TestDb::start_with_vector().await;
    let admin = harness::seed_admin(&test).await;
    let root = std::env::temp_dir().join(format!("kpx-emb-win-{}", uuid::Uuid::now_v7()));
    let data_dir = root.join("data");
    fs::create_dir_all(&data_dir).unwrap();

    // Cinque foto, lotto da 2 → tre giri interni; un solo `run` deve
    // svuotare la coda senza riaccodare backfill (sessione viva per finestra).
    for i in 0..5 {
        let lib = root.join(format!("lib-{i}"));
        let _ = ingest_until_thumb(&test, admin, &lib, &data_dir, &format!("{i}.jpg")).await;
    }

    let outcome = embed_job::run(test.db(), &data_dir, 2, || true)
        .await
        .unwrap();
    assert_eq!(
        outcome.embedded, 5,
        "una finestra deve drenare tutti i pending a lotti piccoli: {outcome:?}"
    );

    let backfill: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE dedup_key = 'embed_assets:backfill' \
         AND status IN ('pending', 'running')",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(
        backfill, 0,
        "coda vuota → nessun riaccodo backfill; riaccodare ricaricherebbe il modello"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn pause_between_batches_stops_and_requeues_backfill() {
    if keeppix_media::first_complete_model_dir().is_none() {
        eprintln!("skipping: MobileCLIP2-S2 incomplete (run scripts/download-mobileclip2-s2.sh)");
        return;
    }

    let test = TestDb::start_with_vector().await;
    let admin = harness::seed_admin(&test).await;
    let root = std::env::temp_dir().join(format!("kpx-emb-pause-{}", uuid::Uuid::now_v7()));
    let data_dir = root.join("data");
    fs::create_dir_all(&data_dir).unwrap();

    for i in 0..5 {
        let lib = root.join(format!("lib-{i}"));
        let _ = ingest_until_thumb(&test, admin, &lib, &data_dir, &format!("{i}.jpg")).await;
    }

    let batches_after = std::sync::atomic::AtomicU32::new(0);
    let outcome = embed_job::run(test.db(), &data_dir, 2, || {
        // Chiamato fra un lotto e l'altro: al primo check la vista riprende.
        batches_after.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        false
    })
    .await
    .unwrap();
    assert_eq!(
        outcome.embedded, 2,
        "la pausa fra lotti deve fermare dopo il primo lotto da 2: {outcome:?}"
    );
    assert_eq!(
        batches_after.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "il gate si valuta una volta fra il primo e il secondo lotto"
    );

    let backfill: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE dedup_key = 'embed_assets:backfill' \
         AND status IN ('pending', 'running')",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(
        backfill, 1,
        "con pending rimanenti la pausa deve riaccodare il backfill"
    );

    let remaining = EmbeddingRepo::new(test.db())
        .list_pending(MODEL_VERSION, 32)
        .await
        .unwrap();
    assert_eq!(remaining.len(), 3, "tre foto ancora da embeddare");

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn embed_assets_job_kind_is_dispatched() {
    assert_eq!(JobKind::EmbedAssets.as_str(), "embed_assets");
    assert_eq!(JobPriority::Background.as_i16(), 3);
}

#[tokio::test]
async fn ingest_enqueue_is_high_priority_and_deduped() {
    let test = TestDb::start().await;
    embed_job::enqueue_after_ingest(test.db()).await.unwrap();
    embed_job::enqueue_after_ingest(test.db()).await.unwrap();
    let rows: Vec<(String, i16)> =
        sqlx::query_as("SELECT kind, priority FROM jobs WHERE dedup_key = 'embed_assets:ingest'")
            .fetch_all(test.db().pool())
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "dedup while pending: {rows:?}");
    assert_eq!(rows[0].0, "embed_assets");
    assert_eq!(rows[0].1, JobPriority::High.as_i16());
}

#[tokio::test]
async fn backfill_schedule_is_background_and_deduped() {
    let test = TestDb::start().await;
    embed_job::schedule_backfill(test.db()).await.unwrap();
    embed_job::schedule_backfill(test.db()).await.unwrap();
    let rows: Vec<(String, i16)> =
        sqlx::query_as("SELECT kind, priority FROM jobs WHERE dedup_key = 'embed_assets:backfill'")
            .fetch_all(test.db().pool())
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "dedup while pending: {rows:?}");
    assert_eq!(rows[0].0, "embed_assets");
    assert_eq!(rows[0].1, JobPriority::Background.as_i16());
}
