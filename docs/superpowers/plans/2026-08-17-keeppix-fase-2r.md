# Keeppix Fase 2R — Rimedio: usabilità, prestazioni, e i buchi di processo

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rendere Keeppix utilizzabile da una persona senza toccare il database, riportare la scansione da 11 giorni a minuti, e chiudere i difetti di processo che hanno permesso a entrambi i problemi di attraversare tre fasi di verde.

**Architecture:** Nessuna decisione architetturale nuova. Si costruiscono le superfici HTTP mancanti sopra repository che esistono già e sono testati, si corregge un difetto di prestazioni localizzato, e si aggiungono tre classi di test che oggi non esistono: percorso utente end-to-end, budget di prestazione, e configurazione di produzione.

**Perché esiste questa fase:** vedi «L'analisi» qui sotto. In breve: dopo tre fasi tutte verdi, **il prodotto non era usabile da una persona**. Non è un difetto di esecuzione — Cursor ha implementato correttamente ciò che i piani chiedevano. È un difetto dei piani, cioè mio.

---

## L'analisi — perché è successo

Cinque difetti di processo, in ordine di gravità. Ognuno ha prodotto danno misurabile.

### D1. I criteri di completamento erano soddisfacibili senza che la funzione funzionasse

Il criterio della Fase 1 diceva: *«il TB reale è indicizzato, la timeline scorre fluida»*. Nessun task del piano creava una libreria, quindi **quel criterio era impossibile da soddisfare** — e nessuno se n'è accorto, perché tutti i *task* passavano.

Il criterio non è mai stato **eseguito**: è stato letto e considerato plausibile.

**Correzione:** ogni fase chiude con un test automatico che percorre il viaggio dell'utente da capo a fondo. Se il criterio non è eseguibile come test, non è un criterio.

### D2. La decomposizione seguiva i livelli architetturali, non il viaggio dell'utente

Ho scomposto i piani per strato: repository → endpoint → interfaccia. Tutto ciò che non è uno strato — «l'utente ha bisogno di un modo per aggiungere una libreria» — cade fra le maglie.

Il livello dati di Keeppix è completo e corretto. Mancano cinque superfici HTTP i cui repository esistono, sono testati e non sono raggiungibili da nessuno.

**Correzione:** ogni piano di fase apre con l'elenco dei **viaggi utente** che la fase deve rendere possibili, e ogni task dichiara a quale viaggio contribuisce. Un repository senza rotta è un task incompleto, non due task.

### D3. La configurazione di produzione non è mai esercitata dai test

`main.rs` passa `stability_wait: Duration::from_secs(5)`. Tutti i test passano `Duration::ZERO`, e la funzione ha un ramo `if !wait.is_zero()` che **salta il sonno**. Il codice testato non è quello spedito.

Risultato: 5 secondi × 1.558 file = **2 ore e 10 minuti** di scansione, misurati sul campo. Sui 200.000 file dell'archivio reale sarebbero **11 giorni**. La spec prometteva 3 minuti.

**Correzione:** un test che costruisce il dispatcher **con la configurazione di produzione** e verifica che si comporti entro i limiti attesi.

### D4. Nessuna asserzione di prestazione

Niente fallisce se un'operazione diventa mille volte più lenta. Le stime vivono nelle spec come prosa, non come soglie verificate.

**Correzione:** budget espliciti nei test per le operazioni che stanno su un percorso caldo, con soglie larghe (3-5× il misurato) per non essere fragili sulle macchine di CI.

### D5. Le spec descrivono interfacce che nessun task costruisce

La spec della Fase 0 descriveva una procedura guidata di primo avvio in cinque passi, incluso *«dove sono le tue foto?»* con la creazione della prima libreria. È stato costruito solo il primo passo, la creazione dell'admin.

Non è un caso isolato: le spec delle fasi 4, 5 e 6 contengono pannelli e wizard descritti nel dettaglio che nessun piano trasformerà in task se non lo si dice esplicitamente.

**Correzione:** quando un piano copre solo una parte di ciò che la spec descrive, deve **dichiararlo** nella sezione «Cosa NON è in questa fase». Il silenzio non è una decisione.

---

## Global Constraints

Gli invarianti di [`/AGENTS.md`](../../../AGENTS.md), più:

- **Ogni rotta nuova richiede `AuthContext`** e passa dai repository esistenti. Nessuna query nuova in `keeppix-api`.
- **`Forbidden`, mai `NotFound`**, sugli id altrui.
- **Nessuna modifica alle migrazioni già applicate.** Le nuove usano prefissi a quattro cifre.
- **Nessun `thread::sleep` in contesto async.** Se serve attendere, `tokio::time::sleep`, o meglio si differisce il job.

---

## I viaggi utente che questa fase deve rendere possibili

Ogni task dichiara a quale contribuisce. A fine fase ognuno è coperto da un test end-to-end.

| # | Viaggio |
|---|---|
| **V1** | Da istanza vuota: creo l'admin, aggiungo una libreria puntando a una cartella, avvio la scansione, vedo le foto comparire in timeline |
| **V2** | Aggiungo un secondo utente, lui accede e vede solo ciò che gli spetta |
| **V3** | Cestino una foto, la ritrovo nel cestino, la ripristino |
| **V4** | Una libreria diventa irraggiungibile: lo vedo, e nulla viene cancellato |

---

## Task 1: La scansione non deve dormire per ogni file

**Contribuisce a:** V1

**Files:**
- Modify: `crates/keeppix-media/src/walk.rs`, `crates/keeppix-jobs/src/discover.rs`
- Create: test di prestazione in `crates/keeppix-jobs/tests/discover_perf.rs`

**Il difetto**, misurato sul campo il 2026-08-16: `restat_if_stable` fa
`std::thread::sleep(wait)` fra i due `stat`, ed è chiamata **dentro il ciclo di
discovery, un file alla volta**, con `wait = 5s` in produzione. Osservato: 650
secondi trascorsi, zero asset creati, job ancora `running`.

Aggravanti: `std::thread::sleep` in un job asincrono blocca il thread del
worker, non solo quel job; e gli asset sono inseriti **solo a fine ciclo**,
quindi non c'è avanzamento visibile e un errore al 99% perde tutto.

- [ ] **Step 1: Scrivere il test che fallisce**

```rust
// crates/keeppix-jobs/tests/discover_perf.rs
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn discovering_a_thousand_settled_files_takes_seconds_not_hours() {
    // 1.000 file con mtime nel passato: nessuno sta arrivando, quindi
    // nessuno deve costare un'attesa di stabilità.
    let dir = tempfile::tempdir().unwrap();
    for i in 0..1_000 {
        let p = dir.path().join(format!("IMG_{i:04}.jpg"));
        std::fs::write(&p, b"\xFF\xD8\xFF\xE0 finto jpeg").unwrap();
        // mtime vecchio: filetime, oppure si accetta il default e si fa
        // dipendere la soglia dall'età minima configurata.
        filetime::set_file_mtime(&p, filetime::FileTime::from_unix_time(1_600_000_000, 0)).unwrap();
    }

    let start = std::time::Instant::now();
    // Configurazione DI PRODUZIONE, non Duration::ZERO.
    discover::run(&db, library_id, PRODUCTION_STABILITY_WAIT).await.unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "1.000 file assestati hanno richiesto {elapsed:?}: la discovery sta \
         dormendo per file invece di saltare i file già fermi"
    );
}
```

Questo test **deve fallire prima della correzione**: con 5s per file impiegherebbe 83 minuti.

- [ ] **Step 2: Eseguirlo e osservare il fallimento** (o interromperlo dopo un minuto, che è già la prova)

- [ ] **Step 3: Correggere**

Quattro interventi, in quest'ordine:

1. **Saltare l'attesa per i file assestati.** Un file con `mtime` più vecchio di
   una soglia (default 60 s) non sta arrivando: un solo `stat`, nessuna attesa.
   Chiude il 99,9% dei casi reali a costo zero.
2. **Non dormire mai nel ciclo.** I file che *sembrano* freschi non bloccano la
   scansione: si accodano come job a sé con `run_after = now() + 5s`. La
   discovery finisce, e i pochi file in transito vengono ricontrollati dopo.
3. **Inserire a lotti dentro il ciclo** (batch da ~500), non accumulare tutto
   in `seen` e scrivere alla fine. L'utente vede le cartelle comparire, e
   un'interruzione non perde il lavoro fatto.
4. **`tokio::time::sleep`** ovunque resti un'attesa in contesto async.

- [ ] **Step 4: Verificare, e misurare**

Run: `cargo test -p keeppix-jobs --test discover_perf`
Registrare nel ledger il tempo reale per 1.000 file.

- [ ] **Step 5: Commit**

```bash
git commit -m "fix(jobs): stop sleeping five seconds per file during discovery"
```

---

## Task 2: `LibraryRepo` esposto — creare, elencare, modificare una libreria

**Contribuisce a:** V1, V4

**Files:**
- Create: `crates/keeppix-api/src/routes/libraries.rs`
- Modify: `crates/keeppix-api/src/lib.rs`, `openapi.rs`

**Interfaces:**

| Metodo | Percorso | Chi | Note |
|---|---|---|---|
| `GET` | `/api/v1/libraries` | autenticato | un non-admin vede solo le proprie |
| `POST` | `/api/v1/libraries` | admin | `409` se il `root_path` è già indicizzato |
| `GET` | `/api/v1/libraries/{id}` | proprietario/admin | `Forbidden` sugli altrui |
| `PATCH` | `/api/v1/libraries/{id}` | proprietario/admin | nome, `scan_enabled`, `exclude_patterns` |
| `DELETE` | `/api/v1/libraries/{id}` | admin | **non tocca i file**; va detto nella risposta |

Il repository esiste già e ha i test: questo task è **solo** la superficie.

- [ ] **Step 1: Test end-to-end che fallisce**

```rust
#[tokio::test]
async fn an_admin_creates_a_library_and_sees_it_listed() { /* … */ }

#[tokio::test]
async fn a_plain_user_cannot_create_a_library() { /* 403 */ }

#[tokio::test]
async fn probing_someone_elses_library_is_forbidden_not_not_found() { /* … */ }

#[tokio::test]
async fn creating_a_library_on_a_path_outside_the_configured_root_is_rejected() {
    // Un admin non deve poter puntare una libreria a /etc o a ~/.ssh.
    // Il percorso va validato contro una radice consentita.
}
```

L'ultimo test è quello che conta: senza validazione del percorso, un endpoint
di creazione libreria è una lettura arbitraria del filesystem del server.

- [ ] **Step 2-4: Implementare, verificare, committare**

**Vincolo di sicurezza**: `root_path` deve stare dentro una radice consentita,
configurabile (`KEEPPIX_LIBRARY_ROOTS`, default `/photos`). Fuori da lì, `422`.

---

## Task 3: Avviare e seguire una scansione

**Contribuisce a:** V1, V4

**Files:**
- Modify: `crates/keeppix-api/src/routes/libraries.rs`
- Modify: `crates/keeppix-jobs/src/watch.rs`

**Interfaces:**

| Metodo | Percorso | Note |
|---|---|---|
| `POST` | `/api/v1/libraries/{id}/scan` | accoda `DiscoverLibrary`; idempotente via `dedup_key` |
| `GET` | `/api/v1/libraries/{id}/scan` | stato: fase, contati, errori, ETA |

**Difetto da correggere nello stesso task:** `watch::spawn_all` legge le
librerie **solo al boot**, e il commento nel codice lo dice: *«le librerie
create dopo il boot restano scoperte fino al riavvio»*. Con la creazione via
API questo diventa inaccettabile — si crea una libreria e non viene mai
sorvegliata.

Correzione: alla creazione di una libreria si avvia il watcher corrispondente,
senza riavviare il processo.

- [ ] **Step 1: Test che falliscono**

Devono coprire: la scansione parte e crea asset; chiamarla due volte non
raddoppia il lavoro; **una libreria creata dopo il boot viene sorvegliata**;
lo stato riflette l'avanzamento reale.

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 4: Gestione utenti

**Contribuisce a:** V2

**Files:**
- Create: `crates/keeppix-api/src/routes/users.rs`

| Metodo | Percorso | Chi |
|---|---|---|
| `GET` | `/api/v1/users` | admin |
| `POST` | `/api/v1/users` | admin |
| `PATCH` | `/api/v1/users/{id}` | admin, o sé stesso per nome/locale |
| `POST` | `/api/v1/users/{id}/disable` | admin |
| `POST` | `/api/v1/users/me/password` | sé stesso, richiede la password attuale |

**Debiti della Fase 0 da saldare qui**, erano stati differiti proprio a quando
sarebbe esistita la gestione utenti:

- **`map_unique_violation` scarta l'errore sqlx sottostante**: «username preso»
  ed «email presa» danno lo stesso messaggio. Qui servono distinti.
- **Disabilitare un utente deve terminarne le sessioni.** Oggi `authenticate`
  fa join su `disabled_at IS NULL`, quindi non può *usare* nulla — ma la
  famiglia di token resta viva. Va aggiunto il test «disabilitare un utente
  termina le sue sessioni».

- [ ] **Step 1: Test che falliscono**, inclusi i due debiti sopra

- [ ] **Step 2-4: Implementare, verificare, committare**

---

## Task 5: Cestino navigabile e stack

**Contribuisce a:** V3

**Files:**
- Modify: `crates/keeppix-api/src/routes/trash.rs`
- Create: rotte per `StackRepo`

| Metodo | Percorso | Note |
|---|---|---|
| `GET` | `/api/v1/trash` | elenco, con giorni residui prima della cancellazione |
| `POST` | `/api/v1/trash/empty` | svuota subito; solo owner/admin |
| `GET` | `/api/v1/assets/{id}/stack` | membri dello stack |
| `POST` | `/api/v1/assets/{id}/stack/primary` | cambia il primario |

Oggi si può cestinare (`DELETE /assets/{id}`) e ripristinare
(`/assets/{id}/restore`), ma **non elencare il cestino**: si può recuperare una
foto solo se se ne conosce l'id, che è come non poterla recuperare.

- [ ] **Step 1-4: TDD, implementare, verificare, committare**

---

## Task 6: Procedura di primo avvio completa

**Contribuisce a:** V1

**Files:**
- Modify: `frontend/src/views/SetupView.vue`
- Create: `frontend/src/views/setup/LibraryStep.vue`, `ScanStep.vue`

La spec della Fase 0 descriveva cinque passi; ne esiste uno. Questo task porta
la procedura a coprire il viaggio V1 per intero:

1. Crea l'amministratore ✅ *(esiste)*
2. **Dove sono le tue foto?** — sfoglia i percorsi consentiti, mostra
   un'anteprima di cosa verrà indicizzato (conteggio per tipo, spazio)
3. **Avvia la scansione**, con avanzamento in tempo reale via WebSocket

Il passo 2 richiede un endpoint di anteprima: `GET /api/v1/libraries/preview?path=…`,
che conta i file per estensione **senza** creare nulla. Vincolato alla stessa
allowlist di radici del Task 2.

- [ ] **Step 1-4: TDD (vitest), implementare, verificare, committare**

---

## Task 7: Il test del viaggio utente — la rete che mancava

**Contribuisce a:** tutti

**Files:**
- Create: `crates/keeppix-api/tests/journeys.rs`

Questo è il task che **impedisce che accada di nuovo**. Un test per viaggio,
che parla solo HTTP come farebbe un browser, contro un filesystem vero.

```rust
#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn v1_from_empty_instance_to_photos_in_the_timeline() {
    let server = TestServer::start().await;

    // Un archivio finto ma reale su disco: due cartelle, sei immagini.
    let archive = build_fixture_archive();

    // 1. setup admin
    assert_eq!(post(&server, "/api/v1/setup", ADMIN).await.status(), 201);

    // 2. creo la libreria — se questo endpoint non esiste, il test non compila
    let lib = post_json(&server, "/api/v1/libraries",
        json!({"name": "Foto", "root_path": archive.path()})).await;
    assert_eq!(lib.status(), 201);

    // 3. avvio la scansione
    assert_eq!(post(&server, &format!("/api/v1/libraries/{id}/scan"), "").await.status(), 202);

    // 4. attendo che finisca, con un tetto: se supera il budget, fallisce
    wait_for_scan(&server, id, Duration::from_secs(60)).await;

    // 5. le foto sono in timeline
    let buckets: Vec<Bucket> = get_json(&server, "/api/v1/timeline/buckets").await;
    assert_eq!(buckets.iter().map(|b| b.count).sum::<i64>(), 6);

    // 6. e le miniature si scaricano
    let timeline: Timeline = get_json(&server, "/api/v1/timeline").await;
    let thumb = get(&server, &format!("/api/v1/media/thumb/{}", timeline.assets[0].hash)).await;
    assert_eq!(thumb.status(), 200);
    assert_eq!(thumb.headers()["content-type"], "image/webp");
}
```

Più V2, V3, V4 nella stessa forma.

**Il punto 4 è la parte che vale**: un tetto temporale sul viaggio completo. È
esattamente ciò che avrebbe fatto fallire la build sul difetto della discovery,
mesi prima che lo trovasse una prova manuale.

- [ ] **Step 1-4: Scrivere, verificare che passino, committare**

---

## Task 8: Budget di prestazione e configurazione di produzione

**Contribuisce a:** impedire il ritorno di D3 e D4

**Files:**
- Create: `crates/keeppix-jobs/tests/production_config.rs`
- Modify: i test dove serve una soglia

Due classi di test che oggi non esistono:

**Configurazione di produzione.** Un test che costruisce il dispatcher con
**gli stessi valori di `main.rs`** e verifica che si comporti. Il difetto della
discovery esisteva perché `main.rs` e i test usavano valori diversi, e nessuno
confrontava i due.

Rendere i valori di produzione **costanti pubbliche** in un solo posto, usate
sia da `main.rs` sia dai test: la divergenza diventa impossibile invece che
improbabile.

**Budget.** Soglie larghe (3-5× il misurato) sulle operazioni del percorso
caldo:

| Operazione | Budget |
|---|---|
| discovery di 1.000 file assestati | < 30 s |
| estrazione preview RAW | < 50 ms per file |
| `GET /timeline/buckets` con 10.000 asset | < 200 ms |
| `GET /timeline` (una pagina) | < 300 ms |

Larghe di proposito: devono cogliere una regressione di ordine di grandezza,
non oscillare con il carico del runner.

- [ ] **Step 1-4: Scrivere, verificare, committare**

---

## Task 9: Prova sul campo, automatizzata

**Contribuisce a:** chiudere il divario fra fixture e realtà

**Files:**
- Modify: `scripts/field-test.sh`
- Create: `docs/FIELD-TEST.md`

Lo script esiste già (`scripts/field-test.sh`) ed è quello che ha trovato il
difetto. Va portato a strumento di prima classe:

- usa gli endpoint invece di `INSERT` in SQL — dopo i Task 2 e 3 è possibile,
  e la prova diventa una verifica del **prodotto**, non del database;
- misura e confronta con i budget del Task 8, uscendo con codice diverso da
  zero quando li sfora;
- documenta come rilanciarlo in `docs/FIELD-TEST.md`.

**Da eseguire davvero**, alla fine della fase, sull'archivio reale
(`/Volumes/NVME/Immagini/…`, 1.558 ARW, 36 GB), e i numeri vanno nel ledger.

- [ ] **Step 1-3: Adeguare, eseguire, registrare le misure**

---

## Criteri di completamento

Ognuno è **eseguibile**: se non lo è, non è un criterio.

- [ ] `cargo test --workspace -- --test-threads=1` verde, clippy e fmt puliti.
- [ ] I quattro test di viaggio (V1-V4) passano.
- [ ] I budget di prestazione del Task 8 sono verificati e verdi.
- [ ] **Una persona, partendo da un'istanza vuota e usando solo il browser**,
      crea l'admin, aggiunge una libreria, avvia la scansione e vede le foto in
      timeline. Nessun SQL, nessun riavvio del container.
- [ ] `scripts/field-test.sh` gira sull'archivio reale, entro i budget, e
      conferma che l'archivio è intatto.
- [ ] **Misure registrate nel ledger**: tempo di discovery, throughput hash,
      ms per derivato, copertura reale delle preview RAW sui 1.558 ARW.
- [ ] CI verde sulla PR.

## Cosa NON è in questa fase

Permessi e condivisione (Fase 3), mappa (Fase 4), WebDAV (Fase 5), video e
backup (Fase 6). Le impostazioni di sistema da interfaccia (formato derivati,
profili energetici, mappe offline) restano fuori: sono descritte nelle spec
delle fasi rispettive, e questa fase le nomina qui perché **il silenzio non è
una decisione** — vedi D5.
