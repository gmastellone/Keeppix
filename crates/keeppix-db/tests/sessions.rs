mod harness;

use std::time::Duration;

use harness::TestDb;
use keeppix_db::{DbError, SessionRepo, UserRepo};
use keeppix_domain::{NewUser, Password, SessionToken, SystemRole, Username, hash_password};

const TTL: Duration = Duration::from_secs(3600);

#[allow(clippy::unwrap_used)]
async fn seed_admin(test: &TestDb) -> keeppix_domain::UserId {
    let password = Password::parse("correct horse battery staple").unwrap();
    let repo = UserRepo::new(test.db());
    repo.create_bootstrap_admin(NewUser {
        username: Username::parse("giovanni").unwrap(),
        email: None,
        display_name: "Giovanni".to_owned(),
        password_hash: hash_password(&password).unwrap().as_str().to_owned(),
        role: SystemRole::Admin,
    })
    .await
    .unwrap()
    .id
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_fresh_token_authenticates() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let token = repo.create(user_id, TTL, Some("test")).await.unwrap();
    let ctx = repo.authenticate(&token).await.unwrap();

    assert_eq!(ctx.user_id(), Some(user_id));
    assert!(ctx.is_admin());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_unknown_token_is_rejected() {
    let test = TestDb::start().await;
    seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let result = repo.authenticate(&SessionToken::generate()).await;
    assert!(matches!(result, Err(DbError::NotFound)));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn rotation_issues_a_new_token_and_retires_the_old_one() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let first = repo.create(user_id, TTL, None).await.unwrap();
    let second = repo.rotate(&first, TTL).await.unwrap();

    assert_ne!(first.as_str(), second.as_str());
    assert!(
        repo.authenticate(&second).await.is_ok(),
        "il nuovo token vale"
    );
    assert!(
        matches!(repo.authenticate(&first).await, Err(DbError::NotFound)),
        "il vecchio token non vale più"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn reusing_a_consumed_token_kills_the_whole_family() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let first = repo.create(user_id, TTL, None).await.unwrap();
    let second = repo.rotate(&first, TTL).await.unwrap();

    // Un attaccante ripresenta il token già consumato: è furto in corso.
    let replay = repo.rotate(&first, TTL).await;
    assert!(matches!(replay, Err(DbError::Forbidden)));

    // Anche il token legittimo viene invalidato: il legittimo proprietario
    // dovrà rifare il login, ma l'attaccante non ha accesso.
    assert!(matches!(
        repo.authenticate(&second).await,
        Err(DbError::NotFound)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn rotate_rejects_an_unknown_token() {
    let test = TestDb::start().await;
    seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let result = repo.rotate(&SessionToken::generate(), TTL).await;
    assert!(matches!(result, Err(DbError::NotFound)));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn rotate_rejects_a_revoked_token() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let token = repo.create(user_id, TTL, None).await.unwrap();
    repo.revoke(&token).await.unwrap();

    let result = repo.rotate(&token, TTL).await;
    assert!(matches!(result, Err(DbError::NotFound)));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn rotate_rejects_an_expired_token() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let token = repo
        .create(user_id, Duration::from_secs(0), None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = repo.rotate(&token, TTL).await;
    assert!(matches!(result, Err(DbError::NotFound)));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn revoking_logs_out_only_that_session() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    // parent e child appartengono alla stessa famiglia: parent -> rotate -> child.
    let parent = repo.create(user_id, TTL, None).await.unwrap();
    let child = repo.rotate(&parent, TTL).await.unwrap();

    repo.revoke(&child).await.unwrap();

    assert!(matches!(
        repo.authenticate(&child).await,
        Err(DbError::NotFound)
    ));

    let parent_revoked_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM sessions WHERE refresh_token_hash = $1")
            .bind(parent.digest().as_slice())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert!(
        parent_revoked_at.is_none(),
        "revocare il child non deve toccare il parent nella stessa famiglia"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn an_expired_token_is_rejected() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let token = repo
        .create(user_id, Duration::from_secs(0), None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(matches!(
        repo.authenticate(&token).await,
        Err(DbError::NotFound)
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn purge_removes_expired_sessions_only() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let _dead = repo
        .create(user_id, Duration::from_secs(0), None)
        .await
        .unwrap();
    let alive = repo.create(user_id, TTL, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(repo.purge_expired().await.unwrap(), 1);
    assert!(repo.authenticate(&alive).await.is_ok());
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_disabled_user_cannot_authenticate() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());
    let token = repo.create(user_id, TTL, None).await.unwrap();

    sqlx::query("UPDATE users SET disabled_at = now() WHERE id = $1")
        .bind(user_id.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    assert!(matches!(
        repo.authenticate(&token).await,
        Err(DbError::NotFound)
    ));
}
