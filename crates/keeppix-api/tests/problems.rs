mod harness;

use harness::TestServer;
use keeppix_db::LibraryRepo;
use keeppix_domain::{AuthContext, LibraryStatus, NewLibrary, SystemRole, UserId};
use serde_json::json;

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
        .expect("setup request");
    let body: serde_json::Value = response.json().await.expect("JSON body");
    body["user"]["id"]
        .as_str()
        .expect("user id")
        .parse()
        .expect("valid uuid")
}

/// `GET /problems` stays additive — the three raw buckets haven't gone
/// away — but now also carries `problems`, the composed flat list the UI
/// will use.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn the_flat_list_reports_an_offline_library_in_italian_by_default() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Braies".to_owned(),
                owner_id: admin,
                root_path: server.photos_root.join("braies"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    LibraryRepo::new(&server.db)
        .set_status(&ctx, library.id, LibraryStatus::Offline)
        .await
        .unwrap();

    let response = server
        .client
        .get(server.url("/api/v1/problems"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();

    // Additive: the raw buckets stay, so an old client doesn't break.
    assert_eq!(body["offline_libraries"].as_array().unwrap().len(), 1);

    let flat = body["problems"].as_array().unwrap();
    let problem = flat
        .iter()
        .find(|p| p["library_id"] == library.id.to_string())
        .expect("the offline library must appear in the flat list");
    assert_eq!(problem["severity"], "error");
    assert!(problem["title"].as_str().unwrap().contains("Braies"));
    assert!(
        problem["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["action"] == "retry-connection" && a["label"] == "Riprova connessione")
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn the_flat_list_is_translated_when_the_query_asks_for_english() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Braies".to_owned(),
                owner_id: admin,
                root_path: server.photos_root.join("braies-en"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    LibraryRepo::new(&server.db)
        .set_status(&ctx, library.id, LibraryStatus::Offline)
        .await
        .unwrap();

    let response = server
        .client
        .get(server.url("/api/v1/problems?lang=en"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();

    let flat = body["problems"].as_array().unwrap();
    let problem = flat
        .iter()
        .find(|p| p["library_id"] == library.id.to_string())
        .expect("must be present");
    assert!(
        problem["title"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("library offline")
    );
    assert_eq!(problem["actions"][0]["label"], "Retry connection");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn the_flat_list_is_translated_from_the_accept_language_header() {
    let server = TestServer::start().await;
    let admin = setup_admin(&server).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Braies".to_owned(),
                owner_id: admin,
                root_path: server.photos_root.join("braies-header"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    LibraryRepo::new(&server.db)
        .set_status(&ctx, library.id, LibraryStatus::Offline)
        .await
        .unwrap();

    let response = server
        .client
        .get(server.url("/api/v1/problems"))
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();

    let flat = body["problems"].as_array().unwrap();
    let problem = flat
        .iter()
        .find(|p| p["library_id"] == library.id.to_string())
        .expect("must be present");
    assert!(
        problem["title"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("library offline")
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_unauthenticated_request_is_rejected() {
    let server = TestServer::start().await;
    let response = server
        .client
        .get(server.url("/api/v1/problems"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
}
