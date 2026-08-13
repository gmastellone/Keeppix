# SDD ledger — plan: docs/superpowers/plans/2026-08-13-keeppix-fase-0.md

Spec: docs/superpowers/specs/2026-08-13-keeppix-design.md (letto)
Branch: fase-0 (da main @ 7b38c1d)
Workspace: .superpowers/sdd/2026-08-13-keeppix-fase-0/

Ruling: lavoro su branch `fase-0` in-place invece che in un worktree separato —
il repo non ha remote (EnterWorktree baseRef `fresh` punta a origin/<default> e
fallirebbe), l'albero era pulito, non c'è lavoro parallelo, e il vincolo SDD è
"non su main", che un branch soddisfa. Costo se sbagliato: l'utente deve spostare
il branch in un worktree, operazione di un comando.

## Scansione pre-volo

### Coppie di task che condividono file o interfacce

| Task A | Task B | Cosa produce A / consuma B | Esito |
|---|---|---|---|
| 1 | 2,3,6 | `keeppix-domain/src/lib.rs`, `Cargo.toml` — creati poi estesi | ok, sempre additivo |
| 2 | 3 | `DomainError` — 3 aggiunge `PasswordHashing` | ok, variante aggiunta |
| 2 | 5 | `NewUser.password_hash: String` (non `PasswordHash`) | ok, voluto: il dominio non impone l'algoritmo al repo |
| 3 | 5 | `hash_password(&Password) -> PasswordHash`, `.as_str()` | ok |
| 4 | 5,6,7 | `tests/harness::TestDb`, `Db`, `DbError` | ok, tutti dichiarano `mod harness;` |
| 4 | 5 | `DbError` — 5 aggiunge `Forbidden` | ok, variante aggiunta |
| 5 | 10 | `find_by_username -> Option<(User, PasswordHash)>` | ok |
| 6 | 7 | `SessionToken::digest() -> [u8;32]` ↔ `bytea` | ok |
| 7 | 10 | `SessionRepo::create(UserId, Duration, Option<&str>)` | ok, `user_agent(&headers)` restituisce `Option<&str>` |
| 8 | 9 | `AppState::new(db, session_ttl_secs: u64)` | ok |
| 8 | 13 | `main.rs` passa da `router(state)` a `router_parts().with_state()` | ok, 13 modifica esplicitamente main.rs |
| 9 | 10,11,13 | `keeppix-api/src/lib.rs` riscritto tre volte | ok ma churn; 13 fissa la forma finale |
| 9 | 11 | `router_without_state()` deve esporre `/api/openapi.json` | ok, step 6 di 11 lo aggiunge a entrambi i router |
| 10 | 11 | `routes/setup.rs`, `routes/auth.rs` annotati con `ToSchema` | ok, additivo |
| 11 | 15 | `docs/api/openapi.json` committato ↔ `git diff --exit-code` in CI | ok |
| 12 | 13 | `frontend/dist` ↔ `rust-embed` folder | ok |
| 12 | 14 | `frontend/package-lock.json` ↔ `COPY` nel Dockerfile | ok, non escluso da `.dockerignore` |
| 13 | 14 | binario con frontend incorporato ↔ immagine | ok |
| 1 | 14 | `rust-toolchain.toml` 1.85 ↔ `rust:1.85-bookworm` | vedi F2 |
| **setup** | **1** | `.gitignore` creato dal setup (`.superpowers/`) ↔ step 9 di 1 lo crea da zero | **F1** |

### Coerenza interna di ciascun task

| Task | Test contro codice, file creati contro file toccati | Esito |
|---|---|---|
| 1 | nessun test, solo build | ok |
| 2 | 8 test ↔ `Username`, `UserId`, `AuthContext` | ok |
| 3 | 7 test ↔ `Password`, `hash_password`, `verify_password` | ok |
| 4 | 4 test ↔ migrazioni e `Db` | ok |
| 5 | 8 test ↔ 5 metodi di `UserRepo` | **F2** (let-chain in `map_unique_violation`) |
| 6 | 5+3 test ↔ `SessionToken`, `SettingsRepo` | ok |
| 7 | 8 test ↔ 5 metodi di `SessionRepo` | ok |
| 8 | 4 test ↔ `Config::load` | **F2** (let-chain in `Config::load`) |
| 9 | 3 test ↔ router, header, problem | ok |
| 10 | 10 test ↔ 6 endpoint | **F3** (`dummy_hash` non fa ciò che dichiara) |
| 11 | 2 test ↔ documento OpenAPI | ok |
| 12 | 6 test ↔ `apiFetch`, traduzioni | ok |
| 13 | 3 test ↔ fallback SPA | ok |
| 14 | verifiche manuali ↔ immagine | ok |
| 15 | workflow ↔ comandi eseguibili in locale | ok |

### Rulings pre-volo

**F1 — `.gitignore` sovrascritto.** Il setup ha creato `.gitignore` con
`.superpowers/`; lo step 9 del Task 1 lo crea da zero con altro contenuto,
cancellando la riga.
Ruling: il Task 1 **estende** il `.gitignore` esistente invece di sovrascriverlo,
mantenendo `.superpowers/`. Costo se sbagliato: il workspace SDD finirebbe nei
commit — visibile subito in `git status`.

**F2 — let-chain con toolchain 1.85.** Il piano fissa `rust-toolchain.toml` a
1.85.0, ma il codice di `keeppix-db/src/users.rs` (`map_unique_violation`) e di
`keeppix-server/src/config.rs` (`Config::load`) usa `if let … && …`, che è
stabile solo da **Rust 1.88**. Con 1.85 entrambi i task falliscono a compilare.
Ruling: **alzare la toolchain a 1.88.0** in `rust-toolchain.toml` (Task 1),
`rust-version` del workspace, `rust:1.88-bookworm` nel Dockerfile (Task 14) e
`dtolnay/rust-toolchain@1.88.0` in CI (Task 15). Preferito alla riscrittura del
codice perché tocca 4 righe invece di due funzioni, e i let-chain rendono quel
codice più leggibile. Costo se sbagliato: nessuno noto — 1.88 è stabile da tempo
e non introduce breaking change rispetto a 1.85.

**F3 — `dummy_hash()` non pareggia i tempi.** In `routes/auth.rs` la verifica
fittizia per utente inesistente usa una stringa PHC malformata: `verify_password`
fallisce il parsing e ritorna subito, quindi **non** esegue Argon2 e non elimina
la differenza di tempo che dovrebbe mascherare. Il commento dichiara una
protezione che il codice non fornisce.
Ruling: il Task 10 deve usare un **hash Argon2id valido** di una password fissa,
generato una volta e incollato come costante, così la verifica fittizia svolge lo
stesso lavoro di quella vera. Il test `login_fails_identically_for_unknown_user`
resta valido. Costo se sbagliato: l'esistenza di un utente resta deducibile dai
tempi di risposta — difetto reale ma non critico su un'istanza familiare.

## Avanzamento

Task 1: minor (deferred): i sette lib.rs contengono un `\n` invece di essere a 0 byte (artefatto dello strumento di scrittura, funzionalmente identico)
Task 1: complete (commits d90022f..fcd5368, review clean — spec ✅, quality approved; build/clippy riverificati dal controller, rustc 1.88.0)
Task 2: minor (deferred): `Username::parse` misura la lunghezza in byte anziché in char — innocuo perché l'alfabeto ammesso è solo ASCII, ma un input non-ASCII troppo corto riceve l'errore "caratteri non validi" invece di "lunghezza"
Task 2: minor (deferred): `GroupId` non ha test diretti (generato dalla stessa macro di `UserId`, che è testata)
Task 2: minor (deferred): `#[allow(clippy::unnecessary_wraps)]` su `AuthContext::user_id()` senza commento `// reason:` — la motivazione (futura variante ShareLink) vive solo nel report
Task 2: fix round 1/5 avviato — Important: `User::is_active()` senza copertura di test (entrambi i rami)
Task 2: fix round 1/5 (1 addressed, 0 open — is_active() ora coperto su entrambi i rami; commits f29394b..c0382a0)
Task 2: complete (commits fcd5368..c0382a0, review clean — spec ✅, quality approved)
Task 3: minor (deferred): `Password::parse` limita a 1024 *caratteri*, non byte — una password di 1024 emoji arriva ad Argon2 come ~4096 byte. Conforme al brief, ma i limiti di dimensione HTTP dei task successivi vanno pensati in byte
Task 3: minor (deferred): `Password` non azzera il buffer in `Drop` e deriva `Clone` — hardening futuro, fuori dal brief
Task 3: nota: la verifica Argon2 legge i parametri dalla stringa PHC, non dall'istanza `Argon2`; l'helper condiviso è igiene, non correttezza
Task 3: complete (commits c0382a0..db82b8c, review clean — spec ✅, quality approved)

Ruling (pre-Task 4, conflitto spec↔piano): lo spec §9.5 dichiara "sqlx con query
verificate a compile-time", ma tutto il codice del piano (Task 4,5,6,7) usa le
forme *funzione* `sqlx::query(...)` / `query_scalar(...)`, che sono verificate a
runtime — non le macro `query!`. Di conseguenza gli step 11-12 del Task 4
(`cargo sqlx prepare`, `SQLX_OFFLINE=true`) produrrebbero una cache vuota e non
verificherebbero nulla.
Decisione: si tengono le forme funzione con parametri bound. La proprietà di
sicurezza che lo spec vuole davvero — nessuna concatenazione di stringhe, query
parametrizzate — è pienamente soddisfatta; la verifica di schema a compile-time
è coperta meglio dai test di integrazione contro un Postgres reale, che il piano
già impone per ogni repository. Gli step 11-12 del Task 4 vengono saltati, e i
riferimenti a `.sqlx`/`SQLX_OFFLINE` vanno rimossi dal Dockerfile (Task 14) e
dalla CI (Task 15).
Costo se sbagliato: un refuso in un nome di colonna fallisce in test invece che
in build — qualche secondo più tardi, mai in produzione.
Task 4: minor (deferred): `serde`, `chrono`, `tracing`, `tokio` aggiunti a keeppix-db ma non ancora usati (provisioning voluto dal brief per i task successivi)
Task 4: nota: dipendenza diretta `testcontainers` rimossa per conflitto di versione con `testcontainers-modules` (che pinna ^0.27); l'harness usa il re-export `testcontainers_modules::testcontainers::*`, importabile identico dai Task 5/6/7
Task 4: complete (commits db82b8c..ada768e, review clean — spec ✅, quality approved)
Task 5: minor (deferred): `map_unique_violation` scarta l'errore sqlx sottostante — collisioni su username ed email producono lo stesso messaggio, perdendo segnale di debug
Task 5: minor (deferred): il ramo "already initialised" di `create_bootstrap_admin` si affida al Drop della transazione invece di un `tx.rollback()` esplicito (corretto ma meno leggibile in un percorso concorrente)
Task 5: minor (deferred): `uuid.workspace = true` resta ridondante in `[dev-dependencies]` dopo la promozione a `[dependencies]`
Task 5: fix round 1/5 avviato — Important: (a) `DbError::Migration` sovraccaricato per righe corrotte, (b) i due test di access-control asseriscono solo `.is_err()` e non coprono l'oracolo di esistenza
Task 5: fix round 1/5 (2 addressed, 0 open — DbError::Corrupted introdotto; test di access-control ora inchiodano la variante e coprono l'oracolo di esistenza; commits 3f7e1ca..78be6fc)
Task 5: complete (commits ada768e..78be6fc, review clean — spec ✅, quality approved)
Task 6: minor (deferred): `sha2 = "0.11"` in keeppix-domain duplica l'albero già presente via sqlx/argon2 (sha2 0.10.9) — bloat di build, nessun impatto funzionale; pinnare a 0.10 deduplicherebbe
Task 6: minor (deferred): `digest()` calcola SHA-256 sui byte UTF-8 della stringa base64url, non sui 32 byte grezzi — equivalente crittograficamente, ma da sapere se codice futuro assume i byte grezzi
Task 6: nota per il Task 7: `SessionToken` deriva `PartialEq` (confronto non a tempo costante). Va bene finché la validazione passa dal lookup del digest in DB; un confronto diretto `==` fra token reintrodurrebbe un canale laterale temporale
Task 6: Ruling: il brief (step 9) prescrive `DbError::Migration` per un segreto memorizzato illeggibile, ma il fix round del Task 5 ha introdotto `DbError::Corrupted` esattamente per "dato già in DB malformato". Il piano è anteriore a quella decisione. Vince il finding: si usa `Corrupted`, per non spezzare la tassonomia degli errori su cui si fa triage. Costo se sbagliato: nessuno — le due varianti sono entrambe gestite dal ramo catch-all previsto nel Task 9
Task 6: fix round 1/5 avviato — Important: `Migration` usato al posto di `Corrupted` in settings.rs
Task 6: fix round 1/5 (1 addressed, 0 open — entrambi i siti in settings.rs ora usano Corrupted; commits 26ec9f5..362af6e)
Task 6: complete (commits 78be6fc..362af6e, review clean — spec ✅, quality approved)
Task 7: minor (deferred): `interval()` tronca i TTL sub-secondo (`Duration::from_millis(500)` -> "0 seconds", token nato scaduto senza errore); `as_secs_f64()` sarebbe fedele
Task 7: minor (deferred): ruolo sconosciuto in sessions.rs degrada a `SystemRole::User` invece di `DbError::Corrupted` come in users.rs — irraggiungibile grazie al CHECK, e fallisce chiuso, ma le due tassonomie divergono
Task 7: minor (deferred): `rotate` non controlla `users.disabled_at` — disabilitare un utente non termina la sua catena di sessioni (authenticate blocca comunque l'accesso). Da decidere quando si costruirà il percorso di disabilitazione
Task 7: minor (deferred): doc `# Errors` di `rotate` omette il caso "revocato" fra i NotFound
Task 7: minor (deferred): due replay concorrenti sulla stessa famiglia possono deadlockare (Postgres aborta uno con 40P01 -> DbError::Connection invece di Forbidden); l'esito di sicurezza regge, degrada solo il codice d'errore del perdente
Task 7: nota informativa: un client legittimo che invia due refresh (retry, due schede) è indistinguibile da un furto e uccide la famiglia — inerente al design senza finestra di grazia, il Task 10 deve aspettarsi re-login occasionali
Task 7: Ruling: includo nel fix round anche il finding M1 (classificato Minor) — il test `revoking_logs_out_only_that_session` crea due famiglie distinte, quindi passerebbe anche se `revoke` fosse allargato all'intera famiglia. È la stessa categoria di I2 (rigore dei test su una proprietà di sicurezza), tocca lo stesso file e lo stesso implementer è già in contesto. Costo se sbagliato: un giro di fix marginalmente più lungo
Task 7: fix round 1/5 avviato — Important: (a) `rotate` confronta la scadenza con l'orologio dell'app invece che del DB, (b) tre dei cinque rami di `rotate` senza test; piu M1
Task 7: minor (deferred): `rotate_rejects_an_expired_token` non distingue strutturalmente "quale orologio" viene usato — app e DB condividono lo stesso host/rete Docker, quindi il test sarebbe passato anche con il bug pre-fix. Verifica il comportamento, non la causa. Una guardia più forte richiederebbe iniezione dell'orologio, non presente nel codebase; forma del test comunque prescritta dal brief
Task 7: nota di provenienza: l'implementer è morto per limite di sessione API dopo aver scritto il fix ma prima di verificare/committare. Il controller ha verificato (11/11 test, clippy pulito, fmt applicato) e committato lui stesso il codice esattamente come lasciato dall'implementer, in `8835447`
Task 7: fix round 1/5 (3 addressed, 0 open — orologio DB invece di app clock; 3 test su rotate aggiunti; revoke ora provato sulla stessa famiglia; commits 78d970d..8835447)
Task 7: complete (commits 362af6e..8835447, review clean — spec ✅, quality approved)
Task 8: minor (deferred): `clear_env()` in tests/config.rs azzera solo 4 delle 7 chiavi KEEPPIX_* possibili (mancano DB_MAX_CONNECTIONS, SESSION_TTL_SECS, ALLOWED_ORIGINS) — nessuna perdita oggi, trappola latente per chi scriverà nuovi test. Verbatim dal brief
Task 8: minor (deferred): nessun test committato copre "env DATABASE_URL vince sul database_url del file" (verificato a mano dal reviewer, la proprieta regge). Verbatim dal brief
Task 8: minor (deferred): il messaggio d'errore di DATABASE_URL mescola inglese e italiano ("is required (es. ...)"). Verbatim dal brief
Task 8: fix round 1/5 avviato — Important: (a) `.expect()` non annotato in config.rs:42 fara fallire la CI del Task 15, (b) `healthcheck()` ignora Config::load e sonda la porta 5673 hardcoded invece di quella configurata
Task 8: fix round 1/5 (2 addressed, 0 open — .expect() sostituito da SocketAddr::from infallibile; healthcheck passa da Config::load e usa cfg.bind.port(); commits 6e4246b..74890b4)
Task 8: nota: healthcheck ora dipende da DATABASE_URL via Config::load anche se non tocca il DB. Accettabile: HEALTHCHECK gira nello stesso container con lo stesso ambiente di `serve`, quindi le due invocazioni vedono la stessa configurazione
Task 8: complete (commits 8835447..74890b4, review clean — spec ✅, quality approved)
Task 9: minor (deferred): `Auth::from_request_parts` mappa *qualsiasi* errore di `SessionRepo::authenticate` — incluso `DbError::Connection`, cioè database irraggiungibile — a `401 keeppix/unauthenticated`. Un blip del DB apparirebbe come "sessione scaduta" a tutti i client invece che come 5xx, e il frontend li rimanderebbe al login in massa. Verbatim dal brief (step 6), quindi difetto del piano, non dell'implementer. Da valutare nel review finale: `Connection` -> 503, il resto -> 401
Task 9: Ruling (difetto del piano trovato dal review): il `common_layers` che ho scritto nel piano chiama `.fallback(not_found)` DOPO la catena `.layer(...)`. In axum 0.8 `Router::fallback` sovrascrive il catch_all_fallback invece di fondersi con quello gia avvolto, e `.layer()` avvolge solo il fallback esistente al momento della chiamata: risultato, le rotte non trovate escono senza CSP, nosniff, referrer-policy e permissions-policy. Il reviewer lo ha verificato costruendo un test usa-e-getta e poi lo ha rimosso. Correzione: `.fallback()` va spostato PRIMA dei `.layer()`. Lo stesso ordine va rispettato nel Task 13, che ristruttura `common_layers`. Costo se sbagliato: nessuno, la correzione e verificata
Task 9: fix round 1/5 avviato — Critical: header di sicurezza assenti sulla rotta di fallback; Important: nessun test copriva quel caso
Task 9: fix round 1/5 (2 addressed, 0 open — .fallback() spostato prima dei .layer() con commento sul meccanismo; helper condiviso assert_security_headers chiamato da entrambi i test, red-then-green documentato con transcript reale; commits 53d842d..a040007)
Task 9: complete (commits 74890b4..a040007, review clean — spec ✅, quality approved)
Task 10: minor (deferred): `reqwest` di test usa la feature `rustls`, che tira dentro aws-lc-rs/aws-lc-sys/quinn/jni (~440 righe di Cargo.lock + requisito cmake e toolchain C) per test che parlano HTTP in chiaro a 127.0.0.1. Togliere la feature tenendo json+cookies ridurrebbe la superficie di build in CI
Task 10: minor (deferred): `refresh` non ricontrolla l'utente — `rotate` tocca solo `sessions`, quindi un account disabilitato puo coniare token all'infinito. Innocuo oggi perche `authenticate` fa join su `users.disabled_at IS NULL`, ma la famiglia non muore mai
Task 10: minor (deferred): le risposte 405 (es. GET su /auth/login) restituiscono il corpo vuoto di default di axum, non `application/problem+json`. Gli header di sicurezza si applicano comunque
Task 10: minor (deferred): nessun rate limiting su /auth/login e /setup; i ~100ms di Argon2 danno solo un throttling incidentale
Task 10: Ruling (F4, host client-controlled): `should_be_secure` fa prefix-match su `Host`, quindi `localhost.evil.com` e `127.0.0.1.evil.com` passano per localhost. Il reviewer ha concluso — correttamente — che e DoS-grade e non disclosure-grade: il browser imposta Host dall'origine visitata, e dove l'header viene riscritto il prefisso `__Host-` fa comunque rifiutare il cookie non-Secure, quindi il login fallisce invece di far trapelare il cookie. Decisione: correggo con match esatto dopo lo strip della porta, piu `[::1]` e `::1`. NON sposto la decisione in configurazione (`AppState.secure_cookies`), che sarebbe architetturalmente piu pulito ma richiede di infilare un campo nuovo lungo Config -> AppState -> entrambi i router -> le funzioni cookie, cioe una modifica di interfaccia del Task 8 sproporzionata rispetto al rischio residuo. Annotato come miglioramento per il review finale. Costo se sbagliato: resta un header client-controlled che decide un attributo di sicurezza, mitigato dal prefisso `__Host-`
Task 10: fix round 1/5 avviato — spec ❌ + 6 Important: F1 clearing_cookie senza Secure viola `__Host-`, F2 il test di pinning del dummy-hash non pinna nulla, F3 should_be_secure senza copertura, F4 prefix-match su Host, F5 logout_invalidates_the_session non prova la revoca server-side, F6 refresh_rotates non prova che il vecchio cookie muoia; piu rimozione di `pub type Ctx`
Task 10: fix round 1/5 (7 addressed, 0 open — F1 clearing_cookie(secure) con Secure+SameSite; F2 test di pinning che asserisce il parsing in positivo; F3 tre unit test su should_be_secure inclusi i lookalike host; F4 match esatto con strip_port e letterali IPv6; F5 logout riprovato con cookie esplicito su client fresco; F6 refresh_rejects_a_reused_token aggiunto; `pub type Ctx` rimosso)
Task 10: nota di provenienza: l'implementer e stato fermato dal controller mentre stava per dimostrare il red-then-green di F5. Tutte le correzioni erano gia complete e coerenti (revoke() intatto, Ctx rimosso, clearing_cookie cablato). Il controller ha verificato: 11 test auth + 3 health + 4 unit = tutti verdi, clippy pulito, fmt applicato, e ha committato
Task 10: fix round 1/5 committato in 4132af7; re-review formale NON eseguita per passaggio a sessione cloud

## Consegna a sessione cloud (2026-08-13)

Il workspace `.superpowers/` e stato tolto da .gitignore e committato, insieme a
docs/superpowers/plans/2026-08-13-keeppix-fase-0-STATO.md che riassume ruling e
difetti differiti. Prossimo passo: re-review del fix round del Task 10, poi
Task 11 (OpenAPI).

## Ripresa in sessione cloud (2026-08-13)

Ruling R9 (ambiente): il pull di `postgis/postgis` e bloccato dalla policy di
egress di questo ambiente (403 su production.cloudfront.docker.com al CONNECT,
confermato da `curl $HTTPS_PROXY/__agentproxy/status`). Testcontainers e quindi
inutilizzabile e l'intera suite di integrazione non era eseguibile: il primo
reviewer ha dovuto patchare gli harness a mano e ripristinarli. Reso permanente
e pulito: se `KEEPPIX_TEST_DATABASE_URL` e impostata, i due harness usano il
server Postgres gia in ascolto a quell'indirizzo creando un database vergine per
test (stesso isolamento del container); senza la variabile il comportamento e
invariato. Commit 55de9b9. Comando di verifica in questo ambiente:
`export KEEPPIX_TEST_DATABASE_URL="postgres://keeppix:keeppix@127.0.0.1:5432/postgres"`
piu `cargo test --workspace -- --test-threads=1` (il flag serve ai test di
config.rs, che manipolano l'ambiente di processo — vincolo pre-esistente).
Caveat: qui gira PostgreSQL 16 invece del 17 dell'immagine; le migrazioni
chiedono solo pg_trgm. Costo se sbagliato: gli harness hanno un ramo in piu, e
la logica di riscrittura dell'URL e duplicata nei due crate (con unit test solo
nella copia di keeppix-db).

Task 10: re-review formale del fix round 1/5 (diff ab19d33..4132af7) — spec OK,
qualita approvata con riserva. Sei dei sette finding verificati **per
mutazione**: rompendo il codice di produzione (DUMMY_HASH corrotto, revoke
disattivato in logout, rotate sostituito da authenticate+create, starts_with
rimesso in should_be_secure) il test corrispondente diventa rosso. F1 verificato
per osservazione diretta degli header, non da un test — da cui N1.
Task 10: minor (deferred): `dummy_hash_is_a_valid_argon2id_phc_string` e
leggermente sovra-pinnato — mutare il solo salt lo fa fallire pur lasciando
intatta la proprieta di sicurezza. Prezzo inevitabile dell'asserzione positiva:
ruotare la costante impone di rigenerarla dal plaintext dichiarato
Task 10: minor (deferred): `logout` risponde 204 anche quando `revoke` fallisce
(errore solo loggato a warn). Con un blip del database l'utente crede di essere
uscito mentre la sessione resta viva lato server fino alla scadenza. Coerente
con il commento "Sempre 204", ma il fallimento e invisibile al client
Task 10: minor (deferred): `should_be_secure` e case-sensitive e non riconosce
`0:0:0:0:0:0:0:1` ne `[127.0.0.1]`. Tutti questi casi cadono sul lato sicuro
(`Secure = true`): rompono al massimo uno sviluppo locale esotico
Task 10: minor (deferred): `setup_creates_the_first_admin_and_logs_in` verifica
gli attributi del cookie con `contains` sull'header intero e senza contraffare
`Host`, quindi non puo vedere `Secure` ed e esposto in linea di principio al
falso positivo del token casuale. La proprieta e ora coperta dai due test nuovi
Task 10: nota per il review finale del branch (osservazione di spec, non un
finding del Task 10): §9.5 chiede come difesa CSRF `SameSite=Lax` **piu**
obbligo di `Content-Type: application/json` e di un header custom sulle
mutazioni. `/auth/refresh` e `/auth/logout` non hanno ne corpo ne header custom
richiesto. Non e nel contratto del Task 10 e `SameSite=Lax` da solo gia impedisce
l'invio del cookie su POST cross-site
Task 10: fix round 2/5 avviato — Important N1: F1 chiuso senza test di
regressione (rimuovendo Secure e SameSite da `clearing_cookie` la suite resta
verde); Minor N2: il doc comment di `refresh_rejects_a_reused_token`
sovradichiara la copertura della revoca di famiglia
Task 10: fix round 2/5 (2 addressed, 0 open — due test con `Host` contraffatto
(`photos.example.com`) che rendono `Secure` osservabile, piu un helper che
splitta l'header `set-cookie` su `;` e confronta gli attributi per uguaglianza
invece di usare `contains`: il valore del token e base64url casuale e potrebbe
contenere `Secure` o `Path=/` per caso. N2 chiuso nella forma forte — il token
*nuovo* post-rotazione viene ripresentato a /auth/me e deve dare 401. Quattro
mutazioni verificate rosse; codice di produzione non toccato, +108 righe di soli
test. Commit 90f8b82)
Task 10: complete (commits a040007..90f8b82, tutti i finding risolti e
riverificati; suite 85 test verdi, clippy pulito, fmt pulito)

Ruling R10 (branch): l'utente ha chiesto esplicitamente di pushare sul branch
della fase in corso — `fase-0` — invece del branch `claude/...` imposto di
default dall'harness della sessione cloud. Il branch `claude/keeppix-fase-0-4c0lku`
e stato cancellato in locale e su origin dopo aver verificato che i suoi commit
fossero tutti contenuti in `fase-0`.

## Allineamento al protocollo superpowers (2026-08-13, sessione cloud)

Il plugin `superpowers@superpowers-marketplace` (6.3.0) non era installato in
questa sessione: i Task 10-11 sono stati eseguiti con una versione a mano del
metodo. Installato su richiesta dell'utente e riallineato il flusso alla skill
`subagent-driven-development`. La directory di workspace che gli script
risolvono (`.superpowers/sdd/2026-08-13-keeppix-fase-0/`) coincide con quella
gia in uso, quindi non c'e stata migrazione. Brief canonici rigenerati con
`scripts/task-brief` per i Task 12-15; le mie note di pre-volo per il Task 12
sono state spostate in `task-12-preflight.md` e viaggiano nel dispatch, non nel
brief, come prescrive la skill.

Ruling R11: la skill impone `rm -rf <workspace>` alla fine, perche "la storia di
git e il record". Questo repository ha deliberatamente fatto la scelta opposta —
`.gitignore` documenta che `.superpowers/` NON e ignorato e i file sono
force-added — e l'utente ha esplicitamente chiesto di mantenerli. Vince la scelta
del repository: il workspace non viene cancellato. Costo se sbagliato: ~600 KB di
testo versionato.

Ruling R12: nel dispatch della review del Task 11 ho elencato i Minor gia noti
chiedendo di non ri-segnalarli come nuovi. La skill lo vieta esplicitamente
("Do not pre-judge findings for the reviewer"): il rischio e che il reviewer
taccia un difetto che avrebbe classificato piu grave. Ho mitigato invitandolo a
contestare la classificazione, e la review va comunque letta sapendolo. Dai
Task 12-15 in poi si usano i template canonici, che non contengono questa
istruzione. Costo se sbagliato: un finding sottopesato nel Task 11, da
recuperare nel review finale del branch.

Ruling R13 (conflitto spec<->piano, trovato nella scansione pre-volo del
Task 12): lo spec §9.5 chiede come difesa CSRF `SameSite=Lax` **piu** obbligo di
`Content-Type: application/json` e di un header custom sulle mutazioni. Lo step 6
del Task 12 fa inviare `x-keeppix-client: web` da `apiFetch`, ma **nessun task
del backend verifica quell'header**: la meta client-side esiste, la meta
server-side no. Non lo infilo nel Task 12, il cui elenco di file e tutto sotto
`frontend/` e che non possiede la superficie HTTP: sarebbe un cambiamento di
interfaccia del backend senza review dedicata. Lo porto invece alla fix wave del
review finale del branch, dove un middleware unico puo coprire tutte le rotte —
incluse quelle che il Task 13 aggiunge. La meta client-side implementata ora non
va sprecata: quando l'enforcement arrivera, il frontend sara gia conforme.
Costo se sbagliato: la Fase 0 chiude con la difesa CSRF a meta, mitigata da
`SameSite=Lax`, che gia impedisce l'invio del cookie su POST cross-site.
Task 11: complete-pending-review (commit 9d88cb4) — implementer: utoipa 5.5.0,
ToSchema sui 7 tipi pubblici, #[utoipa::path] sui 6 handler, openapi.rs, rotta
/api/openapi.json montata DENTRO common_layers in entrambi i router, snapshot
docs/api/openapi.json, 3 test. Red-then-green di N1 documentato
Task 11: minor (deferred): i rustdoc `# Errors` finiscono come `summary` nel
contratto pubblico; si toglierebbero solo con summary/description espliciti
Task 11: minor (deferred): il ramo "scrivi se manca" dello snapshot test passa a
vuoto su un checkout senza il file; la protezione reale e il git diff --exit-code
del Task 15
Task 11: minor (deferred): il fallimento dello snapshot stampa due volte il
documento escapato invece di un diff
Task 11: minor (deferred): docs/api/openapi.json non termina con newline (e cio
che scrive il test; aggiungerlo a mano lo renderebbe incoerente dopo una
rigenerazione)
Task 11: minor (deferred, m1 della review): `components(schemas(...))` e
interamente ridondante — rimuovendo tutte e sette le voci il documento e identico
byte per byte, perche utoipa 5 auto-raccoglie gli schemi referenziati
Task 11: minor (deferred, m2 della review): nessun test confronta il documento
*servito* con quello *committato* — iniettando un percorso fittizio dentro
`serve()` tutti e tre i test restano verdi
Task 11: minor (deferred, m4 della review): `info.version` e la versione del
crate, non dell'API
Task 11: review (spec OK, qualita da correggere) — 0 Critical, 5 Important.
Il reviewer ha risposto alla domanda centrale del task per esecuzione: il
documento **puo** divergere dalle rotte montate. Prova A: rotta aggiunta e non
annotata, suite interamente verde. Prova B: `post`->`put` nel solo
#[utoipa::path] di login — lo snapshot fallisce ma il suo messaggio istruisce a
disattivarlo, e dopo `rm docs/api/openapi.json && cargo test` il contratto
dichiara PUT su una rotta POST con la suite verde. Formulazione corretta: non puo
divergere dalla forma dei dati, puo divergere dalla superficie HTTP
Task 11: fix round 1/5 avviato — I1 divergenza documento<->rotte piu messaggio
dello snapshot che suggerisce la scorciatoia; I2 responses non allineate agli
handler (404 su me, 500 mai dichiarato); I3 operationId generici e collidibili
(`create` e `status` collidono gia con la tabella §9.1); I4 Problem fuori dai
components; I5 nessun securitySchemes
Task 11: Ruling (copertura di I1): la direzione "rotta montata e non
documentata" non e verificabile meccanicamente — axum 0.8 non espone la tabella
delle rotte. Si implementa la sola direzione documento->router e si documenta nel
commento del test quale meta resta scoperta, invece di lasciar credere che il
buco sia chiuso. Costo se sbagliato: una rotta non documentata puo restare tale
senza che nessun test se ne accorga
Task 11: nota di provenienza: il primo tentativo di fix round si e interrotto per
limite di sessione dell'API prima di produrre qualsiasi modifica (albero pulito a
327b44f, commit 9d88cb4 intatto); il round e stato rilanciato
Task 11: fix round 1/5 (5 addressed secondo l'implementer, re-review in corso —
I1 test `documented_operations_are_all_mounted` che scarica il documento dal
server reale e colpisce ogni coppia (path, method) sul router con stato, con
`assert_eq!(checked, 6)` contro il ciclo a vuoto, piu messaggio dello snapshot
riscritto senza il comando che disattivava il controllo; I2 404+500 su me, 500 su
login/setup_create/setup_status, con commento sul perche refresh e logout non
hanno 500; I3 operation_id espliciti piu test di unicita; I4 ToSchema su Problem
e body = Problem sulle risposte d'errore; I5 SecurityAddon apiKey/cookie con nome
preso da SESSION_COOKIE piu test che lega il letterale della macro alla costante.
Snapshot rigenerato di proposito, 295 -> 428 righe; tests/openapi.rs da 3 a 6
test; commit adca7c6)
Task 11: nota: il nome reale del cookie e `__Host-kpx_session`, non
`__Host-keeppix_session` come scritto nella review (refuso del reviewer, senza
conseguenze: l'implementer ha preso il valore dalla costante SESSION_COOKIE)
Task 11: nota di provenienza: durante le mutazioni di verifica un helper
dell'implementer ha eseguito `git checkout -- crates/keeppix-api/src`,
cancellando le modifiche del round, che sono state riscritte. La coerenza del
ripristino e verificata dallo snapshot, mai toccato in quel frangente e ancora
combaciante byte per byte; la re-review scoped e comunque il controllo che
conta
Task 11: nota: 403 non e dichiarato su `me` — il ramo DbError::Forbidden e
irraggiungibile perche l'id viene da ctx.user_id(). Scelta dell'implementer,
dichiarata
Task 11: fix round 1/5 (5 addressed, 0 open — re-review scoped che ha rifatto in
proprio tutte e quattro le mutazioni dichiarate dall'implementer, ripristinando
con Edit invece di git checkout per non ripetere l'incidente; nessuna rottura
nuova, diff puramente additivo; commits 9d88cb4..adca7c6)
Task 11: complete (commits 4b5e354..adca7c6, review clean — spec OK, qualita
approvata dopo 1 fix round)

## Scansione pre-volo dei Task 13-15 (fatta prima di dispatchare il 13)

Task 13: **F5 — il brief reintroduce il bug del ruling R5 e lo estende al
fallback SPA.** Lo step 5 ristruttura `common_layers` e rimette `.fallback()`
DOPO i `.layer(...)` in due punti (`router()` e `base_router_stateless()`), e lo
step 4 fa `router.fallback(get(serve))` su un router gia layerizzato. La
conseguenza e peggiore che nel Task 9: il fallback SPA serve `index.html`, cioe
il documento che carica l'applicazione, che uscirebbe senza CSP mentre /health
ce l'ha.
Ruling: l'invariante da rispettare e "ogni risposta che esce dal binario porta i
quattro header di sicurezza", senza prescrivere la forma della ristrutturazione;
i test esistenti su /health e sul 404 non vanno indeboliti, va aggiunta
l'asserzione equivalente sul fallback SPA, e la cosa va provata per mutazione.
Costo se sbagliato: la pagina principale gira senza CSP in produzione.
Task 13: F6 — il brief non prescrive un test per la proprieta che dichiara in
testa ("i percorsi sotto /api non ricadono mai nel fallback"). Un client che
riceve index.html con status 200 al posto di un 404 problem+json e un bug
silenzioso. Ruling: test richiesto.
Task 13: F7 — `frontend_built()` fa saltare i test senza fallire quando
frontend/dist manca: in un ambiente dove il frontend non viene mai costruito
quei test non provano nulla e passano. Ruling: costruire il frontend prima della
suite e dichiarare nel report se qualche test e stato saltato.

Task 14: **F8 — il Dockerfile del brief non compila.** `FROM rust:1.85-bookworm`
(vedi R2, serve 1.88) e soprattutto `COPY .sqlx/ .sqlx/` su una directory che non
esiste e non e mai esistita (vedi R4), che fa fallire la build immediatamente.
Ruling: rimuovere `ENV SQLX_OFFLINE` e il `COPY .sqlx/`, alzare l'immagine a
1.88, e sostituire il commento che spiegava la cache con uno onesto.
Task 14: **F9 — l'immagine non e verificabile in questo ambiente.** Il pull delle
immagini di base e bloccato dalla policy di egress, quindi tutti gli step di
verifica del brief (docker build, compose up, healthcheck, "l'immagine non
contiene shell") sono ineseguibili. Ruling: scrivere comunque gli artefatti
completi, verificare staticamente tutto il verificabile (percorsi dei COPY,
nomi di variabili e servizi contro config.rs e .env.example, esistenza del
sottocomando healthcheck), e dichiarare esplicitamente nel report cosa NON e
stato verificato con l'elenco dei comandi che restano da eseguire. La prima
verifica reale del Dockerfile sara il job `image` della CI.
Task 14: F10 — il commento "strato di dipendenze, invalidato solo dai manifest"
e falso: `COPY crates/ crates/` sta nello stesso strato, quindi ogni modifica al
codice invalida la cache. O si separano gli strati o si corregge il commento.

Task 15: F11 — `SQLX_OFFLINE: "true"` nel blocco env (R4) e
`dtolnay/rust-toolchain@1.85.0` (R2) vanno corretti.
Task 15: **F12 — lo step 5 prescrive `git push -u origin main`.** Il lavoro vive
su `fase-0` e il merge su main e una decisione dell'utente. Ruling: nessun push
dall'implementer.
Task 15: F13 — la lista licenze di `deny.toml` contiene `AGPL-3.0`, ma i crate
del workspace dichiarano `AGPL-3.0-or-later`, identificatore SPDX diverso: cosi
com'e, cargo-deny rifiuta i crate del progetto stesso. E l'albero delle
dipendenze conterra licenze non elencate, non indovinabili. Ruling: eseguire
davvero `cargo deny check` in locale e iterare, motivando nel report ogni
licenza aggiunta.
Task 15: F14 — la CI non deve impostare `KEEPPIX_TEST_DATABASE_URL`: sui runner
GitHub Docker c'e e il percorso testcontainers funziona. Va pero menzionata in un
commento, perche chi legge la CI sappia che la variabile esiste.

Task 12: nota di provenienza — il primo tentativo si e interrotto per limite di
sessione dell'API (account-wide, reset comunicato per le 20:50 UTC) durante lo
step 5 (RED phase), prima di qualunque commit. Il lavoro fin li resta su disco
(non e una worktree separata) e non e stato perso: scaffold Vite, vite.config.ts,
style.css con Tailwind v4, client.spec.ts verbatim dal brief. Trovato e corretto
in autonomia un difetto reale del piano: i18n.spec.ts importa `it` da vitest e
anche un valore `it` da `./it.json` nello stesso scope di modulo — errore di
parse, non di risoluzione moduli. Corretto con import alias
(`enMessages`/`itMessages`), motivazione in un commento nel file. Il tentativo e
stato ripreso sullo stesso agente con l'istruzione di committare a incrementi
coerenti invece di arrivare in fondo ai 16 step in un colpo solo.

## Fix fuori piano: `__Host-kpx_session` mai valido su HTTP semplice (scoperto dal Task 12)

Ruling (finding Critical e load-bearing, non adjudicato in silenzio): la
verifica a mano dello step 15 del Task 12, eseguita con Chromium headless reale
via Playwright, ha trovato che il cookie di sessione non viene MAI accettato dal
browser su HTTP in chiaro — non solo in produzione senza TLS, ma anche su
loopback in sviluppo. Causa: `should_be_secure` (introdotta dal ruling R7 nel
Task 10) omette `Secure` su host locali pensando che serva a farsi accettare il
cookie dai test; ma il prefisso `__Host-` richiede l'attributo `Secure`
*letteralmente presente* per essere valido (RFC 6265bis §4.1.3.2),
indipendentemente dal trasporto — l'eccezione di "origine potenzialmente
affidabile" che i browser danno al loopback rilassa una regola diversa (poter
onorare `Secure` su un trasporto non cifrato), non la presenza dell'attributo.
Omettendolo, Chromium scarta il cookie per intero. Verificato con Chromium reale
(context.cookies() vuoto dopo il setup) e confermato con un secondo canale
(curl grezzo) prima di scriverlo come difetto genuino.

Nessun test Rust esistente poteva aver preso questo bug: nessuna libreria HTTP
generica (reqwest incluso) implementa la validazione del prefisso `__Host-` —
e la solo controllo automatico possibile e leggere l'header set-cookie
letterale, che l'helper `assert_host_prefix_attributes` del Task 10 gia fa, ma
solo su un Host contraffatto a produzione, mai sul flusso di default su cui
gira tutta la suite.

Root-causa confermata anche dal lato opposto: ho verificato empiricamente (server
HTTP di scratch + client reqwest isolato, stessa versione pinnata nel workspace)
che `cookie_store` 0.22.1 — la libreria dietro reqwest 0.13.4 — ha GIA
un'eccezione di trasporto per il loopback identica a quella dei browser: un
cookie Secure emesso su 127.0.0.1 in chiaro torna al client alla richiesta
successiva; lo stesso cookie emesso su un host non-loopback (192.0.2.2) in
chiaro viene scartato. Il presupposto scritto nel codice ("un client conforme
scarterebbe Secure su 127.0.0.1") era quindi falso anche per reqwest, non solo
per i browser: impostare Secure sempre non dovrebbe rompere l'harness di test
esistente.

Ruling: fix immediato, non differito al review finale — blocca un criterio di
completamento esplicito della Fase 0 ("ricarica pagina con sessione
persistente") e romperebbe identicamente la verifica a mano del Task 13.
Dispatchato come fix fuori piano (non un task numerato), brief in
`.superpowers/sdd/2026-08-13-keeppix-fase-0/fix-cookie-host-secure-brief.md`:
`session_cookie`/`clearing_cookie` impostano Secure incondizionatamente,
`should_be_secure`/`strip_port`/`host()` rimossi, i test del Task 10 fix round 2
che contraffacevano Host per rendere Secure osservabile vanno riscritti contro
il client di default (nessuna contraffazione), piu un test end-to-end del
round-trip che avrebbe intercettato il bug originale. Prova di vitalita per
mutazione richiesta. Costo se sbagliato: nessuno noto — l'analisi e verificata
su due fronti (RFC + comportamento empirico della libreria di test).
Questo ruling risolve R7 per intero: non c'e piu alcuna decisione di sicurezza
guidata da un header controllato dal client, quindi il "difetto noto, accettato
e differito" su should_be_secure nello STATO va rimosso, non solo riesaminato,
a fix chiuso.
Task 12: complete-pending-fix (commit 248f4c8) — Vue 3 + Vite + Tailwind v4 +
vue-i18n, apiFetch/ApiProblem, store Pinia di sessione, router con guardia,
3 componenti UI, 3 viste, it/en. 6/6 vitest verdi, vue-tsc pulito, eslint pulito,
bundle 76.672 byte gzip contro un tetto di 153.600
Task 12: review (spec OK, qualita approvata) — 0 Critical, 1 Important, 3 Minor.
Il reviewer ha riprodotto in proprio tutte e tre le deviazioni dichiarate
dall'implementer e le ha confermate genuine: collisione dell'identificatore `it`
in i18n.spec.ts, TS1294 `erasableSyntaxOnly` sui parametri-proprieta di
ApiProblem, e auth.ts elencato fra i file da creare senza che alcuno step ne
definisca il contenuto. Ha anche verificato in proprio il budget di bundle
(stesso numero al byte) e rotto deliberatamente il test i18n togliendo
`home.logout` da en.json per controllare che diventasse rosso
Task 12: minor (deferred): la deviazione 2 e caratterizzata come "obbligata" nel
report dell'implementer, ma il reviewer ha verificato che
`erasableSyntaxOnly: false` in tsconfig.app.json avrebbe permesso il costruttore
verbatim del piano: era una scelta fra due alternative valide, non un vincolo.
Imprecisione nel report, non un difetto di codice
Task 12: minor (deferred): nessun test unitario per router.ts e stores/session.ts
(non richiesti dal piano)
Task 12: minor (deferred): index.html ha `lang="en"` hardcoded, sovrascritto a
runtime dall'i18n
Task 12: Ruling (finding Important plan-mandated): `signOut()` in HomeView.vue
non gestisce errori — niente try/catch, nessun feedback all'utente — a differenza
di LoginView e SetupView che li gestiscono. E verbatim dal piano, quindi la
regola SDD impone che sia io a decidere, non l'implementer ne il reviewer.
Decido di correggerlo. Se `POST /auth/logout` fallisce a livello di rete, oggi
`user.value` non viene azzerato, il redirect non avviene, e il pulsante "esci"
semplicemente non fa nulla senza dire perche: l'utente resta davanti a
un'interfaccia che lo mostra autenticato dopo che ha chiesto di uscire. Per
un'azione di sicurezza la direzione giusta del fallimento e l'opposta — azzerare
comunque lo stato locale e portare a /login, segnalando l'errore — perche il
danno di un logout apparente non riuscito e maggiore di quello di un logout
locale riuscito con revoca server-side incerta (e il backend, per sua natura,
risponde 204 anche quando la revoca fallisce: vedi il difetto differito su
logout). Il piano non aveva ragione di preferire il silenzio. Costo se
sbagliato: cinque righe in piu in una vista.
Fix cookie __Host-/Secure: implementato (commit b3b1b32) — Secure incondizionato
in session_cookie e clearing_cookie, parametro `secure` rimosso dalle firme,
should_be_secure/strip_port/host() eliminati, `headers` tolto dalle firme di
refresh e logout dove serviva solo a quello. I due test del Task 10 fix round 2
che contraffacevano Host sono stati affiancati (non sostituiti) da due gemelli
che girano sull'host di default dell'harness, piu un test di round-trip
end-to-end. 16 test in tests/auth.rs, suite intera verde, clippy e fmt puliti.
Riverificato dal controller.
Fix cookie: nota di onesta dell'implementer — il test di round-trip
`login_then_me_stays_authenticated_on_the_same_client` NON diventa rosso quando
si reintroduce l'omissione di Secure, e lui lo ha riportato spontaneamente
invece di lasciarlo scoprire. E coerente con l'analisi del brief: cookie_store
non implementa la validazione del prefisso __Host-, quindi un cookie senza
Secure viene riaccettato comunque su HTTP in chiaro. La guardia reale contro la
regressione e `assert_host_prefix_attributes`, che ora gira anche sull'host di
default e diventa rossa (4/16 test in rosso nella prova di vitalita). Il test di
round-trip resta come pin del flusso normale, con un doc comment che dichiara
esplicitamente cio che non prova — che e il modo giusto di tenere un test
debole senza che qualcuno lo scambi per una garanzia.
Fix cookie: il ruling R7 e chiuso — non esiste piu alcuna decisione di sicurezza
guidata da un header controllato dal client. Il difetto corrispondente va
rimosso dall'elenco dei "noti e differiti" nello STATO alla chiusura della fase.
Fix cookie __Host-/Secure: re-review scoped (1 addressed, 0 open — verdetto
ADDRESSED sul Critical; prova di vitalita rifatta dal re-reviewer in proprio
(4/16 rossi forzando set_secure(false), ripristino con Edit, 16/16 verdi);
confermato che il test di round-trip resta verde col bug reintrodotto e che il
suo doc comment lo dichiara onestamente; nessuna logica condizionale residua;
i #[utoipa::path] di refresh/logout non documentavano `headers`, quindi la
rimozione del parametro non tocca lo schema e lo snapshot OpenAPI resta verde;
nessuna rottura nuova; commits d047d44..b3b1b32)
Task 12: fix round 1/5 (1 addressed secondo l'implementer, re-review scoped in
corso — Important su signOut chiuso spostando la correzione in
`useSessionStore.logout()` invece che nella vista: azzera `user` in un `finally`
e non rilancia piu, esponendo un flag `logoutError` che LoginView consuma al
mount. HomeView non ha richiesto modifiche, perche il suo push('/login')
incondizionato era gia corretto una volta che logout() ha smesso di rigettare —
il fix e finito dove vive lo stato, non dove il difetto si manifestava, che e
meglio di quanto avessi prescritto io. 2 test nuovi su entrambi i rami, 8/8
vitest verdi, tsc e lint puliti, bundle 76.893 byte gzip. Riverificato dal
controller. Corretta anche la caratterizzazione imprecisa di ApiProblem nel
report. Commit c6b82f0)
Task 12: fix round 1/5 (1 addressed, 0 open — signOut: il fix e stato messo in
`useSessionStore.logout()` invece che nella vista, dove vive lo stato: azzera
`user` in un `finally`, non rilancia mai, espone un flag `logoutError` che
LoginView consuma al mount. HomeView non ha avuto bisogno di modifiche perche il
suo push('/login') incondizionato era gia corretto una volta che logout() ha
smesso di rigettare. 8/8 vitest, tsc/lint puliti, bundle 76.893 B gzip;
commit c6b82f0)
Task 12: Ruling (deviazione di protocollo, vincolo di risorse): la re-review
scoped del fix round e stata interrotta tre volte dal limite di sessione
dell'API, l'ultima volta lasciando sull'albero la mutazione deliberata con cui
stava verificando il RED. Ho ripristinato io l'albero (la versione corretta era
committata in c6b82f0) e ho eseguito io stesso la verifica di vitalita, invece
di dispatchare un quarto tentativo: mutazione riapplicata (logout senza
try/catch/finally) -> 1 test rosso su 8, esattamente quello del ramo di
fallimento; ripristino -> 8/8 verdi, albero pulito. Il test del ramo di successo
resta verde sotto mutazione, ed e corretto: la mutazione tocca solo il percorso
d'errore. La skill vieta al controller di *correggere* i finding, non di
verificarli; e il review finale dell'intero branch resta come rete. Costo se
sbagliato: una verifica fatta da chi ha gia visto il codice invece che da occhi
freschi, su un finding Important gia verificato funzionalmente (8/8 test, tipi,
lint, build).
Task 12: complete (commits ec799d1..c6b82f0, review clean dopo 1 fix round —
spec OK, qualita approvata)
Task 13: implementato (commit d6c74d6) — `common_layers` rinominata
`with_common_layers` e resa pubblica, con il contratto invertito rispetto al
brief: **richiede** che il router le arrivi con il proprio fallback gia
impostato. Quattro punti di montaggio (`router`, `router_without_state`,
`embed::mount`, `embed::mount_stateless`) rispettano l'ordine. Il difetto F5
della scansione pre-volo e stato evitato. 95 test verdi col frontend
precostruito, 4 test embed confermati non saltati, clippy e fmt puliti.
Riverificato dal controller (`cargo test -p keeppix-server --test embed`:
4/4, nessuno skip).
Task 13: nota: il brief dichiara `embed::spa_fallback() -> MethodRouter` nella
riga "Produces" ma il suo stesso codice non lo definisce mai e nulla nel
repository lo referenzia. Trattato come riga spuria, non implementato.
Task 13: **incidente operativo, causa nota e non una regressione**: durante il
task il filesystem temporaneo si e riempito completamente per l'accumulo di
1512 database `keeppix_test_*` mai eliminati dall'harness (comportamento
documentato nel ruling R9: "i database creati non vengono eliminati,
l'indirizzo deve puntare a un'istanza di scarto"), piu una `scratchpad/pgdata`
stantia. Ha prodotto fallimenti in `keeppix-db` non correlati al codice.
L'implementer ha diagnosticato e liberato 14 GB, poi rieseguito la suite pulita.
Il controller ha ripulito i 44 database residui a fine task.
Task 13: **finding operativo (Important) per la fix wave del review finale**:
l'harness deve eliminare il database usa-e-getta alla fine del test, invece di
lasciarlo. Il difetto era gia noto e documentato come accettabile, ma ora ha un
costo misurato: ha fatto fallire una suite e bruciato tempo di diagnosi in un
task non correlato. In CI il problema non si manifesta (container nuovo a ogni
run), quindi e un difetto di ergonomia locale, non di correttezza — ma la
diagnosi e tutt'altro che ovvia per chi la incontra.
