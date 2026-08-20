# Fase 10 — Superficie API per l'interfaccia — Ledger

Branch: `fase-10`
Base: `main` @ a741ad5 (post-merge Fase 5+6)
Plan: `docs/superpowers/plans/2026-08-20-keeppix-fase-10.md`
Spec: `docs/superpowers/specs/fase-10-api-interfaccia.md`

Ordine obbligatorio (PROSEGUI): Task 1 → Task 1bis → resto.
Task 1 e 1bis vanno per primi, in quest'ordine.

## Decisioni e Ruling

Ruling: `FlagRepo::batch_set_partial` + `AssetRepo::filter_visible` invece di un
loop `set` nell'API — SQL resta in `keeppix-db`, un solo INSERT per i riusciti,
i non visibili finiscono in `failed` con `Forbidden`/`NotFound`. — *Costo se
sbagliato:* un INSERT in meno di efficienza rispetto al vecchio `batch_set`
all-or-nothing; comportamento per-id corretto.

Ruling: `PermissionRepo::partition_editable_assets` è la partizione condivisa
per metadata/geotag — visibilità + editor, senza abortire al primo rifiuto.
`assert_can_edit_assets` lo riusa ma mantiene lo short-circuit admin. — *Costo
se sbagliato:* un asset viewer-only classificato male; i test di partial
success lo bloccano.

Ruling: `recalculate-timezones` espone `BulkOutcome` **senza** rimuovere
`changed_count` (contratto additivo). `failed` è tipicamente vuoto: l'operazione
è per libreria, non per lista di id; i candidati sono già filtrati dalla query.
— *Costo se sbagliato:* un fallimento per-asset in timezone non entra in
`failed` finché non si partiziona il writer; accettabile per Task 1.

Ruling: in `import-gpx`, gli asset modificabili senza match temporale **non**
compaiono né in `succeeded` né in `failed` — non è un errore §7, è assenza di
coordinate da scrivere. Solo i geotaggati entrano nel `batch_id`. — *Costo se
sbagliato:* la UI non vede "processati senza match"; può aggiungere un motivo
additivo più avanti.

Ruling: `FailureReason::Unknown` mappa Conflict/Corrupted/InsufficientStorage/
Gone e IO non classificabile — meglio un quinto caso onesto che fingere
`permission-denied`. — *Costo se sbagliato:* "Riprova" non offerto dove magari
servirebbe; preferibile al contrario.

Task 1: complete (commit 14af320, tests green)

Ruling: `autovacuum_vacuum_scale_factor = 0.05` su `assets` — 5% dead tuples
invece del default 0.2; su librerie grandi la VM map resta fresca senza
aspettare un quinto della tabella. — *Costo se sbagliato:* più cicli
autovacuum su una tabella calda; accettabile per index-only scan affidabile.

Ruling: compose default = profilo SSD/NVMe (`random_page_cost=1.1`, …);
microSD override via `.env`. Valori misurati all'installazione (Fase 7),
non cablati come universali. — *Costo se sbagliato:* profilo sbagliato per
metà degli utenti finché non ricalibrano.

### EXPLAIN Task 1bis (15k righe `indexed`, testcontainer Postgres 17)

`random_page_cost=4.0` — timeline page (mese 2015-06, LIMIT 200):

```
Limit → Sort → Nested Loop → Bitmap Heap Scan on assets
  → Bitmap Index Scan on assets_timeline_idx
  → Index Only Scan folders_pkey
```

`random_page_cost=1.1` — timeline page: **stesso piano** (indice già preferito
con 720 righe nel mese su 15k totali).

`random_page_cost=4.0` — geometry stand-in (`folder_id, taken_at_utc DESC, id DESC`):

```
Limit → Sort → Seq Scan on assets (Filter: status = 'indexed')
```

`random_page_cost=1.1` — geometry stand-in: **stesso piano** (Seq Scan — indice
covering non esiste ancora, Task 2).

Task 1bis: complete (commit 376f030 + ledger cbccdd0, tests green)
