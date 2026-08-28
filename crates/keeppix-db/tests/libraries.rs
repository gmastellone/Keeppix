mod harness;

use harness::TestDb;
use keeppix_db::{DbError, LibraryRepo};
use keeppix_domain::{AuthContext, LibraryStatus, NewLibrary, SystemRole};

fn new_library(name: &str, path: &str, owner: keeppix_domain::UserId) -> NewLibrary {
    NewLibrary {
        name: name.to_owned(),
        owner_id: owner,
        root_path: std::path::PathBuf::from(path),
        exclude_patterns: vec!["@eaDir".to_owned()],
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_admin_creates_a_library() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let library = repo
        .create(&ctx, new_library("Foto", "/mnt/foto", admin))
        .await
        .unwrap();

    assert_eq!(library.name, "Foto");
    assert_eq!(library.root_path, std::path::PathBuf::from("/mnt/foto"));
    assert_eq!(library.status, LibraryStatus::Active);
    assert!(library.scan_enabled);
    assert_eq!(library.exclude_patterns, vec!["@eaDir".to_owned()]);
    assert!(library.last_scan_at.is_none());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_cannot_create_a_library() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let user = harness::seed_user(&test, admin, "mario").await;
    let ctx = AuthContext::user(user, SystemRole::User);

    let denied = LibraryRepo::new(test.db())
        .create(&ctx, new_library("Sue", "/mnt/sue", user))
        .await;

    assert!(matches!(denied, Err(DbError::Forbidden)));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn two_libraries_cannot_share_a_root_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    repo.create(&ctx, new_library("Foto", "/mnt/foto", admin))
        .await
        .unwrap();
    let duplicate = repo
        .create(&ctx, new_library("Foto bis", "/mnt/foto", admin))
        .await;

    assert!(matches!(duplicate, Err(DbError::Conflict(_))));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_lists_only_its_own_libraries() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    repo.create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();
    repo.create(&admin_ctx, new_library("Mario", "/mnt/m", mario))
        .await
        .unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let seen = repo.list(&mario_ctx).await.unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].name, "Mario");

    assert_eq!(
        repo.list(&admin_ctx).await.unwrap().len(),
        2,
        "l'admin le vede tutte"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn reading_someone_elses_library_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let mine = repo
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    // Must be Forbidden, not NotFound: otherwise probing ids would reveal
    // which libraries exist.
    assert!(matches!(
        repo.find_by_id(&mario_ctx, mine.id).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_an_unknown_library_id_is_also_forbidden() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let mario_ctx = AuthContext::user(mario, SystemRole::User);

    let probe = LibraryRepo::new(test.db())
        .find_by_id(&mario_ctx, keeppix_domain::LibraryId::new())
        .await;

    assert!(
        matches!(probe, Err(DbError::Forbidden)),
        "not an existence oracle"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn going_offline_never_deletes_anything() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let library = repo
        .create(&ctx, new_library("Foto", "/mnt/foto", admin))
        .await
        .unwrap();
    repo.set_status(&ctx, library.id, LibraryStatus::Offline)
        .await
        .unwrap();

    let reloaded = repo.find_by_id(&ctx, library.id).await.unwrap();
    assert_eq!(reloaded.status, LibraryStatus::Offline);
    assert_eq!(
        reloaded.root_path, library.root_path,
        "la configurazione resta"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn storage_reports_coherent_free_and_total_bytes() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let library = repo
        .create(&ctx, new_library("Disk", "/tmp", admin))
        .await
        .unwrap();

    let usage = repo.storage(&ctx, library.id).await.unwrap();
    assert!(
        usage.total_bytes > 0,
        "total must be positive on a real volume"
    );
    assert!(
        usage.free_bytes <= usage.total_bytes,
        "free ({}) cannot exceed total ({})",
        usage.free_bytes,
        usage.total_bytes
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn storage_for_someone_elses_library_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let mine = repo
        .create(&admin_ctx, new_library("Admin", "/tmp", admin))
        .await
        .unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        repo.storage(&mario_ctx, mine.id).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_an_unreachable_root_path_marks_the_library_offline() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let missing_root =
        std::env::temp_dir().join(format!("kpx-probe-missing-{}", uuid::Uuid::now_v7()));
    let library = repo
        .create(
            &ctx,
            new_library("Rete", missing_root.to_str().unwrap(), admin),
        )
        .await
        .unwrap();

    let probed = repo.probe(&ctx, library.id).await.unwrap();
    assert_eq!(probed.status, LibraryStatus::Offline);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_a_reachable_root_path_brings_an_offline_library_back_active() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());
    let root = std::env::temp_dir().join(format!("kpx-probe-ok-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();

    let library = repo
        .create(&ctx, new_library("Locale", root.to_str().unwrap(), admin))
        .await
        .unwrap();
    repo.set_status(&ctx, library.id, LibraryStatus::Offline)
        .await
        .unwrap();

    let probed = repo.probe(&ctx, library.id).await.unwrap();
    assert_eq!(probed.status, LibraryStatus::Active);

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn probing_someone_elses_library_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let mine = repo
        .create(&admin_ctx, new_library("Admin", "/tmp", admin))
        .await
        .unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(
        repo.probe(&mario_ctx, mine.id).await,
        Err(DbError::Forbidden)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn mark_scanned_records_the_time() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let library = repo
        .create(&ctx, new_library("Foto", "/mnt/foto", admin))
        .await
        .unwrap();
    assert!(library.last_scan_at.is_none());

    repo.mark_scanned(library.id).await.unwrap();

    assert!(
        repo.find_by_id(&ctx, library.id)
            .await
            .unwrap()
            .last_scan_at
            .is_some()
    );
}
