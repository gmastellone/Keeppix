mod harness;

use harness::TestDb;
use keeppix_db::SettingsRepo;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn secret_is_generated_once_and_then_stable() {
    let test = TestDb::start().await;
    let repo = SettingsRepo::new(test.db());

    let first = repo.get_or_create_secret("session_key").await.unwrap();
    let second = repo.get_or_create_secret("session_key").await.unwrap();

    assert_eq!(
        first, second,
        "the secret must not change between two reads"
    );
    assert_ne!(first, [0u8; 32], "the secret must not be null");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn different_keys_get_different_secrets() {
    let test = TestDb::start().await;
    let repo = SettingsRepo::new(test.db());

    let session = repo.get_or_create_secret("session_key").await.unwrap();
    let totp = repo.get_or_create_secret("totp_key").await.unwrap();

    assert_ne!(session, totp);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn concurrent_generation_yields_a_single_secret() {
    let test = TestDb::start().await;
    let repo_a = SettingsRepo::new(test.db());
    let repo_b = SettingsRepo::new(test.db());

    let (a, b) = tokio::join!(
        repo_a.get_or_create_secret("session_key"),
        repo_b.get_or_create_secret("session_key"),
    );

    assert_eq!(
        a.unwrap(),
        b.unwrap(),
        "two concurrent startups must not diverge"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn put_json_invalidates_a_cached_setting_immediately() {
    let test = TestDb::start().await;
    let repo = SettingsRepo::new(test.db());

    repo.put_json(
        "capabilities",
        &serde_json::json!({ "backend": "software" }),
    )
    .await
    .unwrap();
    let first = repo.get_json("capabilities").await.unwrap().unwrap();
    assert_eq!(first["backend"], "software");

    sqlx::query("UPDATE system_settings SET value = $2, updated_at = now() WHERE key = $1")
        .bind("capabilities")
        .bind(serde_json::json!({ "backend": "vaapi" }))
        .execute(test.db().pool())
        .await
        .unwrap();
    let cached = repo.get_json("capabilities").await.unwrap().unwrap();
    assert_eq!(
        cached["backend"], "software",
        "bypassing the repo must leave the cached value stale until explicit invalidation"
    );

    repo.put_json("capabilities", &serde_json::json!({ "backend": "vaapi" }))
        .await
        .unwrap();
    let second = repo.get_json("capabilities").await.unwrap().unwrap();
    assert_eq!(
        second["backend"], "vaapi",
        "the settings cache must be explicitly invalidated on write"
    );
}
