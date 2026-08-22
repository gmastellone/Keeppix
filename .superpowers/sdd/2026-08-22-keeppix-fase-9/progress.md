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

## CI reale verde sul branch `fase-9` (Tasks 1-2)

Run `32588634367` (commit `23cd1b4`, terzo tentativo dopo i due difetti
sopra) — tutti e 5 i job verdi, incluso `backend`/`Test` (l'intera `cargo
test --workspace`, ~25 min — quindi anche
`batch_delete_partial_success_when_the_trash_folder_is_not_writable`
verde per davvero su CI, come previsto: conferma indipendente che il
fallimento locale era solo l'utente `root` di questo sandbox) e
`La specifica OpenAPI è aggiornata`. Tasks 1-2 chiusi: primitiva
`move_asset`, schema/dominio culling root+ruoli, entrambi con CI reale
verde sul branch, non solo test locali.

Non ancora mergiato in `main` — resta sul branch `fase-9` finché la fase
non è completa (§10 PROSEGUI.md si applica a fine fase, non a ogni task).

### Task 3 — I lotti

Nuovo modulo `crates/keeppix-db/src/culling.rs` (`CullingRepo`), non
aggiunto a `folders.rs`/`libraries.rs`: la query attraversa entrambi i
domini (radice designata sta su `libraries`, lotti e ruoli su `folders`),
e i Task 4/5 aggiungeranno spostamento fisico e integrazione con la
ricerca nello stesso file — un posto dedicato invece di far crescere due
repository che non se ne occupano concettualmente.

`CullingLot` (dominio): `folder_id`, `name`, `created_at`, `pending`,
`taken`, `skipped`. `list_lots(ctx, library_id)`:
- vuoto se `libraries.culling_root_folder_id` è `NULL` (spec §2.6: senza
  radice designata, nessun comportamento nuovo forzato);
- altrimenti i figli diretti della radice, più recenti prima, con tre
  sottoquery indipendenti per lotto (non `JOIN` + `COUNT(DISTINCT ..)`:
  con tre `LEFT JOIN` — radice/`_taken`/`_skipped` — il prodotto
  cartesiano fra i tre insiemi di asset avrebbe gonfiato le righe
  intermedie; "economico" nel piano vuol dire per-lotto, non
  "una query sola a ogni costo") che contano solo `assets.status =
  'indexed'` (coerente col resto dell'app: `discovered`/`offline`/
  `error`/`trashed` non sono foto vere agli occhi dell'utente).

Ruling: **ambito owner/admin, non lo scope di visibilità generale delle
cartelle.** `LibraryRepo::find_by_id` (che risolve
`culling_root_folder_id`) è owner-o-admin per costruzione — non l'ho
aggirato con una query diretta per dare visibilità più larga. La spec
descrive il culling come un flusso personale del proprietario (narrazione
in prima persona in tutto il §0, nessuna menzione di condivisione
dell'area con un editor). Verificato con un test dedicato
(`only_the_owner_or_admin_can_list_lots`): un editor con permesso
esplicito sulla cartella radice del culling riceve comunque `Forbidden`.
— Costo se sbagliato: se in futuro emergerà un bisogno reale di condividere
un lotto con un editor (es. un partner di viaggio che aiuta a scegliere),
va deciso allora con un requisito vero davanti, non anticipato ora
indovinando — allargare lo scope è un cambiamento a basso rischio quando
succede, restringerlo dopo che qualcuno ci ha fatto affidamento non lo è.

Verifica eseguita (locale, workspace intero sbloccato da questo punto in poi):
- `cargo check -p keeppix-domain -p keeppix-db` → pulito.
- `cargo fmt --all --check` → pulito su tutto il workspace.
- `cargo clippy --workspace --all-targets -- -D warnings` → pulito su
  tutto il workspace (7 crate, non solo `keeppix-db`).
- `cargo test -p keeppix-db --test culling` → 8/8 verdi (vuoto senza
  radice, conteggi esatti per lotto, un asset non ancora `indexed` non
  conta, `Forbidden` per un editor non-owner, ordine per data decrescente).
- `cargo test -p keeppix-db --test folders --test libraries` → 22/22 +
  16/16 verdi, nessuna regressione.
- `python3 scripts/check-wired.py` → rosso al primo giro su `list_lots`
  (correttamente), verde dopo l'eccezione (`fase-9`, consumatore la
  schermata Culling di Fase 11).

Task 3: complete.

### Task 4 — Scegliere/scartare sposta il file

`CullingRepo::set_pick(ctx, asset_id, pick)` e `CullingRepo::empty_skipped
(ctx, lot_folder_id)`, in `crates/keeppix-db/src/culling.rs` accanto a
`list_lots`.

Ruling: **il permesso non è un cancello unico per l'intera chiamata.**
Fuori da un lotto il flag resta impostabile da chiunque veda l'asset — lo
stesso permesso di oggi (`FlagRepo::set`), invariato dalla spec (§2.6:
"fuori da un lotto la valutazione resta solo un flag, come oggi"). Dentro
un lotto lo spostamento fisico passa da `AssetRepo::move_asset` (Task 1),
che pretende `editor` su entrambe le cartelle per conto proprio: se il
chiamante non lo è, l'intera chiamata fallisce con `Forbidden` **prima**
di toccare il flag — verificato con un test dedicato
(`without_editor_rights_the_call_is_forbidden_and_the_flag_is_untouched`)
che confronta i flag prima/dopo un tentativo fallito e li trova identici.
— Costo se sbagliato (cancello unico troppo largo): un viewer fuori dal
lotto perderebbe la possibilità di flaggare che ha oggi. Troppo stretto:
un editor dentro un lotto non potrebbe più scegliere/scartare, l'intera
feature del Task diventerebbe inutilizzabile per chiunque non sia owner.

Ruling: **come leggere `culling_root_folder_id` senza il cancello
owner/admin di `LibraryRepo::find_by_id`.** `list_lots` (Task 3) lo
risolve passando dalla libreria, owner/admin per costruzione — corretto lì
perché l'intero elenco lotti è owner/admin per la spec. Qui però lo
spostamento fisico deve restare disponibile a un editor condiviso (vedi
sopra), quindi serve un percorso di lettura visibilità-gated, non
owner-gated. Nuovo `FolderRepo::find_with_library(ctx, id) ->
(Folder, Library)`, wrapper pubblico del metodo privato `visible` già
usato da `assert_editor`/`find_by_id` — stesso cancello di sempre, nessuna
query nuova, nessun aggiramento del sistema di permessi. `set_pick` lo usa
sull'asset per ottenere la cartella corrente **e** la libreria in una
sola chiamata, senza mai passare da `LibraryRepo::find_by_id`.

Risoluzione del contesto di culling (`culling_lot_of`, funzione privata):
un asset è "dentro un lotto" se e solo se la sua cartella è un discendente
diretto della radice designata a due livelli esatti — o è essa stessa
figlia della radice (in attesa), o è marcata `culling_role` **e** sua
madre è figlia della radice (già in `_taken`/`_skipped`). Qualunque altra
cartella, compresa la radice stessa, restituisce "fuori dal culling":
deliberatamente conservativo, riconosce solo la struttura esatta che i
Task 2/3 costruiscono, mai per nome.

**Bug trovato e corretto prima di committare, non in CI**: la prima stesura
chiamava `FolderRepo::ensure_culling_child` (solo riga DB, nessuna
directory) per _taken/_skipped e poi `AssetRepo::move_asset`, che fa
`rename()` per davvero e non crea directory di destinazione (verificato
leggendo `move_asset` stesso — nessuna `create_dir_all` al suo interno,
stesso pattern degli altri test `move_asset` di Task 1 che devono creare
la cartella di destinazione a mano prima di chiamarlo). In produzione
`_taken`/`_skipped` sono creati per la prima volta dal culling stesso, non
scoperti da uno scan — quindi la prima scelta dentro un lotto avrebbe
sempre fallito con `DbError::Io` (directory inesistente). Corretto con
`provision_culling_child` (funzione privata): crea la directory sul disco
**prima** della riga — stesso ordine invertito di `dav::write::mkcol`
(Fase 5 Task 7, la stessa identica scelta per lo stesso identico motivo:
un `INSERT` riuscito con una `create_dir_all` fallita dopo lascerebbe una
riga fantasma senza cartella corrispondente, il rovescio silenzioso è
peggio) — con rollback best-effort della sola directory creata da questa
chiamata se l'`INSERT` fallisce. Nessun repository di `keeppix-db` tocca
il filesystem per creare cartelle salvo dove l'operazione è già fisica di
natura (`TrashRepo`, `UploadSessionRepo`) — spostare un file in una
cartella che potrebbe non essere mai esistita prima d'ora è esattamente
quel caso, non un'eccezione alla regola.

Ordine spostamento-poi-flag (non il contrario): un fallimento del
`move_asset` (permesso, collisione di nome, `Io`) lascia il flag intatto
invece di raccontare uno spostamento mai avvenuto.

`empty_skipped` riusa `TrashRepo::choose` con `DiskAction::Purged` invece
di duplicarne la logica — stesso cancello owner/admin ("un editor non può
distruggere file", spec §4.2), stesso ordine riga-poi-file, stesso audit
in `trash_entries`. Verificato con un test dedicato
(`an_editor_who_is_not_the_owner_cannot_purge`) che un editor condiviso
riceve `Forbidden` e il file resta al suo posto.

Verifica eseguita:
- `cargo build -p keeppix-db` → pulito.
- `cargo fmt --all --check` → pulito su tutto il workspace dopo un
  `cargo fmt --all` (differenze solo di formattazione, wrapping di
  espressioni lunghe).
- `cargo clippy --workspace --all-targets -- -D warnings` → pulito su
  tutto il workspace (7 crate).
- `cargo test -p keeppix-db --test culling` → 16/16 verdi (8 di Task 3 +
  8 nuovi: fuori-lotto solo flag, scegli sposta in `_taken`, scarta sposta
  in `_skipped`, cambio idea `_skipped`→`_taken`, annulla torna nel lotto,
  `Forbidden` senza editor con flag intatto, `empty_skipped` purga solo
  gli scartati e lascia intatti gli scelti, `Forbidden` a un editor non
  owner su `empty_skipped`).
- `cargo test -p keeppix-db` (l'intera suite, 7XX+ test su ~50 file,
  liberato spazio disco con `rm -rf target/debug/incremental` — 9.5G,
  l'allowance del sandbox si era esaurita a metà corsa) → verde ovunque
  tranne 2 test pre-esistenti in `pgvector.rs`
  (`persist_pgvector_status_survives_a_reload`,
  `postgis_only_image_reports_vector_unavailable`): richiedono un
  container Docker (`testcontainers`, immagine PostGIS-only) e questo
  sandbox non ha un demone Docker attivo (`/var/run/docker.sock` assente,
  `service docker start` fallisce su `ulimit: Operation not permitted`,
  un vincolo del sandbox stesso). Diff zero su `pgvector.rs`/`pgvector.rs`
  sorgente rispetto a `origin/main` — non una regressione di questo task,
  stessa categoria del difetto locale-solo di `trash.rs` già documentato
  ai Task 1-2 (CI ha un demone Docker vero, questi due passeranno lì).
- `python3 scripts/check-wired.py` → rosso al primo giro su `empty_skipped`
  (correttamente — nessun chiamante ancora). `set_pick` non segnalato dal
  tool nonostante zero chiamate di produzione reali (`grep -rn
  '\.set_pick(' crates` conferma: tutti gli 11 call-site reali sono nei
  test di questo task) — stesso falso positivo di `move_asset` (Task 1):
  `count_ident` conta anche le menzioni testuali nei commenti di
  documentazione. Aggiunta un'eccezione esplicita per entrambe invece di
  fidarmi del pass accidentale dello strumento su `set_pick`.

Task 4: complete.

### Task 5 — `SearchNode::Pick`

Nuova variante `SearchNode::Pick { value: Pick }` in
`crates/keeppix-db/src/search.rs`, stesso schema per-utente di
`Rating`/`Favorite` già esistenti (Fase 7 Task 10): `asset_flags.pick` è
per utente (spec §4.1), non per asset. L'indice serve già —
`asset_flags_user_pick_idx ON asset_flags (user_id, pick) WHERE pick <>
'none'` esiste dalla migrazione 0012 (Fase 2), mai letto da una ricerca
fino ad ora. Nessuna migrazione nuova.

**Bug trovato e corretto prima di committare, non in CI**: la prima
stesura usava un unico schema `EXISTS (... AND af.pick = $val)` per tutti
e tre i valori, mirror diretto di `Rating`/`Favorite`. Ma quelle due
colonne non hanno questo problema — `favorite` è booleana (falso e
assente sono la stessa cosa per una ricerca "non preferito", che non
esiste) e non c'è un default di colonna che scrive una riga alla
creazione dell'asset. `pick` invece ha un caso reale: **"da valutare"**
(`Pick::None`) deve comprendere sia gli asset con una riga esplicita
`pick = 'none'` (Task 4: scelta annullata) **sia**, soprattutto, la
stragrande maggioranza degli asset che non hanno mai avuto **nessuna
riga** in `asset_flags` per questo utente (mai toccati). Il mirror
diretto di `Rating`/`Favorite` avrebbe trovato solo il primo gruppo,
escludendo silenziosamente tutto il resto — l'esatto contrario del
significato di "da valutare". Corretto con un ramo dedicato:
`Pick::None` compila a `NOT EXISTS (... AND af.pick IN ('pick',
'reject'))` (cattura "nessuna riga" e "riga esplicita none" in un colpo
solo), `Pick::Pick`/`Pick::Reject` restano `EXISTS (... af.pick =
$val)`. Estratto in `compile_pick_axis` (funzione privata) non solo per
il tetto di clippy `too_many_lines` — anche perché la logica a due rami
merita un posto suo, non tre righe in mezzo a un `match` da nove assi.

Verifica eseguita (misurata, non assunta — coerente col mandato "il
default è super-ottimizzato, con la misura come prova"):
- `cargo test -p keeppix-db --test search` → 31/31 verdi, incluse tre
  nuove: `pick_filter_matches_only_the_current_users_explicit_value`
  (isolamento per utente, stesso principio di rating/favorite),
  `pick_none_matches_both_never_flagged_and_explicitly_cleared`
  (verifica diretta del bug sopra: un asset mai toccato e uno con
  `pick='none'` esplicito compaiono entrambi, un asset con `pick='pick'`
  resta fuori), `pick_search_uses_the_partial_index_for_pick_and_reject`
  (`EXPLAIN` reale su 20k asset, stesso principio di
  `favorite_search_uses_the_partial_index`: conferma che
  `asset_flags_user_pick_idx` viene davvero usato dal pianificatore per
  il ramo `Pick`/`Reject`, non assunto dalla forma della query).
- `cargo fmt --all --check` → pulito su tutto il workspace.
- `cargo clippy --workspace --all-targets -- -D warnings` → pulito su
  tutto il workspace (7 crate) dopo l'estrazione di `compile_pick_axis`
  (la prima stesura, con i due rami inline in `compile_search_axis`,
  sforava `too_many_lines` di 27 righe; estrarre anche `Rating` in una
  singola espressione `let sql = ...` ha risparmiato le ultime 3 righe
  necessarie).
- `python3 scripts/check-wired.py` → verde senza eccezioni nuove:
  `SearchNode::Pick` è una variante di un enum pubblico, non una
  funzione — fuori dallo scope del controllo — e `compile_pick_axis` è
  privata.

Task 5: complete. Chiude il Gruppo B (Tasks 1-5) della fase.

## Gruppo C — La rinomina

Il piano la introduce così: *"È la parte del prototipo con più conseguenze
sul disco e le convalide più deboli. Il documento funzionale ne elenca
cinque difetti espliciti. Vanno chiusi tutti prima di toccare file veri."*
Il documento funzionale (`docs/ui/documento-funzionale-ui.md` §62, "Dialog
'Rinomina con formula'") è la fonte autorevole per la sintassi esatta —
il blurb del piano di Fase 9 ("Segnaposto: {data}, {fotocamera}, {luogo},
{titolo}, {prog:03}, {ext}") è una sintesi imprecisa: elenca `{ext}` come
segnaposto e omette `{obiettivo}`, mentre §62.3b è chiaro ed esplicito —
sei pastiglie (Data/Fotocamera/**Obiettivo**/Luogo/Titolo/Numero) e
**l'estensione non fa mai parte dello schema**, riattaccata sempre alla
fine in maiuscolo. Seguito §62, non il blurb, in ogni punto in cui
divergono — è la fonte con la sintassi esatta, i nomi delle funzioni del
prototipo, e i numeri di riga citati.

### Task 6 — Il motore delle formule

Nuovo modulo `crates/keeppix-domain/src/rename.rs` — non `keeppix-db`:
`render_filename`/`resolve_place_label` sono pura manipolazione di
stringhe, senza lettura da disco o database, quindi appartengono al
crate che non conosce SQL, testabili senza `TestDb`. Le fasi successive
del Gruppo C (Task 7-9, in `keeppix-db`) collegano questo motore a
collisioni vere, ambiti, e allo spostamento fisico via `move_asset`
(Task 1).

`RenameValues { date, camera, lens, place, title }` — valori già
risolti e **non slugificati**: la slugificazione è responsabilità di
`render_filename`, non di chi costruisce i valori. `place` è già il
risultato di `resolve_place_label(photo_position, folder_position,
lot_name)`, la precedenza esplicita del piano ("posizione della foto →
posizione della cartella → nome del lotto → niente") — pura, le tre
candidate arrivano già lette dal chiamante (Task 7/8, che sa come
procurarsele dal database).

`render_filename(schema, values, index, current_filename)` implementa
`computeRenamedFilename`/`renameSlug` del prototipo punto per punto (spec
§62.3b, 1-6): estensione sempre maiuscola e mai parte dello schema;
sostituzione letterale dei cinque segnaposto testuali (regex `\{(data|
fotocamera|obiettivo|luogo|titolo)\}` nel prototipo, qui una scansione
lineare a mano — l'insieme di segnaposto è fisso e piccolo, non serve
aggiungere `regex` a un crate che oggi non ne ha per il parsing di
testo); un segnaposto scritto male o inesistente (`{iso}`, `{Data}`
maiuscolo) resta letterale, non è un errore; contatore `\{n(?::(\d+))?\}`
1-based, riusabile più volte nello stesso schema; sanificazione finale
(`/`, `\`, `:` → `-`, spazi bianchi compressi a **uno spazio**, rifilato
ai bordi — deliberatamente non filtra `*?"<>|`, limite dichiarato del
prototipo che il Task 7 chiude); fallback al nome attuale senza
estensione se lo schema è vuoto o sanifica a niente.

Ruling: **`slug()` e la sanificazione finale sono due funzioni diverse
con regole diverse, non la stessa applicata due volte.** `slug()`
(applicata solo a fotocamera/obiettivo/luogo/titolo, mai a `{data}`)
elimina `.`/`,` e comprime gli spazi in un **trattino**; la
sanificazione finale (applicata all'intera stringa assemblata) non
elimina nulla, sostituisce `/\:`, e comprime gli spazi in **uno
spazio**. Confuse la prima stesura del test
`a_schema_that_sanitizes_to_nothing_also_falls_back` (assumeva che
`/`/`\` sparissero, mentre diventano `-`: il test è stato corretto per
riflettere la regola vera invece di piegare l'implementazione a
un'assunzione sbagliata) — verificato riga per riga contro §62.3b prima
di correggere, non a naso. — Costo se confuse in produzione: un titolo
con una virgola diventerebbe un nome con un trattino basso invece che
sparire pulito, o viceversa — piccolo ma visibile su ogni foto rinominata
che tocca quel campo.

Verifica eseguita:
- `cargo test -p keeppix-domain rename` → 19/19 verdi: lo schema di
  default della spec (`{data}_{luogo}_{n:3}` → esempio esatto del
  documento), estensione mai nello schema e sempre maiuscola, segnaposto
  malformato/inesistente lasciato letterale, contatore con e senza
  riempimento e ripetuto più volte, valore mancante che lascia
  separatori orfani (bug noto del prototipo, verificato che questo
  motore lo riproduce fedelmente — chiuderlo è il Task 7, non questo),
  schema vuoto e schema-che-sanifica-a-niente entrambi in fallback,
  `slug()` che elimina punti/virgole e comprime a trattino, `{data}` non
  slugificata, caratteri vietati del filesystem sostituiti da `-` nello
  schema e nei valori, gli altri caratteri illegali (`*?"<>|`)
  deliberatamente lasciati intatti, nome senza estensione senza punto
  finale spurio, le quattro combinazioni di `resolve_place_label`.
- `cargo test -p keeppix-domain` (l'intera suite) → 87/87 verdi, nessuna
  regressione sul resto del crate.
- `cargo fmt --all --check` → pulito su tutto il workspace.
- `cargo clippy --workspace --all-targets -- -D warnings` → due errori
  reali sulla prima stesura (`clippy::expect_used` su uno `.expect()`
  raggiungibile solo in teoria dentro il loop di scansione — riscritto
  come `while let Some(ch) = rest.chars().next()` invece di scartare
  l'avviso con un'eccezione; `clippy::type_complexity` sulla tabella dei
  segnaposto testuali — estratto un alias `type FieldLookup`), poi pulito
  su tutto il workspace (7 crate).
- `python3 scripts/check-wired.py` → verde senza segnalare
  `render_filename`/`resolve_place_label` nonostante zero chiamanti reali
  fuori da `rename.rs` (verificato con `grep`) — stesso falso positivo di
  `move_asset` (Task 1, menzioni testuali nei commenti di documentazione).
  Aggiunta un'eccezione esplicita per entrambe invece di fidarmi del pass
  accidentale.

Task 6: complete.

### Task 7 — Le cinque convalide (parte pura: 2, 3, 4)

Il piano elenca cinque difetti espliciti del prototipo (§62.3d). Tre sono
pura logica di stringa e chiudono qui, nello stesso `rename.rs`, non in un
wrapper separato: modificare `render_filename` in place, non tenere in
vita per sempre due motori paralleli (uno fedele al prototipo con i bug,
uno corretto) — il codice sostituito va cancellato, non commentato o
lasciato accanto (regola trasversale del mandato). I test del Task 6 che
documentavano il comportamento **buggy** del prototipo sono stati
aggiornati per asserire il comportamento **corretto**, non lasciati a
sé stessi ad asserire un difetto ormai chiuso.

1. **Separatori orfani** (difetto 2): `collapse_orphan_separators`, nuovo
   passo fra la sostituzione e la sanificazione finale. Comprime ogni run
   di due o più caratteri fra `_`/`-`/spazio/`.` (in qualunque
   combinazione) in uno solo, poi rifila gli stessi caratteri ai bordi
   (cattura anche il caso di un singolo separatore isolato in testa o in
   coda, non solo le run). `{data}_{luogo}_{n:3}` con luogo mancante
   produce ora `2026-08-14_001`, non più `2026-08-14__001`.
   Ruling: **non si distingue un separatore doppio scritto apposta da uno
   lasciato orfano da un valore mancante** — la scansione lavora sul testo
   assemblato, senza tracciare quali intervalli vengono da un segnaposto
   vuoto. Nessuno schema reale ha bisogno di `__` letterale, quindi il
   compromesso è a costo pressoché nullo. — *Costo se sbagliato:* uno
   schema con un doppio separatore intenzionale lo vedrebbe compattato a
   uno; nessun caso reale osservato che lo richieda.
2. **Sanificazione completa** (difetto 3): l'insieme di caratteri
   sostituiti con `-` in `sanitize()` si estende da `/\:`  (quello che il
   prototipo già faceva) a `*?"<>|` — l'elenco esplicito che la spec
   dichiara come limite del prototipo, chiuso con la stessa regola.
3. **Limite di lunghezza** (difetto 4): `MAX_FILENAME_BYTES = 255`, il
   vero `NAME_MAX` di ext4 e della maggior parte dei filesystem POSIX —
   non una cifra scelta per far passare un test. `cap_length` tronca solo
   la base calcolata dallo schema, mai l'estensione, su un confine di
   carattere UTF-8 valido, e rifila di nuovo un separatore orfano lasciato
   esposto dal taglio (stesso `ORPHAN_SEPARATORS` del punto 1).
4. **Estensione sempre presente** (difetto "4" del dialog nel Task 7, non
   della spec §62.3d — la spec lo elenca come vincolo del dialog, non del
   motore): già garantito per costruzione da `render_filename` fin dal
   Task 6 (l'estensione si riattacca sempre, mai parte dello schema).
   Nessun codice nuovo necessario, verificato con un test esplicito
   invece di darlo per scontato senza controllo.

**Deliberatamente non qui, in questo commit:**
- **Difetto 1** (collisione verificata anche fuori dal gruppo selezionato,
  contro il disco/database reale): richiede l'elenco di asset su cui
  operare, che è esattamente ciò che il Task 8 (i tre ambiti) risolve.
  Costruire il controllo delle collisioni prima di avere quell'elenco
  significherebbe costruire la stessa infrastruttura due volte — rinviato
  al Task 8, non un debito dimenticato: la sequenza Task 7→8 del piano lo
  permette perché il piano stesso lista le convalide (7) prima degli
  ambiti (8) senza dire che debbano finire nello stesso commit.
- **Difetto 5** (`"Applica"` davvero disabilitato): comportamento di
  interfaccia (`opacity:.4`, `pointer-events:none`, `aria-disabled`), non
  logica di backend — Fase 11.

Verifica eseguita:
- `cargo test -p keeppix-domain rename` → 24/24 verdi (i 19 di Task 6,
  2 aggiornati per il fix, 5 nuovi: separatore orfano in testa allo
  schema, separatori misti che collassano comunque, un doppio separatore
  intenzionale comunque compattato — Ruling sopra —, il risultato non
  supera mai 255 byte con l'estensione intatta, il taglio non lascia un
  separatore esposto in coda).
- `cargo test -p keeppix-domain` (l'intera suite) → 92/92 verdi.
- `cargo fmt --all --check` → pulito su tutto il workspace.
- `cargo clippy --workspace --all-targets -- -D warnings` → un errore
  reale (`clippy::case_sensitive_file_extension_comparisons` su un
  `.ends_with(".JPG")` in un test — riscritto con `rsplit_once('.')`),
  poi pulito su tutto il workspace (7 crate).
- `python3 scripts/check-wired.py` → verde, nessuna eccezione nuova
  (stesse due di Task 6, `render_filename`/`resolve_place_label`, ancora
  valide: la firma non è cambiata, solo il corpo).

Task 7 (parte pura): complete. Il difetto 1 si chiude insieme al Task 8.

