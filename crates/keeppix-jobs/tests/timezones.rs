#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use chrono::{DateTime, TimeZone as _, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, DbError, FolderRepo, GeoRepo, LibraryRepo, OverrideRepo};
use keeppix_domain::{
    AssetId, AssetKind, AssetName, AuthContext, FolderId, GeoPoint, LibraryId, NewAsset,
    NewLibrary, OverridePatch, SystemRole, UserId,
};
use keeppix_jobs::geotag::{GeotagError, RecalculateTimezones};

async fn library(
    test: &TestDb,
    actor: UserId,
    owner: UserId,
    label: &str,
) -> (LibraryId, FolderId) {
    let library = LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(actor, SystemRole::Admin),
            NewLibrary {
                name: label.to_owned(),
                owner_id: owner,
                root_path: format!("/photos/{label}-{}", LibraryId::new()).into(),
                exclude_patterns: Vec::new(),
            },
        )
        .await
        .expect("library");
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &["timezone"])
        .await
        .expect("folder");
    (library.id, folder.id)
}

async fn seed_timezones(test: &TestDb) {
    let path = std::env::temp_dir().join(format!(
        "keeppix-jobs-timezones-{}.csv",
        uuid::Uuid::now_v7()
    ));
    tokio::fs::write(
        &path,
        "Asia/Tokyo\t{\"type\":\"MultiPolygon\",\"coordinates\":[[[[138,34],[141,34],[141,37],[138,37],[138,34]]]]}\n\
         Europe/Rome\t{\"type\":\"MultiPolygon\",\"coordinates\":[[[[6,35],[19,35],[19,48],[6,48],[6,35]]]]}\n",
    )
    .await
    .expect("timezone fixture");
    GeoRepo::new(test.db())
        .seed_timezones_from_csv_if_empty(&path)
        .await
        .expect("timezones");
    let _ = tokio::fs::remove_file(path).await;
}

async fn asset(
    test: &TestDb,
    folder: FolderId,
    filename: &str,
    naive_clock: DateTime<Utc>,
    point: Option<GeoPoint>,
) -> AssetId {
    let repo = AssetRepo::new(test.db());
    let id = repo
        .upsert_discovered(NewAsset {
            folder_id: folder,
            filename: AssetName::parse(filename).expect("filename"),
            size_bytes: 1,
            mtime: naive_clock,
            inode: None,
            kind: AssetKind::RawImage,
        })
        .await
        .expect("asset")
        .expect("new asset")
        .id;
    repo.set_indexed(id, naive_clock, 100, 100)
        .await
        .expect("indexed");
    if let Some(point) = point {
        repo.set_exif_location(id, point).await.expect("GPS");
    }
    id
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn preview_is_read_only_and_apply_is_undoable_idempotent_and_selective() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let (library_id, folder) = library(&test, admin, admin, "timezone-recalc").await;
    seed_timezones(&test).await;
    let naive = Utc.with_ymd_and_hms(2026, 8, 18, 14, 0, 0).unwrap();
    let corrected = Utc.with_ymd_and_hms(2026, 8, 18, 5, 0, 0).unwrap();
    let tokyo = asset(
        &test,
        folder,
        "DSC_4412.ARW",
        naive,
        Some(GeoPoint {
            lat: 35.6762,
            lon: 139.6503,
        }),
    )
    .await;
    let ocean = asset(
        &test,
        folder,
        "ocean.ARW",
        naive,
        Some(GeoPoint {
            lat: 0.0,
            lon: -140.0,
        }),
    )
    .await;
    let no_gps = asset(&test, folder, "no-gps.ARW", naive, None).await;
    let already_overridden = asset(
        &test,
        folder,
        "manual.ARW",
        naive,
        Some(GeoPoint {
            lat: 35.6762,
            lon: 139.6503,
        }),
    )
    .await;
    let manual_time = naive + chrono::Duration::hours(2);
    OverrideRepo::new(test.db())
        .apply(
            &ctx,
            already_overridden,
            &OverridePatch {
                taken_at: Some(Some(manual_time)),
                ..Default::default()
            },
        )
        .await
        .expect("manual override");

    let recalculate = RecalculateTimezones::new(test.db());
    let preview = recalculate
        .preview(&ctx, library_id)
        .await
        .expect("preview");

    assert_eq!(preview.count, 1);
    let example = preview.example.expect("preview example");
    assert_eq!(example.asset_id, tokyo);
    assert_eq!(example.filename, "DSC_4412.ARW");
    assert_eq!(example.before, naive);
    assert_eq!(example.after, corrected);
    assert_eq!(example.timezone, "Asia/Tokyo");
    for untouched in [tokyo, ocean, no_gps] {
        assert_eq!(
            OverrideRepo::new(test.db())
                .effective(&ctx, untouched)
                .await
                .unwrap()
                .taken_at,
            Some(naive),
            "preview must not write asset {untouched}"
        );
    }

    let (applied, _) = recalculate
        .apply(&ctx, library_id)
        .await
        .expect("first apply");
    assert_eq!(applied.changed_count, 1);
    let batch_id = applied.batch_id.expect("undoable batch");
    assert_eq!(
        OverrideRepo::new(test.db())
            .effective(&ctx, tokyo)
            .await
            .unwrap()
            .taken_at,
        Some(corrected)
    );
    assert_eq!(
        OverrideRepo::new(test.db())
            .effective(&ctx, ocean)
            .await
            .unwrap()
            .taken_at,
        Some(naive),
        "an ocean point with no timezone polygon stays unchanged"
    );
    assert_eq!(
        OverrideRepo::new(test.db())
            .effective(&ctx, no_gps)
            .await
            .unwrap()
            .taken_at,
        Some(naive),
        "an asset without GPS stays unchanged"
    );
    assert_eq!(
        OverrideRepo::new(test.db())
            .effective(&ctx, already_overridden)
            .await
            .unwrap()
            .taken_at,
        Some(manual_time),
        "an existing taken_at override belongs to the user"
    );

    let (second, _) = recalculate
        .apply(&ctx, library_id)
        .await
        .expect("idempotent second apply");
    assert_eq!(second.changed_count, 0);
    assert_eq!(second.batch_id, None);

    OverrideRepo::new(test.db())
        .undo_batch(&ctx, batch_id)
        .await
        .expect("undo timezone recalculation");
    assert_eq!(
        OverrideRepo::new(test.db())
            .effective(&ctx, tokyo)
            .await
            .unwrap()
            .taken_at,
        Some(naive)
    );
}

#[tokio::test]
async fn a_foreign_library_is_forbidden_for_preview_and_apply() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let stranger = harness::seed_user(&test, admin, "stranger").await;
    let (library_id, _) = library(&test, admin, admin, "private-timezones").await;
    let recalculate = RecalculateTimezones::new(test.db());
    let ctx = AuthContext::user(stranger, SystemRole::User);

    assert!(matches!(
        recalculate.preview(&ctx, library_id).await,
        Err(GeotagError::Db(DbError::Forbidden))
    ));
    assert!(matches!(
        recalculate.apply(&ctx, library_id).await,
        Err(GeotagError::Db(DbError::Forbidden))
    ));
}
