# Task 8 report — Duplicati ed editing batch

Branch: `fase-2`. Non pushato, come richiesto.

## Cosa è stato fatto

### Duplicati (deduplica esatta per `content_hash`, spec §7)

`DuplicateRepo` (`crates/keeppix-db/src/duplicates.rs`) sostituisce
`ProblemsRepo::duplicates` della Fase 1c, che restava un singolo `GET`
senza modo di scegliere quale copia tenere:

| Metodo | Cosa fa |
|---|---|
| `groups(ctx)` | Gruppi con lo stesso `content_hash` e `count > 1`, visibili al chiamante, esclusi gli asset `trashed` |
| `members(ctx, hash)` | I singoli asset di un gruppo (per scegliere quale tenere), esclusi i `trashed` |
| `resolve(ctx, hash, keep, action)` | Applica una delle tre opzioni di cancellazione (spec §6) a ogni membro non tenuto, riusando `TrashRepo::choose` |

Due requisiti pinnati dal brief:

- **I `trashed` non contano come duplicati**: sono già in coda per
  sparire, e "recuperabile" non ha senso per una copia che sta per
  essere cancellata comunque. `groups`/`members` filtrano
  `a.status <> 'trashed'` esplicitamente.
- **Spazio recuperabile = `size_bytes × (copie − 1)`, non la somma
  totale**: `DuplicateGroup::reclaimable_bytes()` è
  `size_bytes.saturating_mul(count.saturating_sub(1))` — la prima copia
  è la foto, non spazio da liberare.

### Editing batch (spec §3)

`OverrideRepo` (già esistente da Task 4, `apply`/`apply_batch`/
`undo_batch`) guadagna un'operazione e un vincolo più stretto
sull'annullamento:

- **`shift_taken_at(ctx, asset_ids, hours)`**: scostamento di N ore
  sulla data di scatto **come operazione a sé**, non calcolato dal
  client sottraendo due date assolute — il rimedio per l'orologio
  della fotocamera sbagliato dopo un viaggio. Un solo statement calcola
  `COALESCE(override, exif).taken_at + make_interval(hours => N)` per
  riga, vale sia per un asset sia per 5.000, registra un batch di
  annullamento esattamente come `apply_batch`, e lascia senza data un
  asset che non ne aveva nessuna (uno scostamento non inventa
  un'origine).
- **`undo_batch` rifiuta con `Conflict` una volta scritto il
  sidecar**: se il sidecar di **anche un solo** asset del batch è già
  stato scritto con i valori di questo batch
  (`xmp_written_at >= metadata_batches.applied_at`), l'annullamento è
  bloccato invece di riportare indietro il database lasciando sul
  disco un file con un valore che Keeppix non ricorda più come
  "attuale". Prima di quel momento resta sempre permesso.

### Rotte API

Tre file nuovi sotto `crates/keeppix-api/src/routes/`, tutti dietro
`Auth` (sessione richiesta) e con `403 Forbidden` — mai `404` — su un
id sondato fuori dalla propria visibilità:

| File | Rotte |
|---|---|
| `duplicates.rs` | `GET /api/v1/duplicates`, `GET /api/v1/duplicates/{content_hash}`, `POST /api/v1/duplicates/{content_hash}/resolve` |
| `metadata.rs` | `GET`/`PATCH /api/v1/assets/{id}/metadata`, `POST /api/v1/metadata/batch`, `POST /api/v1/metadata/batch/shift-taken-at`, `POST /api/v1/metadata/batch/{batch_id}/undo` |
| `flags.rs` | `GET`/`PUT /api/v1/assets/{id}/flags`, `POST /api/v1/flags/batch` |

`flags.rs` espone `FlagRepo`, già scritto in Task 4 ma senza rotta
HTTP fino ad ora. `duplicates.rs` riusa `routes::trash::parse_action`
(reso `pub(crate)`) per tradurre la stringa `disk_action` in
`DiskAction` — stessa validazione, stesso `400`, di
`DELETE /api/v1/assets/{id}`.

`docs/api/openapi.json` rigenerato: 26 → 34 operazioni.

## MISURATO: `apply_batch` su 5.000 asset

Requisito del brief: "sotto un secondo, misurare, non assumere". Test
`apply_batch_on_five_thousand_assets_stays_under_a_second`
(`crates/keeppix-db/tests/overrides.rs`) — seed di 5.000 righe con un
`INSERT ... SELECT ... FROM unnest(...)` di massa (non 5.000
round-trip di `upsert_discovered`, perché qui si misura `apply_batch`,
non la preparazione dei dati), poi un solo `apply_batch` che imposta
la posizione su tutti e 5.000, poi il suo `undo_batch`.

```
$ cargo test -p keeppix-db --release --test overrides \
    apply_batch_on_five_thousand_assets_stays_under_a_second -- --nocapture --test-threads=1

apply_batch su 5000 asset: 57.489636ms
undo_batch su 5000 asset: 10.560479ms
```

In debug (`cargo test` senza `--release`, la modalità in cui gira di
solito lo sviluppo):

```
apply_batch su 5000 asset: 75.375784ms
undo_batch su 5000 asset: 26.658266ms
```

Sia in release sia in debug, **due ordini di grandezza sotto il
vincolo di un secondo**: il collo di bottiglia che il brief temeva (500
o 5.000 round-trip separati) non esiste — `apply_batch` è sempre stato
un solo `INSERT ... SELECT ... FROM unnest(...)` con
`ON CONFLICT DO UPDATE` (Task 4), non un ciclo per asset; misurarlo
qui conferma il design invece di correggerlo. Il limite dentro il test
resta volutamente permissivo (`< 3s`, non `< 1s`) per non renderlo
instabile su una macchina condivisa o più lenta di questa — la cifra
vera è quella stampata sopra, non l'asserzione.

### File creati

- `crates/keeppix-db/src/duplicates.rs` — `DuplicateRepo`,
  `DuplicateGroup` con `reclaimable_bytes()`.
- `crates/keeppix-db/tests/duplicates.rs` — 5 test di dominio + 3 di
  harness (spazio recuperabile corretto, `trashed` escluso, `resolve`
  cestina gli altri membri, `keep` fuori dal gruppo è `Forbidden`, un
  proprietario non vede i duplicati di un altro).
- `crates/keeppix-api/src/routes/duplicates.rs` — le tre rotte, DTO
  `DuplicateGroupView`/`ResolveDuplicateRequest`/`ResolveDuplicateResponse`.
- `crates/keeppix-api/tests/duplicates.rs` — 4 test HTTP end-to-end.
- `crates/keeppix-api/src/routes/metadata.rs` — le cinque rotte, DTO
  con il deserializzatore `double_option` per `Option<Option<T>>`.
- `crates/keeppix-api/tests/metadata.rs` — 6 test HTTP end-to-end
  (patch parziale, `null` esplicito vs campo assente, batch apply +
  undo, `shift_taken_at`, undo rifiutato dopo il sidecar, probing
  `Forbidden`).
- `crates/keeppix-api/src/routes/flags.rs` — le tre rotte, DTO
  `AssetFlagsBody`/`BatchFlagsRequest`.
- `crates/keeppix-api/tests/flags.rs` — 4 test HTTP end-to-end (round
  trip, default quando non si è mai votato, due utenti non si
  sovrascrivono, `batch_set`).

### File modificati

- `crates/keeppix-db/src/overrides.rs` — `shift_taken_at`, il
  controllo `already_synced` in `undo_batch` (nuova variante già
  esistente `DbError::Conflict`), funzione privata `apply_shift`.
- `crates/keeppix-db/tests/overrides.rs` — 6 nuovi test: la misura sui
  5.000, tre su `shift_taken_at` (scostamento positivo/negativo,
  asset senza data nota resta senza data), due sulla finestra di
  annullamento (rifiutato dopo il sidecar, permesso se il sidecar era
  antecedente al batch).
- `crates/keeppix-db/src/problems.rs` — rimossi `DuplicateGroup` e
  `duplicates()`, spostati in `duplicates.rs`.
- `crates/keeppix-db/tests/problems.rs` — rimosso il test che ora vive
  in `tests/duplicates.rs`.
- `crates/keeppix-db/src/lib.rs` — moduli `duplicates`/`flags`
  (`flags` esisteva già da Task 4, qui solo l'esportazione se mancante),
  esportazioni.
- `crates/keeppix-api/src/routes/problems.rs` — rimossa la rotta
  `duplicates` (spostata).
- `crates/keeppix-api/src/routes/trash.rs` — `parse_action` da privato
  a `pub(crate)`, riusata da `duplicates::resolve`.
- `crates/keeppix-api/src/{lib,routes/mod,openapi}.rs` — cablaggio
  delle tre nuove rotte, tag OpenAPI `metadata`/`flags`.
- `crates/keeppix-api/tests/openapi.rs` — liste di percorsi/
  operationId aggiornate (26 → 34 operazioni).
- `crates/keeppix-api/tests/harness/mod.rs` — helper di seed
  refactorizzati per creare libreria/cartella una sola volta per test
  invece che per asset (necessario appena un test seeda più di un
  asset nella stessa libreria: altrimenti `ensure_path` colpiva
  "root_path già indicizzato" al secondo asset).
- `docs/api/openapi.json` — rigenerato con `UPDATE_OPENAPI=1`.

## Verifica prima di dichiarare fatto

```
cargo fmt --check                                            → verde
cargo clippy --workspace --all-targets -- -D warnings         → verde
cargo test -p keeppix-domain --jobs 1 -- --test-threads=1      → 44 passed
cargo test -p keeppix-db --jobs 1 -- --test-threads=1          → tutti verdi
                                                                  (duplicates: 8, overrides: 21)
cargo test -p keeppix-media --jobs 1 -- --test-threads=1       → tutti verdi
                                                                  eccetto video::poster_extracts_one_frame
                                                                  (preesistente, vedi sotto)
cargo test -p keeppix-api --jobs 1 -- --test-threads=1         → tutti verdi
                                                                  (duplicates: 4, metadata: 6, flags: 4,
                                                                  openapi: 6 incluso lo snapshot)
cargo test -p keeppix-jobs --jobs 1 -- --test-threads=1        → tutti verdi
cargo test -p keeppix-server --jobs 1 -- --test-threads=1      → tutti verdi
```

`cargo deny` non è installato in questo ambiente (stessa nota presente
dal Task 6); questo task non tocca alcun `Cargo.toml`, quindi non c'è
un nuovo arco `keeppix-media`↔`keeppix-db` da verificare.

`./scripts/test.sh` non arriva in fondo in questo sandbox: la sua
`cleanup_containers` chiama `docker ps` assumendo che il demone sia
raggiungibile solo perché il binario `docker` esiste nel `PATH` — qui
non c'è un demone Docker in ascolto, `docker ps` fallisce, e
`set -euo pipefail` interrompe lo script subito dopo il primo crate
(`keeppix-api`, il cui test suite intero risultava comunque verde).
Stessa causa già annotata nei ledger di Task 3/5. Non modificato lo
script (fuori scope): rieseguito l'equivalente manualmente,
`cargo test -p <crate> --jobs 1 -- --test-threads=1` con
`KEEPPIX_TEST_DATABASE_URL` puntato a Postgres locale, un crate alla
volta, per l'intero workspace.

**Fallimento preesistente, non toccato**: `keeppix-media --test
video::poster_extracts_one_frame` — l'estrazione del poster fallisce
nel sandbox rlimit di questo ambiente (`ffmpeg poster failed`),
indipendentemente da Task 8 (nessun file di `keeppix-media` è stato
toccato). Stessa causa già annotata nei ledger dei Task 4/5/6/7,
verificata di nuovo qui isolando il test (`cargo test -p keeppix-media
--test video -- --nocapture`): fallisce identicamente in isolamento,
quindi non è un effetto collaterale di test che girano in parallelo.

## I cinque requisiti pinnati dal brief

1. **`apply_batch` su 5.000 asset sotto un secondo, misurato non
   assunto** — vedi la sezione MISURATO sopra: 57ms release / 75ms
   debug.
2. **Scostamento di N ore su `taken_at` come operazione a sé** —
   `shift_taken_at`, tre test:
   `shifting_taken_at_moves_every_asset_by_the_same_number_of_hours`,
   `shifting_taken_at_accepts_a_negative_offset_and_is_undoable`
   (verificato che il segno negativo funzioni e che l'operazione sia
   annullabile come qualunque altro batch),
   `shifting_taken_at_on_an_asset_without_any_known_date_stays_unset`.
3. **L'annullamento funziona finché il sidecar non è stato scritto** —
   due test complementari:
   `undo_is_refused_once_the_sidecar_reflects_this_batchs_values`
   (scrive il sidecar con `mark_sidecar_written` **dopo**
   `applied_at`, l'undo torna `Conflict`) e
   `undo_still_works_when_the_sidecar_was_written_before_this_batch_was_applied`
   (il sidecar era stato scritto **prima**, con un valore ormai
   superato — l'undo deve restare permesso, altrimenti un vecchio
   sidecar bloccherebbe per sempre l'annullamento di un batch
   successivo). Il secondo test è quello che distingue "sidecar
   scritto" da "sidecar scritto **con questi valori**": senza il
   confronto `>= applied_at`, la prima versione ingenua del controllo
   (`xmp_written_at IS NOT NULL`) avrebbe fallito qui.
4. **I duplicati non contano i `trashed`** —
   `a_trashed_copy_does_not_count_as_a_duplicate`: due copie con lo
   stesso hash, una cestinata, il gruppo non compare più (era
   `count = 2`, ora `count = 1` non supera `HAVING count(*) > 1`).
5. **Spazio recuperabile = `size_bytes × (copie − 1)`** —
   `duplicates_report_reclaimable_space_not_the_total`: tre copie da 10
   MB, `reclaimable_bytes()` deve tornare 20 MB (2 copie recuperabili),
   non 30 MB (la somma totale).

## Decisioni degne di nota (vedi ledger per il dettaglio completo)

- **`resolve()` non è tutto-o-niente**: itera i membri del gruppo
  chiamando `TrashRepo::choose` uno alla volta; se uno fallisce, i
  precedenti restano già cestinati/eliminati. Un rollback dovrebbe
  "disfare" un `rename()` o una cancellazione già avvenuta sul
  filesystem — più fragile del comportamento scelto.
- **`parse_action` reso `pub(crate)`** invece di duplicato: stessa
  mappa stringa→`DiskAction`, stesso `400`, un solo punto di verità fra
  `DELETE /api/v1/assets/{id}` e `duplicates::resolve`.
- **`double_option` scritto a mano** invece di aggiungere
  `serde_with`: un solo punto d'uso in tutto il workspace non
  giustifica una dipendenza in più per risolvere un problema di poche
  righe.

## Commit

```
49a2068 feat(db): extract DuplicateRepo with members/resolve, exclude trashed
e62d817 feat(db): shift taken_at by N hours, and refuse undo once synced
75a84ea feat(api): move duplicates listing off problems, add members and resolve
51378bf feat(api): expose effective metadata, batch apply/shift, and undo
91d8ed1 feat(api): expose per-user culling flags with single and batch endpoints
```

Non pushato. Non avviato Task 9.

## Nota per chi riprende

- `DuplicateRepo::resolve` non è transazionale sull'intero gruppo
  (vedi Ruling sopra) — se la Fase 3 introduce condivisione fra utenti
  diversi sulla stessa libreria, un gruppo con permessi misti fra i
  membri potrebbe lasciarsi a metà. Nel modello di visibilità attuale
  non può succedere: tutti i membri di un gruppo di duplicati
  appartengono alla stessa libreria, quindi allo stesso owner.
- `TrashRepo::cleanup_expired` (Task 7) resta non agganciata a uno
  scheduler — nota già presente nel ledger di Task 7, non toccata qui.
- Il fallimento di `video::poster_extracts_one_frame` è ormai annotato
  in cinque ledger di fila (Task 4-8): se dovesse bloccare un task
  futuro che dipende dai poster video, va risolto con un rlimit più
  permissivo o investigato a parte — finora nessun task di questa fase
  ne ha avuto bisogno per i suoi test.
