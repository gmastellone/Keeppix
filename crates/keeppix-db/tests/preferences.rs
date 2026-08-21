mod harness;

use harness::TestDb;
use keeppix_db::{PreferencesRepo, UserPreferences};
use keeppix_domain::{AuthContext, NewUser, Password, SystemRole, Username, hash_password};

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
async fn preferences_default_when_never_saved() {
    let test = TestDb::start().await;
    let repo = PreferencesRepo::new(test.db());
    let admin = keeppix_db::UserRepo::new(test.db())
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);

    let prefs = repo.get(&ctx).await.unwrap();
    assert_eq!(prefs, UserPreferences::default());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn patch_one_field_does_not_clear_the_others() {
    let test = TestDb::start().await;
    let repo = PreferencesRepo::new(test.db());
    let admin = keeppix_db::UserRepo::new(test.db())
        .create_bootstrap_admin(new_user("giovanni", SystemRole::Admin))
        .await
        .unwrap();
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);

    let mut prefs = repo.get(&ctx).await.unwrap();
    prefs
        .apply_patch(&serde_json::json!({
            "theme": "scuro",
            "grid_density": { "desktop": 8, "mobile": 5 },
            "notifications": { "digest": false },
            "language": "en"
        }))
        .unwrap();
    repo.set(&ctx, &prefs).await.unwrap();
    let first = repo.get(&ctx).await.unwrap();
    assert_eq!(first.theme, "scuro");
    assert_eq!(first.grid_density.desktop, 8);
    assert_eq!(first.grid_density.mobile, 5);
    assert!(!first.notifications.digest);
    assert!(first.notifications.condivisioni);
    assert_eq!(first.language, "en");

    prefs
        .apply_patch(&serde_json::json!({ "theme": "sistema" }))
        .unwrap();
    repo.set(&ctx, &prefs).await.unwrap();
    let second = repo.get(&ctx).await.unwrap();
    assert_eq!(second.theme, "sistema");
    assert_eq!(second.grid_density.desktop, 8);
    assert_eq!(second.grid_density.mobile, 5);
    assert!(!second.notifications.digest);
    assert_eq!(second.language, "en");
}
