#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use harness::TestServer;
use serde_json::json;

async fn setup_admin(server: &TestServer) {
    let response = server
        .client
        .post(server.url("/api/v1/setup"))
        .json(&json!({
            "username": "admin",
            "display_name": "Admin",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .expect("setup");
    assert_eq!(response.status(), 201);
}

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

async fn create_plain_user(server: &TestServer) {
    let resp = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "plain",
            "display_name": "Plain User",
            "password": "correct horse battery staple",
            "role": "user"
        }))
        .send()
        .await
        .expect("create user");
    assert_eq!(resp.status(), 201);
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let server = TestServer::start_with_vector().await;
    let resp = server
        .client
        .get(server.url("/api/v1/tags"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn a_plain_user_can_create_list_get_patch_and_delete_tags() {
    let server = TestServer::start_with_vector().await;
    setup_admin(&server).await;
    create_plain_user(&server).await;
    let client = login_as(&server, "plain", "correct horse battery staple").await;

    let create = client
        .post(server.url("/api/v1/tags"))
        .json(&json!({
            "name": "Natura",
            "kind": "category"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create.status(), 201);
    let cat: serde_json::Value = create.json().await.unwrap();
    assert_eq!(cat["name"], "Natura");
    assert_eq!(cat["kind"], "category");
    assert_eq!(cat["assignment_count"], 0);
    let cat_id = cat["id"].as_str().unwrap();

    let create_tag = client
        .post(server.url("/api/v1/tags"))
        .json(&json!({
            "name": "Fauna",
            "kind": "tag",
            "parent_id": cat_id,
            "prompt": "wild animals",
            "threshold": 0.82
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_tag.status(), 201);
    let tag: serde_json::Value = create_tag.json().await.unwrap();
    let tag_id = tag["id"].as_str().unwrap().to_owned();
    assert_eq!(tag["parent_id"], cat_id);
    assert!(
        tag["has_embedding"].as_bool().unwrap(),
        "create di un tag deve calcolare l'embedding testuale quando i pesi ci sono"
    );

    let listed: serde_json::Value = client
        .get(server.url("/api/v1/tags"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 2);

    let got: serde_json::Value = client
        .get(server.url(&format!("/api/v1/tags/{tag_id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(got["name"], "Fauna");

    let patched: serde_json::Value = client
        .patch(server.url(&format!("/api/v1/tags/{tag_id}")))
        .json(&json!({ "prompt": "wildlife in the forest", "threshold": 0.7 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(patched["prompt"], "wildlife in the forest");
    assert!((patched["threshold"].as_f64().unwrap() - 0.7).abs() < 1e-6);
    assert!(patched["has_embedding"].as_bool().unwrap());

    let del = client
        .delete(server.url(&format!("/api/v1/tags/{tag_id}")))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 204);
}

#[tokio::test]
async fn unknown_tag_id_returns_forbidden_not_not_found() {
    let server = TestServer::start_with_vector().await;
    setup_admin(&server).await;
    let missing = uuid::Uuid::now_v7();
    let resp = server
        .client
        .get(server.url(&format!("/api/v1/tags/{missing}")))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["type"], "keeppix/forbidden");
}

#[tokio::test]
async fn duplicate_name_same_kind_is_conflict() {
    let server = TestServer::start_with_vector().await;
    setup_admin(&server).await;

    let first = server
        .client
        .post(server.url("/api/v1/tags"))
        .json(&json!({ "name": "Mare", "kind": "tag" }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 201);

    let second = server
        .client
        .post(server.url("/api/v1/tags"))
        .json(&json!({ "name": "Mare", "kind": "tag" }))
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 409);
}

#[tokio::test]
async fn nesting_violations_are_rejected() {
    let server = TestServer::start_with_vector().await;
    setup_admin(&server).await;

    let leaf: serde_json::Value = server
        .client
        .post(server.url("/api/v1/tags"))
        .json(&json!({ "name": "Tramonto", "kind": "tag" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let leaf_id = leaf["id"].as_str().unwrap();

    let bad_cat = server
        .client
        .post(server.url("/api/v1/tags"))
        .json(&json!({
            "name": "Bad",
            "kind": "category",
            "parent_id": leaf_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_cat.status(), 409);

    let bad_nest = server
        .client
        .post(server.url("/api/v1/tags"))
        .json(&json!({
            "name": "Nested",
            "kind": "tag",
            "parent_id": leaf_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_nest.status(), 409);
}

#[tokio::test]
async fn categories_do_not_require_a_text_embedding() {
    let server = TestServer::start_with_vector().await;
    setup_admin(&server).await;

    let cat: serde_json::Value = server
        .client
        .post(server.url("/api/v1/tags"))
        .json(&json!({ "name": "Viaggi", "kind": "category" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(cat["has_embedding"], false);
}
