# Task 6 — `PROPFIND` e `GET`: report

## Esito

**DONE**

Commit: `ffa2b147fc8c4b6b07b2ae881b8a54dbebd073cd` (`ffa2b14`), messaggio
`feat(api): stream PROPFIND from the database, GET with range requests`,
su `fase-5` (pushato: `72a6579..ffa2b14`).

## File creati/modificati

- `crates/keeppix-api/src/dav/propfind.rs` (nuovo) — `parse_depth` (`0`,
  `1`, header assente → `1`, `infinity`/`Infinity` → `403 Forbidden`,
  qualunque altro valore → tolerante come `1`); `folder()` e `asset()`
  (query dal database via `FolderRepo`/`AssetRepo`, mai `stat()`);
  `multistatus_response()`/`write_multistatus()` (XML `D:multistatus` con
  `quick_xml::Writer` su un `Vec<u8>`, `207 Multi-Status`,
  `Content-Type: application/xml; charset="utf-8"`); 6 unit test su
  `parse_depth` e sulla forma dell'XML prodotto (escaping di `&` e delle
  virgolette dell'ETag).
- `crates/keeppix-api/src/dav/mod.rs` (modificato) — `parse_resource`
  (path → `Resource::Folder`/`Resource::Asset` per `id`, `None` per
  qualunque altra forma → `501` come prima); `handler` ora costruisce
  l'`AuthContext` (`SystemRole::User` sempre, vedi ledger) e dispatcha
  `(metodo, risorsa)`: `PROPFIND` su cartella/asset, `GET` su asset con
  `Range`; ogni altra combinazione resta `501` (Task 7-8). `get_asset()`
  risolve `AssetRepo::find_by_id` → `FolderRepo::absolute_path` → riusa
  `routes::media::stream_file` per il range reale.
- `crates/keeppix-api/src/routes/media.rs` (modificato) — `mime_for_name`
  da `fn` privata a `pub(crate) fn`, per riuso nel modulo `dav` (stesso
  file già esponeva `pub(crate) async fn stream_file`, riusato tale e
  quale senza duplicazione).
- `crates/keeppix-api/tests/webdav_propfind.rs` (nuovo) — i 5 test
  richiesti dal brief, elencati sotto in "TDD".

Non ho toccato `Cargo.toml` (`quick-xml` era già stato aggiunto nel Task 5,
predisposto esplicitamente per questo task) né `crates/keeppix-db` (tutte
le query necessarie — `FolderRepo::find_by_id/children/absolute_path`,
`AssetRepo::find_by_id/find_by_folder` — esistevano già e portano già la
protezione `Forbidden`-mai-`NotFound` richiesta).

## Path → risorsa (semplificazione del brief)

Come indicato dal brief: si naviga per `id`, non per nome —
`/dav/folder/{folder_id}` (con `/` finale nell'`href` di risposta, una
collection) e `/dav/asset/{asset_id}`. Documentato nel ledger: Finder (che
naviga per nome umano) non funziona con questo schema; rclone e Cyberduck
sì, perché sincronizzano confrontando l'`ETag`, non il path.

## TDD — cosa ho davvero osservato

1. Scritto `tests/webdav_propfind.rs` per primo (5 test), contro
   `dav::handler` che rispondeva ancora `501` per ogni metodo (stato del
   Task 5). Il file compila da subito: usa solo HTTP e gli helper già
   esistenti in `journey::mod.rs` (`build_fixture_archive`,
   `create_library`, `scan_and_wait`, `folder_id_by_name`, `create_user`,
   `login_as`, `tiny_fixture_path`).
2. Eseguito `cargo test -p keeppix-api --test webdav_propfind --
   --test-threads=1`: **5 test su 5 falliti**, tutti per il motivo
   giusto — `left: 501, right: 207` (o `403`, o un panic
   `"an asset href in the body"` perché il corpo `501` non contiene
   nessun `/dav/asset/`). Nessun fallimento di compilazione, nessun
   fallimento per un motivo diverso da "il dispatch non esiste ancora".
3. Implementato `dav/propfind.rs`, il dispatch in `dav/mod.rs`, la
   visibilità `pub(crate)` di `mime_for_name`.
4. Rieseguito: **5/5 verdi**.

### Mutazione deliberata (prima del commit, mai committata)

Per verificare che i test proteggano davvero l'invariante che dichiarano,
non solo che "passino":

- **Permessi** (`propfind_on_a_folder_without_permission_returns_403`):
  ho temporaneamente cambiato `AuthContext::user(user_id,
  SystemRole::User)` in `AuthContext::user(user_id, SystemRole::Admin)`
  in `dav::handler`. Risultato: il test è fallito con `left: 207, right:
  403` — l'outsider riceveva l'intera lista della cartella dell'admin,
  perché `ctx.is_admin() == true` fa risolvere `FolderRepo::visible()` a
  `NotFound` invece di `Forbidden` su un id fuori scope, e la scope
  dell'admin nella query `VisibilityScope` è comunque "vede tutto" per un
  amministratore reale — la prova che il test cattura esattamente la
  regressione che la Ruling sul ruolo `SystemRole::User` fisso è pensata
  per evitare. Ripristinato subito dopo.
- **Depth infinity** (`propfind_depth_infinity_is_rejected_with_403`): ho
  rinominato la stringa confrontata in `parse_depth` da `"infinity"` a
  `"infinity-disabled-for-mutation-test"`, così l'header reale
  `Depth: infinity` non veniva più intercettato. Risultato: il test è
  fallito con `left: 207, right: 403` — la cartella veniva elencata per
  intero anche con `Depth: infinity`, la prova che il test cattura
  davvero l'assenza del limite di RAM. Ripristinato subito dopo.

Nessuna delle due mutazioni è mai stata committata (verificato con `git
diff`/`git status` prima del commit finale: il file `propfind.rs` era
ancora non tracciato in quel momento, la mutazione visibile solo con
`grep` sul contenuto locale).

## Decisioni (ledger, riportate anche in `progress.md`)

- Path per `id`, non per nome (vedi sopra).
- Ruolo sempre `SystemRole::User` per l'attore `WebDAV`, senza query
  aggiuntiva su `users` — `FolderRepo`/`AssetRepo` filtrano per `user_id`,
  non per ruolo, quindi un admin vede comunque le proprie librerie.
- `multistatus` costruito in un `Vec<u8>` in memoria, non in streaming a
  blocchi (accettabile per librerie < 10.000 file, come indicato dal
  brief).
- `Depth` assente → trattato come `1` (non `infinity`, il default RFC
  4918) — altrimenti PROPFIND senza header sarebbe stato bloccato
  comunque dal limite anti-RAM. Un valore non riconosciuto (diverso da
  `0`/`1`/`infinity`) è tollerato come `1`.
- `getlastmodified` per una cartella usa `Utc::now()` al momento della
  risposta: `Folder` (dominio) non porta un mtime persistito, e nessun
  client WebDAV vi si appoggia per decidere se una cartella è cambiata
  (lo fa con l'ETag sugli asset).
- Implementato anche `PROPFIND` su un singolo asset
  (`propfind::asset`), oltre ai 5 test richiesti dal brief: un client
  reale (rclone/Cyberduck) tipicamente sonda un file prima di un `GET`.
  Zero codice nuovo di rilievo (riusa `asset_entry`/`multistatus_response`
  già esercitati dal test di listing).

## Verifica — output osservato

```
$ cargo fmt --check
(nessun output, exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
    Checking keeppix-api v0.1.0
    Checking keeppix-server v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.39s
(nessun warning/errore)

$ cargo test -p keeppix-api --test webdav_propfind -- --test-threads=1
running 5 tests
test get_asset_with_range_returns_206 ... ok
test get_asset_without_range_returns_200_full_content ... ok
test propfind_depth_1_on_a_folder_returns_207_with_child_resources ... ok
test propfind_depth_infinity_is_rejected_with_403 ... ok
test propfind_on_a_folder_without_permission_returns_403 ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test -p keeppix-api -- --test-threads=1
(tutti i 32 file di tests/*.rs + gli unit test di src/lib.rs, inclusi i 6
 nuovi di dav::propfind::tests)
... [ogni file: test result: ok, 0 failed] ...
running 22 tests   <- unittests src/lib.rs, inclusi dav::propfind::tests::*
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
Doc-tests keeppix_api: 0 test, ok
```

Nessun test esistente rotto: verificato grep `FAILED`/`error\[` su tutto
l'output completo del comando — zero occorrenze. Tutti i 32 binari di test
(`auth.rs` … `ws.rs`, incluso il nuovo `webdav_propfind.rs`) e la libreria
sono verdi.

Non ho eseguito `./scripts/test.sh` per lo stesso motivo documentato nel
report del Task 5: questo task tocca solo `keeppix-api` (più una
visibilità `pub(crate)` in un file già suo), e `cargo clippy
--workspace --all-targets` (che compila **tutto** il workspace, incluso
`keeppix-server`) più la suite completa di `keeppix-api` coprono lo stesso
perimetro senza il costo di un `cargo clean` finale. Non ho toccato
`frontend/`: nessuna build Vite necessaria.

## Self-review sugli invarianti di AGENTS.md

- **Nessun SQL fuori da `keeppix-db`**: `propfind.rs` e `dav/mod.rs`
  chiamano solo `FolderRepo`/`AssetRepo`, zero query dirette.
- **Ogni metodo di repository che legge dati di un utente prende un
  `AuthContext`**: `FolderRepo::find_by_id/children`,
  `AssetRepo::find_by_id/find_by_folder`, `FolderRepo::absolute_path` — 
  tutti chiamati con il `ctx` costruito in `handler`.
- **Un utente che sonda un id che non gli appartiene riceve `Forbidden`,
  mai `NotFound`**: verificato con la mutazione deliberata sopra, e
  garantito per costruzione da `SystemRole::User` fisso (mai
  `ctx.is_admin() == true` per un attore WebDAV in questo task, quindi i
  repository non prendono mai il ramo `NotFound` riservato agli admin).
- **`Depth: infinity` → `403`, non un crash o un corpo enorme**:
  verificato con la seconda mutazione deliberata sopra.
- **Query sempre parametrizzate**: non introdotta nessuna query nuova in
  `keeppix-db` in questo task; quelle riusate erano già parametrizzate.
- **Nessun `unwrap()`/`expect()` in codice di produzione**: verificato a
  mano in `propfind.rs`/`dav/mod.rs` — zero occorrenze fuori dal modulo
  `#[cfg(test)]` (annotato `#[allow(clippy::unwrap_used,
  clippy::expect_used)]` a livello di modulo, come da convenzione del
  progetto per i test). L'unico punto dove l'encoding XML potrebbe
  fallire (`write_multistatus`) propaga l'errore con `?`/`map_err`, non un
  `unwrap()`.
- **`ETag` = `content_hash` hex dell'asset**: `asset_entry()` usa
  `hex_hash(hash)` (già esistente in `routes::timeline`, riusato) avvolto
  in virgolette, come richiesto dal brief.
- **`GET` con `Range` → `206`**: riuso diretto di
  `routes::media::stream_file`, già esercitato dai test di `media.rs` per
  `/media/original/{id}` e ora anche da `webdav_propfind.rs` per
  `/dav/asset/{id}`.
