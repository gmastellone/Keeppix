# Keeppix Fase 6 — Consolidamento

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (raccomandato) o superpowers:executing-plans per eseguire questo piano task per task. Gli step usano la sintassi checkbox (`- [ ]`) per il tracciamento.

**Goal:** Rendere il sistema mantenibile e pronto per il client mobile. Nessuna funzione fotografica nuova — è la fase che si salta volentieri e che poi manca: video completo, backup/ripristino veri, 2FA, sincronizzazione incrementale, manutenzione automatica, API pubblica pulita, PWA offline. Più i debiti della Fase 0 e le lacune di prestazioni trovate in un audit di questa sessione.

**Architecture:** Non introduce sottosistemi nuovi indipendenti — **completa** infrastruttura già esistente: il probe hardware (oggi finto), lo scheduler di manutenzione (oggi copre 2 pulizie su 5), il `change_log` (oggi alimentato ma senza un endpoint REST), il service worker (oggi fa solo Share Target). Il rischio di questa fase non è la novità, è **duplicare per errore** ciò che la Fase 5 ha già costruito.

**Tech Stack:** `ffmpeg` in processo sandbox (già il pattern di LibRaw) per transcodifica HLS · `age` + `zstd` per il backup · TOTP (RFC 6238, crate `totp-rs` o equivalente) · `utoipa` (già presente) per OpenAPI

**Spec:** [`../specs/fase-6-consolidamento.md`](../specs/fase-6-consolidamento.md) — **leggerla prima**; se piano e spec divergono, vince la spec
**Roadmap:** [`2026-08-13-keeppix-roadmap.md`](2026-08-13-keeppix-roadmap.md) — Fase 6 «consuma: tutto», indipendente da 7/8/9

---

## Cosa esiste già — non è tutto da zero

Verificato su `origin/fase-5` (in chiusura, quasi pronta a mergiare in `main`), non sulla spec:

- **Il probe hardware resta finto**: `keeppix_media::probe()` (`crates/keeppix-media/src/probe.rs:17-23`) restituisce ancora `"unprobed"`, incondizionatamente. **Attenzione al nome**: esiste già un `crates/keeppix-media/src/video.rs::probe(path) -> VideoInfo` — funzione diversa, stesso nome, modulo diverso (durata/codec/rotazione via ffprobe). Non è una collisione oggi, ma il Task 1 di questa fase estende il **primo** (quello hardware), e la **Fase 7** lo estenderà di nuovo per l'inferenza AI — va scritto in modo che una seconda estensione futura non richieda di riscriverlo, non assumendo che questa sia l'ultima volta che cambia.
- **Nessun codice di transcodifica esiste** — verificato con una ricerca su tutto l'albero (`ffmpeg`, `hls`, `transcod`, `m3u8`): zero. `assets.kind` ha già la variante `'video'` nel CHECK (`migrations/0005_assets.sql:16`) — l'identità è pronta, la pipeline no.
- **Nessun codice di backup esiste** — zero riferimenti a `pg_dump`, `kpxb`, `age::`, `zstd`. C'è però un **pattern di scheduling già consolidato** da riusare: ogni job di manutenzione ha una propria `schedule()` chiamata da `crates/keeppix-server/src/main.rs:163-223` — il backup notturno si aggiunge lì, non con un meccanismo nuovo.
- **`SettingsRepo::get_or_create_secret`** (`crates/keeppix-db/src/settings.rs:40`) esiste da Fase 0, zero chiamanti di produzione — confermato ancora vero, è il rinvio dichiarato in `scripts/wired-exceptions.txt:20`. Nessun altro codice TOTP esiste.
- **La parte difficile della sincronizzazione è già risolta**, e questo cambia lo scope del Task di questa fase: `ChangeLogRepo::since`/`safe_cursor` (`crates/keeppix-db/src/changes.rs:42-111`) implementa già l'arretramento a `pg_snapshot_xmin` che la spec descrive come il dettaglio critico — **con il test delle transazioni sovrapposte già scritto** (`crates/keeppix-db/tests/changes.rs:129-168`, `overlapping_transactions_do_not_drop_rows`). Oggi il consumatore è il WebSocket (`routes/ws.rs:108,161`), non una rotta REST. **Il Task 7 riusa `ChangeLogRepo`, non reinventa la sicurezza del cursore.** `Idempotency-Key` invece è completamente da zero — zero riferimenti in tutto il repo.
- **Lo scheduler copre già 2 pulizie su 5**: cestino oltre 30 giorni (`cleanup_trash.rs`) e upload abbandonati (`tmp_cleanup.rs`) sono già schedulati. **Mancano**: sessioni scadute (`SessionRepo::purge_expired`, `crates/keeppix-db/src/sessions.rs:230`, esiste già ma zero chiamanti — stesso pattern del debito TOTP, confermato in `wired-exceptions.txt:29`), cache transcodifiche (bloccato dal Task 2, la transcodifica non esiste ancora), job `done` oltre 7 giorni, `VACUUM ANALYZE`.
- **OpenAPI è già solida**: `utoipa` montato, `docs/api/openapi.json` verificato in CI (`git diff --exit-code`, `.github/workflows/ci.yml:100`). I tre schemi byte-identici (`LoginResponse`, `MeResponse`, `SetupResponse`, ognuno solo `{ user: UserView }`) sono ancora lì, confermato. Nessuna generazione client Kotlin/Swift/Dart/TypeScript esiste.
- **La PWA è già iniziata dalla Fase 5, non da questa fase**: `frontend/public/manifest.webmanifest` (nome, icone, `share_target`) e `frontend/public/sw.js` esistono già — ma il service worker **dichiara esplicitamente nel proprio commento** di non fare caching offline, solo intercettare lo Share Target. **Il Task 10 estende quel file, non ne crea uno nuovo, e non tocca lo Share Target che è già fatto.** Il manifest manca di un set di icone PNG multi-dimensione/maskable — necessario per l'installabilità reale, non solo per il manifest formalmente valido.
- **I tre debiti reali della Fase 0** (`Password` senza `zeroize`, `index.html` con `lang="en"` fisso, `users.locale` mai letto dal frontend) sono confermati ancora presenti. Il quarto che lo spec elencava (`/auth/refresh` mai chiamato) **era già falso** — chiamato dal watchdog SPA da Fase 3 Task 12b (`frontend/src/stores/session.ts:98`) — rimosso dallo spec prima di scrivere questo piano.
- **Le lacune di prestazioni** trovate in un audit di questa sessione sono confermate ancora presenti sull'ultimo commit di `fase-5`: nessun indice trigram su `camera_model`/`lens`, nessuna dipendenza `moka` in nessun `Cargo.toml`, `stacks.primary_asset_id`/`album_assets.added_by` ancora senza indice, `FolderRepo::ensure_path` ancora N+1 (confinato al percorso di scrittura dell'ingest, non toccato da nessun commit di Fase 5).

---

## Global Constraints

Valgono per **ogni** task. Sono gli invarianti di [`/AGENTS.md`](../../../AGENTS.md), più quelli specifici di questa fase.

- **Rust edition 2024, toolchain 1.88.0.**
- **`keeppix-db` è l'unico crate con SQL.**
- **Ogni metodo di repository che legge dati di un utente prende un `AuthContext` come primo parametro.**
- **`Forbidden`, mai `NotFound`.**
- **Query sempre parametrizzate.**
- **Nessun `unwrap()`/`expect()` in produzione.**
- Clippy `all` + `pedantic` a warn, `-D warnings` pulito. `cargo fmt --check` pulito.
- **Commit convenzionali in inglese**, uno per unità logica.

### Specifiche della Fase 6

- **`/api/v1` resta congelato**: solo aggiunte. Le rotte nuove (`/sync/delta`, backup, TOTP) sono aggiunte pure; i tre schemi da disambiguare (`LoginResponse`/`MeResponse`/`SetupResponse`) vanno rinominati senza cambiare la forma del JSON sul filo — è un problema di generazione OpenAPI, non un cambiamento di contratto.
- **Il backup dev'essere apribile senza Keeppix**: `tar`+`age` sono formati standard, mai una struttura proprietaria che solo Keeppix sa leggere.
- **Una cache di permessi scaduta è un difetto di sicurezza**, non solo di prestazioni (Task 12): ogni invalidazione va esplicita, mai un TTL che spera di bastare.
- **Il probe video e il probe AI (Fase 7) condividono la stessa funzione**: questa fase la struttura per essere estesa di nuovo, non la considera definitiva.

---

## Struttura dei file

```
crates/keeppix-media/src/
├── probe.rs              MOD  Capabilities guadagna i campi video (hw accel, codec supportati)
├── transcode.rs          NEW  invocazione ffmpeg sandboxata, HLS playlist, poster/anteprima animata
└── video.rs                MOD  eventuali estensioni per il tone mapping HDR

crates/keeppix-db/
├── migrations/
│   ├── 0029_totp.sql            NEW  totp_secrets, totp_recovery_codes
│   ├── 0030_backup_config.sql   NEW  backup_destinations, backup_runs
│   └── 0031_video_cache.sql     NEW  transcode_cache (se non copribile da file system + TTL)
├── src/
│   ├── totp.rs            NEW  TotpRepo
│   ├── backup.rs          NEW  BackupRepo — configurazione, storico run
│   └── idempotency.rs     NEW  IdempotencyRepo — chiavi viste, per mutazione

crates/keeppix-jobs/src/
├── backup.rs              NEW  job di backup/upload alla destinazione, verifica post-scrittura
├── transcode.rs           NEW  job di transcodifica on-demand con cache
└── maintenance.rs         NEW  completa lo scheduler: purge_expired, VACUUM, pulizia job done

crates/keeppix-api/
├── src/
│   ├── routes/sync.rs      NEW  GET /sync/delta
│   ├── routes/totp.rs      NEW  provisioning, verifica, codici di recupero
│   ├── routes/backup.rs    NEW  wizard, esecuzione manuale, ripristino
│   ├── idempotency.rs      NEW  middleware Idempotency-Key
│   └── openapi.rs           MOD  disambigua i tre schemi, corregge info.version

frontend/src/
├── views/settings/BackupView.vue      NEW  wizard di backup
├── views/settings/RestoreView.vue     NEW  wizard di ripristino
├── views/settings/TotpSetupView.vue   NEW  QR + codici di recupero
├── views/PlayerView.vue                NEW  player HLS
└── sw.js                                MOD  estende il service worker esistente con caching offline
```

**Ordine dei task:** 1 → 2 (video) indipendente da 3 → 4 → 5 (backup) indipendente da 6 (TOTP) indipendente da 7 (sync) indipendente da 8 (scheduler) — questi cinque blocchi non hanno dipendenze reali fra loro, l'ordine è a scelta. 9 (OpenAPI) conviene dopo che le rotte nuove esistono, per generare i client sulla superficie finale. 10 (PWA) dopo 2 se si vuole cache offline anche dei video, altrimenti indipendente. 11 (debiti Fase 0) e 12 (prestazioni) indipendenti da tutto, si possono intercalare.

---

## Task 1: Probe hardware reale — parte video

**Files:**
- Modify: `crates/keeppix-media/src/probe.rs`
- Create: `crates/keeppix-media/tests/probe_video.rs`

**Interfaces:**
- `Capabilities` guadagna campi per l'accelerazione video: `backend: VideoBackend` (enum: `Rkmpp, Nvenc, V4l2m2m, Videotoolbox, Vaapi, Qsv, Amf, Software`), `decode_fps: Option<f32>` (già esiste), lasciando spazio (un campo `extra: serde_json::Value` o simile) per quello che la Fase 7 aggiungerà senza rompere questo schema.
- `probe()` misura per davvero: prova un encode di 2 secondi con ogni backend candidato, ordinati per SoC rilevato (`/proc/device-tree/compatible`, `/proc/cpuinfo`, `/dev/dri/*`), tiene il primo che funziona.

**Casi limite:**
- Nessun backend hardware disponibile → `Software`, non un errore: è un esito valido, non un fallimento.
- La misura richiede meno di ~4 secondi in totale — se un backend si blocca, timeout e passa al successivo, non attende indefinitamente.
- Il risultato è **sovrascrivibile a mano** dall'operatore nel pannello — la misura è un default, non un vincolo.

- [ ] **Step 1: Scrivere i test che falliscono**
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(media): measure real video hardware acceleration instead of a constant"
```

---

## Task 2: Transcodifica HLS

**Files:**
- Create: `crates/keeppix-media/src/transcode.rs`
- Create: `crates/keeppix-jobs/src/transcode.rs`
- Create: `crates/keeppix-api/src/routes/video.rs`
- Create: `frontend/src/views/PlayerView.vue`

**Pipeline**: direct play quando il contenitore/codec del client lo permette (verificato via `Accept`/negoziazione, non assunto); transcodifica on-demand altrimenti, in HLS con playlist segmentata (seek senza attendere il file intero), cache su disco con pulizia a 90 giorni (Task 8), tone mapping HDR (HLG/PQ) quando serve, poster e anteprima animata via `extract_poster` (già esistente in `video.rs`) esteso per la clip animata.

**Casi limite:**
- Transcodifica software su ARM è lenta (rischio dichiarato nella roadmap): direct play copre la maggioranza dei casi, la transcodifica è on-demand e mai preventiva su tutta la libreria.
- Un client che chiede un segmento oltre la fine del video → `404` pulito, non un errore 500.
- Adattamento alla rete («risparmia banda in mobile»): parametro esplicito del client, mai indovinato dal server.

- [ ] **Step 1: Test che falliscono** (direct play vs transcodifica, playlist valida, poster)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(jobs): on-demand HLS transcoding with disk cache and HDR tone mapping"
```

---

## Task 3: Formato e wizard di backup

**Files:**
- Create: `crates/keeppix-db/migrations/0030_backup_config.sql`
- Create: `crates/keeppix-db/src/backup.rs`
- Create: `crates/keeppix-jobs/src/backup.rs`
- Create: `crates/keeppix-api/src/routes/backup.rs`
- Create: `frontend/src/views/settings/BackupView.vue`

**Formato** (spec §2.2, invariato): `keeppix-<timestamp>.kpxb`, un `tar` con `manifest.json`, `database.dump` (`pg_dump` formato custom), `config.toml`, `maps.json`, e sidecar/derivati/originali **opzionali** — compressione `zstd`, cifratura `age`.

```sql
CREATE TABLE backup_destinations (
    id            uuid PRIMARY KEY,
    kind          text NOT NULL CHECK (kind IN ('local','s3','webdav','sftp')),
    label         text NOT NULL,
    config        jsonb NOT NULL,       -- credenziali cifrate a riposo
    enabled       boolean NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE backup_runs (
    id              uuid PRIMARY KEY,
    destination_id  uuid REFERENCES backup_destinations(id) ON DELETE SET NULL,
    started_at      timestamptz NOT NULL DEFAULT now(),
    completed_at    timestamptz,
    size_bytes      bigint,
    verified_at     timestamptz,        -- prova di ripristino mensile
    status          text NOT NULL CHECK (status IN ('running','ok','failed')),
    error           text
);
```

**Casi limite:**
- **L'avviso «senza Originali questo backup non contiene le tue foto»** non è testo decorativo: un test verifica che compaia ogni volta che «Originali» non è selezionato.
- Spazio insufficiente sulla destinazione: verificato **prima** di iniziare la scrittura, non scoperto a metà.
- Cifratura `age`: la passphrase persa rende il backup irrecuperabile per costruzione — va detto esplicitamente nell'interfaccia, non lasciato sottinteso.

- [ ] **Step 1: Test che falliscono** (avviso obbligatorio, formato apribile con `age`/`tar` da riga di comando esterna al progetto)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(jobs): backup format and wizard with mandatory catalog-only warning"
```

---

## Task 4: Destinazioni e ripristino

**Files:**
- Modify: `crates/keeppix-jobs/src/backup.rs` (destinazioni S3/WebDAV/SFTP)
- Create: `crates/keeppix-api/src/routes/restore.rs`
- Create: `frontend/src/views/settings/RestoreView.vue`

**Destinazioni**: locale, S3-compatibile (AWS/B2/R2/MinIO/Wasabi), WebDAV remoto, SFTP — test di connessione prima di salvare la configurazione, più destinazioni contemporanee.

**Ripristino**: selezione della sorgente con manifest leggibile, anteprima in simulazione, dump di sicurezza dello stato attuale prima di sovrascrivere. Backup più recente della versione installata → rifiutato con messaggio chiaro; più vecchio → migrazioni applicate automaticamente. Su server nuovo, alternativa al wizard di primo avvio.

**Casi limite:**
- **La prova di ripristino mensile** (spec §2.4, il dettaglio che "vale più di tutto il resto"): ripristino automatico in uno schema temporaneo, verifica di caricabilità, poi cancellazione — schedulata come gli altri job di manutenzione (Task 8).
- Ripristino delle sole mappe → non tocca il database, utilizzabile a caldo senza fermare il servizio.
- Un ripristino interrotto a metà non deve lasciare il database in uno stato intermedio — dentro una transazione dove possibile, con `pg_dump`/`pg_restore` questo ha un limite noto da documentare, non da fingere risolto.

- [ ] **Step 1: Test che falliscono** (backup più recente rifiutato, prova di ripristino mensile verificabile, dump di sicurezza creato prima di sovrascrivere)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): backup destinations, restore wizard, monthly restore proof"
```

---

## Task 5: 2FA — TOTP

**Files:**
- Create: `crates/keeppix-db/migrations/0029_totp.sql`
- Create: `crates/keeppix-db/src/totp.rs`
- Create: `crates/keeppix-api/src/routes/totp.rs`
- Create: `frontend/src/views/settings/TotpSetupView.vue`

```sql
CREATE TABLE totp_secrets (
    user_id     uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret_enc  bytea NOT NULL,   -- cifrato con chiave derivata da SettingsRepo::get_or_create_secret
    enabled_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE totp_recovery_codes (
    id         uuid PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash  text NOT NULL,
    used_at    timestamptz
);

CREATE INDEX totp_recovery_codes_user_idx ON totp_recovery_codes (user_id) WHERE used_at IS NULL;
```

RFC 6238, finestra di tolleranza ±1 intervallo, **protezione contro il riuso dello stesso codice** (registrare l'ultimo intervallo accettato per utente), 10 codici di recupero monouso salvati come hash. `SettingsRepo::get_or_create_secret` trova qui il suo primo chiamante di produzione.

**Casi limite:**
- Un codice riusato nello stesso intervallo → rifiutato, anche se matematicamente corretto.
- Tutti i 10 codici di recupero consumati → l'utente deve poter rigenerarli (da autenticato), non restare bloccato fuori.
- Attivare il 2FA non deve invalidare la sessione corrente a metà del provisioning — solo dopo conferma del primo codice.

- [ ] **Step 1: Test che falliscono** (riuso rifiutato, finestra ±1, codici di recupero monouso)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): TOTP two-factor authentication with recovery codes"
```

---

## Task 6: `/sync/delta` — sincronizzazione incrementale

**Files:**
- Create: `crates/keeppix-api/src/routes/sync.rs`
- Modify: `crates/keeppix-api/src/lib.rs` (wiring)

**Non reinventa nulla**: `ChangeLogRepo::since`/`safe_cursor` esistono già, testati contro le transazioni sovrapposte. Questo task è **l'endpoint REST sopra quel repository già corretto**, che oggi ha come unico consumatore il WebSocket.

```
GET /api/v1/sync/delta?cursor=88421
→ { cursor: 91055, upserted: [...], deleted: [...], has_more: true }
```

**Casi limite:**
- Il test delle transazioni sovrapposte **esiste già** (`changes.rs:129-168`) — questo task verifica che l'endpoint REST esponga la stessa garanzia, non la riprova da capo.
- `has_more: true` con un client che si ferma a metà pagina → il prossimo `cursor` riparte esattamente da lì, nessuna riga persa né duplicata.

- [ ] **Step 1: Test che falliscono** (endpoint REST, `has_more`, paginazione)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): expose ChangeLogRepo over REST as /sync/delta"
```

---

## Task 7: Idempotenza

**Files:**
- Create: `crates/keeppix-db/src/idempotency.rs`
- Create: `crates/keeppix-api/src/idempotency.rs` (middleware)
- Modify: `crates/keeppix-api/src/lib.rs`

**Header `Idempotency-Key`** su tutte le mutazioni. **Chiude tre debiti in uno** (spec §4.2): il deadlock `40P01` di due replay concorrenti, il re-login occasionale su retry di `refresh`, l'assenza di idempotenza — un'unica soluzione, non tre correzioni separate.

```sql
CREATE TABLE idempotency_keys (
    key          text PRIMARY KEY,
    user_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    response_status smallint NOT NULL,
    response_body   jsonb,
    created_at   timestamptz NOT NULL DEFAULT now()
);
```

**Casi limite:**
- Stessa chiave, stesso utente, richiesta identica → risposta salvata, rieseguita zero volte.
- Stessa chiave, corpo della richiesta **diverso** dalla prima volta → `409`, non un'esecuzione silenziosa del nuovo corpo.
- Chiavi vecchie (>24h) ripulite dallo scheduler (Task 8), non accumulate per sempre.

- [ ] **Step 1: Test che falliscono** (replay identico, corpo diverso rifiutato, pulizia chiavi vecchie)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(api): Idempotency-Key header, closing three previously separate debts"
```

---

## Task 8: Scheduler di manutenzione — completare, non ricostruire

**Files:**
- Create: `crates/keeppix-jobs/src/maintenance.rs`
- Modify: `crates/keeppix-server/src/main.rs` (wiring dei nuovi `schedule()`)

**Riusa lo stesso pattern** di `cleanup_trash.rs`/`tmp_cleanup.rs` (già schedulati da `main.rs:163-223`), aggiungendo quello che manca:

- `SessionRepo::purge_expired` — esiste da Fase 0, mai chiamato: qui trova il suo chiamante.
- Pulizia job `done` oltre 7 giorni.
- Pulizia cache transcodifiche oltre 90 giorni (dipende dal Task 2).
- Pulizia chiavi di idempotenza scadute (Task 7).
- `VACUUM ANALYZE` e dump del database (Task 3) nella finestra notturna.
- **Scrubbing d'integrità**: ri-hash a rotazione del 5% della libreria, per intercettare bit rot.

**Casi limite:**
- Ogni pulizia nuova rispetta `EnergyProfile` esistente — nella finestra `Interactive` non gira nulla di pesante, esattamente come già fanno `cleanup_trash`/`tmp_cleanup`.
- Lo scrubbing d'integrità su un asset che risulta corrotto: segnala, non cancella e non tenta un ripristino automatico — è una scoperta per l'utente, non una decisione presa al posto suo.

- [ ] **Step 1: Test che falliscono** (ogni pulizia mancante, rispetto del profilo energetico)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(jobs): complete the maintenance scheduler — sessions, done jobs, VACUUM, scrubbing"
```

---

## Task 9: API pubblica e client generati

**Files:**
- Modify: `crates/keeppix-api/src/routes/auth.rs`, `routes/setup.rs`, `openapi.rs`
- Create: script/config di generazione client (Kotlin/Swift/Dart/TypeScript)

**Disambigua i tre schemi byte-identici** (`LoginResponse`, `MeResponse`, `SetupResponse` — ognuno oggi solo `{ user: UserView }`): nomi distinti nello schema OpenAPI pur restando identici sul filo, cosicché un generatore di client non li confonda o li collassi in un tipo solo perdendo il significato semantico. `info.version` diventa la versione dell'API, non quella del crate. I doc comment Rust `# Errors` smettono di finire come `summary` OpenAPI.

**Casi limite:**
- Il test CI che confronta `docs/api/openapi.json` (già esistente) deve continuare a passare — questo task **aggiorna** quel file con la generazione corretta, non lo bypassa.
- Generazione client verificata almeno per TypeScript (già consumato dal frontend) e uno fra Kotlin/Swift, non solo dichiarata.

- [ ] **Step 1: Test che falliscono** (schemi distinti nello schema OpenAPI, `info.version` corretto)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "fix(api): disambiguate byte-identical OpenAPI schemas, fix info.version, generate clients"
```

---

## Task 10: PWA — offline reale, non solo Share Target

**Files:**
- Modify: `frontend/public/sw.js` (**estende**, non sostituisce — lo Share Target esistente resta intatto)
- Modify: `frontend/public/manifest.webmanifest` (set di icone completo)

**Il service worker esistente dichiara nel proprio commento di non fare caching offline** — questo task lo aggiunge: shell dell'app precacheata, miniature già viste navigabili offline, stati offline progettati (non un errore generico quando la rete manca).

**Nota onesta sulle notifiche push** (spec §7, invariata): il WebSocket non funziona in background sul mobile, servirebbero FCM/APNs — fuori dagli obiettivi dichiarati; l'API resta pronta perché gli eventi sono già entità serializzate.

**Casi limite:**
- Il precache non deve rompere lo Share Target esistente — un test verifica che `POST /share-target` continui a funzionare dopo questo task.
- Un aggiornamento del service worker non deve intrappolare l'utente su una versione vecchia della shell — strategia di attivazione esplicita (`skipWaiting`/`clients.claim` con cautela, non automatico e silenzioso se ci sono operazioni in corso).

- [ ] **Step 1: Test che falliscono** (Share Target intatto, shell disponibile offline, aggiornamento del service worker non intrappola)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "feat(frontend): offline-caching service worker on top of the existing Share Target"
```

---

## Task 11: Debiti della Fase 0

**Files:**
- Modify: `crates/keeppix-domain/src/password.rs` (`zeroize`)
- Modify: `frontend/index.html`, `frontend/src/i18n/`

- **`Password` non azzera il buffer**: `zeroize` su tutta la catena (corpo JSON, buffer axum, allocazione serde) — azzerare solo l'ultimo anello dà un falso senso di completezza, non basta il tipo da solo.
- **`index.html` con `lang="en"` fisso**: con le impostazioni utente, non hardcoded.
- **`users.locale`/`UserView.locale` mai usati**: la lingua vive in `localStorage`. Riconciliare secondo lo spec §10.10 — decidere se il profilo utente diventa la fonte di verità (sincronizzando `localStorage` da lì) o se si dichiara `localStorage` la fonte di verità e si toglie il campo dal profilo, non lasciarli scollegati.

**Casi limite:**
- Il test che dimostra `zeroize` deve verificare la memoria **dopo** la deserializzazione axum, non solo sull'ultimo `Drop` — altrimenti il buffer intermedio resta in chiaro.

- [ ] **Step 1: Test che falliscono**
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "fix(domain): zeroize Password through the whole chain, honest locale source of truth"
```

---

## Task 12: Prestazioni — indici, cache, N+1

**Files:**
- Create: `crates/keeppix-db/migrations/0032_perf_indices.sql` (o numero libero al momento dell'implementazione)
- Modify: `crates/keeppix-api/Cargo.toml` (`moka`)
- Create: `crates/keeppix-api/src/cache.rs`

Dallo spec §6, tutti e quattro i punti, verificati ancora reali su `fase-5`:

1. Indici trigram su `asset_exif.camera_model`/`.lens`.
2. Cache `moka` per permessi effettivi e impostazioni, **con invalidazione esplicita** — mai un TTL che spera di bastare: un permesso revocato deve sparire dalla cache nello stesso momento in cui sparisce dal database.
3. Indici su `stacks.primary_asset_id`, `album_assets.added_by`; rivedere il filtro `status <> 'trashed'` in `AssetRepo` contro l'indice parziale esistente.
4. `FolderRepo::ensure_path` — decidere, con una misura reale (non assunta) prima di riscrivere, se il costo attuale giustifica la riscrittura a una sola query o se un import resta abbastanza raro da non giustificare il rischio di toccare una funzione oggi corretta.

**Casi limite:**
- La cache dei permessi va invalidata su: revoca di un permesso, cambio di ruolo, rimozione da un gruppo, eliminazione di un utente — un test per ciascuno, non un test generico "la cache si invalida".
- Gli indici nuovi si verificano con `EXPLAIN ANALYZE` reale sul test di scala esistente (`scale_200k.rs`), non assunti efficaci per definizione.

- [ ] **Step 1: Test che falliscono** (in particolare l'invalidazione della cache permessi, ognuno dei suoi trigger)
- [ ] **Step 2-4: Fallimento, implementazione, verifica**
- [ ] **Step 5: Commit**

```bash
git commit -m "perf(db): trigram indices, invalidated in-process permission cache, missing FK indices"
```

---

## Criteri di completamento della Fase 6

- [ ] Dalla specifica OpenAPI si genera un client funzionante (almeno TypeScript + uno fra Kotlin/Swift), verificato non solo dichiarato.
- [ ] Un ripristino da backup su macchina vuota riporta l'istanza allo stato esatto — provato per davvero, non solo per test automatico.
- [ ] La prova di ripristino mensile gira nella finestra notturna e non lascia lo schema temporaneo residuo in caso di fallimento.
- [ ] Un client che sincronizza via `/sync/delta` durante transazioni concorrenti non perde righe — stesso test già esistente per `ChangeLogRepo`, ora verificato anche sopra l'endpoint REST.
- [ ] Il 2FA rifiuta il riuso di un codice nello stesso intervallo.
- [ ] Un permesso revocato sparisce dalla cache nello stesso momento in cui sparisce dal database — verificato, non assunto.
- [ ] Il service worker mantiene lo Share Target funzionante dopo aver aggiunto il caching offline.
- [ ] `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `npx vue-tsc --noEmit`, `npx vitest run`, `npm run build` — tutti verdi.

## Cosa NON è in Fase 6

- **Notifiche push** (FCM/APNs): richiedono servizi Google/Apple, fuori dagli obiettivi dichiarati.
- **WebAuthn/passkey**: evoluzione naturale del 2FA, ma da valutare dopo, non insieme al TOTP.
- **Riconoscimento volti e ricerca semantica**: Fase 7 e Fase 8, con spec proprie — non più «fuori scope», semplicemente non qui.
