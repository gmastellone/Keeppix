# Task 3 — Lo stack collassato nelle viste di browse — Report

## Punto di partenza

Un implementatore precedente si era bloccato a metà lavoro con modifiche non
committate e due migrazioni concorrenti numerate `0035`/`0036` che
duplicavano/contraddicevano la stessa modifica a `assets_geometry_idx`. Ho
verificato ogni file toccato riga per riga (non fidandomi del lavoro
trovato), eliminato le migrazioni duplicate e tenuto solo
`0035_assets_geometry_idx_stack.sql`. Ho anche trovato e corretto un bug
reale lasciato dal lavoro precedente: si veda "Bug trovato in review" sotto.

## Comportamento implementato

1. `AssetView` (`routes/timeline.rs`) ha due campi additivi: `stack_size:
   u16` e `raw_kind: Option<String>` (`"raw"` / `"jpeg"` / `"raw+jpeg"`).
2. `TimelineRepo::page`/`page_in_bounds`, `SearchRepo::run` e tutte le
   varianti di `TimelineRepo::geometry*` restituiscono solo il primario di
   ogni pila (`LEFT JOIN stacks s ON s.id = a.stack_id` + `WHERE
   (a.stack_id IS NULL OR a.id = s.primary_asset_id)`).
3. `TimelineRepo::buckets`/`buckets_in_bounds` contano le pile, non i file:
   riscritte per leggere direttamente da `assets` con lo stesso filtro di
   primario, invece di `folder_month_counts` (che il trigger non aggiorna
   in modo stack-aware — Ruling in `progress.md`).
4. `assets_geometry_idx` (0034) esteso in `0035_assets_geometry_idx_stack.sql`
   con `stack_id`/`kind` nell'`INCLUDE`, per non perdere l'index-only scan
   quando la query di geometria filtra quei due campi.

## Bug trovato in review

Il commento sopra `TimelineRepo::geometry` (percorso senza `bbox`)
dichiarava già il filtro `a.kind <> 'unknown'`, ma l'SQL non lo applicava —
un resto del refactor precedente lasciato a metà. Corretto in `geometry()`,
nel suo `last_modified_sql` e in `geometry_stamp()` (che deve restare sugli
stessi filtri di `geometry()` per non far divergere `count` da
`records.len()`, rompendo il 304 su `If-None-Match`).

## TDD — rosso prima di verde

Il test `geometry_omits_unknown_kind_assets_without_a_bbox_filter`
(`crates/keeppix-db/tests/timeline.rs`) copre il bug sopra. L'ho eseguito
**prima** del fix (commentando temporaneamente il filtro) e osservato
fallire:

```
thread 'geometry_omits_unknown_kind_assets_without_a_bbox_filter' panicked...
assertion `left == right` failed: un asset unknown non è una foto da
mostrare, come nella pagina (D3), anche nel percorso senza bbox
  left: 2
 right: 1
```

Poi ripristinato il filtro e rieseguito: verde (vedi sotto).

I test principali del comportamento di stack (già scritti da un tentativo
precedente, verificati riga per riga contro il brief e mantenuti):

- `crates/keeppix-api/tests/timeline.rs`:
  `timeline_collapses_a_raw_jpeg_stack_into_one_tile_with_a_badge` (una
  tessera, `stack_size=2`, `raw_kind="raw+jpeg"`, `buckets[0].count=1`,
  1 record di geometria) e
  `timeline_reports_stack_size_one_for_an_unstacked_asset`
  (`stack_size=1`, `raw_kind="jpeg"`).
- `crates/keeppix-db/tests/timeline.rs`:
  `page_collapses_a_raw_jpeg_stack_into_its_primary`,
  `page_reports_stack_size_one_for_an_unstacked_asset`,
  `geometry_collapses_a_raw_jpeg_stack_into_one_record`,
  `buckets_count_stacks_not_files`.
- `crates/keeppix-db/tests/search.rs`:
  `search_collapses_a_raw_jpeg_stack_into_its_primary`.

## Risultati dei test

```
keeppix-db  tests/timeline.rs        16 passed (incl. il nuovo test rosso→verde)
keeppix-db  tests/search.rs           8 passed
keeppix-db  tests/scale_200k.rs       2 passed (release, con EXPLAIN aggiornati)
keeppix-db  tests/scale_geometry.rs   1 passed + 3 harness (release, con EXPLAIN aggiornati)
keeppix-api tests/timeline.rs        20 passed
keeppix-api tests/search.rs           2 passed
keeppix-api tests/stacks.rs           2 passed
keeppix-api tests/openapi.rs           7 passed (snapshot rigenerato: due campi additivi)
```

`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi su tutto il workspace. `./scripts/test.sh` completo **non
eseguito** (stesso motivo del Task 2: costerebbe l'intera suite con
`cargo clean` finale); eseguiti invece tutti i test toccati dal task più le
due prove di scala a 200k con `EXPLAIN` alla mano.

## Misure di scala (200k asset, `EXPLAIN ANALYZE, BUFFERS`)

- `geometry` (vista intera): **161ms** (budget 900ms), piano `Index Only
  Scan using assets_geometry_idx ... Heap Fetches: 0` anche con i filtri
  `stack_id`/`kind` aggiunti.
- `buckets`: **81ms** (budget 300ms) con 50 permessi granulari.
- `page` (200 righe): **3ms** (budget 300ms) con 50 permessi granulari.

Il `LEFT JOIN stacks` per il filtro di primario resta a costo
sub-millisecondo anche a 200k asset (tabella `stacks` piccola): **nessuna
denormalizzazione** (`is_stack_primary` booleano su `assets`) si è rivelata
necessaria, come da preferenza del brief.

## Rulings

Tutti documentati in `.superpowers/sdd/2026-08-20-keeppix-fase-10/progress.md`
sotto "## Task 3": forma di `AssetWithStack`/`StackBadge`/`AssetStackRow`,
separazione dei frammenti SQL primario-vs-badge, `buckets` che legge da
`assets` invece di `folder_month_counts`, il bug di `kind` in `geometry`,
la misura JOIN-vs-denormalizzazione, l'estensione dell'indice 0034→0035, e
la semantica di `raw_kind` per un asset non impilato.

## Non fatto (fuori scope)

- Task 4 non iniziato, come richiesto.
- `VisibilityScope::filter_for_folder_aggregate` è rimasto inutilizzato da
  `buckets` (ora legge da `assets`, non più da `folder_month_counts` via
  quel filtro); non l'ho toccato perché è API pubblica e non fa parte di
  questo task — nessun warning di clippy lo segnala (non è dead code
  privato). Da valutare in futuro se resta senza altri chiamanti.
