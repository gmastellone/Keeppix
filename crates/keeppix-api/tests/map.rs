mod harness;

use chrono::{TimeZone as _, Utc};
use harness::TestServer;
use keeppix_db::{FolderRepo, LibraryRepo, UserRepo};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole, UserId, Username};
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn clusters_require_a_session() {
    let server = TestServer::start().await;
    let response = reqwest::Client::new()
        .get(server.url(
            "/api/v1/map/clusters?bbox=-180,-90,180,90&zoom=10&scope=library&scope_id=00000000-0000-0000-0000-000000000001",
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["type"],
        "keeppix/unauthenticated"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn clusters_return_the_scoped_map_payload() {
    let server = TestServer::start().await;
    let user = setup(&server).await;
    let library = seed_library_with_point(&server, user).await;
    let response = server
        .client
        .get(server.url(&format!(
            "/api/v1/map/clusters?bbox=10,40,15,45&zoom=15&scope=library&scope_id={library}"
        )))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let rows = body.as_array().expect("array response");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["lat"], 41.9);
    assert_eq!(rows[0]["lon"], 12.5);
    assert_eq!(rows[0]["count"], 1);
    assert_eq!(rows[0]["clustered"], false);
    assert!(rows[0]["cover_asset_id"].as_str().is_some());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn invalid_map_queries_are_problem_json_400s() {
    let server = TestServer::start().await;
    let user = setup(&server).await;
    let library = seed_library_with_point(&server, user).await;
    let cases = [
        (
            format!(
                "/api/v1/map/clusters?bbox=10,50,15,45&zoom=10&scope=library&scope_id={library}"
            ),
            "keeppix/invalid-bbox",
        ),
        (
            format!(
                "/api/v1/map/clusters?bbox=10,40,15,45&zoom=31&scope=library&scope_id={library}"
            ),
            "keeppix/invalid-zoom",
        ),
        (
            format!(
                "/api/v1/map/clusters?bbox=10,40,15,45&zoom=10&scope=timeline&scope_id={library}"
            ),
            "keeppix/invalid-map-scope",
        ),
    ];

    for (path, expected_type) in cases {
        let response = server.client.get(server.url(&path)).send().await.unwrap();
        assert_eq!(response.status(), 400, "{path}");
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["type"], expected_type, "{path}");
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_foreign_scope_id_is_forbidden_not_not_found() {
    let server = TestServer::start().await;
    setup(&server).await;
    let response = server
        .client
        .get(server.url(
            "/api/v1/map/clusters?bbox=-180,-90,180,90&zoom=10&scope=library&scope_id=00000000-0000-0000-0000-000000000001",
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["type"],
        "keeppix/forbidden"
    );
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
async fn seed_library_with_point(server: &TestServer, user: UserId) -> uuid::Uuid {
    let library = LibraryRepo::new(&server.db)
        .create(
            &AuthContext::user(user, SystemRole::Admin),
            NewLibrary {
                name: "Map".to_owned(),
                owner_id: user,
                root_path: server.photos_root.join("map"),
                exclude_patterns: Vec::new(),
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &["italy"])
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO assets \
             (id, folder_id, filename, size_bytes, mtime, kind, status, taken_at_utc, location) \
         VALUES ($1, $2, 'rome.jpg', 1, $3, 'image', 'indexed', $3, \
                 ST_SetSRID(ST_MakePoint(12.5, 41.9), 4326)::geography)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(folder.id.as_uuid())
    .bind(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap())
    .execute(server.db.pool())
    .await
    .unwrap();
    library.id.as_uuid()
}
