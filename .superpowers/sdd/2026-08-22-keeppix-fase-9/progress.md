# Fase 9 — Organizzazione: culling a cartelle, spostamento sicuro, rinomina

Piano: `docs/superpowers/plans/2026-08-20-keeppix-fase-9.md`
Spec: `docs/superpowers/specs/fase-9-organizzazione.md`
Branch: `fase-9`, da `main` post-merge Fase 8 + fix CTE semantico (`4ebafca`).

> Fase che tocca file veri sul disco dell'utente. Fermarsi prima della prima
> rinomina/spostamento reale su file di produzione (dati di test liberi).
> Riepilogo di sicurezza filesystem obbligatorio prima di chiudere la fase.

## Pre-volo — verifica del piano contro il codice reale (prima di scrivere codice)

Il piano e la spec sono stati riletti per intero. Verifica mirata (subagent di
esplorazione, non fidandosi delle affermazioni del piano) di ogni pezzo di
codice esistente che i Task 1+ citano, con questi scostamenti reali trovati:

Ruling: **`libraries.culling_root_folder_id` esiste già** (migrazione
`0044_culling_root_folder.sql`, commento nella migrazione stessa: *"Fase 9
riusa questa colonna (non la ricrea)"*) — il piano/spec §2.7 la presentano
come se fosse nuova (`ALTER TABLE libraries ADD COLUMN
culling_root_folder_id ...`). Già letta (non scritta) da `embeddings.rs` e
`faces.rs` per escludere il sottoalbero di culling dall'IA. Task 2 di questa
fase **riusa** la colonna, non la ricrea, e aggiunge solo `folders.culling_role`
(confermato assente ovunque). — Costo se sbagliato: una migrazione che tenta
`ADD COLUMN` su una colonna già esistente fallisce a runtime, scoperto solo
in CI.

Ruling: **`OperationKind` ha già tre varianti** (`LibraryScan`, `AiAnalysis`
da Fase 7, `FaceDetection` da Fase 8), non una sola come dichiara il piano
(scritto prima che quelle due fasi esistessero). Task 10 aggiunge una quarta
variante (es. `BulkMove`/`BulkRename`) alle due `match` esistenti in
`crates/keeppix-domain/src/operation.rs`, non alla prima mai scritta. — Costo
se sbagliato: nessuno, è solo un'assunzione del piano da correggere, non un
comportamento da cambiare.

Ruling: **`moves.rs::after_hash` vive in `crates/keeppix-jobs/src/moves.rs`,
non `crates/keeppix-db/src/moves.rs`** (quel file nel crate db non esiste).
Il comportamento descritto dal piano (copia solo EXIF via `copy_exif`, marca
la riga vecchia `offline`, non tocca `asset_flags`/`asset_overrides`) è
confermato esatto — solo il percorso file è sbagliato nel piano. Irrilevante
per `move_asset` comunque: quella funzione aggiorna la riga **esistente**
(stesso `asset_id`), quindi `asset_flags`/`asset_overrides` (chiavi esterne
su `asset_id`, mai su `folder_id`/`filename`) restano collegati senza
copiare nulla — il problema che `after_hash` risolve (riga nuova + riga
vecchia orfana) è strutturalmente il problema che `move_asset` **non ha**,
per costruzione.

Ruling: **`assert_can_edit_assets` vive in `crates/keeppix-db/src/permissions.rs`
su `PermissionRepo`, non in `folders.rs`** — il piano non specifica il file,
solo il comportamento (corretto: risolve la cartella *corrente* di un asset
via join `assets→folders→libraries`, nessuna nozione di cartella di
destinazione). Confermato: `move_asset` deve chiamare
`FolderRepo::assert_editor` due volte (partenza e destinazione), stesso
pattern di `dav/write.rs::move_folder` per le cartelle — non
`assert_can_edit_assets`.

Ruling: **`FailureReason` (`crates/keeppix-api/src/bulk.rs`) non ha una
variante `Collision`** — confermato come dichiara il piano. Le cinque
esistenti (`Unreachable`/`PermissionDenied`/`FileMissing`/`Timeout`/`Unknown`)
mappano oggi una collisione di nome su `Unknown` via `from_db_error`. Va
aggiunta prima che il Task 10 (operazioni di massa) possa riportarla
distintamente all'utente — Task 1 stesso.

Prossima migrazione libera: `0048` (`0047_face_scans.sql` è l'ultima
esistente).

## Gruppo A — La primitiva

### Task 1 — `AssetRepo::move_asset`

`crates/keeppix-db/src/assets.rs`: `move_asset(ctx, asset_id, new_folder_id,
new_filename: AssetName) -> Result<Asset, DbError>`. Permesso via
`FolderRepo::assert_editor` chiamato due volte (partenza e destinazione),
non `PermissionRepo::assert_can_edit_assets` (risolve solo la cartella
corrente via join, nessuna nozione di destinazione — confermato leggendo
`permissions.rs` prima di scrivere codice, non assunto dal piano).
Collisione verificata sia via `SELECT` prima di toccare il filesystem sia
via il vincolo `UNIQUE` reale al momento dell'`UPDATE` (difesa in profondità
contro la finestra fra le due), entrambe mappate su `DbError::Collision`
(nuova variante, `crates/keeppix-db/src/error.rs`) — non il generico
`Conflict` già usato da `crate::uploads::map_unique_violation` per la stessa
collisione in `ingest_direct`, apposta per lasciare che le operazioni di
massa la distinguano (`FailureReason::Collision`,
`crates/keeppix-api/src/bulk.rs`, e mappatura HTTP `409` in
`crates/keeppix-api/src/problem.rs`).

Ruling: **ordine deliberatamente invertito rispetto a `TrashRepo::choose`
(`trash.rs`)** — qui il file fisico si sposta **prima**, la riga **dopo**;
lì la riga si aggiorna prima e il `rename()` segue, con un commento che
spiega esplicitamente perché (un file orfano nel cestino è più confuso di
una riga "trashed" che punta ancora al percorso vecchio). La scelta
opposta per `move_asset` non è un'incoerenza fra le due funzioni: un asset
spostato da questa funzione resta visibile ovunque nell'app (timeline,
ricerca, album), quindi una riga che punta a un percorso inesistente
sarebbe silenziosa e invisibile lì — mentre un file fisico "in più" senza
riga corrispondente lo ritrova la prossima scansione (reindicizzato come
asset nuovo, perde `asset_flags`/`asset_overrides` solo in **questo**
scenario di fallimento a metà, mai nel percorso normale). È esattamente il
ruling che il piano di fase dichiara esplicitamente per questo task — non
una mia preferenza contro la convenzione già in uso, ma la convenzione
giusta per **questa** funzione specifica, con la spiegazione di entrambe
lasciata nel commento del codice perché un lettore futuro non la scambi
per una svista. — Costo se sbagliato (cioè se in pratica l'ordine
file-poi-riga si rivelasse peggiore): un fallimento a metà lascia un
asset "fantasma" temporaneo (file spostato, riga vecchia) finché la
prossima scansione non lo ripara — recuperabile, mai perdita permanente.

Ruling: **il sidecar `.xmp` si sposta *best-effort*, non bloccante.** — Se
il file principale si sposta ma il sidecar fallisce (permessi, spazio),
`move_asset` non annulla né fallisce: logga un avviso e prosegue. Motivo:
il sidecar è un **export** derivato da `asset_overrides`/`asset_flags`
(`OverridesRepo::pending_sidecars`/`mark_sidecar_written`), non la fonte di
verità — il prossimo giro dello sweep dei sidecar lo riscrive da zero alla
posizione corretta quando `asset_overrides` cambia di nuovo. — Costo se
sbagliato: un `.xmp` orfano al vecchio percorso finché qualcosa non tocca
di nuovo gli override di quell'asset o qualcuno lo pulisce a mano — mai
perdita del dato vero, solo dell'export cache.

Ruling: **`check-wired.py` segnala verde `move_asset` per un falso
negativo dello strumento, non perché sia davvero collegato.**
`count_ident` conta le occorrenze testuali del nome in **tutti** i file di
produzione, commenti di documentazione inclusi — non solo le chiamate
reali — e i miei stessi commenti su `move_asset` (in `assets.rs` ed
`error.rs`) bastano a farlo risultare "wired" senza che esista un solo
chiamante di produzione. Verificato con `grep -rn '\.move_asset(' crates`:
sei occorrenze, tutte nei test di questo task, zero in codice di
produzione. Aggiunta comunque l'eccezione esplicita in
`scripts/wired-exceptions.txt` (`fn move_asset fase-9`, rinvio ai Task 4/8
della stessa fase) invece di fidarsi del pass accidentale dello strumento —
esattamente il tipo di lacuna che questo progetto ha già pagato cinque
volte (`docs/CONTINUE.md`, "La lezione che questo progetto ha pagato
cinque volte"). — Costo se sbagliato: nessuno per ora (l'eccezione è più
onesta del pass silenzioso, non meno); da togliere quando Task 4/8
collegano davvero la funzione.

Verifica eseguita (locale, `KEEPPIX_TEST_DATABASE_URL` verso Postgres 16 +
pgvector, non testcontainers):
- `cargo check -p keeppix-db` → pulito.
- `cargo fmt --check -p keeppix-db` → pulito.
- `cargo clippy -p keeppix-db --all-targets -- -D warnings` → pulito.
- `cargo fmt --check -p keeppix-api` (solo formattazione: `keeppix-api` non
  compila in locale in questa sessione, download dei binari `ort` bloccato
  dal proxy dell'ambiente — limite noto, non introdotto da questo task) →
  pulito su `bulk.rs`/`problem.rs`.
- `cargo test -p keeppix-db --test assets` → 24/24 verdi, incluso il nuovo
  modulo `move_asset` (6 test: id/riga/file coerenti dopo lo spostamento,
  `asset_flags` preservati, collisione rifiutata senza toccare né il file
  di partenza né quello di destinazione, `Forbidden` quando l'editor ha
  diritti solo sulla cartella di partenza, sidecar `.xmp` spostato insieme
  al file, no-op quando destinazione e sorgente coincidono).
- `cargo test -p keeppix-db --test folders --test permissions --test trash`
  → 17+22+12 verdi (nessuna regressione sulle funzioni riusate:
  `assert_editor`, `PermissionRepo::grant`, `TrashRepo`).
- `python3 scripts/check-wired.py` → verde (con l'eccezione esplicita sopra).

Task 1: complete. `FailureReason::Collision` (Task 1 richiesto dal piano
prima del Task 10) già aggiunta insieme, non rimandata.

## Gruppo B — Il culling a cartelle

### Task 2 — Cartella radice e ruoli

Migrazione `0048_culling_role.sql`: `folders.culling_role text CHECK (IN
('taken','skipped'))` + indice parziale. `libraries.culling_root_folder_id`
**non ricreata** — esiste già dalla 0044 (Fase 7 Task 5), riusata; il suo
commento stesso lo anticipava ("Fase 9 riusa questa colonna, non la
ricrea").

`keeppix_domain::CullingRole` (`Taken`/`Skipped`, `as_str`), campo
`Folder.culling_role: Option<CullingRole>` e
`Library.culling_root_folder_id: Option<FolderId>` — quest'ultimo non era
mai stato esposto sul dominio prima d'ora (letto solo via JOIN grezzo in
`embeddings.rs`/`faces.rs` per l'esclusione IA).

`FolderRepo::ensure_culling_child(parent, role)` — come `ensure_child`, ma
marca `_taken`/`_skipped` col ruolo giusto invece che lasciarlo `NULL`.

Ruling: **auto-guarigione se la cartella esiste già senza ruolo.** — Se un
utente ha già una cartella chiamata `_taken` per conto suo dentro un lotto
di culling (prima di usare mai la funzione), `ON CONFLICT DO NOTHING`
lascerebbe quella riga col ruolo `NULL` per sempre — esattamente il difetto
che "il ruolo è una colonna, non il nome" vuole evitare, capovolto: una
cartella che *dovrebbe* essere speciale che non lo diventa mai. Un secondo
`UPDATE ... WHERE culling_role IS NULL` dopo l'`INSERT` ignorato la marca
comunque. — Costo se sbagliato: nessuno nuovo, è un miglioramento rispetto
a lasciare la lacuna; verificato con un test dedicato
(`ensure_culling_child_heals_a_role_missing_folder`).

`LibraryRepo::set_culling_root(ctx, id, folder_id: Option<FolderId>)` —
**non** un parametro aggiunto al generico `update()` (che oggi è aperto a
chiunque veda la libreria, non solo al proprietario — verificato leggendo
`routes/libraries.rs::patch`, nessun controllo di ownership oltre
`find_by_id`). Metodo dedicato con **owner-o-admin esplicito**, perché la
radice del culling decide dove finiscono fisicamente le foto scelte/
scartate e cosa l'IA esclude, non è una preferenza di visualizzazione come
`scan_enabled`/`faces_enabled`. Valida anche che la cartella designata
appartenga alla stessa libreria (`Conflict` altrimenti) — spec §2.6 non lo
dice esplicitamente ma è l'unica lettura sensata di "una cartella già
esistente" nel contesto di "questa libreria".

Ruling: **`set_culling_root` accetta solo "imposta" (`Some`) e "rimuovi"
(`None`) — non un terzo stato "non toccare".** A differenza di `update()`
(dove `None` sui suoi parametri significa "lascia invariato", pattern
`COALESCE`), qui `None` significa esplicitamente "nessuna radice": la
spec §2.6 descrive solo l'impostazione, non un caso d'uso per lasciare
invariato *questo specifico campo* mentre si cambia altro sulla libreria —
e comunque non condivide la stessa chiamata di `update()`, quindi non c'è
ambiguità da risolvere. — Costo se sbagliato: nessuno finché nessun
chiamante ha bisogno di un "non toccare" a metà di un'altra scrittura;
aggiungere allora, non ora, un `Option<Option<FolderId>>` più esplicito.

Route HTTP non wired in questo task: la designazione della radice arriva
dalle impostazioni della libreria in Fase 11 (schermata dedicata) — coerente
col piano, che per il Task 2 elenca solo lo schema, non l'endpoint.

Verifica eseguita (stesso ambiente locale di Task 1):
- `cargo check -p keeppix-domain -p keeppix-db` → pulito.
- `cargo fmt --check -p keeppix-db -p keeppix-domain` → pulito.
- `cargo clippy -p keeppix-db -p keeppix-domain --all-targets -- -D warnings` → pulito.
- `cargo test -p keeppix-db --test folders` → 22/22 verdi (16 esistenti +
  6 nuovi: `_taken`/`_skipped` marcate correttamente, idempotenza,
  auto-guarigione, owner può designare e rimuovere, un editor non-owner
  respinto con `Forbidden`, una cartella di un'altra libreria respinta con
  `Conflict`).
- `cargo test -p keeppix-db --test libraries` → 16/16 verdi (nessuna
  regressione: `Library` con un campo in più non rompe `into_domain` né i
  costruttori letterali altrove — verificato con `grep` che nessun altro
  crate costruisce `Folder{}`/`Library{}` per valore, solo via i repo).
- `python3 scripts/check-wired.py` → rosso al primo giro (correttamente,
  stavolta — nessun falso positivo da commenti), verde dopo aver aggiunto
  `ensure_culling_child fase-9` e `set_culling_root fase-9` a
  `wired-exceptions.txt` con i consumatori futuri (Task 4, impostazioni
  Fase 11).

Task 2: complete.

## CI reale ha trovato un difetto nel commit del Task 1

Push del Task 1 (`7524a1e`) su `origin/fase-9`: CI rossa sul job `backend`,
step `Lint` (`cargo clippy --workspace --all-targets -- -D warnings`) —
esattamente il gate che `keeppix-api` non potevo verificare in locale in
questa sessione (download dei binari `ort` bloccato dal proxy). Confermato
il rischio che avevo segnalato esplicitamente nel commit del Task 1 invece
di ignorarlo.

`clippy::match_same_arms` su `detail_for` (`crates/keeppix-api/src/bulk.rs`):
il mio nuovo braccio `DbError::Collision(message) => Some(message.clone())`
aveva lo stesso corpo del braccio preesistente
`DbError::Migration(message) => Some(message.clone())`, separato da esso —
clippy chiede di unirli nello stesso pattern `|`. Fix meccanico, nessun
cambio di comportamento: `Migration` unito al gruppo
`Io|Conflict|Corrupted|Collision`. Verificato solo con `cargo fmt --check
-p keeppix-api` (sintassi) in questa sessione — la vera verifica è la CI
sul prossimo push, non dichiarata chiusa qui prima di vederla verde.

## Sblocco: `keeppix-api`/`keeppix-media`/`keeppix-jobs` compilano in locale

Il commit del fix `Lint` sopra è passato in CI (confermato: job `backend`,
step `Lint` verde), ma lo step `Test` successivo è andato rosso su
`openapi_snapshot_matches_the_committed_file` — un secondo difetto reale
che il limite "`keeppix-api` non compila in locale" non poteva far vedere
prima del push, esattamente come temuto.

A quel punto, invece di continuare a fidarmi ciecamente di CI per ogni
`keeppix-api`/`keeppix-media`/`keeppix-jobs`, ho cercato uno sblocco reale:
`/root/ort-lib/libonnxruntime.so*` era già presente nell'ambiente (non
scaricato da me). Il build script di `ort-sys` (`build/main.rs`,
`build/vars.rs`) legge `ORT_LIB_PATH`/`ORT_LIB_LOCATION` per linkare contro
un onnxruntime di sistema invece di scaricare i binari precompilati — non
documentato nel `Cargo.toml` del progetto, trovato leggendo il sorgente
del crate in `~/.cargo/registry/src`. Con:

```
export ORT_LIB_PATH=/root/ort-lib
export ORT_PREFER_DYNAMIC_LINK=1
export LD_LIBRARY_PATH=/root/ort-lib:$LD_LIBRARY_PATH
```

`cargo check -p keeppix-api` compila per la prima volta in questa sessione
(prima falliva sempre al download di `ort-sys`, bloccato dal proxy
dell'ambiente). Verificato poi con l'intera suite dei comandi di CI:

- `cargo fmt --all --check` → pulito.
- `cargo clippy --workspace --all-targets -- -D warnings` → pulito
  (tutti e 7 i crate del workspace, non solo `keeppix-db`).
- `python3 scripts/check-wired.py` → verde.

Ruling: **questo sblocco vale per il resto della sessione** (Fase 9, Fase
11, e i Task A/B modelli IA che toccano `keeppix-media` per davvero) — non
più solo `cargo check -p keeppix-db`, ma i comandi di CI reali sull'intero
workspace prima di ogni push, da ora in poi. — Costo se l'ambiente perde
`/root/ort-lib` in una sessione futura (container ricreato): si torna al
limite precedente, documentato di nuovo, nessun danno.

Con lo sblocco, rigenerato `docs/api/openapi.json`
(`UPDATE_OPENAPI=1 cargo test -p keeppix-api --test openapi
openapi_snapshot_matches_the_committed_file`) e verificato il diff a mano
prima di committarlo, come il test stesso richiede esplicitamente
("non rigenerarlo per far tornare verde il test... guarda che cosa è
cambiato e decidi"): una sola riga, `"collision"` aggiunta all'enum
`FailureReason` nello schema — esattamente e solo l'aggiunta additiva
attesa da `DbError::Collision`/Task 1, coerente col contratto "solo
aggiunte entro `/api/v1`" dichiarato dallo spec OpenAPI stesso. Nessun
altro campo di Fase 9 (`Folder.culling_role`,
`Library.culling_root_folder_id`) compare nello schema — corretto, non
sono ancora esposti da nessuna vista API (debito dichiarato per Task 4/8/
Fase 11, non un'omissione).

Verifica aggiuntiva con lo sblocco (locale, stesso Postgres):
- `cargo test -p keeppix-api --test openapi` → 8/8 verdi (incluso lo
  snapshot rigenerato).
- `cargo test -p keeppix-api --test scan` → 8/8 verdi (operazioni di
  massa/`BulkOutcome`, area più vicina a `bulk.rs` toccato in questa
  fase).
- `cargo test -p keeppix-api --test libraries --test problems` → 14/14 +
  4/4 verdi (`problems.rs` esercita `From<DbError> for Problem`, la
  mappatura che ho modificato per `Collision`).
- `cargo test -p keeppix-api --test trash` → 5/6: un fallimento reale ma
  **non causato da questa sessione** —
  `batch_delete_partial_success_when_the_trash_folder_is_not_writable`
  simula un `EACCES` con `chmod 0o555` su una cartella, ma questo sandbox
  esegue i test come `root` (`whoami` → `root`), che ignora i bit dei
  permessi Unix per costruzione del kernel — la scrittura riesce dove il
  test si aspetta che fallisca. Verificato **prima** di liquidarlo come
  ambientale: `git diff origin/main..HEAD -- crates/keeppix-api/tests/trash.rs
  crates/keeppix-db/src/trash.rs` → zero differenze su entrambi i file in
  tutta questa sessione. Nessun fix necessario né possibile in locale;
  CI gira come utente `runner` non privilegiato su GitHub Actions, dove il
  test funziona come previsto (era già verde prima di questa fase).
