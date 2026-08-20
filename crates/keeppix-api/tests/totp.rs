mod harness;

use data_encoding::BASE32_NOPAD;
use harness::{TestServer, plain_client};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

#[allow(clippy::unwrap_used, clippy::expect_used)]
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

fn current_step() -> i64 {
    chrono::Utc::now().timestamp() / 30
}

#[allow(clippy::unwrap_used)]
fn totp_code(secret: &[u8], offset_steps: i64) -> String {
    let step = current_step() + offset_steps;
    let mut mac = <HmacSha1 as Mac>::new_from_slice(secret).unwrap();
    let counter = u64::try_from(step.max(0)).unwrap_or(0);
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();
    let offset = (result[19] & 0x0f) as usize;
    let bin = ((u32::from(result[offset]) & 0x7f) << 24)
        | ((u32::from(result[offset + 1]) & 0xff) << 16)
        | ((u32::from(result[offset + 2]) & 0xff) << 8)
        | (u32::from(result[offset + 3]) & 0xff);
    format!("{:06}", bin % 1_000_000)
}

#[allow(clippy::unwrap_used)]
fn secret_from_base32(encoded: &str) -> Vec<u8> {
    BASE32_NOPAD.decode(encoded.as_bytes()).unwrap()
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn setup_confirm_enables_totp_without_dropping_the_current_session() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let me_before = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me_before.status(), 200);

    let setup = server
        .client
        .post(server.url("/api/v1/auth/totp/setup"))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 200);
    let setup_body: serde_json::Value = setup.json().await.unwrap();
    assert!(
        setup_body["otpauth_uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/")
    );
    assert!(setup_body["qr_svg"].as_str().unwrap().contains("<svg"));

    let me_pending = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me_pending.status(), 200);

    let secret = secret_from_base32(setup_body["secret_base32"].as_str().unwrap());
    let code = totp_code(&secret, 0);
    let confirm = server
        .client
        .post(server.url("/api/v1/auth/totp/confirm"))
        .json(&json!({ "code": code }))
        .send()
        .await
        .unwrap();
    assert_eq!(confirm.status(), 200);
    let confirm_body: serde_json::Value = confirm.json().await.unwrap();
    assert_eq!(confirm_body["recovery_codes"].as_array().unwrap().len(), 10);

    let me_after = server
        .client
        .get(server.url("/api/v1/auth/me"))
        .send()
        .await
        .unwrap();
    assert_eq!(me_after.status(), 200);

    let status = server
        .client
        .get(server.url("/api/v1/auth/totp"))
        .send()
        .await
        .unwrap();
    assert_eq!(status.status(), 200);
    let status_body: serde_json::Value = status.json().await.unwrap();
    assert_eq!(status_body["enabled"], true);
    assert_eq!(status_body["unused_recovery_codes"], 10);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn login_requires_totp_once_enabled_and_rejects_reused_codes() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let setup = server
        .client
        .post(server.url("/api/v1/auth/totp/setup"))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), 200);
    let setup_body: serde_json::Value = setup.json().await.unwrap();
    let secret = secret_from_base32(setup_body["secret_base32"].as_str().unwrap());
    let code = totp_code(&secret, 0);
    let confirm = server
        .client
        .post(server.url("/api/v1/auth/totp/confirm"))
        .json(&json!({ "code": code }))
        .send()
        .await
        .unwrap();
    assert_eq!(confirm.status(), 200);

    let bare = plain_client();
    let need = bare
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "giovanni",
            "password": "correct horse battery staple"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(need.status(), 401);
    let need_body: serde_json::Value = need.json().await.unwrap();
    assert_eq!(need_body["type"], "keeppix/totp-required");

    // Confirm already consumed the current step; use an adjacent unused step.
    let login_code = totp_code(&secret, 1);
    let ok = bare
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "giovanni",
            "password": "correct horse battery staple",
            "totp_code": login_code
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);

    let replay = plain_client()
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "giovanni",
            "password": "correct horse battery staple",
            "totp_code": login_code
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(replay.status(), 401);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn recovery_code_can_complete_login_once() {
    let server = TestServer::start().await;
    setup_admin(&server).await;

    let setup = server
        .client
        .post(server.url("/api/v1/auth/totp/setup"))
        .send()
        .await
        .unwrap();
    let setup_body: serde_json::Value = setup.json().await.unwrap();
    let secret = secret_from_base32(setup_body["secret_base32"].as_str().unwrap());
    let code = totp_code(&secret, 0);
    let confirm = server
        .client
        .post(server.url("/api/v1/auth/totp/confirm"))
        .json(&json!({ "code": code }))
        .send()
        .await
        .unwrap();
    let recovery = confirm.json::<serde_json::Value>().await.unwrap()["recovery_codes"][0]
        .as_str()
        .unwrap()
        .to_owned();

    let login = plain_client()
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "giovanni",
            "password": "correct horse battery staple",
            "totp_code": recovery
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), 200);

    let again = plain_client()
        .post(server.url("/api/v1/auth/login"))
        .json(&json!({
            "username": "giovanni",
            "password": "correct horse battery staple",
            "totp_code": recovery
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 401);
}
