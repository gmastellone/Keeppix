## Task 1: Mapping delle righe uniforme

Risolve il ruling R4, riesaminato prima di questa fase. Si adotta
`#[derive(sqlx::FromRow)]` e si convertono anche le tre struct della Fase 0,
per non lasciare due stili accanto.

**Files:**
- Create: `crates/keeppix-db/src/row.rs`
- Modify: `crates/keeppix-db/src/users.rs`, `sessions.rs`, `settings.rs`, `lib.rs`

**Interfaces:**
- Consumes: `Db`, `DbError` (Fase 0).
- Produces: la convenzione che ogni riga è una struct `#[derive(sqlx::FromRow)]` con nomi di campo uguali ai nomi di colonna, e una `into_domain(self) -> Result<T, DbError>` separata che fa la conversione al tipo di dominio. `row::corrupted(field, detail) -> DbError` come costruttore uniforme dell'errore.

- [ ] **Step 1: Verificare il verde di partenza**

Run: `cargo test -p keeppix-db -- --test-threads=1`
Expected: PASS. Annotare il numero di test — deve restare identico a fine task.

- [ ] **Step 2: Scrivere `row.rs`**

```rust
//! Convenzioni di mapping fra righe di database e tipi di dominio.
//!
//! Ogni tabella ha una struct `…Row` con `#[derive(sqlx::FromRow)]`, i cui
//! campi portano lo stesso nome delle colonne, e una `into_domain()` che
//! costruisce il tipo di dominio validando ciò che il database non può
//! garantire da solo. Le due responsabilità restano separate: `FromRow` non
//! sa nulla del dominio, `into_domain` non sa nulla di SQL.

use crate::DbError;

/// Errore uniforme per un valore memorizzato che il dominio rifiuta.
/// Usare sempre questo invece di costruire `DbError::Corrupted` a mano, così
/// i messaggi hanno la stessa forma ovunque.
pub(crate) fn corrupted(field: &str, detail: impl std::fmt::Display) -> DbError {
    DbError::Corrupted(format!("stored {field} is invalid: {detail}"))
}
```

- [ ] **Step 3: Convertire `UserRow`**

Sostituire la struct e il blocco `try_get` in `users.rs`:

```rust
#[derive(sqlx::FromRow)]
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
        let username =
            Username::parse(&self.username).map_err(|e| crate::row::corrupted("username", e))?;
        let role = match self.role.as_str() {
            "admin" => SystemRole::Admin,
            "user" => SystemRole::User,
            other => return Err(crate::row::corrupted("role", other)),
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
```

Poi sostituire ogni sito che costruiva `UserRow` a mano con `query_as`:

```rust
        let row: Option<UserRow> = sqlx::query_as(
            "SELECT id, username, email, display_name, role, locale, created_at, disabled_at \
               FROM users WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(self.db.pool())
        .await?;
```

Attenzione a `find_by_username`, che restituisce anche `PasswordHash`: la
riga porta un campo in più, quindi serve una struct dedicata.

```rust
#[derive(sqlx::FromRow)]
struct UserWithHashRow {
    #[sqlx(flatten)]
    user: UserRow,
    password_hash: String,
}
```

- [ ] **Step 4: Verificare che i test degli utenti passino invariati**

Run: `cargo test -p keeppix-db --test users -- --test-threads=1`
Expected: PASS, stesso numero di test di prima. Nessun test va modificato: il comportamento è identico, cambia solo come la riga viene letta.

- [ ] **Step 5: Convertire `sessions.rs` e `settings.rs`**

Stesso schema. In `sessions.rs` la `SELECT ... FOR UPDATE` di `rotate` legge
anche `now() AS db_now`: la struct deve includerlo.

```rust
#[derive(sqlx::FromRow)]
struct RotateRow {
    id: uuid::Uuid,
    family_id: uuid::Uuid,
    user_id: uuid::Uuid,
    consumed_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    expires_at: chrono::DateTime<chrono::Utc>,
    db_now: chrono::DateTime<chrono::Utc>,
}
```

Il ruolo sconosciuto in `sessions.rs` deve continuare a produrre
`DbError::Corrupted` — è il ruling R3, allineato durante il fix wave finale.

- [ ] **Step 6: Esportare il modulo**

In `crates/keeppix-db/src/lib.rs`, accanto agli altri: `mod row;`
(privato: è una convenzione interna, non superficie pubblica).

- [ ] **Step 7: Verificare l'intero crate e i lint**

Run: `cargo test -p keeppix-db -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
Expected: stesso numero di test dello Step 1, tutti verdi, nessun warning.

- [ ] **Step 8: Commit**

```bash
git add crates/keeppix-db
git commit -m "refactor(db): map rows with sqlx::FromRow instead of by hand"
```

---

