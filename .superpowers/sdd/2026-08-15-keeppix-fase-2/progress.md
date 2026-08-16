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

Ruling (Task 1): estrazione manuale (TIFF IFD walker + scanner ISO-BMFF),
non `rawler`/`rawloader` — non provati affatto: i formati richiesti (ARW,
NEF, CR2, CR3, DNG) sono tutti risolvibili con la stessa manciata di tag
TIFF standard (`JPEGInterchangeFormat`/`Length` a 0x0201/0x0202,
`StripOffsets`/`StripByteCounts` a 0x0111/0x0117 con `Compression` 6/7) più
un box `PRVW` per CR3, verificati byte a byte sui 5 fixture reali. `rawler`
aggiungerebbe ~20 dipendenze transitive (rayon, jxl-oxide, ...) per un
problema che l'AGENTS.md chiede di non complicare oltre il necessario, e
comunque non useremmo la sua pipeline di demosaic in questo task. Costo se
sbagliato: coprire un sesto formato (ORF/RAF, fuori dai test del piano)
richiederebbe scoprirne la posizione dei tag standard con lo stesso
approccio — nessun cambio di libreria.

Ruling (Task 1): un candidato è "preview valida" solo se comincia con SOI
(`FFD8`) *e* il suo primo marker SOF è baseline/progressivo (0xC0/C1/C2),
non lossless (0xC3) né aritmetico. Verificato sul CR2 di test: la IFD dello
strip Bayer (compressione lossless-JPEG, `Compression=6` proprio come la
preview) supera il controllo SOI ma viene scartata qui — altrimenti
`extract_from_tiff` avrebbe potuto scegliere 4.4 MB di dati Bayer grezzi
spacciandoli per la preview solo perché più "larga" sulla carta. Costo se
sbagliato: un file derivato rotto o enorme al posto della preview, silenzioso
fino al rendering.

Ruling (Task 1): fixture reali scelti dal dataset pubblico
`raw.pixls.us` (CC0, verificato via `json/getrepository.php?set=all`),
non da elysiatools.com (troppo grandi, 6-64 MB, licenza non verificabile
per-file). Selezionati i file più piccoli CC0 con una preview di
risoluzione ragionevole per il proprio formato: ~2-15 MB l'uno, 24 MB
totali. Nessun formato omesso: ARW, NEF, CR2, CR3, DNG tutti procurati e
testati con file veri (nessun `#[ignore]` necessario). ORF e RAF (in spec
§2.3 ma non nei test del piano) non procurati — fuori dallo scope dei test
richiesti da questo task.

Ruling (Task 1): la stima "30-80ms" della spec (§2, fase-2-raw-culling.md)
va corretta. Misurato in release, decodifica header esclusa dal disco (file
già in cache): 1.1-5.4ms per estrazione, un ordine di grandezza sotto la
stima. La stima della spec probabilmente includeva la lettura da disco di
RAW reali (30-80 MB, non i 2-15 MB dei fixture) da storage non a caldo — da
verificare con file di dimensione realistica quando il job Task 3 leggerà
da NAS reale, non necessario per questo task che opera su byte già in RAM.

## Avanzamento

| # | Task | Stato | Commit |
|---|---|---|---|
| 0 | Harness PortNotExposed retry | complete | `319e9e5` |
| 1 | Preview RAW incorporata | complete | `d5db5d6` |
| 2 | `derive_from_bytes` | complete | `55e5e70` |
| 3 | Job DeriveRaw | — | |
| 4 | overrides + flags | — | |
| 5 | Sidecar XMP | — | |
| 6 | Stack RAW+JPEG | — | |
| 7 | Cestino a tre opzioni | — | |
| 8 | Duplicati + batch | — | |
| 9 | Frontend culling | — | |

### Task 1: complete (commit `d5db5d6`, test verdi)

`cargo test -p keeppix-media --test raw` → 8 passed (7 test + 1 misura).
`cargo clippy --workspace --all-targets -- -D warnings` e
`cargo fmt --check` verdi su tutto il workspace.

**MEASUREMENTS** (`cargo test -p keeppix-media --test raw --release --
--nocapture`, file già letti da cache disco, MacBook Apple Silicon):

| Formato | Fixture (fonte reale) | Tempo | Preview scelta | Byte |
|---|---|---|---|---|
| ARW | Sony ILCE-7S, 14bit compresso | 3.33 ms | 1616×1080 | 734118 |
| NEF | Nikon D70s, 12bit lossy type 1 | 1.41 ms | 3008×2000 | 717588 |
| CR2 | Canon EOS 40D, sRAW2 | 2.63 ms | 1936×1288 | 366439 |
| CR3 | Canon EOS R6, 3:2 | 5.35 ms | 1620×1080 | 389450 |
| DNG | Adobe DNG Converter, EOS 5D III, lossy JPEG | 1.10 ms | 3960×2640 | 569249 |

Copertura al passo «preview trovata»: 5/5 formati richiesti (100%), con
file reali non sintetici. Spec §2 corretta a 1–6 ms (misura reale).

Task 1 review: APPROVED_WITH_NOTES — spec timing aggiornata in
`docs: correct Fase 2 embedded-preview timing from measured data`.

### Task 2: complete (commit `55e5e70`, test verdi)

`cargo test -p keeppix-media` → 23 passed (22 precedenti + 1 nuovo).
`cargo clippy -p keeppix-media --all-targets -- -D warnings` verde.

Ruling (Task 2): fixture `sample.jpg` del brief assente in
`tests/fixtures/` — usato `tiny.jpg` già presente (stesso ruolo: JPEG
piccolo per test derivati). Costo se sbagliato: nessuno, stessa pipeline.

Estratto `derive_from_bytes(bytes, data_dir, hash)` da `derive_jpeg`;
controllo idempotenza (`thumb.is_file()`) duplicato in entrambe le entry
point come da brief.
