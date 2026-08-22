//! Fase 8 Task 3/4: `faces` — rilevamento, assegnazione manuale, proposte,
//! coda di revisione.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{FaceRepo, FolderRepo, NewDetectedFace, PersonRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, Face, FaceBBox, FolderId, NewAsset, NewLibrary,
    PersonId, SystemRole, UserId,
};

async fn seed_person(test: &TestDb) -> PersonId {
    PersonRepo::new(test.db()).create(None).await.unwrap().id
}

const MODEL: &str = "scrfd-500mf+arcface";

fn bbox() -> FaceBBox {
    FaceBBox {
        x: 0.1,
        y: 0.1,
        w: 0.2,
        h: 0.2,
    }
}

/// Vettore unitario lungo l'asse `axis` (0..511) — stesso trucco di
/// `asset_tags.rs` per confronti di similarità deterministici.
fn unit_axis(axis: usize) -> Vec<f32> {
    let mut v = vec![0.0_f32; 512];
    v[axis] = 1.0;
    v
}

async fn seed_library(test: &TestDb, owner: UserId, path: &str) -> keeppix_domain::LibraryId {
    keeppix_db::LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from(path),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
        .id
}

fn discovered(folder: FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: 100,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

async fn seed_asset(test: &TestDb, folder: FolderId, filename: &str) -> AssetId {
    keeppix_db::AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, filename))
        .await
        .unwrap()
        .unwrap()
        .id
}

async fn seed_asset_in_new_library(test: &TestDb, owner: UserId, tag: &str) -> AssetId {
    let library = seed_library(test, owner, &format!("/mnt/{tag}")).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    seed_asset(test, folder.id, "foto.jpg").await
}

async fn detect_face(test: &TestDb, asset_id: AssetId, embedding: Option<Vec<f32>>) -> Face {
    FaceRepo::new(test.db())
        .insert_detected(NewDetectedFace {
            asset_id,
            bbox: bbox(),
            landmarks: None,
            embedding,
            detect_score: 0.95,
            quality: Some(0.8),
            model_version: MODEL.to_owned(),
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn a_detected_face_starts_without_a_person() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let asset = seed_asset_in_new_library(&test, admin, "detect").await;

    let face = detect_face(&test, asset, Some(unit_axis(0))).await;
    assert!(face.person_id.is_none());
    assert!(!face.is_human_assigned());
    assert!(!face.is_rejected());
}

#[tokio::test]
async fn list_for_asset_excludes_rejected_faces() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "list").await;
    let face = detect_face(&test, asset, None).await;

    let repo = FaceRepo::new(test.db());
    assert_eq!(repo.list_for_asset(&ctx, asset).await.unwrap().len(), 1);

    repo.reject(&ctx, face.id).await.unwrap();
    assert!(repo.list_for_asset(&ctx, asset).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_user_who_cannot_see_the_asset_gets_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let outsider = harness::seed_user(&test, admin, "outsider").await;
    let asset = seed_asset_in_new_library(&test, admin, "hidden").await;
    let face = detect_face(&test, asset, None).await;

    let outsider_ctx = AuthContext::user(outsider, SystemRole::User);
    let repo = FaceRepo::new(test.db());
    assert!(matches!(
        repo.list_for_asset(&outsider_ctx, asset).await,
        Err(keeppix_db::DbError::Forbidden)
    ));
    assert!(matches!(
        repo.assign(&outsider_ctx, face.id, PersonId::new()).await,
        Err(keeppix_db::DbError::Forbidden)
    ));
    assert!(matches!(
        repo.reject(&outsider_ctx, face.id).await,
        Err(keeppix_db::DbError::Forbidden)
    ));
}

#[tokio::test]
async fn manual_assignment_records_who_and_when() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "assign").await;
    let face = detect_face(&test, asset, None).await;
    let person = seed_person(&test).await;

    let repo = FaceRepo::new(test.db());
    repo.assign(&ctx, face.id, person).await.unwrap();

    let updated = repo
        .find_by_id_for_pipeline(face.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.person_id, Some(person));
    assert_eq!(updated.assigned_by, Some(admin));
    assert!(updated.assigned_at.is_some());
    assert!(updated.is_human_assigned());
}

#[tokio::test]
async fn a_human_assigned_face_is_never_touched_by_auto_assign() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "sticky").await;
    let face = detect_face(&test, asset, None).await;
    let chosen_by_human = seed_person(&test).await;
    let auto_candidate = seed_person(&test).await;

    let repo = FaceRepo::new(test.db());
    repo.assign(&ctx, face.id, chosen_by_human).await.unwrap();
    repo.auto_assign(face.id, auto_candidate).await.unwrap();

    let updated = repo
        .find_by_id_for_pipeline(face.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.person_id, Some(chosen_by_human));
}

#[tokio::test]
async fn reject_clears_any_assignment_and_is_permanent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "reject").await;
    let face = detect_face(&test, asset, None).await;
    let person = seed_person(&test).await;

    let repo = FaceRepo::new(test.db());
    repo.assign(&ctx, face.id, person).await.unwrap();
    repo.reject(&ctx, face.id).await.unwrap();

    let updated = repo
        .find_by_id_for_pipeline(face.id)
        .await
        .unwrap()
        .unwrap();
    assert!(updated.is_rejected());
    assert!(updated.person_id.is_none());

    // Un rilevamento successivo non deve poter riassegnare un volto rifiutato
    // in automatico — resta rifiutato finché un umano non decide altrimenti.
    let other_person = seed_person(&test).await;
    repo.auto_assign(face.id, other_person).await.unwrap();
    let still_rejected = repo
        .find_by_id_for_pipeline(face.id)
        .await
        .unwrap()
        .unwrap();
    assert!(still_rejected.is_rejected());
}

#[tokio::test]
async fn a_proposed_face_appears_in_the_review_queue_and_confirm_assigns_it() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "proposed").await;
    let face = detect_face(&test, asset, Some(unit_axis(1))).await;
    let candidate = seed_person(&test).await;

    let repo = FaceRepo::new(test.db());
    repo.propose(face.id, candidate, 0.62).await.unwrap();

    let queue = repo.list_proposed(&ctx).await.unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].id, face.id);
    assert_eq!(queue[0].proposed_person_id, Some(candidate));
    assert_eq!(repo.count_proposed_visible(&ctx).await.unwrap(), 1);

    repo.confirm_proposal(&ctx, face.id).await.unwrap();
    let confirmed = repo
        .find_by_id_for_pipeline(face.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(confirmed.person_id, Some(candidate));
    assert_eq!(confirmed.assigned_by, Some(admin));
    assert!(confirmed.proposed_person_id.is_none());
    assert_eq!(repo.count_proposed_visible(&ctx).await.unwrap(), 0);
}

#[tokio::test]
async fn confirming_a_face_with_no_pending_proposal_conflicts() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "no-proposal").await;
    let face = detect_face(&test, asset, None).await;

    let repo = FaceRepo::new(test.db());
    assert!(matches!(
        repo.confirm_proposal(&ctx, face.id).await,
        Err(keeppix_db::DbError::Conflict(_))
    ));
}

#[tokio::test]
async fn only_faces_with_an_embedding_are_grouping_candidates() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let asset = seed_asset_in_new_library(&test, admin, "candidates").await;
    let _no_embedding = detect_face(&test, asset, None).await;
    let with_embedding = detect_face(&test, asset, Some(unit_axis(2))).await;

    let repo = FaceRepo::new(test.db());
    let candidates = repo
        .list_unassigned_with_embedding(MODEL, 100)
        .await
        .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, with_embedding.id);

    let embedding = repo.embedding_of(with_embedding.id).await.unwrap();
    assert_eq!(embedding, Some(unit_axis(2)));
}
