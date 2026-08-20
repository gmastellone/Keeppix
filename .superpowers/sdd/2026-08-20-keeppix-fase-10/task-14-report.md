# Task 14 — Suggerimenti tipizzati e cluster con destinazione — Report

Branch: `fase-10` (nessun nuovo branch creato, verificato prima di ogni commit).
Commits: `da47ddf` (search), `3ce4c88` (fix clippy), `f37879b` (map),
`4182efa` (ledger).

## 1. `/search/suggest` tipizzato

`SearchRepo::suggest` ora restituisce `Vec<Suggestion>` (`Suggestion{kind,
value, label, color}`, `SuggestionKind` = `Tag|Camera|Folder|Iso|Year|
Country|Filename`) invece di `Vec<String>`. Sei fonti attive in un'unica
`UNION` (camera, filename, folder, year, iso, country); `tag` resta
nell'enum senza fonte (Fase 7). `country`/`folder` seguono le convenzioni
già stabilite dal Task 6 (`assets.place_id` bare, id di cartella come
`value` per poter costruire `SearchNode::Folder`). L'API layer aggiunge
`SuggestionView`/`SuggestionKindView` con `utoipa::ToSchema`; cambio di
forma intenzionale e documentato nel ledger (l'endpoint non ha ancora
consumatore frontend).

## 2. `MapClusterView` additivo

`MapCluster`/`MapClusterView` guadagnano `folder_id` (destinazione per
"Apri cartella") e `place_label` (dalla geocodifica inversa già in
`assets.place_id`). Per il percorso a griglia, `folder_id`/`place_id` sono
aggregati con lo stesso `array_agg(... ORDER BY ...)[1]` di
`cover_asset_id`, non presi da un asset qualunque del cluster — verificato
con un test dedicato che usa due asset in due cartelle/luoghi diversi nella
stessa cella, osservato rosso mutando di proposito l'`ORDER BY` prima di
fissarlo.

## Verifica

- `keeppix-db`: `search.rs` 21/21 (+2), `geo.rs` 15/15 (+1), `migrations.rs`
  11/11.
- `keeppix-api`: `search.rs` 5/5 (+1), `map.rs` 10/10 (+1), `openapi.rs`
  7/7 (snapshot rigenerato), `places.rs` 6/6 (non toccato, controprova).
- `cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
  warnings` verdi su tutto il workspace.
- Due mutazioni manuali (kind letterale rotto, `ORDER BY` disallineato)
  osservate rosse e ripristinate, per confermare che i test provano
  davvero l'assunto dichiarato (TDD, non solo test verdi).
- `./scripts/test.sh` completo non eseguito (stesso motivo dei task
  precedenti: costerebbe l'intera suite); eseguiti i test toccati.

Nessun push. Nessun lavoro su Task 15+. Ledger aggiornato in
`progress.md`.
