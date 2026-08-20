mod harness;

use harness::TestDb;
use hmac::{Hmac, Mac};
use keeppix_db::TotpRepo;
use keeppix_domain::{AuthContext, SystemRole};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

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

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn confirm_enables_totp_and_returns_ten_recovery_codes_once() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = TotpRepo::new(test.db());

    let setup = repo.begin_setup(&ctx, "giovanni").await.unwrap();
    assert!(setup.otpauth_uri.starts_with("otpauth://totp/"));
    assert!(!setup.secret_base32.is_empty());

    let code = totp_code(&setup.secret_raw, 0);
    let confirmed = repo.confirm(&ctx, &code).await.unwrap();
    assert_eq!(confirmed.recovery_codes.len(), 10);
    assert!(repo.is_enabled(&ctx).await.unwrap());

    let status = repo.status(&ctx).await.unwrap();
    assert!(status.enabled);
    assert!(!status.pending);
    assert_eq!(status.unused_recovery_codes, 10);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn verify_accepts_adjacent_timestep_but_rejects_reuse_in_the_same_step() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = TotpRepo::new(test.db());

    let setup = repo.begin_setup(&ctx, "giovanni").await.unwrap();
    let now_step = current_step();
    let code = totp_code(&setup.secret_raw, 0);
    let _ = repo.confirm(&ctx, &code).await.unwrap();

    let prev = totp_code(&setup.secret_raw, -1);
    assert!(
        repo.verify_login(admin, &prev).await.unwrap(),
        "window ±1 must accept the previous interval"
    );

    assert!(
        !repo.verify_login(admin, &prev).await.unwrap(),
        "reusing a code in the same interval must fail"
    );

    let next = totp_code(&setup.secret_raw, 1);
    if next != prev {
        let accepted = repo.verify_login(admin, &next).await.unwrap();
        assert!(
            accepted || current_step() != now_step,
            "a distinct unused step inside ±1 must be accepted"
        );
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn recovery_codes_are_single_use_and_can_be_regenerated_when_exhausted() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = TotpRepo::new(test.db());

    let setup = repo.begin_setup(&ctx, "giovanni").await.unwrap();
    let code = totp_code(&setup.secret_raw, 0);
    let confirmed = repo.confirm(&ctx, &code).await.unwrap();
    let codes = confirmed.recovery_codes;

    assert!(repo.verify_login(admin, &codes[0]).await.unwrap());
    assert!(!repo.verify_login(admin, &codes[0]).await.unwrap());

    for c in &codes[1..] {
        assert!(repo.verify_login(admin, c).await.unwrap());
    }
    assert_eq!(repo.status(&ctx).await.unwrap().unused_recovery_codes, 0);

    let totp = totp_code(&setup.secret_raw, 1);
    let fresh = repo.regenerate_recovery_codes(&ctx, &totp).await.unwrap();
    assert_eq!(fresh.len(), 10);
    assert_eq!(repo.status(&ctx).await.unwrap().unused_recovery_codes, 10);
    assert!(repo.verify_login(admin, &fresh[0]).await.unwrap());
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn secret_is_encrypted_at_rest_with_settings_derived_key() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = TotpRepo::new(test.db());

    let setup = repo.begin_setup(&ctx, "giovanni").await.unwrap();

    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT secret_enc FROM totp_secrets WHERE user_id = $1")
            .bind(admin.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();

    assert!(
        !stored.is_empty()
            && !stored
                .windows(setup.secret_raw.len())
                .any(|w| w == setup.secret_raw.as_slice()),
        "plaintext TOTP secret must not appear in secret_enc"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn begin_setup_does_not_enable_totp_until_confirm() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = TotpRepo::new(test.db());

    let _ = repo.begin_setup(&ctx, "giovanni").await.unwrap();
    let status = repo.status(&ctx).await.unwrap();
    assert!(!status.enabled);
    assert!(status.pending);
}
