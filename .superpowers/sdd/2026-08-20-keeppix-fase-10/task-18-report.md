# Task 18 — Misurare la geometria prima di complicarla — Report

Branch: `fase-10`. Nessun push. Task 19+ non toccati. Nessuna
frammentazione dell'endpoint.

## Misura

Metodologia in `scripts/measure-geometry-mobile.py` + `scale_geometry.rs`
(server-side in-process).

| Grandezza | Valore |
|---|---|
| Raw @ 214k | 1 284 008 byte (8 + N×6, esatto) |
| Gzip spec §2.3 | 451 KiB su record realistici (riferimento progetto) |
| Gzip sintetico | 6.7 KiB (altamente ripetitivo) … 1.05 MiB (max entropia w/h/m) |
| Server @ 200k | 591 ms in-process (`MEASUREMENT geometry`, scale_geometry.rs) |
| Client decode @ 214k | ~20–32 ms (gzip + scan `DataView`) |

**Cold-start → first layout-ready paint** (3×RTT + transfer + server + client),
profilo **Chrome Fast 3G** (1.6 Mbps, 150 ms RTT):

- Con gzip spec §2.3 (451 KiB): **3.38 s** → **supera la soglia 2 s**
- Con max-entropy (1.05 MiB gzip): 6.6 s
- Con libreria ripetitiva sintetica (6.7 KiB gzip): 1.1 s (non rappresentativa:
  l'EXIF reale varia abbastanza da avvicinarsi al riferimento spec, non al minimo
  sintetico)

## Decisione

Soglia superata sul profilo mobile simulato standard → **pianificare geometria
per mese in Fase 11** (mesi vicini + altezza stimata da `/timeline/buckets`).
**Resta whole-view in fase-10**: nessun cambiamento a
`GET /timeline/geometry`.

## Verifica

```
python3 scripts/measure-geometry-mobile.py          → output sopra
cargo test -p keeppix-db --test scale_geometry \
  geometry_of_two_hundred_thousand -- --nocapture   → 591 ms, Index Only Scan ok
```

Ledger aggiornato in `progress.md`.
