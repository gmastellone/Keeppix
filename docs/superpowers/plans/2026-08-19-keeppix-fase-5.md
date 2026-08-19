# Keeppix Fase 5 — WebDAV e upload riprendibili

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) o superpowers:executing-plans per eseguire questo piano task per task. Gli step usano la sintassi checkbox (`- [ ]`) per il tracciamento.

**Goal:** Due porte sullo stesso magazzino. Upload riprendibile da browser/telefono via **tus 1.0** (pre-check per hash, chunk con checksum, ripresa dal punto esatto), e **WebDAV** montabile come disco per sincronizzazione da Finder/Explorer/rclone. Entrambe scrivono file veri in cartelle vere, entrambe passano dagli stessi controlli di permesso della Fase 3, entrambe finiscono nella stessa coda di indicizzazione — a priorità alta, non nei 15 minuti del watcher.

**Architecture:** Le due porte condividono tre fondamenta già esistenti, non da reinventare: `PermissionRepo::effective_role`/`assert_can_edit_assets` (Fase 3) per ogni scrittura, `TrashRepo::choose` (Fase 2) per ogni cancellazione, `JobPriority::High` (già nell'enum, mai usato finora) per l'indicizzazione immediata. L'unica cosa realmente nuova nello strato permessi è l'autenticazione: WebDAV non usa il cookie di sessione ma **app-password dedicate**, che oggi non esistono in nessuna forma.

**Tech Stack:** Protocollo tus 1.0 (implementazione propria, non un crate — il protocollo è ~5 endpoint) · Axum con matching manuale dei metodi WebDAV (`PROPFIND`/`MKCOL`/`MOVE`/`COPY`/`LOCK`/`UNLOCK` non sono verbi standard) · `quick-xml` per il PROPFIND in streaming · blake3 per i checksum (già usato in Fase 2 per l'hashing asset)

**Spec:** [`../specs/fase-5-webdav-upload.md`](../specs/fase-5-webdav-upload.md) — **leggerla prima**; se piano e spec divergono, vince la spec
**Roadmap:** [`2026-08-13-keeppix-roadmap.md`](2026-08-13-keeppix-roadmap.md) — Fase 5 dipende da `FolderRepo`/watcher/coda job (Fase 1) e dai permessi (Fase 3), entrambe già in codice

---

## Cosa esiste già — non è tutto da zero

Verificato sul codice attuale, non sulla spec:

- **`FolderRepo`** (`crates/keeppix-db/src/folders.rs`) ha già `ensure_child`, `ensure_path`, **`move_subtree`** — riusabili direttamente per `MKCOL`/`MOVE`. Non esiste `copy_subtree`: `COPY` è nuovo.
- **Il watcher fa solo rescan dell'intera libreria**, mai un ingest mirato: `crates/keeppix-jobs/src/watch.rs` debounce → `enqueue_rescan` → `JobKind::DiscoverLibrary` a `JobPriority::Background`, dedup `discover:{library_id}`. Conferma esattamente il problema che la spec descrive: un upload non può passare da lì, aspetterebbe fino a 15 minuti.
- **`JobPriority::High` esiste già nell'enum** (`Interactive=0, High=1, Visible=2, Background=3`, `crates/keeppix-domain/src/job.rs`) ma **non è mai usato da nessun chiamante oggi**. Nessuna migrazione di schema serve: basta chiamare `JobRepo::enqueue(kind, payload, JobPriority::High, dedup_key)` dopo la finalizzazione di un upload.
- **Esiste già un upload ospite, ma è un meccanismo diverso e va tenuto separato, non sostituito.** `POST /share/{token}/uploads` (`crates/keeppix-api/src/routes/share.rs:390-436`) scrive l'intero corpo della richiesta in un colpo solo (`write_body_capped`, nessun concetto di offset/ripresa), rifiuta duplicati per nome con 409, e alla fine crea l'asset con `status='discovered'` in una coda di moderazione (`GuestUploadRepo`, approvazione manuale) — **nessun job di indicizzazione viene accodato**. Serve ad altro: un ospite senza account che lascia foto per approvazione, non l'upload riprendibile della Fase 5. I due coesistono: `Actor::ShareLink` per l'uno, utente autenticato (o guest via `share_link_id` sulle sessioni tus) per l'altro.
- **`share_links` ha già `upload_quota_bytes` e `allow_upload`** (`migrations/0017_share_links.sql`) — riusabili per le sessioni tus aperte da un link condiviso, non solo per l'upload ospite esistente.
- **I permessi sono già pronti**: `PermissionRepo::effective_role(ctx, folder_id)` e `assert_can_edit_assets(ctx, asset_ids)` (`crates/keeppix-db/src/permissions.rs`) sono la stessa porta usata oggi dal flusso di cancellazione normale. Un handler WebDAV `PUT`/`MKCOL` chiama questi, non reinventa un controllo permessi.
- **`TrashRepo::choose(ctx, asset_id, DiskAction::Trashed)`** (`crates/keeppix-db/src/trash.rs:85-90`) è la funzione esatta che `DELETE` WebDAV deve chiamare — stessa regola "solo owner/admin, sempre nel cestino" della Fase 3, zero codice permessi nuovo.
- **App-password: non esiste in nessuna forma.** Nessuna tabella, nessun middleware Basic Auth, nessun concetto di credenziale scoped separata dal cookie di sessione. Va progettata da zero (Task 4).
- **Nessun codice WebDAV/tus esiste** — verificato con una ricerca su tutto il repo. Due commenti lo anticipano già: `crates/keeppix-api/src/csrf.rs:20-24` e `crates/keeppix-api/src/lib.rs:303` notano che WebDAV/tus vivranno **fuori da `/api/v1`** e avranno bisogno di una deroga al middleware CSRF (quei client non mandano l'header `x-keeppix-client`).
- **Il router è una catena piatta** (`crates/keeppix-api/src/lib.rs`, `Router::new().route(...)`) con `axum::routing::{get,post,patch,delete}` — nessuno di questi copre `PROPFIND`/`MKCOL`/`MOVE`/`COPY`/`LOCK`/`UNLOCK`. Serve `axum::routing::on(MethodFilter::..., handler)` o un match manuale sul metodo dentro un handler unico per `/dav/{*path}` — **da verificare contro la versione di axum nel `Cargo.toml`** prima di assumere quale via è disponibile.
- **`quick-xml` è dichiarato solo in `keeppix-media/Cargo.toml`**, non a livello di workspace — va aggiunto esplicitamente a `keeppix-api` per il PROPFIND in streaming.
- **`DefaultBodyLimit::disable()` per-rotta è già un pattern consolidato** (`crates/keeppix-api/src/lib.rs:298`, usato dall'upload ospite) — le nuove rotte `/api/v1/upload/*` e `/dav/*` lo useranno allo stesso modo. `docs/DEPLOY.md` documenta `proxy_read_timeout` per il websocket ma **non** `client_max_body_size` — va aggiunto.

---

## Global Constraints

Valgono per **ogni** task. Sono gli invarianti di [`/AGENTS.md`](../../../AGENTS.md), più quelli specifici di questa fase.

- **Rust edition 2024, toolchain 1.88.0.**
- **`keeppix-db` è l'unico crate con SQL.**
- **Ogni metodo di repository che legge dati di un utente prende un `AuthContext` come primo parametro** — eccezione dichiarata: gli handler WebDAV/tus per un attore `ShareLink` guest passano l'`Actor` corrispondente, non un `AuthContext` di utente registrato, coerente con come `share.rs` già fa oggi.
- **`Forbidden`, mai `NotFound`**, quando si sonda un id/percorso altrui.
- **Query sempre parametrizzate.**
- **Nessun `unwrap()`/`expect()` in produzione.**
- Clippy `all` + `pedantic` a warn, `-D warnings` pulito. `cargo fmt --check` pulito.
- **Commit convenzionali in inglese**, uno per unità logica.

### Specifiche della Fase 5

- **Un file caricato non tocca mai la cartella finale prima di essere verificato per intero.** Temporaneo → hash completo verificato → verifica di decodificabilità → `rename()` atomico. I temporanei vivono in `.keeppix-tmp/` **dentro la stessa libreria** (stesso filesystem, `rename()` istantaneo anche su file da 2 GB).
- **Mai una sovrascrittura silenziosa.** Stesso nome e stesso hash → duplicato, si salta e si segnala. Stesso nome, contenuto diverso → si salva con suffisso (`IMG_1234_1.ARW`) e si segnala.
- **L'offset di un upload lo dichiara sempre il server**, mai il client. Dopo qualunque disconnessione, `HEAD` dice da dove riprendere.
- **`DELETE` via WebDAV va sempre nel cestino**, mai una cancellazione diretta — il protocollo non ha modo di fare domande, e un trascinamento per sbaglio nel cestino del Finder non deve essere irreversibile.
- **WebDAV non è una scorciatoia per aggirare i permessi della Fase 3.** Ogni scrittura passa dagli stessi controlli della web app.
- **Le app-password non sono mai la password di login.** Client WebDAV la salvano in chiaro o quasi: deve essere un segreto separato, revocabile individualmente, con `last_used_at`.
- **`PROPFIND` legge dal database, mai dal filesystem con `stat()` a raffica.** È l'ottimizzazione che decide se Finder va in timeout su 40.000 file o risponde in 40 ms.

---

## Struttura dei file

```
crates/keeppix-domain/src/
├── upload.rs            NEW  UploadSession, ChunkChecksum, CollisionOutcome
├── webdav.rs             NEW  DavLock, DavDepth, DavMethod (mirror dei metodi HTTP non standard)
└── credential.rs         NEW  AppPassword, AppPasswordSecret (mai serializzato dopo la creazione)

crates/keeppix-db/
├── migrations/
│   ├── 0026_upload_sessions.sql   NEW  upload_sessions
│   ├── 0027_app_passwords.sql     NEW  app_passwords
│   └── 0028_dav_locks.sql         NEW  dav_locks
├── src/
│   ├── uploads.rs         NEW  UploadSessionRepo — crea, avanza offset, finalizza
│   ├── credentials.rs     NEW  AppPasswordRepo — crea, verifica, revoca, elenca
│   └── dav_locks.rs       NEW  DavLockRepo — LOCK/UNLOCK/scadenza

crates/keeppix-jobs/src/
└── tmp_cleanup.rs         NEW  job che ripulisce `.keeppix-tmp/` oltre 7 giorni

crates/keeppix-api/
├── Cargo.toml              MOD  aggiunge quick-xml
├── src/
│   ├── routes/upload.rs    NEW  pre-check, create, HEAD, PATCH, finalizzazione tus
│   ├── routes/credentials.rs NEW  CRUD app-password (autenticato, cookie di sessione)
│   ├── dav/
│   │   ├── mod.rs          NEW  dispatch sul metodo, autenticazione Basic
│   │   ├── propfind.rs     NEW  XML in streaming dal DB
│   │   ├── write.rs        NEW  PUT, MKCOL, MOVE, COPY
│   │   ├── delete.rs       NEW  DELETE → TrashRepo::choose
│   │   └── lock.rs         NEW  LOCK, UNLOCK
│   └── lib.rs               MOD  monta `/dav/{*path}`, deroga CSRF, DefaultBodyLimit::disable()

frontend/src/
├── stores/upload.ts          NEW  sessioni tus, coda, ripresa dopo refresh
├── components/UploadPanel.vue NEW  persistente, minimizzabile
├── views/settings/WebdavSetupView.vue NEW  wizard, generazione app-password, indicatore live
└── sw.js (o equivalente PWA)  MOD  Share Target
```

**Ordine dei task:** 1 → 2 → 3 → (4, 5 in parallelo) → 6 → 7 → 8 → 9 → 10. I task 1-3 (tus, indicizzazione a priorità alta, pannello upload) sono un sottosistema completo e utile da solo, indipendente da WebDAV — se il tempo stringe, la fase può fermarsi lì e restare comunque una consegna coerente.

---

## Task 1: Sessioni di upload tus — schema e protocollo

**Files:**
- Create: `crates/keeppix-db/migrations/0026_upload_sessions.sql`
- Create: `crates/keeppix-domain/src/upload.rs`
- Create: `crates/keeppix-db/src/uploads.rs`, `crates/keeppix-db/tests/uploads.rs`
- Create: `crates/keeppix-api/src/routes/upload.rs`, `crates/keeppix-api/tests/upload.rs`
- Modify: `crates/keeppix-api/src/lib.rs` (wiring + `DefaultBodyLimit::disable()` sulle rotte PATCH)

**Lo schema** (dalla spec §1.5, invariato, verificato compatibile):

```sql
CREATE TABLE upload_sessions (
    id                uuid PRIMARY KEY,
    user_id           uuid REFERENCES users(id) ON DELETE CASCADE,
    share_link_id     uuid REFERENCES share_links(id) ON DELETE CASCADE,
    target_folder_id  uuid NOT NULL REFERENCES folders(id),
    filename          text NOT NULL,
    expected_size     bigint NOT NULL,
    expected_hash     bytea,
    received_bytes    bigint NOT NULL DEFAULT 0,
    temp_path         text NOT NULL,
    client_mtime      timestamptz,
    expires_at        timestamptz NOT NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    -- Esattamente uno dei due: un upload appartiene a un utente autenticato
    -- OPPURE a un link condiviso con allow_upload, mai a entrambi o a nessuno.
    CONSTRAINT upload_sessions_one_actor CHECK (
        (user_id IS NOT NULL) <> (share_link_id IS NOT NULL)
    )
);

CREATE INDEX upload_sessions_expires_idx ON upload_sessions (expires_at);
```

**Endpoint (spec §1.2):**
```
POST   /api/v1/upload/check          → { known_hashes: [...] }  (pre-check blake3, batch)
POST   /api/v1/upload                → 201 Location: /api/v1/upload/{id}
HEAD   /api/v1/upload/{id}           → Upload-Offset (la verità sta sempre sul server)
PATCH  /api/v1/upload/{id}           → append del chunk, verifica checksum per chunk
```

**Casi limite da pinnare nei test:**
- Pre-check con 47 hash di cui 12 sconosciuti → la risposta elenca esattamente quei 12, zero falsi positivi/negativi.
- `PATCH` con `Upload-Offset` diverso da `received_bytes` reale → `409`, non un'accettazione silenziosa che corrompe il file.
- Checksum del chunk che non combacia → `460`, il chunk **non** viene scritto, il client può rispedirlo senza perdere l'offset precedente.
- A file completo, l'hash blake3 totale non combacia con `expected_hash` → il file **non entra mai in libreria**, temporaneo eliminato, sessione marcata fallita.
- Verifica di decodificabilità fallita (header corrotto, dimensioni illeggibili) anche con hash corretto → stesso esito: mai in libreria, segnalato.
- Stesso nome e stesso hash di un asset già presente nella cartella target → si salta, si segnala, **nessun secondo file**.
- Stesso nome, hash diverso → si salva come `nome_1.ext`, mai sovrascrittura.
- `expires_at` superato senza completamento → la sessione e il temporaneo vengono ripuliti (Task 2 se il cleanup è un job separato, altrimenti qui).
- Spazio su disco insufficiente per `expected_size` → rifiutato **alla creazione** della sessione (`POST /api/v1/upload`), non scoperto a metà upload.
- Sessione aperta da un link condiviso senza `allow_upload` → `403` alla creazione, prima di accettare qualunque byte.
- `mtime` del client preservato su `client_mtime`, usato come `taken_at` di fallback quando l'EXIF non ce l'ha — coerente con l'invariante già esistente su asset senza data.

- [ ] **Step 1: Scrivere i test che falliscono** (i casi sopra, in particolare il checksum-per-chunk e la collisione nome+hash-diverso)
- [ ] **Step 2-4: Fallimento, implementazione, verifica** — `cargo test -p keeppix-db -p keeppix-api -- --test-threads=1` per i soli test toccati
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): resumable tus-style upload sessions with per-chunk and end-to-end checksums"
```

---

## Task 2: Indicizzazione a priorità alta e pulizia dei temporanei

**Files:**
- Modify: `crates/keeppix-api/src/routes/upload.rs` (alla finalizzazione, dopo il `rename()` atomico)
- Create: `crates/keeppix-jobs/src/tmp_cleanup.rs`
- Modify: `crates/keeppix-jobs/src/dispatch.rs` (nuovo `JobKind` per il cleanup periodico, se non già coperto da uno esistente)

**Il punto della fase**: un file caricato non deve aspettare i 15 minuti del watcher. Alla finalizzazione di un upload (`rename()` riuscito):

```rust
job_repo.enqueue(
    JobKind::ExtractMetadata, // o DiscoverLibrary mirato su un singolo file, da decidere in implementazione
    payload,
    JobPriority::High,        // esiste già nell'enum, mai usato finora
    Some(&format!("upload-index:{asset_id}")),
).await?;
```

**Casi limite:**
- Due upload nella stessa cartella nello stesso momento → due job accodati, dedup key per-asset (non per-libreria come il rescan) così non collidono tra loro.
- Sessioni scadute e mai completate: il job di pulizia elimina temporaneo **e** riga `upload_sessions` insieme, mai l'uno senza l'altro (una riga orfana senza file confonderebbe una futura ripresa; un file orfano senza riga non si ripulisce mai).
- Il cleanup gira anche se il server si riavvia a metà di molti upload in corso — non deve toccare sessioni **non ancora scadute**, anche se il `received_bytes` è fermo da ore (una connessione lenta non è un upload abbandonato).

- [ ] **Step 1: Test che falliscono** (dedup per-asset, cleanup non tocca sessioni vive)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(jobs): index uploaded files immediately and clean up expired upload temporaries"
```

---

## Task 3: Frontend — pannello di upload persistente

**Files:**
- Create: `frontend/src/stores/upload.ts`
- Create: `frontend/src/components/UploadPanel.vue`
- Modify: `frontend/src/router.ts` (nessuna nuova rotta: il pannello è un overlay globale, non una vista)

**Persistente e minimizzabile** (spec §4.1): chiudere e riaprire la scheda non perde lo stato — gli upload interrotti riprendono dal punto esatto via `HEAD` allo stesso ID di sessione, salvato in `localStorage` (non in memoria del componente, che sparirebbe alla navigazione).

**Flusso:**
1. Selezione file → pre-check (`POST /upload/check`) → i file già presenti si segnalano e si saltano di default.
2. Destinazione scelta **ogni volta** (decisione già presa dalla spec, non un default silenzioso), con creazione cartella inline.
3. 3 upload in parallelo, chunk sequenziali per file, chunk adattivi (8 MB rete buona → 1 MB se si rileva latenza alta o errori ripetuti).
4. Errore di rete a metà chunk → il pannello mostra "rete persa, riprova", non un fallimento silenzioso dell'intera sessione.

**Casi limite:**
- Refresh della pagina a metà di 40 file: alla riapertura il pannello rilegge le sessioni da `localStorage`, fa `HEAD` su ciascuna, riprende quelle vive, segnala quelle scadute nel frattempo.
- Chiusura del tab con upload in corso: `beforeunload` non blocca la chiusura (non è un pattern accettabile), ma il prossimo avvio dell'app ritrova la sessione via `HEAD`.

- [ ] **Step 1: Test component (Vitest)** — pre-check, ripresa da `localStorage`, retry su errore di rete (mockare l'API)
- [ ] **Step 2-4: Fallimento, implementazione, verifica** — `npx vitest run`, `npx vue-tsc --noEmit`
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(frontend): persistent resumable upload panel"
```

---

## Task 4: App-password

**Files:**
- Create: `crates/keeppix-db/migrations/0027_app_passwords.sql`
- Create: `crates/keeppix-domain/src/credential.rs`
- Create: `crates/keeppix-db/src/credentials.rs`, `crates/keeppix-db/tests/credentials.rs`
- Create: `crates/keeppix-api/src/routes/credentials.rs`

```sql
CREATE TABLE app_passwords (
    id           uuid PRIMARY KEY,
    user_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label        text NOT NULL,               -- "MacBook Finder", "rclone NAS"
    secret_hash  text NOT NULL,                -- stesso schema Argon2id delle password di login
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    revoked_at   timestamptz
);

CREATE INDEX app_passwords_user_idx ON app_passwords (user_id) WHERE revoked_at IS NULL;
```

**Il segreto si mostra una sola volta**, alla creazione — mai più recuperabile, solo revocabile. Stesso principio degli access token di GitHub/GitLab.

**Endpoint:**
```
POST   /api/v1/users/me/app-passwords       → { id, label, secret }  (secret solo qui)
GET    /api/v1/users/me/app-passwords       → [{ id, label, created_at, last_used_at }]  (mai il segreto)
DELETE /api/v1/users/me/app-passwords/{id}  → revoca immediata
```

**Casi limite:**
- Verifica di un'app-password aggiorna `last_used_at` **senza bloccare** la richiesta su quella scrittura (fire-and-forget o batch, non un round-trip sincrono su ogni singola richiesta WebDAV — altrimenti ogni `PROPFIND` paga un `UPDATE`).
- Un'app-password revocata fallisce l'autenticazione **immediatamente**, non alla scadenza di una cache: nessuna cache di verifica più vecchia della revoca stessa.
- L'hash usa lo stesso Argon2id delle password di login (`crate keeppix_domain::hash_password`) — non un secondo schema di hashing da mantenere.

- [ ] **Step 1: Test che falliscono** (revoca immediata, segreto mai ritornato da `GET`)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): revocable app-passwords for non-interactive clients"
```

---

## Task 5: Scaffolding WebDAV — router, autenticazione, deroga CSRF

**Files:**
- Create: `crates/keeppix-api/src/dav/mod.rs`
- Modify: `crates/keeppix-api/src/lib.rs` (monta `/dav/{*path}`)
- Modify: `crates/keeppix-api/src/csrf.rs` (deroga già anticipata nel commento a riga 20-24)
- Modify: `crates/keeppix-api/Cargo.toml` (aggiunge `quick-xml`)

**Verificare prima di scrivere codice**: la versione di axum nel workspace supporta `MethodFilter`/`routing::on` per metodi custom, o serve un match manuale su `req.method().as_str()` dentro un handler `any()`. Questo task **non produce funzionalità visibile**, solo l'impalcatura su cui i task 6-8 si appoggiano — è deliberatamente piccolo e isolato per non mescolare una scelta architetturale con la logica business.

**Autenticazione**: Basic Auth su `/dav/*`, verificata contro `AppPasswordRepo` (Task 4) — **mai** contro il cookie di sessione o la password di login. Un tentativo con la password di login normale deve fallire, non degradare a un percorso alternativo.

**Casi limite:**
- Richiesta a `/dav/*` senza header `Authorization` → `401` con `WWW-Authenticate: Basic realm="Keeppix"`, non un redirect a `/login` (i client WebDAV non seguono redirect verso una pagina HTML).
- CSRF: la deroga vale **solo** per `/dav/*` e `/api/v1/upload/*` (i client tus/WebDAV non mandano `x-keeppix-client`) — non un indebolimento generale del controllo sul resto di `/api/v1`.

- [ ] **Step 1: Test che falliscono** (401 corretto senza auth, password di login rifiutata su `/dav/*`, resto di `/api/v1` non toccato dalla deroga)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): WebDAV router scaffolding with app-password Basic Auth"
```

---

## Task 6: `PROPFIND` e `GET`

**Files:**
- Create: `crates/keeppix-api/src/dav/propfind.rs`
- Modify: `crates/keeppix-api/src/dav/mod.rs`

**La parte che decide se WebDAV è usabile** (spec §2.3): `PROPFIND` risponde **dal database**, mai da `stat()` sul filesystem. Una singola query Postgres ha già nome, dimensione, mtime, hash — la stessa tabella `assets`/`folders` che la timeline già usa.

**XML in streaming**: la risposta a 40.000 file è ~14 MB — non si costruisce in memoria, si genera a flusso costante con `quick-xml`'s writer su un `Body` streaming di Axum.

**`ETag` = content hash**: è la chiave della sincronizzazione — rclone e Cyberduck confrontano gli ETag e scaricano solo ciò che è cambiato davvero.

**Casi limite:**
- `Depth: 0` vs `Depth: 1` vs `Depth: infinity` — implementazioni diverse, un client che chiede `infinity` su una libreria intera non deve far esplodere la memoria (limitare o rifiutare `infinity` oltre una soglia, documentato).
- Permessi: `PROPFIND` su una cartella senza accesso → risposta vuota o `403`, mai un elenco che rivela l'esistenza di file altrui.
- `GET` con `Range` header → file originale, range request reale (necessario per Finder/anteprima), non l'intero file ogni volta.

- [ ] **Step 1: Test che falliscono** (permessi su PROPFIND, Depth infinity limitato, ETag stabile = stesso hash)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): stream PROPFIND from the database, GET with range requests"
```

---

## Task 7: `PUT`, `MKCOL`, `MOVE`, `COPY`

**Files:**
- Create: `crates/keeppix-api/src/dav/write.rs`
- Modify: `crates/keeppix-db/src/folders.rs` (`copy_subtree`, non esiste ancora)

**Riuso diretto**, non logica nuova:
- `MKCOL` → `FolderRepo::ensure_child`, permesso editor via `effective_role`.
- `MOVE` → `FolderRepo::move_subtree` (già esistente, già conserva rating/album/descrizioni per costruzione — non serve altro codice per quella garanzia).
- `PUT` → stesso percorso temporaneo→verifica→`rename()` atomico del Task 1, **non passa dal watcher**: si sa esattamente quando il file è completo.
- `COPY` → nuovo `FolderRepo::copy_subtree`, con avviso sullo spazio se la copia supera un margine di sicurezza sul disco libero.

**Casi limite:**
- `PUT` di un file con lo stesso nome di uno esistente → stessa regola di collisione del Task 1 (hash uguale = skip, hash diverso = suffisso), **mai sovrascrittura silenziosa** anche se il client WebDAV si aspetta di norma un overwrite trasparente — è una scelta deliberata della spec, va documentata per l'utente.
- `MKCOL` in una cartella dove l'attore ha solo ruolo viewer → `403`.
- `.DS_Store` e `._nome` di macOS: accettati ma esclusi dall'indicizzazione, con un'opzione per scartarli del tutto invece di accumularli.

- [ ] **Step 1: Test che falliscono** (permesso editor richiesto, collisione nome, esclusione `.DS_Store` dall'indicizzazione)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): WebDAV write operations reusing folder and permission repos"
```

---

## Task 8: `DELETE`, `LOCK`, `UNLOCK`

**Files:**
- Create: `crates/keeppix-api/src/dav/delete.rs`, `crates/keeppix-api/src/dav/lock.rs`
- Create: `crates/keeppix-db/migrations/0028_dav_locks.sql`
- Create: `crates/keeppix-db/src/dav_locks.rs`

```sql
CREATE TABLE dav_locks (
    token         text PRIMARY KEY,
    resource_path text NOT NULL,
    owner         text,
    depth         text NOT NULL,
    timeout_at    timestamptz NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);
```

**`DELETE` è la regola della Fase 3, applicata dallo stesso codice**: `TrashRepo::choose(ctx, asset_id, DiskAction::Trashed)` — non `Purged`, mai una cancellazione diretta. Un editor che prova a cancellare riceve `403` (stessa regola di `may_purge`), non un successo silenzioso su una porta diversa.

**Lock persistiti in Postgres, non in memoria**: sopravvivono a un riavvio del server — un lock perso a metà scrittura da Finder produce file corrotti.

**Casi limite:**
- Finder/Windows **richiedono** Class 2 (`LOCK`/`UNLOCK`) per scrivere: senza, i client nativi si rifiutano di salvare, non solo "vanno più lenti".
- Lock scaduto (`timeout_at` superato): una richiesta con quel token deve fallire come se il lock non fosse mai esistito, non silenziosamente onorato.
- `DELETE` su una cartella intera (non un singolo file): ogni asset dentro passa dallo stesso `TrashRepo::choose`, uno per uno o in batch — non un `rm -rf` diretto sul filesystem che bypassa il cestino.

- [ ] **Step 1: Test che falliscono** (DELETE via WebDAV finisce nel cestino non cancellato, editor riceve 403, lock scaduto non onorato)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): WebDAV DELETE through trash, Class 2 locks for Finder and Explorer"
```

---

## Task 9: Wizard di configurazione

**Files:**
- Create: `frontend/src/views/settings/WebdavSetupView.vue`
- Modify: `crates/keeppix-api/src/routes/credentials.rs` (se serve un evento "prima connessione" da esporre)

**L'indicatore live in fondo è la parte che fa la differenza** (spec §3): sapere subito se la configurazione funziona, non scoprirlo mezz'ora dopo. Implementazione: polling leggero o riuso del canale WebSocket già esistente per il progresso dei job — non un nuovo meccanismo di notifica se quello già serve.

**Contenuto**: cartella di destinazione, app-password generata (mostrata una sola volta, con copia), istruzioni per macOS Finder (`⌘K`), Windows (Cyberduck consigliato, avviso sul limite 50 MB del client nativo), rclone (blocco di configurazione pronto da copiare), iPhone/Android (QR).

- [ ] **Step 1: Test component** — generazione app-password mostrata una sola volta, indicatore che cambia stato su connessione reale (mockare l'API/WS)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(frontend): WebDAV setup wizard with live first-connection indicator"
```

---

## Task 10: PWA Share Target

**Files:**
- Modify: service worker / manifest PWA (percorso esatto da confermare in implementazione — verificare se una PWA è già configurata da una fase precedente o è nuova qui)

**La risposta al requisito "selezionarle e caricarle a mano" senza un'app nativa**: dalla galleria del telefono, «Condividi → Keeppix» apre il pannello di upload (Task 3) con i file già selezionati.

**Casi limite:**
- Android: Web Share Target è supportato, va verificato il comportamento con file multipli in un colpo solo.
- iOS: supporto più limitato — va verificato esplicitamente cosa funziona davvero prima di documentarlo come feature, non assunto dalla spec.

- [ ] **Step 1: Verifica manuale su un dispositivo Android reale e uno iOS reale** — non è un task pinnabile solo da test automatici, la spec stessa segnala il supporto iOS come incerto
- [ ] **Step 2-4: Implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(frontend): PWA Share Target for photo upload from the phone gallery"
```

---

## Criteri di completamento della Fase 5

- [ ] `rclone bisync` completa un ciclo su una cartella reale e i file caricati compaiono in timeline entro pochi secondi (criterio della spec, verbatim).
- [ ] Un upload da 2 GB interrotto a metà (rete tolta manualmente) riprende dal punto esatto dopo la riconnessione, senza ricaricare byte già ricevuti.
- [ ] macOS Finder scrive, sposta, cancella su `/dav/` senza errori di lock — verificato su un Mac reale, non solo a test automatici.
- [ ] Un editor (non owner/admin) che cancella via WebDAV trova il file nel cestino, non sparito.
- [ ] Un'app-password revocata smette di funzionare alla richiesta successiva, non alla scadenza di una cache.
- [ ] `PROPFIND` su una cartella con qualche migliaio di file risponde in tempo utile (ordine di decine di ms, non secondi) — misurato, non assunto.
- [ ] `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `npx vue-tsc --noEmit`, `npx vitest run`, `npm run build` — tutti verdi.

## Cosa NON è in Fase 5

- **Backup automatico da telefono**: fuori obiettivo — è un prodotto diverso (Google Photos/iCloud lo fanno, Keeppix no).
- **Sincronizzazione bidirezionale gestita da Keeppix**: la fa `rclone bisync`, che lo fa meglio — Keeppix espone solo un WebDAV corretto, non reimplementa la logica di sync.
- **SFTP**: non previsto.
- **App mobile nativa**: la PWA con Share Target (Task 10) è la risposta per ora; un client nativo resta fuori scope.
