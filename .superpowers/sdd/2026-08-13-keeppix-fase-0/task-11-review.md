# Task 11 — Specifica OpenAPI — review

Diff esaminato: `4b5e354..9d88cb4` su `crates/` e `docs/api/`.
Albero al momento della review: `327b44f` (commit successivo, tocca solo
`.superpowers/`; `git diff 9d88cb4..HEAD -- crates/ docs/api/` è vuoto).

## Verdetti

- **Conformità allo spec: ✅ (con una riserva sostanziale, vedi I1).**
- **Qualità: da correggere** — nessun Critical, ma cinque Important, due dei
  quali sono difetti fattuali del contratto già pubblicato e costano poche
  righe adesso, mentre il documento ha sei operazioni.

### Motivazione della conformità

Lo spec §9.1 chiede quattro cose, tutte presenti:

> Tutto sotto `/api/v1`, **contratto congelato**: solo aggiunte.

Le sei operazioni sono tutte sotto `/api/v1`; il documento porta
`"description": "API di Keeppix. Contratto congelato: solo aggiunte entro /api/v1."`.

> **OpenAPI 3.1 generato dal codice** con `utoipa`: gli handler *sono* la
> specifica. Da `/api/openapi.json` si generano i client Kotlin, Swift, Dart e
> TypeScript. Un test in CI fallisce sui cambiamenti incompatibili.

`"openapi": "3.1.0"`, utoipa 5.5.0, rotta `GET /api/openapi.json` servita e
coperta da test, snapshot su disco che fallisce sulle modifiche. Decisione D7
(«generazione automatica del client mobile») è servita.

La riserva riguarda la tabella dei rischi (riga 844 dello spec):

> | Deriva del contratto API | rottura del client mobile | OpenAPI generato,
> test di compatibilità in CI, versionamento esplicito |

La mitigazione dichiarata contro la deriva è «OpenAPI generato». Ho verificato
che questa mitigazione **non copre la deriva fra documento e rotte montate**
(I1). È una conformità sulla lettera, non sull'effetto atteso. Non la conto
come ❌ perché lo spec non prescrive il meccanismo di verifica e perché il
piano del Task 11 non chiedeva nulla di più; la conto come riserva esplicita
da chiudere nel Task 15 o nella fix wave finale.

Sui due Minor che il dispatch mi chiedeva di ripesare (`Problem` fuori dai
components, `securitySchemes` assente): **lo spec non li rende requisiti
letterali** — §9.5 non parla di OpenAPI e §9.2 parla del formato d'errore sul
filo, non della sua descrizione nel documento. Ma entrambi rendono
inutilizzabile ciò per cui §9.1 dice che il documento esiste («da
`/api/openapi.json` si generano i client Kotlin, Swift, Dart e TypeScript»)
proprio sul punto che §9.2 chiama fondante:

> **Errori come dati**: RFC 9457 `application/problem+json` con `type` stabili
> (`keeppix/quota-exceeded`). Il client decide sul codice, mai sul testo.

Un client generato oggi non ha alcun tipo su cui deserializzare il `type` su
cui «decide». **Li promuovo entrambi da Minor a Important** (I4, I5), non a
requisito di conformità.

---

## Finding

### Critical

Nessuno.

### Important

#### I1 — Il documento e le rotte realmente montate possono divergere, in silenzio

Questa è la domanda centrale del task. La risposta è: **sì, possono**, in due
direzioni su tre.

**Prova A — rotta montata e non documentata.** Aggiunta a `api_routes()` in
`crates/keeppix-api/src/lib.rs` una rotta senza alcuna annotazione:

```rust
.route("/auth/me", get(routes::auth::me))
.route("/auth/devices", get(routes::auth::me))
```

Suite completa (`KEEPPIX_TEST_DATABASE_URL=… cargo test --workspace -- --test-threads=1`):
**23 gruppi `test result: ok`, 0 falliti**, esattamente come sull'albero pulito.
`GET /api/v1/auth/devices` risponde e non compare nel documento; nessun test se
ne accorge.

**Prova B — operazione documentata con il metodo sbagliato.** In
`routes/auth.rs`, `post` → `put` dentro il solo `#[utoipa::path]` di `login`
(rotta in `lib.rs` invariata, `axum::routing::post`):

```
test openapi_snapshot_matches_the_committed_file ... FAILED
assertion `left == right` failed: la specifica è cambiata: rigenerare con `rm docs/api/openapi.json && cargo test`
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
```

Lo snapshot vede la modifica — ma il messaggio di fallimento **istruisce a
disattivarla**. Eseguendo ciò che dice (`rm docs/api/openapi.json && cargo test`):

```
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
metodi dichiarati per /api/v1/auth/login: ['put']
```

e la suite completa torna a **23 gruppi ok, 0 falliti**, con un contratto
pubblico che dichiara `PUT /api/v1/auth/login` su una rotta montata `POST`. Il
file committato è stato ripristinato con `git checkout`.

**Prova C (direzione che invece è coperta)** — cambiando solo
`path = "/api/v1/auth/me"` in `"/api/v1/auth/whoami"`, `openapi_document_is_served_and_complete`
e lo snapshot falliscono. Ma sono coperti perché il test contiene una **terza
copia scritta a mano** della lista dei percorsi, non perché consulti il router.
Il punto si vede meglio così: il file di test usa `router_without_state()`, e
`base_router_stateless()` monta soltanto `/health` e `/api/openapi.json` — le
rotte `/api/v1/*` **non esistono nemmeno** nel router che il test interroga.

Rimedio proponibile e a basso costo: un test che, per ogni coppia
(path, method) letta dal documento, esegua una richiesta contro il router reale
e asserisca che lo status non sia 404 né 405. Copre A→B ma non B→A (axum 0.8
non espone la tabella delle rotte); richiede il router con stato, quindi vive
in un file di test con harness DB o dietro una variante stateless completa.

#### I2 — Divergenza già presente nel documento committato: `me` può restituire 404

`crates/keeppix-api/src/routes/auth.rs` dichiara `responses(200, 401)` per `me`,
ma `UserRepo::find_by_id` (`crates/keeppix-db/src/users.rs:172`) fa
`.ok_or(DbError::NotFound)?`, che `Problem::from(DbError)` mappa su
`Problem::not_found()` → **404**. Il rustdoc dell'handler lo dice testualmente,
e — per il difetto già noto n. 1 — quel rustdoc **è** il `summary` pubblicato:

```json
"summary": "# Errors\n`401` se non autenticato, `404` se l'utente è stato nel frattempo rimosso.",
"responses": { "200": {…}, "401": {…} }
```

Il documento contiene, nella propria prosa, uno status che il proprio oggetto
`responses` omette. Stessa classe: nessuna delle sei operazioni dichiara 500,
benché `setup::create` chiami `Problem::internal()` e ogni handler che tocca il
DB possa produrre `Problem::from(DbError) → 500`; e `setup/status` ha come
summary «`Problem` se il conteggio degli utenti fallisce» dichiarando solo 200.

#### I3 — `operationId` generici, collidibili, e già congelati

Gli `operationId` sono derivati dal nome della funzione: `login`, `logout`,
`me`, `refresh`, **`create`**, **`status`**. `operationId` deve essere unico in
tutto il documento e diventa il nome del metodo nel client generato — cioè fa
parte del contratto che lo spec dichiara congelato.

Prova: aggiunto un handler `pub async fn create()` annotato su `/api/v1/albums`
(la tabella §9.1 dello spec prevede «Album | CRUD») e inserito in `paths(...)`.
Documento rigenerato:

```
('/api/v1/albums', 'post', 'create')
('/api/v1/setup', 'post', 'create')
```

Due operazioni con lo stesso `operationId`; test tutti verdi. `openapi-generator`
su un input così o rifiuta il documento o rinomina in `create_0`, e il nome
resta instabile. Costo del rimedio oggi: sei `operation_id = "..."` espliciti
(es. `setup_create`, `setup_status`, `auth_me`) e uno snapshot rigenerato.
Costo domani: rinominare metodi in client Kotlin/Swift/Dart/TS già distribuiti.

#### I4 — `Problem` non è nei components (promozione del Minor 2 dichiarato)

Confermo il difetto, **contesto la classificazione**. Le risposte 401/409/422
non hanno alcun `content`:

```json
"401": { "description": "Credenziali non valide" }
```

mentre il server risponde `application/problem+json` con `{type, title, status,
detail?}` (`crates/keeppix-api/src/problem.rs`). Il tipo esiste, è già
`Serialize`, e serve un solo `derive(ToSchema)` più `body = Problem` su sette
risposte. Per §9.2 («il client decide sul codice») è la parte del contratto su
cui il client mobile ramifica: lasciarla non descritta ora, quando il documento
ha sei operazioni, la rende un lavoro proporzionale al numero di rotte più
avanti. Important.

#### I5 — Nessun `securitySchemes` (promozione del Minor 3 dichiarato)

Confermo, e anche qui alzo a Important. Dal documento `GET /api/v1/auth/me` e
`POST /api/v1/auth/refresh` risultano **pubbliche**: nessun `securitySchemes`,
nessun `security`, nessuna menzione del cookie `__Host-keeppix_session`. Un
generatore produce un client che chiama `/auth/me` senza credenziali e riceve
401 senza sapere perché. La descrizione è di quattro righe
(`apiKey`/`in: cookie` + `security(...)` sulle due operazioni protette).
Nota di contorno: §9.3 dello spec prevede che il client mobile usi
`Authorization`, quindi lo schema andrà comunque scritto — meglio ora che
retroattivamente su un contratto congelato.

### Minor

#### m1 — `components(schemas(...))` è interamente ridondante

utoipa 5 raccoglie da sé gli schemi referenziati dalle operazioni. Ho rimosso
**l'intero blocco** di sette voci da `openapi.rs`:

```
test openapi_snapshot_matches_the_committed_file ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Documento identico byte per byte. Sono nove righe di sorgente da cui non dipende
alcun output e alcun test; la loro presenza suggerisce a chi legge che elencare
un tipo lì sia il modo di farlo comparire nel documento (non lo è, se il tipo
non è referenziato da nessuna operazione — ed è esattamente la trappola in cui
si cadrà provando a rimediare a I4).

#### m2 — Nessun test confronta il documento *servito* con quello *committato*

Lo snapshot confronta `ApiDoc::openapi()` con il file; il test HTTP confronta la
risposta con una lista di asserzioni. I due non si incontrano mai. Prova: ho
fatto sì che `serve()` iniettasse nel documento restituito un percorso
inesistente:

```rust
doc.paths.paths.insert("/api/v1/auth/devices".to_owned(), PathItem::new(HttpMethod::Get, OperationBuilder::new()));
```

```
test openapi_snapshot_matches_the_committed_file ... ok
test openapi_document_carries_the_security_headers ... ok
test openapi_document_is_served_and_complete ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

L'artefatto congelato (da cui si generano i client) e l'endpoint runtime
possono divergere. Oggi il rischio è teorico — `serve()` è una riga — ma
l'asserzione mancante è banale: deserializzare il corpo servito e confrontarlo
con `docs/api/openapi.json`, che renderebbe fra l'altro superfluo metà del
primo test.

#### m3 — Tipi utili al generatore lasciati sul tavolo

- `UserView.role` è `{"type": "string"}` libera, mentre il server emette
  esattamente `"admin"` | `"user"` (match su `SystemRole` in
  `routes/auth.rs`). Un `#[schema(value_type = String, example = "admin")]` con
  `enum` darebbe al client Kotlin/Swift un tipo enumerato invece di `String`.
- `UserView.id` è un UUID (`u.id.to_string()`) senza `"format": "uuid"`, benché
  la feature `uuid` di utoipa sia abilitata in `Cargo.toml`. Lo spec §9.2 fa
  degli ID stabili UUID v7 un punto esplicito del contratto mobile.
- Nessun `servers` nel documento e `/api/openapi.json` non descrive sé stesso.

#### m4 — `info.version` è la versione del crate, non dell'API

`env!("CARGO_PKG_VERSION")` = `0.1.0`. È la scelta del piano (nota N5) e la
condivido come guardia, ma va messo a verbale che ogni bump di patch di
`keeppix-api` cambierà `info.version` del **contratto pubblico** e romperà lo
snapshot senza che l'API sia cambiata: rumore garantito nella CI del Task 15.

#### m5 — Difetti già dichiarati dall'implementer

Confermo 1 (`# Errors` come `summary`), 4 (ramo «scrivi se manca» a vuoto), 5
(fallimento senza diff), 6 (nessuna newline finale — verificato: ultimi byte
`b' }\n  ]\n}'`, `endswith(b'\n') == False`). I punti 2 e 3 sono trattati sopra
come I4 e I5.

---

## Esito dei comandi di verifica

| Comando | Esito |
| --- | --- |
| `pg_isready -h 127.0.0.1 -p 5432` | `accepting connections` (nessun avvio necessario) |
| `KEEPPIX_TEST_DATABASE_URL=… cargo test --workspace -- --test-threads=1` (1ª) | 23 gruppi `test result: ok`, 0 falliti. `openapi` 3/3, `auth` 13/13, `health` 3/3, lib api 4/4, db 7+14+6+12, domain 22, server config 4 |
| idem (2ª esecuzione consecutiva) | 23 gruppi ok, 0 falliti |
| `git status --porcelain` dopo entrambe | **vuoto** |
| `git diff --exit-code -- docs/api` | nessuna differenza — lo snapshot non si riscrive |
| `cargo clippy --workspace --all-targets -- -D warnings` (dopo `touch` su `lib.rs`, per non leggere la cache) | pulito, nessun warning |
| `cargo fmt --check` | pulito |
| `git diff 9d88cb4..HEAD -- crates/ docs/api/ Cargo.lock Cargo.toml` | vuoto: albero di codice identico al commit in review dopo tutte le mutazioni |

Testcontainers non è utilizzabile (egress bloccato): suite eseguita contro il
PostgreSQL 16 locale, come da dispatch. I test del Task 11 non toccano il DB.

### Red-then-green dell'implementer, rifatto

- **Rotta fuori da `common_layers`** — non ripetuto verbatim: la stessa proprietà
  è coperta trasversalmente e l'asserzione è manifestamente viva (l'helper legge
  quattro header con `.unwrap()` su `Option`). Confermo l'output riportato come
  plausibile e coerente con `tests/openapi.rs:15`.
- **Snapshot alterato** → confermato indirettamente e in forma più forte dalla
  prova B di I1: una mutazione del *codice* (non del file) fa fallire lo
  snapshot con il messaggio atteso.
- **Documento incompleto** → confermato dalla prova C di I1 e dalla prova m2
  (rimozione di `/api/v1/auth/logout` dal documento servito → `manca il percorso
  /api/v1/auth/logout`).
- **Mutazione non provata dall'implementer: rimozione di `#[utoipa::path]`.**
  Tolto l'attributo da `logout`, il crate **non compila**:
  `error[E0412]` / `error[E0433]` su `#[derive(OpenApi)]` in `openapi.rs:8`,
  «similarly named struct `__path_login` defined here». Questa direzione è quindi
  protetta dal compilatore, non dai test — ed è protetta bene.

---

## «Nasce dal codice, così non può divergere»: cosa è vero e cosa no

**Vero.** I *corpi* nascono davvero dai tipi Rust. `LoginResponse`, `MeResponse`,
`SetupStatus`, `SetupRequest`, `UserView` sono gli stessi tipi che gli handler
serializzano, quindi `required`, opzionalità e nomi dei campi seguono la struct
senza intervento umano: rinominare un campo o renderlo `Option` aggiorna il
documento da solo. È verificabile nel documento committato (`SetupRequest`
richiede `username, display_name, password` e non `email`; `UserView` marca
`email` e `locale` come `["string","null"]`). E l'aggancio fra `paths(...)` e gli
handler è **rafforzato dal compilatore**: togliere un `#[utoipa::path]` o
nominare un `body =` inesistente non compila.

**Non vero.** *Percorso, metodo ed esistenza stessa dell'operazione sono
stringhe scritte a mano nell'attributo*, non estratte dal router. Niente nel
build e niente nei test lega `#[utoipa::path(path = "/api/v1/auth/login")]` a
`Router::route("/auth/login", post(...))`. Le due stringhe si trovano in file
diversi e possono essere modificate indipendentemente (prove A e B). Il file di
test, per giunta, interroga un router che non monta affatto `/api/v1`.

**Vero a metà.** Gli *status code* sono elencati a mano e non derivano dal tipo
di ritorno: `Result<_, Problem>` può produrre 404, 409, 422 e 500 che
l'annotazione può tacere — e infatti ne tace (I2). Il tipo di successo invece è
inchiodato dal `body =`.

**Formulazione corretta.** Il documento non può divergere dalla *forma dei dati*;
può divergere dalla *superficie HTTP*. Lo snapshot rileva ogni cambiamento del
documento, ma non sa distinguere un cambiamento voluto da uno sbagliato, e il
suo stesso messaggio di errore indica come riallinearlo — quindi protegge dalla
deriva *inosservata*, non dalla deriva *sbagliata*.

---

## Fix wave proposta (scoped)

Ordine di rapporto valore/costo, tutta additiva e tutta dentro il perimetro del
Task 11:

1. **I3** — sei `operation_id` espliciti e namespaced. Costo: sei righe + snapshot.
   È l'unica voce che diventa più cara col tempo, perché finisce nei client generati.
2. **I2** — dichiarare 404 su `me` e 500 dove il piano lo consente. Costo: tre righe.
3. **I4 + I5** — `ToSchema` su `Problem`, `body = Problem` sulle risposte d'errore,
   `securitySchemes` a cookie con `security(...)` sulle due rotte protette.
4. **I1** — test documento→rotte (per ogni operazione, status ≠ 404/405), oppure
   rinvio esplicito al Task 15 con la riserva messa a verbale nel piano.
5. **m1** — rimuovere il blocco `components(schemas(...))` *dopo* aver fatto il
   punto 3 (serve a decidere dove mettere `Problem`), o tenerlo con un commento
   che dica che è documentazione, non configurazione.

Rinviabili senza costo crescente: m2, m3, m4, e i Minor 1/4/5/6 dell'implementer.
