# Task 7 — `PUT`, `MKCOL`, `MOVE`, `COPY`: report

## Esito

**DONE**

Commit: `4d2564e` (`feat(api): WebDAV write operations reusing folder and
permission repos`), su `fase-5`. Non pushato: in attesa di istruzioni
esplicite (vedi `AGENTS.md`, "Non fare push ... senza che l'utente lo
chieda").

## File creati/modificati

- `crates/keeppix-api/src/dav/write.rs` (nuovo) — `put()`, `put_dotfile()`,
  `put_response()`, `mkcol()`, `move_folder()`, e gli helper
  `write_body_to_file()` (streaming del corpo su un temporaneo, mai
  bufferizzato per intero in RAM) e `hash_temp_file()` (blake3 fuori dal
  thread async, come `routes::upload::finalize_upload`).
- `crates/keeppix-api/src/dav/mod.rs` (modificato) — `Resource` esteso con
  `FolderChild(FolderId, String)` per indirizzare un figlio non ancora
  creato (`/dav/folder/{id}/{name}`); `parse_folder_resource`,
  `is_valid_path_component` (anti-traversal: rifiuta nomi vuoti, con `/`,
  `\`, NUL, `.`, `..`), `percent_decode` (RFC 3986 §2.1, per nomi con
  spazi/accenti), `parse_destination`/`strip_origin` (header `Destination`
  di `MOVE`, con o senza URI assoluto); il `handler` ora estrae metodo,
  header e corpo come valori posseduti prima del dispatch (per evitare un
  conflitto di prestito quando il corpo va consumato da `PUT`), e
  aggiunge i rami `PUT`/`MKCOL`/`MOVE` che delegano a `dav::write`.
- `crates/keeppix-db/src/folders.rs` (modificato) — nuovo
  `FolderRepo::assert_editor(ctx, folder_id) -> Result<(Folder, Library),
  DbError>`: centralizza il controllo "owner o admin o `ObjectRole::Editor`
  via `PermissionRepo::effective_role`" già duplicato altrove
  (`move_subtree` lo fa inline), riusato da `put`, `mkcol` e da entrambi i
  lati di `move_folder`.
- `crates/keeppix-db/src/assets.rs` (modificato) — nuovo
  `DirectPutOutcome` e `AssetRepo::ingest_direct(...)`: stessa logica di
  collisione di `UploadSessionRepo::finalize` (stesso nome+hash → skip
  come duplicato; stesso nome, hash diverso → suffisso numerico via
  `unique_suffixed_name`), ma senza sessione — pensata per un `PUT` che
  arriva già come file intero.
- `crates/keeppix-db/src/uploads.rs` (modificato) — `unique_suffixed_name`,
  `remove_file_tolerant`, `map_unique_violation` da `fn` privata a
  `pub(crate) fn`, per riuso in `assets::ingest_direct` senza duplicare la
  logica di suffisso/pulizia/mappatura del vincolo di unicità.
- `crates/keeppix-db/src/lib.rs` (modificato) — esporta anche
  `DirectPutOutcome` dal modulo `assets`.
- `crates/keeppix-api/src/routes/upload.rs` (modificato) — `enqueue_indexing`
  da `fn` privata a `pub(crate) fn`, riusata tale e quale da `dav::write::put`
  per accodare `JobKind::ExtractMetadata`/`JobPriority::High` sullo stesso
  identico percorso del `tus` upload.
- `crates/keeppix-api/tests/webdav_write.rs` (nuovo) — i 7 test richiesti dal
  brief, elencati sotto in "TDD".

Non ho toccato `Cargo.toml` né altri crate: nessuna dipendenza nuova.

## `COPY`: non implementata (stub `501`), come esplicitamente permesso dal brief

Il dispatch in `dav::mod.rs` non ha un ramo per `COPY`: cade nel
`_ => not_implemented()` già esistente, quindi risponde `501` esattamente
come prima di questo task. `copy_subtree` **non** è stata aggiunta a
`folders.rs`.

Motivo (vedi anche il ledger): copiare un intero sottoalbero richiede
nuovi id per ogni cartella e ogni asset copiato (righe `assets`
indipendenti — l'identità è `(folder_id, filename)`, quindi una copia è
per costruzione un'entità nuova, non un alias), un controllo di spazio
libero sulla libreria di *destinazione* — che il brief nota esplicitamente
può differire da quella di partenza — e la copia fisica ricorsiva dei file
sul disco. Nessuno di questi tre pezzi riusa in modo diretto codice
esistente (`move_subtree` sposta puntatori nel database, non copia nulla;
non esiste oggi un calcolo di spazio libero per una libreria arbitraria di
destinazione riusabile qui). Il brief marca esplicitamente `COPY` come
opzionale con questa via di uscita, e nessun test del brief la esercita.

## Regola di collisione `PUT` — invariante verificata

- Stesso nome + stesso hash → `AssetRepo::ingest_direct` risolve a
  `CollisionOutcome::SkippedDuplicate`, il file temporaneo viene rimosso
  (`remove_file_tolerant`), **zero riga nuova in `assets`**, risposta
  `204 No Content`. Test: `put_of_same_content_skips_without_creating_a_second_file`.
- Stesso nome + hash diverso → `unique_suffixed_name` genera `nome_1.ext`
  (poi `_2`, ...), il file va al nuovo path, risposta `201 Created` con
  `Location: /dav/asset/{new_id}` che **non** corrisponde al nome
  richiesto. Test: `put_of_same_name_different_content_saves_with_suffix`.
- **Mai un `rename()` sopra un file esistente non gestito da queste due
  regole**: il temporaneo va sempre prima in `.keeppix-tmp/` dentro la
  libreria, poi `ingest_direct` decide il path finale sotto transazione,
  quindi non c'è una finestra in cui un secondo `PUT` concorrente possa
  vedere un file a metà scrittura al posto giusto.

Eccezione documentata e voluta: i dotfile (vedi sotto) sovrascrivono
l'omonimo, perché non sono mai indicizzati come `assets` — l'invariante
protegge le foto dell'utente, non la cache del suo sistema operativo.

## Dotfile (`.DS_Store`, `._foto.jpg`, ...)

`filename.starts_with('.')` (nessuna dipendenza da
`keeppix_media::walk::is_excluded_name`, che resta privata) accetta il
file (mai un `403`/errore inatteso per il client), lo salva sul disco al
posto finale con un `rename()` diretto (`put_dotfile`), ma **salta del
tutto** `ingest_direct`/`enqueue_indexing`: nessuna riga `assets`, nessun
job. Risposta `204 No Content`. Test:
`put_of_dotfile_saves_on_disk_but_does_not_index` — verifica che il numero
di job `extract_metadata` in coda non cambi rispetto a prima del `PUT` (non
un conteggio assoluto a zero, perché la fixture di test ne crea già alcuni
per gli asset esistenti).

## `MOVE` — scoperta importante sul brief

Il brief afferma: "Sposta anche la directory su disco (già fatto da
`move_subtree` che chiama `rename()`)". **Falso**, verificato leggendo
`folders.rs` per intero: `move_subtree` aggiorna solo `folders.path`
(`ltree`) nel database sotto un lock a livello di libreria
(`SELECT ... FOR UPDATE` su `libraries`), zero chiamate a `rename()` o a
qualunque funzione di `std::fs`/`tokio::fs`. Senza aggiungere lo
spostamento fisico nel handler, la directory sarebbe rimasta al vecchio
path sul disco mentre il database la considerava già altrove — la
directory sarebbe apparsa vuota ai client `WebDAV` (che risolvono i
percorsi tramite il database) ma i file sarebbero comunque stati
raggiungibili solo a un percorso ormai "sbagliato" per lo scanner.

Fix: `write::move_folder` legge `absolute_path` **prima** e **dopo**
`move_subtree`, e fa `tokio::fs::rename(old, new)` se i due percorsi
differiscono, dopo il commit della transazione di database. Rischio
residuo documentato nel ledger: se il `rename()` fisico fallisse dopo che
`move_subtree` ha già commesso (solo un vero errore di I/O, perché tutte
le validazioni — ciclo, libreria diversa, collisione di nome — sono già
avvenute dentro `move_subtree`), risulterebbe un'inconsistenza
database/disco da correggere a mano. È la stessa lacuna, non introdotta
qui, già presente in `PATCH /api/v1/folders/{id}` (che oggi non sposta la
directory sul disco per niente) — il `MOVE` `WebDAV` è quindi già più
corretto dell'endpoint REST esistente, non meno.

Permesso: verificato esplicitamente su **entrambe** le cartelle
(`assert_editor` su `src_id` e su `dst_parent_id`) nel handler, perché
`move_subtree` da sola chiama solo `visible()` (non `effective_role`) sul
genitore di destinazione — un viewer con sola visibilità sulla
destinazione avrebbe altrimenti potuto spostarvi dentro cartelle di
editor.

## TDD — cosa ho davvero osservato

1. Scritto `tests/webdav_write.rs` per primo, con i 7 test richiesti dal
   brief, contro un `dav::write` che non esisteva ancora e un
   `dav::handler` che rispondeva ancora `501` per `PUT`/`MKCOL`/`MOVE`.
2. Prima esecuzione: fallimento di **compilazione** (modulo `write`
   inesistente, `Resource::FolderChild` inesistente) — atteso, non un
   fallimento di asserzione. Aggiunte le firme minime per far compilare,
   poi eseguito di nuovo: **7/7 falliti a runtime**, tutti per il motivo
   giusto (`501` invece di `201`/`204`, o un panic sull'assenza
   dell'header `Location` atteso).
3. Implementata la logica reale (`write.rs`, `assert_editor`,
   `ingest_direct`, il dispatch in `mod.rs`). Rieseguito: 6/7 verdi al
   primo giro, 2 fallimenti genuini scoperti dai test stessi (non difetti
   nei test):
   - `put_of_dotfile_saves_on_disk_but_does_not_index` falliva perché
     asseriva zero job in coda, ma la fixture ne crea già 6 per gli asset
     preesistenti — corretto confrontando il conteggio prima/dopo il
     `PUT` invece di un valore assoluto.
   - `move_folder_changes_its_parent` falliva perché asseriva anche lo
     spostamento fisico della directory, che `move_subtree` non fa (vedi
     sopra) — corretto aggiungendo il `rename()` fisico nel handler.
4. Rieseguito dopo entrambe le correzioni: **7/7 verdi**.

### Mutazione deliberata (prima del commit, mai committata)

- **Collisione di nome su `PUT`**: temporaneamente commentato il ramo
  `CollisionOutcome::SkippedDuplicate` in `ingest_direct` così da
  restituire sempre `Created` anche per un duplicato esatto. Risultato:
  `put_of_same_content_skips_without_creating_a_second_file` è fallito
  con un secondo file effettivamente creato sul disco e una seconda riga
  `assets` — la prova che il test cattura davvero l'invariante "mai un
  duplicato silenzioso". Ripristinato subito dopo.
- **Dotfile indicizzato per errore**: temporaneamente rimossa la
  condizione `filename.starts_with('.')` in `put()`, così un dotfile
  seguiva lo stesso percorso di un asset normale. Risultato:
  `put_of_dotfile_saves_on_disk_but_does_not_index` è fallito perché il
  conteggio dei job `extract_metadata` cresceva di 1 — la prova che il
  test cattura davvero l'assenza di indicizzazione per i dotfile.
  Ripristinato subito dopo.
- **Permesso mancante sul genitore di destinazione in `MOVE`**:
  temporaneamente rimossa la seconda chiamata
  `folder_repo.assert_editor(ctx, dst_parent_id)` in `move_folder`,
  lasciando solo il controllo su `src_id`. Non esiste un test dedicato nel
  brief per questo scenario (nessuno dei 7 test lo esercita direttamente,
  perché il brief non lo richiede esplicitamente come caso di test), quindi
  ho verificato manualmente con una chiamata diretta che senza il secondo
  controllo un viewer sulla destinazione (ma non editor) avrebbe superato
  `move_folder` fino a fallire solo dentro `move_subtree` per un motivo
  diverso — confermando che il controllo aggiunto nel handler è
  l'unico a bloccare correttamente questo caso con `403` invece di un
  errore di altro tipo o nessun errore. Ripristinato subito dopo. **Nota
  per un task futuro**: questo scenario meriterebbe un test dedicato
  esplicito (`move_by_editor_of_source_but_not_of_destination_returns_403`),
  differito perché non richiesto dal brief.

Nessuna delle mutazioni è mai stata committata: verificato con `git diff`
prima del commit finale.

## Verifica — output osservato

```
$ cargo fmt --check
(nessun output, exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.46s
(nessun warning/errore — cache calda dalla sessione precedente sullo
stesso branch, verificato che non ci fossero modifiche non compilate)

$ cargo test -p keeppix-api --test webdav_write -- --test-threads=1
running 7 tests
test mkcol_by_a_viewer_returns_403 ... ok
test mkcol_creates_a_subfolder_and_returns_201 ... ok
test move_folder_changes_its_parent ... ok
test put_creates_an_asset_and_enqueues_high_priority_indexing ... ok
test put_of_dotfile_saves_on_disk_but_does_not_index ... ok
test put_of_same_content_skips_without_creating_a_second_file ... ok
test put_of_same_name_different_content_saves_with_suffix ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.75s

$ cargo test -p keeppix-api -- --test-threads=1
(36 binari fra tests/*.rs + gli unit test di src/lib.rs + doc-test)
... [ogni binario: test result: ok, 0 failed] ...
test result: ok. 22 passed; 0 failed ...   <- unittests src/lib.rs
Doc-tests keeppix_api: 0 test, ok
```

Rieseguita l'intera suite una seconda volta con `grep -E "^test result:|
FAILED|error\[|panicked"` su tutto l'output: 36 righe `test result: ok. N
passed; 0 failed`, zero occorrenze di `FAILED`/`error[`/`panicked`. Nessun
test preesistente rotto da questo task.

Non ho eseguito `./scripts/test.sh` per lo stesso motivo dei task
precedenti della fase (5, 6): questo task tocca solo `keeppix-api` e
`keeppix-db` (nessuna migrazione, nessuna query nuova fuori da
`keeppix-db`), e `cargo clippy --workspace --all-targets` (che compila
l'intero workspace, `keeppix-server` compreso) più la suite completa di
`keeppix-api` coprono lo stesso perimetro senza il costo di un `cargo
clean` finale imposto da `./scripts/test.sh`. Non ho toccato `frontend/`:
nessuna build Vite necessaria per questo task.

## Self-review sugli invarianti di `AGENTS.md`

- **Nessun SQL fuori da `keeppix-db`**: `dav/write.rs` chiama solo
  `FolderRepo`/`AssetRepo` (via `assert_editor`, `ensure_child`,
  `absolute_path`, `move_subtree`, `ingest_direct`) — zero query dirette
  nell'handler.
- **Ogni metodo di repository che legge dati di un utente prende un
  `AuthContext`**: `assert_editor`, `ingest_direct` lo prendono come primo
  parametro, coerentemente con `move_subtree`/`ensure_child`/
  `absolute_path` già esistenti.
- **Un utente che sonda un id che non gli appartiene riceve `Forbidden`,
  mai `NotFound`**: `assert_editor` risolve tramite `self.visible(...)` (già
  `Forbidden`-mai-`NotFound` per costruzione) e poi aggiunge solo un ramo
  `Forbidden` in più per il non-editor — mai un `NotFound` nuovo introdotto.
- **Query sempre parametrizzate**: nessuna query nuova in questo task
  usa concatenazione di stringhe; `ingest_direct` riusa `unique_suffixed_name`
  /`map_unique_violation` già parametrizzati.
- **Nessun `unwrap()`/`expect()` in codice di produzione**: verificato a
  mano in `write.rs`, `assert_editor`, `ingest_direct` — zero occorrenze
  fuori dal modulo test (`#[allow(clippy::unwrap_used, clippy::expect_used)]`
  a livello di modulo su `webdav_write.rs`, come convenzione del progetto).
- **`sqlx` solo in forma funzione**: `ingest_direct` usa `sqlx::query`/
  `sqlx::query_as`, mai `query!`.
- **Mai sovrascrittura silenziosa su `PUT`**: verificato con la mutazione
  deliberata sopra.
- **`.fallback(...)` prima di `with_common_layers(...)`**: non toccato in
  questo task (nessuna nuova registrazione di router al di fuori del
  dispatch interno di `dav::handler`, che resta un singolo handler già
  montato).

## Decisioni riportate nel ledger (`progress.md`)

- `COPY` non implementata, stub `501` (motivo sopra).
- Permesso editor verificato su entrambe le cartelle in `MOVE`, non
  delegato del tutto a `move_subtree`.
- Correzione del brief su `MOVE`: `move_subtree` non sposta la directory
  sul disco, aggiunto nel handler.
- `MKCOL` idempotente (niente `405` su un nome già esistente).
- Dotfile: sovrascrittura ammessa, mai indicizzato.

## Fix round (2 Important dalla review)

**Esito**: DONE. Commit: `e78c9eac30ffc7bf84266784487c1d2a26e906fb`
(`fix(api): WebDAV PUT disk-space guard and MKCOL directory-before-db
ordering`), su `fase-5`, pushato.

### Important #1 — `PUT` senza guardia su spazio disco e dimensione body

Problema segnalato: `write_body_to_file` leggeva il corpo fino a EOF senza
alcun limite, e `ensure_disk_space` (`crates/keeppix-db/src/uploads.rs`,
già usata da `UploadSessionRepo::create` per la sessione `tus`) non veniva
mai chiamata dal `PUT` `WebDAV`.

Fix:

- `ensure_disk_space` è passata da `fn` privata a **`pub`** (non
  `pub(crate)`: il chiamante, `keeppix_api::dav::write`, è in un altro
  crate del workspace — `pub(crate)` non sarebbe stato visibile lì. Non
  contiene SQL, quindi esportarla non viola "nessun SQL fuori da
  `keeppix-db`" né il divieto di dipendenza `keeppix-media` ↔ `keeppix-db`)
  e ri-esportata da `keeppix_db::lib` insieme al resto del modulo
  `uploads`. Nessuna duplicazione della logica di `statvfs`.
- `dav::write::put` estrae ora `content_length: Option<u64>` (il
  dispatcher in `dav::mod.rs` legge l'header `Content-Length` prima di
  consumare il corpo) e, se presente, chiama `ensure_disk_space(&library
  .root_path, content_length)` **prima** di creare il file temporaneo —
  stesso principio della sessione `tus`: rifiutato alla porta, non scoperto
  a metà scrittura. Un `DbError::InsufficientStorage` diventa `507` via
  `From<DbError> for Problem` già esistente.
- Nuova costante `MAX_BODY_BYTES = 10 GiB` in `write.rs` e helper puro
  `check_declared_size(content_length) -> Result<u64, Problem>`: `413` se
  `Content-Length` supera il tetto, altrimenti restituisce il tetto da
  imporre allo streaming (la dimensione dichiarata se presente e nel
  limite, altrimenti `MAX_BODY_BYTES` per un corpo senza `Content-Length`,
  tipicamente `Transfer-Encoding: chunked`).
- `write_body_to_file` prende ora un parametro `max_len: u64` e lo impone
  byte per byte durante lo streaming (non solo una volta sull'header
  dichiarato) — un client che dichiara un corpo piccolo ma ne manda uno
  più grande, o che non dichiara affatto una dimensione, viene troncato
  con `413` comunque, sullo stile di `write_body_capped` in
  `routes/share.rs`.

Ruling (ledger): senza un tetto che valga anche in assenza di
`Content-Length`, un client `chunked` avrebbe bypassato sia il controllo
sulla dimensione dichiarata sia `ensure_disk_space` (che ha bisogno di un
numero per girare), riempiendo il disco in streaming senza che nessun
controllo lo intercettasse. 10 GiB è ben oltre qualunque file
fotografico/video reale indicizzato da Keeppix oggi; costo se sbagliato:
un client che carica legittimamente un file più grande riceve `413`
invece di un upload riuscito — da rivedere se un giorno servirà per
RAW/video enormi.

### Important #2 — `MKCOL` commit DB prima della directory su disco

Problema segnalato: `mkcol` chiamava `FolderRepo::ensure_child` (INSERT +
commit immediato) e solo dopo creava la directory con
`tokio::fs::create_dir_all`; un fallimento su disco lasciava una riga
`folders` fantasma senza directory corrispondente.

Fix: ordine invertito.

1. `folder_repo.absolute_path(ctx, parent_id)` per il path del genitore
   (già esistente, nessuna riga nuova serve per risolverlo), poi
   `.join(new_name)` per il path target — **senza** aver ancora creato
   nulla nel database.
2. `tokio::fs::metadata(&target_dir).await.is_ok()` registra se la
   directory esisteva già (per non cancellarla poi per errore, vedi punto
   4).
3. `tokio::fs::create_dir_all(&target_dir)`: se fallisce, `500` **senza
   aver toccato il database per niente**.
4. Solo se il passo 3 riesce, `folder_repo.ensure_child(&parent,
   new_name)` (INSERT, idempotente per costruzione). Se l'`INSERT`
   fallisce e la directory **non** esisteva già prima di questa chiamata,
   `tokio::fs::remove_dir(&target_dir)` best-effort (silenzioso se non
   vuota o già sparita) — se invece esisteva già (secondo `MKCOL`
   idempotente, o una directory lasciata da uno scanner), non viene
   toccata: non è nostra da cancellare.

Ruling (ledger): la pulizia `remove_dir` è condizionata a "l'abbiamo
creata noi in questa chiamata", non incondizionata — altrimenti un
secondo `MKCOL` idempotente su una cartella già esistente, seguito da un
fallimento imprevisto dell'`INSERT` (es. connessione al database persa a
metà), avrebbe cancellato una directory legittima con contenuto reale.
Costo se la ruling fosse sbagliata: nessuna directory fantasma da
cancellare in più, solo un residuo su disco senza riga DB nel caso raro di
un `INSERT` fallito su una directory appena creata da noi che poi
`remove_dir` non riesce a togliere perché non vuota — scenario già
impossibile perché la directory è appena creata e quindi vuota per
costruzione.

### File toccati

- `crates/keeppix-api/src/dav/write.rs` — `MAX_BODY_BYTES`,
  `check_declared_size`, `put` (guardia + `content_length`),
  `write_body_to_file` (parametro `max_len`), `mkcol` (ordine invertito).
- `crates/keeppix-api/src/dav/mod.rs` — il dispatcher `PUT` estrae
  `Content-Length` dagli header e lo passa a `write::put`.
- `crates/keeppix-db/src/uploads.rs` — `ensure_disk_space` da `fn`
  privata a `pub fn`, con doc `# Errors` (richiesta da
  `clippy::missing_errors_doc` su una funzione ora pubblica).
- `crates/keeppix-db/src/lib.rs` — `ensure_disk_space` aggiunta alla
  ri-esportazione del modulo `uploads`.
- `crates/keeppix-api/tests/webdav_write.rs` — 2 test nuovi (vedi sotto).

### TDD / test nuovi

- 4 unit test in `dav::write::tests` (nessun database, puri su
  `check_declared_size`): dimensione dichiarata nel limite, sopra il
  limite (`413`), esattamente al limite (accettata), assente (tetto =
  `MAX_BODY_BYTES`). Eseguiti *prima* dell'implementazione (compilazione
  falliva, `check_declared_size` non esisteva) e poi verdi dopo aver
  scritto la funzione minima.
- `put_with_a_declared_content_length_over_the_limit_returns_413`
  (`webdav_write.rs`): un `PUT` con corpo reale minuscolo (`tiny.jpg`) ma
  header `Content-Length: 100 TiB` dichiarato a mano — confermato con un
  esperimento isolato (`reqwest` + un server TCP che stampa la richiesta
  grezza) che `reqwest`/`hyper` inviano per davvero l'header impostato a
  mano sul filo, senza ricalcolarlo sulla dimensione reale del corpo
  passato a `.body(...)` — quindi il test esercita davvero il percorso
  "il client dichiara più di quanto manda". Verificato che il file non
  viene scritto su disco e che non viene creata alcuna riga `assets`.
  Osservato fallire prima del fix (il vecchio `put` non aveva il
  parametro `content_length` — errore di compilazione, poi con una firma
  placeholder: `200`/`201` invece di `413`).
- `mkcol_disk_failure_leaves_no_phantom_folder_row` (`webdav_write.rs`):
  crea un **file** omonimo sul disco al posto della cartella target prima
  di chiamare `MKCOL`, così `create_dir_all` fallisce con un errore di I/O
  reale (`AlreadyExists` su un percorso che non è una directory) —
  deterministico e indipendente dai permessi Unix (i test possono girare
  come root, che li bypassa). Verifica `500` e **zero righe** in
  `folders` per quel nome. Osservato fallire prima del fix: con
  `ensure_child` chiamato per primo, la riga in `folders` compariva
  comunque (l'`INSERT` riesce, solo la `create_dir_all` fallisce dopo).

### Output di verifica

```
$ cargo fmt --check
(nessun output, exit 0)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.55s
(nessun warning/errore)

$ cargo test -p keeppix-api --test webdav_write -- --test-threads=1
running 9 tests
test mkcol_by_a_viewer_returns_403 ... ok
test mkcol_creates_a_subfolder_and_returns_201 ... ok
test mkcol_disk_failure_leaves_no_phantom_folder_row ... ok
test move_folder_changes_its_parent ... ok
test put_creates_an_asset_and_enqueues_high_priority_indexing ... ok
test put_of_dotfile_saves_on_disk_but_does_not_index ... ok
test put_of_same_content_skips_without_creating_a_second_file ... ok
test put_of_same_name_different_content_saves_with_suffix ... ok
test put_with_a_declared_content_length_over_the_limit_returns_413 ... ok
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test -p keeppix-api --lib -- --test-threads=1
running 26 tests
...
test dav::write::tests::a_declared_size_exactly_at_the_limit_is_accepted ... ok
test dav::write::tests::a_declared_size_over_the_limit_is_rejected_with_413 ... ok
test dav::write::tests::a_declared_size_within_the_limit_becomes_the_streaming_cap ... ok
test dav::write::tests::no_content_length_caps_streaming_at_max_body_bytes ... ok
...
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo test -p keeppix-api -- --test-threads=1
(tutti i binari tests/*.rs + unit test di src/lib.rs + doc-test)
... [ogni binario: test result: ok, 0 failed] ...
test result: ok. 26 passed; 0 failed ...   <- unittests src/lib.rs
Doc-tests keeppix_api: 0 test, ok

$ cargo test -p keeppix-db -- --test-threads=1
(tutti i binari tests/*.rs, inclusi uploads.rs con
insufficient_disk_space_is_rejected_at_creation_not_mid_upload)
... [ogni binario: test result: ok, 0 failed] ...
```

Nessuna regressione: tutti i test preesistenti di `keeppix-api` e
`keeppix-db` restano verdi. `cargo check -p keeppix-server -p keeppix-dav
--all-targets` verificato pulito (la nuova firma di `ensure_disk_space` e
i cambi in `dav/write.rs`/`dav/mod.rs` non toccano superfici usate da
`keeppix-server`).
