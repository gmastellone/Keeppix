#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, FolderId, NewAsset, NewLibrary, SystemRole,
};

async fn seed_folder(test: &TestDb) -> FolderId {
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    FolderRepo::new(test.db())
        .ensure_path(library.id, &["2024"])
        .await
        .unwrap()
        .id
}

fn photo(folder: FolderId, name: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(name).unwrap(),
        size_bytes: 100,
        mtime: Utc.with_ymd_and_hms(2024, 7, 15, 12, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

#[tokio::test]
async fn month_counts_match_indexed_assets_after_index_move_and_delete() {
    let test = TestDb::start().await;
    let folder = seed_folder(&test).await;
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, "a.jpg"))
        .await
        .unwrap();
    let b = assets
        .upsert_discovered(photo(folder, "b.jpg"))
        .await
        .unwrap();

    let empty: i32 =
        sqlx::query_scalar("SELECT coalesce(sum(asset_count), 0)::int FROM folder_month_counts")
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert_eq!(empty, 0, "discovered assets must not fill the scrollbar");

    let july = Utc.with_ymd_and_hms(2024, 7, 2, 10, 0, 0).unwrap();
    let august = Utc.with_ymd_and_hms(2024, 8, 1, 10, 0, 0).unwrap();
    assets.set_indexed(a.id, july, 100, 100).await.unwrap();
    assets.set_indexed(b.id, july, 100, 100).await.unwrap();

    let july_n: i32 = sqlx::query_scalar(
        "SELECT asset_count FROM folder_month_counts WHERE month = date '2024-07-01'",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(july_n, 2);

    assets.set_indexed(a.id, august, 100, 100).await.unwrap();
    let july_after: i32 = sqlx::query_scalar(
        "SELECT coalesce(asset_count, 0) FROM folder_month_counts WHERE month = date '2024-07-01'",
    )
    .fetch_optional(test.db().pool())
    .await
    .unwrap()
    .unwrap_or(0);
    let aug_n: i32 = sqlx::query_scalar(
        "SELECT asset_count FROM folder_month_counts WHERE month = date '2024-08-01'",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(july_after, 1);
    assert_eq!(aug_n, 1);

    assets.mark_offline(b.id).await.unwrap();
    sqlx::query("DELETE FROM assets WHERE id = $1")
        .bind(a.id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let summed: i64 =
        sqlx::query_scalar("SELECT coalesce(sum(asset_count), 0)::bigint FROM folder_month_counts")
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM assets WHERE status = 'indexed' AND taken_at_utc IS NOT NULL",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert_eq!(summed, live);
    assert_eq!(live, 0);
}
