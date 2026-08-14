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
        "il segreto non deve cambiare fra due letture"
    );
    assert_ne!(first, [0u8; 32], "il segreto non deve essere nullo");
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
        "due avvii concorrenti non devono divergere"
    );
}
