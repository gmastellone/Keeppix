mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AssetStatus, AuthContext, FolderId, LibraryId, NewAsset,
    NewLibrary, SystemRole, UserId,
};

#[allow(clippy::expect_used)]
async fn seed_library(test: &TestDb, owner: UserId, name: &str, path: &str) -> LibraryId {
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
        .expect("libreria")
        .id
}

#[allow(clippy::expect_used)]
fn discovered(folder: FolderId, filename: &str, size: i64) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: size,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(42),
        kind: AssetKind::Image,
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn upsert_discovered_is_idempotent_and_refreshes_stat() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());

    let first = repo
        .upsert_discovered(discovered(folder.id, "DSC_0042.ARW", 1000))
        .await
        .unwrap();
    assert_eq!(first.status, AssetStatus::Discovered);
    assert_eq!(first.size_bytes, 1000);

    let mut again = discovered(folder.id, "DSC_0042.ARW", 2000);
    again.mtime = Utc.with_ymd_and_hms(2024, 6, 2, 12, 0, 0).unwrap();
    let second = repo.upsert_discovered(again).await.unwrap();

    assert_eq!(first.id, second.id, "riscansionare non duplica l'asset");
    assert_eq!(second.size_bytes, 2000);
    assert_eq!(
        second.mtime,
        Utc.with_ymd_and_hms(2024, 6, 2, 12, 0, 0).unwrap()
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_same_filename_in_two_folders_is_two_assets() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folders = FolderRepo::new(test.db());
    let a = folders.ensure_path(library, &["a"]).await.unwrap();
    let b = folders.ensure_path(library, &["b"]).await.unwrap();
    let repo = AssetRepo::new(test.db());

    let left = repo
        .upsert_discovered(discovered(a.id, "DSC_0042.ARW", 100))
        .await
        .unwrap();
    let right = repo
        .upsert_discovered(discovered(b.id, "DSC_0042.ARW", 100))
        .await
        .unwrap();

    assert_ne!(left.id, right.id);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn the_same_hash_on_two_assets_is_not_a_conflict() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());
    let hash = [0xab_u8; 32];

    let a = repo
        .upsert_discovered(discovered(folder.id, "a.jpg", 100))
        .await
        .unwrap();
    let b = repo
        .upsert_discovered(discovered(folder.id, "b.jpg", 100))
        .await
        .unwrap();
    repo.set_hash(a.id, hash).await.unwrap();
    repo.set_hash(b.id, hash).await.unwrap();

    let found = repo.find_by_hash(&ctx, &hash).await.unwrap();
    let ids: Vec<_> = found.iter().map(|asset| asset.id).collect();
    assert!(ids.contains(&a.id) && ids.contains(&b.id));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn status_transitions_follow_the_pipeline() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());

    let asset = repo
        .upsert_discovered(discovered(folder.id, "DSC_0042.ARW", 100))
        .await
        .unwrap();
    assert_eq!(asset.status, AssetStatus::Discovered);

    repo.set_indexed(
        asset.id,
        Utc.with_ymd_and_hms(2024, 6, 1, 10, 0, 0).unwrap(),
        6000,
        4000,
    )
    .await
    .unwrap();
    let indexed = repo.find_by_id(&ctx, asset.id).await.unwrap();
    assert_eq!(indexed.status, AssetStatus::Indexed);
    assert_eq!(indexed.width, Some(6000));
    assert_eq!(indexed.height, Some(4000));

    repo.set_error(asset.id, "unreadable").await.unwrap();
    assert_eq!(
        repo.find_by_id(&ctx, asset.id).await.unwrap().status,
        AssetStatus::Error
    );

    repo.mark_offline(asset.id).await.unwrap();
    assert_eq!(
        repo.find_by_id(&ctx, asset.id).await.unwrap().status,
        AssetStatus::Offline
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_cannot_read_someone_elses_assets() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(discovered(folder.id, "DSC_0042.ARW", 100))
        .await
        .unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        repo.find_by_id(&mario_ctx, asset.id).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.find_by_folder(&mario_ctx, folder.id).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_an_unknown_asset_id_is_forbidden() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let repo = AssetRepo::new(test.db());

    assert!(matches!(
        repo.find_by_id(&mario_ctx, AssetId::new()).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_library_tree_and_assets_round_trip_with_permissions() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folders = FolderRepo::new(test.db());
    let leaf = folders
        .ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();
    assert_eq!(leaf.depth, 4, "radice piu tre livelli");

    let repo = AssetRepo::new(test.db());
    for i in 1..=12 {
        repo.upsert_discovered(discovered(leaf.id, &format!("DSC_{i:04}.ARW"), 1000 + i))
            .await
            .unwrap();
    }

    let listed = repo.find_by_folder(&ctx, leaf.id).await.unwrap();
    assert_eq!(listed.len(), 12);
    assert_eq!(
        repo.count_by_status(&ctx, AssetStatus::Discovered)
            .await
            .unwrap(),
        12
    );

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        repo.find_by_folder(&mario_ctx, leaf.id).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn moving_a_folder_does_not_touch_assets() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folders = FolderRepo::new(test.db());
    folders
        .ensure_path(library, &["2024", "Grecia"])
        .await
        .unwrap();
    let archive = folders.ensure_path(library, &["Archivio"]).await.unwrap();
    let root = folders.ensure_root(library).await.unwrap();
    let y2024 = folders.ensure_child(&root, "2024").await.unwrap();
    let greece = folders.ensure_child(&y2024, "Grecia").await.unwrap();

    let repo = AssetRepo::new(test.db());
    let asset = repo
        .upsert_discovered(discovered(greece.id, "DSC_0042.ARW", 100))
        .await
        .unwrap();

    folders
        .move_subtree(&ctx, greece.id, archive.id)
        .await
        .unwrap();

    let after = repo.find_by_id(&ctx, asset.id).await.unwrap();
    assert_eq!(after.folder_id, greece.id, "l'asset resta sulla cartella");
    assert_eq!(after.filename.as_str(), "DSC_0042.ARW");
    assert_eq!(after.size_bytes, 100);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn find_by_folder_omits_trashed_assets() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = seed_library(&test, admin, "Foto", "/mnt/foto").await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    let repo = AssetRepo::new(test.db());
    let live = repo
        .upsert_discovered(discovered(folder.id, "live.jpg", 10))
        .await
        .unwrap();
    let dumped = repo
        .upsert_discovered(discovered(folder.id, "bin.jpg", 10))
        .await
        .unwrap();
    sqlx::query("UPDATE assets SET status = 'trashed' WHERE id = $1")
        .bind(dumped.id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let listed = repo.find_by_folder(&ctx, folder.id).await.unwrap();
    let ids: Vec<_> = listed.iter().map(|a| a.id).collect();
    assert!(ids.contains(&live.id));
    assert!(!ids.contains(&dumped.id));
}
