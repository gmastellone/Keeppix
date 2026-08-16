# SDD ledger — plan: docs/superpowers/plans/2026-08-15-keeppix-fase-2.md

Spec: docs/superpowers/specs/fase-2-raw-culling.md
Branch: `fase-2`
Workspace: `.superpowers/sdd/2026-08-15-keeppix-fase-2/`

Ruling: si lavora in-place sul branch `fase-2` (checkout da main aggiornato),
non in un worktree separato — l'utente l'ha chiesto esplicitamente.

Ruling: retry con backoff su `get_host_port_ipv4` in tutti e tre gli harness
(db, api, jobs) — PortNotExposed flake in locale con Docker Desktop; CI non
lo vede perché usa il service container. Costo se sbagliato: ritardi di boot
fino a ~4 s nel caso peggiore, invece di fallimenti casuali.

## Avanzamento

| # | Task | Stato | Commit |
|---|---|---|---|
| 0 | Harness PortNotExposed retry | in corso | |
| 1 | Preview RAW incorporata | — | |
| 2 | `derive_from_bytes` | — | |
| 3 | Job DeriveRaw | — | |
| 4 | overrides + flags | — | |
| 5 | Sidecar XMP | — | |
| 6 | Stack RAW+JPEG | — | |
| 7 | Cestino a tre opzioni | — | |
| 8 | Duplicati + batch | — | |
| 9 | Frontend culling | — | |
