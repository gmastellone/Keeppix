#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{NaiveDate, TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, FolderRepo, LibraryRepo, StackRepo, TimelineRepo};
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
        .geometry(&ctx, None, None)
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
    let geometry = repo.geometry(&ctx, Some(library), None).await.unwrap();
    assert_eq!(
        geometry.records.len(),
        usize::try_from(bucket_total).unwrap()
    );

    let stamp = repo.geometry_stamp(&ctx, Some(library)).await.unwrap();
    assert_eq!(
        stamp.count,
        u64::try_from(geometry.records.len()).unwrap(),
        "stamp count must match geometry so If-None-Match can short-circuit"
    );
    assert_eq!(stamp.last_modified, geometry.last_modified);
}

/// Task 4-bis (Fase 10 §5bis, la contingenza mai portata avanti in Fase 11):
/// una vista a schermo freddo chiede solo i primi scatti, non l'intera
/// geometria, per non far aspettare un client su rete lenta. Verifica sia
/// che la paginazione funzioni sia che sia **equivalente** alla vista
/// intera, non solo "non vuota": concatenando le pagine si deve ottenere
/// esattamente lo stesso `Vec` di `geometry(..., None)`, stesso ordine.
#[tokio::test]
async fn geometry_pages_match_the_whole_view_concatenated() {
    let test = TestDb::start().await;
    let (admin, library, folder) = seed(&test).await;
    let assets = AssetRepo::new(test.db());
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    // 7 scatti, tutti in mesi diversi così l'ordine è inequivocabile anche
    // solo sul timestamp (nessun pareggio da risolvere sull'id).
    for month in 1_u32..=7 {
        let asset = assets
            .upsert_discovered(photo(folder, &format!("p{month}.jpg")))
            .await
            .unwrap()
            .unwrap();
        assets
            .set_indexed(
                asset.id,
                Utc.with_ymd_and_hms(2024, month, 1, 12, 0, 0).unwrap(),
                100 + i32::try_from(month).unwrap(),
                200,
            )
            .await
            .unwrap();
    }

    let repo = TimelineRepo::new(test.db());
    let whole = repo.geometry(&ctx, Some(library), None).await.unwrap();
    assert_eq!(whole.records.len(), 7);
    assert!(
        whole.next_cursor.is_none(),
        "la vista intera non pagina, non porta un cursore"
    );

    let mut paged = Vec::new();
    let mut after = None;
    let mut pages = 0;
    loop {
        pages += 1;
        assert!(pages <= 10, "la paginazione non converge, possibile ciclo");
        let page = repo
            .geometry(
                &ctx,
                Some(library),
                Some(keeppix_db::GeometryPage { limit: 3, after }),
            )
            .await
            .unwrap();
        assert!(
            page.records.len() <= 3,
            "una pagina non deve mai superare il limit richiesto"
        );
        paged.extend(page.records.iter().copied());
        match page.next_cursor {
            Some(cursor) => after = Some(cursor),
            None => break,
        }
    }
    assert_eq!(pages, 3, "7 record a limit=3 sono tre pagine: 3+3+1");
    assert_eq!(
        paged, whole.records,
        "le pagine concatenate devono combaciare byte per byte con la vista intera, stesso ordine"
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
        .geometry_in_bounds(&ctx, None, bounds, None)
        .await
        .unwrap();
    assert_eq!(
        geometry.records.len(),
        1,
        "l'asset unknown non è una foto da mostrare, come nella pagina (D3)"
    );
}

#[tokio::test]
async fn geometry_omits_unknown_kind_assets_without_a_bbox_filter() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let assets = AssetRepo::new(test.db());
    let taken = Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap();

    let visible = assets
        .upsert_discovered(photo(folder, "a.jpg"))
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(visible.id, taken, 10, 10).await.unwrap();

    let mut junk = photo(folder, "notes.jpg");
    junk.kind = AssetKind::Unknown;
    let hidden = assets.upsert_discovered(junk).await.unwrap().unwrap();
    assets.set_indexed(hidden.id, taken, 10, 10).await.unwrap();

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let geometry = TimelineRepo::new(test.db())
        .geometry(&ctx, None, None)
        .await
        .unwrap();
    assert_eq!(
        geometry.records.len(),
        1,
        "un asset unknown non è una foto da mostrare, come nella pagina (D3), \
         anche nel percorso senza bbox"
    );
}

/// Semina una pila RAW+JPEG (stesso basename) nella cartella data, la
/// raggruppa e restituisce (id del RAW/primario, id del JPEG/secondario).
async fn seed_stacked_pair(
    test: &TestDb,
    folder: keeppix_domain::FolderId,
    taken: chrono::DateTime<Utc>,
) -> (keeppix_domain::AssetId, keeppix_domain::AssetId) {
    let assets = AssetRepo::new(test.db());
    let raw = assets
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("DSC_0042.ARW").unwrap(),
            size_bytes: 1000,
            mtime: taken,
            inode: Some(101),
            kind: AssetKind::RawImage,
        })
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(raw.id, taken, 6000, 4000).await.unwrap();
    let jpeg = assets
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse("DSC_0042.JPG").unwrap(),
            size_bytes: 500,
            mtime: taken,
            inode: Some(102),
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(jpeg.id, taken, 6000, 4000)
        .await
        .unwrap();

    StackRepo::new(test.db())
        .regroup_folder(folder)
        .await
        .unwrap();

    (raw.id, jpeg.id)
}

#[tokio::test]
async fn page_collapses_a_raw_jpeg_stack_into_its_primary() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let taken = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    let (raw_id, _jpeg_id) = seed_stacked_pair(&test, folder, taken).await;

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let page = TimelineRepo::new(test.db())
        .page(&ctx, NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(), None, 50)
        .await
        .unwrap();

    assert_eq!(page.len(), 1, "raw+jpeg stack must collapse to one tile");
    assert_eq!(page[0].id, raw_id, "the raw is the primary");
    assert_eq!(page[0].stack.stack_size, 2);
    assert_eq!(page[0].stack.raw_kind.as_deref(), Some("raw+jpeg"));
}

#[tokio::test]
async fn page_reports_stack_size_one_for_an_unstacked_asset() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let assets = AssetRepo::new(test.db());
    let taken = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    let lone = assets
        .upsert_discovered(photo(folder, "lone.jpg"))
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(lone.id, taken, 100, 100).await.unwrap();

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let page = TimelineRepo::new(test.db())
        .page(&ctx, NaiveDate::from_ymd_opt(2024, 6, 1).unwrap(), None, 50)
        .await
        .unwrap();

    assert_eq!(page.len(), 1);
    assert_eq!(page[0].stack.stack_size, 1);
    assert_eq!(page[0].stack.raw_kind.as_deref(), Some("jpeg"));
}

#[tokio::test]
async fn geometry_collapses_a_raw_jpeg_stack_into_one_record() {
    let test = TestDb::start().await;
    let (admin, _library, folder) = seed(&test).await;
    let taken = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    seed_stacked_pair(&test, folder, taken).await;

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let geometry = TimelineRepo::new(test.db())
        .geometry(&ctx, None, None)
        .await
        .unwrap();
    assert_eq!(
        geometry.records.len(),
        1,
        "geometry must also collapse the stack to its primary"
    );
}

#[tokio::test]
async fn buckets_count_stacks_not_files() {
    let test = TestDb::start().await;
    let (admin, library, folder) = seed(&test).await;
    let taken = Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap();
    seed_stacked_pair(&test, folder, taken).await;

    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let buckets = TimelineRepo::new(test.db())
        .buckets(&ctx, Some(library))
        .await
        .unwrap();
    assert_eq!(buckets.len(), 1);
    assert_eq!(
        buckets[0].count, 1,
        "a raw+jpeg stack must count as one tile, not two files"
    );
}

#[tokio::test]
async fn probing_someone_elses_library_geometry_is_forbidden() {
    let test = TestDb::start().await;
    let (admin, library, _) = seed(&test).await;
    let user = harness::seed_user(&test, admin, "luca").await;
    let err = TimelineRepo::new(test.db())
        .geometry(
            &AuthContext::user(user, SystemRole::User),
            Some(library),
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::Forbidden));
}
