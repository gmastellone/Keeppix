# Task 4 report: `asset_overrides` e `asset_flags`

**Status: DONE**

Branch `fase-2`. Commit principale `6a17f4b` (`feat(db): add metadata
overrides and per-user flags`), più un commit di rafforzamento test
`1949a5e` (`test(db): pin undo restoring NULL on an existing override
row`) nato dalla mutation testing di questa sessione — vedi §3.

## 1. Cosa è stato fatto

- Migrazione `crates/keeppix-db/migrations/0012_overrides_flags.sql`:
  `asset_overrides`, `asset_flags`, `metadata_batches`, indici inclusi,
  testo SQL identico a quello del brief/piano. Commento del migratore in
  `crates/keeppix-db/src/lib.rs` aggiornato a `0012_overrides_flags`.
- Dominio:
  - `keeppix-domain::flags`: `Rating(u8)` (`parse` rifiuta >5),
    `Pick::{None,Pick,Reject}` con `as_str`/`parse`, `AssetFlags`.
  - `keeppix-domain::overrides`: `GeoPoint`, `OverridePatch` (ogni campo
    `Option<Option<T>>`: `None`=non toccare, `Some(None)`=azzera,
    `Some(Some(v))`=imposta), `EffectiveMetadata`.
  - `BatchId` in `ids.rs`; `InvalidRating`/`InvalidPick` in `error.rs`.
  - Tutto ri-esportato da `keeppix_domain::lib.rs`.
- DB:
  - `OverrideRepo::{effective, apply, apply_batch, undo_batch,
    pending_sidecars}` in `crates/keeppix-db/src/overrides.rs`.
  - `FlagRepo::{set, get, batch_set}` in `crates/keeppix-db/src/flags.rs`.
  - Nuovo `AssetRepo::assert_visible` (in `crates/keeppix-db/src/assets.rs`)
    condiviso da entrambi i repo: un `count(DISTINCT ...)` sul filtro di
    `VisibilityScope` verifica in una sola query che *tutti* gli id di un
    batch siano visibili al chiamante, invece di un round-trip per id.
  - `AuthContext` è il primo parametro di ogni metodo che legge o scrive
    dati utente, come richiesto dall'invariante di AGENTS.md.
- Test:
  - `crates/keeppix-db/tests/overrides.rs` — 15 test.
  - `crates/keeppix-db/tests/flags.rs` — 9 test.
  - Totale 24, tutti i comportamenti richiesti dal brief pinnati (vedi §2).

## 2. Comportamenti pinnati (mappa test ↔ requisito del brief)

| Requisito | Test |
|---|---|
| `effective()` = COALESCE campo per campo | `effective_coalesces_override_and_exif_field_by_field`, `effective_coalesces_location_and_place_id_from_the_asset` |
| Override parziale non azzera campi non toccati | `a_later_partial_override_does_not_erase_an_earlier_field` |
| `apply_batch` su 500 asset = 1 operazione | `apply_batch_on_many_assets_is_one_operation` (timing < 5s **e** `count(*) FROM metadata_batches` = 1, non solo "esiste la mia riga") |
| `undo_batch` ripristina anche NULL (riga mai esistita) | `undo_batch_restores_a_previous_value_that_was_null` |
| `undo_batch` ripristina anche NULL (riga già esistente, altro campo) | `undo_batch_restores_a_null_field_on_a_row_that_already_existed` — aggiunto in questa sessione, vedi §3 |
| `undo_batch` ripristina la riga esatta | `undo_batch_restores_the_exact_previous_row` |
| `undo_batch` su batch già annullato è idempotente | `undoing_an_already_undone_batch_is_idempotent` |
| Rating per utente, due utenti non si sovrascrivono | `two_users_rating_the_same_asset_do_not_overwrite_each_other` |
| Non proprietario / id inesistente → `Forbidden` mai `NotFound` | `a_plain_user_cannot_apply_overrides_on_someone_elses_asset`, `probing_a_nonexistent_asset_id_is_forbidden_not_not_found` (overrides), `a_plain_user_cannot_set_flags_on_someone_elses_asset`, `probing_a_nonexistent_asset_id_is_forbidden_not_not_found` (flags), `undo_batch_rejects_a_non_owner_and_a_nonexistent_id` |
| `pending_sidecars` solo `updated_at > COALESCE(xmp_written_at, '-infinity')` | `pending_sidecars_only_lists_updates_not_yet_written` |

## 3. TDD: RED → GREEN, con mutation testing

### 3.1 Ciclo naturale

Tutti i test in `tests/overrides.rs` e `tests/flags.rs` sono stati scritti
seguendo l'elenco del brief prima di scrivere `OverrideRepo`/`FlagRepo`;
compilavano contro API che non esistevano ancora (RED per assenza di
implementazione), poi sono diventati verdi con `OverrideRepo`/`FlagRepo`
scritti secondo lo schema del piano.

### 3.2 Mutation testing (questa sessione)

Un test verde non dimostra nulla se non si è mai visto rosso per la
regressione che dichiara di proteggere. Rieseguito il ciclo mutare →
rosso → ripristinare → verde su tre invarianti critiche:

**Mutazione 1 — override parziale che azzera un campo non toccato.**

In `touched()` (`crates/keeppix-db/src/overrides.rs`), forzato
`None => (true, None)` invece di `(false, None)`:

```
test a_later_partial_override_does_not_erase_an_earlier_field ...
thread 'a_later_partial_override_does_not_erase_an_earlier_field' panicked:
assertion `left == right` failed
  left: None
 right: Some("Titolo")
test result: FAILED. 0 passed; 1 failed
```

Ripristinato → `test result: ok. 1 passed; 0 failed`.

**Mutazione 2 — undo che non ripristina NULL su una riga già esistente.**

In `restore_previous` (stesso file), sostituito `EXCLUDED.col` con
`COALESCE(EXCLUDED.col, asset_overrides.col)` nell'`UPSERT` di ripristino
(la trappola descritta dal brief: «l'annullamento trasforma un campo mai
valorizzato in stringa vuota» — qui, il campo resta al valore sbagliato
invece di tornare NULL). **I due test esistenti sull'undo restavano
entrambi verdi sotto questa mutazione**: un gap reale nella copertura.

- `undo_batch_restores_a_previous_value_that_was_null` esercita solo il
  ramo `DELETE` di `restore_previous` (l'asset non aveva *nessuna* riga
  di override prima del batch: `previous` per quell'id è `None` a
  livello di mappa, non "campi tutti NULL").
- `undo_batch_restores_the_exact_previous_row` esercita il ramo `UPDATE`
  ma ripristina un valore non-NULL (`"Titolo originale"`).

Nessuno dei due copre "riga già esistente da un batch precedente, un
campo da riportare a NULL con un `UPDATE`". Aggiunto
`undo_batch_restores_a_null_field_on_a_row_that_already_existed`
(commit `1949a5e`): batch 1 imposta `title`, batch 2 imposta
`description` (creando così, per il batch 2, uno stato precedente con
`description = NULL` su una riga che esiste già); l'annullamento del
batch 2 deve riportare `description` a `NULL` senza toccare `title`.

Sotto la mutazione:

```
test undo_batch_restores_a_null_field_on_a_row_that_already_existed ...
thread '...' panicked:
assertion `left == right` failed: la description non esisteva prima del
secondo batch: l'annullamento su una riga già esistente deve tornare a
NULL, non restare al valore appena scritto
  left: Some("Descrizione")
 right: None
test result: FAILED. 0 passed; 1 failed
```

Ripristinato `restore_previous` → tutti e 15 i test di `overrides.rs`
verdi, incluso il nuovo.

**Mutazione 3 — rating che ignora l'isolamento per utente.**

In `FlagRepo::get`, tolto il filtro `AND user_id = $2` dalla `SELECT`:

```
test two_users_rating_the_same_asset_do_not_overwrite_each_other ...
thread '...' panicked:
assertion `left == right` failed
  left: Some(Rating(5))
 right: Some(Rating(2))
test result: FAILED. 0 passed; 1 failed
```

Ripristinato → `test result: ok. 9 passed; 0 failed`.

## 4. Verifica finale (comandi eseguiti, output osservato)

```
$ export KEEPPIX_TEST_DATABASE_URL='postgres://postgres:postgres@127.0.0.1:5432/postgres'
$ cargo test -p keeppix-db --test overrides --test flags -- --test-threads=1
...
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out  (flags)
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out (overrides)
```

```
$ cargo fmt --check
(nessun output — verde)

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s)
(nessun warning — verde)
```

Suite completa, un crate alla volta (`./scripts/test.sh` non eseguibile
in questo ambiente: `docker ps` fallisce con `set -e`/`pipefail` perché il
demone Docker non è disponibile qui, per costruzione — solo
`KEEPPIX_TEST_DATABASE_URL` è previsto, coerente con le istruzioni
d'ambiente di questo task):

```
keeppix-domain     → 42 passed
keeppix-test-support → 0 (nessun test)
keeppix-media      → FAILED: video::poster_extracts_one_frame
keeppix-db         → 13 + 7 + 9 (flags) + 15 (overrides) = tutti ok
keeppix-dav        → 0 (nessun test)
keeppix-jobs       → 9 + 5 = tutti ok
keeppix-api        → 8 + 2 + 2 = tutti ok
keeppix-server     → 4 + 5 = tutti ok
```

`keeppix-media::video::poster_extracts_one_frame` è preesistente e
indipendente da Task 4: il commit `6a17f4b` non tocca alcun file di
`keeppix-media` (verificato con `git show 6a17f4b --stat`), e il
fallimento è nel wrapper ffmpeg per l'estrazione poster di un video, non
in una query o in un tipo di dominio toccati da questo task. Causa più
probabile: limiti del sandbox verso il processo ffmpeg in questo
ambiente specifico (rlimit/seccomp), non una regressione introdotta qui.

`cargo deny check bans`: non esiste un arco nuovo `keeppix-media` ↔
`keeppix-db` (`OverrideRepo`/`FlagRepo` restano dentro `keeppix-db`, il
dominio non conosce sqlx).

## 5. Ledger

Aggiornato `.superpowers/sdd/2026-08-15-keeppix-fase-2/progress.md`:
tabella di avanzamento (Task 4 → complete), sezione narrativa con le tre
mutazioni sopra e tre `Ruling`:

- `AssetRepo::assert_visible` come nuovo helper condiviso (non elencato
  esplicitamente nel brief) per verificare 500 id in una query.
- `metadata_batches.previous` cattura l'intera riga di `asset_overrides`,
  non solo i campi toccati dal patch — necessario perché `undo_batch`
  possa ripristinare lo stato esatto anche quando un batch successivo ha
  toccato un campo diverso.
- `pending_sidecars` senza `AuthContext`, stesso pattern già concordato
  per `LibraryRepo::mark_scanned` (userà l'AuthContext solo Task 5 col
  job `WriteSidecar`).

## 6. Non fatto (fuori scope, rimandato)

- Nessuna scrittura effettiva del sidecar XMP: `pending_sidecars` espone
  solo la query di selezione, come da confine del brief — il job che la
  consuma è Task 5.
- Non toccato `keeppix-api` (nessun endpoint HTTP per overrides/flags):
  fuori dai file elencati nel brief di Task 4.

## 7. Non pushato

Nessun `git push` eseguito, come richiesto. `git log --oneline -3` sul
branch `fase-2`:

```
1949a5e test(db): pin undo restoring NULL on an existing override row
6a17f4b feat(db): add metadata overrides and per-user flags
57f4169 Cursor: Apply local changes for cloud agent
```
