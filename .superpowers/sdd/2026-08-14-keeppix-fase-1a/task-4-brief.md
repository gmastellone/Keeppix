## Task 4: `LibraryRepo`

**Files:**
- Create: `crates/keeppix-db/src/libraries.rs`, `crates/keeppix-db/tests/libraries.rs`
- Modify: `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Consumes: `Db`, `DbError`, `row::corrupted`; `Library`, `NewLibrary`, `LibraryStatus`, `LibraryId`, `AuthContext`.
- Produces `LibraryRepo` con:
  - `new(db: &Db) -> LibraryRepo`
  - `create(&self, ctx: &AuthContext, new: NewLibrary) -> Result<Library, DbError>` — solo admin; `Conflict` se il `root_path` è già indicizzato.
  - `list(&self, ctx: &AuthContext) -> Result<Vec<Library>, DbError>` — un non-admin vede solo le proprie.
  - `find_by_id(&self, ctx: &AuthContext, id: LibraryId) -> Result<Library, DbError>` — `Forbidden` prima di `NotFound`, come `UserRepo::find_by_id`.
  - `set_status(&self, ctx: &AuthContext, id: LibraryId, status: LibraryStatus) -> Result<(), DbError>`
  - `mark_scanned(&self, id: LibraryId) -> Result<(), DbError>` — senza `AuthContext`: la chiama lo scanner, non un utente. Documentare l'eccezione nel doc comment come per le tre della Fase 0.

- [ ] **Step 1: Scrivere i test che falliscono**

`crates/keeppix-db/tests/libraries.rs`:

```rust
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

    let library = repo.create(&ctx, new_library("Foto", "/mnt/foto", admin)).await.unwrap();

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

    repo.create(&ctx, new_library("Foto", "/mnt/foto", admin)).await.unwrap();
    let duplicate = repo.create(&ctx, new_library("Foto bis", "/mnt/foto", admin)).await;

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

    repo.create(&admin_ctx, new_library("Admin", "/mnt/a", admin)).await.unwrap();
    repo.create(&admin_ctx, new_library("Mario", "/mnt/m", mario)).await.unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    let seen = repo.list(&mario_ctx).await.unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].name, "Mario");

    assert_eq!(repo.list(&admin_ctx).await.unwrap().len(), 2, "l'admin le vede tutte");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn reading_someone_elses_library_is_forbidden_not_not_found() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let admin_ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let mine = repo.create(&admin_ctx, new_library("Admin", "/mnt/a", admin)).await.unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    // Deve essere Forbidden, non NotFound: altrimenti sondando gli id si
    // scoprirebbe quali librerie esistono.
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

    assert!(matches!(probe, Err(DbError::Forbidden)), "nessun oracolo di esistenza");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn going_offline_never_deletes_anything() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let library = repo.create(&ctx, new_library("Foto", "/mnt/foto", admin)).await.unwrap();
    repo.set_status(&ctx, library.id, LibraryStatus::Offline).await.unwrap();

    let reloaded = repo.find_by_id(&ctx, library.id).await.unwrap();
    assert_eq!(reloaded.status, LibraryStatus::Offline);
    assert_eq!(reloaded.root_path, library.root_path, "la configurazione resta");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn mark_scanned_records_the_time() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = LibraryRepo::new(test.db());

    let library = repo.create(&ctx, new_library("Foto", "/mnt/foto", admin)).await.unwrap();
    assert!(library.last_scan_at.is_none());

    repo.mark_scanned(library.id).await.unwrap();

    assert!(repo.find_by_id(&ctx, library.id).await.unwrap().last_scan_at.is_some());
}
```

- [ ] **Step 2: Aggiungere `seed_user` all'harness**

```rust
/// Crea un utente non-admin. Serve a ogni test che verifichi i permessi.
///
/// # Panics
/// Se la creazione fallisce.
#[allow(clippy::expect_used, dead_code)]
pub async fn seed_user(
    test: &TestDb,
    admin: keeppix_domain::UserId,
    username: &str,
) -> keeppix_domain::UserId {
    use keeppix_domain::{AuthContext, NewUser, Password, SystemRole, Username, hash_password};

    let password = Password::parse("correct horse battery staple").expect("password valida");
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    keeppix_db::UserRepo::new(test.db())
        .create(
            &ctx,
            NewUser {
                username: Username::parse(username).expect("username valido"),
                email: None,
                display_name: username.to_owned(),
                password_hash: hash_password(&password).expect("hash").as_str().to_owned(),
                role: SystemRole::User,
            },
        )
        .await
        .expect("creazione utente")
        .id
}
```

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-db --test libraries -- --test-threads=1`
Expected: FAIL — `unresolved import keeppix_db::LibraryRepo`.

- [ ] **Step 4: Implementare `libraries.rs`**

```rust
use std::path::PathBuf;

use keeppix_domain::{AuthContext, Library, LibraryId, LibraryStatus, NewLibrary, UserId};

use crate::{Db, DbError};

pub struct LibraryRepo<'a> {
    db: &'a Db,
}

#[derive(sqlx::FromRow)]
struct LibraryRow {
    id: uuid::Uuid,
    name: String,
    owner_id: uuid::Uuid,
    root_path: String,
    scan_enabled: bool,
    exclude_patterns: Vec<String>,
    status: String,
    last_scan_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl LibraryRow {
    fn into_domain(self) -> Result<Library, DbError> {
        let status = match self.status.as_str() {
            "active" => LibraryStatus::Active,
            "offline" => LibraryStatus::Offline,
            other => return Err(crate::row::corrupted("library status", other)),
        };
        Ok(Library {
            id: LibraryId::from_uuid(self.id),
            name: self.name,
            owner_id: UserId::from_uuid(self.owner_id),
            root_path: PathBuf::from(self.root_path),
            scan_enabled: self.scan_enabled,
            exclude_patterns: self.exclude_patterns,
            status,
            last_scan_at: self.last_scan_at,
            created_at: self.created_at,
        })
    }
}

const fn status_str(status: LibraryStatus) -> &'static str {
    match status {
        LibraryStatus::Active => "active",
        LibraryStatus::Offline => "offline",
    }
}

const COLUMNS: &str = "id, name, owner_id, root_path, scan_enabled, exclude_patterns, \
                       status, last_scan_at, created_at";

impl<'a> LibraryRepo<'a> {
    #[must_use]
    pub const fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// # Errors
    /// `Forbidden` se il chiamante non è admin; `Conflict` se il percorso è
    /// già indicizzato da un'altra libreria.
    pub async fn create(
        &self,
        ctx: &AuthContext,
        new: NewLibrary,
    ) -> Result<Library, DbError> {
        if !ctx.is_admin() {
            return Err(DbError::Forbidden);
        }

        let row: LibraryRow = sqlx::query_as(&format!(
            "INSERT INTO libraries (id, name, owner_id, root_path, exclude_patterns) \
             VALUES ($1, $2, $3, $4, $5) RETURNING {COLUMNS}"
        ))
        .bind(LibraryId::new().as_uuid())
        .bind(&new.name)
        .bind(new.owner_id.as_uuid())
        .bind(new.root_path.to_string_lossy().as_ref())
        .bind(&new.exclude_patterns)
        .fetch_one(self.db.pool())
        .await
        .map_err(map_root_path_conflict)?;

        row.into_domain()
    }

    /// Un amministratore vede tutte le librerie, chiunque altro solo le
    /// proprie.
    ///
    /// # Errors
    /// `Connection` se la query fallisce.
    pub async fn list(&self, ctx: &AuthContext) -> Result<Vec<Library>, DbError> {
        let owner_filter = if ctx.is_admin() { None } else { ctx.user_id() };

        let rows: Vec<LibraryRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM libraries \
              WHERE $1::uuid IS NULL OR owner_id = $1 \
              ORDER BY name"
        ))
        .bind(owner_filter.map(UserId::as_uuid))
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter().map(LibraryRow::into_domain).collect()
    }

    /// # Errors
    /// `Forbidden` se la libreria non è del chiamante e non è admin — anche
    /// quando l'id non esiste, per non offrire un oracolo di esistenza.
    /// `NotFound` solo a un admin che chiede un id inesistente.
    pub async fn find_by_id(
        &self,
        ctx: &AuthContext,
        id: LibraryId,
    ) -> Result<Library, DbError> {
        let row: Option<LibraryRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM libraries WHERE id = $1"))
                .bind(id.as_uuid())
                .fetch_optional(self.db.pool())
                .await?;

        match row {
            Some(row) if ctx.is_admin() || Some(UserId::from_uuid(row.owner_id)) == ctx.user_id() => {
                row.into_domain()
            }
            Some(_) => Err(DbError::Forbidden),
            None if ctx.is_admin() => Err(DbError::NotFound),
            None => Err(DbError::Forbidden),
        }
    }

    /// # Errors
    /// `Forbidden` se il chiamante non può vedere la libreria.
    pub async fn set_status(
        &self,
        ctx: &AuthContext,
        id: LibraryId,
        status: LibraryStatus,
    ) -> Result<(), DbError> {
        // Riusa il controllo di find_by_id invece di riscriverlo.
        self.find_by_id(ctx, id).await?;

        sqlx::query("UPDATE libraries SET status = $2, updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .bind(status_str(status))
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// Registra l'istante dell'ultima scansione completata.
    ///
    /// Non prende un `AuthContext` perché la chiama lo scanner, che non
    /// agisce per conto di un utente. È la quarta e ultima eccezione alla
    /// regola, e non ne vanno aggiunte altre senza la stessa giustificazione.
    ///
    /// # Errors
    /// `Connection` se l'aggiornamento fallisce.
    pub async fn mark_scanned(&self, id: LibraryId) -> Result<(), DbError> {
        sqlx::query("UPDATE libraries SET last_scan_at = now(), updated_at = now() WHERE id = $1")
            .bind(id.as_uuid())
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}

fn map_root_path_conflict(err: sqlx::Error) -> DbError {
    if let sqlx::Error::Database(ref db_err) = err
        && db_err.code().as_deref() == Some("23505")
    {
        return DbError::Conflict("root_path is already indexed by another library".to_owned());
    }
    DbError::Connection(err)
}
```

> **Nota sul `format!` nelle query.** Interpola solo la costante `COLUMNS`,
> mai un valore che venga dall'esterno: tutti i dati passano da `bind`.
> È la stessa disciplina del resto del crate — se un giorno serve interpolare
> qualcosa di variabile, non si fa.

- [ ] **Step 5: Esportare**

```rust
pub mod libraries;
pub use libraries::LibraryRepo;
```

- [ ] **Step 6: Eseguire i test**

Run: `cargo test -p keeppix-db --test libraries -- --test-threads=1`
Expected: PASS — 8 test.

- [ ] **Step 7: Verificare l'intero workspace**

Run: `cargo test --workspace -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
Expected: tutto verde.

- [ ] **Step 8: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add library repository"
```

---

