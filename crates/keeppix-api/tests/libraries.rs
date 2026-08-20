mod harness;

use std::fs;

use harness::TestServer;
use keeppix_db::UserRepo;
use keeppix_domain::{AuthContext, NewUser, Password, SystemRole, Username, hash_password};
use serde_json::json;

#[allow(clippy::expect_used)]
async fn setup_admin(server: &TestServer) {
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
    assert_eq!(response.status(), 201);
}

#[allow(clippy::expect_used)]
async fn login_as(server: &TestServer, username: &str, password: &str) -> reqwest::Client {
    let client = reqwest::Client::builder()
        .cookie_store(true)
        .default_headers(harness::client_headers())
        .build()
        .expect("client");
    let response = client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": username, "password": password }))
        .send()
        .await
        .expect("login");
    assert_eq!(response.status(), 200);
    client
}

#[allow(clippy::expect_used)]
fn library_dir(server: &TestServer, name: &str) -> std::path::PathBuf {
    let path = server.photos_root.join(name);
    fs::create_dir_all(&path).expect("library dir");
    path
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_admin_creates_a_library_and_sees_it_listed() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "foto");

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Foto",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let body: serde_json::Value = created.json().await.unwrap();
    assert_eq!(body["name"], "Foto");
    let id = body["id"].as_str().unwrap().to_owned();

    let listed = server
        .client
        .get(server.url("/api/v1/libraries"))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let list: serde_json::Value = listed.json().await.unwrap();
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["id"].as_str())
        .collect();
    assert!(ids.contains(&id.as_str()));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_plain_user_cannot_create_a_library() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let admin_id = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin_id, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(&Password::parse("mario-password-ok").unwrap())
                    .unwrap()
                    .as_str()
                    .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let root = library_dir(&server, "mario-lib");
    let response = mario
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Sue",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_plain_user_lists_only_its_own_libraries() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let admin_id = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let mario = UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin_id, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(&Password::parse("mario-password-ok").unwrap())
                    .unwrap()
                    .as_str()
                    .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let admin_root = library_dir(&server, "admin-lib");
    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "AdminLib",
            "root_path": admin_root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);

    // Libreria di Mario via repository (owner diverso dall'admin).
    keeppix_db::LibraryRepo::new(&server.db)
        .create(
            &AuthContext::user(admin_id, SystemRole::Admin),
            keeppix_domain::NewLibrary {
                name: "MarioLib".to_owned(),
                owner_id: mario.id,
                root_path: library_dir(&server, "mario-only"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();

    let mario_client = login_as(&server, "mario", "mario-password-ok").await;
    let listed = mario_client
        .get(server.url("/api/v1/libraries"))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let list: serde_json::Value = listed.json().await.unwrap();
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["name"].as_str())
        .collect();
    assert_eq!(names, vec!["MarioLib"]);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_someone_elses_library_is_forbidden_not_not_found() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let admin_id = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin_id, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(&Password::parse("mario-password-ok").unwrap())
                    .unwrap()
                    .as_str()
                    .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let root = library_dir(&server, "admin-lib");
    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "AdminLib",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let response = mario
        .get(server.url(&format!("/api/v1/libraries/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/forbidden");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_a_nonexistent_library_id_is_also_forbidden() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let admin_id = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin_id, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(&Password::parse("mario-password-ok").unwrap())
                    .unwrap()
                    .as_str()
                    .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let missing = uuid::Uuid::now_v7();
    let response = mario
        .get(server.url(&format!("/api/v1/libraries/{missing}")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/forbidden");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_path_outside_the_allowed_roots_is_rejected() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Etc",
            "root_path": "/etc",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 422);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/path-not-allowed");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_path_that_escapes_via_dotdot_is_rejected() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    // Fuori dall'allowlist, ma raggiungibile come `{photos}/../outside`.
    let outside = server.data_dir.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let escaped = server.photos_root.join("..").join("outside");

    let response = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Escape",
            "root_path": escaped.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 422);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/path-not-allowed");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn two_libraries_cannot_share_a_root_path() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "shared");

    let first = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "One",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 201);

    let second = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Two",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 409);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn storage_returns_coherent_bytes_for_a_visible_library() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "storage-lib");

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Storage",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let response = server
        .client
        .get(server.url(&format!("/api/v1/libraries/{id}/storage")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let free = body["free_bytes"].as_u64().unwrap();
    let total = body["total_bytes"].as_u64().unwrap();
    assert!(total > 0);
    assert!(free <= total);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_storage_for_someone_elses_library_is_forbidden() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let admin_id = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin_id, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(&Password::parse("mario-password-ok").unwrap())
                    .unwrap()
                    .as_str()
                    .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let root = library_dir(&server, "admin-storage");
    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "AdminLib",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let response = mario
        .get(server.url(&format!("/api/v1/libraries/{id}/storage")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/forbidden");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn deleting_a_library_leaves_the_files_untouched() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "keep-files");
    let marker = root.join("keep-me.txt");
    fs::write(&marker, b"stay").unwrap();
    let before = fs::read_dir(&root).unwrap().count();

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Keep",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let deleted = server
        .client
        .delete(server.url(&format!("/api/v1/libraries/{id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), 200);
    let body: serde_json::Value = deleted.json().await.unwrap();
    assert_eq!(body["files_untouched"], true);

    assert!(marker.is_file());
    let after = fs::read_dir(&root).unwrap().count();
    assert_eq!(before, after);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_a_library_whose_root_path_vanished_marks_it_offline() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "vanishing");

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "Vanishing",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    fs::remove_dir_all(&root).unwrap();

    let response = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/probe")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "offline");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_a_library_whose_root_path_came_back_marks_it_active_again() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let root = library_dir(&server, "coming-back");

    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "ComingBack",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    fs::remove_dir_all(&root).unwrap();
    server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/probe")))
        .send()
        .await
        .unwrap();

    fs::create_dir_all(&root).unwrap();
    let response = server
        .client
        .post(server.url(&format!("/api/v1/libraries/{id}/probe")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "active");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn probing_someone_elses_library_via_the_probe_endpoint_is_forbidden() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let admin_id = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin_id, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(&Password::parse("mario-password-ok").unwrap())
                    .unwrap()
                    .as_str()
                    .to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let root = library_dir(&server, "probe-forbidden");
    let created = server
        .client
        .post(server.url("/api/v1/libraries"))
        .json(&json!({
            "name": "ProbeForbidden",
            "root_path": root.to_string_lossy(),
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let response = mario
        .post(server.url(&format!("/api/v1/libraries/{id}/probe")))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let problem: serde_json::Value = response.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/forbidden");
}
