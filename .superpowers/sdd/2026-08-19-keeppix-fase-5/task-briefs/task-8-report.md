# Task 8 — `DELETE`, `LOCK`, `UNLOCK`: report

## Esito

**DONE**

Commit: `cb217dd` (`feat(api): WebDAV DELETE through trash, Class 2 locks
for Finder and Explorer`), su `fase-5`. Non pushato: in attesa di
istruzioni esplicite (vedi `AGENTS.md`, "Non fare push ... senza che
l'utente lo chieda").

## File creati/modificati

- CREATE `crates/keeppix-db/migrations/0028_dav_locks.sql` — tabella
  `dav_locks` esattamente come nello schema del brief, più un indice su
  `resource_path` (usato da `LOCK` senza `If:` per verificare in fretta se
  la risorsa ha già un lock attivo).
- CREATE `crates/keeppix-db/src/dav_locks.rs` — `DavLockRepo` con
  esattamente i 4 metodi richiesti dal brief (`create`, `refresh`,
  `delete`, `is_locked`), nessuno in più. TTL fisso a 3600s
  (`LOCK_TTL_SECONDS`), interpolato come costante del codice in `format!`
  (mai un valore esterno concatenato in SQL).
- MODIFY `crates/keeppix-db/src/lib.rs` — `pub mod dav_locks;` +
  re-export `DavLockRepo`, commento del migratore aggiornato a
  `0028_dav_locks`.
- MODIFY `crates/keeppix-db/src/folders.rs` — nuovo
  `FolderRepo::delete_subtree(ctx, folder_id) -> Result<(), DbError>`:
  cancella dal **solo database** la cartella e tutto il suo sottoalbero
  (`DELETE FROM folders WHERE path <@ ...`, riusando lo stesso predicato
  di `subtree`), dopo aver verificato il permesso con `assert_editor`
  (poi ristretto a owner/admin dal chiamante, vedi sotto). Nessuna azione
  sul filesystem qui: la separa `dav::delete::folder`.
- CREATE `crates/keeppix-api/src/dav/delete.rs` — `asset()` e `folder()`.
  Entrambi passano **sempre** da `TrashRepo::choose(ctx, id,
  DiskAction::MovedToTrash)`, mai un `rm`/`remove_file` diretto. Prima di
  chiamare `choose`, un gate esplicito (`only_owner_or_admin`, stesso
  predicato di `may_purge` in `trash.rs`) richiede owner/admin — vedi la
  sezione "Scoperta importante" sotto per il perché.
- CREATE `crates/keeppix-api/src/dav/lock.rs` — `lock()` (`pub(crate)`,
  prende `&Resource`, tipo privato al modulo `dav`) e `unlock()` (`pub`).
  `LOCK` senza `If:` crea un nuovo lock esclusivo, `423 Locked` se la
  risorsa ne ha già uno attivo (usa `DavLockRepo::is_locked`); con `If:
  (<token>)` tenta un rinnovo (`412` se il token è scaduto/inesistente).
  `UNLOCK` richiede l'header `Lock-Token: <token>` (`400` se assente),
  usa `DavLockRepo::refresh` come test-and-set per distinguere "token
  attivo" da "scaduto o mai esistito" senza introdurre un quinto metodo di
  repository (vedi ledger).
- MODIFY `crates/keeppix-api/src/dav/mod.rs` — `pub mod delete;`/`pub mod
  lock;`; `Resource` promossa da privata a `pub(crate)` (serve a
  `dav::lock::lock`, che ne ha bisogno per calcolare la chiave del lock);
  dispatch nuovo per `DELETE`/`LOCK`/`UNLOCK`; commenti di modulo
  aggiornati (solo `COPY` resta `501`).
- MODIFY `crates/keeppix-api/src/problem.rs` — due nuovi costruttori,
  `Problem::precondition_failed()` (`412`) e `Problem::locked()` (`423`),
  sullo stesso stile delle altre costanti già presenti nel file (es.
  `payload_too_large`).
- CREATE `crates/keeppix-api/tests/webdav_delete_lock.rs` — 5 test (4
  richiesti dal brief + 1 in più, `unlock_without_a_lock_token_header_returns_400`,
  per non lasciare il caso limite "`UNLOCK` senza header → `400`" solo
  descritto a parole nel brief senza un test che lo pinni).

Non ho toccato `Cargo.toml`: nessuna dipendenza nuova (`uuid`, `chrono`,
`sqlx` erano già dipendenze di `keeppix-db`; `axum`/`http` già di
`keeppix-api`).

## Scoperta importante rispetto al brief: il gate di permesso su `DELETE`

Il brief afferma: *"Non serve codice aggiuntivo per il permesso:
`TrashRepo::choose` lo gestisce già"* e subito prima: *"`TrashRepo::choose`
con `DiskAction::MovedToTrash` richiede... owner/admin oppure... editor
senza `may_purge`"*. Ho verificato leggendo `trash.rs` per intero: per
`DiskAction::MovedToTrash`, `choose` chiama `PermissionRepo::assert_can_edit_assets`,
che **accetta volutamente** un editor (non solo owner/admin) —
`may_purge` (owner/admin) si applica **solo** a `DiskAction::Purged`. Ho
confermato che questo è comportamento intenzionale e già testato: il test
esistente `crates/keeppix-api/tests/permissions_roles.rs::a_folder_editor_can_edit_metadata_and_trash`
(Task 14b, Fase 3) asserisce esplicitamente `204` quando un editor manda
`DELETE /api/v1/assets/{id}` con `disk_action: moved_to_trash` sulla
REST API.

Le istruzioni del compito sono però esplicite e senza margine
d'interpretazione: *"Un editor riceve 403 su DELETE (stesso codice che
`may_purge` applica nella web app)"*. Scritto il primo test
(`delete_by_editor_returns_403`) esattamente per questo scenario
(`grant_folder_editor`, poi `DELETE /dav/asset/{id}`), l'ho eseguito prima
di aggiungere qualunque gate extra: **falliva con `204`, non `403`** —
la conferma diretta che chiamare solo `TrashRepo::choose` non basta.

Ho quindi aggiunto un gate esplicito, **prima** di `TrashRepo::choose`,
in `dav::delete::asset`/`folder`: `only_owner_or_admin(ctx, &library)`,
lo stesso predicato di `may_purge` (owner della libreria o admin). Il
risultato è che `DELETE` via `WebDAV` è **deliberatamente più
restrittivo** della REST API — solo owner/admin, mai un editor — anche
se l'azione fisica eseguita resta sempre `MovedToTrash` (mai `Purged`:
l'invariante "sempre reversibile via cestino" non è toccata, cambia solo
*chi* può innescarla via questo protocollo specifico). Motivazione:
il protocollo `WebDAV` non ha un dialogo di conferma né la possibilità di
scegliere `disk_action` come la REST API — un trascinamento per sbaglio
nel Finder da parte di chi ha solo un permesso di editor su una cartella
condivisa non deve poter cestinare file altrui senza che nessuno lo possa
prevenire con un "sei sicuro?".

Documentato nel ledger (`progress.md`) come `Ruling`, con il costo se
l'interpretazione fosse sbagliata: rimuovere una singola guardia, non un
redesign.

## `LOCK`/`UNLOCK` — dettagli di design

- **Token**: `format!("opaquelocktoken:{}", Uuid::now_v7())`, esattamente
  come nel brief.
- **Percorso opaco** (`dav_locks.resource_path`): lo stesso path
  `/dav/folder/{id}` / `/dav/asset/{id}` / `/dav/folder/{id}/{name}` con
  cui il client indirizza la risorsa — mai un percorso filesystem, stesso
  principio del resto di `dav::mod`.
- **Visibilità**: `LOCK` richiede che il chiamante veda almeno la risorsa
  (o, per un figlio non ancora creato, la cartella genitore) —
  `FolderRepo::find_by_id`/`AssetRepo::find_by_id`, che sono già
  `Forbidden`-mai-`NotFound` per un id che non appartiene al chiamante.
  Non richiede *editor*: bloccare un file per scriverci sopra è un passo
  che precede il vero controllo di scrittura, applicato poi dal `PUT`
  stesso.
- **Refresh vs nuovo lock**: `LOCK` con header `If: (<token>)` tenta un
  rinnovo (`DavLockRepo::refresh`); senza `If:`, crea un lock nuovo — ma
  prima controlla `is_locked(path)` e risponde `423 Locked` se la risorsa
  ha già un lock attivo (non richiesto da un test dedicato, ma l'unico
  modo di rendere `is_locked` — quarto metodo esplicitamente nella spec
  del brief — effettivamente usato da qualche parte).
- **`UNLOCK`**: richiede `Lock-Token: <token>` (`400` se assente/vuoto).
  Usa `refresh` come test-and-set (vedi ledger) per distinguere "token
  attivo" (rinnova+cancella, `204`) da "scaduto o mai esistito" (`404`),
  senza introdurre un quinto metodo di repository fuori dai 4 elencati
  dal brief.
- **XML di risposta**: esattamente lo schema del brief
  (`D:lockdiscovery`/`D:activelock`/...), con `depth`/`token` interpolati.
  Header `Lock-Token: <opaquelocktoken:...>` e `Content-Type: application/xml;
  charset="utf-8"`.

## `DELETE` su una cartella intera

Non esercitata da nessuno dei 4 test richiesti (solo `DELETE
/dav/asset/{id}` lo è), ma implementata per completezza dello spec del
brief: `dav::delete::folder` usa `FolderRepo::subtree` per ottenere la
cartella e ogni suo discendente, `AssetRepo::find_by_folder` +
`TrashRepo::choose` per cestinare ogni asset di ciascuna (mai un `rm -rf`
diretto), poi il nuovo `FolderRepo::delete_subtree` (singola `DELETE ...
WHERE path <@ ...`, nessuna azione sul filesystem) e infine
`tokio::fs::remove_dir_all` sulla directory fisica — **solo dopo** che
ogni asset è già al sicuro nel cestino. Stesso gate owner/admin di
`asset()`. Nessun test dedicato: annotato nel ledger come lacuna di
copertura nota, non come funzionalità mancante.

## TDD — cosa ho davvero osservato

1. Scritto `tests/webdav_delete_lock.rs` per primo, con i 4 test richiesti
   dal brief più `unlock_without_a_lock_token_header_returns_400`, contro
   un dispatcher che rispondeva ancora `501` per `DELETE`/`LOCK`/`UNLOCK`.
2. Prima esecuzione (`cargo test -p keeppix-api --test webdav_delete_lock
   --no-run`): **compilazione riuscita** senza modifiche — a differenza
   del Task 7, qui non serviva alcun tipo/simbolo nuovo lato test
   (`grant_folder_editor` esisteva già in `journey/mod.rs`), quindi il
   fallimento atteso arriva solo all'esecuzione.
3. Prima esecuzione reale: **5/5 falliti**, tutti con `501` invece dello
   status atteso (`204`/`403`/`200`/`400`) — il motivo giusto, non un
   errore di battitura nel test.
4. Implementata la migrazione, `DavLockRepo`, `dav::delete`, `dav::lock`,
   il dispatch in `mod.rs`, i due nuovi costruttori di `Problem`.
   Rieseguito: **4/5 verdi al primo giro**, un fallimento genuino scoperto
   dal test stesso — `delete_by_editor_returns_403` restituiva `204`
   invece di `403`, perché `TrashRepo::choose` da sola accetta un editor
   per `MovedToTrash` (vedi la sezione "Scoperta importante" sopra).
   Corretto aggiungendo il gate owner/admin. Rieseguito: **5/5 verdi**.

### Mutazioni deliberate (prima del commit, mai committate)

- **Gate di permesso rimosso** (`if !only_owner_or_admin(...)` →
  `if !true`): `delete_by_editor_returns_403` è tornato a fallire con
  `204` — la prova che il test cattura davvero l'invariante richiesta
  dal compito. Ripristinato subito dopo.
- **`DiskAction::MovedToTrash` → `DiskAction::Purged`** in
  `dav::delete::asset`: `delete_asset_moves_it_to_trash_not_file_system_removal`
  è fallito sull'assert `disk_action == "moved_to_trash"` (valeva
  `"purged"`) — la prova che il test distingue davvero le due azioni, non
  solo "una riga di audit qualunque esiste". Ripristinato subito dopo.
- **Condizione `AND timeout_at > now()` rimossa** da `DavLockRepo::refresh`:
  `lock_and_unlock_work_and_unlock_rejects_expired_token` è fallito
  sull'`UNLOCK` con il token scaduto, che è tornato a rispondere `204`
  invece di `404` — la prova che il test intercetta davvero un lock
  scaduto trattato come ancora valido. Ripristinato subito dopo.

Nessuna delle tre mutazioni è mai stata committata: verificato con `git
diff`/`git status` prima del commit finale (working tree pulito subito
prima di `git add`).

## Verifica — output osservato

```
$ cargo fmt --check
(nessun output, exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.37s
(nessun warning/errore)

$ cargo test -p keeppix-api --test webdav_delete_lock -- --test-threads=1
running 5 tests
test class_2_lock_token_response_has_correct_headers ... ok
test delete_asset_moves_it_to_trash_not_file_system_removal ... ok
test delete_by_editor_returns_403 ... ok
test lock_and_unlock_work_and_unlock_rejects_expired_token ... ok
test unlock_without_a_lock_token_header_returns_400 ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test -p keeppix-api -- --test-threads=1
(tutti i binari tests/*.rs + unit test di src/lib.rs + doc-test)
... [ogni binario: test result: ok, 0 failed] ...
37 righe "test result: ok" totali (grep), zero FAILED/panicked/error[.

$ cargo test -p keeppix-db -- --test-threads=1
(tutti i binari tests/*.rs, inclusi folders.rs/dav_locks non hanno un
proprio file di test dedicato in keeppix-db — coperti indirettamente
dai test HTTP di keeppix-api)
... [ogni binario: test result: ok, 0 failed] ...
34 righe "test result: ok" totali (grep), zero FAILED/panicked/error[.
```

Rieseguita l'intera suite `keeppix-api` una seconda volta dopo `cargo
fmt` (che ha solo riformattato spazi bianchi, nessuna modifica
semantica) per la conferma finale: stesso risultato, 37/37 verde.

Non ho eseguito `./scripts/test.sh` per lo stesso motivo dei task
precedenti della fase (5, 6, 7): questo task tocca solo `keeppix-api` e
`keeppix-db`, e `cargo clippy --workspace --all-targets` (che compila
l'intero workspace, `keeppix-server` compreso) più le suite complete di
`keeppix-api`/`keeppix-db` coprono lo stesso perimetro senza il costo di
un `cargo clean` finale imposto dallo script. Non ho toccato `frontend/`.

## Self-review sugli invarianti di `AGENTS.md`

- **Nessun SQL fuori da `keeppix-db`**: `dav/delete.rs` e `dav/lock.rs`
  chiamano solo `AssetRepo`/`FolderRepo`/`TrashRepo`/`DavLockRepo` — zero
  query dirette negli handler.
- **`DELETE` via `WebDAV` sempre attraverso `TrashRepo::choose` con
  `DiskAction::MovedToTrash`, mai `rm` diretto**: verificato con la
  mutazione deliberata (Purged) sopra — nessun percorso del codice chiama
  `std::fs::remove_file`/`remove_dir` su un asset, solo su una directory
  già vuota di asset reali dopo che `choose` li ha spostati via.
- **Un editor riceve `403`, non un successo silenzioso**: verificato con
  la mutazione deliberata sul gate — il test fallisce se il gate manca.
- **Query sempre parametrizzate**: l'unica interpolazione non
  parametrizzata in `dav_locks.rs` è `LOCK_TTL_SECONDS`, una costante del
  codice (mai un valore dal client), nello stesso spirito dell'eccezione
  già documentata in `AGENTS.md` per le liste di colonne.
- **Nessun `unwrap()`/`expect()` in codice di produzione**: verificato a
  mano in `delete.rs`, `lock.rs`, `dav_locks.rs`, `folders.rs` — zero
  occorrenze fuori dai moduli test (`#[allow(clippy::unwrap_used,
  clippy::expect_used)]` a livello di modulo su `webdav_delete_lock.rs`,
  come convenzione del progetto).
- **`sqlx` solo in forma funzione**: `dav_locks.rs`/`folders.rs` usano
  solo `sqlx::query`/`sqlx::query_as`/`sqlx::query_scalar`, mai `query!`.
- **Un id che non appartiene al chiamante riceve `Forbidden`, mai
  `NotFound`**: `only_owner_or_admin` si applica **dopo**
  `FolderRepo::assert_editor`/`AssetRepo::find_by_id`, che sono già
  `Forbidden`-per-costruzione su un id non visibile; il gate aggiunge solo
  un `Forbidden` in più per un editor visibile ma non owner/admin — mai
  un `NotFound` nuovo introdotto.

## Decisioni riportate nel ledger (`progress.md`)

- Gate owner/admin esplicito su `DELETE` `WebDAV` (più restrittivo della
  REST API), con la scoperta che il brief era in errore su questo punto.
- `unlock()` riusa `refresh()` come test-and-set, niente quinto metodo di
  repository.
- `423 Locked`/`412 Precondition Failed` implementati ma non esercitati
  da un test dedicato (casi limite del brief, non dei 4 test richiesti).
- `DELETE /dav/folder/{id}` implementata per completezza ma senza un test
  dedicato.
