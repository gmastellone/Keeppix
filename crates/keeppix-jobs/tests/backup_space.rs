//! Destination free-space must be checked *before* staging work (spec §2.2 /
//! adversarial review). A failure mid-backup after pg_dump + pack wastes I/O.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod harness;

use std::fs;
use std::path::Path;

use harness::{TestDb, seed_admin};
use keeppix_db::{
    BackupKind, BackupPreferences, BackupRepo, NewBackupDestination, SettingsRepo,
};
use keeppix_domain::{AuthContext, SystemRole};
use keeppix_jobs::backup::{BackupContext, run};
use serde_json::json;

#[tokio::test]
async fn insufficient_destination_space_aborts_before_creating_staging() {
    let test = TestDb::start().await;
    let admin = seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let data_dir = tempfile::tempdir().unwrap();
    // Sentinel directory name: available_bytes returns 0 under test (see backup.rs).
    let dest_path = data_dir.path().join("keeppix-nospace");
    fs::create_dir_all(&dest_path).unwrap();

    let mut prefs = BackupPreferences::default();
    prefs.passphrase = Some("test-passphrase-not-secret".into());
    prefs.include_database = true;
    prefs.include_originals = false;
    prefs.verify_after_write = false;
    BackupRepo::new(test.db())
        .put_preferences(&ctx, &prefs)
        .await
        .expect("prefs");

    BackupRepo::new(test.db())
        .create_destination(
            &ctx,
            NewBackupDestination {
                kind: BackupKind::Local,
                label: "full-disk".into(),
                config: json!({ "path": dest_path.to_string_lossy() }),
            },
        )
        .await
        .expect("destination");

    // Touch settings so get_or_create_secret paths used by encrypt are warm.
    let _ = SettingsRepo::new(test.db())
        .get_or_create_secret("backup_key")
        .await;

    let err = run(
        test.db(),
        &BackupContext {
            database_url: test.database_url().to_owned(),
            data_dir: data_dir.path().to_path_buf(),
            config_path: None,
        },
    )
    .await
    .expect_err("backup must refuse when the destination has no free space");

    let msg = err.to_string();
    assert!(
        msg.contains("insufficient space"),
        "expected a space error, got: {msg}"
    );

    assert!(
        !has_named_prefix(data_dir.path(), "backup-staging-"),
        "staging must not be created when space check fails first"
    );
    assert!(
        !has_extension(data_dir.path(), "kpxb"),
        "no packed backup must exist when space check fails first"
    );
    assert!(
        dest_path
            .read_dir()
            .unwrap()
            .filter_map(Result::ok)
            .next()
            .is_none(),
        "destination must stay empty — upload never starts"
    );
}

fn has_named_prefix(dir: &Path, prefix: &str) -> bool {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().starts_with(prefix))
}

fn has_extension(dir: &Path, ext: &str) -> bool {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| e.path().extension().is_some_and(|x| x == ext))
}
