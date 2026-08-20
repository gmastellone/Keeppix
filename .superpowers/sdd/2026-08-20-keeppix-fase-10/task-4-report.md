# Task 4 — Eliminazione di massa a tre vie — Report

## Comportamento implementato

1. `POST /api/v1/assets/batch/delete` con `{asset_ids, disk_action}`, risposta
   `BulkOutcome` (involucro del Task 1). `disk_action` obbligatorio, stessa
   validazione di `parse_action` (riusato, non duplicato).
2. Per elemento riusa `TrashRepo::choose` — nessuna nuova logica di
   cancellazione, come richiesto.
3. `disk_action = purged`: `TrashRepo::assert_batch_purge_authorized` gira
   **prima** di qualunque `choose`, e rifiuta l'intero lotto con `Forbidden`
   al primo id non purgabile (non owner/admin della sua libreria) — nessun
   file toccato. `kept`/`moved_to_trash` restano a riuscita parziale per-id,
   come le altre operazioni di massa del Task 1.
4. `TrashRepo::choose` è stato refattorizzato: il suo preambolo di
   autorizzazione (visibilità, editor/owner-admin, risoluzione di
   `asset`/`library`/`folder_abs`) è ora la funzione libera
   `authorize_choose`, condivisa con `assert_batch_purge_authorized` — la
   pre-verifica del lotto e l'esecuzione per elemento usano lo stesso
   cancello, non una copia.

## TDD — rosso prima di verde

Tre test in `crates/keeppix-api/tests/trash.rs`, eseguiti **prima**
dell'implementazione e osservati fallire con `404` (la rotta non esisteva
ancora):

```
test batch_delete_partial_success_when_one_file_is_already_missing ... FAILED
  left: 404  right: 200
test batch_delete_partial_success_when_the_trash_folder_is_not_writable ... FAILED
  left: 404  right: 200
test batch_delete_purged_by_a_non_owner_editor_rejects_the_whole_batch_untouched ... FAILED
  left: 404  right: 403
```

Dopo l'implementazione (route + handler + refactor db), tutti e tre verdi
insieme ai tre test preesistenti di `trash.rs`.

- `batch_delete_purged_by_a_non_owner_editor_rejects_the_whole_batch_untouched`:
  un editor (visibile e modificabile, non owner/admin) chiede `purged` su due
  asset — `403`, ed entrambi i file restano sul disco. Verifica il cancello
  "autorizzazione prima dell'esecuzione, non a metà" richiesto dal brief.
- `batch_delete_partial_success_when_one_file_is_already_missing`: un file
  scompare dal disco fuori da Keeppix; il lotto usa `moved_to_trash` (non
  `purged`, che è tollerante ai file mancanti — `remove_file_tolerant`
  ignora `NotFound`). L'asset presente va in `succeeded`, quello assente in
  `failed` con `reason = "file-missing"`.
- `batch_delete_partial_success_when_the_trash_folder_is_not_writable`: due
  asset in due sottocartelle diverse della stessa libreria; la sottocartella
  di cestino di una delle due è pre-creata e resa di sola lettura (`chmod
  0o555`) prima della richiesta. L'altro asset finisce in `succeeded`,
  quello bloccato in `failed` con `reason = "permission-denied"` — il caso
  reale più probabile secondo il brief.

## Verifica richiesta dal brief: «nessuna posizione» come valore

Ho scritto un test end-to-end contro Postgres reale
(`crates/keeppix-db/tests/overrides.rs`,
`effective_location_after_an_explicit_clear_does_not_fall_back_to_the_exif_value`)
per confermare che l'azzeramento esplicito di una posizione vinca su quella
exif dell'asset. **Non vince**: osservato RED —

```
left: Some(GeoPoint { lat: 41.9, lon: 12.5 })   (l'exif)
right: None                                      (atteso: nessuna posizione)
```

`OverrideRepo::effective` legge `COALESCE(o.location, a.location)`: un
override con quel campo esplicitamente `NULL` (l'utente ha negato il luogo)
produce lo stesso `NULL` SQL di "nessuna riga di override ancora scritta" —
`COALESCE` non li distingue. La stessa ambiguità vale per `taken_at` e
`place_id` (stesso pattern), non solo per `location`.

**Non risolto in questo task**: un fix corretto richiede una
colonna/sentinella per campo che distingua "esplicitamente azzerato" da
"non toccato" — tocca `apply_patch`, `load_previous`/`restore_previous`
(l'annullamento dei batch) e `sidecar_source`, non solo `effective`. È un
cambiamento di modello dati più grande del batch delete di questo task, e
farlo solo per `location` lascerebbe `taken_at`/`place_id` incoerenti. Il
test resta nel repository marcato `#[ignore]` (non `#[allow]` su
un'asserzione sbagliata: il corpo è il comportamento corretto voluto, quindi
risolvere il difetto significa solo togliere l'attributo). Dettagli e
motivazione completa nel ledger, sezione Task 4.

## Risultati dei test

```
keeppix-api tests/trash.rs        6 passed (3 nuovi + 3 preesistenti)
keeppix-api tests/duplicates.rs   4 passed (usa parse_action/choose via resolve)
keeppix-api tests/metadata.rs     8 passed
keeppix-api tests/openapi.rs      7 passed (snapshot rigenerato, operazioni 82 -> 83,
                                   elenchi operationId/security aggiornati)
keeppix-db  tests/trash.rs       12 passed (refactor authorize_choose, comportamento invariato)
keeppix-db  tests/duplicates.rs   8 passed (DuplicateRepo::resolve chiama choose in loop)
keeppix-db  tests/permissions.rs 17 passed
keeppix-db  tests/overrides.rs   23 passed + 1 ignored (difetto noto, vedi sopra)
```

`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi su tutto il workspace. `./scripts/test.sh` completo **non
eseguito** (stesso motivo dei Task 2/3: costerebbe l'intera suite);
eseguiti invece tutti i test toccati dal task più i moduli con dipendenza
diretta su `TrashRepo::choose`/`parse_action`.

## Rulings

Documentati in `.superpowers/sdd/2026-08-20-keeppix-fase-10/progress.md`
sotto "## Task 4": la scomposizione di `choose` in `authorize_choose` +
`assert_batch_purge_authorized`, la scelta di lasciare `kept`/`moved_to_trash`
a riuscita parziale mentre solo `purged` è tutto-o-niente
sull'autorizzazione, e il difetto deferito su `EffectiveMetadata`/location
con la sua motivazione completa.

## Non fatto (fuori scope)

- Task 5 non iniziato, come richiesto.
- Il difetto `EffectiveMetadata`/`COALESCE` (location, e per estensione
  `taken_at`/`place_id`) non è stato corretto — deferito, vedi sopra.
- Nessuna migrazione toccata; nessun file fuori da
  `routes/trash.rs`/`trash.rs` (db)/`lib.rs`/`openapi.rs`/test modificato per
  la feature stessa (solo `overrides.rs` per la verifica separata richiesta
  dal brief).
