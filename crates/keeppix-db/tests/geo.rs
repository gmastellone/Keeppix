#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{TimeZone as _, Utc};
use harness::TestDb;
use keeppix_db::{
    AlbumRepo, DbError, FolderRepo, GeoRepo, LibraryRepo, MapBounds, MapScope, NewAlbum, NewGrant,
    ObjectType, PermissionRepo, SearchRepo, SubjectType,
};
use keeppix_domain::{
    AlbumId, AssetId, AuthContext, FolderId, GeoPoint, LibraryId, NewLibrary, ObjectRole,
    SystemRole, UserId,
};

async fn library(test: &TestDb, owner: UserId, name: &str) -> (LibraryId, FolderId) {
    let id = LibraryRepo::new(test.db())
        .create(
            &AuthContext::user(owner, SystemRole::Admin),
            NewLibrary {
                name: name.to_owned(),
                owner_id: owner,
                root_path: format!("/photos/{name}").into(),
                exclude_patterns: Vec::new(),
            },
        )
        .await
        .expect("library")
        .id;
    let folder = FolderRepo::new(test.db())
        .ensure_path(id, &["map"])
        .await
        .expect("folder")
        .id;
    (id, folder)
}

async fn point(
    test: &TestDb,
    folder: FolderId,
    filename: &str,
    lon: f64,
    lat: f64,
    taken_hour: u32,
) -> AssetId {
    let id = AssetId::new();
    sqlx::query(
        "INSERT INTO assets \
             (id, folder_id, filename, size_bytes, mtime, kind, status, taken_at_utc, location) \
         VALUES ($1, $2, $3, 1, $4, 'image', 'indexed', $5, \
                 ST_SetSRID(ST_MakePoint($6, $7), 4326)::geography)",
    )
    .bind(id.as_uuid())
    .bind(folder.as_uuid())
    .bind(filename)
    .bind(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap())
    .bind(Utc.with_ymd_and_hms(2026, 1, 1, taken_hour, 0, 0).unwrap())
    .bind(lon)
    .bind(lat)
    .execute(test.db().pool())
    .await
    .expect("point asset");
    id
}

fn world() -> MapBounds {
    MapBounds {
        west: -180.0,
        south: -90.0,
        east: 180.0,
        north: 90.0,
    }
}

#[tokio::test]
async fn map_clusters_reuse_folder_visibility_and_viewer_rating_for_cover() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let viewer = harness::seed_user(&test, admin, "viewer").await;
    let (library_id, visible_folder) = library(&test, admin, "visible").await;
    let hidden_folder = FolderRepo::new(test.db())
        .ensure_path(library_id, &["hidden"])
        .await
        .unwrap()
        .id;
    let lower_rated = point(&test, visible_folder, "low.jpg", 12.01, 41.01, 20).await;
    let higher_rated = point(&test, visible_folder, "high.jpg", 12.02, 41.02, 10).await;
    point(&test, hidden_folder, "hidden.jpg", 12.03, 41.03, 23).await;

    PermissionRepo::new(test.db())
        .grant(
            &AuthContext::user(admin, SystemRole::Admin),
            NewGrant {
                subject: SubjectType::User,
                subject_id: viewer.as_uuid(),
                object: ObjectType::Folder,
                object_id: visible_folder.as_uuid(),
                role: ObjectRole::Viewer,
                inherit: true,
            },
        )
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO asset_flags (asset_id, user_id, rating, pick) \
         VALUES ($1, $3, 1, 'none'), ($2, $3, 5, 'none')",
    )
    .bind(lower_rated.as_uuid())
    .bind(higher_rated.as_uuid())
    .bind(viewer.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let rows = GeoRepo::new(test.db())
        .clusters(
            &AuthContext::user(viewer, SystemRole::User),
            world(),
            10,
            MapScope::Library(library_id),
        )
        .await
        .unwrap();

    assert_eq!(rows.len(), 1, "hidden folder must not produce a cluster");
    assert_eq!(rows[0].count, 2);
    assert_eq!(rows[0].cover_asset_id, higher_rated);
    assert!(rows[0].clustered);
    assert!((rows[0].lon - 12.041_015_625).abs() < 1e-9);
    assert!((rows[0].lat - 41.044_921_875).abs() < 1e-9);
}

#[tokio::test]
async fn fiji_bbox_splits_at_the_antimeridian_and_uses_override_location() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let (library_id, folder) = library(&test, admin, "fiji").await;
    let east = point(&test, folder, "east.jpg", 179.2, -17.0, 1).await;
    let overridden = point(&test, folder, "override.jpg", 0.0, -10.0, 2).await;
    let atlantic = point(&test, folder, "atlantic.jpg", 0.0, -10.0, 3).await;
    sqlx::query(
        "INSERT INTO asset_overrides (asset_id, location, updated_by) \
         VALUES ($1, ST_SetSRID(ST_MakePoint(-179.4, -16.5), 4326)::geography, $2)",
    )
    .bind(overridden.as_uuid())
    .bind(admin.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let rows = GeoRepo::new(test.db())
        .clusters(
            &AuthContext::user(admin, SystemRole::Admin),
            MapBounds {
                west: 170.0,
                south: -20.0,
                east: -170.0,
                north: 0.0,
            },
            15,
            MapScope::Library(library_id),
        )
        .await
        .unwrap();
    let ids: Vec<_> = rows.iter().map(|row| row.cover_asset_id).collect();

    assert_eq!(rows.len(), 2);
    assert!(ids.contains(&east));
    assert!(ids.contains(&overridden));
    assert!(!ids.contains(&atlantic));
    assert!(rows.iter().all(|row| !row.clustered && row.count == 1));
}

#[tokio::test]
async fn zoom_fifteen_is_unclustered_until_the_five_hundred_point_cap() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let (library_id, folder) = library(&test, admin, "cap").await;
    point(&test, folder, "one.jpg", 7.0, 45.0, 1).await;
    point(&test, folder, "two.jpg", 7.0, 45.0, 2).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = GeoRepo::new(test.db());

    let individual = repo
        .clusters(&ctx, world(), 15, MapScope::Library(library_id))
        .await
        .unwrap();
    assert_eq!(individual.len(), 2);
    assert!(individual.iter().all(|row| !row.clustered));

    sqlx::query(
        "INSERT INTO assets \
             (id, folder_id, filename, size_bytes, mtime, kind, status, taken_at_utc, location) \
         SELECT md5($1::text || i::text)::uuid, $1, 'bulk-' || i || '.jpg', 1, now(), \
                'image', 'indexed', now(), \
                ST_SetSRID(ST_MakePoint(7.0, 45.0), 4326)::geography \
           FROM generate_series(1, 499) AS i",
    )
    .bind(folder.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let clustered = repo
        .clusters(&ctx, world(), 15, MapScope::Library(library_id))
        .await
        .unwrap();
    assert_eq!(clustered.iter().map(|row| row.count).sum::<i64>(), 501);
    assert!(clustered.len() < 500);
    assert!(clustered.iter().all(|row| row.clustered));
}

#[tokio::test]
async fn library_album_folder_and_search_scopes_are_applied() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let (_library_id, folder) = library(&test, admin, "scopes").await;
    let matching = point(&test, folder, "holiday-map.jpg", 9.0, 44.0, 1).await;
    point(&test, folder, "work-map.jpg", 10.0, 45.0, 2).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let album = AlbumRepo::new(test.db())
        .create(
            &ctx,
            NewAlbum {
                name: "Map".to_owned(),
                description: String::new(),
            },
        )
        .await
        .unwrap();
    AlbumRepo::new(test.db())
        .add_asset(&ctx, album.id, matching)
        .await
        .unwrap();
    let saved = SearchRepo::new(test.db())
        .create_saved(&ctx, "Holiday", "holiday")
        .await
        .unwrap();
    let repo = GeoRepo::new(test.db());

    for scope in [
        MapScope::Album(album.id),
        MapScope::Folder(folder),
        MapScope::Search(saved.id),
    ] {
        let rows = repo.clusters(&ctx, world(), 15, scope).await.unwrap();
        if matches!(scope, MapScope::Folder(_)) {
            assert_eq!(rows.len(), 2);
        } else {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].cover_asset_id, matching);
        }
    }
}

#[tokio::test]
async fn foreign_scope_ids_are_forbidden_for_every_scope_kind() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let stranger = harness::seed_user(&test, admin, "stranger").await;
    let (library_id, folder) = library(&test, admin, "foreign").await;
    let ctx_admin = AuthContext::user(admin, SystemRole::Admin);
    let album = AlbumRepo::new(test.db())
        .create(
            &ctx_admin,
            NewAlbum {
                name: "Private".to_owned(),
                description: String::new(),
            },
        )
        .await
        .unwrap();
    let search = SearchRepo::new(test.db())
        .create_saved(&ctx_admin, "Private", "secret")
        .await
        .unwrap();
    let ctx = AuthContext::user(stranger, SystemRole::User);
    let repo = GeoRepo::new(test.db());

    for scope in [
        MapScope::Library(library_id),
        MapScope::Album(AlbumId::from_uuid(album.id.as_uuid())),
        MapScope::Folder(folder),
        MapScope::Search(search.id),
    ] {
        assert!(matches!(
            repo.clusters(&ctx, world(), 10, scope).await,
            Err(DbError::Forbidden)
        ));
    }
}

#[tokio::test]
async fn four_thousand_points_cluster_within_the_local_budget() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let (library_id, folder) = library(&test, admin, "performance").await;
    sqlx::query(
        "INSERT INTO assets \
             (id, folder_id, filename, size_bytes, mtime, kind, status, taken_at_utc, location) \
         SELECT md5($1::text || i::text)::uuid, $1, 'photo-' || i || '.jpg', 1, now(), \
                'image', 'indexed', now() - (i || ' seconds')::interval, \
                ST_SetSRID(ST_MakePoint(8.0 + (i % 100) * 0.001, \
                                       44.0 + (i / 100) * 0.001), 4326)::geography \
           FROM generate_series(1, 4000) AS i",
    )
    .bind(folder.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let started = Instant::now();
    let rows = GeoRepo::new(test.db())
        .clusters(
            &AuthContext::user(admin, SystemRole::Admin),
            MapBounds {
                west: 7.0,
                south: 43.0,
                east: 10.0,
                north: 46.0,
            },
            10,
            MapScope::Library(library_id),
        )
        .await
        .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(rows.iter().map(|row| row.count).sum::<i64>(), 4000);
    assert!(
        elapsed < Duration::from_secs(1),
        "cluster query took {elapsed:?}"
    );
}

async fn seed_timezone(test: &TestDb, name: &str, west: f64, south: f64, east: f64, north: f64) {
    sqlx::query(
        "INSERT INTO tz_boundaries (tz_name, boundary) \
         VALUES ($1, ST_Multi(ST_MakeEnvelope($2, $3, $4, $5, 4326))::geography)",
    )
    .bind(name)
    .bind(west)
    .bind(south)
    .bind(east)
    .bind(north)
    .execute(test.db().pool())
    .await
    .expect("timezone boundary");
}

#[tokio::test]
async fn timezone_lookup_finds_tokyo_and_leaves_the_open_ocean_unmatched() {
    let test = TestDb::start().await;
    seed_timezone(&test, "Asia/Tokyo", 138.0, 34.0, 141.0, 37.0).await;

    let repo = GeoRepo::new(test.db());
    assert_eq!(
        repo.timezone_for(GeoPoint {
            lat: 35.6762,
            lon: 139.6503,
        })
        .await
        .expect("Tokyo lookup")
        .as_deref(),
        Some("Asia/Tokyo")
    );
    assert_eq!(
        repo.timezone_for(GeoPoint {
            lat: 0.0,
            lon: -140.0,
        })
        .await
        .expect("ocean lookup"),
        None
    );
}

#[tokio::test]
async fn timezone_lookup_on_a_shared_boundary_never_errors_or_double_matches() {
    let test = TestDb::start().await;
    seed_timezone(&test, "Europe/West", 8.0, 40.0, 10.0, 50.0).await;
    seed_timezone(&test, "Europe/East", 10.0, 40.0, 12.0, 50.0).await;

    let result = GeoRepo::new(test.db())
        .timezone_for(GeoPoint {
            lat: 45.0,
            lon: 10.0,
        })
        .await;

    let timezone = result.expect("a boundary point must not make the lookup fail");
    assert!(
        matches!(
            timezone.as_deref(),
            None | Some("Europe/West" | "Europe/East")
        ),
        "a boundary point must produce zero or one timezone: {timezone:?}"
    );
}

fn temporary_timezone_csv() -> PathBuf {
    std::env::temp_dir().join(format!("keeppix-timezones-{}.csv", uuid::Uuid::now_v7()))
}

#[tokio::test]
async fn normalized_timezone_csv_seeds_an_empty_catalog_only_once() {
    let test = TestDb::start().await;
    let path = temporary_timezone_csv();
    tokio::fs::write(
        &path,
        "Asia/Tokyo\t{\"type\":\"MultiPolygon\",\"coordinates\":[[[[138,34],[141,34],[141,37],[138,37],[138,34]]]]}\n\
         Europe/Rome\t{\"type\":\"MultiPolygon\",\"coordinates\":[[[[6,35],[19,35],[19,48],[6,48],[6,35]]]]}\n",
    )
    .await
    .expect("timezone fixture");
    let repo = GeoRepo::new(test.db());

    assert_eq!(
        repo.seed_timezones_from_csv_if_empty(&path)
            .await
            .expect("first seed"),
        2
    );
    assert_eq!(
        repo.timezone_for(GeoPoint {
            lat: 35.6762,
            lon: 139.6503,
        })
        .await
        .unwrap()
        .as_deref(),
        Some("Asia/Tokyo")
    );

    tokio::fs::write(&path, "corrupt replacement\n")
        .await
        .expect("replace fixture");
    assert_eq!(
        repo.seed_timezones_from_csv_if_empty(&path)
            .await
            .expect("second seed"),
        0,
        "an already populated catalog is not replaced at boot"
    );
    let _ = tokio::fs::remove_file(path).await;
}

#[tokio::test]
async fn missing_timezone_csv_is_a_noop_but_a_corrupt_present_file_fails_atomically() {
    let test = TestDb::start().await;
    let path = temporary_timezone_csv();
    let repo = GeoRepo::new(test.db());

    assert_eq!(
        repo.seed_timezones_from_csv_if_empty(&path)
            .await
            .expect("missing file is allowed"),
        0
    );

    tokio::fs::write(
        &path,
        "Asia/Tokyo\t{\"type\":\"MultiPolygon\",\"coordinates\":[[[[138,34],[141,34],[141,37],[138,37],[138,34]]]]}\n\
         broken-row\n",
    )
    .await
    .expect("corrupt fixture");
    assert!(matches!(
        repo.seed_timezones_from_csv_if_empty(&path).await,
        Err(DbError::Corrupted(_))
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tz_boundaries")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(count, 0, "a corrupt import must roll back every row");
    let _ = tokio::fs::remove_file(path).await;
}

#[tokio::test]
async fn timezone_seed_rejects_an_unknown_iana_name_atomically() {
    let test = TestDb::start().await;
    let path = temporary_timezone_csv();
    let repo = GeoRepo::new(test.db());

    tokio::fs::write(
        &path,
        "Mars/Olympus_Mons\t{\"type\":\"MultiPolygon\",\"coordinates\":[[[[1,1],[2,1],[2,2],[1,2],[1,1]]]]}\n",
    )
    .await
    .expect("invalid timezone fixture");

    assert!(matches!(
        repo.seed_timezones_from_csv_if_empty(&path).await,
        Err(DbError::Corrupted(_))
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tz_boundaries")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(count, 0, "an unknown IANA timezone must roll back the seed");
    let _ = tokio::fs::remove_file(path).await;
}
