#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;
mod journey;

use std::fs;
use std::time::{Duration, Instant};

use harness::TestServer;
use journey::{
    create_library, create_share_link_from, scan_and_wait, setup_admin, share_client,
    tiny_fixture_path,
};
use keeppix_db::{AssetRepo, HomeRepo, UserRepo};
use keeppix_domain::{
    AuthContext, GeoPoint, NewUser, Password, SystemRole, UserId, Username, hash_password,
};
use serde_json::{Value, json};

const HOME_LAT: f64 = 45.0;
const HOME_LON: f64 = 7.0;
const RADIUS_M: i32 = 200;

#[allow(clippy::too_many_lines)]
async fn seed_asset(server: &TestServer, filename: &str, lat: f64, lon: f64) -> (String, String) {
    let root = server
        .photos_root
        .join(format!("geofence-{}", uuid::Uuid::now_v7().simple()));
    fs::create_dir_all(&root).unwrap();
    fs::copy(tiny_fixture_path(), root.join(filename)).unwrap();

    let library_id = create_library(server, "Geofence", &root).await;
    let deadline = Instant::now() + Duration::from_secs(90);
    scan_and_wait(server, &library_id, 1, deadline).await;

    let buckets = server
        .client
        .get(server.url("/api/v1/timeline/buckets"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let month = buckets[0]["month"].as_str().unwrap();
    let page = server
        .client
        .get(server.url(&format!("/api/v1/timeline?bucket={month}")))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let asset_row = page["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["filename"].as_str() == Some(filename))
        .expect("seeded asset");
    let asset_id: keeppix_domain::AssetId = asset_row["id"].as_str().unwrap().parse().unwrap();
    AssetRepo::new(&server.db)
        .set_exif_location(asset_id, GeoPoint { lat, lon })
        .await
        .unwrap();

    (
        asset_id.to_string(),
        asset_row["folder_id"].as_str().unwrap().to_owned(),
    )
}

async fn admin_id(server: &TestServer) -> UserId {
    UserRepo::new(&server.db)
        .find_by_username(&Username::parse("giovanni").unwrap())
        .await
        .unwrap()
        .unwrap()
        .0
        .id
}

async fn set_home_api(server: &TestServer, lat: f64, lon: f64, radius_m: i32) {
    let response = server
        .client
        .put(server.url("/api/v1/users/me/home"))
        .json(&json!({ "lat": lat, "lon": lon, "radius_m": radius_m }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

async fn share_assets(server: &TestServer, folder_id: &str, hide_metadata: bool) -> Value {
    let token = create_share_link_from(
        server,
        json!({
            "object_type": "folder",
            "object_id": folder_id,
            "hide_metadata": hide_metadata,
        }),
    )
    .await;
    let guest = share_client(&token);
    guest
        .get(server.url(&format!("/api/v1/share/{token}/assets")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn first_asset(body: &Value) -> &Value {
    &body["assets"][0]
}

#[tokio::test]
async fn public_share_shows_location_when_no_home_is_configured() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let (_, folder_id) = seed_asset(&server, "open.jpg", HOME_LAT, HOME_LON).await;

    let body = share_assets(&server, &folder_id, false).await;
    let asset = first_asset(&body);
    assert!(asset.get("location").is_some());
    assert_eq!(asset["location"]["lat"], HOME_LAT);
    assert_eq!(asset["location"]["lon"], HOME_LON);
}

#[tokio::test]
async fn public_share_omits_location_inside_home_radius() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let (_, folder_id) = seed_asset(&server, "inside.jpg", HOME_LAT, HOME_LON).await;
    set_home_api(&server, HOME_LAT, HOME_LON, RADIUS_M).await;

    let body = share_assets(&server, &folder_id, false).await;
    let asset = first_asset(&body);
    assert!(asset.get("location").is_none());
    assert!(asset.get("place_id").is_none());
}

#[tokio::test]
async fn public_share_omits_location_on_radius_boundary() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let (asset_id, folder_id) = seed_asset(&server, "edge.jpg", HOME_LAT, HOME_LON).await;
    set_home_api(&server, HOME_LAT, HOME_LON, RADIUS_M).await;

    let admin = admin_id(&server).await;
    sqlx::query(
        "UPDATE assets SET location = ( \
             SELECT ST_Project(h.location, h.radius_m, radians(90)) \
               FROM user_home_locations h \
              WHERE h.user_id = $2 \
         ) \
         WHERE id = $1",
    )
    .bind(uuid::Uuid::parse_str(&asset_id).unwrap())
    .bind(admin.as_uuid())
    .execute(server.db.pool())
    .await
    .unwrap();

    let body = share_assets(&server, &folder_id, false).await;
    let asset = first_asset(&body);
    assert!(asset.get("location").is_none());
}

#[tokio::test]
async fn public_share_shows_location_outside_home_radius() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let (_, folder_id) = seed_asset(&server, "away.jpg", 48.0, 11.0).await;
    set_home_api(&server, HOME_LAT, HOME_LON, RADIUS_M).await;

    let body = share_assets(&server, &folder_id, false).await;
    let asset = first_asset(&body);
    assert!(asset.get("location").is_some());
}

#[tokio::test]
async fn geofence_uses_library_owner_home_not_another_user() {
    let server = TestServer::start().await;
    let admin = {
        setup_admin(&server).await;
        admin_id(&server).await
    };
    let (_, folder_id) = seed_asset(&server, "owner.jpg", HOME_LAT, HOME_LON).await;

    let mario = UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario-geofence").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(
                    &Password::parse("correct horse battery staple").unwrap(),
                )
                .unwrap()
                .as_str()
                .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap()
        .id;
    HomeRepo::new(&server.db)
        .set(
            &AuthContext::user(mario, SystemRole::User),
            GeoPoint {
                lat: HOME_LAT,
                lon: HOME_LON,
            },
            RADIUS_M,
        )
        .await
        .unwrap();

    let body = share_assets(&server, &folder_id, false).await;
    assert!(first_asset(&body).get("location").is_some());

    set_home_api(&server, HOME_LAT, HOME_LON, RADIUS_M).await;
    let body = share_assets(&server, &folder_id, false).await;
    assert!(first_asset(&body).get("location").is_none());
}

#[tokio::test]
async fn hide_metadata_strips_dates_and_coordinates() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let (_, folder_id) = seed_asset(&server, "hidden.jpg", HOME_LAT, HOME_LON).await;

    let body = share_assets(&server, &folder_id, true).await;
    let asset = first_asset(&body);
    assert!(asset.get("taken_at_utc").is_none());
    assert!(asset.get("location").is_none());
    assert!(asset.get("place_id").is_none());
}
