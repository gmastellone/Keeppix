# Keeppix Fase 1a — Fondamenta dei dati

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Costruire il modello dati dell'ingestione — librerie, albero delle cartelle, asset, EXIF, registro delle modifiche — con i repository che lo interrogano applicando i permessi, senza ancora indicizzare un solo file.

**Architecture:** Tre migrazioni che estendono lo schema della Fase 0. L'albero delle cartelle usa `ltree` con etichette numeriche, così "tutto ciò che sta sotto" è una singola condizione indicizzata. L'identità dell'asset è il percorso (`folder_id` + `filename`), con `content_hash` indicizzato ma non unico. Una sola funzione costruisce il filtro di visibilità e ogni repository la attraversa. Il mapping riga→struct passa a `sqlx::FromRow`, comprese le tre struct della Fase 0.

**Tech Stack:** Rust 1.88 (edition 2024) · sqlx 0.8 con `ltree` via `PgLTree` · PostgreSQL 17 + PostGIS · testcontainers

**Spec:** [`../specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md) — §4 Modello dati
**Roadmap:** [`2026-08-13-keeppix-roadmap.md`](2026-08-13-keeppix-roadmap.md) — Fase 1
**Stato Fase 0:** [`2026-08-13-keeppix-fase-0-STATO.md`](2026-08-13-keeppix-fase-0-STATO.md) — leggerlo prima di iniziare

## Perché la Fase 1 è divisa in tre

La roadmap prevede di spezzare la Fase 1 se supera i 20 task, e li supera
ampiamente. La divisione scelta segue i confini naturali del lavoro, non
una quota:

| Piano | Contenuto | Chiuso quando |
|---|---|---|
| **1a** (questo) | migrazioni, repository, visibilità | si creano librerie, cartelle e asset via repository e si interrogano coi permessi applicati |
| **1b** | coda job, worker, profili energetici, `keeppix-media`, discovery, hash, derivati, watcher | si punta Keeppix a una cartella reale e la si trova indicizzata con le miniature su disco |
| **1c** | `TimelineRepo`, endpoint, WebSocket, griglia, scrubber, ricerca | si naviga il TB reale dal browser |

Ognuno produce software funzionante e verificabile da solo. 1a non ha
interfaccia e non tocca un file su disco: è deliberato — il modello dati va
sbagliato adesso, quando costa una migrazione e non 200.000 righe.

**Non pianificare 1b e 1c adesso.** I numeri veri di throughput e la
copertura reale delle preview incorporate escono da 1b, e devono informare
1c. Un piano di dettaglio scritto prima del codice su cui poggia è finzione
plausibile.

## Global Constraints

Valgono per **ogni** task. Molti sono contratti congelati dalla roadmap:
implementarli anche quando sembrano prematuri è ciò che impedisce di
smontare il lavoro alla fase successiva.

- **Rust edition 2024, toolchain 1.88.0.** Let-chains disponibili.
- **`keeppix-db` è l'unico crate con SQL.** Gli handler non scrivono query.
- **Ogni metodo di repository che legge dati di un utente prende un `AuthContext` come primo parametro.** Le uniche eccezioni ammesse restano le tre della Fase 0 (`count`, `create_bootstrap_admin`, `find_by_username`), e non se ne aggiungono di nuove.
- **Query sempre parametrizzate.** Nessuna concatenazione di stringhe in SQL, nemmeno per i percorsi `ltree`.
- **Forme funzione di sqlx** (`sqlx::query`, `query_as`), mai le macro `query!`. Nessuna directory `.sqlx/`, nessun `SQLX_OFFLINE`.
- **Identità dell'asset = `(folder_id, filename)`**, `content_hash` indicizzato ma **non** unico. Cancellare una foto cancella *quel* file in *quella* cartella.
- **I metadati originali sono immutabili.** `asset_exif` non viene mai riscritto; le modifiche dell'utente vivono in `asset_overrides`, che arriva in Fase 2.
- **Nessun percorso assoluto denormalizzato sugli asset.** Il path si ricostruisce dall'albero, altrimenti un `mv` di una cartella con 40.000 foto diventa un UPDATE di 40.000 righe.
- **Nessuna tabella di visibilità materializzata per utente.** Cambiare un permesso deve avere effetto immediato.
- **Errori RFC 9457** con `type` stabile prefissato `keeppix/`; il backend non traduce.
- Clippy `all` + `pedantic` a warn, `unwrap_used`/`expect_used` a warn; `cargo clippy --workspace --all-targets -- -D warnings` pulito. Nessun `unwrap`/`expect` in codice di produzione; i test portano `#[allow(...)]` localizzati.
- **Commit convenzionali in inglese.** Ogni task finisce con un commit e i test verdi.
- I test di integrazione girano contro Postgres reale via testcontainers; `cargo test --workspace -- --test-threads=1`.

---

## Checkpoint: prestazioni della suite di test

Non è un task, è una decisione da prendere **durante** l'esecuzione, con un
innesco preciso — non prima di iniziare, non ignorata fino alla fine.

**I numeri di partenza.** Il primo run reale della CI della Fase 0 (`backend`,
build + lint + test) ha impiegato **10m28s** a cache fredda e **5m23s** a cache
calda. La causa non è la compilazione: è che ogni test di integrazione chiama
`TestDb::start()`, che avvia un **container Postgres nuovo per test**, e
`cargo test --workspace -- --test-threads=1` li esegue in **sequenza** — il
vincolo è reale (i quattro test di `keeppix-server/tests/config.rs`
manipolano l'ambiente di processo e non tollerano il parallelismo), non
rimovibile con un flag.

**La proiezione.** La Fase 0 ha chiuso a 107 test Rust. Questo piano (1a) ne
aggiunge circa 45-55 fra i Task 2, 4, 5, 6, 7 e 8. Con lo stesso schema —
un container per test, tutto sequenziale — la CI supererebbe verosimilmente i
20-30 minuti già a metà di 1a, e la Fase 1b (coda job, worker) aggiungerà
ancora test di integrazione.

**Quando decidere:** non ora. Il Task 5 (`FolderRepo`) è il primo con un
numero di test a due cifre in un solo file (11) e un tempo di esecuzione
misurabile isolato. **Dopo aver completato il Task 5**, eseguire:

```bash
cargo test -p keeppix-db -- --test-threads=1
```

e confrontare il tempo con quello registrato a fine Fase 0 per lo stesso
crate. Se il rapporto tempo/numero-di-test è rimasto lineare, il problema è
ancora lontano e si rimanda la decisione al Task 8. Se è peggiorato in modo
visibile, è il momento di agire — prima di scrivere altri 30 test con lo
stesso schema.

**Le strade, con le implicazioni reali:**

1. **Un container per binario di test, schema Postgres separato per test.**
   `TestDb` diventa un `tokio::sync::OnceCell` statico per processo: il primo
   test che lo tocca avvia il container e applica le migrazioni una volta;
   ogni test successivo apre una connessione, esegue `CREATE SCHEMA
   test_<nome univoco>`, imposta `search_path` sulla connessione (o sul pool
   dedicato a quel test) e lavora lì. Il costo del boot del container (la
   parte lenta: 1-3 secondi) si paga una sola volta per binario invece che
   una volta per test. **Attenzione**: i test che aprono più connessioni
   verso lo stesso stato — per esempio `concurrent_generation_yields_a_single_secret`
   in `settings.rs`, che fa `tokio::join!` su due repository condividendo il
   pool — devono continuare a condividere lo stesso schema all'interno dello
   stesso test, non uno schema a testa: l'isolamento è fra test, non fra
   connessioni dello stesso test.
2. **`#[sqlx::test]`** (macro attributo di sqlx, non da confondere con
   `query!`: non verifica SQL a compile-time, gestisce il ciclo di vita di
   un database di test). Clona un database da un template per ogni test,
   che su Postgres è quasi gratuito (`CREATE DATABASE ... TEMPLATE`). Più
   vicino allo stile idiomatico di sqlx, ma richiede un `DATABASE_URL` fisso
   puntato a un server già in ascolto — un disallineamento reale con
   l'architettura attuale, dove ogni chiamata a `TestDb::start()` sceglie
   una porta a caso via testcontainers. Adottarlo bene richiede far partire
   **un** container all'inizio della suite (non per test) e derivare da lì
   sia il percorso testcontainers sia quello `KEEPPIX_TEST_DATABASE_URL` già
   esistente (vedi R9 in STATO.md) — i due meccanismi convergerebbero.
3. **Frammentare i job CI** (matrice o `cargo nextest` con partizionamento)
   invece di ridurre il lavoro. Riduce il tempo di attesa ma non il costo
   di calcolo, e non risolve nulla in locale durante lo sviluppo — dove il
   problema si sente comunque a ogni `cargo test`.

**La raccomandazione**, da riconsiderare con i numeri reali del Task 5: la
strada 1 costa meno da integrare nell'harness esistente (`crates/keeppix-db/tests/harness/mod.rs`
già astrae `TestDb`, quindi il cambiamento resta dentro quel file) e riduce
il costo dominante — l'avvio del container — senza cambiare come i test sono
scritti. La strada 2 è più elegante ma cambia il modo in cui *ogni* test
futuro si scrive, per tutte le fasi rimanenti: un cambiamento del genere va
deciso una volta sola e presto, non introdotto a metà.

Qualunque strada si scelga, va applicata **anche** ai file di test della Fase
0, non solo a quelli nuovi — altrimenti la suite ha due stili di harness
contemporaneamente, esattamente il tipo di incoerenza che la review finale
della Fase 0 ha già segnalato altrove (`assert_security_headers`
triplicato). Il crate `keeppix-test-support`, nato nella fix wave finale
della Fase 0, è il posto dove far vivere l'harness comune: ci vive già la
sospensione per gli header di sicurezza, ci può vivere anche questo.

---

## Struttura dei file

```
crates/keeppix-domain/src/
├── ids.rs              + LibraryId, FolderId, AssetId (stessa macro)
├── library.rs          NEW  Library, NewLibrary, LibraryStatus
├── folder.rs           NEW  Folder, FolderPath
├── asset.rs            NEW  Asset, NewAsset, AssetKind, AssetStatus, LocationSource
└── lib.rs              + riesportazioni

crates/keeppix-db/
├── migrations/
│   ├── 0004_libraries_folders.sql   NEW
│   ├── 0005_assets.sql              NEW
│   └── 0006_change_log.sql          NEW
├── src/
│   ├── row.rs          NEW  convenzioni di mapping condivise
│   ├── visibility.rs   NEW  visibility_scope(ctx) — l'unico costruttore di filtro
│   ├── libraries.rs    NEW  LibraryRepo
│   ├── folders.rs      NEW  FolderRepo
│   ├── assets.rs       NEW  AssetRepo
│   ├── changes.rs      NEW  ChangeLogRepo
│   ├── users.rs        MOD  conversione a FromRow
│   ├── sessions.rs     MOD  conversione a FromRow
│   └── settings.rs     MOD  conversione a FromRow
└── tests/
    ├── libraries.rs · folders.rs · assets.rs · visibility.rs · changes.rs   NEW
    └── harness/mod.rs  MOD  helper di seeding condivisi
```

**Ordine dei task:** 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8. Ogni task dipende dal precedente.

---

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

## Task 2: Migrazione librerie e cartelle

**Files:**
- Create: `crates/keeppix-db/migrations/0004_libraries_folders.sql`
- Create: `crates/keeppix-db/tests/schema_0004.rs`

**Interfaces:**
- Consumes: la tabella `users` della migrazione `0001`.
- Produces: le tabelle `libraries` e `folders`, l'estensione `ltree`, e gli indici su cui poggiano tutte le query dei task successivi.

- [ ] **Step 1: Scrivere il test che fallisce**

`crates/keeppix-db/tests/schema_0004.rs`:

```rust
mod harness;

use harness::TestDb;

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn ltree_extension_is_enabled() {
    let test = TestDb::start().await;
    let enabled: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'ltree')",
    )
    .fetch_one(test.db().pool())
    .await
    .unwrap();
    assert!(enabled, "ltree serve all'albero delle cartelle");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_library_requires_an_existing_owner() {
    let test = TestDb::start().await;
    let orphan = sqlx::query(
        "INSERT INTO libraries (id, name, owner_id, root_path) VALUES ($1, 'X', $2, '/tmp')",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(uuid::Uuid::now_v7())
    .execute(test.db().pool())
    .await;
    assert!(orphan.is_err(), "owner_id deve essere una foreign key");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn root_path_is_unique() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;

    let insert = "INSERT INTO libraries (id, name, owner_id, root_path) VALUES ($1, $2, $3, $4)";
    sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("Foto")
        .bind(owner.as_uuid())
        .bind("/mnt/foto")
        .execute(test.db().pool())
        .await
        .unwrap();

    let duplicate = sqlx::query(insert)
        .bind(uuid::Uuid::now_v7())
        .bind("Foto bis")
        .bind(owner.as_uuid())
        .bind("/mnt/foto")
        .execute(test.db().pool())
        .await;

    assert!(duplicate.is_err(), "due librerie non possono indicizzare lo stesso path");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn deleting_a_library_removes_its_folders() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let library = uuid::Uuid::now_v7();

    sqlx::query("INSERT INTO libraries (id, name, owner_id, root_path) VALUES ($1,'F',$2,'/m')")
        .bind(library)
        .bind(owner.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO folders (id, library_id, parent_id, name, path) \
         VALUES ($1, $2, NULL, '', '1'::ltree)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(library)
    .execute(test.db().pool())
    .await
    .unwrap();

    sqlx::query("DELETE FROM libraries WHERE id = $1")
        .bind(library)
        .execute(test.db().pool())
        .await
        .unwrap();

    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM folders")
        .fetch_one(test.db().pool())
        .await
        .unwrap();
    assert_eq!(remaining, 0, "le cartelle seguono la libreria");
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn sibling_folders_cannot_share_a_name() {
    let test = TestDb::start().await;
    let owner = harness::seed_admin(&test).await;
    let library = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO libraries (id, name, owner_id, root_path) VALUES ($1,'F',$2,'/m')")
        .bind(library)
        .bind(owner.as_uuid())
        .execute(test.db().pool())
        .await
        .unwrap();

    let root = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO folders (id, library_id, parent_id, name, path) \
         VALUES ($1, $2, NULL, '', '1'::ltree)",
    )
    .bind(root)
    .bind(library)
    .execute(test.db().pool())
    .await
    .unwrap();

    let child = "INSERT INTO folders (id, library_id, parent_id, name, path) \
                 VALUES ($1, $2, $3, '2024', '1.2'::ltree)";
    sqlx::query(child)
        .bind(uuid::Uuid::now_v7())
        .bind(library)
        .bind(root)
        .execute(test.db().pool())
        .await
        .unwrap();

    let duplicate = sqlx::query(child)
        .bind(uuid::Uuid::now_v7())
        .bind(library)
        .bind(root)
        .execute(test.db().pool())
        .await;

    assert!(duplicate.is_err(), "due sorelle non possono chiamarsi uguale");
}
```

- [ ] **Step 2: Aggiungere l'helper di seeding all'harness**

In `crates/keeppix-db/tests/harness/mod.rs`, che finora esponeva solo
`TestDb`, aggiungere una funzione riusata da tutti i test di questa fase:

```rust
/// Crea un amministratore e ne restituisce l'id. Ogni test di questa fase
/// ha bisogno di un proprietario per le librerie.
///
/// # Panics
/// Se la creazione fallisce: in un test è il comportamento voluto.
#[allow(clippy::expect_used, dead_code)]
pub async fn seed_admin(test: &TestDb) -> keeppix_domain::UserId {
    use keeppix_domain::{NewUser, Password, SystemRole, Username, hash_password};

    let password = Password::parse("correct horse battery staple").expect("password valida");
    keeppix_db::UserRepo::new(test.db())
        .create_bootstrap_admin(NewUser {
            username: Username::parse("giovanni").expect("username valido"),
            email: None,
            display_name: "Giovanni".to_owned(),
            password_hash: hash_password(&password).expect("hash").as_str().to_owned(),
            role: SystemRole::Admin,
        })
        .await
        .expect("creazione admin")
        .id
}
```

`dead_code` è necessario: ogni binario di test compila l'harness per intero
e non tutti usano questa funzione.

- [ ] **Step 3: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-db --test schema_0004 -- --test-threads=1`
Expected: FAIL — `relation "libraries" does not exist`.

- [ ] **Step 4: Scrivere la migrazione**

`crates/keeppix-db/migrations/0004_libraries_folders.sql`:

```sql
-- `ltree` rende "tutto ciò che sta sotto questa cartella" una singola
-- condizione indicizzata (`path <@ prefisso`) invece di una ricorsione.
-- È un'estensione trusted: non richiede privilegi di superuser.
CREATE EXTENSION IF NOT EXISTS ltree;

CREATE TABLE libraries (
    id               uuid        PRIMARY KEY,
    name             text        NOT NULL,
    owner_id         uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    root_path        text        NOT NULL,
    scan_enabled     boolean     NOT NULL DEFAULT true,
    exclude_patterns text[]      NOT NULL DEFAULT '{}',
    -- 'active' | 'offline' : offline significa "path non raggiungibile",
    -- stato in cui la scansione si ferma e non viene cancellato nulla.
    status           text        NOT NULL DEFAULT 'active'
                                 CHECK (status IN ('active', 'offline')),
    last_scan_at     timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

-- Due librerie che indicizzano lo stesso albero produrrebbero asset duplicati
-- con cancellazioni ambigue.
CREATE UNIQUE INDEX libraries_root_path_key ON libraries (root_path);
CREATE INDEX libraries_owner_idx ON libraries (owner_id);

CREATE TABLE folders (
    id         uuid        PRIMARY KEY,
    library_id uuid        NOT NULL REFERENCES libraries (id) ON DELETE CASCADE,
    parent_id  uuid        REFERENCES folders (id) ON DELETE CASCADE,
    -- Nome così come appare sul filesystem: spazi, accenti, qualsiasi cosa.
    -- La radice della libreria ha nome vuoto.
    name       text        NOT NULL,
    -- Percorso materializzato. Le etichette sono numeri progressivi per
    -- libreria, non nomi: `ltree` ammette solo [A-Za-z0-9_-] e "Matrimonio
    -- Rossi 2024" non è un'etichetta valida.
    path       ltree       NOT NULL,
    depth      int         NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- La condizione che serve a ogni query di sottoalbero.
CREATE INDEX folders_path_gist ON folders USING gist (path);
CREATE INDEX folders_library_idx ON folders (library_id);
CREATE INDEX folders_parent_idx ON folders (parent_id);

-- Un percorso identifica una cartella dentro la sua libreria.
CREATE UNIQUE INDEX folders_library_path_key ON folders (library_id, path);

-- Due sorelle non possono avere lo stesso nome. `parent_id` è NULL per la
-- radice, e in Postgres NULL non è uguale a NULL: serve un indice separato
-- che imponga una sola radice per libreria.
CREATE UNIQUE INDEX folders_sibling_name_key
    ON folders (parent_id, name) WHERE parent_id IS NOT NULL;
CREATE UNIQUE INDEX folders_single_root_key
    ON folders (library_id) WHERE parent_id IS NULL;

-- Contatori per la scrollbar della timeline, mantenuti da trigger in 1c.
-- La tabella nasce qui perché `assets` la referenzia concettualmente e
-- crearla dopo significherebbe ricalcolarla su tutta la libreria.
CREATE TABLE folder_month_counts (
    folder_id   uuid  NOT NULL REFERENCES folders (id) ON DELETE CASCADE,
    month       date  NOT NULL,
    asset_count int   NOT NULL DEFAULT 0,
    PRIMARY KEY (folder_id, month)
);
```

- [ ] **Step 5: Eseguire i test**

Run: `cargo test -p keeppix-db --test schema_0004 -- --test-threads=1`
Expected: PASS — 5 test.

- [ ] **Step 6: Verificare che le migrazioni precedenti reggano**

Run: `cargo test -p keeppix-db -- --test-threads=1`
Expected: tutti verdi. In particolare `migrations_are_idempotent` e `expected_tables_exist` non devono rompersi.

- [ ] **Step 7: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add libraries and the ltree folder tree"
```

---

## Task 3: Tipi di dominio per librerie, cartelle e asset

Tutti i tipi in un solo task, perché sono puri, si definiscono a vicenda e
non hanno senso separati. Nessun I/O: `keeppix-domain` resta senza database.

**Files:**
- Create: `crates/keeppix-domain/src/library.rs`, `folder.rs`, `asset.rs`
- Modify: `crates/keeppix-domain/src/ids.rs`, `lib.rs`

**Interfaces:**
- Consumes: la macro `id_type!` e `UserId` (Fase 0).
- Produces:
  - `LibraryId`, `FolderId`, `AssetId` — stessa macro, UUID v7.
  - `Library { id, name, owner_id, root_path: PathBuf, scan_enabled, exclude_patterns, status, last_scan_at, created_at }`, `LibraryStatus::{Active, Offline}`, `NewLibrary`.
  - `Folder { id, library_id, parent_id, name, path: FolderPath, depth }`.
  - `FolderPath` — newtype sul percorso `ltree`, con `root(seq) -> Self`, `child(&self, seq) -> Self`, `as_str()`, `depth()`, `parse(&str) -> Result<Self, DomainError>`.
  - `Asset { id, folder_id, filename, content_hash: Option<[u8;32]>, size_bytes, mtime, inode, kind, status, taken_at_utc, width, height, created_at }`, `AssetKind::{Image, RawImage, Video, Unknown}`, `AssetStatus::{Discovered, Indexed, Offline, Error, Trashed}`, `LocationSource::{Exif, User, MapPin, Copied, Gpx}`, `NewAsset`.
  - `DomainError::{InvalidFolderPath(String), InvalidAssetName(String)}` aggiunti.

- [ ] **Step 1: Scrivere i test che falliscono**

`crates/keeppix-domain/src/folder.rs`, in fondo:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn root_path_is_a_single_label() {
        let root = FolderPath::root(1);
        assert_eq!(root.as_str(), "1");
        assert_eq!(root.depth(), 1);
    }

    #[test]
    fn children_extend_the_parent() {
        let root = FolderPath::root(1);
        let child = root.child(7);
        let grandchild = child.child(42);
        assert_eq!(grandchild.as_str(), "1.7.42");
        assert_eq!(grandchild.depth(), 3);
    }

    #[test]
    fn parsing_accepts_a_numeric_path() {
        assert_eq!(FolderPath::parse("1.7.42").unwrap().as_str(), "1.7.42");
    }

    #[test]
    fn parsing_rejects_non_numeric_labels() {
        // Il nome della cartella non entra MAI nel percorso: ltree non
        // ammette spazi e accenti, e un nome interpolato sarebbe anche una
        // via di iniezione.
        assert!(FolderPath::parse("1.Matrimonio Rossi").is_err());
        assert!(FolderPath::parse("1.foto").is_err());
    }

    #[test]
    fn parsing_rejects_malformed_separators() {
        assert!(FolderPath::parse("").is_err());
        assert!(FolderPath::parse("1..7").is_err());
        assert!(FolderPath::parse(".1").is_err());
        assert!(FolderPath::parse("1.").is_err());
    }

    #[test]
    fn a_path_is_its_own_ancestor_check() {
        let root = FolderPath::root(1);
        let child = root.child(7);
        assert!(child.is_descendant_of(&root));
        assert!(!root.is_descendant_of(&child));
        assert!(root.is_descendant_of(&root), "ltree <@ include se stesso");
    }
}
```

- [ ] **Step 2: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-domain folder`
Expected: FAIL — `cannot find type FolderPath`.

- [ ] **Step 3: Aggiungere gli id**

In `crates/keeppix-domain/src/ids.rs`, sotto quelli esistenti:

```rust
id_type!(LibraryId);
id_type!(FolderId);
id_type!(AssetId);
```

- [ ] **Step 4: Implementare `folder.rs`**

```rust
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{FolderId, LibraryId};

/// Percorso materializzato di una cartella, nella forma `1.7.42`.
///
/// Le etichette sono numeri progressivi assegnati dal database, **mai** i
/// nomi delle cartelle: `ltree` ammette solo `[A-Za-z0-9_-]`, e un nome come
/// "Matrimonio Rossi 2024" non è un'etichetta valida. Tenere i nomi fuori dal
/// percorso evita anche di dover interpolare testo dell'utente in una query.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FolderPath(String);

impl FolderPath {
    #[must_use]
    pub fn root(seq: i64) -> Self {
        Self(seq.to_string())
    }

    #[must_use]
    pub fn child(&self, seq: i64) -> Self {
        Self(format!("{}.{seq}", self.0))
    }

    /// # Errors
    /// `DomainError::InvalidFolderPath` se il percorso non è una sequenza di
    /// numeri separati da punti.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        if raw.is_empty() {
            return Err(DomainError::InvalidFolderPath("empty".to_owned()));
        }
        for label in raw.split('.') {
            if label.is_empty() || !label.bytes().all(|b| b.is_ascii_digit()) {
                return Err(DomainError::InvalidFolderPath(format!(
                    "label {label:?} is not a number"
                )));
            }
        }
        Ok(Self(raw.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.split('.').count()
    }

    /// Stessa semantica dell'operatore `<@` di ltree: un percorso discende da
    /// sé stesso.
    #[must_use]
    pub fn is_descendant_of(&self, other: &Self) -> bool {
        self.0 == other.0 || self.0.starts_with(&format!("{}.", other.0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub library_id: LibraryId,
    pub parent_id: Option<FolderId>,
    /// Nome sul filesystem. Vuoto per la radice della libreria.
    pub name: String,
    pub path: FolderPath,
    pub depth: i32,
}
```

- [ ] **Step 5: Implementare `library.rs`**

```rust
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{LibraryId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryStatus {
    Active,
    /// Il percorso radice non è raggiungibile. In questo stato la scansione
    /// si ferma e **nulla viene cancellato**: un disco non montato non è una
    /// libreria svuotata.
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: LibraryId,
    pub name: String,
    pub owner_id: UserId,
    pub root_path: PathBuf,
    pub scan_enabled: bool,
    pub exclude_patterns: Vec<String>,
    pub status: LibraryStatus,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewLibrary {
    pub name: String,
    pub owner_id: UserId,
    pub root_path: PathBuf,
    pub exclude_patterns: Vec<String>,
}
```

- [ ] **Step 6: Implementare `asset.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::ids::{AssetId, FolderId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Image,
    RawImage,
    Video,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    /// Trovato dal walker, nient'altro letto.
    Discovered,
    /// Metadati letti e derivati generati.
    Indexed,
    /// Il file non è più sul disco. Non è una cancellazione: se il disco
    /// torna, l'asset torna con i suoi rating e album.
    Offline,
    /// Illeggibile o corrotto. Compare nella pagina Problemi.
    Error,
    Trashed,
}

/// Da dove arrivano le coordinate di un asset. Serve dalla Fase 4 in poi,
/// ed è qui perché aggiungere una colonna a `assets` dopo l'indicizzazione
/// di 200.000 righe costa molto più che prevederla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationSource {
    Exif,
    User,
    MapPin,
    Copied,
    Gpx,
}

/// Nome di file dentro una cartella. Rifiuta i separatori di percorso, così
/// un nome non può mai far uscire dalla cartella che lo contiene.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetName(String);

impl AssetName {
    /// # Errors
    /// `DomainError::InvalidAssetName` se vuoto, se contiene `/`, `\` o un
    /// byte nullo, o se è `.` / `..`.
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let invalid = raw.is_empty()
            || raw.contains('/')
            || raw.contains('\\')
            || raw.contains('\0')
            || raw == "."
            || raw == "..";
        if invalid {
            return Err(DomainError::InvalidAssetName(format!("{raw:?}")));
        }
        Ok(Self(raw.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub folder_id: FolderId,
    pub filename: AssetName,
    /// blake3. `None` finché la fase di hash non è passata.
    pub content_hash: Option<[u8; 32]>,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub inode: Option<i64>,
    pub kind: AssetKind,
    pub status: AssetStatus,
    /// Data di scatto normalizzata in UTC. `None` finché gli EXIF non sono
    /// stati letti; a quel punto si ripiega su `mtime` se il file non ne ha.
    pub taken_at_utc: Option<DateTime<Utc>>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub created_at: DateTime<Utc>,
}

/// Ciò che il walker sa di un file appena trovato: nient'altro che quello
/// che `stat()` restituisce.
#[derive(Debug, Clone)]
pub struct NewAsset {
    pub folder_id: FolderId,
    pub filename: AssetName,
    pub size_bytes: i64,
    pub mtime: DateTime<Utc>,
    pub inode: Option<i64>,
    pub kind: AssetKind,
}
```

Aggiungere i test per `AssetName` in fondo a `asset.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_accepts_ordinary_filenames() {
        assert!(AssetName::parse("DSC_0042.ARW").is_ok());
        assert!(AssetName::parse("foto di famiglia.jpg").is_ok());
        assert!(AssetName::parse("émoji 🎉.png").is_ok());
    }

    #[test]
    fn asset_name_rejects_path_separators() {
        assert!(AssetName::parse("../etc/passwd").is_err());
        assert!(AssetName::parse("a/b.jpg").is_err());
        assert!(AssetName::parse("a\\b.jpg").is_err());
    }

    #[test]
    fn asset_name_rejects_dot_entries_and_empty() {
        assert!(AssetName::parse(".").is_err());
        assert!(AssetName::parse("..").is_err());
        assert!(AssetName::parse("").is_err());
    }
}
```

- [ ] **Step 7: Aggiungere le varianti d'errore ed esportare**

In `error.rs`:

```rust
    #[error("invalid folder path: {0}")]
    InvalidFolderPath(String),
    #[error("invalid asset name: {0}")]
    InvalidAssetName(String),
```

In `lib.rs`:

```rust
pub mod asset;
pub mod folder;
pub mod library;

pub use asset::{Asset, AssetKind, AssetName, AssetStatus, LocationSource, NewAsset};
pub use folder::{Folder, FolderPath};
pub use ids::{AssetId, FolderId, LibraryId};
pub use library::{Library, LibraryStatus, NewLibrary};
```

- [ ] **Step 8: Eseguire i test**

Run: `cargo test -p keeppix-domain && cargo clippy -p keeppix-domain --all-targets -- -D warnings`
Expected: PASS — 22 test esistenti più 9 nuovi.

- [ ] **Step 9: Commit**

```bash
git add crates/keeppix-domain
git commit -m "feat(domain): add library, folder and asset types"
```

---

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

## Task 5: `FolderRepo` e l'albero `ltree`

Il task più delicato della fase: qui si decide come si naviga e si sposta un
albero di decine di migliaia di nodi.

**Files:**
- Create: `crates/keeppix-db/src/folders.rs`, `crates/keeppix-db/tests/folders.rs`
- Modify: `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Consumes: `LibraryRepo` (per i test), `Folder`, `FolderPath`, `FolderId`, `LibraryId`, `AuthContext`.
- Produces `FolderRepo` con:
  - `new(db: &Db)`
  - `ensure_root(&self, library_id: LibraryId) -> Result<Folder, DbError>` — idempotente.
  - `ensure_child(&self, parent: &Folder, name: &str) -> Result<Folder, DbError>` — idempotente; crea o restituisce l'esistente.
  - `ensure_path(&self, library_id: LibraryId, relative: &[&str]) -> Result<Folder, DbError>` — crea l'intera catena in una transazione.
  - `children(&self, ctx: &AuthContext, folder_id: FolderId) -> Result<Vec<Folder>, DbError>`
  - `subtree(&self, ctx: &AuthContext, folder_id: FolderId) -> Result<Vec<Folder>, DbError>` — usa `path <@`.
  - `find_by_id(&self, ctx: &AuthContext, id: FolderId) -> Result<Folder, DbError>`
  - `move_subtree(&self, ctx: &AuthContext, folder_id: FolderId, new_parent: FolderId) -> Result<(), DbError>` — riscrive i percorsi dell'intero sottoalbero con **una** query.
  - `absolute_path(&self, ctx: &AuthContext, folder_id: FolderId) -> Result<PathBuf, DbError>` — ricostruisce il percorso su disco risalendo l'albero.

Le funzioni `ensure_*` non prendono `AuthContext` perché le chiama lo
scanner. Documentarlo come per `mark_scanned`.

- [ ] **Step 1: Scrivere i test che falliscono**

`crates/keeppix-db/tests/folders.rs`:

```rust
mod harness;

use harness::TestDb;
use keeppix_db::{DbError, FolderRepo, LibraryRepo};
use keeppix_domain::{AuthContext, LibraryId, NewLibrary, SystemRole, UserId};

async fn seed_library(test: &TestDb, owner: UserId) -> LibraryId {
    let ctx = AuthContext::user(owner, SystemRole::Admin);
    LibraryRepo::new(test.db())
        .create(
            &ctx,
            NewLibrary {
                name: "Foto".to_owned(),
                owner_id: owner,
                root_path: std::path::PathBuf::from("/mnt/foto"),
                exclude_patterns: vec![],
            },
        )
        .await
        .expect("libreria")
        .id
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn the_root_has_an_empty_name_and_a_single_label() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;

    let root = FolderRepo::new(test.db()).ensure_root(library).await.unwrap();

    assert_eq!(root.name, "");
    assert!(root.parent_id.is_none());
    assert_eq!(root.path.depth(), 1);
    assert_eq!(root.depth, 1);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn ensure_root_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let first = repo.ensure_root(library).await.unwrap();
    let second = repo.ensure_root(library).await.unwrap();

    assert_eq!(first.id, second.id, "una libreria ha una sola radice");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn children_extend_the_parent_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let root = repo.ensure_root(library).await.unwrap();
    let year = repo.ensure_child(&root, "2024").await.unwrap();
    let event = repo.ensure_child(&year, "Matrimonio Rossi").await.unwrap();

    assert!(event.path.is_descendant_of(&root.path));
    assert!(event.path.is_descendant_of(&year.path));
    assert_eq!(event.depth, 3);
    assert_eq!(event.name, "Matrimonio Rossi", "il nome resta quello vero");
    assert!(
        !event.path.as_str().contains("Matrimonio"),
        "il nome non deve MAI finire nel percorso ltree"
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn ensure_child_is_idempotent() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let root = repo.ensure_root(library).await.unwrap();
    let a = repo.ensure_child(&root, "2024").await.unwrap();
    let b = repo.ensure_child(&root, "2024").await.unwrap();

    assert_eq!(a.id, b.id, "riscansionare non duplica le cartelle");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn ensure_path_creates_the_whole_chain() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let leaf = repo
        .ensure_path(library, &["2024", "Grecia", "Santorini"])
        .await
        .unwrap();

    assert_eq!(leaf.name, "Santorini");
    assert_eq!(leaf.depth, 4, "radice piu tre livelli");

    // Rieseguirla non crea nulla di nuovo.
    let again = repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();
    assert_eq!(leaf.id, again.id);
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn subtree_returns_descendants_including_itself() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();
    repo.ensure_path(library, &["2024", "Italia"]).await.unwrap();
    repo.ensure_path(library, &["2023"]).await.unwrap();

    let root = repo.ensure_root(library).await.unwrap();
    let y2024 = repo.ensure_child(&root, "2024").await.unwrap();

    let under_2024 = repo.subtree(&ctx, y2024.id).await.unwrap();
    let names: Vec<&str> = under_2024.iter().map(|f| f.name.as_str()).collect();

    assert!(names.contains(&"2024"), "ltree <@ include il nodo stesso");
    assert!(names.contains(&"Grecia"));
    assert!(names.contains(&"Santorini"));
    assert!(names.contains(&"Italia"));
    assert!(!names.contains(&"2023"), "un fratello non e un discendente");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn children_are_direct_only() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();
    let root = repo.ensure_root(library).await.unwrap();
    let y2024 = repo.ensure_child(&root, "2024").await.unwrap();

    let direct = repo.children(&ctx, y2024.id).await.unwrap();
    let names: Vec<&str> = direct.iter().map(|f| f.name.as_str()).collect();

    assert_eq!(names, vec!["Grecia"], "solo i figli diretti, non i nipoti");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn moving_a_subtree_rewrites_every_descendant_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    // /2024/Grecia/Santorini  ->  spostiamo Grecia sotto /Archivio
    repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();
    let archive = repo.ensure_path(library, &["Archivio"]).await.unwrap();

    let root = repo.ensure_root(library).await.unwrap();
    let y2024 = repo.ensure_child(&root, "2024").await.unwrap();
    let greece = repo.ensure_child(&y2024, "Grecia").await.unwrap();

    repo.move_subtree(&ctx, greece.id, archive.id).await.unwrap();

    let moved = repo.find_by_id(&ctx, greece.id).await.unwrap();
    assert_eq!(moved.parent_id, Some(archive.id));
    assert!(moved.path.is_descendant_of(&archive.path));
    assert_eq!(moved.depth, 3);

    // Il nipote deve essere sceso con lui.
    let under_archive = repo.subtree(&ctx, archive.id).await.unwrap();
    let santorini = under_archive
        .iter()
        .find(|f| f.name == "Santorini")
        .expect("Santorini e sceso con Grecia");
    assert!(santorini.path.is_descendant_of(&moved.path));
    assert_eq!(santorini.depth, 4);

    // E non deve piu stare sotto 2024.
    let under_2024 = repo.subtree(&ctx, y2024.id).await.unwrap();
    assert_eq!(under_2024.len(), 1, "sotto 2024 resta solo 2024 stesso");
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_folder_cannot_be_moved_inside_itself() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    let leaf = repo.ensure_path(library, &["2024", "Grecia"]).await.unwrap();
    let root = repo.ensure_root(library).await.unwrap();
    let y2024 = repo.ensure_child(&root, "2024").await.unwrap();

    // Spostare 2024 dentro il proprio figlio scollegherebbe il sottoalbero.
    let cycle = repo.move_subtree(&ctx, y2024.id, leaf.id).await;
    assert!(matches!(cycle, Err(DbError::Conflict(_))));
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn absolute_path_reconstructs_the_filesystem_path() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let library = seed_library(&test, admin).await;
    let ctx = AuthContext::user(admin, SystemRole::Admin);
    let repo = FolderRepo::new(test.db());

    let leaf = repo.ensure_path(library, &["2024", "Grecia", "Santorini"]).await.unwrap();

    assert_eq!(
        repo.absolute_path(&ctx, leaf.id).await.unwrap(),
        std::path::PathBuf::from("/mnt/foto/2024/Grecia/Santorini")
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn a_plain_user_cannot_read_someone_elses_folders() {
    let test = TestDb::start().await;
    let admin = harness::seed_admin(&test).await;
    let mario = harness::seed_user(&test, admin, "mario").await;
    let library = seed_library(&test, admin).await;
    let repo = FolderRepo::new(test.db());

    let folder = repo.ensure_path(library, &["2024"]).await.unwrap();

    let mario_ctx = AuthContext::user(mario, SystemRole::User);
    assert!(matches!(repo.find_by_id(&mario_ctx, folder.id).await, Err(DbError::Forbidden)));
    assert!(matches!(repo.children(&mario_ctx, folder.id).await, Err(DbError::Forbidden)));
    assert!(matches!(repo.subtree(&mario_ctx, folder.id).await, Err(DbError::Forbidden)));
}
```

- [ ] **Step 2: Eseguire e verificare il fallimento**

Run: `cargo test -p keeppix-db --test folders -- --test-threads=1`
Expected: FAIL — `unresolved import keeppix_db::FolderRepo`.

- [ ] **Step 3: Implementare `folders.rs`**

Punti su cui il codice deve essere esatto, perché sono quelli che i test
inchiodano:

- Le etichette `ltree` vengono da una **sequenza per libreria**, non da un
  contatore globale: due librerie possono avere entrambe `1.2.3`.
- `ensure_child` deve essere idempotente **sotto concorrenza**: si usa
  `INSERT ... ON CONFLICT (parent_id, name) DO NOTHING` seguito da una
  rilettura, non un `SELECT` seguito da `INSERT`.
- `move_subtree` riscrive tutti i discendenti con **una** query, usando
  `ltree`:

```sql
UPDATE folders
   SET path  = $new_prefix::ltree || subpath(path, nlevel($old_prefix::ltree)),
       depth = nlevel($new_prefix::ltree) + nlevel(path) - nlevel($old_prefix::ltree)
 WHERE library_id = $library AND path <@ $old_prefix::ltree;
```

  Spostare una cartella con 40.000 foto tocca le righe di `folders`, non
  quelle di `assets`: è il motivo per cui nessun asset porta un percorso
  assoluto denormalizzato.

- Il ciclo va rifiutato **prima** dell'UPDATE: se `new_parent.path` discende
  da `folder.path`, restituire `DbError::Conflict`. Senza questo controllo il
  sottoalbero si scollega e non è più raggiungibile da nessuna radice.
- `absolute_path` risale l'albero con una CTE ricorsiva e concatena i `name`
  sotto `libraries.root_path`. I nomi vengono dal database, non dal client.
- I metodi con `AuthContext` risolvono la visibilità dalla libreria
  proprietaria: `Forbidden` prima di `NotFound`, come ovunque.

Aggiungere alla migrazione una sequenza per le etichette:

```sql
-- In 0004: il numero progressivo delle etichette ltree.
ALTER TABLE libraries ADD COLUMN next_folder_seq bigint NOT NULL DEFAULT 1;
```

e incrementarla con `UPDATE libraries SET next_folder_seq = next_folder_seq + 1
RETURNING next_folder_seq - 1` dentro la stessa transazione dell'inserimento.

- [ ] **Step 4: Eseguire i test**

Run: `cargo test -p keeppix-db --test folders -- --test-threads=1`
Expected: PASS — 11 test.

- [ ] **Step 5: Verificare il workspace**

Run: `cargo test --workspace -- --test-threads=1 && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`

- [ ] **Step 6: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add folder repository with ltree subtree operations"
```

---

## Task 6: Migrazione asset ed EXIF

**Files:**
- Create: `crates/keeppix-db/migrations/0005_assets.sql`
- Create: `crates/keeppix-db/tests/schema_0005.rs`

**Interfaces:**
- Produces: `assets` e `asset_exif` con gli indici su cui poggeranno la timeline (1c) e la ricerca.

- [ ] **Step 1: Scrivere la migrazione**

```sql
CREATE TABLE assets (
    id          uuid        PRIMARY KEY,
    folder_id   uuid        NOT NULL REFERENCES folders (id) ON DELETE CASCADE,
    filename    text        NOT NULL,

    -- blake3, NULL finche la fase di hash non e passata. Indicizzato ma NON
    -- unico: la stessa foto in due cartelle sono due asset distinti, con
    -- cancellazioni indipendenti. La deduplica e una scelta di presentazione,
    -- non di identita.
    content_hash bytea,

    size_bytes  bigint      NOT NULL,
    mtime       timestamptz NOT NULL,
    inode       bigint,

    kind        text        NOT NULL DEFAULT 'unknown'
                            CHECK (kind IN ('image', 'raw_image', 'video', 'unknown')),
    status      text        NOT NULL DEFAULT 'discovered'
                            CHECK (status IN ('discovered','indexed','offline','error','trashed')),
    error_detail text,

    -- Normalizzata in UTC dal fuso ricavato dal GPS quando c'e; altrimenti
    -- dall'ora locale del file. E la colonna su cui ordina la timeline.
    taken_at_utc timestamptz,
    tz_offset_minutes int,

    width       int,
    height      int,
    duration_ms int,

    -- Predisposte per le fasi successive: aggiungere colonne a una tabella
    -- con 200.000 righe costa, prevederle no.
    location    geography(Point, 4326),
    place_id    bigint,
    location_source text CHECK (location_source IN ('exif','user','map_pin','copied','gpx')),
    stack_id    uuid,

    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

-- L'identita: un file in una cartella.
CREATE UNIQUE INDEX assets_folder_filename_key ON assets (folder_id, filename);

-- Duplicati e rilevamento degli spostamenti.
CREATE INDEX assets_content_hash_idx ON assets (content_hash) WHERE content_hash IS NOT NULL;

-- L'ordinamento della timeline: (data, id) come chiave di paginazione
-- keyset, che non degrada come OFFSET.
CREATE INDEX assets_timeline_idx ON assets (taken_at_utc DESC, id DESC)
    WHERE status = 'indexed';

CREATE INDEX assets_folder_idx ON assets (folder_id);
CREATE INDEX assets_status_idx ON assets (status) WHERE status IN ('discovered', 'error');
CREATE INDEX assets_location_gist ON assets USING gist (location) WHERE location IS NOT NULL;

-- EXIF grezzi, mai riscritti. Le modifiche dell'utente vivranno in
-- asset_overrides (Fase 2), e il valore mostrato sara COALESCE(override, exif).
CREATE TABLE asset_exif (
    asset_id  uuid  PRIMARY KEY REFERENCES assets (id) ON DELETE CASCADE,
    raw       jsonb NOT NULL,
    camera_make  text,
    camera_model text,
    lens         text,
    iso          int,
    f_number     real,
    exposure     text,
    focal_length real,
    parsed_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX asset_exif_camera_idx ON asset_exif (camera_model) WHERE camera_model IS NOT NULL;
```

- [ ] **Step 2: Scrivere i test**

`crates/keeppix-db/tests/schema_0005.rs` — verificare almeno:
la coppia `(folder_id, filename)` è unica; due asset **possono** condividere
`content_hash`; cancellare una cartella cancella i suoi asset; cancellare un
asset cancella i suoi EXIF; un `kind` fuori dal CHECK viene rifiutato;
l'indice della timeline esiste (`pg_indexes`).

Scrivere i test **prima** di applicare la migrazione, verificarne il
fallimento, poi applicarla.

- [ ] **Step 3: Eseguire**

Run: `cargo test -p keeppix-db --test schema_0005 -- --test-threads=1`
Expected: PASS — 6 test.

- [ ] **Step 4: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add assets and exif tables"
```

---

## Task 7: `AssetRepo` e la funzione di visibilità

Qui nasce `visibility_scope`, la funzione che ogni query sugli asset
attraverserà da qui alla Fase 6. La sua firma è un contratto congelato: la
Fase 3 la estenderà con la tabella `permissions` senza cambiare i chiamanti.

**Files:**
- Create: `crates/keeppix-db/src/visibility.rs`, `assets.rs`
- Create: `crates/keeppix-db/tests/assets.rs`, `visibility.rs`
- Modify: `crates/keeppix-db/src/lib.rs`

**Interfaces:**
- Produces:
  - `VisibilityScope` con `VisibilityScope::resolve(db, ctx) -> Result<Self, DbError>` e `library_ids(&self) -> &[LibraryId]`, più `is_unrestricted(&self) -> bool` per l'admin.
  - `AssetRepo` con `upsert_discovered`, `set_hash`, `set_indexed`, `set_error`, `mark_offline`, `find_by_folder`, `find_by_hash`, `count_by_status`.

**Nota di progettazione da rispettare:** in Fase 1a la visibilità è
"le librerie che possiedi, o tutte se sei admin". In Fase 3 diventerà
"più i sottoalberi condivisi con te o con i tuoi gruppi". Il tipo
`VisibilityScope` deve poter esprimere entrambe senza che i chiamanti
cambino: esporre un metodo che produce la clausola SQL e i suoi parametri,
non l'elenco grezzo degli id.

- [ ] **Step 1: Scrivere i test di visibilità**

`crates/keeppix-db/tests/visibility.rs` deve pinnare almeno:
l'admin ha scope illimitato; un utente vede solo le proprie librerie; un
utente senza librerie ha scope vuoto e ogni query restituisce zero righe
senza errore; lo scope si aggiorna quando gli viene creata una libreria.

- [ ] **Step 2: Scrivere i test degli asset**

`crates/keeppix-db/tests/assets.rs` deve pinnare:
`upsert_discovered` è idempotente su `(folder_id, filename)` e aggiorna
size/mtime se il file è cambiato; due cartelle diverse possono avere lo
stesso filename; `set_hash` accetta lo stesso hash su asset diversi;
`find_by_hash` li trova entrambi; le transizioni di stato sono quelle
attese; un utente non proprietario riceve `Forbidden`, e su un id
inesistente riceve **anch'esso** `Forbidden`, non `NotFound`.

- [ ] **Step 3: Verificare il fallimento, poi implementare**

- [ ] **Step 4: Eseguire**

Run: `cargo test -p keeppix-db -- --test-threads=1`
Expected: tutto verde.

- [ ] **Step 5: Commit**

```bash
git add crates/keeppix-db
git commit -m "feat(db): add asset repository and the visibility scope"
```

---

## Task 8: Registro delle modifiche

L'endpoint `/sync/delta` del client mobile (Fase 6) poggia su questo, ma il
registro va alimentato **da subito**: attivarlo dopo significherebbe che
tutto ciò che è successo prima è invisibile alla sincronizzazione.

**Files:**
- Create: `crates/keeppix-db/migrations/0006_change_log.sql`, `crates/keeppix-db/src/changes.rs`, `crates/keeppix-db/tests/changes.rs`

**Interfaces:**
- Produces: `ChangeLogRepo` con `since(&self, ctx, cursor) -> Result<ChangePage, DbError>`, dove `ChangePage { cursor, upserted, deleted, has_more }`.

**Il dettaglio che va preso bene**, e che il piano della Fase 0 aveva già
segnalato: una transazione con `seq` più basso può committare **dopo** una
con `seq` più alto. Un client che legge fino all'ultimo `seq` visto si
perderebbe le righe committate nel frattempo. Il cursore restituito va
arretrato al limite delle transazioni certamente concluse:

```sql
SELECT COALESCE(min(seq), (SELECT COALESCE(max(seq), 0) FROM change_log)) - 1
  FROM change_log
 WHERE seq > $cursor
   AND xmin::text::bigint >= pg_snapshot_xmin(pg_current_snapshot())::text::bigint;
```

Il test che lo dimostra apre **due** transazioni sovrapposte, committandole
in ordine inverso, e verifica che il client non perda righe.

- [ ] **Step 1: Scrivere la migrazione**

```sql
CREATE TABLE change_log (
    seq       bigserial   PRIMARY KEY,
    entity    text        NOT NULL CHECK (entity IN ('asset', 'folder', 'album', 'library')),
    entity_id uuid        NOT NULL,
    op        text        NOT NULL CHECK (op IN ('upsert', 'delete')),
    at        timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX change_log_seq_idx ON change_log (seq);
CREATE INDEX change_log_entity_idx ON change_log (entity, entity_id);

CREATE OR REPLACE FUNCTION log_asset_change() RETURNS trigger AS $$
BEGIN
    IF (TG_OP = 'DELETE') THEN
        INSERT INTO change_log (entity, entity_id, op) VALUES ('asset', OLD.id, 'delete');
        RETURN OLD;
    END IF;
    INSERT INTO change_log (entity, entity_id, op) VALUES ('asset', NEW.id, 'upsert');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER assets_change_log
    AFTER INSERT OR UPDATE OR DELETE ON assets
    FOR EACH ROW EXECUTE FUNCTION log_asset_change();
```

- [ ] **Step 2-5: test, implementazione, verifica, commit**

Seguire lo schema TDD dei task precedenti. Il test sulle transazioni
sovrapposte è quello che vale l'intero task: senza, il difetto si manifesta
solo in produzione con un client mobile che perde foto in modo intermittente.

```bash
git add crates/keeppix-db
git commit -m "feat(db): add the change log that feeds delta sync"
```

---

## Criteri di completamento della Fase 1a

- [ ] `cargo test --workspace -- --test-threads=1` verde, con almeno 45 test nuovi rispetto ai 107 della Fase 0.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` pulito.
- [ ] `cargo fmt --check` pulito.
- [ ] Un test di integrazione crea una libreria, un albero di cartelle profondo tre livelli e una dozzina di asset, e li rilegge tutti con i permessi applicati.
- [ ] Spostare una cartella con discendenti riscrive i percorsi di tutto il sottoalbero e **non** tocca la tabella `assets`.
- [ ] Un utente non proprietario riceve `Forbidden` — mai `NotFound` — sondando id di librerie, cartelle e asset che non gli appartengono.
- [ ] Il registro delle modifiche non perde righe con transazioni sovrapposte.
- [ ] La CI è verde sulla pull request.

## Cosa NON è nella Fase 1a

Da non implementare, per quanto vicino sembri: la coda dei job, i worker, i
profili energetici, qualunque cosa in `keeppix-media`, il walker del
filesystem, l'hashing, i derivati, il watcher, gli endpoint HTTP, il
WebSocket, la timeline e il frontend. Sono la Fase 1b e la Fase 1c.

Il valore di questa fase è che il modello dati sia sbagliato adesso, quando
correggerlo costa una migrazione e non una riscansione di un terabyte.


