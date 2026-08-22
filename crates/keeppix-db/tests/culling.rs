//! Fase 9 Task 3: `CullingRepo::list_lots`.

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AssetRepo, CullingRepo, FolderRepo, LibraryRepo, NewGrant, ObjectType, PermissionRepo,
    SubjectType,
};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, CullingRole, NewAsset, NewLibrary, ObjectRole, SystemRole,
    UserId,
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
        .expect("libreria")
        .id
}

#[allow(clippy::expect_used)]
fn discovered(folder: keeppix_domain::FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
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
        "senza radice designata il culling si comporta come oggi: nessun lotto"
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

    // Due in attesa, direttamente nella radice del lotto.
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
    // Tre scelte.
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
    // Una scartata.
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

    // Mai passato da `set_indexed`: resta `discovered`, non ancora una
    // foto vera agli occhi dell'utente.
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
        "il culling è un ambito owner/admin, un editor condiviso non basta: {result:?}"
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
