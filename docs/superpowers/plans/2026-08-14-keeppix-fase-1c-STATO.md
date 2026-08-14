# Fase 1c — stato di avanzamento e consegna

**Aggiornato:** 2026-08-14, chiusura della Fase 1c sul branch `fase-1`
**Piano:** [`2026-08-14-keeppix-fase-1c.md`](2026-08-14-keeppix-fase-1c.md)
**Spec:** [`../specs/fase-1c-timeline.md`](../specs/fase-1c-timeline.md)
**Design:** [`../specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md)
**1b STATO:** [`2026-08-14-keeppix-fase-1b-STATO.md`](2026-08-14-keeppix-fase-1b-STATO.md)
**PR:** bozza [#3](https://github.com/gmastellone/Keeppix/pull/3) — CI; **non mergiare**
  su `main` prima della suite complessiva stile Fase 0
**Stato:** **chiusa sul branch `fase-1`**. Non mergiata su `main`.

Questo documento è la **consegna della Fase 1c**: qui c'è ciò che serve a
riprendere il lavoro senza rileggere il ledger. Il ledger cronologico vive in
`.superpowers/sdd/2026-08-14-keeppix-fase-1c/progress.md`.

## Metodo di esecuzione

TDD in-line sul branch `fase-1`. Spec vince sul piano; le divergenze sono
nei ruling sotto. Dopo ogni suite con testcontainers i container Postgres
vengono **spenti e rimossi** (`docker rm -f`), non solo `docker container prune`.

## Avanzamento

**Fase 1c completa.** I 11 task del piano sono chiusi.

| # | Task | Stato | Commit |
|---|---|---|---|
| 1 | Trigger month counts | ✅ | `be786a8` |
| 2 | `TimelineRepo` | ✅ | `0d5b283` |
| 3 | HTTP timeline + cartelle | ✅ | `4653651` |
| 4 | Media + SPA fallback | ✅ | `ea921a8` |
| 5 | Viewport promote | ✅ | `ff2f716` |
| 6 | Ricerca AST | ✅ | `f8c3930` |
| 7 | WebSocket ticket | ✅ | `73f7227` |
| 8 | Cache sessioni | ✅ | `34a2cb2` |
| 9 | Frontend timeline | ✅ | `8b3ad9a` |
| 10 | Ricerca / viewer / problemi | ✅ | `3baf18e` |
| 11 | STATO | ✅ | questo commit |

## Ruling

1. Si resta sul branch `fase-1`. La PR #3 è **draft**; merge su `main` solo
   dopo la suite complessiva, e solo se l'utente lo chiede.
2. HLS, `rating:` in search, service worker, moka oltre la cache sessioni:
   fuori da 1c.
3. `GET /timeline` restituisce `assets` + `next_cursor`, senza flag né luogo.
4. I derivati stanno su `/media/thumb|{preview}/{hash}` e
   `/media/original/{id}`, **non** sotto `/api/v1`. Hash sconosciuto → `403`
   anche per l'admin.
5. Parser di ricerca a mano in TypeScript, senza Chevrotain (budget gzip).
6. `pg_trgm` è già in `0001`; `00010` aggiunge `saved_searches` e GIN sul
   filename.
7. Ticket WebSocket consumato in `FromRequestParts` **prima** dell'upgrade.
   Allowlist vuota = same-origin only.
8. Cache sessioni: digest del token, TTL 30 s, drop su logout/refresh.
   Una revoke di famiglia lascia i gemelli in cache fino al TTL.
9. `Problem::from(DbError::Connection)` è 503: con la cache, `/auth/me`
   dopo un outage passa da `UserRepo`.
10. `Asset.thumbhash` in hex opzionale sulla vista. Decoder frontend =
    DC + luma AC, non il port completo.
11. Job `failed` in `GET /problems` solo admin: `JobRepo` non ha proprietario.
12. Selezione multipla e pinch-density non sono in 1c (non erano nel piano
    del Task 9). Menu Album **assente**.

## Cosa non è in 1c (di proposito)

- HLS `/media/video/{id}/hls`.
- Rating persistenti, preferiti, culling, RAW, sidecar XMP (Fase 2).
- Album, sharing, link pubblici (Fase 3).
- Mappa (Fase 4).
- Upload tus (Fase 5).
- Service worker offline.
- `permessage-deflate` (feature tungstenite non abilitata).
- Fan-out WebSocket dai worker: coda e `resync` sono pinnati dai test di
  modulo; il socket oggi fa heartbeat e chiude i testi >64 KB.

## Bundle

Chunk iniziale (index.js + CSS in `index.html`): **~79 KB gzip** su un
budget di 150 KB. Timeline, search, problems, viewer sono chunk lazy.

## Come riprendere — suite di merge, poi Fase 2

1. Suite complessiva stile Fase 0 sul branch `fase-1` (vedi piano 1c,
   «A fine Fase 1»): frontend build, `cargo test --workspace -- --test-threads=1`,
   clippy `-D warnings`, `fmt --check`, `cargo deny`, `docker build` +
   compose bundled, poi `docker rm -f` dei testcontainers.
2. Analisi dei fallimenti con un modello più capace; fix con il modello
   efficiente.
3. Solo allora: togliere il draft dalla PR #3 e mergiare su `main`, se
   l'utente lo chiede. Non aprire Fase 2 prima.
