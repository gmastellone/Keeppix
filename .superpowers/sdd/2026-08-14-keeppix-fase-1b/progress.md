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
| 6 | Discovery | complete | `e4e69b6` |
| 7 | EXIF | complete | `a84cd10` |
| 8 | Hash | complete | `1cba2ed` |
| 9 | Derivati | complete | `34d8dd3` |
| 10 | Sandbox + poster | complete | `d50d158` |
| 11 | Watcher, move, probe | complete | `6ec3037` |
| 12 | Integrazione + STATO | complete | `bd915e0` |

Checkpoint Task 1: `cargo test -p keeppix-db -- --test-threads=1` wall ~176 s
(era ~6–7 min). Isolamento: `two_test_databases_are_isolated` verde.

Ruling: `promote` usa `LEAST(priority, $n)` invece dello sketch `SET priority = 2`.
Altrimenti un job interactive (0) verrebbe declassato. — *Costo:* nessuno se
il chiamante passa sempre 2.

Ruling: sandbox `rlimit` su Unix, niente seccomp in 1b. Interfaccia
`run(program, args, memory_bytes, cpu_secs)`. — *Costo:* un figlio Linux
non è filtrato da seccomp; upgrade con `libseccomp`.

Ruling: uno spostamento è stesso `(content_hash, size)` **e** il file vecchio
non è sul disco. Due copie presenti non si marcanono `offline`. File
`moves.rs` (non `r#move.rs`). — *Costo:* un rename non rilevato se il path
vecchio è ancora un file (non è un rename).

Ruling: NFS solo da `/proc/mounts` su Linux; macOS resta Native. —
*Costo:* un mount NFS su macOS usa FSEvents, che sui network FS è inaffidabile,
e cade sul polling solo se si forza `WatcherMode::Polling`.

Ruling: `LibraryRepo::list_for_scan` senza `AuthContext` — il watcher all'avvio
non agisce per conto di un utente. — *Costo:* un'eccezione in più.

Ruling: encode WebP lossless (`image-webp` non ha q78). — *Costo:* thumb più
pesanti del q78 della spec.

Task 9: complete (commit `34d8dd3`)
Task 10: complete (commit `d50d158`)
Task 11: complete (commit `6ec3037`)
Task 12: complete (commit `bd915e0`)

Fixture (macchina di close): wall 818 ms / 3 file (~272 ms/file), metadata
<1 ms, derive 5 ms su 64×64. Numeri del fixture, non del TB. Vedi STATO.md.

