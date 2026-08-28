#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{DateTime, TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AssetRepo, FolderRepo, LibraryRepo, PlaceRepo, SearchNode, SearchRepo, StackRepo,
    SuggestionKind,
};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, ExifData, GeoPoint, NewAsset, NewLibrary, Place, Rating,
    SystemRole,
};

async fn seed(test: &TestDb) -> (AuthContext, keeppix_domain::FolderId) {
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
    (ctx, folder.id)
}

fn photo(folder: keeppix_domain::FolderId, name: &str, kind: AssetKind) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(name).unwrap(),
        size_bytes: 10,
        mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
        inode: Some(1),
        kind,
    }
}

async fn index(
    test: &TestDb,
    folder: keeppix_domain::FolderId,
    name: &str,
    kind: AssetKind,
    day: u32,
) -> keeppix_domain::AssetId {
    index_at(
        test,
        folder,
        name,
        kind,
        Utc.with_ymd_and_hms(2024, 7, day, 12, 0, 0).unwrap(),
    )
    .await
}

/// Like [`index`], but with an arbitrary `taken_at_utc` — used by the
/// `Day`/`Month`/`DateRange` tests, where `index`'s fixed month isn't
/// enough.
async fn index_at(
    test: &TestDb,
    folder: keeppix_domain::FolderId,
    name: &str,
    kind: AssetKind,
    taken_at: DateTime<Utc>,
) -> keeppix_domain::AssetId {
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, name, kind))
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(a.id, taken_at, 1, 1).await.unwrap();
    a.id
}

fn blank_exif(taken_at: DateTime<Utc>) -> ExifData {
    ExifData {
        raw: serde_json::json!({}),
        taken_at_utc: taken_at,
        tz_offset_minutes: 0,
        tz_assumed: true,
        width: None,
        height: None,
        camera_make: None,
        camera_model: None,
        lens: None,
        iso: None,
        f_number: None,
        exposure: None,
        focal_length: None,
        gps: None,
    }
}

async fn set_place(test: &TestDb, asset_id: keeppix_domain::AssetId, place_id: i64) {
    sqlx::query("UPDATE assets SET place_id = $1 WHERE id = $2")
        .bind(place_id)
        .bind(asset_id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();
}

async fn set_pick(
    test: &TestDb,
    asset_id: keeppix_domain::AssetId,
    user: keeppix_domain::UserId,
    pick: &str,
) {
    sqlx::query(
        "INSERT INTO asset_flags (asset_id, user_id, pick, updated_at) \
         VALUES ($1, $2, $3, now()) \
         ON CONFLICT (asset_id, user_id) DO UPDATE SET pick = EXCLUDED.pick",
    )
    .bind(asset_id.as_uuid())
    .bind(user.as_uuid())
    .bind(pick)
    .execute(test.db().pool())
    .await
    .unwrap();
}

async fn set_favorite(
    test: &TestDb,
    asset_id: keeppix_domain::AssetId,
    user: keeppix_domain::UserId,
    favorite: bool,
) {
    sqlx::query(
        "INSERT INTO asset_flags (asset_id, user_id, favorite, updated_at) \
         VALUES ($1, $2, $3, now()) \
         ON CONFLICT (asset_id, user_id) DO UPDATE SET favorite = EXCLUDED.favorite",
    )
    .bind(asset_id.as_uuid())
    .bind(user.as_uuid())
    .bind(favorite)
    .execute(test.db().pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn type_filter_does_not_interpolate_the_user_string() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    index(&test, folder, "a.jpg", AssetKind::Image, 2).await;
    index(&test, folder, "b.mp4", AssetKind::Video, 3).await;

    let found = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Type {
                value: "image".to_owned(),
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].filename.as_str(), "a.jpg");

    let injected = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Text {
                value: "a.jpg'; drop table assets; --".to_owned(),
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert!(injected.is_empty());
    let still_there: i64 = sqlx::query_scalar("SELECT count(*) FROM assets")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(still_there, 2);
}

#[tokio::test]
async fn camera_and_iso_and_year_and_has_gps() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let sony = index(&test, folder, "sony.jpg", AssetKind::Image, 4).await;
    let other = index(&test, folder, "other.jpg", AssetKind::Image, 5).await;
    let assets = AssetRepo::new(test.db());
    assets
        .insert_exif(
            sony,
            &ExifData {
                raw: serde_json::json!({}),
                taken_at_utc: Utc.with_ymd_and_hms(2024, 7, 4, 12, 0, 0).unwrap(),
                tz_offset_minutes: 0,
                tz_assumed: true,
                width: None,
                height: None,
                camera_make: Some("Sony".to_owned()),
                camera_model: Some("α7 IV".to_owned()),
                lens: None,
                iso: Some(6400),
                f_number: None,
                exposure: None,
                focal_length: None,
                gps: None,
            },
        )
        .await
        .unwrap();
    assets
        .insert_exif(
            other,
            &ExifData {
                raw: serde_json::json!({}),
                taken_at_utc: Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap(),
                tz_offset_minutes: 0,
                tz_assumed: true,
                width: None,
                height: None,
                camera_make: None,
                camera_model: Some("Canon".to_owned()),
                lens: None,
                iso: Some(100),
                f_number: None,
                exposure: None,
                focal_length: None,
                gps: None,
            },
        )
        .await
        .unwrap();

    assets
        .set_indexed(
            other,
            Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();

    let ast = SearchNode::And {
        args: vec![
            SearchNode::Camera {
                value: "α7".to_owned(),
            },
            SearchNode::Iso {
                cmp: keeppix_db::IsoCmp::Gt,
                value: 3200,
            },
            SearchNode::Year { value: 2024 },
        ],
    };
    let found = SearchRepo::new(test.db())
        .run(&ctx, &ast, None, 50)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, sony);
}

#[tokio::test]
async fn not_camera_includes_assets_without_exif() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let sony = index(&test, folder, "sony.jpg", AssetKind::Image, 4).await;
    let bare = index(&test, folder, "bare.jpg", AssetKind::Image, 6).await;
    AssetRepo::new(test.db())
        .insert_exif(
            sony,
            &ExifData {
                raw: serde_json::json!({}),
                taken_at_utc: Utc.with_ymd_and_hms(2024, 7, 4, 12, 0, 0).unwrap(),
                tz_offset_minutes: 0,
                tz_assumed: true,
                width: None,
                height: None,
                camera_make: Some("Sony".to_owned()),
                camera_model: Some("α7 IV".to_owned()),
                lens: None,
                iso: Some(6400),
                f_number: None,
                exposure: None,
                focal_length: None,
                gps: None,
            },
        )
        .await
        .unwrap();

    let found = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Not {
                arg: Box::new(SearchNode::Camera {
                    value: "α7".to_owned(),
                }),
            },
            None,
            50,
        )
        .await
        .unwrap();
    let ids: Vec<_> = found.iter().map(|a| a.id).collect();
    assert!(ids.contains(&bare), "no EXIF is not a Sony");
    assert!(!ids.contains(&sony));
}

#[tokio::test]
async fn search_collapses_a_raw_jpeg_stack_into_its_primary() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let taken = Utc.with_ymd_and_hms(2024, 7, 4, 12, 0, 0).unwrap();
    let assets = AssetRepo::new(test.db());
    let raw = assets
        .upsert_discovered(photo(folder, "DSC_0100.ARW", AssetKind::RawImage))
        .await
        .unwrap()
        .unwrap();
    assets.set_indexed(raw.id, taken, 6000, 4000).await.unwrap();
    let jpeg = assets
        .upsert_discovered(photo(folder, "DSC_0100.JPG", AssetKind::Image))
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

    let found = SearchRepo::new(test.db())
        .run(&ctx, &SearchNode::And { args: vec![] }, None, 50)
        .await
        .unwrap();
    assert_eq!(found.len(), 1, "the stack must collapse to its primary");
    assert_eq!(found[0].id, raw.id);
    assert_eq!(found[0].stack.stack_size, 2);
    assert_eq!(found[0].stack.raw_kind.as_deref(), Some("raw+jpeg"));
}

#[tokio::test]
async fn saved_search_quotes_keep_a_field_shaped_token_as_text() {
    let test = TestDb::start().await;
    let (ctx, _) = seed(&test).await;
    let repo = SearchRepo::new(test.db());
    let quoted = repo
        .create_saved(&ctx, "Quoted", r#""camera:Sony""#)
        .await
        .unwrap();
    let unquoted = repo
        .create_saved(&ctx, "Unquoted", "camera:Sony")
        .await
        .unwrap();

    assert_eq!(
        repo.saved_query(&ctx, quoted.id).await.unwrap(),
        SearchNode::Text {
            value: "camera:Sony".to_owned()
        }
    );
    assert_eq!(
        repo.saved_query(&ctx, unquoted.id).await.unwrap(),
        SearchNode::Camera {
            value: "Sony".to_owned()
        }
    );
}

#[tokio::test]
async fn rating_filter_is_per_user_not_per_asset() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let admin = ctx.user_id().unwrap();
    // A second user rates the same file 5 stars: if the filter weren't
    // per-user, their rating would surface the asset for admin too, even
    // though admin never rated that file.
    let other_user = harness::seed_user(&test, admin, "marta").await;
    let five_stars = index(&test, folder, "five-stars.jpg", AssetKind::Image, 2).await;
    let unrated = index(&test, folder, "unrated.jpg", AssetKind::Image, 3).await;

    keeppix_db::FlagRepo::new(test.db())
        .set(
            &ctx,
            five_stars,
            &keeppix_domain::AssetFlags {
                rating: Some(Rating::parse(5).unwrap()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO asset_flags (asset_id, user_id, rating, updated_at) \
         VALUES ($1, $2, 5, now())",
    )
    .bind(unrated.as_uuid())
    .bind(other_user.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let found = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Rating {
                cmp: keeppix_db::IsoCmp::Gte,
                value: 4,
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(
        found.len(),
        1,
        "marta's rating on unrated.jpg must not appear in admin's search"
    );
    assert_eq!(found[0].id, five_stars);
}

#[tokio::test]
async fn favorite_filter_is_per_user_not_per_asset() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let admin = ctx.user_id().unwrap();
    // A second user, never granted visibility on this library: enough to
    // prove the filter checks `user_id`, not just `favorite`, without also
    // having to set up permissions for a second actor who can see photos.
    let other_user = harness::seed_user(&test, admin, "marta").await;
    let loved = index(&test, folder, "loved.jpg", AssetKind::Image, 2).await;
    let plain = index(&test, folder, "plain.jpg", AssetKind::Image, 3).await;

    set_favorite(&test, loved, admin, true).await;
    set_favorite(&test, plain, admin, false).await;
    set_favorite(&test, plain, other_user, true).await;

    let found = SearchRepo::new(test.db())
        .run(&ctx, &SearchNode::Favorite, None, 50)
        .await
        .unwrap();
    assert_eq!(
        found.len(),
        1,
        "marta's favorite on plain.jpg must not appear in admin's search"
    );
    assert_eq!(found[0].id, loved);
}

#[tokio::test]
async fn pick_filter_matches_only_the_current_users_explicit_value() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let admin = ctx.user_id().unwrap();
    // Like rating/favorite: another user's pick must not appear in this
    // user's search.
    let other_user = harness::seed_user(&test, admin, "marta").await;
    let taken = index(&test, folder, "taken.jpg", AssetKind::Image, 2).await;
    let skipped = index(&test, folder, "skipped.jpg", AssetKind::Image, 3).await;
    let other_users_pick = index(&test, folder, "not-mine.jpg", AssetKind::Image, 4).await;

    set_pick(&test, taken, admin, "pick").await;
    set_pick(&test, skipped, admin, "reject").await;
    set_pick(&test, other_users_pick, other_user, "pick").await;

    let picked = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Pick {
                value: keeppix_domain::Pick::Pick,
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(picked.len(), 1);
    assert_eq!(picked[0].id, taken);

    let rejected = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Pick {
                value: keeppix_domain::Pick::Reject,
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].id, skipped);
}

#[tokio::test]
async fn pick_none_matches_both_never_flagged_and_explicitly_cleared() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let admin = ctx.user_id().unwrap();
    // Never touched via asset_flags: no row at all for this user, which is
    // how the vast majority of assets actually exist (no column default
    // writes 'none' on its own).
    let never_touched = index(&test, folder, "never-touched.jpg", AssetKind::Image, 2).await;
    // Picked and then the pick was cleared: an explicit pick='none' row.
    let cleared = index(&test, folder, "cleared.jpg", AssetKind::Image, 3).await;
    set_pick(&test, cleared, admin, "none").await;
    // The opposite case: must stay out of both results above.
    let picked = index(&test, folder, "picked.jpg", AssetKind::Image, 4).await;
    set_pick(&test, picked, admin, "pick").await;

    let found = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Pick {
                value: keeppix_domain::Pick::None,
            },
            None,
            50,
        )
        .await
        .unwrap();

    let mut ids: Vec<_> = found.iter().map(|a| a.id.as_uuid()).collect();
    ids.sort();
    let mut expected = vec![never_touched.as_uuid(), cleared.as_uuid()];
    expected.sort();
    assert_eq!(
        ids, expected,
        "\"to review\" must include both assets that never had a flag and \
         ones where it was explicitly cleared — never only the latter"
    );
}

/// A real EXPLAIN at a scale that makes the planner's choice visible, same
/// principle as `favorite_search_uses_the_partial_index` (with few assets
/// the partial index and a seq scan cost the same). Measured, not assumed:
/// `asset_flags_user_pick_idx` is partial on `pick <> 'none'` (migration
/// 0012), so it isn't obvious that the `Pick::None` branch (`NOT EXISTS ...
/// IN ('pick','reject')`) uses it the same way as the `Pick::Pick`/`Reject`
/// branch (`EXISTS ... pick = $2`).
#[tokio::test]
async fn pick_search_uses_the_partial_index_for_pick_and_reject() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let user = ctx.user_id().unwrap();
    let pool = test.db().pool();

    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status, \
                             taken_at_utc, width, height) \
         SELECT gen_random_uuid(), $1, 'IMG_' || lpad(g::text, 6, '0') || '.jpg', \
                200000, now(), 'image', 'indexed', now() - make_interval(mins => g), 100, 100 \
           FROM generate_series(1, 20000) AS g",
    )
    .bind(folder.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO asset_flags (asset_id, user_id, pick, updated_at) \
         SELECT a.id, $1, \
                CASE WHEN row_number() OVER (ORDER BY a.id) % 12 = 0 THEN 'pick' \
                     WHEN row_number() OVER (ORDER BY a.id) % 12 = 1 THEN 'reject' \
                     ELSE 'none' END, \
                now() \
           FROM assets a WHERE a.folder_id = $2",
    )
    .bind(user.as_uuid())
    .bind(folder.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("VACUUM ANALYZE assets, asset_flags")
        .execute(pool)
        .await
        .unwrap();

    let explain_pick = "EXPLAIN (FORMAT TEXT) SELECT a.id FROM assets a \
        WHERE a.status = 'indexed' \
          AND (EXISTS (SELECT 1 FROM asset_flags af \
                       WHERE af.asset_id = a.id AND af.user_id = $1 AND af.pick = $2)) \
        ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC LIMIT 200";
    let plan: Vec<String> = sqlx::query_scalar(explain_pick)
        .bind(user.as_uuid())
        .bind("pick")
        .fetch_all(pool)
        .await
        .unwrap();
    let plan = plan.join("\n");
    eprintln!("EXPLAIN pick:\n{plan}");
    assert!(
        plan.contains("asset_flags_user_pick_idx"),
        "the Pick::Pick filter must be served from the partial index asset_flags_user_pick_idx:\n{plan}"
    );
}

#[tokio::test]
async fn date_range_filter_includes_both_endpoints() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let before = index_at(
        &test,
        folder,
        "before.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2024, 3, 9, 23, 59, 59).unwrap(),
    )
    .await;
    let start = index_at(
        &test,
        folder,
        "start.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2024, 3, 10, 0, 0, 0).unwrap(),
    )
    .await;
    let end = index_at(
        &test,
        folder,
        "end.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2024, 3, 12, 23, 59, 59).unwrap(),
    )
    .await;
    let after = index_at(
        &test,
        folder,
        "after.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2024, 3, 13, 0, 0, 1).unwrap(),
    )
    .await;

    let found = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::DateRange {
                from: Utc.with_ymd_and_hms(2024, 3, 10, 0, 0, 0).unwrap(),
                to: Utc.with_ymd_and_hms(2024, 3, 12, 23, 59, 59).unwrap(),
            },
            None,
            50,
        )
        .await
        .unwrap();
    let ids: Vec<_> = found.iter().map(|a| a.id).collect();
    assert!(ids.contains(&start), "the start endpoint is included");
    assert!(ids.contains(&end), "the end endpoint is included");
    assert!(!ids.contains(&before));
    assert!(!ids.contains(&after));
}

#[tokio::test]
async fn day_filter_matches_the_day_of_month_regardless_of_year_or_month() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let jan = index_at(
        &test,
        folder,
        "jan15.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2022, 1, 15, 12, 0, 0).unwrap(),
    )
    .await;
    let aug = index_at(
        &test,
        folder,
        "aug15.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2024, 8, 15, 6, 0, 0).unwrap(),
    )
    .await;
    let other_day = index_at(
        &test,
        folder,
        "aug16.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2024, 8, 16, 6, 0, 0).unwrap(),
    )
    .await;

    let found = SearchRepo::new(test.db())
        .run(&ctx, &SearchNode::Day { value: 15 }, None, 50)
        .await
        .unwrap();
    let ids: Vec<_> = found.iter().map(|a| a.id).collect();
    assert!(ids.contains(&jan));
    assert!(ids.contains(&aug));
    assert!(!ids.contains(&other_day));
}

#[tokio::test]
async fn month_filter_matches_the_month_of_year_regardless_of_year() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let march_2022 = index_at(
        &test,
        folder,
        "march2022.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2022, 3, 3, 12, 0, 0).unwrap(),
    )
    .await;
    let march_2024 = index_at(
        &test,
        folder,
        "march2024.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2024, 3, 20, 12, 0, 0).unwrap(),
    )
    .await;
    let december = index_at(
        &test,
        folder,
        "december.jpg",
        AssetKind::Image,
        Utc.with_ymd_and_hms(2023, 12, 3, 12, 0, 0).unwrap(),
    )
    .await;

    let found = SearchRepo::new(test.db())
        .run(&ctx, &SearchNode::Month { value: 3 }, None, 50)
        .await
        .unwrap();
    let ids: Vec<_> = found.iter().map(|a| a.id).collect();
    assert!(ids.contains(&march_2022));
    assert!(ids.contains(&march_2024));
    assert!(!ids.contains(&december));
}

#[tokio::test]
async fn country_filter_follows_place_id_to_the_places_catalog() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let italy = Place {
        id: 3_169_070,
        name: "Roma".to_owned(),
        ascii_name: "Roma".to_owned(),
        country_code: Some("IT".to_owned()),
        admin1: None,
        admin2: None,
        location: GeoPoint {
            lat: 41.891_93,
            lon: 12.511_33,
        },
        population: 2_318_895,
    };
    let france = Place {
        id: 2_988_507,
        name: "Paris".to_owned(),
        ascii_name: "Paris".to_owned(),
        country_code: Some("FR".to_owned()),
        admin1: None,
        admin2: None,
        location: GeoPoint {
            lat: 48.856_61,
            lon: 2.351_22,
        },
        population: 2_138_551,
    };
    PlaceRepo::new(test.db()).upsert(&italy).await.unwrap();
    PlaceRepo::new(test.db()).upsert(&france).await.unwrap();

    let roman = index(&test, folder, "rome.jpg", AssetKind::Image, 2).await;
    let parisian = index(&test, folder, "paris.jpg", AssetKind::Image, 3).await;
    set_place(&test, roman, italy.id).await;
    set_place(&test, parisian, france.id).await;

    let found = SearchRepo::new(test.db())
        .run(
            &ctx,
            // Lowercase on purpose: the comparison normalizes the country.
            &SearchNode::Country {
                value: "it".to_owned(),
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, roman);
}

#[tokio::test]
async fn aperture_filter_compares_f_number() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let wide = index(&test, folder, "wide.jpg", AssetKind::Image, 2).await;
    let narrow = index(&test, folder, "narrow.jpg", AssetKind::Image, 3).await;
    let assets = AssetRepo::new(test.db());
    assets
        .insert_exif(
            wide,
            &ExifData {
                f_number: Some(1.8),
                ..blank_exif(Utc.with_ymd_and_hms(2024, 7, 2, 12, 0, 0).unwrap())
            },
        )
        .await
        .unwrap();
    assets
        .insert_exif(
            narrow,
            &ExifData {
                f_number: Some(11.0),
                ..blank_exif(Utc.with_ymd_and_hms(2024, 7, 3, 12, 0, 0).unwrap())
            },
        )
        .await
        .unwrap();

    let found = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Aperture {
                cmp: keeppix_db::IsoCmp::Lte,
                value: 4.0,
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, wide);
}

#[tokio::test]
async fn shutter_filter_compares_exposure_time_in_seconds() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let fast = index(&test, folder, "fast.jpg", AssetKind::Image, 2).await;
    let slow_fraction = index(&test, folder, "slow-fraction.jpg", AssetKind::Image, 3).await;
    let long_exposure = index(&test, folder, "long.jpg", AssetKind::Image, 4).await;
    let assets = AssetRepo::new(test.db());
    for (id, exposure, day) in [
        (fast, "1/1000", 2),
        (slow_fraction, "1/30", 3),
        (long_exposure, "2", 4),
    ] {
        assets
            .insert_exif(
                id,
                &ExifData {
                    exposure: Some(exposure.to_owned()),
                    ..blank_exif(Utc.with_ymd_and_hms(2024, 7, day, 12, 0, 0).unwrap())
                },
            )
            .await
            .unwrap();
    }

    // Faster than 1/50s (0.02s): only 1/1000s (0.001s) qualifies.
    let found = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Shutter {
                cmp: keeppix_db::IsoCmp::Lt,
                value: 0.02,
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, fast);
}

#[tokio::test]
async fn place_filter_matches_the_exact_place_not_a_folder() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let a = index(&test, folder, "a.jpg", AssetKind::Image, 2).await;
    let b = index(&test, folder, "b.jpg", AssetKind::Image, 3).await;
    set_place(&test, a, 111).await;
    set_place(&test, b, 222).await;

    let found = SearchRepo::new(test.db())
        .run(&ctx, &SearchNode::Place { id: 111 }, None, 50)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, a);
}

#[tokio::test]
async fn a_deeply_nested_ast_still_hits_the_depth_guard() {
    let test = TestDb::start().await;
    let (ctx, _) = seed(&test).await;
    let mut ast = SearchNode::HasGps;
    for _ in 0..24 {
        ast = SearchNode::Not { arg: Box::new(ast) };
    }

    let result = SearchRepo::new(test.db()).run(&ctx, &ast, None, 50).await;
    assert!(
        matches!(result, Err(keeppix_db::DbError::Conflict(_))),
        "a 24-level AST must be rejected by the depth guard, not executed"
    );
}

/// A real EXPLAIN at a scale that makes the planner's choice visible (with
/// few assets, the partial index and a seq scan cost the same). Same SQL
/// produced by `compile_for_sql` for `Favorite`, duplicated here on purpose
/// because that query is private to the repository — same pattern as
/// `scale_geometry.rs`.
#[tokio::test]
async fn favorite_search_uses_the_partial_index() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let user = ctx.user_id().unwrap();
    let pool = test.db().pool();

    sqlx::query(
        "INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, kind, status, \
                             taken_at_utc, width, height) \
         SELECT gen_random_uuid(), $1, 'IMG_' || lpad(g::text, 6, '0') || '.jpg', \
                200000, now(), 'image', 'indexed', now() - make_interval(mins => g), 100, 100 \
           FROM generate_series(1, 20000) AS g",
    )
    .bind(folder.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO asset_flags (asset_id, user_id, favorite, updated_at) \
         SELECT a.id, $1, (row_number() OVER (ORDER BY a.id) % 12 = 0), now() \
           FROM assets a WHERE a.folder_id = $2",
    )
    .bind(user.as_uuid())
    .bind(folder.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("VACUUM ANALYZE assets, asset_flags")
        .execute(pool)
        .await
        .unwrap();

    let explain = "EXPLAIN (FORMAT TEXT) SELECT a.id FROM assets a \
        WHERE a.status = 'indexed' \
          AND (EXISTS (SELECT 1 FROM asset_flags af \
                       WHERE af.asset_id = a.id AND af.user_id = $1 AND af.favorite)) \
        ORDER BY a.taken_at_utc DESC NULLS LAST, a.id DESC LIMIT 200";
    let plan: Vec<String> = sqlx::query_scalar(explain)
        .bind(user.as_uuid())
        .fetch_all(pool)
        .await
        .unwrap();
    let plan = plan.join("\n");
    eprintln!("EXPLAIN favorite:\n{plan}");
    assert!(
        plan.contains("asset_flags_favorite_idx"),
        "the Favorite filter must be served from the partial index asset_flags_favorite_idx:\n{plan}"
    );
}

/// A suggestion for each of the six active sources. `tag` has no source
/// here yet: it never shows up, but it remains in the enum.
#[tokio::test]
async fn suggest_returns_a_typed_result_for_each_supported_kind() {
    let test = TestDb::start().await;
    let (ctx, root) = seed(&test).await;
    let root_folder = FolderRepo::new(test.db())
        .find_by_id(&ctx, root)
        .await
        .unwrap();
    let child = FolderRepo::new(test.db())
        .ensure_child(&root_folder, "Cortile")
        .await
        .unwrap();

    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(child.id, "veranda.jpg", AssetKind::Image))
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(
            a.id,
            Utc.with_ymd_and_hms(2031, 6, 1, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();
    assets
        .insert_exif(
            a.id,
            &ExifData {
                camera_model: Some("PrototipoX9000".to_owned()),
                iso: Some(777),
                ..blank_exif(Utc.with_ymd_and_hms(2031, 6, 1, 12, 0, 0).unwrap())
            },
        )
        .await
        .unwrap();
    let place = Place {
        id: 9_988_776,
        name: "Urbino".to_owned(),
        ascii_name: "Urbino".to_owned(),
        country_code: Some("IT".to_owned()),
        admin1: None,
        admin2: None,
        location: GeoPoint {
            lat: 43.726,
            lon: 12.636,
        },
        population: 14_800,
    };
    PlaceRepo::new(test.db()).upsert(&place).await.unwrap();
    set_place(&test, a.id, place.id).await;

    let repo = SearchRepo::new(test.db());
    // (query, kind, expected value and label — they coincide for every
    // source except `folder`, where `value` is the id for navigation and
    // `label` the readable name: handled separately below).
    let cases = [
        (
            "veranda",
            SuggestionKind::Filename,
            "veranda.jpg",
            "veranda.jpg",
        ),
        (
            "Prototipo",
            SuggestionKind::Camera,
            "PrototipoX9000",
            "PrototipoX9000",
        ),
        ("2031", SuggestionKind::Year, "2031", "2031"),
        ("777", SuggestionKind::Iso, "777", "777"),
        ("IT", SuggestionKind::Country, "IT", "IT"),
    ];
    for (query, kind, value, label) in cases {
        let found = repo.suggest(&ctx, query).await.unwrap();
        let matching = found
            .iter()
            .find(|s| s.kind == kind)
            .unwrap_or_else(|| panic!("no suggestion of type {kind:?} for {query:?}: {found:?}"));
        assert_eq!(matching.value, value, "query {query:?}");
        assert_eq!(matching.label, label, "query {query:?}");
    }

    let folder_found = repo.suggest(&ctx, "Cort").await.unwrap();
    let folder_match = folder_found
        .iter()
        .find(|s| s.kind == SuggestionKind::Folder)
        .expect("no folder-type suggestion for \"Cort\"");
    assert_eq!(
        folder_match.value,
        child.id.as_uuid().to_string(),
        "a folder suggestion's value is the id, used to build SearchNode::Folder"
    );
    assert_eq!(folder_match.label, "Cortile");
}

#[tokio::test]
async fn tag_filter_matches_only_confirmed_assignments() {
    use keeppix_db::{NewTag, TagRepo};
    use keeppix_domain::TagKind;

    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let keep = index(&test, folder, "keep.jpg", AssetKind::Image, 1).await;
    let proposed = index(&test, folder, "proposed.jpg", AssetKind::Image, 2).await;
    let _other = index(&test, folder, "other.jpg", AssetKind::Image, 3).await;

    let tag = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Mare".into(),
                kind: TagKind::Tag,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) VALUES \
         ($1, $2, 'confirmed', 'ai', 0.9), ($3, $2, 'proposed', 'ai', 0.9)",
    )
    .bind(keep.as_uuid())
    .bind(tag.id.as_uuid())
    .bind(proposed.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let hits = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Tag {
                id: tag.id.as_uuid(),
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, keep);
}

#[tokio::test]
async fn category_filter_matches_confirmed_child_tags() {
    use keeppix_db::{NewTag, TagRepo};
    use keeppix_domain::TagKind;

    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let in_cat = index(&test, folder, "in.jpg", AssetKind::Image, 1).await;
    let _out = index(&test, folder, "out.jpg", AssetKind::Image, 2).await;

    let cat = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Viaggi".into(),
                kind: TagKind::Category,
                parent_id: None,
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();
    let child = TagRepo::new(test.db())
        .create(
            &ctx,
            NewTag {
                name: "Grecia".into(),
                kind: TagKind::Tag,
                parent_id: Some(cat.id),
                prompt: None,
                color: None,
                threshold: None,
                embedding: None,
                model_version: None,
            },
        )
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) VALUES ($1, $2, 'confirmed', 'user', NULL)",
    )
    .bind(in_cat.as_uuid())
    .bind(child.id.as_uuid())
    .execute(test.db().pool())
    .await
    .unwrap();

    let hits = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Category {
                id: cat.id.as_uuid(),
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, in_cat);
}

async fn assign_face(
    test: &TestDb,
    ctx: &AuthContext,
    asset_id: keeppix_domain::AssetId,
    person: keeppix_domain::PersonId,
) {
    use keeppix_db::{FaceRepo, NewDetectedFace};
    use keeppix_domain::FaceBBox;

    let face = FaceRepo::new(test.db())
        .insert_detected(NewDetectedFace {
            asset_id,
            bbox: FaceBBox {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
            },
            landmarks: None,
            embedding: None,
            detect_score: 0.9,
            quality: None,
            model_version: "test".to_owned(),
        })
        .await
        .unwrap();
    FaceRepo::new(test.db())
        .assign(ctx, face.id, person)
        .await
        .unwrap();
}

#[tokio::test]
async fn person_filter_matches_only_assigned_faces() {
    use keeppix_db::PersonRepo;

    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let with_person = index(&test, folder, "with.jpg", AssetKind::Image, 1).await;
    let _without = index(&test, folder, "without.jpg", AssetKind::Image, 2).await;

    let person = PersonRepo::new(test.db()).create(None).await.unwrap();
    assign_face(&test, &ctx, with_person, person.id).await;

    let hits = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Person {
                id: person.id.as_uuid(),
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, with_person);
}

#[tokio::test]
async fn person_group_filter_matches_any_member() {
    use keeppix_db::{NewPersonGroup, PersonGroupRepo, PersonRepo};

    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let a_photo = index(&test, folder, "a.jpg", AssetKind::Image, 1).await;
    let b_photo = index(&test, folder, "b.jpg", AssetKind::Image, 2).await;
    let _outside = index(&test, folder, "outside.jpg", AssetKind::Image, 3).await;

    let persons = PersonRepo::new(test.db());
    let a = persons.create(None).await.unwrap();
    let b = persons.create(None).await.unwrap();
    assign_face(&test, &ctx, a_photo, a.id).await;
    assign_face(&test, &ctx, b_photo, b.id).await;

    let groups = PersonGroupRepo::new(test.db());
    let family = groups
        .create(
            &ctx,
            NewPersonGroup {
                name: "Famiglia".to_owned(),
            },
        )
        .await
        .unwrap();
    groups.add_member(&ctx, family.id, a.id).await.unwrap();
    groups.add_member(&ctx, family.id, b.id).await.unwrap();

    let hits = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::PersonGroup {
                id: family.id.as_uuid(),
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|a| a.id == a_photo));
    assert!(hits.iter().any(|a| a.id == b_photo));
}

#[tokio::test]
async fn person_count_filter_counts_distinct_persons_not_faces() {
    use keeppix_db::{IsoCmp, PersonRepo};

    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let group_photo = index(&test, folder, "group.jpg", AssetKind::Image, 1).await;
    let solo_photo = index(&test, folder, "solo.jpg", AssetKind::Image, 2).await;

    let persons = PersonRepo::new(test.db());
    let a = persons.create(None).await.unwrap();
    let b = persons.create(None).await.unwrap();
    assign_face(&test, &ctx, group_photo, a.id).await;
    assign_face(&test, &ctx, group_photo, b.id).await;
    assign_face(&test, &ctx, solo_photo, a.id).await;

    let hits = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::PersonCount {
                cmp: IsoCmp::Gte,
                value: 2,
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, group_photo);
}

#[tokio::test]
async fn semantic_filters_top_k_then_orders_by_date() {
    use keeppix_db::{EmbeddingRepo, MODEL_VERSION};

    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let far = index(&test, folder, "far.jpg", AssetKind::Image, 1).await;
    let mid = index(&test, folder, "mid.jpg", AssetKind::Image, 2).await;
    let near = index(&test, folder, "near.jpg", AssetKind::Image, 3).await;

    let mut unit = vec![0.0_f32; 512];
    unit[0] = 1.0;
    let mut almost = vec![0.0_f32; 512];
    almost[0] = 0.9;
    almost[1] = (1.0_f32 - 0.9_f32 * 0.9_f32).sqrt();
    let mut orthogonal = vec![0.0_f32; 512];
    orthogonal[1] = 1.0;

    let repo = EmbeddingRepo::new(test.db());
    repo.upsert(near, &unit, MODEL_VERSION).await.unwrap();
    repo.upsert(mid, &almost, MODEL_VERSION).await.unwrap();
    repo.upsert(far, &orthogonal, MODEL_VERSION).await.unwrap();

    let hits = SearchRepo::new(test.db())
        .run(
            &ctx,
            &SearchNode::Semantic {
                query: "ignored when embedding set".into(),
                limit: 2,
                embedding: Some(unit.clone()),
            },
            None,
            50,
        )
        .await
        .unwrap();
    assert_eq!(hits.len(), 2, "K=2 nearest");
    assert_eq!(hits[0].id, near, "newest among the K first");
    assert_eq!(hits[1].id, mid);
    assert!(!hits.iter().any(|h| h.id == far));
}

/// The part that's easy to get wrong: the K nearest semantic neighbors
/// must be computed **inside** the visibility perimeter, not as a global K
/// filtered afterward. The private asset is the exact match and would be
/// the global K=1; if visibility were applied only by the outer `WHERE`
/// (and not inside the subquery), the stranger — who can't see `private`
/// — would get zero results instead of the public one, which is still a
/// valid neighbor.
#[tokio::test]
async fn semantic_search_selects_the_k_nearest_among_visible_assets_only() {
    use keeppix_db::{
        EmbeddingRepo, MODEL_VERSION, NewGrant, ObjectType, PermissionRepo, SubjectType,
    };
    use keeppix_domain::ObjectRole;

    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    let admin = ctx.user_id().unwrap();
    let stranger = harness::seed_user(&test, admin, "estranea").await;

    let root_folder = FolderRepo::new(test.db())
        .find_by_id(&ctx, folder)
        .await
        .unwrap();
    let public = FolderRepo::new(test.db())
        .ensure_child(&root_folder, "public")
        .await
        .unwrap();
    let private = FolderRepo::new(test.db())
        .ensure_child(&root_folder, "private")
        .await
        .unwrap();
    PermissionRepo::new(test.db())
        .grant(
            &ctx,
            NewGrant {
                subject: SubjectType::User,
                subject_id: stranger.as_uuid(),
                object: ObjectType::Folder,
                object_id: public.id.as_uuid(),
                role: ObjectRole::Viewer,
                inherit: true,
            },
        )
        .await
        .unwrap();

    let public_asset = index(&test, public.id, "public.jpg", AssetKind::Image, 1).await;
    let private_asset = index(&test, private.id, "private.jpg", AssetKind::Image, 2).await;

    let mut query_vector = vec![0.0_f32; 512];
    query_vector[0] = 1.0;
    let mut near_but_not_exact = vec![0.0_f32; 512];
    near_but_not_exact[0] = 0.9;
    near_but_not_exact[1] = (1.0_f32 - 0.9_f32 * 0.9_f32).sqrt();

    let embeddings = EmbeddingRepo::new(test.db());
    embeddings
        .upsert(private_asset, &query_vector, MODEL_VERSION)
        .await
        .unwrap();
    embeddings
        .upsert(public_asset, &near_but_not_exact, MODEL_VERSION)
        .await
        .unwrap();

    let semantic = SearchNode::Semantic {
        query: "ignored when embedding set".into(),
        limit: 1,
        embedding: Some(query_vector.clone()),
    };

    let as_admin = SearchRepo::new(test.db())
        .run(&ctx, &semantic, None, 50)
        .await
        .unwrap();
    assert_eq!(as_admin.len(), 1);
    assert_eq!(
        as_admin[0].id, private_asset,
        "admin sees everything: the global K=1 is the private one"
    );

    let stranger_ctx = AuthContext::user(stranger, SystemRole::User);
    let as_stranger = SearchRepo::new(test.db())
        .run(&stranger_ctx, &semantic, None, 50)
        .await
        .unwrap();
    assert_eq!(
        as_stranger.len(),
        1,
        "K nearest must be computed inside the visible perimeter: the \
         stranger must see the public one, not zero results"
    );
    assert_eq!(as_stranger[0].id, public_asset);
}

#[tokio::test]
async fn suggest_never_offers_the_tag_kind_without_a_source() {
    let test = TestDb::start().await;
    let (ctx, folder) = seed(&test).await;
    index(&test, folder, "tag.jpg", AssetKind::Image, 2).await;

    let found = SearchRepo::new(test.db())
        .suggest(&ctx, "tag")
        .await
        .unwrap();
    assert!(
        !found.iter().any(|s| s.kind == SuggestionKind::Tag),
        "without a populated tag table, it must not invent tag suggestions"
    );
}
