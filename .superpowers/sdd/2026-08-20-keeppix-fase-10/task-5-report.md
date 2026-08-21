# Task 5 — Album: «Aggiorna album» al posto dei dinamici — Report

## Comportamento implementato

1. Migrazione `0036_albums_refresh.sql`: `albums.rule jsonb`, `rule_run_at
   timestamptz`, `is_shared boolean NOT NULL DEFAULT false`, `cover_tint
   text`, `monochrome boolean NOT NULL DEFAULT false`. Nessun `kind`, nessun
   vincolo `rule`↔`kind` — come da brief.
2. `AlbumRepo::refresh(ctx, album_id) -> Result<Option<AlbumRefresh>, DbError>`:
   - `assert_owner` (solo owner/admin, come `add_asset`/`remove_asset`).
   - Legge `rule`; se `NULL` ritorna `Ok(None)` — non è un problema di
     permesso.
   - Ricompila `rule` con lo **stesso** `compile_for_sql` di `SearchRepo::run`,
     applicato con la stessa `VisibilityScope` del chiamante (owner/admin),
     `status = 'indexed'`.
   - Calcola il diff fra il set combaciante e `album_assets` corrente, scrive
     `to_add`/`to_remove` in una transazione, aggiorna `rule_run_at = now()`.
   - Ritorna `AlbumRefresh { added, removed }` (tipo di `keeppix-db`, non
     `BulkOutcome` — quel tipo vive in `keeppix-api`, `keeppix-db` non lo
     conosce, invariante architetturale di AGENTS.md).
3. `POST /api/v1/albums/{id}/refresh` (keeppix-api): traduce `AlbumRefresh`
   in `BulkOutcome::from_partition(succeeded, &[], None)` con
   `succeeded = added ++ removed`; `400 keeppix/album-has-no-rule` se
   `refresh` ritorna `None`; `403` (via `assert_owner` dentro il repository)
   per album non propri.
4. `NewAlbum.rule: Option<SearchNode>` additivo su `POST /albums` esistente
   (`CreateAlbumBody.rule`, `#[schema(value_type = Option<Object>)]` come già
   fa `SearchRequest.ast` in `routes/search.rs`). `AlbumView` espone anche
   `rule`, `rule_run_at`, `is_shared`, `cover_tint`, `monochrome` (i campi
   opzionali con `skip_serializing_if`).
5. I membri restano **sempre** in `album_assets`: nessuna cache, nessuna
   invalidazione, `list_assets`/conteggio invariati.

## TDD — rosso prima di verde

Ho scritto i test in `crates/keeppix-db/tests/albums.rs` (e aggiornato le
`NewAlbum { .. }` preesistenti in quel file e in `tests/geo.rs` col nuovo
campo `rule`) **prima** di scrivere `AlbumRepo::refresh`, poi ripristinato
temporaneamente `albums.rs`/`lib.rs` alla versione pre-task per osservare il
rosso:

```
error[E0560]: struct `NewAlbum` has no field named `rule`          (x5)
error[E0599]: no method named `refresh` found for struct `AlbumRepo`  (x4)
error: could not compile `keeppix-db` (test "albums") due to 13 previous errors
```

Poi ripristinata l'implementazione: 11/11 verdi (7 preesistenti + 4 nuovi:
`refreshing_an_album_without_a_rule_returns_none`,
`refresh_adds_matches_and_removes_non_matches_and_is_idempotent`,
`refreshing_a_foreign_album_is_forbidden`, più l'aggiornamento delle
`NewAlbum` preesistenti che da sole avrebbero già reso rosso il file).

Il test di idempotenza (`refresh_adds_matches_and_removes_non_matches_and_is_idempotent`)
copre esplicitamente il punto 2 del brief: crea un album con `rule =
Type(image)`, aggiunge **a mano** un video fuori filtro, chiama `refresh` e
verifica che le due foto entrino (`added`) e il video esca (`removed`),
verifica `rule_run_at` scritto, poi richiama `refresh` una seconda volta senza
cambi nel catalogo e verifica `added`/`removed` entrambi vuoti (nessun
duplicato, nessun movimento spurio).

Test API in `crates/keeppix-api/tests/albums.rs` (nuovo file, 3 test):
`refresh_returns_added_ids_as_succeeded_bulk_outcome`,
`refresh_without_a_rule_is_a_bad_request`,
`refresh_on_a_foreign_album_is_forbidden` — verificano la forma
`BulkOutcome`, il `400` con `type = "keeppix/album-has-no-rule"`, e il `403`
per un utente senza permesso sull'album (non `404`).

Test di migrazione `album_refresh_columns_exist` in
`crates/keeppix-db/tests/migrations.rs`: verifica che le 5 colonne esistano
in `information_schema.columns` dopo `0036`.

## Ruling — semantica di `BulkOutcome` per il refresh

Documentato nel ledger (`.superpowers/sdd/2026-08-20-keeppix-fase-10/progress.md`,
sezione «Task 5»):

- `succeeded` elenca **sia** gli asset id **aggiunti** sia quelli **rimossi**
  in questa esecuzione — sono due facce della stessa mutazione riuscita, non
  due categorie di risultato distinte. Non c'è un modo di distinguerli nel
  corpo oggi.
- `failed` resta tipicamente vuoto: il refresh è un diff calcolato
  server-side sugli asset già visibili al chiamante (stessa
  `VisibilityScope` di `SearchRepo::run`), non un'operazione per-id come le
  altre del Task 1 dove ogni elemento può fallire indipendentemente.
- Album senza `rule` → `400 keeppix/album-has-no-rule`, non `403`/`409`: non
  è un problema di autorizzazione né di conflitto, è che non c'è nulla da
  rilanciare. Il repository segnala questo caso con `Ok(None)` (non un
  `DbError` nuovo), e la traduzione in `400` avviene nel livello HTTP — stesso
  pattern di `routes/geotag.rs` (`source_has_no_location`).

## Risultati dei test

```
keeppix-db   tests/albums.rs       11 passed (4 nuovi + 7 preesistenti)
keeppix-db   tests/migrations.rs   11 passed (1 nuovo: album_refresh_columns_exist) + 1 ignored (invariato)
keeppix-db   tests/geo.rs          14 passed (invariato, NewAlbum.rule additivo)
keeppix-api  tests/albums.rs        3 passed (nuovo file)
keeppix-api  tests/openapi.rs       7 passed (invariato: albums resta fuori dalla
                                     superficie OpenAPI generata, la chiude il
                                     Task 10/23 — nessuno snapshot da rigenerare)
```

`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi su tutto il workspace. `./scripts/test.sh` completo **non
eseguito** (stesso motivo dei task precedenti nel ledger: costerebbe l'intera
suite, testcontainer per crate, `--jobs 1`); eseguiti invece tutti i moduli
di test toccati dal task più `openapi.rs` per confermare l'assenza di
regressioni sullo snapshot.

## Non fatto (fuori scope)

- Nessuna UI per il badge "condiviso"/tinta copertina: solo le colonne e la
  lettura via `AlbumView` (il brief chiede la migrazione + il refresh, non il
  frontend).
- `is_shared`/`cover_tint`/`monochrome` non sono ancora scrivibili via
  `PATCH /albums/{id}` (`AlbumPatch` non li porta): il brief non lo chiede
  esplicitamente («minimo per questo task: rule + refresh»); restano colonne
  presenti, leggibili, con default sensato, pronte per un task successivo che
  le esponga in scrittura.
- Task 6 non iniziato, come richiesto.
