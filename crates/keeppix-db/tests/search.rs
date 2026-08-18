#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, SearchNode, SearchRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, ExifData, NewAsset, NewLibrary, SystemRole,
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
    let assets = AssetRepo::new(test.db());
    let a = assets
        .upsert_discovered(photo(folder, name, kind))
        .await
        .unwrap()
        .unwrap();
    assets
        .set_indexed(
            a.id,
            Utc.with_ymd_and_hms(2024, 7, day, 12, 0, 0).unwrap(),
            1,
            1,
        )
        .await
        .unwrap();
    a.id
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
