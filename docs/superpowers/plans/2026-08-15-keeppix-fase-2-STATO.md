# Fase 2 — stato di avanzamento e consegna

**Aggiornato:** 2026-08-16, chiusura della Fase 2 sul branch `fase-2`
**Piano:** [`2026-08-15-keeppix-fase-2.md`](2026-08-15-keeppix-fase-2.md)
**Spec:** [`../specs/fase-2-raw-culling.md`](../specs/fase-2-raw-culling.md)
**Design:** [`../specs/2026-08-13-keeppix-design.md`](../specs/2026-08-13-keeppix-design.md)
**1c STATO:** [`2026-08-14-keeppix-fase-1c-STATO.md`](2026-08-14-keeppix-fase-1c-STATO.md)
**Branch:** `fase-2` (da `main` aggiornato). **Non mergiare** senza conferma.
**Stato:** **chiusa sul branch `fase-2`**. I 9 task (+ harness PortNotExposed) sono
chiusi. Suite workspace verde in cloud (Postgres locale via
`KEEPPIX_TEST_DATABASE_URL`; Docker daemon assente).

Questo documento è la **consegna della Fase 2**: qui c'è ciò che serve a
riprendere il lavoro senza rileggere il ledger. Il ledger cronologico vive in
`.superpowers/sdd/2026-08-15-keeppix-fase-2/progress.md`.

## Metodo di esecuzione

TDD in-line sul branch `fase-2`. Spec vince sul piano; le divergenze sono
nei ruling sotto e nel ledger.

## Avanzamento

**Fase 2 completa** (implementazione). Criteri di completamento misurabili
in ambiente reale (sessione ≥100 foto, hash RAW pre/post) restano da
confermare sul NAS dell'utente; CI sulla PR ancora da far girare.

| # | Task | Stato | Commit chiave |
|---|---|---|---|
| 0 | Harness PortNotExposed retry | ✅ | `319e9e5` |
| 1 | Preview RAW incorporata | ✅ | `d5db5d6` |
| 2 | `derive_from_bytes` | ✅ | `55e5e70` |
| 3 | Job DeriveRaw + cascade | ✅ | `86a8a3e` (+ thumbhash `bcdde13`) |
| 4 | overrides + flags | ✅ | `6a17f4b` (+ undo NULL `1949a5e`) |
| 5 | Sidecar XMP | ✅ | `af00600`…`7dcb4e0` |
| 6 | Stack RAW+JPEG | ✅ | `8a3308f` |
| 7 | Cestino a tre opzioni | ✅ | `3edb207`…`04e8cb6` |
| 8 | Duplicati + batch API | ✅ | `49a2068`…`91d8ed1` |
| 9 | Frontend culling | ✅ | `62b6689` |
| — | ffmpeg sandbox AS 1 GiB | ✅ | `20eafc6` (fix pre-esistente 1b) |

Migrazioni nuove (prefisso a 4 cifre): `0012` overrides/flags, `0013` stacks,
`0014` trash.

## MEASUREMENTS (registrati nel ledger)

| Cosa | Valore |
|---|---|
| Estrazione preview RAW (release, fixture cached) | **1.1–5.4 ms**/formato (ARW/NEF/CR2/CR3/DNG) |
| Preview utilizzabile senza demosaic | **5/5** fixture CC0 raw.pixls.us |
| Spec §2 corretta | da «30–80 ms» a **1–6 ms** |
| Batch flags 5000 apply (release) | **~57 ms** |
| Bundle iniziale gzip | **~80 KB** / 150 KB (chunk culling lazy) |
| Floor `RLIMIT_AS` ffmpeg poster (Ubuntu 6.1) | **~800 MiB**; tetto codice **1 GiB** |

## Ruling principali (sintesi)

1. Estrazione preview **manuale** (TIFF IFD + BMFF `PRVW`), non `rawler`.
2. Preview valida solo se SOI + SOF baseline/progressivo (non lossless Bayer).
3. Cascade DeriveRaw: long side ≥1440 → `derive_from_bytes` (conteggio
   chiamate sandbox, non timing); altrimenti `dcraw_emu` half-size;
   fallimento → `set_error`, job `Ok(())`.
4. Identità asset invariata; overrides immutano solo `asset_overrides`.
5. XMP: `quick-xml`, campi estranei preservati, scrittura atomica.
6. Stack: regola 1 (stesso basename); regola 2 (2s/camera) **differita**.
7. Trash: tre opzioni; walker esclude `.keeppix-trash/`.
8. Culling: unico ingresso da header Timeline (niente cartella/selezione
   ancora); zoom 1:1 = preload originale + CSS crop (RAW puro non
   decodifica in browser).
9. `RLIMIT_AS` video portato a 1 GiB: 512 MiB bastava a ffprobe ma non a
   ffmpeg distro (VA delle shared libs).

## Cosa resta aperto / differito

- Native WS `Authorization` senza Origin (Fase app).
- Spec grouping rule 2 per stack (2s + camera + shot#).
- Flag-only → niente enqueue XMP sweep (solo override).
- `TrashRepo::cleanup_expired` non schedulato.
- Culling senza UI cartella/selezione multipla.
- Zoom 1:1 ad alta risoluzione per RAW puri (serve crop server-side).
- Smoke compose bundled / CI PR su questa fase.
- Conferma manuale: sessione ≥100 foto; hash RAW invariati dopo editing.

## Verifica (2026-08-16, cloud)

```
frontend: npm ci && npm run build     → ok; gzip ingresso ~80 KB
npx vitest run                        → 35/35
cargo fmt --check                     → ok
cargo clippy --workspace --all-targets -- -D warnings → ok
KEEPPIX_TEST_DATABASE_URL=… \
  cargo test --workspace --jobs 1 -- --test-threads=1 → tutte le suite ok
  (incluso keeppix-media --test video)
```

`./scripts/test.sh` non usato in cloud: fa `cargo clean` e assume Docker
per il cleanup testcontainers; qui Postgres è un servizio locale.

## Prossimo passo

Review umana → PR draft `fase-2` → `main` (CI) → merge solo se chiesto.
**Non aprire Fase 3** finché non è esplicitamente richiesto.
