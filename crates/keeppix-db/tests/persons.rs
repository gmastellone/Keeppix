//! Fase 8 Task 6/7: persone — visibilità transitiva via i volti, unisci,
//! separa, centroide.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{FaceRepo, FolderRepo, NewDetectedFace, PersonRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, PersonName,
    SystemRole, UserId,
};

fn unit_axis(axis: usize) -> Vec<f32> {
    let mut v = vec![0.0_f32; 128];
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

async fn confirmed_face(
    test: &TestDb,
    ctx: &AuthContext,
    asset_id: AssetId,
    person: keeppix_domain::PersonId,
    embedding: Vec<f32>,
) -> keeppix_domain::FaceId {
    let face = FaceRepo::new(test.db())
        .insert_detected(NewDetectedFace {
            asset_id,
            bbox: keeppix_domain::FaceBBox {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
            },
            landmarks: None,
            embedding: Some(embedding),
            detect_score: 0.9,
            quality: Some(0.8),
            model_version: "yunet+sface".to_owned(),
        })
        .await
        .unwrap();
    FaceRepo::new(test.db())
        .assign(ctx, face.id, person)
        .await
        .unwrap();
    face.id
}

#[tokio::test]
async fn a_person_is_invisible_to_a_user_who_cannot_see_any_of_their_photos() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let outsider = harness::seed_user(&test, admin, "outsider").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "private").await;

    let person = PersonRepo::new(test.db()).create(None).await.unwrap();
    confirmed_face(&test, &admin_ctx, asset, person.id, unit_axis(0)).await;

    let outsider_ctx = AuthContext::user(outsider, SystemRole::User);
    let repo = PersonRepo::new(test.db());
    assert!(matches!(
        repo.find_by_id(&outsider_ctx, person.id).await,
        Err(keeppix_db::DbError::Forbidden)
    ));
    assert!(
        repo.list_visible(&outsider_ctx, false)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_share_link_never_sees_any_person() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "shared").await;
    let person = PersonRepo::new(test.db()).create(None).await.unwrap();
    confirmed_face(&test, &admin_ctx, asset, person.id, unit_axis(0)).await;

    let share_ctx = AuthContext::share_link(
        uuid::Uuid::now_v7(),
        keeppix_domain::ShareLinkParams {
            object_type: "asset".to_owned(),
            object_id: asset.as_uuid(),
            allow_download: false,
            allow_original: false,
            hide_metadata: true,
            allow_upload: false,
            upload_quota_bytes: None,
        },
    );
    let repo = PersonRepo::new(test.db());
    assert!(matches!(
        repo.find_by_id(&share_ctx, person.id).await,
        Err(keeppix_db::DbError::Forbidden)
    ));
    assert!(
        repo.list_visible(&share_ctx, false)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn list_visible_reports_the_confirmed_face_count() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset_a = seed_asset_in_new_library(&test, admin, "count-a").await;
    let asset_b = seed_asset_in_new_library(&test, admin, "count-b").await;

    let person = PersonRepo::new(test.db())
        .create(Some(PersonName::parse("Marta").unwrap()))
        .await
        .unwrap();
    confirmed_face(&test, &ctx, asset_a, person.id, unit_axis(0)).await;
    confirmed_face(&test, &ctx, asset_b, person.id, unit_axis(0)).await;

    let summaries = PersonRepo::new(test.db())
        .list_visible(&ctx, false)
        .await
        .unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].face_count, 2);
    assert_eq!(summaries[0].person.name.as_deref(), Some("Marta"));
}

#[tokio::test]
async fn hidden_persons_are_excluded_unless_asked_for() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "hidden-person").await;
    let person = PersonRepo::new(test.db()).create(None).await.unwrap();
    confirmed_face(&test, &ctx, asset, person.id, unit_axis(0)).await;

    let repo = PersonRepo::new(test.db());
    repo.set_hidden(&ctx, person.id, true).await.unwrap();

    assert!(repo.list_visible(&ctx, false).await.unwrap().is_empty());
    assert_eq!(repo.list_visible(&ctx, true).await.unwrap().len(), 1);
}

#[tokio::test]
async fn renaming_to_a_blank_name_is_impossible_by_construction() {
    // `PersonName::parse` rifiuta il vuoto prima ancora di arrivare al repo
    // — il difetto del prototipo (Task 6) non ha modo di riprodursi qui.
    assert!(PersonName::parse("").is_err());
    assert!(PersonName::parse("   ").is_err());
}

#[tokio::test]
async fn merge_reassigns_faces_and_keeps_the_named_survivor() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset_a = seed_asset_in_new_library(&test, admin, "merge-a").await;
    let asset_b = seed_asset_in_new_library(&test, admin, "merge-b").await;

    let repo = PersonRepo::new(test.db());
    let survivor = repo.create(None).await.unwrap();
    let absorbed = repo
        .create(Some(PersonName::parse("Giovanni").unwrap()))
        .await
        .unwrap();
    confirmed_face(&test, &ctx, asset_a, survivor.id, unit_axis(0)).await;
    confirmed_face(&test, &ctx, asset_b, absorbed.id, unit_axis(0)).await;

    let merged = repo.merge(&ctx, survivor.id, &[absorbed.id]).await.unwrap();
    assert_eq!(merged.name.as_deref(), Some("Giovanni"));
    assert!(matches!(
        repo.find_by_id(&ctx, absorbed.id).await,
        Err(keeppix_db::DbError::NotFound)
    ));
    let summaries = repo.list_visible(&ctx, false).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].face_count, 2);
}

#[tokio::test]
async fn separate_creates_a_new_person_and_records_the_split() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset_a = seed_asset_in_new_library(&test, admin, "split-a").await;
    let asset_b = seed_asset_in_new_library(&test, admin, "split-b").await;

    let repo = PersonRepo::new(test.db());
    let source = repo
        .create(Some(PersonName::parse("Gemelli").unwrap()))
        .await
        .unwrap();
    let stays = confirmed_face(&test, &ctx, asset_a, source.id, unit_axis(0)).await;
    let leaves = confirmed_face(&test, &ctx, asset_b, source.id, unit_axis(1)).await;

    let split_off = repo
        .separate(
            &ctx,
            source.id,
            &[leaves],
            Some(PersonName::parse("Elena").unwrap()),
        )
        .await
        .unwrap();
    assert_ne!(split_off.id, source.id);
    assert_eq!(split_off.name.as_deref(), Some("Elena"));

    let source_faces = FaceRepo::new(test.db())
        .list_for_asset(&ctx, asset_a)
        .await
        .unwrap();
    assert_eq!(source_faces[0].id, stays);
    assert_eq!(source_faces[0].person_id, Some(source.id));
    let split_faces = FaceRepo::new(test.db())
        .list_for_asset(&ctx, asset_b)
        .await
        .unwrap();
    assert_eq!(split_faces[0].person_id, Some(split_off.id));

    let (lo, hi) = if source.id.as_uuid() < split_off.id.as_uuid() {
        (source.id, split_off.id)
    } else {
        (split_off.id, source.id)
    };
    let separation: Option<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT person_a FROM person_separations WHERE person_a = $1 AND person_b = $2",
    )
    .bind(lo.as_uuid())
    .bind(hi.as_uuid())
    .fetch_optional(test.db().pool())
    .await
    .unwrap();
    assert!(
        separation.is_some(),
        "the split must be recorded in person_separations"
    );
    assert!(repo.has_any_separation(source.id).await.unwrap());
    assert!(repo.has_any_separation(split_off.id).await.unwrap());
}

#[tokio::test]
async fn separate_does_not_restore_a_previous_state_on_a_second_call() {
    // Domanda aperta n.5 del documento funzionale: separare non è annullabile.
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset = seed_asset_in_new_library(&test, admin, "no-undo").await;

    let repo = PersonRepo::new(test.db());
    let source = repo.create(None).await.unwrap();
    let leaves = confirmed_face(&test, &ctx, asset, source.id, unit_axis(0)).await;

    let first_split = repo
        .separate(&ctx, source.id, &[leaves], None)
        .await
        .unwrap();
    // Un secondo tentativo di separare lo stesso volto (già altrove) fallisce
    // con un conflitto esplicito, non con un ripristino silenzioso.
    assert!(matches!(
        repo.separate(&ctx, source.id, &[leaves], None).await,
        Err(keeppix_db::DbError::Conflict(_))
    ));
    assert_ne!(first_split.id, source.id);
}

#[tokio::test]
async fn centroid_is_the_average_of_confirmed_embeddings() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let asset_a = seed_asset_in_new_library(&test, admin, "centroid-a").await;
    let asset_b = seed_asset_in_new_library(&test, admin, "centroid-b").await;

    let repo = PersonRepo::new(test.db());
    let person = repo.create(None).await.unwrap();
    confirmed_face(&test, &ctx, asset_a, person.id, unit_axis(0)).await;
    confirmed_face(&test, &ctx, asset_b, person.id, unit_axis(0)).await;

    let centroid: Option<(Option<String>,)> =
        sqlx::query_as("SELECT centroid::text FROM persons WHERE id = $1")
            .bind(person.id.as_uuid())
            .fetch_optional(test.db().pool())
            .await
            .unwrap();
    let centroid_text = centroid.unwrap().0.unwrap();
    assert!(
        centroid_text.starts_with("[1,0,0"),
        "centroid should average to the shared axis: {centroid_text}"
    );
}
