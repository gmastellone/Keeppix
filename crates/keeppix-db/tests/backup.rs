mod harness;

use harness::TestDb;
use keeppix_db::{
    BackupKind, BackupPreferences, BackupRepo, BackupRunStatus, NewBackupDestination,
};
use keeppix_domain::{AuthContext, SystemRole};
use serde_json::json;

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn preferences_default_without_originals_requires_warning() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let prefs = BackupRepo::new(test.db())
        .get_preferences(&ctx)
        .await
        .unwrap();
    assert!(!prefs.include_originals);
    assert!(
        prefs.requires_originals_warning(),
        "catalog-only backup must force the originals warning"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn warning_clears_only_when_originals_are_included() {
    let prefs = BackupPreferences {
        include_originals: true,
        ..BackupPreferences::default()
    };
    assert!(!prefs.requires_originals_warning());
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn destination_config_is_encrypted_at_rest() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = BackupRepo::new(test.db());

    let dest = repo
        .create_destination(
            &ctx,
            NewBackupDestination {
                kind: BackupKind::Local,
                label: "NAS".to_owned(),
                config: json!({ "path": "/backups/keeppix", "secret": "s3cr3t" }),
            },
        )
        .await
        .unwrap();

    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT config FROM backup_destinations WHERE id = $1")
            .bind(dest.id)
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    let enc = stored.get("enc").and_then(|v| v.as_str()).unwrap();
    assert!(
        !enc.contains("s3cr3t"),
        "plaintext credential must not appear in stored config"
    );
    assert_eq!(dest.config["path"], "/backups/keeppix");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn unknown_destination_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let err = BackupRepo::new(test.db())
        .find_destination(&ctx, uuid::Uuid::now_v7())
        .await
        .unwrap_err();
    assert!(matches!(err, keeppix_db::DbError::Forbidden));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn run_lifecycle_records_ok_and_verification() {
    let test = TestDb::start().await;
    let repo = BackupRepo::new(test.db());
    let run = repo.start_run(None, "0.1.0").await.unwrap();
    assert_eq!(run.status, BackupRunStatus::Running);

    let done = repo
        .complete_run(run.id, 1024, "/tmp/keeppix-test.kpxb")
        .await
        .unwrap();
    assert_eq!(done.status, BackupRunStatus::Ok);
    assert_eq!(done.size_bytes, Some(1024));

    let verified = repo.mark_verified(run.id).await.unwrap();
    assert!(verified.verified_at.is_some());
}
