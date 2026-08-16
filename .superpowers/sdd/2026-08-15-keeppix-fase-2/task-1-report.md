# Task 1 report — Estrazione della preview incorporata nei RAW

## Cosa ho implementato

`crates/keeppix-media/src/raw.rs`:

- `extract_embedded_preview(path: &Path) -> Result<Option<RawPreview>, RawError>`
- `RawPreview { bytes, width, height, source: PreviewSource }`,
  `PreviewSource::{Embedded, Demosaic}` (quest'ultimo esiste solo per il
  contratto dell'interfaccia, non prodotto da questo modulo), `RawError::{Io,
  Unsupported, Corrupt}`.
- Legge l'intero file, rileva TIFF (`II*\0`/`MM\0*`) o CR3/ISO-BMFF
  (`ftyp`+brand `crx `), poi:
  - **TIFF (ARW/NEF/CR2/DNG)**: cammina la catena di IFD partendo dalla
    prima IFD, seguendo anche i tag puntatore `SubIFDs` (0x014A, multipli) e
    `ExifIFD` (0x8769). In ogni IFD raccoglie un candidato se trova la
    coppia `JPEGInterchangeFormat`/`Length` (0x0201/0x0202) oppure
    `StripOffsets`/`StripByteCounts` (0x0111/0x0117) a strip singolo con
    `Compression` 6 o 7.
  - **CR3**: scansiona l'intero file per il marker ASCII `PRVW` (il box vive
    in un `uuid` di primo livello, non dentro `moov` come suggerito da una
    fonte secondaria — verificato sui byte reali, vedi sotto) e legge
    l'header a lunghezza fissa documentato da `lclevy/canon_cr3`.
  - Ogni candidato è accettato solo se comincia con SOI (`FFD8`) *e* il suo
    primo marker SOF è 0xC0/0xC1/0xC2 (baseline/progressivo): scarta gli
    strip Bayer compressi con schemi "JPEG-like" ma non lossless-JPEG (SOF3),
    che altrimenti supererebbero un controllo ingenuo "inizia con FFD8".
  - Tra i candidati validi si prende quello con il lato lungo (`max(w,h)`)
    maggiore.
- Limiti di sicurezza: `MAX_PREVIEW_BYTES = 64 MiB` (rifiuta dimensioni
  dichiarate assurde prima di allocare/copiare), `MAX_IFDS = 64` e
  `MAX_SUBIFD_FANOUT = 16` (anti-ciclo/DoS su file corrotti), `MAX_BMFF_BOXES
  = 1024` per il walker dei box di primo livello.
- Nessun `unwrap`/`expect` in `raw.rs`; ogni accesso a byte è tramite
  `.get()`/slicing bounds-checked, mai un indice diretto senza verifica
  preventiva della lunghezza.

`crates/keeppix-media/tests/raw.rs`: gli 7 test dello Step 2 del piano,
verbatim, più un ottavo test (`measures_extraction_time_per_format`) che
stampa le misure richieste dallo Step 7 quando lanciato con `--nocapture`.

## Fixture procurati

Tutti e 5 i formati richiesti dal piano procurati con **file reali**, non
sintetici, da [raw.pixls.us](https://raw.pixls.us) (dataset CC0, verificato
via `json/getrepository.php?set=all`, non a occhio sulla pagina). Scelti i
file più piccoli disponibili in CC0 con una preview di risoluzione
ragionevole per il formato — niente `#[ignore]`:

| Formato | Fixture | Fonte reale | Dimensione |
|---|---|---|---|
| ARW | `sample.arw` | Sony ILCE-7S, 14bit compressed | 5.9 MB |
| NEF | `sample.nef` | Nikon D70s, 12bit compressed lossy type 1 | 5.0 MB |
| CR2 | `sample.cr2` | Canon EOS 40D, sRAW2 | 5.5 MB |
| CR3 | `sample.cr3` | Canon EOS R6, 3:2 | 5.0 MB |
| DNG | `sample.dng` | Adobe DNG Converter, Canon EOS 5D Mark III, lossy JPEG | 2.4 MB |

Totale 24 MB nel repository. Ho scartato una prima scelta più grande
(masterset `data-unique`, 5.5-27 MB) dopo aver trovato, tramite la vera
API JSON del sito (non la sola pagina HTML, che nasconde la tabella dietro
un `ajax`), varianti più piccole degli stessi modelli con licenza CC0
verificata. ORF e RAF (in spec §2.3 ma non richiesti dai test del piano)
non procurati: fuori dallo scope di questo task.

## Strategia di estrazione scelta e perché

**Estrazione manuale** (TIFF IFD walker + scanner ISO-BMFF), non
`rawler`/`rawloader`. Non li ho nemmeno provati: ho verificato empiricamente
con `exiftool -v3` sui 5 fixture reali che tutti i formati richiesti sono
risolvibili con la stessa manciata di tag TIFF standard (0x0201/0x0202,
0x0111/0x0117+Compression, 0x014A, 0x8769) più un solo box ISO-BMFF per
CR3 — nessuna logica specifica per produttore necessaria. `rawler`
porterebbe ~20 dipendenze transitive (rayon, jxl-oxide, backtrace, ...) per
un problema interamente risolto da <400 righe di parsing a byte, e in
questo task non useremmo comunque la sua pipeline di demosaic. Coerente con
la regola "nessuna dipendenza senza necessità reale".

Copertura reale per formato (misurata, non stimata): **5/5 = 100%** dei
formati richiesti raggiungono una preview embedded valida al primo tentativo,
su file reali di fotocamere/converter reali.

## Evidenza TDD

**RED** — `cargo test -p keeppix-media --test raw`:

```
error[E0432]: unresolved import `keeppix_media::raw`
 --> crates/keeppix-media/tests/raw.rs:3:20
  |
3 | use keeppix_media::raw::{PreviewSource, extract_embedded_preview};
  |                    ^^^ could not find `raw` in `keeppix_media`
```

Esattamente il fallimento previsto dal piano (Step 3).

**GREEN** — `cargo test -p keeppix-media --test raw -- --nocapture`:

```
running 8 tests
test dng_yields_a_preview ... ok
test a_non_raw_file_is_unsupported_not_a_crash ... ok
test the_extracted_bytes_decode_as_a_real_image ... ok
test nikon_nef_yields_a_preview ... ok
test sony_arw_yields_a_full_size_embedded_jpeg ... ok
test a_truncated_raw_is_an_error_not_a_panic ... ok
test canon_cr3_yields_the_prvw_box ... ok
test measures_extraction_time_per_format ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Nota su un fallimento intermedio durante l'implementazione (non nascosto,
riportato perché istruttivo): la prima versione di `extract_from_cr3`
cercava il box `PRVW` solo dentro `moov`, seguendo una fonte secondaria
letta online. Il test `canon_cr3_yields_the_prvw_box` è fallito con `None`
alla prima esecuzione reale. Ho verificato con uno script Python la
struttura byte-per-byte del fixture: il box `PRVW` vive in un `uuid` di
primo livello separato, non dentro `moov`. Corretto scansionando l'intero
file (giustificazione: `mdat`, che segue sempre `moov`/`uuid`, non può
contenere per caso la stessa sequenza di 4 byte ASCII "PRVW" con un header
valido a seguire — rischio statisticamente irrilevante). Da qui il secondo
GREEN, questa volta reale.

## MEASUREMENTS

`cargo test -p keeppix-media --test raw --release -- --nocapture`
(MacBook Apple Silicon, file già in cache disco):

| Formato | Fixture | Tempo | Preview scelta | Byte |
|---|---|---|---|---|
| ARW | Sony ILCE-7S | 3.33 ms | 1616×1080 | 734118 |
| NEF | Nikon D70s | 1.41 ms | 3008×2000 | 717588 |
| CR2 | Canon EOS 40D | 2.63 ms | 1936×1288 | 366439 |
| CR3 | Canon EOS R6 | 5.35 ms | 1620×1080 | 389450 |
| DNG | Adobe DNG Converter | 1.10 ms | 3960×2640 | 569249 |

Tutti sotto 6 ms, un ordine di grandezza sotto la stima di spec di
30-80 ms. **Ho corretto la nota in `fase-2-raw-culling.md` §2** (vedi
ledger, Ruling) — la stima della spec probabilmente includeva la lettura
da disco/rete di RAW a piena dimensione (30-80 MB) su storage non a caldo,
non la sola estrazione su byte già in RAM. Da riverificare con file di
dimensione realistica quando il Task 3 (job `DeriveRaw`) leggerà da un NAS
reale — non necessario per questo task, che opera sui byte del file già
letto in memoria.

Copertura al passo «preview trovata»: **5/5 formati richiesti (100%)**, su
file reali.

## Commit

- `d5db5d6` — `feat(media): extract the embedded preview from raw files`
- `a8b79b0` — `docs: record task 1 rulings and measurements in fase-2 ledger`
- `497fd01` — `chore: update Cargo.lock for keeppix-media tempfile dev-dependency`

Nessun push, nessuna PR.

## Concerns

1. **`fs::read` legge l'intero file**, anche per CR3 dove servirebbe solo
   scansionare i primi ~500 KB (dove vive tipicamente il box preview prima
   di `mdat`). Per i fixture (2-6 MB) è irrilevante (misurato: 1-5 ms
   totali). Per RAW reali da 30-80 MB su un NAS, la lettura I/O potrebbe
   dominare il tempo totale — non un problema di *questo* task (che misura
   byte già in RAM), ma degno di nota per il Task 3 se la latenza I/O reale
   risultasse superiore all'estrazione stessa.
2. **CR3: nessuna dipendenza dalla struttura `moov`/`trak`** — mi affido
   interamente al marker ASCII `PRVW` più il suo header fisso, dopo aver
   verificato che vivere fuori da `moov` è il comportamento reale (non
   quello documentato da tutte le fonti secondarie). Se un futuro modello
   Canon cambiasse la posizione o il formato esatto dell'header PRVW,
   l'estrazione fallirebbe silenziosamente con `Ok(None)` — accettabile per
   il contratto ("preview assente" è un caso legittimo), ma vale la pena
   ricordarlo se in futuro compaiono report di CR3 senza preview da modelli
   non testati qui.
3. **ORF e RAF non testati** (in spec §2.3, non nei test del piano): il
   parser TIFF generico dovrebbe già coprire ORF (è TIFF-based) senza
   modifiche, RAF invece ha una preview in coda al file secondo la spec e
   richiederebbe un terzo branch di parsing — non implementato, fuori scope.

## Status: DONE
