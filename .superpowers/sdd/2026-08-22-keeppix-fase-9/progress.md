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
