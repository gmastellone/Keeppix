# Task 19 — Il protocollo WebSocket: da due eventi a nove — Report

Branch: `fase-10`. Nessun push. Task 20+ non toccati.

## Cosa emette ora il canale

| Evento | Fonte reale | Note |
|---|---|---|
| `assets.upserted` / `assets.deleted` | `change_log` | invariato |
| `operation.progress` | `operations` | invariato (Task 16) |
| `scan.progress` | `scan_phase()` (condivisa con `GET /libraries/{id}/scan`) + `AssetRepo::count_in_library` | **nuovo**, copre anche le riscansioni del watcher senza `operation_id` |
| `problems.changed` | firma su `ProblemsRepo::list` | **nuovo**, magro: solo un `count` |
| `asset.derivative.ready` | `jobs` (`kind='transcode_video', status='done'`) filtrato per visibilità | **nuovo** |
| `backup.finished` | `backup_runs` (`BackupRepo::list_runs`), admin-only | **nuovo** |

`analysis.progress`, `suggestions.changed`, `culling.changed` **non sono
cablati**: nessun codice di Fase 7/8 esiste da cui leggerli (Ruling nel
ledger). `storage.changed` resta fuori: `GET /bootstrap` (Task 17) copre già
lo stesso bisogno.

## Come sono cablati i quattro nuovi

Stesso disegno degli esistenti (poll ogni 1s dentro `socket_loop`, stato
"visto" per connessione, `enqueue`/`resync` su overflow) — nessun canale
in-process nuovo:

- **`scan.progress`**: per ogni libreria visibile, `(phase, asset_count)`;
  emette solo se cambiato dall'ultimo giro.
- **`problems.changed`**: firma ordinata sui tre secchi di `ProblemsRepo`;
  emette un `count`, mai gli id (contratto "canale di notifica").
- **`asset.derivative.ready`**: `JobRepo::list_recently_done` (nuovo) con un
  cursore per connessione inizializzato da `JobRepo::max_done_id` (nuovo) —
  un client che si connette ora non rivede transcodifiche vecchie. Filtro
  `AssetRepo::filter_visible` prima di emettere.
- **`backup.finished`**: ultimo run da `BackupRepo::list_runs`; emette solo
  alla transizione fuori da `running`; non-admin esce presto (niente
  `Forbidden` che romperebbe il socket).

## Verifica

```
cargo test -p keeppix-db --test jobs                    → 14/14 (+1)
cargo test -p keeppix-api --test ws                      → 8/8 (+4)
cargo fmt --check                                         → verde
cargo clippy --workspace --all-targets -- -D warnings     → verde
cargo deny check bans                                     → verde (bans ok)
```

Quattro mutazioni manuali (`if false && drain_x(...)` su ognuno dei nuovi
emettitori) osservate **rosse**, poi ripristinate — ogni test prova
davvero il proprio emettitore, non solo che il socket resta vivo.

Difetto preesistente osservato e **non toccato** (differito, vedi ledger):
`bootstrap_emits_no_more_queries_than_individual_repos` fallisce quando gira
insieme all'altro test di `tests/bootstrap.rs` nello stesso binario;
riprodotto anche su `HEAD` prima di questo task via `git stash`. Isolato
passa sempre.

Ledger aggiornato in `progress.md` (4 Ruling + 1 nota sul test preesistente
adattato + 1 nota sul difetto differito).
