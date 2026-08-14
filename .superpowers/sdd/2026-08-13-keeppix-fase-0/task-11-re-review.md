# Task 11 — fix round 1/5 — re-review

Diff esaminato: `9d88cb4..adca7c6` (`review-9d88cb4..adca7c6.diff`), un solo
commit `adca7c6 fix(api): tie the openapi document to the routes it claims to
describe`. Albero verificato identico a `adca7c6` per `crates/` e `docs/api/`
al termine di questa re-review (`git diff adca7c6..HEAD -- crates/ docs/api/`
vuoto, `git status` pulito).

## Verdetti sui finding

**I1 — Documento e rotte montate possono divergere in silenzio: ADDRESSED.**
Nuovo test `documented_operations_are_all_mounted`
(`crates/keeppix-api/tests/openapi.rs:113-159`) avvia `TestServer` (router
**con stato**, non `router_without_state()`), scarica `/api/openapi.json` dal
server reale, e per ogni coppia (path, method) del documento — filtrata sugli
otto metodi HTTP validi — esegue la richiesta corrispondente asserendo status
≠ 404 e ≠ 405. `assert_eq!(checked, 6, …)` alla fine chiude la vacuità: un
documento vuoto o un `paths` non-oggetto farebbe fallire quell'asserzione
invece di passare a ciclo mai eseguito. Il commento sopra il test dichiara
esplicitamente la direzione non coperta (rotta montata e non documentata),
motivandola con l'assenza di introspezione del router in axum 0.8, esattamente
come richiesto dal ruling. Il messaggio di fallimento dello snapshot
(`tests/openapi.rs:264-270`) non contiene più il comando di disattivazione:
ora spiega che il contratto è congelato, i client che ne dipendono, e chiude
con «Non rigenerarlo per far tornare verde il test: guarda che cosa è cambiato
e decidi».

Verifica indipendente (non fidandomi del racconto): ho rifatto io stesso le
quattro mutazioni dell'implementer, una alla volta, con `git diff` pulito
prima e dopo ciascuna.

- **M1 — `post`→`put` nel solo `#[utoipa::path]` di `login`** (route axum
  invariata): `documented_operations_are_all_mounted` FAILED con
  `assertion `left != right` failed: il documento dichiara put
  /api/v1/auth/login, ma la rotta non accetta quel metodo` — `left: 405,
  right: 405`. Fallimento reale su uno status code reale, non un ciclo vuoto.
- **M2 — `path = "/api/v1/auth/me"` → `"/api/v1/auth/whoami"`**:
  `documented_operations_are_all_mounted` FAILED, `left: 404, right: 404`,
  messaggio «quel percorso non è montato».
- **M3 — `security(("session" = []))` su `me`** (addon invariato):
  `security_requirements_name_a_declared_scheme` FAILED: «richiede lo schema
  session, che non è dichiarato in components».
- **M4 — `operation_id` di `setup::status` cambiato in `"setup_create"`**:
  `operation_ids_are_explicit_and_unique` FAILED: «operationId duplicato:
  [..., "setup_create"]», `left: 5, right: 6`.

Tutte e quattro riproducono esattamente l'output riportato dall'implementer.
Dopo ciascuna mutazione ho ripristinato il file con `Edit` (non `git
checkout`, per non ripetere l'incidente descritto sotto) e ho confermato alla
fine `git diff adca7c6..HEAD -- crates/ docs/api/` vuoto e la suite
`cargo test -p keeppix-api --test openapi` di nuovo 6/6 verde.

**I2 — `responses` allineate a ciò che gli handler possono restituire:
ADDRESSED.** Verificato leggendo il codice sorgente, non solo il diff:
- `auth::me` (`routes/auth.rs:217-226`) chiama
  `UserRepo::find_by_id(&ctx, id)?`; `find_by_id`
  (`crates/keeppix-db/src/users.rs:160-172`) fa
  `if !ctx.is_admin() && ctx.user_id() != Some(id) { return
  Err(DbError::Forbidden) }` seguito da `.ok_or(DbError::NotFound)?`. Poiché
  `id` in `me` è per costruzione `ctx.user_id().unwrap()`, la condizione del
  403 è sempre falsa: il 403 dichiarato-non-dichiarato è correttamente
  giustificato come irraggiungibile, e il documento ora porta 200/401/404/500
  — coerenti con `Problem::from(DbError)`.
- `setup::status`, `setup::create`, `auth::login` propagano `?` su chiamate
  `keeppix-db`/hashing che possono produrre `Problem::internal()` o
  `Problem::from(DbError)` non-Conflict → 500, ora dichiarato in tutti e tre.
- `auth::refresh` collassa ogni errore di `rotate` (DB compreso) su 401 via
  `.map_err(|_| Problem::unauthenticated())?`: nessun 500 raggiungibile, e il
  codice ora lo dice con un commento — corretto non dichiararlo.
- `auth::logout` non restituisce `Result` (l'errore di revoca è loggato, non
  propagato): 204 resta l'unico esito, nessuna correzione necessaria.

**I3 — `operationId` generici e collidibili: ADDRESSED.** Tutti e sei gli
handler hanno ora `operation_id` esplicito e namespaced
(`auth_login`, `auth_logout`, `auth_me`, `auth_refresh`, `setup_create`,
`setup_status` — `routes/auth.rs`, `routes/setup.rs`). Nuovo test
`operation_ids_are_explicit_and_unique` (`tests/openapi.rs:212-241`) verifica
sia l'unicità sia l'elenco atteso, quindi una futura collisione (es.
`albums::create`) non passerebbe inosservata. Rigenerazione confermata nel
documento committato.

**I4 — `Problem` non è nei components: ADDRESSED.** `Problem` ora deriva
`utoipa::ToSchema` (`crates/keeppix-api/src/problem.rs:13`); tutte e otto le
risposte d'errore (401×3, 404, 409, 422, 500×4) portano `body = Problem`. Lo
schema generato (`docs/api/openapi.json`, blocco `Problem`) omette
correttamente il campo privato `status_code` (rispettato via `#[serde(skip)]`,
nessun `#[schema(ignore)]` necessario) ed espone `type` via
`#[serde(rename)]`. Confermato che `content: application/json` compare ora su
ogni risposta d'errore nel diff.

**I5 — Nessun `securitySchemes`: ADDRESSED.** `SecurityAddon`
(`openapi.rs:43-56`) registra lo schema `session_cookie` come
`apiKey`/`in: cookie` con nome preso da `crate::extract::SESSION_COOKIE`
(verificato: la costante vale `"__Host-kpx_session"` in
`crates/keeppix-api/src/extract.rs:10`, non `__Host-keeppix_session` come
ipotizzato dalla review — prendere la costante invece di riscriverla ha
evitato l'errore). `security(("session_cookie" = []))` applicato a `me` e
`refresh`; `logout` resta pubblica con un commento che ne spiega il motivo
(funziona anche senza cookie). Nuovo test
`security_requirements_name_a_declared_scheme` verifica che ogni requisito
punti a uno schema dichiarato e che le rotte protette siano esattamente
`{me, refresh}` — confermato vivo dalla mutazione M3 sopra.

## Nuova rottura nel diff di fix

Nessuna. Il diff è additivo su tutti i file toccati (annotazioni, un nuovo
`Modify`, tre nuovi test, snapshot rigenerato in modo coerente); non tocca
`lib.rs`, `common_layers`, la logica degli handler oltre alla dichiarazione
dei tipi di errore, né alcun file fuori da `crates/keeppix-api` e
`docs/api/openapi.json`. `cargo clippy -p keeppix-api --all-targets -- -D
warnings` e `cargo fmt --check` restano puliti sull'albero ripristinato.

## Sull'incidente riportato dall'implementer

Il report dichiara che un helper di shell ha eseguito `git checkout --
crates/keeppix-api/src` durante le prove di mutazione, cancellando le
modifiche del round (i test e lo snapshot, fuori da `src/`, sono
sopravvissuti), e che i sorgenti sono stati riscritti. Ho verificato la
conseguenza sul codice consegnato, non il racconto dell'incidente in sé:
`git diff 9d88cb4..adca7c6` (il diff sotto revisione) è un diff pulito e
coerente file per file — nessun residuo, nessuna riga incoerente con lo
snapshot committato — e la mia riesecuzione delle quattro mutazioni (sopra)
conferma che i sorgenti attuali producono esattamente il documento atteso
byte per byte (`openapi_snapshot_matches_the_committed_file` passa
sull'albero pulito). L'incidente non ha lasciato tracce osservabili nel
risultato consegnato.

## Osservazioni fuori perimetro

Confermo, senza che blocchino questo round, i differiti già dichiarati e non
riaperti dall'implementer: i rustdoc `# Errors` che finiscono nel `summary`
pubblico (ora anche ridondanti rispetto alle `responses` complete); nessun
confronto diretto fra documento *servito* e documento *committato* (m2 della
review — parzialmente attenuato perché `documented_operations_are_all_mounted`
ora legge il documento servito dal server reale); `role` senza `enum`, `id`
senza `format: uuid`; `info.version` legato alla versione del crate; il
messaggio di fallimento dello snapshot non è un vero diff riga-per-riga; il
file `docs/api/openapi.json` senza newline finale. Nessuno di questi è nel
perimetro dei cinque Important verdettati.

## Esito dei comandi rieseguiti

| Comando | Esito |
| --- | --- |
| `KEEPPIX_TEST_DATABASE_URL=… cargo test -p keeppix-api --test openapi -- --test-threads=1` (baseline, albero pulito) | 6/6 ok |
| Mutazione M1 (post→put su `login`) | `documented_operations_are_all_mounted` FAILED, 405/405, messaggio atteso |
| Mutazione M2 (`me`→`whoami`) | `documented_operations_are_all_mounted` FAILED, 404/404, messaggio atteso |
| Mutazione M3 (schema `"session"` invece di `"session_cookie"`) | `security_requirements_name_a_declared_scheme` FAILED, messaggio atteso |
| Mutazione M4 (`setup_status`→`setup_create`) | `operation_ids_are_explicit_and_unique` FAILED, 5/6, messaggio atteso |
| `git diff adca7c6..HEAD -- crates/ docs/api/` dopo ripristino finale | vuoto |
| `git status` | pulito |
| `cargo test -p keeppix-api --test openapi -- --test-threads=1` (dopo ripristino) | 6/6 ok |
| `cargo clippy -p keeppix-api --all-targets -- -D warnings` | pulito |
| `cargo fmt --check` | pulito |

Non ho ri-eseguito `cargo test --workspace`: il diff di fix non tocca alcun
file fuori da `crates/keeppix-api` e `docs/api/openapi.json`, e il report
dell'implementer mostra già l'esito di due esecuzioni complete post-commit
(23 gruppi ok, 0 falliti). Le uniche verifiche mirate necessarie erano sui
test nuovi/modificati di `keeppix-api`, eseguite sopra.

## Verdetto

**Fix round: tutti e cinque i finding indirizzati (I1-I5 ADDRESSED), nessuna
nuova rottura Critical/Important nel diff di fix.** Le quattro mutazioni
dell'implementer sono state rieseguite indipendentemente e producono lo
stesso esito riportato — nessuna è vacua. L'incidente `git checkout` non ha
lasciato conseguenze osservabili sul codice consegnato. Nessun blocco.
