//! Fase 7 Task 7: CRUD vocabolario condiviso `tags` (un livello di nesting).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, NewTag, TagPatch, TagRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole, TagId,
    TagKind, UserId,
};

async fn seed_library(test: &TestDb, owner: UserId, path: &str) -> keeppix_domain::LibraryId {
    LibraryRepo::new(test.db())
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
    AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, filename))
        .await
        .unwrap()
        .unwrap()
        .id
}

async fn attach_assignment(test: &TestDb, asset_id: AssetId, tag_id: TagId) {
    sqlx::query(
        "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) \
         VALUES ($1, $2, 'proposed', 'ai', 0.9)",
    )
    .bind(asset_id.as_uuid())
    .bind(tag_id.as_uuid())
    .execute(test.db().pool())
    .await
    .expect("asset_tags insert");
}

#[tokio::test]
async fn creating_a_category_and_a_child_tag_round_trips() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let nature = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Natura".to_owned(),
                kind: TagKind::Category,
                parent_id: None,
                prompt: None,
                color: Some("#2d6a4f".to_owned()),
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(nature.kind, TagKind::Category);
    assert!(nature.parent_id.is_none());
    assert!((nature.threshold - 0.75).abs() < f32::EPSILON);
    assert_eq!(nature.assignment_count, 0);

    let wildlife = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Fauna selvatica".to_owned(),
                kind: TagKind::Tag,
                parent_id: Some(nature.id),
                prompt: Some("wild animals in nature".to_owned()),
                color: None,
                threshold: Some(0.8),
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(wildlife.parent_id, Some(nature.id));
    assert_eq!(wildlife.prompt.as_deref(), Some("wild animals in nature"));
    assert!((wildlife.threshold - 0.8).abs() < f32::EPSILON);

    let listed = TagRepo::new(test.db()).list(&ctx).await.unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().any(|t| t.id == nature.id));
    assert!(listed.iter().any(|t| t.id == wildlife.id));

    let got = TagRepo::new(test.db())
        .get(&ctx, wildlife.id)
        .await
        .unwrap();
    assert_eq!(got.name, "Fauna selvatica");
}

#[tokio::test]
async fn a_category_cannot_have_a_parent_and_a_tag_parent_must_be_a_category() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let leaf = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Tramonto".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();

    let err = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Bad cat".to_owned(),
                kind: TagKind::Category,
                parent_id: Some(leaf.id),
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Conflict(_)));

    let err = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Nested tag".to_owned(),
                kind: TagKind::Tag,
                parent_id: Some(leaf.id),
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Conflict(_)));
}

#[tokio::test]
async fn tag_names_are_unique_per_kind() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Mare".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();

    // Stesso nome, stesso kind → conflict.
    let err = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Mare".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Conflict(_)));

    // Stesso nome, kind diverso → ok (UNIQUE è su (name, kind)).
    TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Mare".to_owned(),
                kind: TagKind::Category,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn unknown_tag_id_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let missing = TagId::from_uuid(uuid::Uuid::now_v7());

    let err = TagRepo::new(test.db())
        .get(&ctx, missing)
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Forbidden));

    let err = TagRepo::new(test.db())
        .update(
            &ctx,
            missing,
            TagPatch {
                name: Some("x".to_owned()),
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Forbidden));

    let err = TagRepo::new(test.db())
        .delete(&ctx, missing)
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Forbidden));
}

#[tokio::test]
async fn deleting_a_tag_cascades_assignments_and_get_reports_the_count() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Spiaggia".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();

    let library = seed_library(&test, admin, "/mnt/tags-cascade").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let a = seed_asset(&test, folder.id, "a.jpg").await;
    let b = seed_asset(&test, folder.id, "b.jpg").await;
    attach_assignment(&test, a, tag.id).await;
    attach_assignment(&test, b, tag.id).await;

    let got = TagRepo::new(test.db()).get(&ctx, tag.id).await.unwrap();
    assert_eq!(got.assignment_count, 2);

    TagRepo::new(test.db()).delete(&ctx, tag.id).await.unwrap();

    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*)::bigint FROM asset_tags WHERE tag_id = $1")
            .bind(tag.id.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert_eq!(remaining, 0);
}

#[tokio::test]
async fn patching_stores_a_text_embedding_without_touching_asset_embeddings() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Neve".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();
    assert!(!tag.has_embedding);

    let vector = vec![0.01_f32; 512];
    let updated = TagRepo::new(test.db())
        .update(
            &ctx,
            tag.id,
            TagPatch {
                name: None,
                parent_id: None,
                prompt: Some(Some("fresh snow on mountains".to_owned())),
                color: None,
                threshold: None,
                embedding: Some(Some(vector.clone())),
                model_version: Some(Some("openclip-xlmr-it-en".to_owned())),
            },
        )
        .await
        .unwrap();
    assert!(updated.has_embedding);
    assert_eq!(updated.prompt.as_deref(), Some("fresh snow on mountains"));
    assert_eq!(
        updated.model_version.as_deref(),
        Some("openclip-xlmr-it-en")
    );

    let stored: String = sqlx::query_scalar("SELECT embedding::text FROM tags WHERE id = $1")
        .bind(tag.id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert!(
        stored.starts_with('['),
        "embedding materializzato: {stored}"
    );

    let asset_emb: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM asset_embeddings")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(asset_emb, 0, "il CRUD del tag non deve toccare le foto");
}

#[tokio::test]
async fn any_authenticated_user_can_manage_the_shared_vocabulary() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let plain = harness::seed_user(&test, admin, "mario").await;
    let ctx = AuthContext::user(plain, SystemRole::User);

    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Cibo".to_owned(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(tag.created_by, plain);

    let listed = TagRepo::new(test.db()).list(&ctx).await.unwrap();
    assert!(listed.iter().any(|t| t.id == tag.id));
}
