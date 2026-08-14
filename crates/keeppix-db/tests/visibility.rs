mod harness;

use harness::TestDb;
use keeppix_db::{LibraryRepo, VisibilityScope};
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

    let admin_lib = repo
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();
    let mario_lib = repo
        .create(&admin_ctx, new_library("Mario", "/mnt/m", mario))
        .await
        .unwrap();

    let scope = VisibilityScope::resolve(test.db(), &AuthContext::user(mario, SystemRole::User))
        .await
        .unwrap();

    assert!(!scope.is_unrestricted());
    assert_eq!(scope.library_ids(), &[mario_lib.id]);
    assert!(!scope.library_ids().contains(&admin_lib.id));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_user_with_no_libraries_matches_zero_rows() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Admin", "/mnt/a", admin))
        .await
        .unwrap();

    let scope = VisibilityScope::resolve(test.db(), &AuthContext::user(mario, SystemRole::User))
        .await
        .unwrap();

    assert!(scope.library_ids().is_empty());
    assert!(!scope.is_unrestricted());

    // Il contratto congelato: le query usano la clausola, non l'elenco di id.
    // Un elenco vuoto interpolato a mano diventerebbe `IN ()`, che è un errore
    // di sintassi; la clausola deve invece restituire zero righe senza errore.
    let filter = scope.filter("id", 1);
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM libraries WHERE {}",
        filter.sql()
    ))
    .bind(filter.bind())
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
    assert!(before.library_ids().is_empty());

    let created = LibraryRepo::new(test.db())
        .create(&admin_ctx, new_library("Mario", "/mnt/m", mario))
        .await
        .unwrap();

    let after = VisibilityScope::resolve(test.db(), &mario_ctx)
        .await
        .unwrap();
    assert_eq!(after.library_ids(), &[created.id]);
}
