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

/// Task 9: coda di revisione — list / confirm / reject / bulk + badge revision.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn review_queue_lists_confirms_rejects_and_updates_bootstrap_revision() {
    use chrono::{TimeZone, Utc};
    use keeppix_db::{AssetRepo, FolderRepo, LibraryRepo};
    use keeppix_domain::{
        AssetKind, AssetName, AuthContext, NewAsset, NewLibrary, SystemRole, UserId,
    };
    use std::fs;

    let server = TestServer::start_with_vector().await;
    setup_admin(&server).await;
    let admin_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = 'admin'")
        .fetch_one(server.db.pool())
        .await
        .unwrap();
    let admin = UserId::from_uuid(admin_id);
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let root = std::env::temp_dir().join(format!("kpx-review-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(root.join("2024")).unwrap();
    let library = LibraryRepo::new(&server.db)
        .create(
            &ctx,
            NewLibrary {
                name: "Review".into(),
                owner_id: admin,
                root_path: root.clone(),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(&server.db)
        .ensure_path(library.id, &["2024"])
        .await
        .unwrap();
    fs::write(root.join("2024/a.jpg"), b"a").unwrap();
    fs::write(root.join("2024/b.jpg"), b"b").unwrap();
    let a = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse("a.jpg").unwrap(),
            size_bytes: 1,
            mtime: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap()
        .id;
    let b = AssetRepo::new(&server.db)
        .upsert_discovered(NewAsset {
            folder_id: folder.id,
            filename: AssetName::parse("b.jpg").unwrap(),
            size_bytes: 1,
            mtime: Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
            inode: None,
            kind: AssetKind::Image,
        })
        .await
        .unwrap()
        .unwrap()
        .id;

    let tag: serde_json::Value = server
        .client
        .post(server.url("/api/v1/tags"))
        .json(&json!({ "name": "Mare", "kind": "tag" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let tag_id = tag["id"].as_str().unwrap().to_owned();

    for (asset, score) in [(a, 0.95_f32), (b, 0.80_f32)] {
        sqlx::query(
            "INSERT INTO asset_tags (asset_id, tag_id, state, source, score) \
             VALUES ($1, $2, 'proposed', 'ai', $3)",
        )
        .bind(asset.as_uuid())
        .bind(uuid::Uuid::parse_str(&tag_id).unwrap())
        .bind(score)
        .execute(server.db.pool())
        .await
        .unwrap();
    }

    let boot: serde_json::Value = server
        .client
        .get(server.url("/api/v1/bootstrap"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(boot["badges"]["revision"], 2);

    let list: serde_json::Value = server
        .client
        .get(server.url("/api/v1/tags/proposals"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["filename"], "a.jpg");
    assert!((arr[0]["score"].as_f64().unwrap() - 0.95).abs() < 1e-5);

    let confirm = server
        .client
        .post(server.url(&format!("/api/v1/tags/{tag_id}/assets/{a}/confirm")))
        .send()
        .await
        .unwrap();
    assert_eq!(confirm.status(), 204);

    let reject_all: serde_json::Value = server
        .client
        .post(server.url(&format!("/api/v1/tags/{tag_id}/proposals/reject")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reject_all["succeeded"].as_array().unwrap().len(), 1);
    assert_eq!(reject_all["failed"].as_array().unwrap().len(), 0);

    let empty: serde_json::Value = server
        .client
        .get(server.url("/api/v1/tags/proposals"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(empty.as_array().unwrap().is_empty());

    let boot2: serde_json::Value = server
        .client
        .get(server.url("/api/v1/bootstrap"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(boot2["badges"]["revision"], 0);

    let _ = fs::remove_dir_all(&root);
}
