# Fase 0 — stato di avanzamento e consegna

**Aggiornato:** 2026-08-13, dopo la re-review del Task 10
**Piano:** [`2026-08-13-keeppix-fase-0.md`](2026-08-13-keeppix-fase-0.md)
**Spec:** [`../specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md)
**Roadmap:** [`2026-08-13-keeppix-roadmap.md`](2026-08-13-keeppix-roadmap.md)
**Branch:** `fase-0`, da `main @ 7b38c1d`

Questo documento esiste perché il ledger di lavoro vive in `.superpowers/`, che è
git-ignored e quindi non viaggia con il repository. Qui c'è tutto ciò che serve a
riprendere il lavoro da un'altra macchina o da un'altra sessione.

## Metodo di esecuzione

Un subagent implementatore per task, con brief estratto dal piano; a seguire una
review del task (conformità allo spec + qualità); i finding Critical/Important
entrano in un fix round, i Minor vengono differiti e annotati qui. Le review dei
task più delicati (7 e 10) sono state fatte su un modello più capace.

**Nessun finding si è rivelato un falso positivo.** Sette task su dieci hanno
richiesto almeno un giro di correzione.

## Avanzamento

| # | Task | Stato | Commit finale |
|---|---|---|---|
| 1 | Workspace e toolchain | ✅ review pulita | `fcd5368` |
| 2 | Tipi di dominio | ✅ dopo 1 fix round | `c0382a0` |
| 3 | Hashing Argon2id | ✅ review pulita | `db82b8c` |
| 4 | DB, migrazioni, harness | ✅ review pulita | `ada768e` |
| 5 | `UserRepo` | ✅ dopo 1 fix round | `78be6fc` |
| 6 | Token di sessione e segreti | ✅ dopo 1 fix round | `362af6e` |
| 7 | `SessionRepo` | ✅ dopo 1 fix round | `8835447` |
| 8 | Config, telemetria, CLI | ✅ dopo 1 fix round | `74890b4` |
| 9 | Stato, problem+json, extractor | ✅ dopo 1 fix round | `a040007` |
| 10 | Setup e autenticazione | ✅ dopo 2 fix round | `90f8b82` |
| 11 | Specifica OpenAPI | ⬜ da fare | — |
| 12 | Frontend | ⬜ da fare | — |
| 13 | Frontend incorporato | ⬜ da fare | — |
| 14 | Immagine Docker e compose | ⬜ da fare | — |
| 15 | Integrazione continua | ⬜ da fare | — |

Stato attuale della suite: **85 esecuzioni di test** tutte verdi (22 domain,
39 db, 20 api, 4 server; i 3 unit test dell'harness di `keeppix-db` girano una
volta per binario di integrazione, cioè quattro volte);
`cargo clippy --workspace --all-targets -- -D warnings` pulito;
`cargo fmt --check` pulito; `cargo build --workspace` verde.

## Come si esegue la suite

I test di integrazione vogliono un Postgres reale. Di norma se lo avviano da
soli con testcontainers e non serve fare nulla:

```bash
cargo test --workspace
```

Dove il registry delle immagini non è raggiungibile — è il caso della sessione
cloud in cui sono stati fatti gli ultimi commit, vedi **R9** — si punta la suite
a un Postgres già in ascolto:

```bash
export KEEPPIX_TEST_DATABASE_URL="postgres://utente:password@127.0.0.1:5432/postgres"
cargo test --workspace -- --test-threads=1
```

`--test-threads=1` serve ai quattro test di `keeppix-server/tests/config.rs`,
che manipolano l'ambiente di processo e non tollerano il parallelismo: è un
vincolo pre-esistente, documentato nel file stesso, non una regressione.

## Ripresa del lavoro

Il prossimo passo è il **Task 11 — Specifica OpenAPI**. Il piano contiene i suoi
step verbatim. Il metodo suggerito resta uno subagent per task con review
successiva, ma il piano è eseguibile anche a mano.

Prima di iniziare, leggere i ruling qui sotto: due di essi (R2 sulla toolchain e
R4 su sqlx) cambiano istruzioni scritte nel piano, e ignorarli farebbe fallire i
Task 14 e 15.

Nel Task 11 c'è una trappola che il piano non nomina: la rotta
`/api/openapi.json` va aggiunta **dentro** l'argomento di `common_layers`, non
dopo la chiamata, altrimenti esce senza header di sicurezza. È la stessa classe
di bug del ruling R5, in un punto nuovo.

## Decisioni prese durante l'esecuzione

Ognuna è una scelta fatta senza poter consultare l'autore del piano. Sono
elencate perché siano riesaminabili.

**R1 — Branch invece di worktree.**
Il repository non aveva un remote quando l'esecuzione è iniziata, quindi la
creazione di un worktree con base `origin/<default>` sarebbe fallita. Il vincolo
reale era "non implementare su `main`", che un branch soddisfa.
*Se sbagliato:* si sposta il branch in un worktree con un comando.

**R2 — Toolchain 1.88.0 invece di 1.85.0.**
Il piano fissava 1.85, ma il codice che il piano stesso specifica per i Task 5 e
8 usa i let-chain (`if let ... && ...`), stabili solo da Rust 1.88: con 1.85 quei
due task non compilano. Alzata la toolchain in `rust-toolchain.toml` e in
`rust-version`. **Il Task 14 (Dockerfile) e il Task 15 (CI) devono usare 1.88,
non 1.85 come scritto nel piano.**
*Se sbagliato:* nessun costo noto.

**R3 — `DbError::Corrupted` al posto di `Migration` per dati malformati.**
Il fix round del Task 5 ha introdotto una variante dedicata ai dati già presenti
in database che il codice non riesce a interpretare, distinta da "una migrazione
è fallita". Il Task 6 la prescriveva ancora come `Migration`; ha vinto la
coerenza della tassonomia su cui si fa triage.
*Se sbagliato:* nessuno, entrambe cadono nel ramo generico del `From<DbError>`.

**R4 — sqlx con le forme funzione, non le macro.**
Lo spec §9.5 dichiara "query verificate a compile-time", ma tutto il codice del
piano usa `sqlx::query(...)` (verificata a runtime), non `sqlx::query!`. Gli step
11-12 del Task 4 (`cargo sqlx prepare`, cache `.sqlx/`) avrebbero prodotto una
cache vuota. Si tengono le forme funzione con parametri bound: la proprietà di
sicurezza che conta — query parametrizzate, mai concatenazione — è pienamente
soddisfatta, e la verifica dello schema è coperta dai test di integrazione contro
Postgres reale. **Il Task 14 (Dockerfile) e il Task 15 (CI) devono rimuovere ogni
riferimento a `SQLX_OFFLINE` e `.sqlx/`.**
*Se sbagliato:* un refuso in un nome di colonna fallisce in test invece che in
build.

**R5 — Ordine di `.fallback()` in `common_layers`.**
In axum 0.8 `Router::fallback` sovrascrive il catch-all invece di fondersi con
quello già avvolto, e `.layer()` avvolge solo il fallback esistente al momento
della chiamata. Il piano metteva `.fallback(not_found)` **dopo** i `.layer(...)`,
con il risultato che ogni 404 usciva senza CSP, nosniff, referrer-policy e
permissions-policy. Spostato prima, con un commento che spiega il meccanismo.
**Il Task 13 ristruttura `common_layers`: deve mantenere quest'ordine.**
*Se sbagliato:* nessuno, la correzione è verificata da test su entrambe le rotte.

**R6 — `dummy_hash()` con un hash Argon2id reale.**
Il piano usava una stringa PHC malformata per pareggiare i tempi di risposta
quando l'utente non esiste. Non funzionava: `verify_password` fallisce il
*parsing* e ritorna subito, senza eseguire Argon2, lasciando intatto il segnale
temporale che doveva mascherare. Sostituita con un hash valido, e il test che lo
inchioda asserisce il successo del parsing in positivo.
*Se sbagliato:* l'esistenza di un utente resta deducibile dai tempi di risposta.

**R7 — `should_be_secure` corretto in loco, non spostato in configurazione.**
La funzione faceva prefix-match sull'header `Host`, controllato dal client, per
cui `localhost.evil.com` passava per locale. Corretta con match esatto dopo lo
strip della porta, inclusi i letterali IPv6. La soluzione architetturalmente
migliore — spostare la decisione in `AppState`/`Config` — richiedeva di infilare
un campo lungo `Config → AppState → entrambi i router → funzioni cookie`, cioè
una modifica di interfaccia sproporzionata rispetto a un rischio che il prefisso
`__Host-` già smorza (un cookie non-`Secure` con quel prefisso viene rifiutato
dal browser, quindi il login fallisce invece di far trapelare il cookie).
*Se sbagliato:* resta un header client-controlled che decide un attributo di
sicurezza. **Da riesaminare nel review finale del branch.**

**R8 — Il fix del Task 7 committato dal controller.**
L'implementatore ha completato il fix ma è morto per limite di sessione prima di
verificare e committare. Il controller ha eseguito la verifica (11/11 test,
clippy pulito) e committato il codice esattamente come lasciato. Stessa cosa per
il fix round del Task 10, interrotto a lavoro concluso ma prima del commit.
*Se sbagliato:* il codice era comunque coperto dalla re-review successiva.

**R9 — Via d'uscita negli harness di test verso un Postgres esistente.**
Nell'ambiente cloud il pull di `postgis/postgis` è bloccato dalla policy di
egress (403 al CONNECT verso `production.cloudfront.docker.com`), quindi
testcontainers non è utilizzabile e l'intera suite di integrazione non è
eseguibile — il primo reviewer ha dovuto patchare gli harness a mano e
ripristinarli. Se `KEEPPIX_TEST_DATABASE_URL` è impostata, i due harness usano
il server già in ascolto a quell'indirizzo e creano un database vergine per
test, con lo stesso isolamento del container; senza la variabile nulla cambia e
il container resta la via predefinita per CI e sviluppo.
*Se sbagliato:* gli harness hanno un ramo in più e la riscrittura dell'URL è
duplicata nei due crate, con unit test solo nella copia di `keeppix-db`.

**R10 — Push su `fase-0`.**
L'harness della sessione cloud impone di default un branch `claude/...`;
l'utente ha chiesto esplicitamente che il lavoro finisca sul branch della fase.
Il branch `claude/keeppix-fase-0-4c0lku` è stato cancellato dopo aver verificato
che i suoi commit fossero tutti in `fase-0`.
*Se sbagliato:* nessun costo, i due branch avevano lo stesso contenuto.

## Difetti noti, accettati e differiti

Nessuno blocca la fase. Vanno triati nel review finale del branch, prima del
merge.

### Sicurezza e operatività

- **`Auth` mappa qualsiasi errore a 401.** `Auth::from_request_parts` tratta
  anche `DbError::Connection` — database irraggiungibile — come
  `401 keeppix/unauthenticated`. Un blip del database apparirebbe come "sessione
  scaduta" a tutti i client e il frontend li rimanderebbe al login in massa,
  invece di un 5xx. *Difetto del piano.* Correzione suggerita: `Connection` →
  503, il resto → 401.
- **`refresh` non ricontrolla l'utente.** `rotate` tocca solo `sessions`, quindi
  un account disabilitato può coniare token all'infinito. Innocuo oggi perché
  `authenticate` fa join su `users.disabled_at IS NULL`, ma la famiglia di
  sessioni non muore mai.
- **Nessun rate limiting** su `/auth/login` e `/api/v1/setup`; i ~100 ms di
  Argon2 danno solo un throttling incidentale.
- **Le risposte 405** (per esempio `GET /api/v1/auth/login`) restituiscono il
  corpo vuoto di default di axum, non `application/problem+json`. Gli header di
  sicurezza si applicano comunque.
- **Due replay concorrenti sulla stessa famiglia** possono deadlockare: Postgres
  ne aborta uno con 40P01, che diventa `DbError::Connection` invece di
  `Forbidden`. L'esito di sicurezza regge, degrada solo il codice d'errore.
- **`rotate` non controlla `users.disabled_at`** — vedi sopra, stessa causa.
- **`logout` risponde 204 anche quando `revoke` fallisce:** l'errore viene solo
  loggato a `warn`. Con un blip del database l'utente crede di essere uscito
  mentre la sessione resta viva lato server fino alla scadenza. Coerente con il
  commento "Sempre 204", ma il fallimento è invisibile al client.
- **CSRF, difesa parziale rispetto allo spec §9.5:** lo spec chiede
  `SameSite=Lax` **più** obbligo di `Content-Type: application/json` e di un
  header custom sulle mutazioni. `/auth/refresh` e `/auth/logout` non hanno né
  corpo né header custom richiesto. `SameSite=Lax` da solo già impedisce l'invio
  del cookie su POST cross-site, ma il contratto dello spec non è completo.
- **`should_be_secure` è case-sensitive** e non riconosce `0:0:0:0:0:0:0:1` né
  `[127.0.0.1]`. Tutti questi casi cadono sul lato sicuro (`Secure = true`):
  rompono al massimo uno sviluppo locale esotico.

### Qualità e manutenzione

- **`Password::parse` limita a 1024 caratteri, non byte:** una password di 1024
  emoji arriva ad Argon2 come ~4096 byte. I limiti di dimensione HTTP dei task
  futuri vanno pensati in byte.
- **`Password` non azzera il buffer in `Drop`** e deriva `Clone`.
- **`sha2 = "0.11"`** in `keeppix-domain` duplica l'albero già presente via
  sqlx/argon2 (0.10.9). Pinnare a 0.10 deduplicherebbe.
- **`reqwest` di test usa la feature `rustls`**, che tira dentro
  aws-lc-rs/quinn/jni (~440 righe di `Cargo.lock` più un requisito cmake) per
  test che parlano HTTP in chiaro a 127.0.0.1.
- **`interval()` tronca i TTL sub-secondo:** `Duration::from_millis(500)` diventa
  `"0 seconds"`, cioè un token nato scaduto, senza errore.
- **Ruolo sconosciuto in `sessions.rs`** degrada a `SystemRole::User` invece di
  `DbError::Corrupted` come in `users.rs`. Irraggiungibile grazie al CHECK e
  fallisce chiuso, ma le due tassonomie divergono.
- **`map_unique_violation` scarta l'errore sqlx sottostante:** collisioni su
  username ed email producono lo stesso messaggio.
- **`clear_env()` nei test di config** azzera solo 4 delle 7 chiavi `KEEPPIX_*`
  possibili. Nessuna perdita oggi, trappola per chi scriverà nuovi test.
- **Il messaggio d'errore di `DATABASE_URL`** mescola inglese e italiano.
- **`rotate_rejects_an_expired_token`** non distingue strutturalmente quale
  orologio venga usato: app e database condividono l'host, quindi il test
  sarebbe passato anche prima della correzione. Verifica il comportamento, non
  la causa.
- **`create` non popola la colonna `ip`** della tabella `sessions`.
- **I sette `lib.rs` iniziali** contengono un `\n` invece di essere a 0 byte.
- **`uuid.workspace = true`** resta ridondante in `[dev-dependencies]` di
  `keeppix-db`.
- **`dummy_hash_is_a_valid_argon2id_phc_string` è sovra-pinnato:** mutare il
  solo salt lo fa fallire pur lasciando intatta la proprietà di sicurezza. È il
  prezzo dell'asserzione positiva; ruotare la costante impone di rigenerarla dal
  plaintext dichiarato nel test.
- **`setup_creates_the_first_admin_and_logs_in`** verifica gli attributi del
  cookie con `contains` sull'header intero e senza contraffare `Host`: non può
  vedere `Secure` ed è esposto in linea di principio al falso positivo del token
  casuale. La proprietà è coperta dai due test aggiunti nel secondo fix round.
- **La logica di `with_database` negli harness è duplicata** fra `keeppix-db` e
  `keeppix-api`, con unit test solo nella prima copia: i due crate non
  condividono codice di test e non esiste un crate di test-support.

## Nota sul metodo, per chi riprende

I finding più utili di questa fase non sono venuti dalla lettura del codice ma
da reviewer che hanno **eseguito** qualcosa: chi ha costruito un test usa-e-getta
per osservare gli header mancanti sul 404, chi è andato a leggere il sorgente di
`rand` nel registry per confermare che il generatore di token fosse davvero un
CSPRNG, chi ha provato a corrompere una costante per vedere se il test che
doveva proteggerla se ne accorgeva. Tre dei test scritti seguendo il piano
passavano senza provare ciò che il loro nome affermava.
