# SDD ledger — plan: docs/superpowers/plans/2026-08-14-keeppix-fase-1c.md

Spec: docs/superpowers/specs/fase-1c-timeline.md
1b STATO: docs/superpowers/plans/2026-08-14-keeppix-fase-1b-STATO.md
Branch: `fase-1`
PR bozza: https://github.com/gmastellone/Keeppix/pull/3 (CI; merge a fine Fase 1)
Workspace: `.superpowers/sdd/2026-08-14-keeppix-fase-1c/`

Ruling: si resta sul branch `fase-1`, in-place. La PR #3 è **draft** perché
la CI giri; l'utente ha chiesto i test complessivi stile Fase 0 **prima**
del merge su `main`, a fine Fase 1 (dopo 1c), non ora.

Ruling: HLS, `rating:` in search, service worker, moka oltre la cache
sessioni — fuori da 1c (vedi piano «Cosa NON è in 1c»).

Ruling: dopo ogni suite con testcontainers si spengono e si rimuovono i
container. L'utente l'ha chiesto esplicitamente (disco).

## Avanzamento

| # | Task | Stato | Commit |
|---|---|---|---|
| 1 | Trigger month counts | complete | `be786a8` |
| 2 | `TimelineRepo` | — | |
| 3 | HTTP timeline + cartelle | — | |
| 4 | Media + SPA fallback | — | |
| 5 | Viewport promote | — | |
| 6 | Ricerca | — | |
| 7 | WebSocket | — | |
| 8 | Cache sessioni | — | |
| 9 | Frontend timeline | — | |
| 10 | Ricerca / viewer / problemi | — | |
| 11 | STATO | — | |
