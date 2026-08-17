mod harness;

use harness::TestDb;
use keeppix_db::{AssetRepo, ChangeLogRepo, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole, UserId,
};
use sqlx::PgConnection;

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_library_and_folder(test: &TestDb, owner: UserId) -> FolderId {
    let library = LibraryRepo::new(test.db())
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
        .expect("libreria")
        .id;
    FolderRepo::new(test.db())
        .ensure_path(library, &["2024"])
        .await
        .expect("cartella")
        .id
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
fn discovered(folder: FolderId, filename: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(filename).expect("nome"),
        size_bytes: 100,
        mtime: chrono::Utc::now(),
        inode: None,
        kind: AssetKind::Image,
    }
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn insert_asset_on(conn: &mut PgConnection, folder: FolderId, name: &str) -> AssetId {
    let id = AssetId::new();
    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime) \
         VALUES ($1, $2, $3, 100, now())",
    )
    .bind(id.as_uuid())
    .bind(folder.as_uuid())
    .bind(name)
    .execute(&mut *conn)
    .await
    .expect("insert asset");
    id
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn inserting_an_asset_appends_an_upsert() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = seed_library_and_folder(&test, admin).await;

    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, "a.jpg"))
        .await
        .unwrap()
        .unwrap();

    let page = ChangeLogRepo::new(test.db()).since(&ctx, 0).await.unwrap();
    assert!(page.upserted.contains(&asset.id));
    assert!(page.deleted.is_empty());
    assert!(!page.has_more);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn deleting_an_asset_appends_a_delete() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = seed_library_and_folder(&test, admin).await;
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, "a.jpg"))
        .await
        .unwrap()
        .unwrap();

    sqlx::query("DELETE FROM assets WHERE id = $1")
        .bind(asset.id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let page = ChangeLogRepo::new(test.db()).since(&ctx, 0).await.unwrap();
    assert!(page.deleted.contains(&asset.id));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn deleting_a_folder_still_logs_asset_deletes() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = seed_library_and_folder(&test, admin).await;
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, "a.jpg"))
        .await
        .unwrap()
        .unwrap();

    sqlx::query("DELETE FROM folders WHERE id = $1")
        .bind(folder.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let page = ChangeLogRepo::new(test.db()).since(&ctx, 0).await.unwrap();
    assert!(
        page.deleted.contains(&asset.id),
        "CASCADE dalla cartella non deve far cadere il log: folders è già sparita"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn overlapping_transactions_do_not_drop_rows() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = seed_library_and_folder(&test, admin).await;
    let repo = ChangeLogRepo::new(test.db());

    // Due transazioni sovrapposte: A prende seq più basso ma committà dopo B.
    // Un cursore che avanza fino al max seq visibile perderebbe A.
    let mut tx_a = test.db().pool().begin().await.unwrap();
    let id_a = insert_asset_on(&mut tx_a, folder, "a.jpg").await;

    let mut tx_b = test.db().pool().begin().await.unwrap();
    let id_b = insert_asset_on(&mut tx_b, folder, "b.jpg").await;
    tx_b.commit().await.unwrap();

    let first = repo.since(&ctx, 0).await.unwrap();
    assert!(
        first.upserted.contains(&id_b),
        "B è committata, deve comparire"
    );

    tx_a.commit().await.unwrap();

    let second = repo.since(&ctx, first.cursor).await.unwrap();
    let seen: Vec<AssetId> = first
        .upserted
        .iter()
        .chain(second.upserted.iter())
        .copied()
        .collect();
    assert!(
        seen.contains(&id_a),
        "A ha seq più basso ma è committata dopo: il cursore non deve averla saltata (cursor={})",
        first.cursor
    );
    assert!(seen.contains(&id_b));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_does_not_see_someone_elses_changes() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let folder = seed_library_and_folder(&test, admin).await;
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(discovered(folder, "a.jpg"))
        .await
        .unwrap()
        .unwrap();

    let page = ChangeLogRepo::new(test.db())
        .since(&AuthContext::user(mario, SystemRole::User), 0)
        .await
        .unwrap();
    assert!(
        !page.upserted.contains(&asset.id),
        "il delta non è un oracolo sulle foto altrui"
    );
}
