mod harness;

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
async fn admin_id(server: &TestServer) -> keeppix_domain::UserId {
    server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .expect("me")
        .json::<serde_json::Value>()
        .await
        .expect("json")["user"]["id"]
        .as_str()
        .expect("id")
        .parse()
        .expect("uuid")
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_plain_user_cannot_list_users() {
    let server = TestServer::start().await;
    setup_admin(&server).await;
    let admin = admin_id(&server).await;
    UserRepo::new(&server.db)
        .create(
            &AuthContext::user(admin, SystemRole::Admin),
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
    let response = mario.get(server.url("/api/v1/users")).send().await.unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_admin_creates_and_lists_users() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let created = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "mario",
            "display_name": "Mario",
            "password": "mario-password-ok",
            "role": "user"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);

    let listed = server
        .client
        .get(server.url("/api/v1/users"))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let body: serde_json::Value = listed.json().await.unwrap();
    let names: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|u| u["username"].as_str())
        .collect();
    assert!(names.contains(&"giovanni"));
    assert!(names.contains(&"mario"));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn duplicate_username_and_email_are_distinct_conflicts() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let first = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "mario",
            "email": "mario@example.com",
            "display_name": "Mario",
            "password": "mario-password-ok",
            "role": "user"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(first.status(), 201);

    let dup_user = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "mario",
            "display_name": "Other",
            "password": "mario-password-ok",
            "role": "user"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup_user.status(), 409);
    let problem: serde_json::Value = dup_user.json().await.unwrap();
    assert_eq!(problem["type"], "keeppix/conflict");
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("username"),
        "detail deve nominare username: {}",
        problem["detail"]
    );

    let dup_email = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "luigi",
            "email": "mario@example.com",
            "display_name": "Luigi",
            "password": "luigi-password-ok",
            "role": "user"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup_email.status(), 409);
    let problem: serde_json::Value = dup_email.json().await.unwrap();
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("email"),
        "detail deve nominare email: {}",
        problem["detail"]
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn disabling_a_user_terminates_their_sessions() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let created = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "mario",
            "display_name": "Mario",
            "password": "mario-password-ok",
            "role": "user"
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let me = mario
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);

    let disabled = server
        .client
        .post(server.url(&format!("/api/v1/users/{id}/disable")))
        .send()
        .await
        .unwrap();
    assert_eq!(disabled.status(), 204);

    let me_after = mario
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me_after.status(), 401);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn changing_your_password_requires_the_current_one() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let response = server
        .client
        .post(server.url("/api/v1/users/me/password"))
        .json(&json!({
            "current_password": "wrong password here!!",
            "new_password": "a brand new password ok"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn changing_your_password_revokes_other_sessions() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let other = login_as(&server, "giovanni", "correct horse battery staple").await;
    let still = other
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(still.status(), 200);

    let changed = server
        .client
        .post(server.url("/api/v1/users/me/password"))
        .json(&json!({
            "current_password": "correct horse battery staple",
            "new_password": "a brand new password ok"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(changed.status(), 204);

    // La sessione corrente resta; l'altra no.
    let me = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);

    let other_after = other
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(other_after.status(), 401);

    // Login con la nuova password.
    let fresh = login_as(&server, "giovanni", "a brand new password ok").await;
    let me2 = fresh
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me2.status(), 200);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_admin_patches_a_user_role_to_admin() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let created = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "mario",
            "display_name": "Mario",
            "password": "mario-password-ok",
            "role": "user"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 201);
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let patched = server
        .client
        .patch(server.url(&format!("/api/v1/users/{id}")))
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patched.status(), 200);
    let body: serde_json::Value = patched.json().await.unwrap();
    assert_eq!(body["role"], "admin");

    let listed = server
        .client
        .get(server.url("/api/v1/users"))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status(), 200);
    let users: Vec<serde_json::Value> = listed.json().await.unwrap();
    let mario = users
        .iter()
        .find(|u| u["username"] == "mario")
        .expect("mario listed");
    assert_eq!(mario["role"], "admin");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn enabling_a_disabled_user_lets_them_log_in_again() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let created = server
        .client
        .post(server.url("/api/v1/users"))
        .json(&json!({
            "username": "mario",
            "display_name": "Mario",
            "password": "mario-password-ok",
            "role": "user"
        }))
        .send()
        .await
        .unwrap();
    let id = created.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let disabled = server
        .client
        .post(server.url(&format!("/api/v1/users/{id}/disable")))
        .send()
        .await
        .unwrap();
    assert_eq!(disabled.status(), 204);

    let denied = server
        .client
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({ "username": "mario", "password": "mario-password-ok" }))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 401);

    let enabled = server
        .client
        .post(server.url(&format!("/api/v1/users/{id}/enable")))
        .send()
        .await
        .unwrap();
    assert_eq!(enabled.status(), 204);

    let mario = login_as(&server, "mario", "mario-password-ok").await;
    let me = mario
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me.status(), 200);
}
