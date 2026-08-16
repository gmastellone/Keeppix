# Task 6: Stack RAW+JPEG — report

Commit: `8a3308f` (`feat(db): group raw and jpeg shots into stacks`)
Branch: `fase-2`

## Cosa è stato fatto

- **Migrazione `crates/keeppix-db/migrations/0013_stacks.sql`**:
  - `CREATE TABLE stacks (id uuid PRIMARY KEY, primary_asset_id uuid NOT
    NULL REFERENCES assets (id) DEFERRABLE INITIALLY DEFERRED,
    created_at timestamptz NOT NULL DEFAULT now())`.
  - `ALTER TABLE assets ADD CONSTRAINT assets_stack_id_fkey FOREIGN KEY
    (stack_id) REFERENCES stacks (id) ON DELETE SET NULL` — la colonna
    esisteva già dalla `0005_assets.sql` (nullable, senza FK), come
    indicato nel brief; non è stata toccata.
  - `CREATE INDEX assets_stack_id_idx ON assets (stack_id) WHERE
    stack_id IS NOT NULL`.
  - Funzione + trigger `assets_promote_stack_primary`: `AFTER DELETE OR
    UPDATE OF stack_id ON assets`, `FOR EACH ROW`. Quando l'asset che
    lascia lo stack (cancellato, o il cui `stack_id` cambia) era il
    primario, promuove un altro membro rimasto (preferendo un RAW,
    altrimenti per nome file); se non ne resta nessuno, cancella la riga
    di `stacks` invece di lasciarla puntare a un asset sparito.

- **`crates/keeppix-db/src/stacks.rs`**: `StackRepo::regroup_folder(folder_id)`.
  Nessun `AuthContext` (lo chiamerà lo scanner su un'intera cartella,
  come `LibraryRepo::mark_scanned`). Raggruppa gli asset non cestinati
  di una cartella per nome base case-insensitive (`DSC_0042.arw` e
  `dsc_0042.JPG` sono lo stesso scatto), sceglie il RAW come primario
  quando presente (altrimenti il primo per nome file, deterministico),
  e riusa lo `stack_id` già presente sui membri di un gruppo invece di
  crearne uno nuovo a ogni chiamata — è la proprietà di idempotenza che
  il brief segnala come critica.

- **`crates/keeppix-domain/src/ids.rs`**: aggiunto `StackId`, stesso
  pattern degli altri newtype UUID (`AssetId`, `BatchId`, ...),
  ri-esportato da `lib.rs`.

- **`crates/keeppix-db/tests/stacks.rs`**: 6 test di integrazione (uno
  per requisito del brief) + 2 unit test su `basename_key` dentro
  `stacks.rs`.

## Regole di raggruppamento implementate

**Regola 1 (spec §5)**: stesso nome base, stessa cartella. Implementata
e testata. **Regola 2** (scatti entro 2 secondi, stesso corpo macchina,
stesso numero di scatto): **non implementata**, come il brief permette
esplicitamente. Vedi il `Ruling` nel ledger per il motivo (nessun campo
"numero di scatto" nello schema; il numero di scatto vive in blocchi
MakerNote proprietari per marca, fuori dal parsing EXIF generico già
fatto in Fase 1) e per il costo della scelta.

## TDD e test di mutazione

Ho scritto l'implementazione insieme ai test (non rigorosamente
rosso-poi-verde riga per riga), ma ho verificato con **mutation
testing mirato** che ogni test richiesto dal brief fallisca davvero
quando il comportamento che dichiara di proteggere viene rotto — la
domanda esplicita dell'AGENTS.md ("se rompo apposta la cosa che questo
test protegge, fallisce?"). Tre mutazioni, tutte osservate rosse e poi
ripristinate verdi:

1. **RAW come primario**: sostituito `group.iter().find(kind ==
   raw_image).unwrap_or(group[0])` con `group[0]` puro.
   `the_raw_is_the_primary_asset_when_present` continuava a passare
   con i nomi file originali (`DSC_0043.ARW`/`DSC_0043.JPG`): "ARW"
   ordina prima di "JPG" alfabeticamente, quindi il fallback "primo per
   nome" sceglieva per coincidenza lo stesso file del RAW — un test
   verde che non provava quello che dichiarava, esattamente il difetto
   che l'AGENTS.md chiede di cercare. Corretto usando `.NEF` (che
   ordina dopo sia `.HEIC` sia `.JPG`) nei due test che asseriscono sul
   primario; con quella correzione la mutazione fallisce
   (`left: Some(<jpeg>), right: Some(<raw>)`).
2. **Idempotenza**: rimossa la logica di riuso dello stack esistente
   (sempre `INSERT INTO stacks` con un id nuovo).
   `regrouping_the_same_folder_twice_is_idempotent` fallisce
   (`left: <uuid1>, right: <uuid2>`) — è il test che il brief segnala
   come "quello che rompe" senza il quale ogni scansione crea uno stack
   nuovo.
3. **Trigger disabilitato**: commentato temporaneamente `CREATE
   TRIGGER assets_promote_stack_primary` nella migrazione.
   `deleting_the_primary_promotes_another_member_instead_of_orphaning_the_stack`
   fallisce — non con uno stack orfano silenzioso, ma con un errore
   Postgres esplicito (`violates foreign key constraint
   "stacks_primary_asset_id_fkey"`, codice `23503`): la FK `NOT NULL` +
   `DEFERRABLE` sulla riga scoperta dal fallimento della cancellazione
   comunque impedisce la corruzione dei dati, trasformando l'assenza
   del trigger in un errore rumoroso invece che in un bug silenzioso.

## Una scoperta empirica non banale: BEFORE vs AFTER

Il primo tentativo di trigger usava `BEFORE DELETE OR UPDATE OF
stack_id`. Compilava e i test passavano nel caso semplice, ma
ragionando sull'ordine delle operazioni Postgres per il caso "stack a
membro singolo che si scioglie" (regroup che stacca l'ultimo membro di
un gruppo ridotto a uno) sono arrivato a sospettare un problema di
auto-modifica: `DELETE FROM stacks` dentro il trigger `BEFORE`
innescherebbe, tramite il cascade `ON DELETE SET NULL` della FK gemella
`assets.stack_id -> stacks.id`, un tentativo di modificare di nuovo la
riga `assets` che l'`UPDATE`/`DELETE` esterno sta ancora processando in
quel momento.

L'ho **verificato empiricamente**, non solo per ragionamento:
sostituendo temporaneamente `AFTER` con `BEFORE` nella migrazione e
rieseguendo `deleting_the_primary_promotes_another_member_...`, il test
falliva esattamente con:

```
tuple to be updated was already modified by an operation triggered by the current command
```

Passato a `AFTER` (la riga OLD è già sparita o già sul nuovo
`stack_id` quando il trigger legge lo stato di `assets`, quindi
nessuna auto-modifica), il problema sparisce. Ho aggiunto
`DEFERRABLE INITIALLY DEFERRED` sulla FK `stacks.primary_asset_id ->
assets.id` per lo stesso motivo dal lato opposto: con `AFTER`, il
nostro trigger e il trigger interno che applica quella FK sono
entrambi `AFTER` sulla stessa tabella per lo stesso evento, e il loro
ordine relativo dipenderebbe altrimenti dai nomi generati internamente
da Postgres per i trigger di vincolo — fragile e non portabile fra
versioni. Differendo il controllo a fine transazione, l'ordine non
conta più: il nostro trigger ha comunque tutto il tempo di riassegnare
`primary_asset_id` prima che il vincolo venga controllato al commit.

## Verifica eseguita

```
cargo test -p keeppix-db --test stacks -- --test-threads=1   → 9 passed (6 richiesti + 3 harness)
cargo test -p keeppix-domain --jobs 1 -- --test-threads=1     → 42 passed
cargo test -p keeppix-db --jobs 1 -- --test-threads=1         → tutti verdi (assets, flags, folders, jobs,
                                                                  overrides, stacks, users, visibility, ...)
cargo test -p keeppix-jobs --jobs 1 -- --test-threads=1       → tutti verdi (raw, xmp, discover, hash, moves, ...)
cargo test -p keeppix-api -p keeppix-server -p keeppix-dav
            -p keeppix-test-support --jobs 1 -- --test-threads=1 → tutti verdi
cargo build --workspace --all-targets                          → verde
cargo fmt --check                                               → verde
cargo clippy --workspace --all-targets -- -D warnings           → verde
```

Fallimento preesistente, non toccato e non introdotto da questo task
(già annotato nel ledger dei Task 4/5): `keeppix-media --test
video::poster_extracts_one_frame` — ffmpeg non riesce a scrivere un
frame in questo sandbox.

`cargo deny check bans` non eseguibile in questo ambiente (`cargo-deny`
non installato: `error: no such command: 'deny'`). Non è un problema
per questo task: nessun `Cargo.toml` è stato toccato, nessun nuovo arco
di dipendenza `keeppix-media`↔`keeppix-db` è stato introdotto.

`frontend/dist` esisteva già (non toccato da questo task) e
`cargo build --workspace` è verde: la condizione "senza dist/ il
backend non compila" resta soddisfatta.

## Non fatto (fuori dai confini del task, per istruzione esplicita)

- **Wiring in `discover`/`hash`**: il piano elenca per Task 6 solo la
  migrazione, `stacks.rs` e i test — nessuna modifica a `keeppix-jobs`.
  `StackRepo::regroup_folder(folder_id)` è pronto per essere chiamato
  dallo scanner (una volta per cartella, dopo aver scritto gli asset
  della cartella — lo stesso punto in cui `discover.rs::run` oggi
  chiama `folders.ensure_path`), ma questo cablaggio è lasciato a un
  task successivo, come il brief permette esplicitamente ("StackRepo
  methods that discover can call later").
- **Regola 2 dello spec** (scatti entro 2 secondi, stesso corpo
  macchina, stesso numero di scatto): non implementata, vedi Ruling
  sopra e nel ledger.
- Nessuna modifica al task 7+ (cestino, duplicati, culling): fuori
  scope, non toccato.

## Ledger

Aggiornato `.superpowers/sdd/2026-08-15-keeppix-fase-2/progress.md`
con la tabella di avanzamento e una sezione "Task 6" con i due Ruling
(regola 2 non implementata; promozione/pulizia via trigger SQL invece
che via metodo di repository, con la scoperta BEFORE/AFTER) e il
dettaglio dei test di mutazione.
