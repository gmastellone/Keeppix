# Keeppix Fase 2R — Rimedio: usabilità, prestazioni, e i buchi di processo

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rendere Keeppix usabile da una persona senza toccare il database, riportare la scansione da giorni a minuti, e chiudere i difetti di processo che hanno permesso a entrambi i problemi di attraversare tre fasi di test tutti verdi.

**Architecture:** Nessuna decisione architetturale nuova. Si costruiscono le superfici HTTP mancanti sopra repository che **esistono già e sono testati**, si corregge un difetto di prestazioni localizzato in due file, e si aggiungono tre classi di test che oggi non esistono.

**Dipende da:** Fase 2, mergiata su `main` (PR #4).

---

## Il vincolo che governa ogni scelta di questa fase

Keeppix deve essere **estremamente usabile** e insieme **estremamente leggero**.
Non sono in tensione: sono lo stesso requisito visto da due lati. Il bersaglio
dichiarato è un **Raspberry Pi 5 con 8 GB di RAM e disco NVMe**, che deve
servire 200.000 foto.

Tradotto in regole operative per questa fase:

- **Nessuna dipendenza nuova** senza una ragione scritta nel ledger. Ogni crate
  aggiunto è tempo di build in CI, superficie di CVE, e RAM a runtime.
- **Le nuove rotte non caricano tutto in memoria.** Elenchi paginati con keyset,
  mai `SELECT *` senza `LIMIT`. Un elenco di 200.000 asset non deve poter essere
  chiesto.
- **Il frontend nuovo va in chunk lazy.** Il bundle d'ingresso resta sotto
  **150 KB gzip** (oggi 80,4 KB): amministrazione, gestione librerie e
  procedura di primo avvio non li apre chi guarda le foto.
- **Nessun `thread::sleep` in contesto async**, mai. Blocca un thread del worker
  pool, non solo quel job — su 4 core significa perdere un quarto della capacità.
- **Ogni operazione su percorso caldo ha un budget** verificato da un test
  (Task 8). Larghi di proposito: devono cogliere una regressione di ordine di
  grandezza, non oscillare col carico del runner.

---

## I difetti verificati — cosa è stato osservato, esattamente

Misurati il 2026-08-16 con una prova sul campo su un archivio reale:
**1.558 file Sony ARW, 36 GB, cinque cartelle**, montato read-only in Docker.

### DIFETTO 1 — La discovery dorme 5 secondi per ogni file

**Dove.** `crates/keeppix-media/src/walk.rs`, funzione `restat_if_stable`:

```rust
pub fn restat_if_stable(path: &Path, wait: Duration) -> std::io::Result<Option<Metadata>> {
    let first = std::fs::metadata(path)?;
    if !wait.is_zero() {
        std::thread::sleep(wait);          // ← qui
    }
    let second = std::fs::metadata(path)?;
    if is_stable(&first, &second) { Ok(Some(second)) } else { Ok(None) }
}
```

Chiamata **dentro il ciclo di discovery, un file alla volta**, in
`crates/keeppix-jobs/src/discover.rs`:

```rust
let mut seen = Vec::new();
for walked in iter_entries(root, &library.exclude_patterns) {
    let Some(meta) = restat_if_stable(&walked.path, stability_wait)   // ← per ogni file
        .map_err(...)? else { continue; };
    ...
    seen.push(file);                        // ← accumula, non scrive
}
```

Con il valore di produzione, in `crates/keeppix-server/src/main.rs`:

```rust
let handler = keeppix_jobs::IngestHandler {
    db: db.clone(),
    data_dir: config.data_dir.clone(),
    stability_wait: std::time::Duration::from_secs(5),   // ← 5 secondi
};
```

**Cosa è stato osservato.** Dopo **650 secondi** di esecuzione: `assets` = 0,
`asset_exif` = 0, job `discover_library` ancora in stato `running`, nessun
errore. Coerente con ~130 file processati su 1.558.

**Il conto.** 1.558 file × 5 s = **7.790 s = 2 ore 10 minuti**. Sull'archivio
reale di 200.000 file: **11 giorni**. La spec
([`fase-1b-ingestione.md`](../specs/fase-1b-ingestione.md) §5.2) promette
**~3 minuti su 1 TB**.

**Perché nessun test l'ha colto.** Tutti i test passano `Duration::ZERO`, e il
ramo `if !wait.is_zero()` **salta il sonno**. Il codice eseguito dai test non è
il codice spedito.

**Due aggravanti nello stesso punto:**

1. `std::thread::sleep` in un job asincrono **blocca il thread del worker
   pool**, non solo quel job. Su 4 worker, quattro thread dormono.
2. `seen.push(file)` accumula in memoria e scrive **solo a fine ciclo**. Quindi:
   nessun avanzamento visibile all'utente, un errore al 99% perde tutto, e su
   200.000 file l'accumulo in RAM è di per sé un problema su un Pi.

---

### DIFETTO 2 — Non esiste alcun modo di creare una libreria o avviare una scansione

**Cosa è stato osservato.** Per eseguire la prova sul campo è stato necessario
inserire la riga a mano:

```sql
INSERT INTO libraries (id, name, owner_id, root_path)
VALUES (gen_random_uuid(), 'Campo', '<owner>', '/photos');
```

e poi accodare il job a mano:

```sql
INSERT INTO jobs (kind, payload, priority, dedup_key)
VALUES ('discover_library', '{"library_id":"..."}'::jsonb, 3, 'discover:...');
```

**Le rotte registrate oggi** (da `crates/keeppix-api/src/lib.rs`):

```
/health  /api/openapi.json  /setup  /setup/status
/auth/login  /auth/logout  /auth/me  /auth/refresh
/timeline  /timeline/buckets  /folders/tree  /folders/{id}/children
/search  /search/suggest  /saved-searches  /viewport
/media/thumb/{hash}  /media/preview/{hash}  /media/original/{id}
/assets/{id}  /assets/{id}/flags  /assets/{id}/metadata  /assets/{id}/restore
/metadata/batch  /metadata/batch/shift-taken-at  /metadata/batch/{id}/undo
/duplicates  /duplicates/{hash}  /duplicates/{hash}/resolve
/problems  /ws  /ws/ticket
```

**Cosa manca**, con il repository già scritto, testato e funzionante dietro:

| Repository esistente | Rotta | Conseguenza |
|---|---|---|
| `LibraryRepo` (`crates/keeppix-db/src/libraries.rs`) | **nessuna** | non si può aggiungere una cartella di foto |
| `watch::enqueue_rescan` (`crates/keeppix-jobs/src/watch.rs`) | **nessuna** | non si può avviare né rilanciare una scansione |
| `UserRepo::create` | solo `/setup` (primo admin) | non si può creare un secondo utente |
| `TrashRepo` — elenco | solo `DELETE` e `restore` per id | il cestino non è navigabile: si recupera una foto solo conoscendone l'id |
| `StackRepo` (`crates/keeppix-db/src/stacks.rs`) | **nessuna** | gli stack RAW+JPEG esistono nel database e non si vedono |

**Un terzo difetto scoperto qui.** `watch::spawn_all` legge le librerie **solo
al boot**, e il commento nel codice lo dichiara: *«le librerie create dopo il
boot restano scoperte fino al riavvio»*. Con la creazione via API questo diventa
inaccettabile: si crea una libreria e non viene mai sorvegliata.

---

### DIFETTO 3 — Un difetto nel mio script di prova, riportato perché è la stessa classe

Lo script `scripts/field-test.sh` verificava che gli originali fossero montati
in sola lettura così:

```bash
docker compose exec -T keeppix sh -c 'touch /photos/.probe'
```

L'immagine è **distroless: `sh` non esiste**. Il comando falliva sempre, e il
controllo «passava» senza verificare niente. Corretto interrogando Docker
(`docker inspect ... .RW`), che è la fonte di verità.

È la stessa classe di difetto dei tre test della Fase 0 che passavano senza
provare ciò che il loro nome affermava. **Un controllo che non può fallire non
è un controllo.**

---

## L'analisi — perché sono passati

Cinque difetti di processo. Non sono difetti di esecuzione: Cursor ha
implementato correttamente ciò che i piani chiedevano. Sono difetti dei piani.

### D1. I criteri di completamento erano soddisfacibili senza che la funzione funzionasse

Il criterio della Fase 1 diceva *«il TB reale è indicizzato, la timeline scorre
fluida»*. Nessun task del piano creava una libreria, quindi **quel criterio era
impossibile da soddisfare** — e nessuno se n'è accorto, perché tutti i *task*
passavano. Il criterio non è mai stato eseguito: è stato letto e giudicato
plausibile.

**Correzione (Task 7):** ogni fase chiude con test automatici che percorrono il
viaggio dell'utente da capo a fondo. Se un criterio non è eseguibile come test,
non è un criterio.

### D2. La decomposizione seguiva gli strati architetturali, non il viaggio dell'utente

I piani sono stati scomposti per strato: repository → endpoint → interfaccia.
Tutto ciò che non è uno strato — *«l'utente ha bisogno di un modo per aggiungere
una libreria»* — cade fra le maglie. Da qui il DIFETTO 2.

**Correzione:** questo piano apre con i **viaggi utente**, e ogni task dichiara
a quale contribuisce. Un repository senza rotta è un task incompleto, non due
task separati.

### D3. La configurazione di produzione non è mai esercitata dai test

`main.rs` passa `5s`, i test passano `ZERO`, e il ramo `is_zero` salta il
comportamento. Da qui il DIFETTO 1.

**Correzione (Task 8):** i valori di produzione diventano **costanti pubbliche
in un solo posto**, usate sia da `main.rs` sia dai test. La divergenza diventa
impossibile invece che improbabile.

### D4. Nessuna asserzione di prestazione

Niente fallisce se un'operazione diventa mille volte più lenta. Le stime vivono
nelle spec come prosa.

**Correzione (Task 8):** budget espliciti nei test.

### D5. Le spec descrivono interfacce che nessun task costruisce

La spec della Fase 0 descriveva una procedura di primo avvio in **cinque passi**,
incluso *«dove sono le tue foto?»*. È stato costruito **il primo**. Lo stesso
rischio è nelle spec 4, 5 e 6, piene di pannelli descritti nel dettaglio.

**Correzione:** quando un piano copre solo una parte di ciò che la spec
descrive, deve **dichiararlo** nella sezione «Cosa NON è in questa fase».
Il silenzio non è una decisione.

---

## Global Constraints

Gli invarianti di [`/AGENTS.md`](../../../AGENTS.md), più:

- **Ogni rotta nuova richiede `AuthContext`** e passa dai repository esistenti.
  Nessuna query nuova in `keeppix-api`: se serve una query, va in `keeppix-db`.
- **`Forbidden`, mai `NotFound`**, sugli id altrui — anche se l'id non esiste.
- **Nessuna modifica alle migrazioni già applicate.** Le nuove usano prefissi a
  **quattro cifre** (oggi convivono `0009_` e `00010_`: l'ordinamento è corretto
  perché sqlx legge il numero prima del primo `_`, ma è confuso).
- **Nessun `thread::sleep` in contesto async.**
- Il bundle d'ingresso del frontend resta **sotto 150 KB gzip**.

---

## I viaggi utente che questa fase deve rendere possibili

| # | Viaggio | Task |
|---|---|---|
| **V1** | Da istanza vuota: creo l'admin, aggiungo una libreria puntando a una cartella, avvio la scansione, vedo le foto comparire in timeline | 1, 2, 3, 6 |
| **V2** | Aggiungo un secondo utente, accede e vede solo ciò che gli spetta | 4 |
| **V3** | Cestino una foto, la ritrovo nel cestino, la ripristino | 5 |
| **V4** | Una libreria diventa irraggiungibile: lo vedo, e nulla viene cancellato | 2, 3 |

---

## Task 1: La discovery non deve dormire per ogni file

**Risolve:** DIFETTO 1 · **Contribuisce a:** V1

**Files:**
- Modify: `crates/keeppix-media/src/walk.rs` — `restat_if_stable`
- Modify: `crates/keeppix-jobs/src/discover.rs` — il ciclo
- Create: `crates/keeppix-jobs/tests/discover_perf.rs`

**Interfaces:**
- `restat_if_stable(path, wait)` cambia semantica: **non dorme mai**.
  Restituisce `Settled(Metadata)` se il file è fermo da abbastanza tempo,
  `InFlight` se sembra ancora in arrivo.
- Nuova costante pubblica `SETTLED_AFTER: Duration` (default 60 s) in
  `keeppix-media`.

- [ ] **Step 1: Scrivere il test che fallisce**

`crates/keeppix-jobs/tests/discover_perf.rs`:

```rust
mod harness;

use std::time::{Duration, Instant};

/// 1.000 file con `mtime` nel passato: nessuno sta arrivando, quindi nessuno
/// deve costare un'attesa di stabilità.
///
/// Con il difetto (5 s per file) questo test impiegherebbe 83 minuti: va
/// osservato fallire per timeout, che è già la prova.
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn discovering_a_thousand_settled_files_takes_seconds_not_hours() {
    let test = harness::TestDb::start().await;
    let dir = tempfile::tempdir().unwrap();

    for i in 0..1_000 {
        let p = dir.path().join(format!("IMG_{i:04}.jpg"));
        std::fs::write(&p, b"\xFF\xD8\xFF\xE0\x00\x10JFIF\x00").unwrap();
        // mtime di due ore fa: ampiamente oltre SETTLED_AFTER.
        let old = filetime::FileTime::from_unix_time(
            chrono::Utc::now().timestamp() - 7200, 0);
        filetime::set_file_mtime(&p, old).unwrap();
    }

    let library = harness::seed_library(&test, dir.path()).await;

    let start = Instant::now();
    // CONFIGURAZIONE DI PRODUZIONE, non Duration::ZERO.
    keeppix_jobs::discover::run(
        test.db(),
        library,
        keeppix_jobs::PRODUCTION_STABILITY_WAIT,
    )
    .await
    .unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(30),
        "1.000 file assestati hanno richiesto {elapsed:?}: la discovery sta \
         dormendo per file invece di saltare i file già fermi"
    );
}

/// Gli asset devono comparire DURANTE la scansione, non solo alla fine:
/// altrimenti non c'è avanzamento e un errore al 99% perde tutto.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn assets_appear_while_the_scan_is_still_running() {
    // Avvia la discovery su 2.000 file in un task, e verifica che il conteggio
    // in `assets` sia > 0 prima che il job termini.
}

/// Un file che sta ancora arrivando non blocca la scansione: viene rimandato.
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn a_file_still_being_written_is_deferred_not_waited_for() {
    // Un file con mtime = adesso viene classificato InFlight, la discovery
    // prosegue senza dormire, e resta un job di ricontrollo in coda.
}
```

- [ ] **Step 2: Eseguirlo e osservare il fallimento**

Run: `cargo test -p keeppix-jobs --test discover_perf`
Expected: il primo test non termina entro il timeout — è la prova del difetto.
Interromperlo dopo un minuto è sufficiente; annotare nel ledger cosa si è visto.

- [ ] **Step 3: Correggere — quattro interventi, in quest'ordine**

**3a. Saltare l'attesa per i file assestati.** Un file con `mtime` più vecchio
di `SETTLED_AFTER` (60 s) non sta arrivando: un solo `stat`, nessuna attesa.
Chiude il 99,9% dei casi reali a costo zero.

```rust
pub enum Freshness { Settled(Metadata), InFlight }

/// Non dorme mai. Un file fermo da più di `SETTLED_AFTER` è assestato:
/// un solo `stat` e via. Solo i file toccati di recente sono ambigui, e
/// quelli vengono rimandati dal chiamante invece di bloccarlo.
pub fn freshness(path: &Path, settled_after: Duration) -> std::io::Result<Freshness> {
    let meta = std::fs::metadata(path)?;
    let age = meta.modified().ok()
        .and_then(|m| m.elapsed().ok())
        .unwrap_or(Duration::MAX);
    Ok(if age >= settled_after { Freshness::Settled(meta) } else { Freshness::InFlight })
}
```

**3b. Non dormire mai nel ciclo.** I file `InFlight` non bloccano la scansione:
si accodano come job a sé con `run_after = now() + 5s`. La discovery finisce, e
i pochi file in transito vengono ricontrollati dopo.

**3c. Inserire a lotti dentro il ciclo.** Batch da ~500 file, non `seen`
accumulato fino alla fine. L'utente vede le cartelle comparire, un'interruzione
non perde il lavoro, e la RAM resta costante — che su un Pi conta.

**3d. `tokio::time::sleep`** ovunque resti un'attesa in contesto async.

- [ ] **Step 4: Verificare e misurare**

Run: `cargo test -p keeppix-jobs --test discover_perf`
Expected: PASS, i tre test.

**Registrare nel ledger** il tempo reale per 1.000 file. È il numero che
sostituisce una stima mai verificata.

- [ ] **Step 5: Commit**

```bash
git commit -m "fix(jobs): stop sleeping five seconds per file during discovery"
```

---

## Task 2: `LibraryRepo` esposto

**Risolve:** DIFETTO 2 · **Contribuisce a:** V1, V4

**Files:**
- Create: `crates/keeppix-api/src/routes/libraries.rs`
- Modify: `crates/keeppix-api/src/lib.rs`, `openapi.rs`, `crates/keeppix-server/src/config.rs`

**Interfaces:**

| Metodo | Percorso | Chi | Note |
|---|---|---|---|
| `GET` | `/api/v1/libraries` | autenticato | un non-admin vede solo le proprie |
| `POST` | `/api/v1/libraries` | admin | `409` se il `root_path` è già indicizzato |
| `GET` | `/api/v1/libraries/{id}` | proprietario/admin | `Forbidden` sugli altrui |
| `PATCH` | `/api/v1/libraries/{id}` | proprietario/admin | nome, `scan_enabled`, `exclude_patterns` |
| `DELETE` | `/api/v1/libraries/{id}` | admin | **non tocca i file**, e la risposta lo dice |
| `GET` | `/api/v1/libraries/preview?path=` | admin | conteggio per estensione, senza creare nulla |

`LibraryRepo` esiste già in `crates/keeppix-db/src/libraries.rs` con i suoi
test: questo task è **solo** la superficie HTTP.

**Vincolo di sicurezza — nuovo, e non opzionale.** Senza validazione, un
endpoint che accetta `root_path` è **lettura arbitraria del filesystem del
server**: un admin potrebbe puntare una libreria a `/etc` o a `~/.ssh` e leggerne
il contenuto attraverso `/media/original/{id}`.

Aggiungere a `Config` un campo `library_roots: Vec<PathBuf>`
(env `KEEPPIX_LIBRARY_ROOTS`, default `["/photos"]`). Un `root_path` fuori da
quelle radici → `422 keeppix/path-not-allowed`. La validazione va fatta **dopo**
`canonicalize`, altrimenti `/photos/../etc` passa.

- [ ] **Step 1: Test che falliscono**

```rust
#[tokio::test]
async fn an_admin_creates_a_library_and_sees_it_listed() { /* 201, poi GET la contiene */ }

#[tokio::test]
async fn a_plain_user_cannot_create_a_library() { /* 403 */ }

#[tokio::test]
async fn a_plain_user_lists_only_its_own_libraries() { /* … */ }

#[tokio::test]
async fn probing_someone_elses_library_is_forbidden_not_not_found() { /* 403 */ }

#[tokio::test]
async fn probing_a_nonexistent_library_id_is_also_forbidden() {
    // Nessun oracolo di esistenza.
}

#[tokio::test]
async fn a_path_outside_the_allowed_roots_is_rejected() {
    // POST con root_path "/etc" → 422 keeppix/path-not-allowed
}

#[tokio::test]
async fn a_path_that_escapes_via_dotdot_is_rejected() {
    // "/photos/../etc" → 422. Va canonicalizzato PRIMA di confrontare.
}

#[tokio::test]
async fn two_libraries_cannot_share_a_root_path() { /* 409 */ }

#[tokio::test]
async fn deleting_a_library_leaves_the_files_untouched() {
    // Il conteggio dei file su disco è identico prima e dopo.
}
```

I due test sui percorsi sono i più importanti: senza, questo task **apre** una
vulnerabilità invece di chiudere un buco.

- [ ] **Step 2-4: Eseguire, implementare, verificare, committare**

---

## Task 3: Avviare e seguire una scansione

**Risolve:** DIFETTO 2 · **Contribuisce a:** V1, V4

**Files:**
- Modify: `crates/keeppix-api/src/routes/libraries.rs`
- Modify: `crates/keeppix-jobs/src/watch.rs`

**Interfaces:**

| Metodo | Percorso | Note |
|---|---|---|
| `POST` | `/api/v1/libraries/{id}/scan` | `202`; accoda `DiscoverLibrary`; idempotente via `dedup_key` |
| `GET` | `/api/v1/libraries/{id}/scan` | fase, contati, errori, ETA |

`watch::enqueue_rescan` esiste già in `crates/keeppix-jobs/src/watch.rs`: la
rotta la chiama, non reimplementa l'accodamento.

**Difetto da correggere nello stesso task.** `watch::spawn_all` legge le
librerie **solo al boot**:

```rust
/// Avvia un watcher per ogni libreria. Le librerie create dopo il boot
/// restano scoperte fino al riavvio (ponytail: 1c può rinfrescare).
pub async fn spawn_all(db: &Db, debounce: Duration) -> Result<Vec<JoinHandle<()>>, JobError> {
    let libs = LibraryRepo::new(db).list_for_scan().await?;
    ...
```

Con la creazione via API questo diventa inaccettabile: si crea una libreria e
non viene mai sorvegliata. Alla creazione va avviato il watcher corrispondente,
**senza riavviare il processo**.

- [ ] **Step 1: Test che falliscono**

```rust
#[tokio::test]
async fn starting_a_scan_creates_assets() { /* … */ }

#[tokio::test]
async fn starting_a_scan_twice_does_not_double_the_work() {
    // dedup_key: la seconda chiamata non accoda un secondo job.
}

#[tokio::test]
async fn a_library_created_after_boot_is_watched() {
    // Si crea la libreria via API, si aggiunge un file sul disco,
    // e l'asset compare senza riavviare il processo.
}

#[tokio::test]
async fn an_unreachable_library_goes_offline_and_deletes_nothing() {
    // Si indicizza, si smonta/rinomina la radice, si rilancia la scansione:
    // status='offline' e il conteggio degli asset è INVARIATO.
}
```

L'ultimo è la protezione più importante del prodotto: un disco non montato non
è una libreria svuotata.

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 4: Gestione utenti

**Risolve:** DIFETTO 2 · **Contribuisce a:** V2

**Files:**
- Create: `crates/keeppix-api/src/routes/users.rs`

| Metodo | Percorso | Chi |
|---|---|---|
| `GET` | `/api/v1/users` | admin |
| `POST` | `/api/v1/users` | admin |
| `PATCH` | `/api/v1/users/{id}` | admin, o sé stesso per nome e locale |
| `POST` | `/api/v1/users/{id}/disable` | admin |
| `POST` | `/api/v1/users/me/password` | sé stesso, **richiede la password attuale** |

**Due debiti della Fase 0 da saldare qui**, differiti proprio a quando sarebbe
esistita la gestione utenti:

1. **`map_unique_violation` scarta l'errore sqlx sottostante**
   (`crates/keeppix-db/src/users.rs`): «username già in uso» ed «email già in
   uso» producono lo stesso messaggio. Qui servono distinti, altrimenti chi
   crea un utente non sa quale campo cambiare.
2. **Disabilitare un utente non ne termina le sessioni.** Oggi `authenticate`
   fa join su `disabled_at IS NULL`, quindi un disabilitato non può *usare*
   nulla — ma la famiglia di token resta viva. Va aggiunto il comportamento e
   il test «disabilitare un utente termina le sue sessioni».

- [ ] **Step 1: Test che falliscono**, inclusi i due debiti sopra e:

```rust
#[tokio::test]
async fn changing_your_password_requires_the_current_one() { /* 403 senza */ }

#[tokio::test]
async fn changing_your_password_revokes_other_sessions() {
    // Cambiare password deve buttare fuori chi ha rubato una sessione.
}

#[tokio::test]
async fn a_plain_user_cannot_list_users() { /* 403 */ }
```

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 5: Cestino navigabile e stack

**Risolve:** DIFETTO 2 · **Contribuisce a:** V3

**Files:**
- Modify: `crates/keeppix-api/src/routes/trash.rs`
- Create: rotte per `StackRepo` (`crates/keeppix-db/src/stacks.rs`)

| Metodo | Percorso | Note |
|---|---|---|
| `GET` | `/api/v1/trash` | **paginato**, con giorni residui prima della cancellazione |
| `POST` | `/api/v1/trash/empty` | svuota subito; solo owner/admin |
| `GET` | `/api/v1/assets/{id}/stack` | membri dello stack |
| `POST` | `/api/v1/assets/{id}/stack/primary` | cambia il primario |

Oggi si può cestinare (`DELETE /assets/{id}`) e ripristinare
(`/assets/{id}/restore`), ma **non elencare il cestino**: si recupera una foto
solo conoscendone l'id, che è come non poterla recuperare.

- [ ] **Step 1: Test che falliscono**

Includere: l'elenco è paginato e non carica tutto; mostra i giorni residui;
`empty` richiede owner/admin; un utente non vede il cestino altrui.

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 6: Procedura di primo avvio completa

**Risolve:** D5 · **Contribuisce a:** V1

**Files:**
- Modify: `frontend/src/views/SetupView.vue`
- Create: `frontend/src/views/setup/LibraryStep.vue`, `ScanStep.vue`

La spec della Fase 0 descriveva cinque passi; ne esiste **uno**. Questo task
porta la procedura a coprire V1 per intero:

1. Crea l'amministratore ✅ *(esiste)*
2. **Dove sono le tue foto?** — sfoglia i percorsi consentiti, mostra
   un'anteprima di cosa verrà indicizzato (conteggio per estensione, spazio),
   usando `GET /api/v1/libraries/preview`
3. **Avvia la scansione**, con avanzamento in tempo reale via WebSocket

- [ ] **Step 1: Test vitest che falliscono**

Includere: senza libreria non si può proseguire; l'anteprima mostra i conteggi
reali; l'avanzamento si aggiorna; **un errore di rete durante la scansione non
lascia la pagina bianca** (il backend distingue `503` da `401`, il frontend deve
distinguerli).

- [ ] **Step 2-4: Implementare, verificare, committare**

Vincolo: la procedura va in un **chunk lazy** — la si usa una volta nella vita
dell'istanza.

---

## Task 7: I test del viaggio utente

**Risolve:** D1 · **Contribuisce a:** tutti

**Files:**
- Create: `crates/keeppix-api/tests/journeys.rs`

È il task che **impedisce che accada di nuovo**. Un test per viaggio, che parla
solo HTTP come farebbe un browser, contro un filesystem vero.

```rust
#[tokio::test]
#[allow(clippy::unwrap_used, clippy::expect_used)]
async fn v1_from_empty_instance_to_photos_in_the_timeline() {
    let server = TestServer::start().await;
    let archive = build_fixture_archive();   // due cartelle, sei immagini vere

    // 1. admin
    assert_eq!(post(&server, "/api/v1/setup", ADMIN).await.status(), 201);

    // 2. libreria — se l'endpoint non esiste, il test NON COMPILA
    let created = post_json(&server, "/api/v1/libraries",
        json!({"name": "Foto", "root_path": archive.path()})).await;
    assert_eq!(created.status(), 201);
    let id = created.json::<Library>().await.id;

    // 3. scansione
    assert_eq!(post(&server, &format!("/api/v1/libraries/{id}/scan"), "").await.status(), 202);

    // 4. TETTO TEMPORALE sul viaggio completo: è ciò che avrebbe fatto
    //    fallire la build sul difetto della discovery.
    wait_for_scan(&server, id, Duration::from_secs(60)).await;

    // 5. le foto sono in timeline
    let buckets: Vec<Bucket> = get_json(&server, "/api/v1/timeline/buckets").await;
    assert_eq!(buckets.iter().map(|b| b.count).sum::<i64>(), 6);

    // 6. e le miniature si scaricano davvero
    let tl: Timeline = get_json(&server, "/api/v1/timeline").await;
    let thumb = get(&server, &format!("/api/v1/media/thumb/{}", tl.assets[0].hash)).await;
    assert_eq!(thumb.status(), 200);
    assert_eq!(thumb.headers()["content-type"], "image/webp");
}
```

Più `v2_a_second_user_sees_only_what_it_should`,
`v3_trash_and_restore_round_trip`,
`v4_an_unreachable_library_never_loses_data`.

- [ ] **Step 1-4: Scrivere, verificare, committare**

---

## Task 8: Budget di prestazione e configurazione di produzione

**Risolve:** D3, D4

**Files:**
- Create: `crates/keeppix-jobs/tests/production_config.rs`
- Modify: `crates/keeppix-jobs/src/lib.rs` — le costanti
- Modify: `crates/keeppix-server/src/main.rs` — usa le costanti

**8a. Costanti condivise.** I valori di produzione vivono in **un solo posto**:

```rust
// crates/keeppix-jobs/src/lib.rs
/// Attesa prima di riconsiderare un file che sembra ancora in arrivo.
/// Usata da `main.rs` e dai test: se divergessero, un difetto potrebbe
/// vivere solo nel codice spedito — è già successo.
pub const PRODUCTION_STABILITY_WAIT: Duration = Duration::from_secs(5);
pub const PRODUCTION_SETTLED_AFTER: Duration = Duration::from_secs(60);
pub const PRODUCTION_BATCH_SIZE: usize = 500;
```

`main.rs` le usa invece di ripetere i letterali. Un test verifica che il
dispatcher costruito come in produzione si comporti entro i budget.

**8b. Budget.** Soglie larghe (3-5× il misurato), pensate per il bersaglio
dichiarato (Raspberry Pi 5):

| Operazione | Budget |
|---|---|
| discovery di 1.000 file assestati | < 30 s |
| estrazione preview RAW | < 50 ms per file |
| `GET /timeline/buckets` con 10.000 asset | < 200 ms |
| `GET /timeline` (una pagina) | < 300 ms |
| `GET /libraries` con 20 librerie | < 100 ms |

- [ ] **Step 1-4: Scrivere, verificare, committare**

---

## Task 9: Prova sul campo automatizzata

**Risolve:** il divario fra fixture e realtà

**Files:**
- Modify: `scripts/field-test.sh`
- Create: `docs/FIELD-TEST.md`

Lo script **esiste già** ed è quello che ha trovato il DIFETTO 1. Va portato a
strumento di prima classe:

- **usa gli endpoint invece di `INSERT` in SQL** — dopo i Task 2 e 3 è
  possibile, e la prova diventa una verifica del *prodotto*, non del database;
- confronta con i budget del Task 8 ed **esce diverso da zero** quando li sfora;
- documenta come rilanciarlo in `docs/FIELD-TEST.md`.

**Da eseguire davvero** alla fine della fase, sull'archivio reale
(1.558 ARW, 36 GB), con i numeri nel ledger.

- [ ] **Step 1-3: Adeguare, eseguire, registrare le misure**

---

## Criteri di completamento

Ognuno è **eseguibile**: se non lo è, non è un criterio.

- [ ] `cargo test --workspace -- --test-threads=1` verde; clippy e fmt puliti.
- [ ] I quattro test di viaggio (V1-V4) passano.
- [ ] I budget del Task 8 sono verdi.
- [ ] **Una persona, da istanza vuota e usando solo il browser**, crea l'admin,
      aggiunge una libreria, avvia la scansione e vede le foto in timeline.
      Nessun SQL, nessun riavvio del container.
- [ ] `scripts/field-test.sh` gira sull'archivio reale entro i budget e conferma
      che l'archivio è intatto.
- [ ] **Misure nel ledger**: tempo di discovery su 1.000 file, throughput hash,
      ms per derivato, copertura reale delle preview RAW sui 1.558 ARW.
- [ ] Bundle d'ingresso del frontend sotto 150 KB gzip.
- [ ] CI verde sulla PR.

## Cosa NON è in questa fase

Permessi e condivisione (Fase 3), mappa (Fase 4), WebDAV (Fase 5), video e
backup (Fase 6).

**Dichiarato esplicitamente, perché il silenzio non è una decisione:** le
impostazioni di sistema da interfaccia — formato dei derivati, profili
energetici, mappe offline, backup — sono descritte nelle spec delle fasi
rispettive e **non** vengono costruite qui. Chi esegue questa fase non deve
aggiungerle.
