# Fase 0 — Fondamenta

**Stato:** ✅ **costruita, mergiata su `main`**
**Consegna:** [`../plans/2026-08-13-keeppix-fase-0-STATO.md`](../plans/2026-08-13-keeppix-fase-0-STATO.md)
**Piano eseguito:** [`../plans/2026-08-13-keeppix-fase-0.md`](../plans/2026-08-13-keeppix-fase-0.md)

Questa spec è **retrospettiva**: descrive ciò che esiste, non ciò che va
costruito. Serve a chi deve costruirci sopra e ha bisogno di sapere cosa può
dare per scontato — e cosa non deve rompere.

Per le decisioni prese *durante* l'esecuzione (R1-R13) e per i difetti noti
differiti, vedi il documento di consegna.

---

## 1. Cosa fa il sistema oggi

Un binario che si avvia, migra il database, serve un frontend incorporato,
crea il primo amministratore e gestisce il login. **Nessuna funzione
fotografica**: quelle iniziano in Fase 1.

Superficie utente completa:

- `/` → pagina di setup (istanza vergine) o timeline vuota (già configurata)
- Creazione del primo admin, login, logout, sessione persistente
- Rilevamento della lingua dal browser, italiano e inglese

---

## 2. Struttura del workspace

Sette crate con confini **imposti meccanicamente**, non per convenzione.

| Crate | Responsabilità | Dipende da |
|---|---|---|
| `keeppix-domain` | tipi ed entità pure: nessun I/O, nessun SQL, nessuna rete | — |
| `keeppix-db` | migrazioni, repository, query — **l'unico crate con SQL** | domain |
| `keeppix-media` | decodifica, EXIF, thumbnail, RAW, video — **non conosce il database** | domain |
| `keeppix-jobs` | coda, worker, retry | db, media |
| `keeppix-api` | router Axum, extractor, DTO, OpenAPI | db, jobs |
| `keeppix-dav` | adapter WebDAV con permessi applicati | db |
| `keeppix-server` | binario: config, wiring, watcher, frontend incorporato | tutti |
| `keeppix-test-support` | asserzioni condivise fra i test di crate diversi | — |

`keeppix-media`, `keeppix-jobs` e `keeppix-dav` sono **vuoti** in Fase 0: i
confini esistono prima del contenuto, di proposito.

### 2.1 Come i confini sono imposti

Non sono raccomandazioni:

- **`sqlx` è fra le `[dependencies]` del solo `keeppix-db`.** In `keeppix-api`
  è solo `[dev-dependencies]` (serve all'harness dei test per il `CREATE
  DATABASE`). Un handler che provasse a scrivere una query **non compila**.
- **`deny.toml` ha una regola `[[bans.deny]]`** che vieta `keeppix-db` fuori da
  un'allowlist di dipendenti. Aggiungere l'arco `media → db` fa uscire
  `cargo deny check bans` con `error[banned]`, exit 2. Verificato in entrambi i
  versi.

Limite noto della regola: `cargo-deny` controlla i **dipendenti diretti**,
quindi `media → jobs → db` passerebbe se `jobs` fosse autorizzato. È
documentato dentro `deny.toml`.

---

## 3. Schema del database

Tre migrazioni. **Non modificarle**: sqlx verifica il checksum e rifiuta di
partire se cambiano dopo essere state applicate.

```
0001_users.sql     users, groups, group_members
                   + estensioni pg_trgm e postgis
0002_sessions.sql  sessions (famiglie di refresh token)
0003_settings.sql  system_settings (segreti generati)
```

### 3.1 `users`

`id uuid PK` · `username` · `email` · `display_name` · `password_hash` ·
`role` (CHECK `admin`/`user`) · `locale` · `totp_secret_enc bytea` ·
`created_at` · `updated_at` · `disabled_at`

- Indice unico su `lower(username)` — case-insensitive.
- Indice unico **parziale** su `lower(email)` `WHERE email IS NOT NULL`.

### 3.2 `sessions`

`id` · `family_id` · `user_id` · `refresh_token_hash bytea` · `parent_id`
(self-FK) · `user_agent` · `ip inet` · `created_at` · `expires_at` ·
`consumed_at` · `revoked_at`

Indice unico su `refresh_token_hash`; indici su `family_id`, `user_id`; indice
parziale su `expires_at WHERE revoked_at IS NULL`.

### 3.3 `system_settings`

`key text PK` · `value jsonb` · `updated_at`

### 3.4 Perché PostGIS è già attivo

`postgis` **non è un'estensione trusted**: richiede il superuser. Abilitarla in
Fase 4 su un Postgres gestito già popolato sarebbe stato impossibile o molto
scomodo. Modificare `0001` era gratis solo finché non esisteva un rilascio, ed
è stato fatto nella fix wave finale.

---

## 4. Autenticazione

### 4.1 Il modello

**Token opachi, non JWT.** 32 byte casuali da un CSPRNG (`rand::rng()`, ChaCha12
riseminato dall'OS — verificato leggendo il sorgente), codificati base64url
senza padding: 43 caratteri.

**In database vive solo il digest SHA-256.** Un dump del database non permette
di impersonare nessuno.

La validazione passa da una **lookup del digest**, mai da un confronto `==` fra
token in Rust — quello reintrodurrebbe un canale laterale temporale.

### 4.2 Famiglie e rilevamento del riuso

Una «famiglia» è la catena di refresh token nata da un login. Ogni rotazione
consuma il vecchio token e ne emette uno nuovo nella stessa famiglia.

> Se un token **già consumato** viene ripresentato, l'unica spiegazione è che
> una copia sia in mano a qualcun altro: **l'intera famiglia viene revocata** e
> tutti, incluso il legittimo proprietario, devono rifare il login.

È il compromesso voluto: una sessione rubata muore, al costo di un re-login.

Dettagli che rendono corretto il meccanismo:
- La revoca della famiglia è **committata prima** di restituire `Forbidden`. Se
  fosse rollbackata, la sessione rubata sopravvivrebbe e tutto sarebbe
  decorativo.
- Il lock `FOR UPDATE` è preso **prima** di qualsiasi decisione sullo stato
  della riga.
- La scadenza si confronta con `now()` **letto dalla stessa riga bloccata**, non
  con l'orologio dell'applicazione: uno scarto fra i due host aprirebbe una
  finestra in cui un token scaduto si rinnova.

### 4.3 `authenticate` rifiuta cinque condizioni, indistinguibili

Token sconosciuto · scaduto · consumato · revocato · **utente disabilitato**.
Tutte restituiscono `NotFound` dall'esterno: nessun oracolo.

### 4.4 Password

**Argon2id** con parametri OWASP: `m = 19456 KiB`, `t = 2`, `p = 1`,
versione `0x13`.

`verify_password` restituisce `false` — **mai un errore, mai un panico** — se
l'hash memorizzato è malformato: un record corrotto nega l'accesso invece di
far esplodere il login.

Il login usa un **hash Argon2id reale come dummy** per il caso «utente
inesistente», così il tempo di risposta è comparabile e l'esistenza di un utente
non è deducibile. Una stringa PHC malformata non funzionerebbe: il parsing
fallisce e la funzione ritorna subito, senza eseguire Argon2.

### 4.5 Cookie

`__Host-kpx_session`, con `Secure` · `HttpOnly` · `SameSite=Lax` · `Path=/` ·
`Max-Age`.

**`Secure` è incondizionato.** Il prefisso `__Host-` esige la presenza
*letterale* dell'attributo indipendentemente dal trasporto: senza, un browser
conforme scarta il cookie **per intero**. Separatamente, i browser esentano le
origini loopback dal requisito che un cookie `Secure` viaggi su TLS — quindi
funziona anche in sviluppo su `127.0.0.1` in chiaro. Verificato con un browser
reale.

**Non reintrodurre logica condizionale sull'host**: è già stato sbagliato una
volta (R7).

---

## 5. Superficie HTTP

### 5.1 Endpoint

| Metodo | Percorso | Note |
|---|---|---|
| `GET` | `/health` | `{status, version}` |
| `GET` | `/api/openapi.json` | specifica generata dal codice |
| `GET` | `/api/v1/setup/status` | `{initialised}`, pubblico |
| `POST` | `/api/v1/setup` | primo admin; `409` se già configurata |
| `POST` | `/api/v1/auth/login` | `401 keeppix/invalid-credentials` |
| `POST` | `/api/v1/auth/refresh` | `204` + cookie ruotato |
| `POST` | `/api/v1/auth/logout` | `204` sempre, anche senza cookie |
| `GET` | `/api/v1/auth/me` | richiede `Auth` |
| `GET` | `/*` | frontend incorporato + fallback SPA |

### 5.2 Errori — RFC 9457

Ogni errore è `application/problem+json` con un campo `type` **stabile**
prefissato `keeppix/`. Il backend **non traduce**: `title` è in inglese e serve
al debug, la traduzione avviene nel frontend a partire dal codice.

Codici esistenti: `not-found` · `unauthenticated` · `forbidden` ·
`internal-error` · `conflict` · `already-initialised` · `invalid-credentials` ·
`invalid-username` · `invalid-password` · `service-unavailable` ·
`csrf-check-failed` · `unsupported-media-type` · `invalid-json` ·
`method-not-allowed`

**Anche le rejection native di axum** passano da qui: `keeppix_api::Json<T>`
(wrapper su `axum::Json`) produce `415`/`400`/`422` in `problem+json`, e
`method_not_allowed_fallback` produce `405`. Un client API non riceve mai HTML
né testo semplice.

### 5.3 Header di sicurezza — su **ogni** risposta

Applicati da `with_common_layers`, su rotte esistenti, 404, 405, fallback SPA e
documento OpenAPI:

```
x-content-type-options: nosniff
referrer-policy: no-referrer
content-security-policy: default-src 'self'; script-src 'self';
    style-src 'self'; img-src 'self' data: blob:; connect-src 'self';
    frame-ancestors 'none'; base-uri 'none'; form-action 'self'
permissions-policy: camera=(), microphone=(), geolocation=()
strict-transport-security: max-age=31536000; includeSubDomains
cache-control: private     (se non già impostato dalla rotta)
```

**Nessuna deroga `unsafe-*` nella CSP.** L'asserzione dei test verifica la
policy **direttiva per direttiva**, non con `contains`: `default-src 'self' *`
contiene `default-src 'self'` e non deve passare.

`Cache-Control` usa `if_not_present`, **non** `overriding`, così gli asset
hashati conservano `public, max-age=31536000, immutable`.

### 5.4 L'ordine che va rispettato

> **`.fallback(...)` va registrato PRIMA di `with_common_layers(...)`.**

In axum 0.8 `Router::fallback` **sovrascrive** il catch-all invece di fondersi
con quello già avvolto, e `.layer()` avvolge solo il fallback esistente al
momento della chiamata. Mettendolo dopo, ogni 404 esce **senza header di
sicurezza**.

Vale per tutti i punti di montaggio, `embed::mount()` compreso — la funzione che
costruisce il binario spedito. Verificato per mutazione su tutti e quattro.

### 5.5 CSRF

Difesa in due metà, entrambe presenti:

1. `SameSite=Lax` + obbligo di `Content-Type: application/json` (imposto da
   `Json<T>`);
2. **header custom `x-keeppix-client` obbligatorio** su POST/PUT/PATCH/DELETE
   dentro `/api/v1`, con `403 keeppix/csrf-check-failed` se assente.

Un form HTML da un altro sito non può impostare un header custom. I metodi
sicuri (GET/HEAD/OPTIONS) sono esclusi.

### 5.6 Il punto unico di autenticazione

**L'extractor `Auth` è l'unico modo in cui un `AuthContext` entra nel livello
HTTP.** L'unico `AuthContext::user(...)` fuori dai test è dentro
`SessionRepo::authenticate`.

Conseguenza: la cache prevista dallo spec §9.4 si inserirà lì e da nessun'altra
parte.

Mappatura degli errori (funzione unica `extract::session_problem`, usata sia
dall'extractor sia da `refresh`):

| Errore | Risposta |
|---|---|
| `Connection` (database irraggiungibile) | `503 keeppix/service-unavailable` + `Retry-After` |
| `NotFound` / `Forbidden` | `401 keeppix/unauthenticated` |
| `Corrupted` / `Migration` | `500` — una riga malformata è un difetto del server, non una sessione scaduta |

Prima di questa correzione un riavvio di Postgres si presentava a **tutti** i
client come «sessione scaduta».

---

## 6. Configurazione

Precedenza: **variabili d'ambiente → `config.toml` → default**.

L'unica variabile obbligatoria è `DATABASE_URL`, accettata anche senza prefisso
perché è la convenzione che tutti si aspettano.

| Variabile | Default |
|---|---|
| `DATABASE_URL` | — (obbligatoria) |
| `KEEPPIX_BIND` | `0.0.0.0:5673` |
| `KEEPPIX_DATA_DIR` | `/data` |
| `KEEPPIX_DB_MAX_CONNECTIONS` | `10` |
| `KEEPPIX_SESSION_TTL_SECS` | `2592000` (30 giorni) |
| `KEEPPIX_LOG_FORMAT` | `json` |
| `KEEPPIX_ALLOWED_ORIGINS` | `[]` |

**Nessun segreto predefinito**: la chiave di sessione è generata al primo avvio
e persistita.

CLI: `keeppix serve` (default) · `keeppix migrate` · `keeppix healthcheck`.

`healthcheck` esiste perché l'immagine distroless **non ha né shell né curl**:
passa da `Config::load` e sonda la porta realmente configurata.

---

## 7. Frontend

**Vue 3 + TypeScript + Vite + Tailwind v4 + Reka UI.** Niente Vuetify.

- Bundle iniziale misurato: **~77 KB gzip** su un budget di 150 KB, verificato
  in CI. I chunk lazy per rotta sono fuori dal budget.
- Rotte: `/setup`, `/login`, `/` (protetta), con guardia che redirige secondo lo
  stato dell'istanza.
- i18n: `vue-i18n`, lingua **rilevata** da `navigator.language`, italiano e
  inglese. Test in CI che verifica che le due lingue abbiano le stesse chiavi e
  nessuna traduzione vuota.
- `apiFetch` è **l'unico** `fetch` del frontend: invia sempre
  `x-keeppix-client`, gestisce `problem+json`, e traduce dal codice `type`.
- Tema chiaro/scuro da preferenza di sistema.

**Deroga registrata**: i plurali usano la sintassi nativa di `vue-i18n`, non ICU
MessageFormat. Italiano e inglese hanno due categorie plurali CLDR, che è
esattamente ciò che la sintassi nativa esprime. Da riaprire alla prima lingua
con più di due categorie, e allora con un compilatore a build time.

---

## 8. Distribuzione

**Immagine distroless** `gcr.io/distroless/cc-debian12`, multi-arch amd64 +
arm64. Misurata: **58,6 MB**.

- **Nessuna shell**: `/bin/sh` e `/bin/bash` assenti (exit 127 al tentativo).
- Utente `nonroot:nonroot`, root filesystem in sola lettura,
  `no-new-privileges`, capability azzerate.
- glibc, **non musl**: l'allocatore di musl è lento sui carichi Rust
  multi-thread, ed è esattamente il caso dell'elaborazione immagini.
- `HEALTHCHECK` via sottocomando del binario.

`frontend/dist` **non è un prerequisito dei test ma della compilazione**:
`rust-embed` la incorpora a compile time, quindi senza di essa `keeppix-server`
non compila affatto.

Compose a profili: `--profile bundled` avvia anche Postgres; con `DATABASE_URL`
esterno il servizio `db` non parte. **Anche per fermare serve il profilo**,
altrimenti il database resta acceso.

---

## 9. Qualità e CI

Suite: **107 esecuzioni di test Rust** + 9 vitest, tutte verdi. I test di
integrazione girano contro **Postgres reale** via testcontainers, un container
per test.

CI su GitHub Actions, quattro job: `backend` · `frontend` · `audit` · `image`.
Tempi misurati al primo run reale: 10m28s a cache fredda, 5m23s a caldo.

`cargo deny check advisories bans licenses` verde.

**`--test-threads=1` è obbligatorio**: i test di `keeppix-server/tests/config.rs`
manipolano l'ambiente di processo.

---

## 10. Cosa la Fase 1 può dare per scontato

- Un database migrato, con `pg_trgm` e `postgis` attivi.
- Utenti, sessioni, autenticazione funzionante.
- `AuthContext` che arriva agli handler solo attraverso `Auth`.
- `Problem` (RFC 9457) e `Json<T>` per gli errori.
- Header di sicurezza applicati ovunque.
- `keeppix-test-support` per le asserzioni condivise fra crate.
- Immagine, compose e CI funzionanti.

## 11. Cosa la Fase 1 non deve rompere

I sette invarianti di [`/AGENTS.md`](../../../AGENTS.md), più i tre punti di
frizione noti che vanno affrontati **in Fase 1**, non dopo:

1. **Il fallback SPA inghiotte `/media/*` e `/dav/*`**: `embed.rs` esclude solo
   `api/`. Una miniatura mancante restituirebbe `index.html` con `200` a un tag
   `<img>`. Due righe adesso.
2. **`Auth` fa una query per ogni richiesta autenticata**: irrilevante oggi, non
   in una griglia da centinaia di richieste.
3. **`Db::ping()` non è usata da `/health`**: un container col pool esaurito
   resta `healthy` per sempre.
