mod harness;

use harness::TestDb;
use keeppix_db::{AppPasswordRepo, DbError};
use keeppix_domain::{AppPasswordId, AuthContext, SystemRole};

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn created_app_password_can_be_verified_with_the_returned_secret() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = AppPasswordRepo::new(test.db());

    let (summary, secret) = repo
        .create(&ctx, "MacBook Finder".to_owned())
        .await
        .unwrap();
    assert_eq!(summary.label, "MacBook Finder");
    assert_eq!(summary.user_id, admin);
    assert!(summary.last_used_at.is_none());

    let verified = repo.verify("giovanni", secret.expose()).await.unwrap();
    assert_eq!(verified, Some(admin));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn verify_rejects_the_wrong_secret() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = AppPasswordRepo::new(test.db());

    let (_summary, _secret) = repo.create(&ctx, "rclone NAS".to_owned()).await.unwrap();

    let verified = repo
        .verify("giovanni", "definitely-the-wrong-secret")
        .await
        .unwrap();
    assert_eq!(verified, None);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_revoked_app_password_fails_verification_immediately_without_any_cache() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = AppPasswordRepo::new(test.db());

    let (summary, secret) = repo.create(&ctx, "telefono".to_owned()).await.unwrap();

    // Before revocation, the secret is valid.
    assert_eq!(
        repo.verify("giovanni", secret.expose()).await.unwrap(),
        Some(admin)
    );

    repo.revoke(&ctx, summary.id).await.unwrap();

    // No cache: revocation takes effect on the very next request.
    assert_eq!(
        repo.verify("giovanni", secret.expose()).await.unwrap(),
        None
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn secret_is_never_returned_by_list() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = AppPasswordRepo::new(test.db());

    repo.create(&ctx, "MacBook Finder".to_owned())
        .await
        .unwrap();

    let listed = repo.list(&ctx).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].label, "MacBook Finder");

    // The domain type returned by `list` has no field that could carry the
    // hash or the secret: if someone added `secret_hash` to
    // `AppPasswordSummary`, this file would stop compiling — the guarantee
    // is in the type itself, not just in the endpoint that can't expose it.
    let keeppix_domain::AppPasswordSummary {
        id: _,
        user_id: _,
        label: _,
        created_at: _,
        last_used_at: _,
    } = listed.into_iter().next().unwrap();
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_revoked_app_password_is_absent_from_list() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = AppPasswordRepo::new(test.db());

    let (summary, _secret) = repo
        .create(&ctx, "vecchio laptop".to_owned())
        .await
        .unwrap();
    repo.revoke(&ctx, summary.id).await.unwrap();

    let listed = repo.list(&ctx).await.unwrap();
    assert!(listed.is_empty());
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn revoking_someone_elses_password_is_forbidden() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let repo = AppPasswordRepo::new(test.db());

    let (summary, _secret) = repo
        .create(&admin_ctx, "MacBook di Giovanni".to_owned())
        .await
        .unwrap();

    let result = repo.revoke(&mario_ctx, summary.id).await;
    assert!(matches!(result, Err(DbError::Forbidden)));

    // Never an existence oracle: a truly nonexistent id stays Forbidden for
    // a non-admin, NotFound only for an admin.
    let missing = AppPasswordId::new();
    assert!(matches!(
        repo.revoke(&mario_ctx, missing).await,
        Err(DbError::Forbidden)
    ));
    assert!(matches!(
        repo.revoke(&admin_ctx, missing).await,
        Err(DbError::NotFound)
    ));

    // The admin's password must not have been touched by the other user's
    // attempt.
    let listed = repo.list(&admin_ctx).await.unwrap();
    assert_eq!(listed.len(), 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn an_admin_can_revoke_someone_elses_password() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let repo = AppPasswordRepo::new(test.db());

    let (summary, secret) = repo
        .create(&mario_ctx, "telefono di Mario".to_owned())
        .await
        .unwrap();

    repo.revoke(&admin_ctx, summary.id).await.unwrap();

    assert_eq!(repo.verify("mario", secret.expose()).await.unwrap(), None);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn verify_does_not_touch_a_revoked_password() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = AppPasswordRepo::new(test.db());

    let (summary, secret) = repo.create(&ctx, "telefono".to_owned()).await.unwrap();
    repo.revoke(&ctx, summary.id).await.unwrap();

    assert_eq!(
        repo.verify("giovanni", secret.expose()).await.unwrap(),
        None
    );

    // An attempt against an already-revoked password must not make
    // `last_used_at` reappear: there is no fire-and-forget update to wait
    // for, because `verify` doesn't even find the row among the candidates.
    let revoked_at_is_set: bool = sqlx::query_scalar(
        "SELECT revoked_at IS NOT NULL AND last_used_at IS NULL FROM app_passwords WHERE id = $1",
    )
    .bind(summary.id.as_uuid())
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert!(
        revoked_at_is_set,
        "revoked_at must stay set and last_used_at must not be touched by a failed verify"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn verify_updates_last_used_at_in_the_background() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = AppPasswordRepo::new(test.db());

    let (summary, secret) = repo.create(&ctx, "telefono".to_owned()).await.unwrap();
    assert_eq!(
        repo.verify("giovanni", secret.expose()).await.unwrap(),
        Some(admin)
    );

    // The update is fire-and-forget: allow a short grace period before
    // checking it.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let last_used_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT last_used_at FROM app_passwords WHERE id = $1")
            .bind(summary.id.as_uuid())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert!(last_used_at.is_some());
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_share_link_cannot_create_an_app_password() {
    let test = TestDb::start().await;
    let ctx = AuthContext::share_link(
        uuid::Uuid::now_v7(),
        keeppix_domain::ShareLinkParams {
            object_type: "folder".to_owned(),
            object_id: uuid::Uuid::now_v7(),
            allow_download: true,
            allow_original: false,
            hide_metadata: true,
            allow_upload: false,
            upload_quota_bytes: None,
        },
    );
    let repo = AppPasswordRepo::new(test.db());

    let result = repo.create(&ctx, "ospite".to_owned()).await;
    assert!(matches!(result, Err(DbError::Forbidden)));
}
