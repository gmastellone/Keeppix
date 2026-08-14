## Task 4: Connessione al database e migrazioni

**Files:**
- Create: `crates/keeppix-db/src/error.rs`
- Create: `crates/keeppix-db/migrations/0001_users.sql`
- Create: `crates/keeppix-db/migrations/0002_sessions.sql`
- Create: `crates/keeppix-db/migrations/0003_settings.sql`
- Create: `crates/keeppix-db/tests/harness/mod.rs`
- Create: `crates/keeppix-db/tests/migrations.rs`
- Modify: `crates/keeppix-db/src/lib.rs`
- Modify: `crates/keeppix-db/Cargo.toml`

**Interfaces:**
- Consumes: nulla dai task precedenti.
- Produces:
  - `Db` — wrapper clonabile su `PgPool`, con `Db::connect(url: &str, max_connections: u32) -> Result<Db, DbError>`, `Db::migrate(&self) -> Result<(), DbError>`, `Db::pool(&self) -> &PgPool`, `Db::ping(&self) -> Result<(), DbError>`.
  - `DbError::{Connection(sqlx::Error), Migration(String), NotFound, Conflict(String)}`.
  - `tests/harness::TestDb` con `TestDb::start().await -> TestDb` (Postgres in container, migrazioni applicate) e `TestDb::db(&self) -> &Db`.

- [ ] **Step 1: Aggiungere le dipendenze**

```bash
cargo add sqlx --no-default-features \
  --features runtime-tokio,tls-rustls-ring,postgres,uuid,chrono,macros,migrate -p keeppix-db
cargo add keeppix-domain --path crates/keeppix-domain -p keeppix-db
cargo add tokio serde uuid chrono thiserror tracing -p keeppix-db
cargo add --dev testcontainers testcontainers-modules --features postgres -p keeppix-db
cargo add --dev tokio --features macros,rt-multi-thread -p keeppix-db
```

- [ ] **Step 2: Scrivere `error.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Connection(#[from] sqlx::Error),
    #[error("migration failed: {0}")]
    Migration(String),
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
}

impl From<sqlx::migrate::MigrateError> for DbError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(e.to_string())
    }
}
```

- [ ] **Step 3: Scrivere la migrazione `0001_users.sql`**

```sql
-- Estensioni richieste dallo schema completo. PostGIS arriva in Fase 4 ma
-- l'immagine è già postgis/postgis, quindi la si abilita subito per evitare
-- una migrazione che richieda privilegi elevati più avanti.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE users (
    id            uuid        PRIMARY KEY,
    username      text        NOT NULL,
    email         text,
    display_name  text        NOT NULL,
    password_hash text        NOT NULL,
    role          text        NOT NULL CHECK (role IN ('admin', 'user')),
    locale        text,
    totp_secret_enc bytea,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    disabled_at   timestamptz
);

-- Unicità case-insensitive: gli username sono già normalizzati in minuscolo
-- dal dominio, questo indice impedisce che un bug futuro crei duplicati.
CREATE UNIQUE INDEX users_username_key ON users (lower(username));
CREATE UNIQUE INDEX users_email_key ON users (lower(email)) WHERE email IS NOT NULL;

CREATE TABLE groups (
    id         uuid        PRIMARY KEY,
    name       text        NOT NULL,
    created_by uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX groups_name_key ON groups (lower(name));

CREATE TABLE group_members (
    group_id uuid        NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    user_id  uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    added_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, user_id)
);

CREATE INDEX group_members_user_idx ON group_members (user_id);
```

- [ ] **Step 4: Scrivere la migrazione `0002_sessions.sql`**

```sql
-- Una "famiglia" è la catena di refresh token nata da un singolo login.
-- Il riuso di un token già consumato indica furto: si revoca l'intera famiglia.
CREATE TABLE sessions (
    id                uuid        PRIMARY KEY,
    family_id         uuid        NOT NULL,
    user_id           uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    refresh_token_hash bytea      NOT NULL,
    parent_id         uuid        REFERENCES sessions (id) ON DELETE SET NULL,
    user_agent        text,
    ip                inet,
    created_at        timestamptz NOT NULL DEFAULT now(),
    expires_at        timestamptz NOT NULL,
    consumed_at       timestamptz,
    revoked_at        timestamptz
);

CREATE UNIQUE INDEX sessions_refresh_hash_key ON sessions (refresh_token_hash);
CREATE INDEX sessions_family_idx ON sessions (family_id);
CREATE INDEX sessions_user_idx ON sessions (user_id);
CREATE INDEX sessions_expiry_idx ON sessions (expires_at) WHERE revoked_at IS NULL;
```

- [ ] **Step 5: Scrivere la migrazione `0003_settings.sql`**

```sql
-- Impostazioni di sistema e segreti generati al primo avvio.
-- `value` è jsonb per non dover migrare lo schema a ogni nuova chiave.
CREATE TABLE system_settings (
    key        text        PRIMARY KEY,
    value      jsonb       NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
```

- [ ] **Step 6: Scrivere `lib.rs`**

```rust
//! Accesso al database. È l'unico crate del workspace che contiene SQL.

pub mod error;

pub use error::DbError;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone, Debug)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    /// # Errors
    /// `DbError::Connection` se il pool non riesce a raggiungere il database.
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    /// Applica tutte le migrazioni non ancora eseguite.
    ///
    /// # Errors
    /// `DbError::Migration` se una migrazione fallisce o è stata modificata
    /// dopo essere stata applicata.
    pub async fn migrate(&self) -> Result<(), DbError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// # Errors
    /// `DbError::Connection` se il database non risponde.
    pub async fn ping(&self) -> Result<(), DbError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 7: Scrivere il test harness**

`crates/keeppix-db/tests/harness/mod.rs`:

```rust
//! Postgres reale in container per i test di integrazione.
//! Ogni `TestDb` è isolato: container proprio, database vuoto, migrazioni applicate.

use keeppix_db::Db;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

pub struct TestDb {
    // Tenuto vivo: alla deallocazione il container viene fermato.
    _container: ContainerAsync<Postgres>,
    db: Db,
}

impl TestDb {
    /// # Panics
    /// Se Docker non è disponibile o le migrazioni falliscono: in un test è
    /// il comportamento voluto.
    pub async fn start() -> Self {
        let container = Postgres::default()
            .with_tag("17-3.5")
            .with_name("postgis/postgis")
            .start()
            .await
            .expect("avvio del container Postgres");

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .expect("porta mappata");
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

        let db = Db::connect(&url, 5).await.expect("connessione");
        db.migrate().await.expect("migrazioni");

        Self { _container: container, db }
    }

    #[must_use]
    pub const fn db(&self) -> &Db {
        &self.db
    }
}
```

- [ ] **Step 8: Scrivere il test delle migrazioni**

`crates/keeppix-db/tests/migrations.rs`:

```rust
mod harness;

use harness::TestDb;

#[tokio::test]
async fn migrations_apply_to_an_empty_database() {
    let test = TestDb::start().await;
    test.db().ping().await.expect("il database risponde");
}

#[tokio::test]
async fn migrations_are_idempotent() {
    let test = TestDb::start().await;
    // Rieseguire il migratore su un database già migrato non deve fallire.
    test.db().migrate().await.expect("seconda esecuzione");
}

#[tokio::test]
async fn expected_tables_exist() {
    let test = TestDb::start().await;

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = 'public' ORDER BY table_name",
    )
    .fetch_all(test.db().pool())
    .await
    .expect("elenco tabelle");

    for expected in ["users", "groups", "group_members", "sessions", "system_settings"] {
        assert!(tables.contains(&expected.to_owned()), "manca la tabella {expected}");
    }
}

#[tokio::test]
async fn usernames_are_unique_case_insensitively() {
    let test = TestDb::start().await;
    let pool = test.db().pool();

    let insert = "INSERT INTO users (id, username, display_name, password_hash, role) \
                  VALUES ($1, $2, 'X', 'hash', 'user')";

    sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("giovanni")
        .execute(pool)
        .await
        .expect("primo inserimento");

    let second = sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("GIOVANNI")
        .execute(pool)
        .await;

    assert!(second.is_err(), "l'indice unico deve rifiutare il duplicato");
}
```

- [ ] **Step 9: Aggiungere `uuid` alle dev-dependencies**

```bash
cargo add --dev uuid --features v7 -p keeppix-db
```

- [ ] **Step 10: Eseguire i test e verificare che passino**

Assicurarsi che Docker sia in esecuzione, poi:

Run: `cargo test -p keeppix-db`
Expected: PASS — 4 test. Il primo avvio scarica l'immagine `postgis/postgis:17-3.5` (~400 MB) e richiede qualche minuto.

- [ ] **Step 11: Generare la cache offline di sqlx**

Le query verificate a compile-time richiedono un database raggiungibile in fase di build, oppure una cache committata. Si usa la cache, così CI e Docker build non hanno bisogno di Postgres.

```bash
cargo install sqlx-cli --no-default-features --features rustls,postgres
docker run -d --name keeppix-sqlx -e POSTGRES_PASSWORD=postgres -p 55432:5432 postgis/postgis:17-3.5
sleep 5
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/postgres
cargo sqlx migrate run --source crates/keeppix-db/migrations
cargo sqlx prepare --workspace -- --all-targets
docker rm -f keeppix-sqlx
```

Aggiungere a `.gitignore` **nulla**: la cartella `.sqlx/` va committata.

- [ ] **Step 12: Verificare che la build funzioni offline**

Run: `SQLX_OFFLINE=true cargo build --workspace`
Expected: build OK senza `DATABASE_URL`.

- [ ] **Step 13: Commit**

```bash
git add crates/keeppix-db .sqlx
git commit -m "feat(db): add connection pool, migrations and test harness"
```

---

