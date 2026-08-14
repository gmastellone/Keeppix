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

Ruling: `GET /timeline` restituisce `assets` + `next_cursor`, senza flag né
luogo. `asset_flags` è Fase 2; `assets.location` non è ancora sul tipo
`Asset`. Aggiungerli nella stessa risposta quando quei campi esistono, senza
un secondo round-trip — costo se sbagliato: il client 1c deve rifare una
chiamata in 2/4.

Ruling: i derivati stanno su `/media/thumb|{preview}/{hash}` e
`/media/original/{id}`, **non** sotto `/api/v1`. La tabella della spec §3 li
elenca con gli altri endpoint; §3.1 e il design chiedono URL cacheabili e
l'esclusione SPA di `media/` e `dav/`. Un hash sconosciuto è `403` anche per
l'admin: non è un oracolo di esistenza del contenuto.

Ruling: parser di ricerca a mano in TypeScript, **senza Chevrotain**. Lo spec
cita urocissa; il budget gzip 150 KB del chunk iniziale vince. Costo se
sbagliato: query esotiche mal parseate, si può sostituire il parser senza
toccare l'AST JSON.

Ruling: `pg_trgm` è già in `0001`; `00010` aggiunge `saved_searches` e un
indice GIN sul filename.

## Avanzamento

| # | Task | Stato | Commit |
|---|---|---|---|
| 1 | Trigger month counts | complete | `be786a8` |
| 2 | `TimelineRepo` | complete | `0d5b283` |
| 3 | HTTP timeline + cartelle | complete | `4653651` |
| 4 | Media + SPA fallback | complete | `ea921a8` |
| 5 | Viewport promote | complete | `ff2f716` |
| 6 | Ricerca | complete | |
| 7 | WebSocket | — | |
| 8 | Cache sessioni | — | |
| 9 | Frontend timeline | — | |
| 10 | Ricerca / viewer / problemi | — | |
| 11 | STATO | — | |
