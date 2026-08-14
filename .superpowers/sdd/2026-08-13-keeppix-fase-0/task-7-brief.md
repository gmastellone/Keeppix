## Task 7: Repository delle sessioni con rotazione e rilevamento del riuso

**Files:**
- Create: `crates/keeppix-db/src/sessions.rs`
- Create: `crates/keeppix-db/tests/sessions.rs`
- Modify: `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Consumes: `Db`, `DbError`, `UserRepo` (Task 4-5); `SessionToken` (Task 6).
- Produces `SessionRepo` con:
  - `SessionRepo::new(db: &Db) -> SessionRepo`
  - `create(&self, user_id: UserId, ttl: Duration, user_agent: Option<&str>) -> Result<SessionToken, DbError>` — nuova famiglia, token restituito in chiaro una sola volta.
  - `authenticate(&self, token: &SessionToken) -> Result<AuthContext, DbError>` — `DbError::NotFound` se assente, scaduto, consumato o revocato.
  - `rotate(&self, token: &SessionToken, ttl: Duration) -> Result<SessionToken, DbError>` — consuma il vecchio, emette il nuovo nella stessa famiglia. Se il token era **già consumato**, revoca l'intera famiglia e restituisce `DbError::Forbidden`.
  - `revoke(&self, token: &SessionToken) -> Result<(), DbError>` — logout della sola sessione.
  - `purge_expired(&self) -> Result<u64, DbError>` — manutenzione.

- [ ] **Step 1: Scrivere i test che falliscono**

`crates/keeppix-db/tests/sessions.rs`:

```rust
mod harness;

use std::time::Duration;

use harness::TestDb;
use keeppix_db::{DbError, SessionRepo, UserRepo};
use keeppix_domain::{NewUser, Password, SessionToken, SystemRole, Username, hash_password};

const TTL: Duration = Duration::from_secs(3600);

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
async fn an_unknown_token_is_rejected() {
    let test = TestDb::start().await;
    seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let result = repo.authenticate(&SessionToken::generate()).await;
    assert!(matches!(result, Err(DbError::NotFound)));
}

#[tokio::test]
async fn rotation_issues_a_new_token_and_retires_the_old_one() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let first = repo.create(user_id, TTL, None).await.unwrap();
    let second = repo.rotate(&first, TTL).await.unwrap();

    assert_ne!(first.as_str(), second.as_str());
    assert!(repo.authenticate(&second).await.is_ok(), "il nuovo token vale");
    assert!(matches!(repo.authenticate(&first).await, Err(DbError::NotFound)),
            "il vecchio token non vale più");
}

#[tokio::test]
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
    assert!(matches!(repo.authenticate(&second).await, Err(DbError::NotFound)));
}

#[tokio::test]
async fn revoking_logs_out_only_that_session() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let phone = repo.create(user_id, TTL, Some("phone")).await.unwrap();
    let laptop = repo.create(user_id, TTL, Some("laptop")).await.unwrap();

    repo.revoke(&phone).await.unwrap();

    assert!(matches!(repo.authenticate(&phone).await, Err(DbError::NotFound)));
    assert!(repo.authenticate(&laptop).await.is_ok(), "l'altro dispositivo resta connesso");
}

#[tokio::test]
async fn an_expired_token_is_rejected() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let token = repo.create(user_id, Duration::from_secs(0), None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(matches!(repo.authenticate(&token).await, Err(DbError::NotFound)));
}

#[tokio::test]
async fn purge_removes_expired_sessions_only() {
    let test = TestDb::start().await;
    let user_id = seed_admin(&test).await;
    let repo = SessionRepo::new(test.db());

    let _dead = repo.create(user_id, Duration::from_secs(0), None).await.unwrap();
    let alive = repo.create(user_id, TTL, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(repo.purge_expired().await.unwrap(), 1);
    assert!(repo.authenticate(&alive).await.is_ok());
}

#[tokio::test]
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

    assert!(matches!(repo.authenticate(&token).await, Err(DbError::NotFound)));
}
```

- [ ] **Step 2: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-db --test sessions`
Expected: FAIL — `unresolved import keeppix_db::SessionRepo`.

- [ ] **Step 3: Implementare `sessions.rs`**

```rust
use std::time::Duration;

use keeppix_domain::{AuthContext, SessionToken, SystemRole, UserId};
use sqlx::Row;
use uuid::Uuid;

use crate::{Db, DbError};

pub struct SessionRepo<'a> {
    db: &'a Db,
}

impl<'a> SessionRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Apre una nuova famiglia di sessione. Il token in chiaro è restituito
    /// una sola volta: nel database resta solo il digest.
    ///
    /// # Errors
    /// `DbError::Connection` se l'inserimento fallisce.
    pub async fn create(
        &self,
        user_id: UserId,
        ttl: Duration,
        user_agent: Option<&str>,
    ) -> Result<SessionToken, DbError> {
        let token = SessionToken::generate();
        let id = Uuid::now_v7();

        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, user_agent, expires_at) \
             VALUES ($1, $1, $2, $3, $4, now() + $5::interval)",
        )
        .bind(id)
        .bind(user_id.as_uuid())
        .bind(token.digest().as_slice())
        .bind(user_agent)
        .bind(interval(ttl))
        .execute(self.db.pool())
        .await?;

        Ok(token)
    }

    /// # Errors
    /// `DbError::NotFound` se il token è sconosciuto, scaduto, consumato,
    /// revocato, oppure se l'utente è disabilitato.
    pub async fn authenticate(&self, token: &SessionToken) -> Result<AuthContext, DbError> {
        let row = sqlx::query(
            "SELECT u.id AS user_id, u.role \
               FROM sessions s JOIN users u ON u.id = s.user_id \
              WHERE s.refresh_token_hash = $1 \
                AND s.consumed_at IS NULL \
                AND s.revoked_at IS NULL \
                AND s.expires_at > now() \
                AND u.disabled_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(self.db.pool())
        .await?
        .ok_or(DbError::NotFound)?;

        let user_id: Uuid = row.try_get("user_id")?;
        let role: String = row.try_get("role")?;
        let role = match role.as_str() {
            "admin" => SystemRole::Admin,
            _ => SystemRole::User,
        };

        Ok(AuthContext::user(UserId::from_uuid(user_id), role))
    }

    /// Ruota il token. Se quello presentato risulta **già consumato**, l'unica
    /// spiegazione è che una copia sia in mano a qualcun altro: si revoca
    /// l'intera famiglia e si costringe a un nuovo login.
    ///
    /// # Errors
    /// `DbError::Forbidden` in caso di riuso rilevato; `DbError::NotFound` se
    /// il token non esiste o è scaduto.
    pub async fn rotate(
        &self,
        token: &SessionToken,
        ttl: Duration,
    ) -> Result<SessionToken, DbError> {
        let mut tx = self.db.pool().begin().await?;

        let row = sqlx::query(
            "SELECT id, family_id, user_id, consumed_at, revoked_at, expires_at \
               FROM sessions WHERE refresh_token_hash = $1 FOR UPDATE",
        )
        .bind(token.digest().as_slice())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::NotFound)?;

        let family_id: Uuid = row.try_get("family_id")?;
        let consumed: Option<chrono::DateTime<chrono::Utc>> = row.try_get("consumed_at")?;
        let revoked: Option<chrono::DateTime<chrono::Utc>> = row.try_get("revoked_at")?;

        if consumed.is_some() {
            sqlx::query(
                "UPDATE sessions SET revoked_at = now() \
                  WHERE family_id = $1 AND revoked_at IS NULL",
            )
            .bind(family_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Err(DbError::Forbidden);
        }

        if revoked.is_some() {
            return Err(DbError::NotFound);
        }

        let expires_at: chrono::DateTime<chrono::Utc> = row.try_get("expires_at")?;
        if expires_at <= chrono::Utc::now() {
            return Err(DbError::NotFound);
        }

        let parent_id: Uuid = row.try_get("id")?;
        let user_id: Uuid = row.try_get("user_id")?;

        sqlx::query("UPDATE sessions SET consumed_at = now() WHERE id = $1")
            .bind(parent_id)
            .execute(&mut *tx)
            .await?;

        let next = SessionToken::generate();
        sqlx::query(
            "INSERT INTO sessions \
                 (id, family_id, user_id, refresh_token_hash, parent_id, expires_at) \
             VALUES ($1, $2, $3, $4, $5, now() + $6::interval)",
        )
        .bind(Uuid::now_v7())
        .bind(family_id)
        .bind(user_id)
        .bind(next.digest().as_slice())
        .bind(parent_id)
        .bind(interval(ttl))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(next)
    }

    /// # Errors
    /// `DbError::Connection` se l'aggiornamento fallisce.
    pub async fn revoke(&self, token: &SessionToken) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE sessions SET revoked_at = now() \
              WHERE refresh_token_hash = $1 AND revoked_at IS NULL",
        )
        .bind(token.digest().as_slice())
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// # Errors
    /// `DbError::Connection` se la cancellazione fallisce.
    pub async fn purge_expired(&self) -> Result<u64, DbError> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected())
    }
}

/// Postgres non accetta un `Duration` di Rust: si passa un intervallo in secondi.
fn interval(ttl: Duration) -> String {
    format!("{} seconds", ttl.as_secs())
}
```

- [ ] **Step 4: Esportare da `lib.rs`**

```rust
pub mod sessions;
pub use sessions::SessionRepo;
```

- [ ] **Step 5: Eseguire i test**

Run: `cargo test -p keeppix-db --test sessions`
Expected: PASS — 8 test.

- [ ] **Step 6: Verificare l'intera suite e i lint**

Run: `cargo test -p keeppix-db && cargo clippy -p keeppix-db --all-targets -- -D warnings`
Expected: 23 test verdi, nessun warning.

- [ ] **Step 7: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add session repository with rotation and reuse detection"
```

---

