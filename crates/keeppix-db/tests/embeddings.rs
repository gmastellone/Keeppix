//! The pending queue and upsert for `asset_embeddings`.

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, EmbeddingRepo, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole, UserId,
};

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library(
    test: &TestDb,
    owner: UserId,
    name: &str,
    path: &str,
) -> keeppix_domain::LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: name.to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from(path),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn discovered(folder: FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("name"),
        size_bytes: 100,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_hashed_asset(
    test: &TestDb,
    folder: FolderId,
    filename: &str,
    hash: [u8; 32],
) -> AssetId {
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, filename))
        .await
        .unwrap()
        .unwrap();
    AssetRepo::new(test.db())
        .set_hash(asset.id, hash)
        .await
        .unwrap();
    asset.id
}

const MODEL: &str = "openclip-xlmr-it-en";

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn list_pending_skips_assets_already_embedded_for_the_same_model() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/emb-skip").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();

    let done = seed_hashed_asset(&test, folder.id, "done.jpg", [0x11; 32]).await;
    let pending = seed_hashed_asset(&test, folder.id, "todo.jpg", [0x22; 32]).await;

    let zeros = vec![0.0_f32; 512];
    EmbeddingRepo::new(test.db())
        .upsert(done, &zeros, MODEL)
        .await
        .unwrap();

    let batch = EmbeddingRepo::new(test.db())
        .list_pending(MODEL, 100)
        .await
        .unwrap();
    let ids: Vec<_> = batch.iter().map(|p| p.asset_id).collect();
    assert!(
        !ids.contains(&done),
        "already embedded with the same model_version must not reappear"
    );
    assert!(
        ids.contains(&pending),
        "with no embedding it must stay in the queue"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn list_pending_excludes_the_entire_culling_subtree() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/emb-cull").await;
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

    let batch = EmbeddingRepo::new(test.db())
        .list_pending(MODEL, 100)
        .await
        .unwrap();
    let ids: Vec<_> = batch.iter().map(|p| p.asset_id).collect();
    assert!(ids.contains(&keep), "outside culling it must remain");
    assert!(
        !ids.contains(&in_root),
        "the culling root is excluded entirely"
    );
    assert!(
        !ids.contains(&in_taken),
        "the culling subtree is excluded via path <@"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn list_pending_is_inert_when_culling_root_is_null() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/emb-null").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["any"])
        .await
        .unwrap();
    let asset = seed_hashed_asset(&test, folder.id, "a.jpg", [0x66; 32]).await;

    let batch = EmbeddingRepo::new(test.db())
        .list_pending(MODEL, 100)
        .await
        .unwrap();
    assert!(
        batch.iter().any(|p| p.asset_id == asset),
        "with no culling_root the predicate must not filter anything"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn upsert_stores_a_512d_vector_and_is_idempotent_for_same_model() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/emb-up").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let asset = seed_hashed_asset(&test, folder.id, "a.jpg", [0x77; 32]).await;

    let mut emb = vec![0.0_f32; 512];
    emb[0] = 1.0;
    let repo = EmbeddingRepo::new(test.db());
    repo.upsert(asset, &emb, MODEL).await.unwrap();
    repo.upsert(asset, &emb, MODEL).await.unwrap();

    let stored = repo.get(asset).await.unwrap().expect("row");
    assert_eq!(stored.model_version, MODEL);
    assert_eq!(stored.embedding.len(), 512);
    assert!((stored.embedding[0] - 1.0).abs() < 1e-5);
}
