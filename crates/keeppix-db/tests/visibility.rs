mod harness;

use harness::TestDb;
use keeppix_db::{FolderRepo, LibraryRepo, VisibilityScope};
use keeppix_domain::{AuthContext, NewLibrary, SystemRole};

fn new_library(name: &str, path: &str, owner: keeppix_domain::UserId) -> NewLibrary {
    NewLibrary {
        name: name.to_owned(),
        owner_id: owner,
        root_path: std::path::PathBuf::from(path),
        exclude_patterns: vec![],
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_admin_has_unrestricted_scope() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);

    let scope = VisibilityScope::resolve(test.db(), &ctx).await.unwrap();

    assert!(scope.is_unrestricted());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_plain_user_sees_only_its_libraries() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());
    let folders = FolderRepo::new(test.db());

    let admin_lib = repo
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();
    let mario_lib = repo
        .create(&admin_ctx, new_library("Mario", "/mnt/m", mario))
        .await
        .unwrap();
    let admin_root = folders.ensure_path(admin_lib.id, &[]).await.unwrap();
    let mario_root = folders.ensure_path(mario_lib.id, &[]).await.unwrap();

    let scope = VisibilityScope::resolve(test.db(), &AuthContext::user(mario, SystemRole::User))
        .await
        .unwrap();

    assert!(!scope.is_unrestricted());
    assert!(scope.allows(mario_lib.id, mario_root.path.as_str()));
    assert!(!scope.allows(admin_lib.id, admin_root.path.as_str()));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_user_with_no_libraries_matches_zero_rows() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let admin_lib = LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();
    FolderRepo::new(test.db())
        .ensure_path(admin_lib.id, &[])
        .await
        .unwrap();

    let scope = VisibilityScope::resolve(test.db(), &AuthContext::user(mario, SystemRole::User))
        .await
        .unwrap();

    assert!(
        scope
            .filter("folders.path", "folders.library_id", 1)
            .bind()
            .is_some_and(<[uuid::Uuid]>::is_empty)
    );
    assert!(!scope.is_unrestricted());

    let filter = scope.filter("folders.path", "folders.library_id", 1);
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM folders WHERE {}",
        filter.sql()
    ))
    .bind(filter.bind())
    .bind(filter.holes())
    .fetch_one(test.db().pool())
    .await
    .expect("uno scope vuoto non è un errore");
    assert_eq!(n, 0);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn scope_updates_when_a_library_is_created() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);

    let before = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert!(
        before
            .filter("folders.path", "folders.library_id", 1)
            .bind()
            .is_some_and(<[uuid::Uuid]>::is_empty)
    );

    let created = LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Mario", "/mnt/m", mario))
        .await
        .unwrap();
    let root = FolderRepo::new(test.db())
        .ensure_path(created.id, &[])
        .await
        .unwrap();

    let after = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert_eq!(
        after
            .filter("folders.path", "folders.library_id", 1)
            .bind()
            .unwrap(),
        [root.id.as_uuid()].as_slice()
    );
}
