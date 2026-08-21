# Task 12 — L'indice che manca alla timeline — Report

## Migrazione

`0040_assets_timeline_indexed_idx.sql`:

```sql
CREATE INDEX assets_timeline_indexed_idx ON assets (taken_at_utc DESC, id DESC)
    WHERE status = 'indexed' AND kind <> 'unknown';
```

Aggiornato il commento in `keeppix-db/src/lib.rs` (requisito `sqlx::migrate!` compile-time).

## TDD — rosso → verde

Test nuovo: `timeline_page_uses_assets_timeline_indexed_idx` in `scale_200k.rs`.

**Rosso** (migrazione assente / non incorporata): asserzione fallita — piano senza
`assets_timeline_indexed_idx` (`assets_timeline_idx` o seq scan).

**Verde** (dopo `0040` + touch `lib.rs`):

```
cargo test -p keeppix-db --test scale_200k timeline_page_uses_assets_timeline_indexed_idx
test timeline_page_uses_assets_timeline_indexed_idx ... ok
```

## EXPLAIN (200k righe, `scale_200k.rs`)

### `explain_page` — query reale con vincolo di mese

Prima e dopo: il planner resta su `assets_taken_day_idx` (Task 6) + `Filter:
(kind <> 'unknown') AND (status = 'indexed')` — **Execution Time ~0.47 ms**.

### `explain_timeline_ordering` — predicati/ORDER BY di `page` su sola `assets`

Con indici concorrenti nascosti in transazione (pattern `perf_task12.rs`):

| | Indice | Execution Time |
|---|---|---|
| **Prima** | `assets_timeline_idx` + filtro `kind` | ~101 ms |
| **Dopo** | `assets_timeline_indexed_idx` (nessun filtro `kind`) | ~62 ms |

## Verifica

```
keeppix-db  migrations.rs   12/12 (+ assets_timeline_indexed_idx in performance_indexes_exist)
keeppix-db  scale_200k.rs    3/3
cargo fmt --check
cargo clippy -p keeppix-db --all-targets -- -D warnings
```
