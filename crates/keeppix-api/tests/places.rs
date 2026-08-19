mod harness;

use chrono::{TimeZone as _, Utc};
use harness::TestServer;
use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo, OverrideRepo, PlaceRepo, UserRepo};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, GeoPoint, NewAsset, NewLibrary, OverridePatch, Place,
    SystemRole, UserId, Username,
};
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn places_endpoints_require_an_authenticated_session() {
    let server = TestServer::start().await;
    let client = reqwest::Client::new();

    for path in [
        "/api/v1/places/reverse?lat=40&lon=14",
        "/api/v1/places/suggest?q=rome&near_user=true",
    ] {
        let response = client.get(server.url(path)).send().await.unwrap();
        assert_eq!(response.status(), 401);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["type"], "keeppix/unauthenticated");
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn reverse_returns_the_large_city_inside_its_population_radius() {
    let server = TestServer::start().await;
    setup(&server).await;
    let repo = PlaceRepo::new(&server.db);
    repo.upsert(&place(
        10,
        "Tiny village",
        "Tiny village",
        "IT",
        45.0,
        10.041,
        600,
    ))
    .await
    .unwrap();
    repo.upsert(&place(
        11,
        "Large city",
        "Large city",
        "IT",
        45.18,
        10.0,
        500_000,
    ))
    .await
    .unwrap();

    let response = server
        .client
        .get(server.url("/api/v1/places/reverse?lat=45&lon=10"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["id"], 11);
    assert_eq!(body["name"], "Large city");
    assert_eq!(body["country_code"], "IT");
    assert_eq!(body["lat"], 45.18);
    assert_eq!(body["lon"], 10.0);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn reverse_over_ocean_is_a_clean_problem_json_404() {
    let server = TestServer::start().await;
    setup(&server).await;
    PlaceRepo::new(&server.db)
        .upsert(&place(
            20, "Rome", "Rome", "IT", 41.891_93, 12.511_33, 2_318_895,
        ))
        .await
        .unwrap();

    let response = server
        .client
        .get(server.url("/api/v1/places/reverse?lat=-30&lon=-140"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/place-not-found");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn suggest_rejects_queries_shorter_than_two_characters() {
    let server = TestServer::start().await;
    setup(&server).await;

    let response = server
        .client
        .get(server.url("/api/v1/places/suggest?q=x&near_user=true"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/place-query-too-short");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn suggest_ranks_sorrento_near_the_users_recent_overrides_first() {
    let server = TestServer::start().await;
    let user = setup(&server).await;
    let repo = PlaceRepo::new(&server.db);
    repo.upsert(&place(
        30, "Sorrento", "Sorrento", "IT", 40.626_78, 14.377_71, 14_950,
    ))
    .await
    .unwrap();
    repo.upsert(&place(
        31,
        "Sorrento Valley",
        "Sorrento",
        "US",
        32.899_77,
        -117.194_26,
        20_000,
    ))
    .await
    .unwrap();
    seed_override_history(&server, user, 50).await;

    let response = server
        .client
        .get(server.url("/api/v1/places/suggest?q=sorren&near_user=true"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body.as_array().unwrap()[0]["id"], 30);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn suggest_returns_at_most_ten_places() {
    let server = TestServer::start().await;
    setup(&server).await;
    let repo = PlaceRepo::new(&server.db);
    for index in 0_i32..12 {
        repo.upsert(&place(
            100 + i64::from(index),
            &format!("Springfield {index}"),
            &format!("Springfield {index}"),
            "US",
            40.0 + f64::from(index) / 100.0,
            -90.0,
            10_000 + index,
        ))
        .await
        .unwrap();
    }

    let response = server
        .client
        .get(server.url("/api/v1/places/suggest?q=spring&near_user=false"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 10);
}

fn place(
    id: i64,
    name: &str,
    ascii_name: &str,
    country_code: &str,
    lat: f64,
    lon: f64,
    population: i32,
) -> Place {
    Place {
        id,
        name: name.to_owned(),
        ascii_name: ascii_name.to_owned(),
        country_code: Some(country_code.to_owned()),
        admin1: None,
        admin2: None,
        location: GeoPoint { lat, lon },
        population,
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn setup(server: &TestServer) -> UserId {
    server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "giovanni",
            "display_name": "Giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    UserRepo::new(&server.db)
        .find_by_username(&Username::parse("giovanni").unwrap())
        .await
        .unwrap()
        .expect("admin")
        .0
        .id
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn seed_override_history(server: &TestServer, user: UserId, count: usize) {
    let ctx = AuthContext::user(user, SystemRole::Admin);
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Campania history".to_owned(),
                owner_id: user,
                root_path: server.photos_root.join("campania"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("history library");
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &[])
        .await
        .expect("history folder");
    let assets = AssetRepo::new(&server.db);
    let overrides = OverrideRepo::new(&server.db);
    for index in 0..count {
        let asset = assets
            .upsert_discovered(NewAsset {
                folder_id: folder.id,
                filename: AssetName::parse(&format!("campania-{index:02}.jpg"))
                    .expect("asset name"),
                size_bytes: 1,
                mtime: Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap(),
                inode: None,
                kind: AssetKind::Image,
            })
            .await
            .expect("history asset")
            .expect("new history asset");
        overrides
            .apply(
                &ctx,
                asset.id,
                &OverridePatch {
                    location: Some(Some(GeoPoint {
                        lat: 40.75,
                        lon: 14.5,
                    })),
                    ..OverridePatch::default()
                },
            )
            .await
            .expect("history override");
    }
}
