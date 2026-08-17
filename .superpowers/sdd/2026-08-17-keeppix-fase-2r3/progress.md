# SDD ledger — plan: docs/superpowers/plans/2026-08-17-keeppix-fase-2r3.md

Branch: `fase-2r3`
Base: `main` @ `8b25ee3` (merge di fase-2r2, verificata sull'archivio reale)
Spec di riferimento: docs/superpowers/specs/2026-08-13-keeppix-design.md
Vincolo: Raspberry Pi 5, 8 GB, 200.000 foto.

Ruling: lavoro in-place sul branch `fase-2r3`, non in un worktree. Il
checkout cloud è isolato; l'utente ha chiesto il branch da `main`, non
un worktree. Costo se sbagliato: spostare il branch, un comando.

Nessun push/PR/merge. Field test sull'archivio reale: lo esegue
l'operatore (rapporti derivati, zoom RAW, thumbhash stabile).

## Scansione pre-volo

Task 2 dopo Task 1: alzare l'anteprima a 2048 restando lossless
triplicherebbe lo spazio. Task 7 dopo 4 e 6 *nel senso del piano*, ma
la guardia va **scritta mentre i due difetti sono ancora nel repo** —
quindi: RED della guardia prima di cablare 4 e 6, poi 4 e 6, poi
GREEN della guardia.

Task 3, 5, 8 indipendenti. Task 2 richiede riproduzione nello zoom del
culling prima della correzione.

---

## Task 1 — WebP con perdita

**RED osservato:** `derived_preview_uses_lossy_vp8_not_vp8l` trovava chunk
`VP8L` (byte `VP8L`). `derived_preview_is_under_one_third_of_lossless`
aveva preview = lossless = 278244 sul pattern sintetico.
`webp_quality_changes_the_preview_size` dava q40 = q90.

Ruling: dipendenza `webp = { version = "0.3.1", default-features = false }`
(lock: `libwebp-sys 0.9.6`). È l'unico crate citato dal piano che espone
l'encoder con perdita di libwebp; `default-features = false` evita il
crate `image`. Motivo: `image-webp` fa solo VP8L, e i derivati a 1440 px
lossless pesavano 1,54 MB/foto (308 GB su 200k). Costo se sbagliato: una
CVE in libwebp tocca l'encoder in-process — vedi deroga sotto.

Ruling (deroga decoder-in-sandbox): libwebp gira **in-process**. La regola
di AGENTS.md copre la **decodifica** di input non fidato (ffmpeg/libraw).
Qui si **codifica** un buffer RGB8 che abbiamo già decodificato noi
(zune-jpeg o demosaic sandboxato). L'input di libwebp sono i nostri byte,
non i byte dell'utente. Costo se sbagliato: un bug dell'encoder C può
abortire il worker; non legge l'archivio.

Ruling: qualità unica 82 per thumb e preview (`KEEPPIX_WEBP_QUALITY`,
`Config.webp_quality`, `set_webp_quality` all'avvio). Il piano permette
valori diversi; due manopole senza misura sono YAGNI. Costo se sbagliato:
la thumb a 240 px potrebbe stare a ~70 con un po' meno peso — irrilevante
sul Pi rispetto all'anteprima.

Ruling: `image-webp` resta solo come `[dev-dependencies]` per il
confronto lossless nel test di soglia. Non è nel binario.

Ruling: il test «< 1/3» sul pattern sintetico ad alta frequenza dava
lossy 96240 vs lossless 278244 (34,6%, sopra 1/3). Il piano calibra la
soglia sulle foto, non sul worst-case DCT. Il test usa la preview
incorporata di `sample.arw`. Costo se sbagliato: un regress al lossless
su foto reali resta intercettato; uno su checkerboard no.

Ruling: `libwebp-sys` è C, quindi `[profile.dev/test.package.libwebp-sys]
opt-level = 2` come `keeppix-media`. Senza, a -O0 il lossy era 1,06 s
contro 0,48 s del lossless Rust — misura di rustc, non di produzione.

### Misure — stessa fixture, test isolato (`--exact`)

Preview JPEG incorporata di `sample.arw` → `derive_from_bytes`.

| | test (opt-level 2) | release |
|---|---|---|
| **Prima** (VP8L) | 480,6 ms, ΔRSS 19,6 MB, preview 1 940 932 B | 44,8 ms, ΔRSS 18,9 MB |
| **Dopo** (q82) | 574,7 ms, ΔRSS 19,9 MB, preview 213 048 B | 136,0 ms, ΔRSS 19,3 MB |

L'attesa «lossy più veloce, RAM uguale»: **la RAM è uguale** (il picco è
il buffer RGB). **Il tempo no**: in release il lossy è ~3× più lento
(136 ms vs 45 ms). Vince la misura. Su 200k foto sono ~7,5 h vs ~2,5 h
di sola encode; lo spazio scende da ~1,94 MB a ~0,21 MB di preview
(più thumb 9 KB vs 66 KB). Non si alza un budget: non c'era un tetto
di tempo sul derive, solo l'attesa smentita.

### Docker

`docker compose build` con overlayfs nested è fallito (`invalid argument`
sui mount overlay). Con `storage-driver=vfs` + BuildKit l'immagine
`keeppix:dev` **si costruisce**: `webp 0.3.1` e `keeppix-media` compilano
nello stage `rust:1.88-bookworm`, il binario sta in distroless. Tempo
a freddo (pull rust + cargo release): **184 s**, di cui cargo 2 m 45 s.
Baseline senza libwebp non misurata: il primo tentativo è morto su
`/usr/bin/time` assente prima del cambio di encoder. Costo CI ricorrente
stimato dal compile locale di `libwebp-sys`: ~1 min.

Verifica: `npm ci && npm run build`, `cargo fmt --check`,
`clippy -D warnings`, `./scripts/test.sh` — verdi.
`docker compose build` (vfs+BuildKit) — `keeppix:dev` in 184 s.

Task 1: complete (commit `8dc61f5`, test verdi, immagine Docker costruita)

---

**RED osservato (Task 3):**
`a_duplicate_hashed_after_the_first_derive_still_gets_the_thumbhash` —
il secondo asset restava `thumbhash = NULL`.

Ruling: `AssetRepo::propagate_thumbhash_for_hash` (SQL del piano, con
`assets.` sul WHERE: Postgres 42702 «column reference thumbhash is
ambiguous» senza qualifica). Chiamata sul ramo idempotente di
`raw::run_with`. Stessa eccezione `AuthContext` di `set_thumbhash_for_hash`
(pipeline). Costo se sbagliato: un JPEG duplicato via `derive_asset` ha
lo stesso buco — differito, il piano parla solo di `derive_raw`.

Task 3: complete (commit `33b0499`, test raw verdi)

---

## Task 7 — guardia CI (RED, prima di cablare 4 e 6)

**RED osservato** (`python3 scripts/check-wired.py`, exit 1):

```
public functions with no production caller:
  keeppix-db::cleanup_expired (crates/keeppix-db/src/trash.rs)
mounted routes with no frontend consumer:
  /ws/ticket
  /ws
```

Sono i due difetti veri (Task 4 e 6), non un caso costruito.

Ruling: lo script ignora `*.spec.ts` / `*.spec.vue` e i moduli
`#[cfg(test)]`, come i test Rust — un test che chiama la funzione è
esattamente ciò che la produzione non fa. Costo se sbagliato: una rotta
citata solo in un spec (oggi `/auth/refresh`) non conta come cablata.

Ruling: le altre funzioni/rotte senza chiamante non sono Task 4/6.
Eccezioni in `scripts/wired-exceptions.txt` con la fase che le consumerà
(o `ops`/`ci` per `/health` e `/api/openapi.json`, che non passano dalla
SPA). Costo se sbagliato: restano morte e la CI è verde. Voci già
documentate (`get_or_create_secret` → Fase 6, `ping` → Fase 1) non si
ricablano qui.

Ruling: `ChangeLogRepo::since` è eccezione `fase-2r3` — è la fonte
eventi del WS, da consumare nel Task 6. Se il Task 6 non la tocca, si
toglie l'eccezione o si sposta la fase. Costo se sbagliato: il WS si
collega e non emette cambiamenti.

La CI ha lo step; resta rossa finché 4 e 6 non cablano i tre nomi
sopra. GREEN dopo quei task.

---

