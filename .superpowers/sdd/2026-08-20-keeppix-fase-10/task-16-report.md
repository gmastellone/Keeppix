# Task 16 — Report

Branch: `fase-10`. Nessun push, nessun task successivo toccato.

## Cosa è stato costruito

Fase 9 (rinomina di massa) non esiste ancora nel codice, quindi
l'infrastruttura richiesta (`operation_id`, avanzamento sul WebSocket,
`cancel` con esito parziale) è generica e agganciata all'unico long-op reale
disponibile: la scansione di libreria.

- `operations` (migrazione `0042`) + `OperationsRepo`: `create`, `find`
  (`Forbidden`, non `NotFound`, su id non visibile), `list_running`,
  `set_total`/`set_phase` (pipeline), `record_success`, `is_cancel_requested`,
  `request_cancel` (owner/admin), `finish_cancelled`/`finish_done`.
- `discover::run_with_operation`: segue un `OperationId` opzionale, imposta
  `total` (onestamente `None` alla prima importazione), registra ogni asset
  riuscito, controlla `cancel_requested` per file dentro `flush_batch`.
  Cancellare a metà lascia gli asset già scritti e chiude come `Cancelled`
  con l'elenco esatto — nessun rollback.
- `dispatch`/`watch`: il worker passa `operation_id` dal payload del job;
  `enqueue_rescan_with_operation` condivide la `dedup_key` `discover:{id}`
  con `enqueue_rescan` e segnala se il job accodato porta davvero il nostro
  id (altrimenti un'altra richiesta ha vinto il dedup).
- `POST /api/v1/libraries/{id}/scan`: apre un'operazione solo se non c'è già
  un job pending/running per quella libreria; altrimenti `operation_id:
  null` invece di un'operazione orfana che nessun job farebbe avanzare.
- `POST /api/v1/operations/{id}/cancel`: chiede l'annullamento e risponde
  con `BulkOutcome` (stesso involucro del Task 1) contenente ciò che era già
  applicato in quel momento.
- WebSocket: `drain_operations` sul poll già esistente (1 s) emette
  `operation.progress {operation_id, done, total, phase}` quando lo stato
  cambia, più un messaggio finale col `phase` terminale quando l'operazione
  esce da `running`. Nessuna memoria fra connessioni: un client riconnesso a
  metà vede lo stato corrente al primo giro utile.

## Verifica

`keeppix-jobs` (100% del crate) + `keeppix-db` (`operations.rs`,
`migrations.rs`) + `keeppix-api` (100% del crate, inclusi `scan.rs`, `ws.rs`,
`openapi.rs`) tutti verdi. `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo deny check bans` verdi. Sei mutazioni
manuali (dettagliate nel ledger) confermano che i nuovi test falliscono
davvero se la proprietà che dichiarano regredisce.

`./scripts/test.sh` completo non eseguito (costoso; sostituito dalle suite
complete dei crate toccati, come nei task precedenti).

Dettagli e Ruling completi in `progress.md`.
