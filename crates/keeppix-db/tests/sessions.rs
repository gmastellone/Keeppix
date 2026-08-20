mod harness;

use std::time::Duration;

use harness::TestDb;
use keeppix_db::{DbError, SessionRepo, UserRepo};
use keeppix_domain::{
    AuthContext, NewUser, Password, SessionToken, SystemRole, Username, hash_password,
};

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
async fn authenticate_does_not_slide_expiry() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());
    let token = repo.create(user_id, TTL, None).await.unwrap();
    let before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM sessions WHERE refresh_token_hash = $1")
            .bind(token.digest().as_slice())
            .fetch_one(test.db().pool())
            .await
            .unwrap();

    repo.authenticate(&token).await.unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    repo.authenticate(&token).await.unwrap();

    let after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM sessions WHERE refresh_token_hash = $1")
            .bind(token.digest().as_slice())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert_eq!(
        before, after,
        "le richieste autenticate non slittano expires_at: senza /auth/refresh la sessione è assoluta"
    );
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

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn create_stores_a_short_label_and_never_the_raw_user_agent() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
              (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
    let token = repo.create(user_id, TTL, Some(ua)).await.unwrap();

    let (device_label, stored_user_agent): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT device_label, user_agent FROM sessions WHERE refresh_token_hash = $1",
    )
    .bind(token.digest().as_slice())
    .fetch_one(test.db().pool())
    .await
    .unwrap();

    assert_eq!(device_label, Some("Chrome on Windows".to_owned()));
    assert_eq!(
        stored_user_agent, None,
        "il raw User-Agent non deve mai finire nella colonna legacy"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn rotation_carries_the_device_label_forward() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let first = repo.create(user_id, TTL, Some("curl/8.7.1")).await.unwrap();
    let second = repo.rotate(&first, TTL).await.unwrap();

    let device_label: Option<String> =
        sqlx::query_scalar("SELECT device_label FROM sessions WHERE refresh_token_hash = $1")
            .bind(second.digest().as_slice())
            .fetch_one(test.db().pool())
            .await
            .unwrap();
    assert_eq!(device_label, Some("Unknown device".to_owned()));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn list_active_marks_only_the_calling_family_as_current() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());
    let ctx = AuthContext::user(user_id, SystemRole::Admin);

    let device_a = repo.create(user_id, TTL, Some("device-a")).await.unwrap();
    let device_b = repo.create(user_id, TTL, Some("device-b")).await.unwrap();

    let current = repo.family_of(&device_a).await.unwrap().unwrap();
    let sessions = repo.list_active(&ctx, current).await.unwrap();

    assert_eq!(sessions.len(), 2);
    let current_count = sessions.iter().filter(|s| s.current).count();
    assert_eq!(
        current_count, 1,
        "esattamente una sessione è quella corrente"
    );
    let current_row = sessions.iter().find(|s| s.current).unwrap();
    assert_eq!(current_row.id, current);

    // La sessione non corrente deve comunque comparire, non-corrente.
    let other = repo.family_of(&device_b).await.unwrap().unwrap();
    assert!(sessions.iter().any(|s| s.id == other && !s.current));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn list_active_excludes_revoked_and_expired_sessions() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());
    let ctx = AuthContext::user(user_id, SystemRole::Admin);

    let alive = repo.create(user_id, TTL, None).await.unwrap();
    let revoked = repo.create(user_id, TTL, None).await.unwrap();
    repo.revoke(&revoked).await.unwrap();
    let expired = repo
        .create(user_id, Duration::from_secs(0), None)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = expired;

    let current = repo.family_of(&alive).await.unwrap().unwrap();
    let sessions = repo.list_active(&ctx, current).await.unwrap();

    assert_eq!(sessions.len(), 1, "solo la sessione viva compare");
    assert_eq!(sessions[0].id, current);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn revoke_family_kills_that_device_but_not_others() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());
    let ctx = AuthContext::user(user_id, SystemRole::Admin);

    let device_a = repo.create(user_id, TTL, None).await.unwrap();
    let device_b = repo.create(user_id, TTL, None).await.unwrap();
    let family_b = repo.family_of(&device_b).await.unwrap().unwrap();

    repo.revoke_family(&ctx, family_b).await.unwrap();

    assert!(
        matches!(repo.authenticate(&device_b).await, Err(DbError::NotFound)),
        "il token della famiglia revocata non deve più autenticare"
    );
    assert!(
        repo.authenticate(&device_a).await.is_ok(),
        "le altre famiglie non devono essere toccate"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn revoke_family_of_another_user_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let owner_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let password = Password::parse("correct horse battery staple").unwrap();
    let other = UserRepo::new(test.db())
        .create(
            &AuthContext::user(owner_id, SystemRole::Admin),
            NewUser {
                username: Username::parse("mario").unwrap(),
                email: None,
                display_name: "Mario".to_owned(),
                password_hash: hash_password(&password).unwrap().as_str().to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .unwrap();

    let owners_token = repo.create(owner_id, TTL, None).await.unwrap();
    let owners_family = repo.family_of(&owners_token).await.unwrap().unwrap();

    let other_ctx = AuthContext::user(other.id, SystemRole::User);
    let result = repo.revoke_family(&other_ctx, owners_family).await;
    assert!(matches!(result, Err(DbError::Forbidden)));

    // Un id di famiglia mai esistito deve dare lo stesso esito: nessun modo
    // di distinguere "non tuo" da "non esiste" sondando l'endpoint.
    let unknown = keeppix_domain::SessionId::new();
    let result = repo.revoke_family(&other_ctx, unknown).await;
    assert!(matches!(result, Err(DbError::Forbidden)));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_disabled_user_cannot_rotate() {
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
        repo.rotate(&token, TTL).await,
        Err(DbError::NotFound)
    ));
}
