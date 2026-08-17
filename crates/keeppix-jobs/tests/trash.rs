#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, TrashRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, DiskAction, FolderId, LibraryId, NewAsset, NewLibrary,
    SystemRole, UserId,
};

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_library_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keeppix-jobs-trash-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("orologio")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("radice");
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
        .expect("libreria")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn discovered(folder: FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: 9,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: None,
        kind: AssetKind::Image,
    }
}

/// Una riga oltre la finestra sparisce **senza** chiamare `TrashRepo` dal
/// test: passa dal job di manutenzione. Una più recente resta.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn expired_trash_is_removed_by_the_maintenance_job_without_a_manual_empty() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let root = temp_library_root();
    let library = seed_library(&test, admin, &root).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .unwrap();
    fs::create_dir_all(root.join("2024")).unwrap();

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

    keeppix_jobs::cleanup_trash::run(test.db(), 30)
        .await
        .unwrap();

    let old_trash = PathBuf::from(old_entry.trash_path.unwrap());
    assert!(
        !old_trash.exists(),
        "il file oltre la finestra deve sparire da solo"
    );
    let old_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM trash_entries WHERE id = $1")
        .bind(old_entry.id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(old_rows, 0);

    let recent_trash = PathBuf::from(recent_entry.trash_path.clone().unwrap());
    assert!(recent_trash.exists(), "il cestino recente resta");
    let recent_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM trash_entries WHERE id = $1")
        .bind(recent_entry.id.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(recent_rows, 1);

    let _ = fs::remove_dir_all(&root);
}
