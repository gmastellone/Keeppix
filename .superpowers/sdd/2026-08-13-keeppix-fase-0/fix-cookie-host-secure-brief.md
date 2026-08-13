# Fix fuori piano — il cookie `__Host-kpx_session` non è mai valido su HTTP semplice

Non è un task del piano: è un difetto **Critical** scoperto dal Task 12
(verifica a mano con Chromium reale via Playwright) in codice del Task 9/10,
già rivisto e chiuso mesi fa in questo ledger. Blocca un criterio di
completamento esplicito della Fase 0 («ricarica pagina con sessione
persistente») e romperebbe allo stesso modo la verifica a mano del Task 13.
Va corretto ora, prima di procedere.

Il report completo del Task 12 (sezione «Prova a mano dello step 15») è in
`.superpowers/sdd/2026-08-13-keeppix-fase-0/task-12-report.md`, righe
260-360: leggilo per il contesto della scoperta, ma **questo brief contiene
già tutta l'analisi e la verifica empirica necessarie per il fix** — non
serve ripetere l'indagine.

## Il difetto

`crates/keeppix-api/src/cookie.rs` (`should_be_secure`) omette
deliberatamente l'attributo `Secure` quando l'header `Host` della richiesta
è `localhost`/`127.0.0.1`/`[::1]` (con o senza porta). L'intenzione dichiarata
nel commento era: «un client conforme scarterebbe un cookie `Secure`» quando
si parla in chiaro su loopback, quindi ometterlo permette ai test di leggerlo.

**Questa premessa è sbagliata**, e lo è in un modo specifico che vale la pena
capire per non reintrodurre l'errore:

Il prefisso `__Host-` richiede **letteralmente** l'attributo `Secure` nel
`Set-Cookie` per essere valido (RFC 6265bis §4.1.3.2), indipendentemente dal
fatto che il trasporto sia effettivamente TLS. L'eccezione di «origine
potenzialmente affidabile» che i browser applicano a loopback riguarda una
regola **diversa**: se un cookie *ha già* l'attributo `Secure`, un browser lo
accetta e lo rinvia anche su una connessione HTTP in chiaro verso
`127.0.0.1`/`localhost`/`::1` (perché quell'origine è considerata sicura a
prescindere dal trasporto). Non rende mai opzionale la *presenza* letterale
dell'attributo per un cookie con prefisso `__Host-`: se `Secure` manca, il
cookie fallisce la validazione del prefisso e viene scartato **per intero**,
loopback o no.

Codice attuale (`should_be_secure`) fa esattamente l'omissione sbagliata:
ometterla su loopback significa che **ogni** cookie di sessione emesso in
sviluppo locale, in `docker compose up` senza reverse proxy, o in qualunque
test manuale su `127.0.0.1`, viene scartato per intero da un browser reale.
Il sintomo osservato dal Task 12 con Chromium headless: `context.cookies()`
è `[]` subito dopo `POST /api/v1/setup`, e ogni richiesta successiva a
`/api/v1/auth/me` torna 401.

## Perché nessun test esistente l'ha mai preso

Vale la pena capirlo perché altrimenti sembra strano che sia sfuggito a
undici round di review. **Nessuna libreria HTTP generica (curl, reqwest,
qualunque client Rust) implementa la validazione del prefisso `__Host-`**:
quella regola è un'estensione specifica dei browser, non fa parte del cuore
di RFC 6265 che le librerie client implementano. `reqwest` (via il crate
`cookie_store`) applica solo le regole generiche sui cookie — incluso il
flag `Secure` in relazione allo schema della richiesta — ma non sa nulla del
prefisso `__Host-` e non lo controlla mai. Di conseguenza: un test che
verifica solo che il cookie "vada e torni" tramite il cookie-jar di `reqwest`
non può mai rilevare questo bug, qualunque cosa faccia il codice sotto test.

L'unico modo per accorgersene è ispezionare **il valore letterale
dell'header** `Set-Cookie` (che è già ciò che fa l'helper
`assert_host_prefix_attributes` in `tests/auth.rs`, per fortuna) oppure usare
un vero motore di rendering che implementi la regola — è la via che il
Task 12 ha percorso con Playwright/Chromium, ed è la ragione per cui quel
passo di verifica a mano non è ridondante rispetto alla suite automatica:
prova una proprietà che nessun test Rust può provare.

## Prova empirica che il fix funziona senza toccare l'harness di test

Ho verificato personalmente, con un piccolo server HTTP e un client
`reqwest` isolato (stessa versione pinnata nel workspace, `reqwest 0.13.4`
con `cookie_store(true)`), il comportamento reale della libreria:

- Server bindato su `127.0.0.1` (loopback), risponde con
  `Set-Cookie: probe=abc123; Secure; HttpOnly; SameSite=Lax; Path=/` su una
  richiesta HTTP in chiaro. Alla richiesta successiva sulla stessa
  connessione client: `cookie=Some("probe=abc123")` — **il cookie torna**.
- Stesso identico server bindato su un indirizzo non-loopback
  (`192.0.2.2`), stessa risposta `Secure` su HTTP in chiaro. Alla richiesta
  successiva: `cookie=None` — **il cookie viene scartato**, come ci si
  aspetta per un `Secure` senza TLS.

Cioè: **`cookie_store` 0.22.1 (quello che `reqwest` 0.13.4 usa) ha già
un'eccezione per il loopback identica a quella dei browser reali**, sul
piano della regola "`Secure` richiede trasporto sicuro". Il presupposto
scritto nel codice (`crates/keeppix-api/src/cookie.rs:12` e
`crates/keeppix-api/tests/auth.rs:355-356,406`, «un client conforme
scarterebbe un cookie Secure [su 127.0.0.1]») è quindi **falso** per la
libreria realmente in uso in questo workspace, non solo per i browser.

**Conseguenza pratica: impostare `Secure` sempre, senza eccezioni, non
dovrebbe rompere nessun test esistente.** Verificalo comunque per davvero —
non fidarti di questa premessa senza controllo — ma non serve inventare un
cookie-jar personalizzato per l'harness.

## Il fix

**Rendere `Secure` incondizionato.** `session_cookie` e `clearing_cookie` in
`crates/keeppix-api/src/cookie.rs` devono impostare `Secure` sempre, senza
alcuna logica derivata dall'header `Host`.

Nel dettaglio:

1. **`crates/keeppix-api/src/cookie.rs`**
   - `session_cookie` e `clearing_cookie` non prendono più un parametro
     `secure: bool`: `set_secure(true)` incondizionato in entrambe.
   - Rimuovi `should_be_secure` e `strip_port` interamente, e i loro test
     (`localhost_variants_are_not_secure`, `real_hosts_and_missing_host_are_secure`,
     `lookalike_hosts_are_not_treated_as_local`).
   - Riscrivi i doc comment delle due funzioni e del modulo: spiega la
     distinzione fra "il prefisso `__Host-` richiede l'attributo sempre" e
     "i browser (e, empiricamente, `cookie_store`/`reqwest`) esentano il
     loopback dal requisito di trasporto sicuro per *onorare* quell'attributo,
     non dal requisito di *impostarlo*". È la parte che, se persa, qualcuno
     reintrodurrà l'eccezione fra sei mesi pensando di semplificare i test.

2. **`crates/keeppix-api/src/routes/auth.rs`** (`login`, `refresh`, `logout`)
   e **`crates/keeppix-api/src/routes/setup.rs`** (`create`): rimuovi ogni
   `let secure = should_be_secure(host(&headers));` e il calcolo che lo
   precede; chiama `session_cookie(&token, state.session_ttl)` e
   `clearing_cookie()` senza l'argomento. Togli `should_be_secure` dagli
   `use`.

3. **`host()` in `routes/setup.rs`** (righe attorno alla 134) diventa morto:
   rimuovilo. Verifica se questo lascia `headers: HeaderMap` inutilizzato nei
   parametri di `refresh` e `logout` (in `auth.rs`) — lì `headers` serviva
   solo per `host(&headers)` — e se sì, **rimuovi il parametro dalla firma
   dell'handler**, non limitarti a un `#[allow(unused)]`. In `login` e
   `setup::create`, `headers` resta necessario per `user_agent(&headers)`:
   non toccare quella parte.

4. **Test in `crates/keeppix-api/tests/auth.rs`**: qui è dove serve più
   giudizio, non solo meccanica.
   - `assert_host_prefix_attributes` (l'helper che legge l'header
     `set-cookie` grezzo) resta **esattamente com'è**: è corretto e continua
     a essere il modo giusto di pinnare gli attributi letterali. Correggi
     solo il suo doc comment, che ripete la stessa premessa falsa
     sull'eccezione loopback di `reqwest` (righe 399-407) — di nuovo, non
     cancellare la spiegazione, correggila con quanto hai appena verificato.
   - `logout_clears_the_cookie_with_a_valid_host_prefix` e
     `login_issues_the_cookie_with_a_valid_host_prefix` contraffanno l'header
     `Host` con `PRODUCTION_HOST = "photos.example.com"` **perché era l'unico
     modo di rendere `Secure` osservabile**, dato che prima era assente su
     host loopback. Con il fix quella necessità sparisce: `Secure` è presente
     comunque. **Riscrivi entrambi i test perché girino contro il client di
     default della `TestServer` (nessun `Host` contraffatto)**, così
     dimostrano esattamente la proprietà che era rotta: che un client che
     parla con l'host reale dei test (loopback) riceve comunque un cookie
     valido secondo il prefisso `__Host-`. Questa è la parte più importante
     del fix round: un test che continua a contraffare `Host` per far
     apparire `Secure` non proverebbe che il bug è chiuso, proverebbe solo
     che il vecchio comportamento condizionale funzionava ancora nel ramo che
     lo attivava.
   - Puoi decidere se tenere `PRODUCTION_HOST` come test aggiuntivo (una
     guardia di regressione: "qualunque cosa dichiari `Host`, l'output è
     comunque `Secure`", utile contro un futuro tentativo di reintrodurre
     logica condizionale) oppure eliminarlo se lo trovi ridondante col nuovo
     test di default — motiva la scelta nel report.
   - **Aggiungi un test end-to-end che prova la proprietà originale rotta**:
     dopo un login o un setup riuscito (client di default, nessun `Host`
     contraffatto), una richiesta successiva a `/api/v1/auth/me` **sullo
     stesso client `reqwest` con cookie-jar automatico** — non con un cookie
     riattaccato a mano come fa `revoking_logs_out_only_that_session` — deve
     restituire 200. Questo è il test comportamentale (round-trip tramite il
     jar) che affianca quello letterale (`assert_host_prefix_attributes`):
     insieme pinnano sia "l'attributo c'è" sia "il client normale lo
     riutilizza davvero", che è la sequenza `setup → reload → resta
     autenticato` del criterio di completamento della fase. Verifica che
     TROVI il bug se lo reintroduci (vedi sotto): non deve essere un test che
     passerebbe comunque per altre ragioni.

## Prova di vitalità richiesta

Per almeno due delle asserzioni chiave — l'header letterale su un cookie di
sessione emesso su un host loopback di default, e il round-trip end-to-end
appena descritto — **rimetti temporaneamente l'omissione di `Secure` su
loopback** (puoi anche solo commentare `set_secure(true)` e rimettere la
vecchia logica per il tempo della prova, o più semplicemente forzare
`set_secure(false)` incondizionato), esegui i test, verifica che diventino
rossi con un messaggio comprensibile, ripristina il fix, verifica che
tornino verdi. Riporta l'output reale di entrambi i passaggi — è la prova
che i nuovi test avrebbero effettivamente bloccato la spedizione del bug
originale.

## Verifica

```bash
export KEEPPIX_TEST_DATABASE_URL="postgres://keeppix:keeppix@127.0.0.1:5432/postgres"
cargo test --workspace -- --test-threads=1
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Se Postgres non risponde: `pg_ctlcluster 16 main start`.

Non toccare `frontend/`, `docs/`, `Cargo.toml` a livello di workspace. Non
toccare `crates/keeppix-api/src/openapi.rs` né gli attributi `utoipa` sugli
handler che modifichi (a meno che la rimozione del parametro `headers` da
`refresh`/`logout` lo richieda per compilare — in tal caso limitati al
minimo necessario e dillo nel report).

## Commit

Sul branch `fase-0`. Messaggio in italiano, prefisso `fix(api):`, che spieghi
il *perché* (il prefisso `__Host-` richiede `Secure` sempre, non solo su
host non-locali) più in coda:

```
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01S1JPfGqKvQQG9FkLX3pnCi
```

Niente `git push`. Albero pulito alla fine (a parte il file di report).

## Report

Scrivilo in
`.superpowers/sdd/2026-08-13-keeppix-fase-0/fix-cookie-host-secure-report.md`.
Deve contenere: i file toccati e perché, la decisione su `PRODUCTION_HOST`,
l'output reale delle due prove di vitalità (rosso poi verde), l'esito dei tre
comandi di verifica, e qualunque cosa tu abbia notato ma deliberatamente non
corretto. Rispondimi solo con stato, hash del commit, una riga di riepilogo
dei test, e le preoccupazioni — il report ha il resto.

Se incontri un limite di sessione dell'API, fermati e dimmelo invece di
lasciare l'albero a metà.
