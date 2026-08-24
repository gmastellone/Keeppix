//! Fase 8 Task 3/4: `faces` — rilevamento, assegnazione manuale, proposte,
//! coda di revisione.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{FaceRepo, FolderRepo, NewDetectedFace, PersonRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, Face, FaceBBox, FolderId, NewAsset, NewLibrary,
    PersonId, PersonName, SystemRole, UserId,
};

async fn seed_person(test: &TestDb) -> PersonId {
    PersonRepo::new(test.db()).create(None).await.unwrap().id
}

/// Stato grezzo di una riga `faces`, letto direttamente via SQL: a
/// differenza di `FaceRepo::list_for_asset` (che esclude i rifiutati per
/// disegno), qui i test devono poter osservare anche lo stato dopo un
/// rifiuto.
struct FaceState {
    person_id: Option<uuid::Uuid>,
    assigned_by: Option<uuid::Uuid>,
    assigned_at: Option<chrono::DateTime<Utc>>,
    rejected_at: Option<chrono::DateTime<Utc>>,
    proposed_person_id: Option<uuid::Uuid>,
}

type FaceStateRow = (
    Option<uuid::Uuid>,
    Option<uuid::Uuid>,
    Option<chrono::DateTime<Utc>>,
    Option<chrono::DateTime<Utc>>,
    Option<uuid::Uuid>,
);

async fn fetch_face_state(test: &TestDb, id: keeppix_domain::FaceId) -> FaceState {
    let row: FaceStateRow = sqlx::query_as(
        "SELECT person_id, assigned_by, assigned_at, rejected_at, proposed_person_id \
         FROM faces WHERE id = $1",
    )
    .bind(id.as_uuid())
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    FaceState {
        person_id: row.0,
        assigned_by: row.1,
        assigned_at: row.2,
        rejected_at: row.3,
        proposed_person_id: row.4,
    }
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

    let updated = fetch_face_state(&test, face.id).await;
    assert_eq!(updated.person_id, Some(person.as_uuid()));
    assert_eq!(updated.assigned_by, Some(admin.as_uuid()));
    assert!(updated.assigned_at.is_some());
    assert!(updated.assigned_by.is_some());
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

    let updated = fetch_face_state(&test, face.id).await;
    assert_eq!(updated.person_id, Some(chosen_by_human.as_uuid()));
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

    let updated = fetch_face_state(&test, face.id).await;
    assert!(updated.rejected_at.is_some());
    assert!(updated.person_id.is_none());

    // Un rilevamento successivo non deve poter riassegnare un volto rifiutato
    // in automatico — resta rifiutato finché un umano non decide altrimenti.
    let other_person = seed_person(&test).await;
    repo.auto_assign(face.id, other_person).await.unwrap();
    let still_rejected = fetch_face_state(&test, face.id).await;
    assert!(still_rejected.rejected_at.is_some());
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
    let confirmed = fetch_face_state(&test, face.id).await;
    assert_eq!(confirmed.person_id, Some(candidate.as_uuid()));
    assert_eq!(confirmed.assigned_by, Some(admin.as_uuid()));
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

async fn seed_hashed_asset(
    test: &TestDb,
    folder: FolderId,
    filename: &str,
    hash: [u8; 32],
) -> AssetId {
    let asset = keeppix_db::AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, filename))
        .await
        .unwrap()
        .unwrap();
    keeppix_db::AssetRepo::new(test.db())
        .set_hash(asset.id, hash)
        .await
        .unwrap();
    asset.id
}

const FACE_MODEL: &str = "scrfd-500mf+arcface";

#[tokio::test]
async fn list_pending_scan_excludes_the_entire_culling_subtree() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "/mnt/face-cull").await;
    let library_folder = FolderRepo::new(test.db())
        .ensure_path(library, &["album"])
        .await
        .unwrap();
    let cull_root = FolderRepo::new(test.db())
        .ensure_path(library, &["Culling"])
        .await
        .unwrap();
    let cull_child = FolderRepo::new(test.db())
        .ensure_path(library, &["Culling", "_taken"])
        .await
        .unwrap();

    sqlx::query("UPDATE libraries SET culling_root_folder_id = $1 WHERE id = $2")
        .bind(cull_root.id.as_uuid())
        .bind(library.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let keep = seed_hashed_asset(&test, library_folder.id, "keep.jpg", [0x33; 32]).await;
    let in_root = seed_hashed_asset(&test, cull_root.id, "in-root.jpg", [0x44; 32]).await;
    let in_taken = seed_hashed_asset(&test, cull_child.id, "taken.jpg", [0x55; 32]).await;

    let batch = FaceRepo::new(test.db())
        .list_pending_scan(FACE_MODEL, 100)
        .await
        .unwrap();
    let ids: Vec<_> = batch.iter().map(|p| p.asset_id).collect();
    assert!(ids.contains(&keep), "fuori dal culling deve restare");
    assert!(!ids.contains(&in_root), "radice culling esclusa per intero");
    assert!(
        !ids.contains(&in_taken),
        "sottoalbero culling escluso via path <@"
    );
}

#[tokio::test]
async fn list_pending_scan_respects_faces_enabled() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "/mnt/face-toggle").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["a"])
        .await
        .unwrap();
    let asset = seed_hashed_asset(&test, folder.id, "a.jpg", [0x66; 32]).await;

    let repo = FaceRepo::new(test.db());
    assert!(
        repo.list_pending_scan(FACE_MODEL, 100)
            .await
            .unwrap()
            .iter()
            .any(|p| p.asset_id == asset)
    );

    keeppix_db::LibraryRepo::new(test.db())
        .update(&ctx, library, None, None, Some(false), None)
        .await
        .unwrap();
    assert!(
        !repo
            .list_pending_scan(FACE_MODEL, 100)
            .await
            .unwrap()
            .iter()
            .any(|p| p.asset_id == asset)
    );
}

#[tokio::test]
async fn mark_scanned_removes_the_asset_from_the_pending_queue_even_with_zero_faces() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "/mnt/zero-faces").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["a"])
        .await
        .unwrap();
    let asset = seed_hashed_asset(&test, folder.id, "a.jpg", [0x11; 32]).await;

    let repo = FaceRepo::new(test.db());
    assert!(
        repo.list_pending_scan(FACE_MODEL, 100)
            .await
            .unwrap()
            .iter()
            .any(|p| p.asset_id == asset)
    );

    // Nessun volto trovato — comunque va segnato come analizzato.
    repo.mark_scanned(asset, FACE_MODEL).await.unwrap();

    assert!(
        !repo
            .list_pending_scan(FACE_MODEL, 100)
            .await
            .unwrap()
            .iter()
            .any(|p| p.asset_id == asset)
    );
    assert_eq!(repo.count_pending_scan(FACE_MODEL).await.unwrap(), 0);
}

#[tokio::test]
async fn confirm_all_proposed_for_person_assigns_only_that_persons_proposals() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset_a = seed_asset_in_new_library(&test, admin, "bulk-a").await;
    let asset_b = seed_asset_in_new_library(&test, admin, "bulk-b").await;

    let candidate = seed_person(&test).await;
    let other_candidate = seed_person(&test).await;
    let face_a = detect_face(&test, asset_a, Some(unit_axis(3))).await;
    let face_b = detect_face(&test, asset_b, Some(unit_axis(4))).await;

    let repo = FaceRepo::new(test.db());
    repo.propose(face_a.id, candidate, 0.6).await.unwrap();
    repo.propose(face_b.id, other_candidate, 0.6).await.unwrap();

    let confirmed = repo
        .confirm_all_proposed_for_person(&ctx, candidate)
        .await
        .unwrap();
    assert_eq!(confirmed, vec![face_a.id]);

    let a = fetch_face_state(&test, face_a.id).await;
    assert_eq!(a.person_id, Some(candidate.as_uuid()));
    let b = fetch_face_state(&test, face_b.id).await;
    assert!(
        b.person_id.is_none(),
        "other candidate's proposal must be untouched"
    );
}

#[tokio::test]
async fn reject_all_proposed_for_person_is_permanent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "bulk-reject").await;
    let candidate = seed_person(&test).await;
    let face = detect_face(&test, asset, Some(unit_axis(5))).await;

    let repo = FaceRepo::new(test.db());
    repo.propose(face.id, candidate, 0.6).await.unwrap();

    let rejected = repo
        .reject_all_proposed_for_person(&ctx, candidate)
        .await
        .unwrap();
    assert_eq!(rejected, vec![face.id]);

    let updated = fetch_face_state(&test, face.id).await;
    assert!(updated.rejected_at.is_some());
    assert!(updated.proposed_person_id.is_none());
}

#[tokio::test]
async fn delete_all_data_requires_an_admin() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let outsider = harness::seed_user(&test, admin, "plain").await;
    let ctx = AuthContext::user(outsider, SystemRole::User);

    let err = FaceRepo::new(test.db())
        .delete_all_data(&ctx)
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Forbidden));
}

#[tokio::test]
async fn delete_all_data_wipes_faces_persons_groups_and_scan_state() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "wipe").await;

    let person = seed_person(&test).await;
    let face = detect_face(&test, asset, Some(unit_axis(3))).await;
    FaceRepo::new(test.db())
        .assign(&ctx, face.id, person)
        .await
        .unwrap();
    FaceRepo::new(test.db())
        .mark_scanned(asset, MODEL)
        .await
        .unwrap();
    let group = keeppix_db::PersonGroupRepo::new(test.db())
        .create(
            &ctx,
            keeppix_db::NewPersonGroup {
                name: "Famiglia".to_owned(),
            },
        )
        .await
        .unwrap();

    FaceRepo::new(test.db())
        .delete_all_data(&ctx)
        .await
        .unwrap();

    let faces_left: i64 = sqlx::query_scalar("SELECT count(*) FROM faces")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let persons_left: i64 = sqlx::query_scalar("SELECT count(*) FROM persons")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let groups_left: i64 = sqlx::query_scalar("SELECT count(*) FROM person_groups")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    let scans_left: i64 = sqlx::query_scalar("SELECT count(*) FROM asset_face_scans")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(faces_left, 0);
    assert_eq!(persons_left, 0);
    assert_eq!(groups_left, 0);
    assert_eq!(scans_left, 0, "wiped scan state so re-detection can happen");
    let _ = (face.id, person, group.id);
}

// Fase 11 Task 7 (SP-3 §11, dimensione "Persone" — `AssetView`).

#[tokio::test]
async fn confirmed_among_includes_both_manual_and_auto_assignments() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "confirmed-among").await;
    let manual_face = detect_face(&test, asset, None).await;
    let auto_face = detect_face(&test, asset, None).await;
    let manual_person = PersonRepo::new(test.db())
        .create(Some(PersonName::parse("Marta").unwrap()))
        .await
        .unwrap();
    let auto_person = seed_person(&test).await;

    let repo = FaceRepo::new(test.db());
    repo.assign(&ctx, manual_face.id, manual_person.id)
        .await
        .unwrap();
    repo.auto_assign(auto_face.id, auto_person).await.unwrap();

    let map = repo.confirmed_among(&[asset]).await.unwrap();

    let badges = &map[&asset];
    assert_eq!(badges.len(), 2, "both the manual and the auto assignment count as confirmed");
    assert!(badges.iter().any(|b| b.person_id == manual_person.id && b.person_name.as_deref() == Some("Marta")));
    assert!(badges.iter().any(|b| b.person_id == auto_person && b.person_name.is_none()));
}

#[tokio::test]
async fn confirmed_among_excludes_rejected_and_proposed_only_faces() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "confirmed-among-excl").await;
    let rejected_face = detect_face(&test, asset, None).await;
    let proposed_face = detect_face(&test, asset, Some(unit_axis(2))).await;
    let person = seed_person(&test).await;
    let candidate = seed_person(&test).await;
    let repo = FaceRepo::new(test.db());
    repo.assign(&ctx, rejected_face.id, person).await.unwrap();
    repo.reject(&ctx, rejected_face.id).await.unwrap();
    repo.propose(proposed_face.id, candidate, 0.62)
        .await
        .unwrap();

    let map = repo.confirmed_among(&[asset]).await.unwrap();

    assert!(
        !map.contains_key(&asset),
        "neither a rejected assignment nor an undecided proposal counts as confirmed"
    );
}

#[tokio::test]
async fn confirmed_among_is_empty_for_an_empty_id_list() {
    let test = TestDb::start().await;
    let map = FaceRepo::new(test.db()).confirmed_among(&[]).await.unwrap();
    assert!(map.is_empty());
}
