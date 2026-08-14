# SDD ledger — plan: docs/superpowers/plans/2026-08-14-keeppix-fase-1b.md

Spec: docs/superpowers/specs/fase-1b-ingestione.md
Design: docs/superpowers/specs/2026-08-13-keeppix-design.md
Branch: `fase-1` (continua dalla 1a, già su origin)
Workspace: `.superpowers/sdd/2026-08-14-keeppix-fase-1b/`

Ruling: si resta sul branch `fase-1`, in-place — stesso precedente R1 / 1a.
L'utente ha chiesto push (fatto) ed esecuzione della 1b.

Ruling: `system_capabilities` «già in Fase 0» non esiste; si usa
`system_settings` (jsonb). — *Costo:* Impostazioni 1c legge una chiave, non
una tabella.

Ruling: `AssetStatus::Indexed` dopo i metadati, non dopo i derivati — altrimenti
la timeline della 1c resta vuota finché non ci sono le thumb. Vince spec 1b §1
sul commento di dominio della 1a.

Ruling: `TestServer::start_stoppable()` per il test 503. Il container
condiviso non può essere spento: fermarlo ucciderebbe gli altri test dello
stesso binario. — *Costo:* un boot extra, solo su quel test.

## Avanzamento

| # | Task | Stato | Commit |
|---|---|---|---|
| 1 | Harness container condiviso | complete | `de75da7` |
| 2 | Migrazione `jobs` + tipi | complete | `8662b33` |
| 3 | `JobRepo` | complete | `c939aef` |
| 4 | Magic-number kind | complete | `db25913` |
| 5 | Worker pool + profili | complete | `9257bba` |
| 6 | Discovery | complete | |
| 7 | EXIF | — | |
| 8 | Hash | — | |
| 9 | Derivati | — | |
| 10 | Sandbox + poster | — | |
| 11 | Watcher, move, probe | — | |
| 12 | Integrazione + STATO | — | |

Checkpoint Task 1: `cargo test -p keeppix-db -- --test-threads=1` wall ~176 s
(era ~6–7 min). Isolamento: `two_test_databases_are_isolated` verde.

Ruling: `promote` usa `LEAST(priority, $n)` invece dello sketch `SET priority = 2`.
Altrimenti un job interactive (0) verrebbe declassato. — *Costo:* nessuno se
il chiamante passa sempre 2.

