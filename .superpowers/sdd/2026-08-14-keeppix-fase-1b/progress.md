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

## Avanzamento

| # | Task | Stato | Commit |
|---|---|---|---|
| 1 | Harness container condiviso | — | |
| 2 | Migrazione `jobs` + tipi | — | |
| 3 | `JobRepo` | — | |
| 4 | Magic-number kind | — | |
| 5 | Worker pool + profili | — | |
| 6 | Discovery | — | |
| 7 | EXIF | — | |
| 8 | Hash | — | |
| 9 | Derivati | — | |
| 10 | Sandbox + poster | — | |
| 11 | Watcher, move, probe | — | |
| 12 | Integrazione + STATO | — | |
