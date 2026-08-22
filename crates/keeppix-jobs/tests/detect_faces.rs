#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Fase 8 Task 4/5: job `detect_faces` — miniature 240px per il
//! rilevamento, preview 2048px per l'impronta, esclusione culling,
//! raggruppamento incrementale.
//!
//! Senza pesi SCRFD/ArcFace su disco (questa sandbox non ha rete verso
//! `HuggingFace`, stesso limite di `embed.rs` per MobileCLIP2-S2), i test che
//! richiedono un'inferenza reale saltano — vedi il ledger di fase. Restano
//! verificabili senza pesi: la coda vuota, l'errore esplicito a coda piena
//! senza modello, e la validazione di `limit_from_payload`.

mod harness;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use harness::TestDb;
use keeppix_db::{FaceRepo, LibraryRepo};
use keeppix_domain::{AssetId, AuthContext, NewLibrary, SystemRole, UserId};
use keeppix_jobs::derive as derive_job;
use keeppix_jobs::detect_faces;
use keeppix_jobs::discover;
use keeppix_jobs::hash as hash_job;
use keeppix_jobs::metadata;
use keeppix_media::derivative_paths;
use keeppix_media::face::MODEL_VERSION;

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
        "thumb must exist before detect_faces: {}",
        thumb.display()
    );
    asset_id
}

#[tokio::test]
async fn run_is_a_no_op_when_nothing_is_pending() {
    // Non serve alcun peso: `run` controlla la coda prima di richiedere il
    // modello, esattamente come `embed::run`.
    let test = TestDb::start_with_vector().await;
    let data_dir = std::env::temp_dir().join(format!("kpx-detect-empty-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&data_dir).unwrap();

    let outcome = detect_faces::run(test.db(), &data_dir, 16, || true)
        .await
        .unwrap();
    assert_eq!(outcome.assets_scanned, 0);
    assert_eq!(outcome.faces_found, 0);
}

#[tokio::test]
async fn run_fails_explicitly_when_work_is_pending_but_weights_are_missing() {
    if keeppix_media::face::first_complete_model_dir().is_some() {
        eprintln!("skipping: real SCRFD/ArcFace weights are present on this machine");
        return;
    }

    let test = TestDb::start_with_vector().await;
    let admin = harness::seed_admin(&test).await;
    let root = std::env::temp_dir().join(format!("kpx-detect-missing-{}", uuid::Uuid::now_v7()));
    let data_dir = root.join("data");
    fs::create_dir_all(&data_dir).unwrap();
    ingest_until_thumb(&test, admin, &root, &data_dir, "a.jpg").await;

    let err = detect_faces::run(test.db(), &data_dir, 16, || true)
        .await
        .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("face models missing"),
        "expected an explicit missing-model error, got: {message}"
    );

    // Niente è stato segnato come analizzato: un errore esplicito non deve
    // far sparire silenziosamente il lavoro dalla coda.
    assert_eq!(
        FaceRepo::new(test.db())
            .count_pending_scan(MODEL_VERSION)
            .await
            .unwrap(),
        1
    );
}

#[test]
fn limit_from_payload_defaults_when_absent() {
    let payload = serde_json::json!({});
    assert_eq!(
        detect_faces::limit_from_payload(&payload).unwrap(),
        detect_faces::DEFAULT_BATCH_SIZE
    );
}

#[test]
fn limit_from_payload_rejects_zero_and_negative() {
    assert!(detect_faces::limit_from_payload(&serde_json::json!({"limit": 0})).is_err());
    assert!(detect_faces::limit_from_payload(&serde_json::json!({"limit": -3})).is_err());
}

#[test]
fn limit_from_payload_accepts_a_positive_override() {
    let payload = serde_json::json!({"limit": 4});
    assert_eq!(detect_faces::limit_from_payload(&payload).unwrap(), 4);
}

#[tokio::test]
async fn detects_and_groups_faces_when_weights_are_present() {
    let Some(_dir) = keeppix_media::face::first_complete_model_dir() else {
        eprintln!("skipping: SCRFD/ArcFace weights missing (see fase-8 ledger)");
        return;
    };

    let test = TestDb::start_with_vector().await;
    let admin = harness::seed_admin(&test).await;
    let root = std::env::temp_dir().join(format!("kpx-detect-real-{}", uuid::Uuid::now_v7()));
    let data_dir = root.join("data");
    fs::create_dir_all(&data_dir).unwrap();
    ingest_until_thumb(&test, admin, &root, &data_dir, "a.jpg").await;

    let outcome = detect_faces::run(test.db(), &data_dir, 16, || true)
        .await
        .unwrap();
    assert_eq!(outcome.assets_scanned, 1);
    // La fixture `tiny.jpg` non contiene necessariamente un volto reale:
    // qui verifichiamo solo che la passata converga (l'asset esce dalla
    // coda), non un conteggio di volti specifico.
    assert_eq!(
        FaceRepo::new(test.db())
            .count_pending_scan(MODEL_VERSION)
            .await
            .unwrap(),
        0
    );
}
