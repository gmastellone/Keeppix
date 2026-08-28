mod harness;

use harness::TestDb;
use keeppix_db::UserRepo;
use keeppix_domain::Password;
use keeppix_domain::{AuthContext, NewUser, SystemRole, UserId, Username, hash_password};

#[allow(clippy::unwrap_used)]
fn new_user(username: &str, role: SystemRole) -> NewUser {
    let password = Password::parse("correct horse battery staple").unwrap();
    NewUser {
        username: Username::parse(username).unwrap(),
        email: None,
        display_name: username.to_owned(),
        password_hash: hash_password(&password).unwrap().as_str().to_owned(),
        role,
    }
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn fresh_instance_has_no_users() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    assert_eq!(repo.count().await.unwrap(), 0);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn bootstrap_admin_can_be_created_once() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());

    let admin = repo
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();
    assert_eq!(admin.username.as_str(), "giovanni");
    assert!(admin.role.is_admin());
    assert_eq!(repo.count().await.unwrap(), 1);

    let second = repo
        .create_bootstrap_admin(new_user("mario", SystemRole::Admin))
        .await;
    assert!(second.is_err(), "bootstrap must be possible only once");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn login_lookup_returns_user_and_hash() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();

    let found = repo
        .find_by_username(&Username::parse("GIOVANNI").unwrap())
        .await
        .unwrap();

    let (user, hash) = found.expect("the user exists, lookup is case-insensitive");
    assert_eq!(user.username.as_str(), "giovanni");
    assert!(hash.as_str().starts_with("$argon2id$"));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn login_lookup_returns_none_for_unknown_user() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let found = repo
        .find_by_username(&Username::parse("nessuno").unwrap())
        .await
        .unwrap();
    assert!(found.is_none());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn only_admins_can_create_users() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();

    let admin_ctx = AuthContext::user(admin.id, SystemRole::Admin);
    let created = repo
        .create(&admin_ctx, new_user("mario", SystemRole::User))
        .await
        .unwrap();

    let user_ctx = AuthContext::user(created.id, SystemRole::User);
    let denied = repo
        .create(&user_ctx, new_user("luigi", SystemRole::User))
        .await;
    assert!(matches!(denied, Err(keeppix_db::DbError::Forbidden)));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn plain_user_can_only_read_itself() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();
    let admin_ctx = AuthContext::user(admin.id, SystemRole::Admin);
    let mario = repo
        .create(&admin_ctx, new_user("mario", SystemRole::User))
        .await
        .unwrap();

    let mario_ctx = AuthContext::user(mario.id, SystemRole::User);
    assert!(repo.find_by_id(&mario_ctx, mario.id).await.is_ok());
    assert!(matches!(
        repo.find_by_id(&mario_ctx, admin.id).await,
        Err(keeppix_db::DbError::Forbidden)
    ));
    assert!(repo.find_by_id(&admin_ctx, mario.id).await.is_ok());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn plain_user_probing_an_unknown_id_gets_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();
    let admin_ctx = AuthContext::user(admin.id, SystemRole::Admin);
    let mario = repo
        .create(&admin_ctx, new_user("mario", SystemRole::User))
        .await
        .unwrap();

    let mario_ctx = AuthContext::user(mario.id, SystemRole::User);
    // A non-admin probing an id that exists nowhere must get Forbidden, not
    // NotFound: otherwise the error variant itself would leak whether a
    // given id exists, turning find_by_id into an existence oracle.
    let result = repo.find_by_id(&mario_ctx, UserId::new()).await;
    assert!(matches!(result, Err(keeppix_db::DbError::Forbidden)));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn duplicate_username_is_a_conflict() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);

    let dup = repo
        .create(&ctx, new_user("giovanni", SystemRole::User))
        .await;
    assert!(matches!(dup, Err(keeppix_db::DbError::Conflict(_))));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn unknown_id_is_not_found() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);

    let missing = repo.find_by_id(&ctx, UserId::new()).await;
    assert!(matches!(missing, Err(keeppix_db::DbError::NotFound)));
}

/// The UI shows "Last changed" for the password: at creation time there
/// has been no change yet, so the initial value is the creation itself,
/// not `NULL` or an arbitrary epoch.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn password_changed_at_starts_equal_to_created_at() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();

    assert_eq!(admin.password_changed_at, admin.created_at);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn changing_the_password_hash_bumps_password_changed_at() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);
    let original = admin.password_changed_at;

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let password = Password::parse("a different password entirely").unwrap();
    repo.set_password_hash(&ctx, admin.id, hash_password(&password).unwrap().as_str())
        .await
        .unwrap();

    let reloaded = repo.find_by_id(&ctx, admin.id).await.unwrap();
    assert!(
        reloaded.password_changed_at > original,
        "changing the password must update password_changed_at"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn two_test_databases_are_isolated() {
    let a = TestDb::start().await;
    UserRepo::new(a.db())
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();
    assert_eq!(UserRepo::new(a.db()).count().await.unwrap(), 1);

    let b = TestDb::start().await;
    assert_eq!(
        UserRepo::new(b.db()).count().await.unwrap(),
        0,
        "B must be a fresh database"
    );
}
