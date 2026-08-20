#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{NaiveDate, TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, FolderRepo, LibraryRepo, TimelineRepo};
use keeppix_domain::{AssetKind, AssetName, AuthContext, NewAsset, NewLibrary, SystemRole, UserId};

async fn seed(test: &TestDb) -> (UserId, keeppix_domain::LibraryId, keeppix_domain::FolderId) {
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
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    (admin, library.id, folder.id)
}

fn photo(folder: keeppix_domain::FolderId, name: &str) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(name).unwrap(),
        size_bytes: 10,
        mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

#[tokio::test]
async fn buckets_sum_indexed_photos_by_month() {
    let test = TestDb::start().await;
    let (admin, library, folder) = seed(&test).await;
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, "a.jpg"))
        .await
        .unwrap()
        .unwrap();
    let b = assets
        .upsert_discovered(photo(folder, "b.jpg"))
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(
            a.id,
            Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();
    assets
        .set_indexed(
            b.id,
            Utc.with_ymd_and_hms(2024, 8, 3, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let buckets = TimelineRepo::new(test.db())
        .buckets(&ctx, Some(library))
        .await
        .unwrap();
    assert_eq!(buckets.len(), 2);
    assert_eq!(
        buckets[0].month,
        NaiveDate::from_ymd_opt(2024, 8, 1).unwrap()
    );
    assert_eq!(buckets[0].count, 1);
    assert_eq!(
        buckets[1].month,
        NaiveDate::from_ymd_opt(2024, 7, 1).unwrap()
    );
    assert_eq!(buckets[1].count, 1);
}

#[tokio::test]
async fn page_uses_keyset_not_offset() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let assets = AssetRepo::new(test.db());
    let mut ids = Vec::new();
    for (i, name) in ["c.jpg", "d.jpg", "e.jpg"].iter().enumerate() {
        let a = assets
            .upsert_discovered(photo(folder, name))
            .await
            .unwrap()
            .unwrap();
        let t = Utc
            .with_ymd_and_hms(2024, 7, 10 - u32::try_from(i).unwrap(), 12, 0, 0)
            .unwrap();
        assets.set_indexed(a.id, t, 1, 1).await.unwrap();
        ids.push(a.id);
    }
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = TimelineRepo::new(test.db());
    let bucket = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
    let first = repo.page(&ctx, bucket, None, 2).await.unwrap();
    assert_eq!(first.len(), 2);
    let cursor = (first[1].taken_at_utc.unwrap(), first[1].id);
    let second = repo.page(&ctx, bucket, Some(cursor), 2).await.unwrap();
    assert_eq!(second.len(), 1);
    assert_ne!(second[0].id, first[0].id);
    assert_ne!(second[0].id, first[1].id);
}

#[tokio::test]
async fn timeline_page_omits_unknown_assets() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let assets = AssetRepo::new(test.db());
    let taken = Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap();

    let visible = assets
        .upsert_discovered(photo(folder, "a.jpg"))
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(visible.id, taken, 1, 1).await.unwrap();

    let mut junk = photo(folder, "notes.jpg");
    junk.kind = AssetKind::Unknown;
    let hidden = assets.upsert_discovered(junk).await.unwrap().unwrap();
    assets.set_indexed(hidden.id, taken, 1, 1).await.unwrap();

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let page = TimelineRepo::new(test.db())
        .page(&ctx, NaiveDate::from_ymd_opt(2024, 7, 1).unwrap(), None, 50)
        .await
        .unwrap();
    let ids: Vec<_> = page.iter().map(|a| a.id).collect();
    assert!(ids.contains(&visible.id));
    assert!(
        !ids.contains(&hidden.id),
        "un asset unknown non è una foto da mostrare (D3)"
    );
}

#[tokio::test]
async fn probing_someone_elses_library_is_forbidden() {
    let test = TestDb::start().await;
    let (admin, library, _) = seed(&test).await;
    let user = harness::seed_user(&test, admin, "luca").await;
    let err = TimelineRepo::new(test.db())
        .buckets(&AuthContext::user(user, SystemRole::User), Some(library))
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::Forbidden));
}

#[tokio::test]
async fn geometry_orders_records_like_the_timeline_and_encodes_nulls_as_none() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let assets = AssetRepo::new(test.db());
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let newest = assets
        .upsert_discovered(photo(folder, "newest.jpg"))
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(
            newest.id,
            Utc.with_ymd_and_hms(2024, 8, 10, 12, 0, 0).unwrap(),
            6000,
            4000,
        )
        .await
        .unwrap();

    // Un asset indicizzato ma senza width/height nota (fase di sizing non
    // ancora passata): deve comparire con None, non essere escluso.
    let unsized_asset = assets
        .upsert_discovered(photo(folder, "unsized.jpg"))
        .await
        .unwrap()
        .unwrap();
    sqlx::query(
        "UPDATE assets SET status = 'indexed', taken_at_utc = $2, width = NULL, height = NULL \
         WHERE id = $1",
    )
    .bind(unsized_asset.id.as_uuid())
    .bind(Utc.with_ymd_and_hms(2024, 8, 5, 12, 0, 0).unwrap())
    .execute(test.db().pool())
    .await
    .unwrap();

    let geometry = TimelineRepo::new(test.db())
        .geometry(&ctx, None)
        .await
        .unwrap();
    assert_eq!(geometry.records.len(), 2);
    assert_eq!(geometry.records[0].width, Some(6000));
    assert_eq!(geometry.records[0].height, Some(4000));
    assert_eq!(geometry.records[1].width, None);
    assert_eq!(geometry.records[1].height, None);
    assert!(geometry.last_modified.is_some());
}

#[tokio::test]
async fn geometry_matches_bucket_counts() {
    let test = TestDb::start().await;
    let (admin, library, folder) = seed(&test).await;
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, "a.jpg"))
        .await
        .unwrap()
        .unwrap();
    let b = assets
        .upsert_discovered(photo(folder, "b.jpg"))
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(
            a.id,
            Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();
    assets
        .set_indexed(
            b.id,
            Utc.with_ymd_and_hms(2024, 8, 3, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = TimelineRepo::new(test.db());
    let buckets = repo.buckets(&ctx, Some(library)).await.unwrap();
    let bucket_total: i64 = buckets.iter().map(|b| b.count).sum();
    let geometry = repo.geometry(&ctx, Some(library)).await.unwrap();
    assert_eq!(
        geometry.records.len(),
        usize::try_from(bucket_total).unwrap()
    );
}

#[tokio::test]
async fn geometry_omits_unknown_kind_assets_when_filtering_by_bbox() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let assets = AssetRepo::new(test.db());
    let taken = Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap();

    let visible = assets
        .upsert_discovered(photo(folder, "rome.jpg"))
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(visible.id, taken, 10, 10).await.unwrap();
    assets
        .set_exif_location(
            visible.id,
            keeppix_domain::GeoPoint {
                lat: 41.9028,
                lon: 12.4964,
            },
        )
        .await
        .unwrap();

    let mut junk = photo(folder, "notes.jpg");
    junk.kind = AssetKind::Unknown;
    let hidden = assets.upsert_discovered(junk).await.unwrap().unwrap();
    assets.set_indexed(hidden.id, taken, 10, 10).await.unwrap();
    assets
        .set_exif_location(
            hidden.id,
            keeppix_domain::GeoPoint {
                lat: 41.9028,
                lon: 12.4964,
            },
        )
        .await
        .unwrap();

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let bounds = keeppix_db::MapBounds {
        west: 10.0,
        south: 40.0,
        east: 13.0,
        north: 43.0,
    };
    let geometry = TimelineRepo::new(test.db())
        .geometry_in_bounds(&ctx, None, bounds)
        .await
        .unwrap();
    assert_eq!(
        geometry.records.len(),
        1,
        "l'asset unknown non è una foto da mostrare, come nella pagina (D3)"
    );
}

#[tokio::test]
async fn probing_someone_elses_library_geometry_is_forbidden() {
    let test = TestDb::start().await;
    let (admin, library, _) = seed(&test).await;
    let user = harness::seed_user(&test, admin, "luca").await;
    let err = TimelineRepo::new(test.db())
        .geometry(&AuthContext::user(user, SystemRole::User), Some(library))
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::Forbidden));
}
