# Task 7 report — Cestino e cancellazione a tre opzioni

Branch: `fase-2`. Non pushato, come richiesto.

## Cosa è stato fatto

Le tre opzioni di cancellazione (spec §6), presentate ogni volta senza
default implicito:

| Opzione | Effetto | `disk_action` |
|---|---|---|
| Rimuovi dall'indice | il file resta sul disco, l'asset sparisce dall'indice (tornerà alla prossima scansione) | `kept` |
| Sposta nel cestino | `rename()` in `.keeppix-trash/` dentro la stessa libreria, recuperabile per 30 giorni | `moved_to_trash` |
| Elimina dal disco | cancellazione irreversibile, solo owner/admin | `purged` |

### File creati

- `crates/keeppix-domain/src/trash.rs` — `DiskAction` (enum a stringa
  snake_case con `parse`/`as_str`) e `TrashEntry` (riga di
  audit/ripristino).
- `crates/keeppix-db/migrations/0014_trash.sql` — tabella
  `trash_entries`, senza FK verso `assets.id` (motivato nel commento
  della migrazione: l'audit deve sopravvivere alla cancellazione della
  riga `assets` che avviene nella stessa transazione per `kept` e
  `purged`).
- `crates/keeppix-db/src/trash.rs` — `TrashRepo` con `choose`,
  `restore`, `cleanup_expired`, e l'helper puro `may_purge`.
- `crates/keeppix-db/tests/trash.rs` — 12 test di integrazione (8 di
  copertura richiesta dal brief più 4 aggiuntivi: harness).
- `crates/keeppix-api/src/routes/trash.rs` — `DELETE
  /api/v1/assets/{id}` e `POST /api/v1/assets/{id}/restore`.
- `crates/keeppix-api/tests/trash.rs` — 3 test di integrazione HTTP
  end-to-end (round-trip cestino→ripristino, `disk_action`
  sconosciuto → 400, asset non visibile → 403).

### File modificati

- `crates/keeppix-domain/src/{error,ids,lib}.rs` — `InvalidDiskAction`,
  `TrashEntryId`, esportazioni.
- `crates/keeppix-db/src/error.rs` — nuova variante `DbError::Io` per
  gli errori del filesystem che accompagnano la scrittura sul database
  (`rename()`, cancellazione), distinta da `Connection`.
- `crates/keeppix-db/src/lib.rs` — modulo `trash`, esportazioni
  `TrashRepo`/`TRASH_DIR_NAME`.
- `crates/keeppix-media/tests/walk.rs` — nuovo test
  `walker_excludes_the_keeppix_trash_directory` (il codice sorgente
  `walk.rs` già escludeva `.keeppix-trash` da un task precedente
  dell'archiviazione — Task 7 aggiunge il test dedicato che lo pinna
  esplicitamente, dato che il brief lo segnala come "il difetto che si
  dimentica e produce un ciclo infinito su una libreria grande").
- `crates/keeppix-api/src/{lib,routes/mod,openapi}.rs` — cablaggio delle
  due rotte, tag OpenAPI `trash`.
- `crates/keeppix-api/tests/openapi.rs` — aggiornate le liste di
  percorsi/operationId attese (22 → 24 operazioni).
- `docs/api/openapi.json` — rigenerato con `UPDATE_OPENAPI=1`.

## Verifica prima di dichiarare fatto

```
cargo fmt --check                                          → verde
cargo clippy --workspace --all-targets -- -D warnings       → verde
cargo test -p keeppix-domain --jobs 1 -- --test-threads=1    → 44 passed
cargo test -p keeppix-db --jobs 1 -- --test-threads=1        → tutti verdi (trash: 12 passed)
cargo test -p keeppix-media --jobs 1 -- --test-threads=1     → tutti verdi
                                                                eccetto video::poster_extracts_one_frame
                                                                (preesistente, ffmpeg nel sandbox — vedi sotto)
cargo test -p keeppix-api --jobs 1 -- --test-threads=1       → tutti verdi (trash: 3 passed,
                                                                openapi_snapshot_matches_the_committed_file verde)
```

`cargo deny` non è installato in questo ambiente (stessa nota già
presente nel ledger dal Task 6); questo task non aggiunge dipendenze
né tocca alcun `Cargo.toml`, quindi non c'è un nuovo arco
`keeppix-media`↔`keeppix-db` da verificare.

`./scripts/test.sh` non eseguito: richiede Docker per i container
`testcontainers`, non disponibile in questo ambiente (per istruzione
esplicita "No Docker"). Eseguito l'equivalente crate per crate con
`KEEPPIX_TEST_DATABASE_URL` puntato a Postgres locale, come nei task
precedenti di questa fase.

**Fallimento preesistente, non toccato**: `keeppix-media --test
video::poster_extracts_one_frame` — ffmpeg non riesce a scrivere un
frame in questo sandbox. Stessa causa già annotata nei ledger dei Task
4/5/6, indipendente da questo lavoro.

## I sei requisiti pinnati con TDD + mutation testing

Ognuno scritto come test, poi verificato con una mutazione deliberata
dell'implementazione (rotta, osservata rossa, ripristinata, riverificata
verde):

1. **`rename()`, non copia, inode invariato** —
   `moving_to_trash_is_a_rename_that_keeps_the_inode`. Mutazione:
   `copy()` + `remove_file()` invece di `rename()` in `move_into_trash`
   → l'assert sull'inode fallisce (`left != right`).
2. **`.keeppix-trash/` escluso dalla scansione** —
   `walker_excludes_the_keeppix_trash_directory`. Mutazione: rimossa
   `.keeppix-trash` da `is_excluded_name` → il file cestinato torna
   visibile al walker.
3. **Il ripristino rimette il file al percorso originale, asset a
   `indexed`** — `restore_puts_the_file_back_and_marks_the_asset_indexed`.
4. **Il ripristino non sovrascrive se il percorso è occupato** —
   `restore_does_not_overwrite_a_file_that_now_occupies_the_original_path`.
   Mutazione: rimosso il controllo `original.exists()` → il file che
   occupava il posto verrebbe sovrascritto.
5. **Solo owner e admin possono `purged`; altrimenti `Forbidden`** —
   `only_owner_and_admin_can_purge_an_editor_gets_forbidden` (unit test
   diretti su `may_purge` per admin estraneo/owner/nessuno dei due).
   Mutazione: `may_purge` sempre `true` → il test fallisce.
6. **La pulizia oltre 30 giorni cancella dal disco e rimuove la
   riga** — `cleanup_expired_deletes_the_file_and_the_row_past_the_cutoff`.
   Mutazione: rimosso il filtro `deleted_at < before` → viene pulito
   anche il cestinamento recente (`cleaned == 2` invece di `1`).

Più due test aggiuntivi non esplicitamente elencati dal brief ma
coerenti con l'invariante "Forbidden mai NotFound" e con la robustezza
del ripristino:

- `restoring_an_asset_that_is_not_in_the_trash_is_a_conflict` (nessun
  cestinamento pendente → `Conflict`, non un panic o un no-op
  silenzioso).
- `probing_someone_elses_asset_for_trash_is_forbidden` (a livello db) e
  `probing_someone_elses_asset_is_forbidden_not_found` (a livello HTTP,
  403 non 404).

## Decisioni degne di nota (vedi ledger per il dettaglio completo)

- **`may_purge` come funzione pura**: nel modello di visibilità di
  questa fase (nessuna condivisione prima della Fase 3), chiunque veda
  un asset è già owner o admin — un test end-to-end su "editor
  Forbidden" non potrebbe distinguere il cancello di `Purged` dal
  controllo di visibilità che lo precede. La funzione pura si pinna
  con tre unit test diretti, indipendenti da come la visibilità
  evolverà in Fase 3.
- **Nessuna FK `trash_entries.asset_id → assets.id`**: `kept` e
  `purged` cancellano la riga `assets` nella stessa transazione in cui
  scrivono l'audit; una FK con cascade la distruggerebbe insieme
  all'asset.
- **`cleanup_expired` non ancora agganciata a un job schedulato**: il
  brief di questo task non lo richiede esplicitamente (elenca solo
  migrazione, `trash.rs`, rotta API). Il metodo è pronto e testato;
  chi lo chiamerà passerà `Utc::now() - Duration::days(30)`.

## Commit

```
3edb207 feat(domain): add DiskAction and TrashEntry types
a35c1a4 feat(db): add trash repository with three delete options and restore
b82380a test(media): pin that the walker excludes .keeppix-trash
04e8cb6 feat(api): add delete and restore endpoints for the trash
```

Non pushato. Non avviato Task 8.

## Nota per chi riprende

`TrashRepo::cleanup_expired` esiste e ha copertura di test ma non è
richiamata da nessun job scheduler in questo task — se il piano
generale prevede un job dedicato (es. `JobKind::CleanupTrash`) in un
task successivo, questo è il punto di aggancio pronto. Se nessun task
successivo lo prevede, va segnalato come lavoro differito prima della
chiusura della fase.
