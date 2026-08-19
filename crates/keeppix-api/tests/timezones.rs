mod harness;

use chrono::{TimeZone as _, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, GeoRepo, LibraryRepo, UserRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, GeoPoint, NewAsset, NewLibrary, NewUser, Password,
    SystemRole, UserId, Username, hash_password,
};
use serde_json::{Value, json};

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn setup_admin(server: &TestServer) -> UserId {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .expect("setup");
    response.json::<Value>().await.expect("setup JSON")["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("valid user id")
}

#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn seed_user(server: &TestServer, admin: UserId) {
    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            NewUser {
                username: Username::parse("stranger").expect("username"),
                email: None,
                display_name: "Stranger".to_owned(),
                password_hash: hash_password(
                    &Password::parse("correct horse battery staple").expect("password"),
                )
                .expect("hash")
                .as_str()
                .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .expect("user");
}

#[tokio::test]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::too_many_lines)]
async fn preview_and_apply_are_separate_authenticated_operations() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Tokyo".to_owned(),
                owner_id: admin,
                root_path: server.photos_root.join("tokyo"),
                exclude_patterns: Vec::new(),
            },
        )
        .await
        .expect("library");
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &["2026"])
        .await
        .expect("folder");
    let naive = Utc.with_ymd_and_hms(2026, 8, 18, 14, 0, 0).unwrap();
    let asset = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse("DSC_4412.ARW").unwrap(),
            size_bytes: 1,
            mtime: naive,
            inode: None,
            kind: AssetKind::RawImage,
        })
        .await
        .expect("asset")
        .expect("new asset");
    AssetRepo::new(&server.db)
        .set_indexed(asset.id, naive, 100, 100)
        .await
        .expect("indexed");
    AssetRepo::new(&server.db)
        .set_exif_location(
            asset.id,
            GeoPoint {
                lat: 35.6762,
                lon: 139.6503,
            },
        )
        .await
        .expect("GPS");
    let timezone_fixture = server.data_dir.join("tz_boundaries.csv");
    tokio::fs::write(
        &timezone_fixture,
        "Asia/Tokyo\t{\"type\":\"MultiPolygon\",\"coordinates\":[[[[138,34],[141,34],[141,37],[138,37],[138,34]]]]}\n",
    )
    .await
    .expect("timezone fixture");
    GeoRepo::new(&server.db)
        .seed_timezones_from_csv_if_empty(&timezone_fixture)
        .await
        .expect("timezone");

    let preview = server
        .client
        .post(server.url("/api/v1/metadata/batch/recalculate-timezones/preview"))
        .json(&json!({ "library_id": library.id.to_string() }))
        .send()
        .await
        .expect("preview");
    assert_eq!(preview.status(), 200);
    let preview: Value = preview.json().await.expect("preview JSON");
    assert_eq!(preview["count"], 1);
    assert_eq!(preview["example"]["filename"], "DSC_4412.ARW");
    assert_eq!(preview["example"]["before"], "2026-08-18T14:00:00Z");
    assert_eq!(preview["example"]["after"], "2026-08-18T05:00:00Z");
    let preview_token = preview["preview_token"].as_str().expect("preview_token");

    // Apply without token must fail.
    let no_token = server
        .client
        .post(server.url("/api/v1/metadata/batch/recalculate-timezones"))
        .json(&json!({ "library_id": library.id.to_string() }))
        .send()
        .await
        .expect("apply without token");
    assert_eq!(no_token.status(), 409);

    let apply = server
        .client
        .post(server.url("/api/v1/metadata/batch/recalculate-timezones"))
        .json(&json!({
            "library_id": library.id.to_string(),
            "preview_token": preview_token
        }))
        .send()
        .await
        .expect("apply");
    assert_eq!(apply.status(), 200);
    let apply: Value = apply.json().await.expect("apply JSON");
    assert_eq!(apply["changed_count"], 1);
    assert!(apply["batch_id"].is_string());

    // Token is single-use: second apply fails.
    let reuse = server
        .client
        .post(server.url("/api/v1/metadata/batch/recalculate-timezones"))
        .json(&json!({
            "library_id": library.id.to_string(),
            "preview_token": preview_token
        }))
        .send()
        .await
        .expect("reuse");
    assert_eq!(reuse.status(), 409);
}

#[tokio::test]
#[allow(clippy::expect_used, clippy::unwrap_used)]
async fn a_foreign_library_returns_forbidden_problem_json() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let library = LibraryRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            NewLibrary {
                name: "Private".to_owned(),
                owner_id: admin,
                root_path: server.photos_root.join("private"),
                exclude_patterns: Vec::new(),
            },
        )
        .await
        .expect("library");
    seed_user(&server, admin).await;
    let login = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "stranger",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .expect("login");
    assert_eq!(login.status(), 200);

    for path in [
        "/api/v1/metadata/batch/recalculate-timezones/preview",
        "/api/v1/metadata/batch/recalculate-timezones",
    ] {
        let response = server
            .client
            .post(server.url(path))
            .json(&json!({ "library_id": library.id.to_string() }))
            .send()
            .await
            .expect("request");
        assert_eq!(response.status(), 403);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        assert_eq!(
            response.json::<Value>().await.unwrap()["type"],
            "keeppix/forbidden"
        );
    }
}
