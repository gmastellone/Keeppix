# Task 2 — Endpoint di geometria della timeline: report

Branch: `fase-10`. Base per questo task: `187bdf0` (fine Task 1bis).
Commit di questo task: `e3c7944`, `447cd66`, `597372a`, `9bab5d0`, `bad0e79`.

**Status: DONE.** Il formato binario a 6 byte e l'index-only scan richiesti
dal piano (§7 PROSEGUI) sono entrambi realizzati e verificati con `EXPLAIN
ANALYZE` su 200.000 righe — non è stato necessario fermarsi con BLOCKED.

## Cosa c'è

1. Migrazione `0034_assets_geometry_idx.sql`: indice di copertura
   `assets_geometry_idx ON assets (folder_id, taken_at_utc DESC, id DESC)
   INCLUDE (width, height) WHERE status = 'indexed'`, esattamente come nella
   spec §2.4.
2. `TimelineRepo::geometry` / `geometry_in_bounds` in `keeppix-db` (con
   `AuthContext` come primo parametro, `Forbidden` su libreria non propria).
   Restituiscono `Geometry { records: Vec<GeometryRecord>, last_modified }`;
   nessun uuid nel record, coerente con la spec.
3. `GET /api/v1/timeline/geometry` in `keeppix-api`: stessi parametri di
   `/timeline/buckets` (`library`, `bbox`), stessa `VisibilityScope`. Risposta
   `application/octet-stream`: 8 byte di intestazione (`version: u32 LE`,
   `count: u32 LE`) + N record da 6 byte (`w: u16`, `h: u16`, `month: u16`,
   tutti LE). `ETag` sul conteggio + `max(updated_at)`; `If-None-Match` che
   combacia torna `304` a corpo vuoto.
4. Registrato nel router (`lib.rs`, vicino a `/timeline/buckets`) e
   nell'OpenAPI (`openapi.rs`), con lo snapshot congelato
   `docs/api/openapi.json` rigenerato (diff additivo, solo la nuova
   operazione `timeline_geometry`).
5. Test:
   - `crates/keeppix-db/tests/timeline.rs`: ordine/encoding dei record,
     `None` per asset non dimensionati, corrispondenza col conteggio dei
     bucket, filtro `kind<>'unknown'` nel percorso bbox, `Forbidden` su
     libreria altrui.
   - `crates/keeppix-db/tests/scale_geometry.rs`: 200.000 asset in una
     libreria, `EXPLAIN (ANALYZE, BUFFERS)` della query reale mostra
     `Index Only Scan using assets_geometry_idx` con `Heap Fetches: 0`,
     sotto un budget esplicito di 900ms per l'intera vista.
   - `crates/keeppix-api/tests/timeline.rs`: richiesta autenticazione,
     ordine e decodifica binaria, `w=0,h=0` per asset non dimensionati,
     `304` su `If-None-Match` combaciante, conteggio uguale alla somma dei
     bucket, `403` su libreria altrui via HTTP.
   - `crates/keeppix-api/tests/openapi.rs`: percorso aggiunto alle liste
     attese (`documented_operations_are_all_mounted`,
     `operation_ids_are_explicit_and_unique`,
     `security_requirements_name_a_declared_scheme`), contatore
     `checked` portato da 81 a 82, snapshot rigenerato.
   - `crates/keeppix-db/tests/migrations.rs`: `assets_geometry_idx` aggiunto
     all'elenco atteso in `performance_indexes_exist`.

## TDD — RED prima di GREEN

### DB layer

RED (compilazione, metodi assenti):

```
$ cargo test -p keeppix-db --test timeline --no-run
error[E0599]: no method named `geometry` found for struct `TimelineRepo`
error[E0599]: no method named `geometry_in_bounds` found for struct `TimelineRepo`
error: could not compile `keeppix-db` (test "timeline") due to 4 previous errors
```

GREEN dopo l'implementazione (`TimelineRepo::geometry`/`geometry_in_bounds`,
`GeometryRecord`/`Geometry`, migrazione 0034):

```
running 11 tests
test buckets_sum_indexed_photos_by_month ... ok
test geometry_matches_bucket_counts ... ok
test geometry_omits_unknown_kind_assets_when_filtering_by_bbox ... ok
test geometry_orders_records_like_the_timeline_and_encodes_nulls_as_none ... ok
test harness::tests::appends_when_the_url_has_no_database ... ok
test harness::tests::preserves_the_query_string ... ok
test harness::tests::replaces_an_existing_database_name ... ok
test page_uses_keyset_not_offset ... ok
test probing_someone_elses_library_geometry_is_forbidden ... ok
test probing_someone_elses_library_is_forbidden ... ok
test timeline_page_omits_unknown_assets ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### API layer (le richieste HTTP compilano comunque: RED è un fallimento a
runtime — 404/401 mancante — non un errore di compilazione)

RED, prima dell'handler e della rotta:

```
running 18 tests
test probing_someone_elses_library_geometry_is_forbidden ... FAILED
  left: 404  right: 403
test timeline_geometry_count_matches_bucket_counts ... FAILED
  assertion `left == right` failed: versione del formato binario
  left: 2037654139  right: 1
test timeline_geometry_encodes_missing_dimensions_as_zero ... FAILED
  left: 404  right: 200
test timeline_geometry_requires_auth ... FAILED
  left: 404  right: 401
test timeline_geometry_returns_304_on_matching_if_none_match ... FAILED
  left: 404  right: 200
test timeline_geometry_returns_ordered_binary_records ... FAILED
  left: 404  right: 200

test result: FAILED. 12 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out
```

GREEN dopo l'handler, la rotta in `lib.rs` e la registrazione OpenAPI:

```
running 18 tests
test asset_detail_returns_the_existing_public_view_without_coordinates ... ok
test buckets_return_month_counts_for_indexed_assets ... ok
test folder_children_include_direct_folders_and_assets ... ok
test folder_tree_lists_visible_folders ... ok
test folder_tree_roots_omits_descendants ... ok
test moving_a_folder_does_not_rewrite_asset_rows ... ok
test probing_someone_elses_folder_is_forbidden ... ok
test probing_someone_elses_library_buckets_is_forbidden ... ok
test probing_someone_elses_library_geometry_is_forbidden ... ok
test timeline_bbox_filters_pages_and_bucket_counts ... ok
test timeline_buckets_require_auth ... ok
test timeline_geometry_count_matches_bucket_counts ... ok
test timeline_geometry_encodes_missing_dimensions_as_zero ... ok
test timeline_geometry_requires_auth ... ok
test timeline_geometry_returns_304_on_matching_if_none_match ... ok
test timeline_geometry_returns_ordered_binary_records ... ok
test timeline_keyset_keeps_assets_that_share_a_truncated_second ... ok
test timeline_page_uses_keyset_cursor ... ok

test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Test di scala (200k) — la parte che poteva finire in BLOCKED

Prima iterazione (bug nel mio helper `EXPLAIN`, non nel codice di
produzione): avevo bindato `library_id = NULL` nell'`EXPLAIN` mentre il
test misurato chiamava `geometry(&ctx, Some(library_id))`. Con
`NULL` Postgres scarta `assets_geometry_idx` per il vecchio
`assets_timeline_idx` (niente `Sort`, ma heap fetch riga per riga):

```
thread '...' panicked: la query di /timeline/geometry deve servirsi dal
solo assets_geometry_idx, non degradare a seq scan o a heap fetch per riga:
Nested Loop ...
  ->  Index Scan using assets_timeline_idx on assets a ...
```

Corretto il bind (`Some(library_id)`, lo stesso della chiamata misurata),
il piano usa l'indice nuovo:

```
MEASUREMENT geometry (whole view) 200000: 615.624013ms (200000 record)
EXPLAIN geometry:
Sort  (cost=5008.10..5010.93 rows=1132 width=32) (actual time=72.506..85.507 rows=200000 loops=1)
  Sort Key: a.taken_at_utc DESC, a.id DESC
  Sort Method: external merge  Disk: 8240kB
  ->  Nested Loop  (cost=0.57..4950.68 rows=1132 width=32) (actual time=0.024..39.840 rows=200000 loops=1)
        ->  Index Scan using folders_pkey on folders f (... rows=11 loops=1)
              Filter: (library_id = '...'::uuid)
        ->  Index Only Scan using assets_geometry_idx on assets a
              (cost=0.42..1431.09 rows=20000 width=48) (actual time=0.007..2.366 rows=18182 loops=11)
              Index Cond: ((folder_id = f.id) AND (taken_at_utc IS NOT NULL))
              Heap Fetches: 0
Execution Time: 94.941 ms
test geometry_of_two_hundred_thousand_assets_stays_within_budget_and_index_only ... ok
```

`Heap Fetches: 0` conferma l'index-only scan richiesto dalla spec §2.4.
Rieseguito due volte per la stabilità del tempo (571ms, 620ms), sempre sotto
il budget di 900ms.

## Verifica finale (comandi eseguiti e output osservato)

```
cargo fmt --check                                            → verde (dopo `cargo fmt`)
cargo clippy --workspace --all-targets -- -D warnings         → verde, 0 warning
cargo build --workspace                                       → verde
cargo test -p keeppix-db --test timeline                      → 11/11 ok
cargo test -p keeppix-db --test scale_geometry                → 4/4 ok
cargo test -p keeppix-db --test migrations                    → 10/10 ok (1 ignored, preesistente)
cargo test -p keeppix-api --test timeline                     → 18/18 ok
cargo test -p keeppix-api --test openapi                      → 7/7 ok (incl. snapshot rigenerato)
```

**`./scripts/test.sh` completo NON eseguito.** Costerebbe l'intera suite del
workspace in serie (`--jobs 1 --test-threads=1`), inclusi `scale_200k.rs`,
`perf_task12.rs`, `fase2_culling_1k.rs` e gli altri test di scala non
toccati da questo task — costoso e non necessario per isolare la verifica di
questo lavoro. Ho eseguito invece: build e clippy su tutto il workspace
(che tocca ogni crate a livello di compilazione) più i test focalizzati sui
file toccati (`keeppix-db::timeline`, `keeppix-db::scale_geometry`,
`keeppix-db::migrations`, `keeppix-api::timeline`, `keeppix-api::openapi`).
Non ho toccato `scale_200k.rs` né altri file di test preesistenti oltre a
quanto elencato sopra, quindi il rischio di regressione fuori da questi file
è quello ordinario di un'aggiunta additiva (nuova migrazione, nuovo
endpoint, nessuna modifica a query esistenti).

## Rulings (vedi anche il ledger `progress.md`, sezione "Task 2")

1. **Nessun `id`, nessuna `count(*)` separata.** `records.len()` è il
   conteggio; l'`ETag` combina quello con `max(updated_at)`.
2. **Il percorso senza `bbox` non filtra `kind<>'unknown'`** (a differenza
   di `page`/`buckets_in_bounds`), per restare index-only-scan compatibile
   e per combaciare esattamente con `folder_month_counts` (il cui trigger
   non guarda `kind`). Il percorso con `bbox` invece lo filtra, per
   combaciare con `buckets_in_bounds`. Documentato nel ledger con il costo
   se la scelta si rivela sbagliata.
3. **`assets_geometry_idx` ha `folder_id` come colonna guida**, come da
   spec. Senza un filtro di libreria che lo restringa, Postgres preferisce
   il vecchio `assets_timeline_idx` (niente `Sort`, ma heap fetch per riga)
   — misurato, non ipotizzato. Il test di scala usa quindi `?library=...`,
   il caso per cui l'indice è disegnato; un ipotetico multi-libreria senza
   filtro resterebbe su un piano diverso (non peggiore di un seq scan, ma
   nemmeno index-only). Fuori dal perimetro che la spec chiede di
   verificare.
4. **Budget di scala = 900ms** su 200k asset per l'intera vista (vs 300ms
   per una singola pagina di `page`/`buckets`): il grosso del tempo
   osservato (~500-600ms su ~615ms totali) è trasferimento/decodifica
   client-side di 200k righe in una sola risposta, non il piano
   (`EXPLAIN ANALYZE` mostra ~85-110ms server-side). È il costo che
   l'endpoint sostituisce a 1.070 richieste paginate, non uno che aggiunge.
5. **Formato binario**: intestazione 8 byte (`version: u32 LE = 1`,
   `count: u32 LE`), poi record da 6 byte (`w: u16`, `h: u16`,
   `month: u16 = anno*12+mese`), tutto LE. Width/height e month saturano ai
   margini di `u16` invece di traboccare (niente panic su EXIF corrotto).
6. **`ETag` con una seconda query leggera** (`max(updated_at)`, stessi
   filtri, senza `width`/`height`): selezionare `updated_at` nella query
   principale romperebbe l'index-only scan (quella colonna non è
   nell'`INCLUDE`). Misurata: ~26ms su 200k righe, trascurabile.
7. **OpenAPI**: `body = [u8]` → utoipa risolve da solo a
   `application/octet-stream`. Non ho introdotto una struct segnaposto per
   ottenere `{type: string, format: binary}` (soluzione nota ma più
   invasiva, vedi juhaku/utoipa#1146): il content-type — la parte che
   conta per un client generato — è comunque corretto.

## Concerns

- **Nessun BLOCKED**: sia il formato binario a 6 byte sia l'index-only scan
  sono stati raggiunti come da spec, non c'è stato bisogno di ricorrere a un
  fallback JSON.
- Il budget di scala (900ms) è generoso (3x il misurato) per non rendere il
  test instabile in CI; una regressione più piccola di quel margine non
  verrebbe segnalata. Preferibile a un budget stretto che fallisce a caso.
- Il percorso "tutte le librerie, nessun filtro" (admin senza `?library=`)
  non ottiene l'index-only scan (Postgres sceglie `assets_timeline_idx` con
  heap fetch per riga) — non degrada a seq scan, ma non è nemmeno la
  garanzia della spec. Non è il caso che la spec chiede di verificare
  esplicitamente (che è "la vista", tipicamente una libreria), quindi non ho
  aperto un secondo indice per coprirlo; segnalato nel ledger come nota per
  chi mai volesse ottimizzarlo.
- Task 3 (collasso degli stack) non è stato toccato: la geometria oggi
  include ogni asset indicizzato, stack member compresi, come richiesto
  esplicitamente dal piano per questo task.

## File toccati

```
crates/keeppix-db/migrations/0034_assets_geometry_idx.sql   (nuovo)
crates/keeppix-db/src/timeline.rs
crates/keeppix-db/src/lib.rs
crates/keeppix-db/tests/timeline.rs
crates/keeppix-db/tests/scale_geometry.rs                    (nuovo)
crates/keeppix-db/tests/migrations.rs
crates/keeppix-api/src/routes/timeline.rs
crates/keeppix-api/src/lib.rs
crates/keeppix-api/src/openapi.rs
crates/keeppix-api/tests/timeline.rs
crates/keeppix-api/tests/openapi.rs
docs/api/openapi.json
.superpowers/sdd/2026-08-20-keeppix-fase-10/progress.md
```
