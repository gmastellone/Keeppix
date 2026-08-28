#![allow(clippy::unwrap_used, clippy::expect_used)]

mod harness;

use chrono::{TimeZone, Utc};
use harness::TestDb;
use keeppix_db::{
    AssetRepo, FolderRepo, JobRepo, LibraryRepo, ProblemLanguage, ProblemSeverity, ProblemsRepo,
};
use keeppix_domain::{
    AssetKind, AssetName, AuthContext, JobKind, JobPriority, LibraryStatus, NewAsset, NewLibrary,
    SystemRole, UserId,
};

async fn seed(test: &TestDb) -> (UserId, keeppix_domain::LibraryId, keeppix_domain::FolderId) {
    let admin = harness::seed_admin(test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let library = LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: admin,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .unwrap();
    let folder = FolderRepo::new(test.db())
        .ensure_path(library.id, &[])
        .await
        .unwrap();
    (admin, library.id, folder.id)
}

fn photo(folder: keeppix_domain::FolderId, name: &str, size: i64) -> NewAsset {
    NewAsset {
        folder_id: folder,
        filename: AssetName::parse(name).unwrap(),
        size_bytes: size,
        mtime: Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
        inode: Some(1),
        kind: AssetKind::Image,
    }
}

#[tokio::test]
async fn problems_lists_offline_libraries_and_error_assets() {
    let test = TestDb::start().await;
    let (admin, library, folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .set_status(&ctx, library, LibraryStatus::Offline)
        .await
        .unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(photo(folder, "broken.jpg", 10))
        .await
        .unwrap()
        .unwrap();
    AssetRepo::new(test.db())
        .set_error(asset.id, "unreadable")
        .await
        .unwrap();

    let set = ProblemsRepo::new(test.db()).list(&ctx).await.unwrap();
    assert_eq!(set.offline_libraries.len(), 1);
    assert_eq!(set.error_assets.len(), 1);
    assert_eq!(set.error_assets[0].filename, "broken.jpg");
}

#[tokio::test]
async fn failed_jobs_are_admin_only() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let user = harness::seed_user(&test, admin, "luca").await;
    let job = JobRepo::new(test.db())
        .enqueue(
            JobKind::HashAsset,
            serde_json::json!({}),
            JobPriority::Background,
            None,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status = 'failed', last_error = 'boom' WHERE id = $1")
        .bind(job.id)
        .execute(test.db().pool())
        .await
        .unwrap();

    let as_admin = ProblemsRepo::new(test.db())
        .list(&AuthContext::user(admin, SystemRole::Admin))
        .await
        .unwrap();
    assert_eq!(as_admin.failed_jobs.len(), 1);

    let as_user = ProblemsRepo::new(test.db())
        .list(&AuthContext::user(user, SystemRole::User))
        .await
        .unwrap();
    assert!(as_user.failed_jobs.is_empty());
}

/// The server-composed flat list, not `list`'s three raw buckets.
#[tokio::test]
async fn composed_flat_list_has_an_offline_library_problem_with_a_retry_action() {
    let test = TestDb::start().await;
    let (admin, library, _folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .set_status(&ctx, library, LibraryStatus::Offline)
        .await
        .unwrap();

    let problems = ProblemsRepo::new(test.db())
        .composed(&ctx, ProblemLanguage::It)
        .await
        .unwrap();

    let problem = problems
        .iter()
        .find(|p| p.library_id == Some(library))
        .expect("the offline library must appear as a problem");
    assert_eq!(problem.severity, ProblemSeverity::Error);
    assert!(problem.title.contains("Foto"), "{}", problem.title);
    assert!(problem.title.to_lowercase().contains("offline"));
    assert!(
        problem
            .actions
            .iter()
            .any(|a| a.action == "retry-connection"),
        "{:?}",
        problem.actions
    );
}

#[tokio::test]
async fn composed_flat_list_translates_the_offline_library_problem_in_english() {
    let test = TestDb::start().await;
    let (admin, library, _folder) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .set_status(&ctx, library, LibraryStatus::Offline)
        .await
        .unwrap();

    let problems = ProblemsRepo::new(test.db())
        .composed(&ctx, ProblemLanguage::En)
        .await
        .unwrap();

    let problem = problems
        .iter()
        .find(|p| p.library_id == Some(library))
        .expect("must be present");
    assert!(problem.title.to_lowercase().contains("library offline"));
    assert!(
        problem
            .actions
            .iter()
            .any(|a| a.action == "retry-connection" && a.label == "Retry connection"),
        "{:?}",
        problem.actions
    );
}

/// Reproduces the real message that lands in `jobs.last_error`: the
/// `write_sidecar` job wraps each per-asset failure in a `JobError::Worker`,
/// then wraps the whole batch in another `JobError::Worker` — two nested
/// "worker: " prefixes (see `keeppix-jobs::xmp`). The `permission-denied:`
/// marker must stay recognizable even inside this nesting.
fn realistic_permission_denied_last_error(asset_id: keeppix_domain::AssetId) -> String {
    format!("worker: {asset_id}: worker: permission-denied: io: Permission denied (os error 13)")
}

#[tokio::test]
async fn composed_flat_list_recognizes_the_unwritable_sidecar_nature() {
    let test = TestDb::start().await;
    let (admin, library, _root) = seed(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let folder = FolderRepo::new(test.db())
        .ensure_path(library, &["Chioggia"])
        .await
        .unwrap();
    let asset = AssetRepo::new(test.db())
        .upsert_discovered(photo(folder.id, "chioggia.jpg", 10))
        .await
        .unwrap()
        .unwrap();

    let job = JobRepo::new(test.db())
        .enqueue(
            JobKind::WriteSidecar,
            serde_json::json!({}),
            JobPriority::Background,
            None,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status = 'failed', last_error = $2 WHERE id = $1")
        .bind(job.id)
        .bind(realistic_permission_denied_last_error(asset.id))
        .execute(test.db().pool())
        .await
        .unwrap();

    let problems = ProblemsRepo::new(test.db())
        .composed(&ctx, ProblemLanguage::It)
        .await
        .unwrap();

    let problem = problems
        .iter()
        .find(|p| p.folder_id == Some(folder.id))
        .expect("the permission failure must become a composite problem");
    assert_eq!(problem.severity, ProblemSeverity::Warning);
    assert!(
        problem.title.to_lowercase().contains("sidecar"),
        "{}",
        problem.title
    );
    assert!(
        problem.description.contains("Chioggia"),
        "{}",
        problem.description
    );
    assert!(
        problem
            .description
            .to_lowercase()
            .contains("permessi di scrittura"),
        "{}",
        problem.description
    );
    assert!(
        !problem.actions.is_empty(),
        "must propose at least one action"
    );
}

#[tokio::test]
async fn composed_flat_list_keeps_a_generic_job_failure_that_is_not_a_permission_problem() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let job = JobRepo::new(test.db())
        .enqueue(
            JobKind::HashAsset,
            serde_json::json!({}),
            JobPriority::Background,
            None,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET status = 'failed', last_error = 'boom' WHERE id = $1")
        .bind(job.id)
        .execute(test.db().pool())
        .await
        .unwrap();

    let problems = ProblemsRepo::new(test.db())
        .composed(&ctx, ProblemLanguage::It)
        .await
        .unwrap();

    assert!(
        problems
            .iter()
            .any(|p| p.id == format!("job-failed:{}", job.id)),
        "{problems:?}"
    );
}
