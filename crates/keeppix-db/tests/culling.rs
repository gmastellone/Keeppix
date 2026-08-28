//! `CullingRepo::list_lots`.
//! `CullingRepo::set_pick` and `CullingRepo::empty_skipped`.

mod harness;

use std::fs;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AssetRepo, CullingRepo, DbError, FlagRepo, FolderRepo, LibraryRepo, NewGrant, ObjectType,
    PermissionRepo, SubjectType,
};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, CullingRole, NewAsset, NewLibrary, ObjectRole, Pick,
    SystemRole, UserId,
};

#[allow(clippy::expect_used)]
async fn seed_library(test: &TestDb, owner: UserId) -> keeppix_domain::LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from("/mnt/culling-test"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library")
        .id
}

/// A real library root on the filesystem, not `/mnt/culling-test`: `set_pick`
/// calls `AssetRepo::move_asset`, which does a real `rename()` (same
/// principle as `tests/assets.rs::move_asset`).
#[allow(clippy::expect_used, clippy::unwrap_used)]
fn temp_library_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "keeppix-culling-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create test root");
    root
}

#[allow(clippy::expect_used)]
async fn seed_library_at(
    test: &TestDb,
    owner: UserId,
    root: &std::path::Path,
) -> keeppix_domain::LibraryId {
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

#[allow(clippy::expect_used)]
fn discovered(folder: keeppix_domain::FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("name"),
        size_bytes: 9,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: None,
        kind: AssetKind::Image,
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn returns_empty_when_no_root_is_designated() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::User);

    let lots = CullingRepo::new(test.db())
        .list_lots(&ctx, library)
        .await
        .unwrap();

    assert!(
        lots.is_empty(),
        "with no designated root, culling behaves as before: no lots"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn counts_pending_taken_and_skipped_exactly_per_lot() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::User);
    let folders = FolderRepo::new(test.db());
    let assets = AssetRepo::new(test.db());

    let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
    LibraryRepo::new(test.db())
        .set_culling_root(&ctx, library, Some(culling_root.id))
        .await
        .unwrap();

    let lot = folders
        .ensure_child(&culling_root, "Vacanze 2026-07")
        .await
        .unwrap();
    let taken_folder = folders
        .ensure_culling_child(&lot, CullingRole::Taken)
        .await
        .unwrap();
    let skipped_folder = folders
        .ensure_culling_child(&lot, CullingRole::Skipped)
        .await
        .unwrap();

    // Two pending, directly in the lot's root.
    for name in ["a.jpg", "b.jpg"] {
        let asset = assets
            .upsert_discovered(discovered(lot.id, name))
            .await
            .unwrap()
            .unwrap();
        assets
            .set_indexed(
                asset.id,
                Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
                100,
                100,
            )
            .await
            .unwrap();
    }
    // Three taken (picked).
    for name in ["c.jpg", "d.jpg", "e.jpg"] {
        let asset = assets
            .upsert_discovered(discovered(taken_folder.id, name))
            .await
            .unwrap()
            .unwrap();
        assets
            .set_indexed(
                asset.id,
                Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
                100,
                100,
            )
            .await
            .unwrap();
    }
    // One skipped.
    let scartata = assets
        .upsert_discovered(discovered(skipped_folder.id, "f.jpg"))
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(
            scartata.id,
            Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            100,
            100,
        )
        .await
        .unwrap();

    let lots = CullingRepo::new(test.db())
        .list_lots(&ctx, library)
        .await
        .unwrap();

    assert_eq!(lots.len(), 1);
    assert_eq!(lots[0].folder_id, lot.id);
    assert_eq!(lots[0].name, "Vacanze 2026-07");
    assert_eq!(lots[0].pending, 2);
    assert_eq!(lots[0].taken, 3);
    assert_eq!(lots[0].skipped, 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_not_yet_indexed_asset_does_not_count() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::User);
    let folders = FolderRepo::new(test.db());
    let assets = AssetRepo::new(test.db());

    let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
    LibraryRepo::new(test.db())
        .set_culling_root(&ctx, library, Some(culling_root.id))
        .await
        .unwrap();
    let lot = folders
        .ensure_child(&culling_root, "Vacanze 2026-07")
        .await
        .unwrap();

    // Never passed through `set_indexed`: stays `discovered`, not yet a
    // real photo from the user's point of view.
    assets
        .upsert_discovered(discovered(lot.id, "appena-vista.jpg"))
        .await
        .unwrap()
        .unwrap();

    let lots = CullingRepo::new(test.db())
        .list_lots(&ctx, library)
        .await
        .unwrap();

    assert_eq!(lots[0].pending, 0);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn only_the_owner_or_admin_can_list_lots() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::User);
    let culling_root = FolderRepo::new(test.db())
        .ensure_path(library, &["Culling"])
        .await
        .unwrap();
    LibraryRepo::new(test.db())
        .set_culling_root(&ctx, library, Some(culling_root.id))
        .await
        .unwrap();

    let editor = harness::seed_user(&test, admin, "editor").await;
    PermissionRepo::new(test.db())
        .grant(
            &AuthContext::user(admin, SystemRole::Admin),
            NewGrant {
                subject: SubjectType::User,
                subject_id: editor.as_uuid(),
                object: ObjectType::Folder,
                object_id: culling_root.id.as_uuid(),
                role: ObjectRole::Editor,
                inherit: true,
            },
        )
        .await
        .unwrap();

    let editor_ctx = AuthContext::user(editor, SystemRole::User);
    let result = CullingRepo::new(test.db())
        .list_lots(&editor_ctx, library)
        .await;

    assert!(
        matches!(result, Err(keeppix_db::DbError::Forbidden)),
        "culling is an owner/admin scope, a shared editor is not enough: {result:?}"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn newest_lot_first() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::User);
    let folders = FolderRepo::new(test.db());

    let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
    LibraryRepo::new(test.db())
        .set_culling_root(&ctx, library, Some(culling_root.id))
        .await
        .unwrap();

    folders
        .ensure_child(&culling_root, "Vacanze 2026-07")
        .await
        .unwrap();
    folders
        .ensure_child(&culling_root, "Vacanze 2026-09")
        .await
        .unwrap();

    let lots = CullingRepo::new(test.db())
        .list_lots(&ctx, library)
        .await
        .unwrap();

    let names: Vec<&str> = lots.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, vec!["Vacanze 2026-09", "Vacanze 2026-07"]);
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn indexed_asset(
    assets: &AssetRepo<'_>,
    folder: keeppix_domain::FolderId,
    filename: &str,
) -> keeppix_domain::AssetId {
    let asset = assets
        .upsert_discovered(discovered(folder, filename))
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(
            asset.id,
            Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
            100,
            100,
        )
        .await
        .unwrap();
    asset.id
}

mod set_pick {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn outside_a_lot_only_the_flag_changes() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("outside");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let normal = folders.ensure_path(library, &["2024"]).await.unwrap();
        fs::create_dir_all(root.join("2024")).unwrap();
        fs::write(root.join("2024").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(&assets, normal.id, "a.jpg").await;

        let updated = CullingRepo::new(test.db())
            .set_pick(&ctx, asset_id, Pick::Pick)
            .await
            .unwrap();

        assert_eq!(
            updated.folder_id, normal.id,
            "no culling root designated: the file stays where it is"
        );
        assert!(root.join("2024").join("a.jpg").is_file());
        let flags = FlagRepo::new(test.db()).get(&ctx, asset_id).await.unwrap();
        assert_eq!(flags.pick, Pick::Pick);

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn picking_inside_a_lot_moves_the_file_into_taken() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("pick");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
        LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library, Some(culling_root.id))
            .await
            .unwrap();
        let lot = folders
            .ensure_child(&culling_root, "Vacanze")
            .await
            .unwrap();
        fs::create_dir_all(root.join("Culling").join("Vacanze")).unwrap();
        fs::write(root.join("Culling").join("Vacanze").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(&assets, lot.id, "a.jpg").await;

        let updated = CullingRepo::new(test.db())
            .set_pick(&ctx, asset_id, Pick::Pick)
            .await
            .unwrap();

        assert_ne!(updated.folder_id, lot.id, "moved out of the lot");
        let new_path = root
            .join("Culling")
            .join("Vacanze")
            .join("_taken")
            .join("a.jpg");
        assert!(new_path.is_file(), "the file ended up in _taken on disk");
        assert!(!root.join("Culling").join("Vacanze").join("a.jpg").exists());
        let flags = FlagRepo::new(test.db()).get(&ctx, asset_id).await.unwrap();
        assert_eq!(flags.pick, Pick::Pick);

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn rejecting_inside_a_lot_moves_the_file_into_skipped() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("reject");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
        LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library, Some(culling_root.id))
            .await
            .unwrap();
        let lot = folders
            .ensure_child(&culling_root, "Vacanze")
            .await
            .unwrap();
        fs::create_dir_all(root.join("Culling").join("Vacanze")).unwrap();
        fs::write(root.join("Culling").join("Vacanze").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(&assets, lot.id, "a.jpg").await;

        CullingRepo::new(test.db())
            .set_pick(&ctx, asset_id, Pick::Reject)
            .await
            .unwrap();

        let new_path = root
            .join("Culling")
            .join("Vacanze")
            .join("_skipped")
            .join("a.jpg");
        assert!(new_path.is_file());

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn changing_mind_moves_from_skipped_to_taken() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("change-mind");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());
        let culling = CullingRepo::new(test.db());

        let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
        LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library, Some(culling_root.id))
            .await
            .unwrap();
        let lot = folders
            .ensure_child(&culling_root, "Vacanze")
            .await
            .unwrap();
        fs::create_dir_all(root.join("Culling").join("Vacanze")).unwrap();
        fs::write(root.join("Culling").join("Vacanze").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(&assets, lot.id, "a.jpg").await;

        culling
            .set_pick(&ctx, asset_id, Pick::Reject)
            .await
            .unwrap();
        let updated = culling.set_pick(&ctx, asset_id, Pick::Pick).await.unwrap();

        assert_ne!(updated.folder_id, lot.id);
        let taken_path = root
            .join("Culling")
            .join("Vacanze")
            .join("_taken")
            .join("a.jpg");
        let skipped_path = root
            .join("Culling")
            .join("Vacanze")
            .join("_skipped")
            .join("a.jpg");
        assert!(taken_path.is_file(), "moved again, this time into _taken");
        assert!(!skipped_path.exists(), "must not also remain in _skipped");

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn clearing_the_pick_moves_the_file_back_to_the_lot_root() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("clear");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());
        let culling = CullingRepo::new(test.db());

        let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
        LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library, Some(culling_root.id))
            .await
            .unwrap();
        let lot = folders
            .ensure_child(&culling_root, "Vacanze")
            .await
            .unwrap();
        fs::create_dir_all(root.join("Culling").join("Vacanze")).unwrap();
        fs::write(root.join("Culling").join("Vacanze").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(&assets, lot.id, "a.jpg").await;

        culling.set_pick(&ctx, asset_id, Pick::Pick).await.unwrap();
        let updated = culling.set_pick(&ctx, asset_id, Pick::None).await.unwrap();

        assert_eq!(
            updated.folder_id, lot.id,
            "moves back into the lot, pending again"
        );
        assert!(root.join("Culling").join("Vacanze").join("a.jpg").is_file());
        let flags = FlagRepo::new(test.db()).get(&ctx, asset_id).await.unwrap();
        assert_eq!(flags.pick, Pick::None);

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn without_editor_rights_the_call_is_forbidden_and_the_flag_is_untouched() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("no-editor");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());

        let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
        LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library, Some(culling_root.id))
            .await
            .unwrap();
        let lot = folders
            .ensure_child(&culling_root, "Vacanze")
            .await
            .unwrap();
        fs::create_dir_all(root.join("Culling").join("Vacanze")).unwrap();
        fs::write(root.join("Culling").join("Vacanze").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(&assets, lot.id, "a.jpg").await;

        // A viewer shared only on the lot: sees the asset (for
        // FlagRepo::get/set) but is not an editor, so the physical move
        // inside the lot must stop it before the flag changes.
        let viewer = harness::seed_user(&test, admin, "viewer").await;
        PermissionRepo::new(test.db())
            .grant(
                &ctx,
                NewGrant {
                    subject: SubjectType::User,
                    subject_id: viewer.as_uuid(),
                    object: ObjectType::Folder,
                    object_id: culling_root.id.as_uuid(),
                    role: ObjectRole::Viewer,
                    inherit: true,
                },
            )
            .await
            .unwrap();
        let viewer_ctx = AuthContext::user(viewer, SystemRole::User);

        let before = FlagRepo::new(test.db())
            .get(&viewer_ctx, asset_id)
            .await
            .unwrap();
        let result = CullingRepo::new(test.db())
            .set_pick(&viewer_ctx, asset_id, Pick::Pick)
            .await;

        assert!(matches!(result, Err(DbError::Forbidden)), "{result:?}");
        assert!(
            root.join("Culling").join("Vacanze").join("a.jpg").is_file(),
            "the file has not moved"
        );
        let after = FlagRepo::new(test.db())
            .get(&viewer_ctx, asset_id)
            .await
            .unwrap();
        assert_eq!(before, after, "the flag stays as it was, not half-updated");

        let _ = fs::remove_dir_all(&root);
    }
}

mod empty_skipped {
    use super::*;

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn purges_every_asset_currently_in_skipped_and_returns_the_count() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("empty-skipped");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());
        let culling = CullingRepo::new(test.db());

        let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
        LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library, Some(culling_root.id))
            .await
            .unwrap();
        let lot = folders
            .ensure_child(&culling_root, "Vacanze")
            .await
            .unwrap();
        fs::create_dir_all(root.join("Culling").join("Vacanze")).unwrap();

        for name in ["a.jpg", "b.jpg"] {
            fs::write(root.join("Culling").join("Vacanze").join(name), b"x").unwrap();
            let asset_id = indexed_asset(&assets, lot.id, name).await;
            culling
                .set_pick(&ctx, asset_id, Pick::Reject)
                .await
                .unwrap();
        }
        // A third asset, picked instead of skipped: it must not disappear.
        fs::write(root.join("Culling").join("Vacanze").join("c.jpg"), b"x").unwrap();
        let kept_id = indexed_asset(&assets, lot.id, "c.jpg").await;
        culling.set_pick(&ctx, kept_id, Pick::Pick).await.unwrap();

        let purged = culling.empty_skipped(&ctx, lot.id).await.unwrap();

        assert_eq!(purged.len(), 2);
        assert!(
            purged.iter().all(|(_, result)| result.is_ok()),
            "{purged:?}"
        );
        let skipped_dir = root.join("Culling").join("Vacanze").join("_skipped");
        assert!(
            skipped_dir.read_dir().unwrap().next().is_none(),
            "_skipped is empty on disk"
        );
        assert!(
            root.join("Culling")
                .join("Vacanze")
                .join("_taken")
                .join("c.jpg")
                .is_file(),
            "the picked asset was not touched"
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn an_editor_who_is_not_the_owner_cannot_purge() {
        let test = TestDb::start().await;
        let admin = harness::seed_admin(&test).await;
        let root = temp_library_root("empty-skipped-forbidden");
        let library = seed_library_at(&test, admin, &root).await;
        let ctx = AuthContext::user(admin, SystemRole::Admin);
        let folders = FolderRepo::new(test.db());
        let assets = AssetRepo::new(test.db());
        let culling = CullingRepo::new(test.db());

        let culling_root = folders.ensure_path(library, &["Culling"]).await.unwrap();
        LibraryRepo::new(test.db())
            .set_culling_root(&ctx, library, Some(culling_root.id))
            .await
            .unwrap();
        let lot = folders
            .ensure_child(&culling_root, "Vacanze")
            .await
            .unwrap();
        fs::create_dir_all(root.join("Culling").join("Vacanze")).unwrap();
        fs::write(root.join("Culling").join("Vacanze").join("a.jpg"), b"x").unwrap();
        let asset_id = indexed_asset(&assets, lot.id, "a.jpg").await;
        culling
            .set_pick(&ctx, asset_id, Pick::Reject)
            .await
            .unwrap();

        let editor = harness::seed_user(&test, admin, "editor").await;
        PermissionRepo::new(test.db())
            .grant(
                &ctx,
                NewGrant {
                    subject: SubjectType::User,
                    subject_id: editor.as_uuid(),
                    object: ObjectType::Folder,
                    object_id: culling_root.id.as_uuid(),
                    role: ObjectRole::Editor,
                    inherit: true,
                },
            )
            .await
            .unwrap();
        let editor_ctx = AuthContext::user(editor, SystemRole::User);

        let result = culling.empty_skipped(&editor_ctx, lot.id).await;

        assert!(matches!(result, Err(DbError::Forbidden)), "{result:?}");
        assert!(
            root.join("Culling")
                .join("Vacanze")
                .join("_skipped")
                .join("a.jpg")
                .is_file(),
            "a non-owner editor cannot destroy files, nothing moves"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
