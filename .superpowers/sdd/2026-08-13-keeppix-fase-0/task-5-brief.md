## Task 5: Repository degli utenti

**Files:**
- Create: `crates/keeppix-db/src/users.rs`
- Create: `crates/keeppix-db/tests/users.rs`
- Modify: `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Consumes: `Db`, `DbError` (Task 4); `User`, `NewUser`, `Username`, `SystemRole`, `UserId`, `PasswordHash`, `AuthContext` (Task 2-3).
- Produces `UserRepo` con:
  - `UserRepo::new(db: &Db) -> UserRepo`
  - `count(&self) -> Result<i64, DbError>` — senza `AuthContext`: serve a decidere se l'istanza è vergine, prima che esista un utente.
  - `create_bootstrap_admin(&self, new: NewUser) -> Result<User, DbError>` — fallisce con `DbError::Conflict` se esistono già utenti. Unica scrittura senza `AuthContext`, e il nome lo dichiara.
  - `create(&self, ctx: &AuthContext, new: NewUser) -> Result<User, DbError>` — richiede admin.
  - `find_by_username(&self, username: &Username) -> Result<Option<(User, PasswordHash)>, DbError>` — senza `AuthContext` perché è il login stesso; restituisce l'hash per la verifica.
  - `find_by_id(&self, ctx: &AuthContext, id: UserId) -> Result<User, DbError>` — un utente non-admin può leggere solo sé stesso.

- [ ] **Step 1: Scrivere i test che falliscono**

`crates/keeppix-db/tests/users.rs`:

```rust
mod harness;

use harness::TestDb;
use keeppix_db::UserRepo;
use keeppix_domain::{AuthContext, NewUser, SystemRole, UserId, Username, hash_password};
use keeppix_domain::Password;

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
async fn fresh_instance_has_no_users() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    assert_eq!(repo.count().await.unwrap(), 0);
}

#[tokio::test]
async fn bootstrap_admin_can_be_created_once() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());

    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();
    assert_eq!(admin.username.as_str(), "giovanni");
    assert!(admin.role.is_admin());
    assert_eq!(repo.count().await.unwrap(), 1);

    let second = repo.create_bootstrap_admin(new_user("mario", SystemRole::Admin)).await;
    assert!(second.is_err(), "il bootstrap deve essere possibile una sola volta");
}

#[tokio::test]
async fn login_lookup_returns_user_and_hash() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();

    let found = repo
        .find_by_username(&Username::parse("GIOVANNI").unwrap())
        .await
        .unwrap();

    let (user, hash) = found.expect("l'utente esiste, la ricerca è case-insensitive");
    assert_eq!(user.username.as_str(), "giovanni");
    assert!(hash.as_str().starts_with("$argon2id$"));
}

#[tokio::test]
async fn login_lookup_returns_none_for_unknown_user() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let found = repo.find_by_username(&Username::parse("nessuno").unwrap()).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn only_admins_can_create_users() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();

    let admin_ctx = AuthContext::user(admin.id, SystemRole::Admin);
    let created = repo.create(&admin_ctx, new_user("mario", SystemRole::User)).await.unwrap();

    let user_ctx = AuthContext::user(created.id, SystemRole::User);
    let denied = repo.create(&user_ctx, new_user("luigi", SystemRole::User)).await;
    assert!(denied.is_err(), "un utente non-admin non può creare utenti");
}

#[tokio::test]
async fn plain_user_can_only_read_itself() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();
    let admin_ctx = AuthContext::user(admin.id, SystemRole::Admin);
    let mario = repo.create(&admin_ctx, new_user("mario", SystemRole::User)).await.unwrap();

    let mario_ctx = AuthContext::user(mario.id, SystemRole::User);
    assert!(repo.find_by_id(&mario_ctx, mario.id).await.is_ok());
    assert!(repo.find_by_id(&mario_ctx, admin.id).await.is_err());
    assert!(repo.find_by_id(&admin_ctx, mario.id).await.is_ok());
}

#[tokio::test]
async fn duplicate_username_is_a_conflict() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);

    let dup = repo.create(&ctx, new_user("giovanni", SystemRole::User)).await;
    assert!(matches!(dup, Err(keeppix_db::DbError::Conflict(_))));
}

#[tokio::test]
async fn unknown_id_is_not_found() {
    let test = TestDb::start().await;
    let repo = UserRepo::new(test.db());
    let admin = repo.create_bootstrap_admin(new_user("giovanni", SystemRole::Admin)).await.unwrap();
    let ctx = AuthContext::user(admin.id, SystemRole::Admin);

    let missing = repo.find_by_id(&ctx, UserId::new()).await;
    assert!(matches!(missing, Err(keeppix_db::DbError::NotFound)));
}
```

- [ ] **Step 2: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-db --test users`
Expected: FAIL — `unresolved import keeppix_db::UserRepo`.

- [ ] **Step 3: Aggiungere `DbError::Forbidden`**

In `crates/keeppix-db/src/error.rs`, dentro l'enum:

```rust
    #[error("forbidden")]
    Forbidden,
```

- [ ] **Step 4: Implementare `users.rs`**

```rust
use keeppix_domain::{
    AuthContext, NewUser, PasswordHash, SystemRole, User, UserId, Username,
};
use sqlx::Row;

use crate::{Db, DbError};

pub struct UserRepo<'a> {
    db: &'a Db,
}

/// Riga grezza della tabella `users`, convertita in `User` dal dominio.
struct UserRow {
    id: uuid::Uuid,
    username: String,
    email: Option<String>,
    display_name: String,
    role: String,
    locale: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    disabled_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl UserRow {
    fn into_domain(self) -> Result<User, DbError> {
        let username = Username::parse(&self.username)
            .map_err(|e| DbError::Migration(format!("stored username is invalid: {e}")))?;
        let role = match self.role.as_str() {
            "admin" => SystemRole::Admin,
            "user" => SystemRole::User,
            other => return Err(DbError::Migration(format!("unknown role: {other}"))),
        };
        Ok(User {
            id: UserId::from_uuid(self.id),
            username,
            email: self.email,
            display_name: self.display_name,
            role,
            locale: self.locale,
            created_at: self.created_at,
            disabled_at: self.disabled_at,
        })
    }
}

const fn role_str(role: SystemRole) -> &'static str {
    match role {
        SystemRole::Admin => "admin",
        SystemRole::User => "user",
    }
}

/// Traduce la violazione dell'indice unico in un conflitto leggibile.
fn map_unique_violation(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("username or email already in use".to_owned());
    }
    DbError::Connection(err)
}

impl<'a> UserRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Numero totale di utenti. Non richiede `AuthContext` perché serve a
    /// stabilire se l'istanza è ancora vergine, cioè prima che un contesto
    /// possa esistere.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn count(&self) -> Result<i64, DbError> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(self.db.pool())
            .await?;
        Ok(n)
    }

    /// Crea il primo amministratore. Unica scrittura priva di `AuthContext`,
    /// permessa solo finché la tabella è vuota.
    ///
    /// # Errors
    /// `DbError::Conflict` se esistono già utenti.
    pub async fn create_bootstrap_admin(&self, new: NewUser) -> Result<User, DbError> {
        let mut tx = self.db.pool().begin().await?;

        // Blocca la tabella per la durata della transazione: due richieste di
        // setup concorrenti non possono creare due amministratori.
        sqlx::query("LOCK TABLE users IN EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await?;

        let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&mut *tx)
            .await?;
        if existing > 0 {
            return Err(DbError::Conflict("instance is already initialised".to_owned()));
        }

        let row = insert_user(&mut tx, &new).await?;
        tx.commit().await?;
        row.into_domain()
    }

    /// # Errors
    /// `DbError::Forbidden` se il chiamante non è admin; `DbError::Conflict`
    /// se username o email sono già in uso.
    pub async fn create(&self, ctx: &AuthContext, new: NewUser) -> Result<User, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }
        let mut tx = self.db.pool().begin().await?;
        let row = insert_user(&mut tx, &new).await?;
        tx.commit().await?;
        row.into_domain()
    }

    /// Ricerca per il login: restituisce anche l'hash della password.
    /// Non richiede `AuthContext` perché è il passo che lo produce.
    ///
    /// # Errors
    /// `DbError::Connection` se la query fallisce.
    pub async fn find_by_username(
        &self,
        username: &Username,
    ) -> Result<Option<(User, PasswordHash)>, DbError> {
        let row = sqlx::query(
            "SELECT id, username, email, display_name, password_hash, role, locale, \
                    created_at, disabled_at \
               FROM users WHERE lower(username) = lower($1)",
        )
        .bind(username.as_str())
        .fetch_optional(self.db.pool())
        .await?;

        let Some(row) = row else { return Ok(None) };

        let hash = PasswordHash::from_stored(row.try_get("password_hash")?);
        let user = UserRow {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            email: row.try_get("email")?,
            display_name: row.try_get("display_name")?,
            role: row.try_get("role")?,
            locale: row.try_get("locale")?,
            created_at: row.try_get("created_at")?,
            disabled_at: row.try_get("disabled_at")?,
        }
        .into_domain()?;

        Ok(Some((user, hash)))
    }

    /// # Errors
    /// `DbError::Forbidden` se un utente non-admin chiede un id diverso dal
    /// proprio; `DbError::NotFound` se l'utente non esiste.
    pub async fn find_by_id(&self, ctx: &AuthContext, id: UserId) -> Result<User, DbError> {
        if !ctx.is_admin() && ctx.user_id() != Some(id) {
            return Err(DbError::Forbidden);
        }

        let row = sqlx::query(
            "SELECT id, username, email, display_name, role, locale, created_at, disabled_at \
               FROM users WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?
        .ok_or(DbError::NotFound)?;

        UserRow {
            id: row.try_get("id")?,
            username: row.try_get("username")?,
            email: row.try_get("email")?,
            display_name: row.try_get("display_name")?,
            role: row.try_get("role")?,
            locale: row.try_get("locale")?,
            created_at: row.try_get("created_at")?,
            disabled_at: row.try_get("disabled_at")?,
        }
        .into_domain()
    }
}

async fn insert_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    new: &NewUser,
) -> Result<UserRow, DbError> {
    let row = sqlx::query(
        "INSERT INTO users (id, username, email, display_name, password_hash, role) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING id, username, email, display_name, role, locale, created_at, disabled_at",
    )
    .bind(UserId::new().as_uuid())
    .bind(new.username.as_str())
    .bind(new.email.as_deref())
    .bind(&new.display_name)
    .bind(&new.password_hash)
    .bind(role_str(new.role))
    .fetch_one(&mut **tx)
    .await
    .map_err(map_unique_violation)?;

    Ok(UserRow {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        email: row.try_get("email")?,
        display_name: row.try_get("display_name")?,
        role: row.try_get("role")?,
        locale: row.try_get("locale")?,
        created_at: row.try_get("created_at")?,
        disabled_at: row.try_get("disabled_at")?,
    })
}
```

- [ ] **Step 5: Esportare da `lib.rs`**

```rust
pub mod users;
pub use users::UserRepo;
```

- [ ] **Step 6: Eseguire i test**

Run: `cargo test -p keeppix-db --test users`
Expected: PASS — 8 test.

- [ ] **Step 7: Verificare che l'intera suite sia ancora verde e senza warning**

Run: `cargo test -p keeppix-db && cargo clippy -p keeppix-db --all-targets -- -D warnings`
Expected: 12 test verdi, nessun warning.

- [ ] **Step 8: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add user repository with auth-context enforcement"
```

---

