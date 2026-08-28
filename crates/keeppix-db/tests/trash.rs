mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Duration, TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, FolderRepo, LibraryRepo, TRASH_DIR_NAME, TrashRepo};
use keeppix_domain::{
    AssetKind, AssetName, AssetStatus, AuthContext, DiskAction, FolderId, LibraryId, NewAsset,
    NewLibrary, SystemRole, UserId,
};

/// A real library root on the filesystem, not `/mnt/foto`: the trash does
/// a real `rename()`, so it needs a path it can actually write to.
#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_library_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keeppix-trash-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create test root");
    root
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library(test: &TestDb, owner: UserId, root: &Path) -> LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: root.to_path_buf(),
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
        size_bytes: 9,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: None,
        kind: AssetKind::Image,
    }
}

#[cfg(unix)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
fn inode_of(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt as _;
    fs::metadata(path).expect("stat").ino()
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn moving_to_trash_is_a_rename_that_keeps_the_inode() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `ensure_path` only creates the domain rows: the real folder on disk,
    // where the tests write real files, has to be created by hand.
    fs::create_dir_all(root.join("2024")).unwrap();
    let original = root.join("2024").join("foto.jpg");
    fs::write(&original, b"contenuto").unwrap();
    #[cfg(unix)]
    let inode_before = inode_of(&original);

    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "foto.jpg"))
        .await
        .unwrap()
        .unwrap();

    let entry = TrashRepo::new(test.db())
        .choose(&ctx, asset.id, DiskAction::MovedToTrash)
        .await
        .unwrap();

    assert!(
        !original.exists(),
        "the file must no longer be at the original path"
    );
    let trash_path = PathBuf::from(entry.trash_path.expect("trash_path for moved_to_trash"));
    assert!(trash_path.is_file(), "the file must exist in the trash");
    assert!(
        trash_path.starts_with(root.join(TRASH_DIR_NAME)),
        "the trash must live inside .keeppix-trash of the same library: {}",
        trash_path.display()
    );
    assert_eq!(
        fs::read(&trash_path).unwrap(),
        b"contenuto",
        "rename(), not a copy: it's the same file's content"
    );

    #[cfg(unix)]
    assert_eq!(
        inode_before,
        inode_of(&trash_path),
        "rename() on the same filesystem doesn't change the inode"
    );

    let after = AssetRepo::new(test.db())
        .find_by_id(&ctx, asset.id)
        .await
        .unwrap();
    assert_eq!(after.status, AssetStatus::Trashed);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn kept_removes_the_asset_from_the_index_but_leaves_the_file() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `ensure_path` only creates the domain rows: the real folder on disk,
    // where the tests write real files, has to be created by hand.
    fs::create_dir_all(root.join("2024")).unwrap();
    let original = root.join("2024").join("foto.jpg");
    fs::write(&original, b"contenuto").unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "foto.jpg"))
        .await
        .unwrap()
        .unwrap();

    TrashRepo::new(test.db())
        .choose(&ctx, asset.id, DiskAction::Kept)
        .await
        .unwrap();

    assert!(original.is_file(), "the file remains on disk");
    assert!(
        matches!(
            AssetRepo::new(test.db()).find_by_id(&ctx, asset.id).await,
            Err(DbError::NotFound)
        ),
        "the asset is gone from the index"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn purged_deletes_the_file_and_the_row() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `ensure_path` only creates the domain rows: the real folder on disk,
    // where the tests write real files, has to be created by hand.
    fs::create_dir_all(root.join("2024")).unwrap();
    let original = root.join("2024").join("foto.jpg");
    fs::write(&original, b"contenuto").unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "foto.jpg"))
        .await
        .unwrap()
        .unwrap();

    TrashRepo::new(test.db())
        .choose(&ctx, asset.id, DiskAction::Purged)
        .await
        .unwrap();

    assert!(!original.exists(), "the file is deleted from disk");
    assert!(matches!(
        AssetRepo::new(test.db()).find_by_id(&ctx, asset.id).await,
        Err(DbError::NotFound)
    ));

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn only_owner_and_admin_can_purge_an_editor_gets_forbidden() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let root = temp_library_root();
    // The library belongs to admin: mario is neither its owner nor an
    // admin — the closest available stand-in for an "editor" who doesn't
    // own the library, so the gate on Purged can be exercised even without
    // real sharing yet.
    let library = seed_library(&test, admin, &root).await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `ensure_path` only creates the domain rows: the real folder on disk,
    // where the tests write real files, has to be created by hand.
    fs::create_dir_all(root.join("2024")).unwrap();
    let original = root.join("2024").join("foto.jpg");
    fs::write(&original, b"contenuto").unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "foto.jpg"))
        .await
        .unwrap()
        .unwrap();

    // Note: mario cannot see this library at all (no sharing set up), so
    // even `kept`/`moved_to_trash` would be denied to him too — consistent
    // with "Forbidden always before any other check". Here mario is given
    // the only way to see it available: making him admin would have been a
    // way around the test, so the editor-without-Purged case is exercised
    // directly against the dedicated gate, not against visibility.
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        TrashRepo::new(test.db())
            .choose(&mario_ctx, asset.id, DiskAction::Purged)
            .await,
        Err(DbError::Forbidden)
    ));

    // The owner can.
    TrashRepo::new(test.db())
        .choose(&admin_ctx, asset.id, DiskAction::Purged)
        .await
        .unwrap();
    assert!(!original.exists());

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn restore_puts_the_file_back_and_marks_the_asset_indexed() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `ensure_path` only creates the domain rows: the real folder on disk,
    // where the tests write real files, has to be created by hand.
    fs::create_dir_all(root.join("2024")).unwrap();
    let original = root.join("2024").join("foto.jpg");
    fs::write(&original, b"contenuto").unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "foto.jpg"))
        .await
        .unwrap()
        .unwrap();
    TrashRepo::new(test.db())
        .choose(&ctx, asset.id, DiskAction::MovedToTrash)
        .await
        .unwrap();
    assert!(!original.exists());

    TrashRepo::new(test.db())
        .restore(&ctx, asset.id)
        .await
        .unwrap();

    assert!(original.is_file(), "the file returns to the original path");
    assert_eq!(fs::read(&original).unwrap(), b"contenuto");
    let after = AssetRepo::new(test.db())
        .find_by_id(&ctx, asset.id)
        .await
        .unwrap();
    assert_eq!(after.status, AssetStatus::Indexed);

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn restore_does_not_overwrite_a_file_that_now_occupies_the_original_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `ensure_path` only creates the domain rows: the real folder on disk,
    // where the tests write real files, has to be created by hand.
    fs::create_dir_all(root.join("2024")).unwrap();
    let original = root.join("2024").join("foto.jpg");
    fs::write(&original, b"originale").unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "foto.jpg"))
        .await
        .unwrap()
        .unwrap();
    let entry = TrashRepo::new(test.db())
        .choose(&ctx, asset.id, DiskAction::MovedToTrash)
        .await
        .unwrap();
    let trash_path = PathBuf::from(entry.trash_path.expect("trash_path"));

    // Another file comes to occupy the original path — for example a new
    // scan, or another application.
    fs::write(&original, b"un file diverso, non toccare").unwrap();

    let result = TrashRepo::new(test.db()).restore(&ctx, asset.id).await;
    assert!(matches!(result, Err(DbError::Conflict(_))));

    assert_eq!(
        fs::read(&original).unwrap(),
        b"un file diverso, non toccare",
        "restoring must not overwrite the file that occupies the spot"
    );
    assert!(
        trash_path.is_file(),
        "the trashed file remains in the trash, it isn't lost"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn restoring_an_asset_that_is_not_in_the_trash_is_a_conflict() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `ensure_path` only creates the domain rows: the real folder on disk,
    // where the tests write real files, has to be created by hand.
    fs::create_dir_all(root.join("2024")).unwrap();
    let original = root.join("2024").join("foto.jpg");
    fs::write(&original, b"contenuto").unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "foto.jpg"))
        .await
        .unwrap()
        .unwrap();

    let result = TrashRepo::new(test.db()).restore(&ctx, asset.id).await;
    assert!(matches!(result, Err(DbError::Conflict(_))));

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn cleanup_expired_deletes_the_file_and_the_row_past_the_cutoff() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `ensure_path` only creates the domain rows: the real folder on disk,
    // where the tests write real files, has to be created by hand.
    fs::create_dir_all(root.join("2024")).unwrap();

    // A trash entry 40 days old: it must be cleaned up.
    let old_path = root.join("2024").join("old.jpg");
    fs::write(&old_path, b"vecchio").unwrap();
    let old_asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "old.jpg"))
        .await
        .unwrap()
        .unwrap();
    let old_entry = TrashRepo::new(test.db())
        .choose(&ctx, old_asset.id, DiskAction::MovedToTrash)
        .await
        .unwrap();
    sqlx::query("UPDATE trash_entries SET deleted_at = now() - interval '40 days' WHERE id = $1")
        .bind(old_entry.id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    // A recent trash entry: it must remain untouched.
    let recent_path = root.join("2024").join("recent.jpg");
    fs::write(&recent_path, b"recente").unwrap();
    let recent_asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "recent.jpg"))
        .await
        .unwrap()
        .unwrap();
    let recent_entry = TrashRepo::new(test.db())
        .choose(&ctx, recent_asset.id, DiskAction::MovedToTrash)
        .await
        .unwrap();
    let recent_trash_path = PathBuf::from(recent_entry.trash_path.clone().unwrap());

    let cutoff = Utc::now() - Duration::days(30);
    let cleaned = TrashRepo::new(test.db())
        .cleanup_expired(cutoff)
        .await
        .unwrap();
    assert_eq!(
        cleaned, 1,
        "only the entry past 30 days should be cleaned up"
    );

    let old_trash_path = PathBuf::from(old_entry.trash_path.unwrap());
    assert!(
        !old_trash_path.exists(),
        "the file past 30 days is deleted from the trash"
    );
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM trash_entries WHERE id = $1")
        .bind(old_entry.id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0, "the audit row is removed after cleanup");
    let old_asset_row: i64 = sqlx::query_scalar("SELECT count(*) FROM assets WHERE id = $1")
        .bind(old_asset.id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(
        old_asset_row, 0,
        "the trashed asset is removed after cleanup"
    );

    assert!(
        recent_trash_path.is_file(),
        "the recent trash entry must not be touched"
    );

    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_someone_elses_asset_for_trash_is_forbidden() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    // `ensure_path` only creates the domain rows: the real folder on disk,
    // where the tests write real files, has to be created by hand.
    fs::create_dir_all(root.join("2024")).unwrap();
    fs::write(root.join("2024").join("foto.jpg"), b"contenuto").unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder.id, "foto.jpg"))
        .await
        .unwrap()
        .unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        TrashRepo::new(test.db())
            .choose(&mario_ctx, asset.id, DiskAction::Kept)
            .await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        TrashRepo::new(test.db())
            .restore(&mario_ctx, asset.id)
            .await,
        Err(DbError::Forbidden)
    ));

    let _ = fs::remove_dir_all(&root);
}
