mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, StackRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, LibraryId, NewAsset, NewLibrary,
    SystemRole, UserId,
};
use uuid::Uuid;

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library(test: &TestDb, owner: UserId) -> LibraryId {
    LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("library")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_folder(test: &TestDb, owner: UserId) -> FolderId {
    let library = seed_library(test, owner).await;
    FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .expect("folder")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn discovered(folder: FolderId, filename: &str, kind: AssetKind) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("name"),
        size_bytes: 1000,
        mtime: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind,
    }
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_asset(test: &TestDb, folder: FolderId, filename: &str, kind: AssetKind) -> AssetId {
    AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, filename, kind))
        .await
        .expect("asset")
        .unwrap()
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn stack_id_of(test: &TestDb, asset: AssetId) -> Option<Uuid> {
    sqlx::query_scalar("SELECT stack_id FROM assets WHERE id = $1")
        .bind(asset.as_uuid())
        .fetch_one(test.db().pool())
        .await
        .expect("read stack_id")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn primary_of(test: &TestDb, stack: Uuid) -> Option<Uuid> {
    sqlx::query_scalar("SELECT primary_asset_id FROM stacks WHERE id = $1")
        .bind(stack)
        .fetch_optional(test.db().pool())
        .await
        .expect("read primary_asset_id")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn stack_count(test: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM stacks")
        .fetch_one(test.db().pool())
        .await
        .expect("stack count")
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn same_basename_in_the_same_folder_stacks_raw_and_jpeg_together() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    let raw = seed_asset(&test, folder, "DSC_0042.ARW", AssetKind::RawImage).await;
    let jpeg = seed_asset(&test, folder, "DSC_0042.JPG", AssetKind::Image).await;

    StackRepo::new(test.db())
        .regroup_folder(folder)
        .await
        .unwrap();

    let raw_stack = stack_id_of(&test, raw).await;
    let jpeg_stack = stack_id_of(&test, jpeg).await;
    assert!(raw_stack.is_some(), "the RAW must end up in a stack");
    assert_eq!(
        raw_stack, jpeg_stack,
        "same base name in the same folder: they must share the stack"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn the_raw_is_the_primary_asset_when_present() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    // The JPEG is written first and its name sorts before the RAW's
    // alphabetically ("JPG" < "NEF"), on purpose: neither write order nor
    // name ordering should decide the primary, only type should. With
    // ".ARW" (which sorts before ".JPG") this test would also pass with a
    // "first by name" fallback, without actually proving the preference
    // for the RAW.
    let jpeg = seed_asset(&test, folder, "DSC_0043.JPG", AssetKind::Image).await;
    let raw = seed_asset(&test, folder, "DSC_0043.NEF", AssetKind::RawImage).await;

    StackRepo::new(test.db())
        .regroup_folder(folder)
        .await
        .unwrap();

    let stack = stack_id_of(&test, jpeg).await.expect("stack");
    let primary = primary_of(&test, stack).await;
    assert_eq!(
        primary,
        Some(raw.as_uuid()),
        "the RAW carries more information: it must be the primary, not the JPEG"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_lone_jpeg_does_not_form_a_stack() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    let jpeg = seed_asset(&test, folder, "DSC_0099.JPG", AssetKind::Image).await;

    StackRepo::new(test.db())
        .regroup_folder(folder)
        .await
        .unwrap();

    assert_eq!(
        stack_id_of(&test, jpeg).await,
        None,
        "a single file with that base name must never form a stack"
    );
    assert_eq!(stack_count(&test).await, 0);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn three_files_with_the_same_basename_but_different_extensions_stack_together() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    // The RAW's extension was chosen on purpose because it sorts *after*
    // the other two alphabetically ("HEIC" < "JPG" < "NEF"): the primary
    // choice must come from type, not from a "first by name" fallback,
    // which here would pick the HEIC.
    let heic = seed_asset(&test, folder, "DSC_0100.HEIC", AssetKind::Image).await;
    let jpeg = seed_asset(&test, folder, "DSC_0100.JPG", AssetKind::Image).await;
    let raw = seed_asset(&test, folder, "DSC_0100.NEF", AssetKind::RawImage).await;

    StackRepo::new(test.db())
        .regroup_folder(folder)
        .await
        .unwrap();

    let stack = stack_id_of(&test, raw).await.expect("stack");
    assert_eq!(stack_id_of(&test, jpeg).await, Some(stack));
    assert_eq!(stack_id_of(&test, heic).await, Some(stack));
    assert_eq!(primary_of(&test, stack).await, Some(raw.as_uuid()));
    assert_eq!(
        stack_count(&test).await,
        1,
        "a single stack for the three files"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn deleting_the_primary_promotes_another_member_instead_of_orphaning_the_stack() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    let raw = seed_asset(&test, folder, "DSC_0200.ARW", AssetKind::RawImage).await;
    let jpeg = seed_asset(&test, folder, "DSC_0200.JPG", AssetKind::Image).await;
    let heic = seed_asset(&test, folder, "DSC_0200.HEIC", AssetKind::Image).await;

    let repo = StackRepo::new(test.db());
    repo.regroup_folder(folder).await.unwrap();
    let stack = stack_id_of(&test, jpeg).await.expect("stack");
    assert_eq!(primary_of(&test, stack).await, Some(raw.as_uuid()));

    // Direct deletion: this must hold up even without going through
    // StackRepo, exactly as the trash will do too.
    sqlx::query("DELETE FROM assets WHERE id = $1")
        .bind(raw.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let promoted = primary_of(&test, stack).await;
    assert!(
        promoted == Some(jpeg.as_uuid()) || promoted == Some(heic.as_uuid()),
        "the primary must be promoted to a surviving member, not {promoted:?}"
    );
    assert_ne!(
        promoted,
        Some(raw.as_uuid()),
        "the just-deleted RAW cannot remain primary"
    );

    // The stack is not orphaned: the two surviving members stay linked.
    assert_eq!(stack_id_of(&test, jpeg).await, Some(stack));
    assert_eq!(stack_id_of(&test, heic).await, Some(stack));
    assert_eq!(stack_count(&test).await, 1, "the stack must not disappear");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn regrouping_the_same_folder_twice_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let folder = seed_folder(&test, admin).await;

    let raw = seed_asset(&test, folder, "DSC_0300.ARW", AssetKind::RawImage).await;
    let jpeg = seed_asset(&test, folder, "DSC_0300.JPG", AssetKind::Image).await;

    let repo = StackRepo::new(test.db());
    repo.regroup_folder(folder).await.unwrap();
    let stack_first = stack_id_of(&test, jpeg)
        .await
        .expect("stack after the first pass");
    let primary_first = primary_of(&test, stack_first).await;
    assert_eq!(primary_first, Some(raw.as_uuid()));

    // A second scan of the same, unchanged folder: it must not create a
    // new stack or move the primary. Without this property, every rescan
    // would produce a new stack.
    repo.regroup_folder(folder).await.unwrap();
    let stack_second = stack_id_of(&test, jpeg)
        .await
        .expect("stack after the second pass");
    assert_eq!(
        stack_first, stack_second,
        "rescanning must not create a new stack for the same group"
    );
    assert_eq!(primary_of(&test, stack_second).await, primary_first);
    assert_eq!(
        stack_count(&test).await,
        1,
        "a single stack row, not one per scan"
    );
}
