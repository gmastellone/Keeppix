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
| 3 | Job DeriveRaw | complete | `86a8a3e` (+ thumbhash `bcdde13`) |
| 4 | overrides + flags | complete | `6a17f4b` (+ test `1949a5e`) |
| 5 | Sidecar XMP | complete | `af00600`, `51127d7`, `6ee8f04`, `7dcb4e0` |
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

### Task 3: complete (commit `86a8a3e`, test verdi)

`cargo test -p keeppix-jobs --test raw -- --test-threads=1` → 7 passed
(4 richiesti dal piano + gli assert di harness). `cargo test -p
keeppix-media --test raw` → 10 passed (8 precedenti + 2 nuovi su
`demosaic_half`, gated su `dcraw_emu_available()` come già fa
`video::ffprobe_available`). Suite completa (`./scripts/test.sh`
sostituito da un loop equivalente, vedi Ruling sotto) → 60/60 blocchi
`test result: ok` su tutto il workspace, incluso `keeppix-server`.
`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi. `cargo deny check bans` verde (nessun arco nuovo fra
`keeppix-media` e `keeppix-db`).

**TDD sul test critico** (`a_raw_with_a_large_preview_never_calls_libraw`):
scritti tutti e 4 i test del piano, eseguiti e verdi al primo colpo con
l'implementazione già scritta (design esplorato prima per via
dell'interazione fra trait di iniezione e cascata — vedi Ruling
sotto). Per non fidarsi di un test verde che non ha mai visto il rosso,
ho mutato la cascata in `derive_raw` per saltare il controllo dei
1440px e chiamare sempre `demosaic.demosaic(src)`: il test è failato
correttamente (`left: 1, right: 0`, vedi report), poi ripristinato e
riverificato verde. Vedi `task-3-report.md` per l'output completo.

Ruling (Task 3): `JobKind::DeriveRaw` (non l'estensione di `DeriveAsset`
via branch su `AssetKind`) — come da piano/brief, così il dispatcher
resta un match piatto un-kind-per-ramo invece di annidare una seconda
cascata dentro `derive::run`. Nessuna migrazione necessaria: `jobs.kind`
è `text` senza `CHECK` (a differenza di `assets.kind`), quindi
aggiungere una variante è solo codice Rust. `hash.rs` sceglie
`DeriveRaw` vs `DeriveAsset` in base ad `asset.kind` con dedup key
`derive_raw:{hex}` separata da `derive:{hex}`. Costo se sbagliato:
nessuno strutturale — tornare a un branch unico dentro `DeriveAsset`
sarebbe un refactor locale a `derive.rs`/`raw.rs`, non uno schema da
disfare.

Ruling (Task 3): demosaic iniettato via trait object (`dyn Demosaic`),
non via feature flag o mock del processo — l'unica interfaccia di cui
il job ha bisogno è "dammi pixel RGB8 e le dimensioni per questo file",
e un trait la rende sostituibile nei test senza toccare `sandbox::run`
né avviare mai `dcraw_emu`. Il contatore delle chiamate vive nel mock
di test, non nel job. Costo se sbagliato: nessuno, è puro dependency
injection, zero comportamento diverso in produzione (`SandboxDemosaic`
è l'unica implementazione usata da `raw::run`).

Ruling (Task 3): `keeppix_media::derive_from_rgb` è una nuova funzione
pubblica (non prevista esplicitamente dal brief, che elenca solo
`derive_from_bytes` fra le interfacce consumate) perché l'uscita di
`dcraw_emu` è un PPM di pixel RGB8 già decodificati, non un JPEG:
`derive_from_bytes` decodifica sempre con `zune_jpeg` e non può
accettarla. La coda condivisa (resize, webp, thumbhash) è stata
estratta in un helper privato `build_derivatives` usato da entrambe le
funzioni, per non duplicare la logica di encoding fra il percorso
preview-incorporata e quello demosaic. Costo se sbagliato: un secondo
punto di manutenzione nel modulo derive, ma la duplicazione sarebbe
stata peggiore (due copie della stessa logica di resize/webp/thumbhash
che potrebbero divergere silenziosamente).

Ruling (Task 3): il parsing del PPM (`P6`) prodotto da `dcraw_emu -Z -`
è scritto a mano (un parser di ~30 righe in `keeppix-media::raw`)
invece di aggiungere una dipendenza come `image`/`ppm` — è un formato
che generiamo e consumiamo noi stessi con un solo produttore
conosciuto, tre token interi e un separatore fisso; non serve un parser
PNM completo (commenti multipli, P1-P6, 16-bit). Verificato manualmente
sui 5 fixture RAW (ARW/NEF/CR2/CR3/DNG, tempi 130-360ms per il
demosaic half-size sulle risoluzioni ridotte dei fixture) e su un file
corrotto (`dcraw_emu` esce con status 2, stdout vuoto, il parser lo
riconosce come errore invece di panicare). Costo se sbagliato:
riscrivere il parser se un giorno si usa un altro strumento di demosaic
che scrive PPM in modo diverso (16-bit, commenti) — isolato in una
funzione, non nel job.

Ruling (Task 3): timeout della sandbox per il demosaic 30s CPU / 512MiB
RAM — stessi ordini di grandezza già usati per `ffmpeg`/`ffprobe` in
`video.rs`. La spec stima 1,5-4s su ARM per RAW reali; sui 5 fixture di
Fase 2 (bassa risoluzione) `dcraw_emu -h -w` ha impiegato 130-360ms.
30s è generoso ma finito, coerente col vincolo del task. Costo se
sbagliato: un RAW anomalo (medio formato, sensore molto più grande dei
consumer testati) potrebbe eccedere il rlimit RAM e fallire con
`set_error` invece di completare — comportamento sicuro, non un
crash, ma da rivedere se emergono RAW di fascia più alta in Fase 2+.

Ruling (Task 3): `./scripts/test.sh` non gira su questa macchina — usa
`mapfile`, builtin di bash ≥4, e macOS spedisce bash 3.2 senza
un'alternativa in `/opt/homebrew/bin`. Ho rieseguito manualmente la
stessa logica (un crate alla volta, `--jobs 1 -- --test-threads=1`,
rimozione dei container testcontainers fra un crate e l'altro) invece
di modificare lo script, che probabilmente gira su CI/Linux con bash
recente — fuori dallo scope di questo task. Costo se sbagliato: nessuno
per la correttezza dei test di Task 3 (rieseguiti e verdi), ma lo
script resta rotto per chiunque lavori da questa stessa macchina finché
qualcuno non lo aggiorna o installa bash via Homebrew.

### Task 3 — fix da review critica: thumbhash mai persistito per i RAW

La review di Task 3 ha trovato un difetto Critical: `derive_raw`
scartava `DeriveResult.thumbhash` (tornava solo `Result<(), String>`) e
`run_with` non chiamava mai `AssetRepo::set_thumbhash_for_hash`, a
differenza di `derive.rs::run`. Per l'idempotenza basata sull'esistenza
del thumbnail (`thumb_path.is_file()`), un RAW derivato una volta resta
con `thumbhash IS NULL` per sempre — nessuna riesecuzione lo corregge.

Fix (commit `fix(jobs): persist thumbhash when deriving raw previews`):
`derive_raw` ora torna `Result<DeriveResult, String>`; `run_with`, dopo
un derive riuscito con `!result.skipped`, chiama
`assets.set_thumbhash_for_hash(&hash, &result.thumbhash)`, esattamente
come fa `derive.rs::run`. TDD: due nuovi test in
`crates/keeppix-jobs/tests/raw.rs`
(`deriving_from_the_embedded_preview_populates_the_thumbhash` su
`sample.arw` e `deriving_from_the_demosaic_fallback_populates_the_thumbhash`
sul path di fallback) — osservati FAILED prima del fix (thumbhash
assente), poi verdi dopo. `cargo test -p keeppix-jobs` (tutto il crate,
`--jobs 1 -- --test-threads=1`) → verde; `cargo fmt --check`, `cargo
clippy --workspace --all-targets -- -D warnings`, `cargo deny check
bans` → verdi. Vedi `task-3-fix-report.md`.

### Task 4: complete (commit `6a17f4b`, test aggiuntivo `1949a5e`, test verdi)

Migrazione `0012_overrides_flags.sql` (asset_overrides, asset_flags,
metadata_batches) esattamente come da brief/piano. Dominio:
`Rating`/`Pick`/`AssetFlags` in `keeppix-domain::flags`,
`OverridePatch`/`EffectiveMetadata`/`GeoPoint` in
`keeppix-domain::overrides` (ogni campo di `OverridePatch` è
`Option<Option<T>>`: `None`=non toccare, `Some(None)`=azzera,
`Some(Some(v))`=imposta), `BatchId` in `ids.rs`. DB: `OverrideRepo`
(`effective`/`apply`/`apply_batch`/`undo_batch`/`pending_sidecars`) e
`FlagRepo` (`set`/`get`/`batch_set`), entrambi con `AuthContext` come
primo parametro su ogni metodo che tocca dati utente, tramite un nuovo
`AssetRepo::assert_visible` condiviso che controlla la visibilità di un
intero batch di id in una sola query (non una per asset).

`cargo test -p keeppix-db --test overrides --test flags -- --test-threads=1`
→ 24 passed (15 + 9). `cargo fmt --check` e `cargo clippy --workspace
--all-targets -- -D warnings` verdi su tutto il workspace. Suite completa
eseguita crate per crate (vedi Ruling Task 3 sullo script rotto):
tutti i crate verdi tranne `keeppix-media --test video::poster_extracts_one_frame`,
preesistente e indipendente da questo task (ffmpeg non riesce a
scrivere un frame in questo sandbox; confermato rieseguendo lo stesso
test da `main` prima di qualunque modifica di Task 4).

**TDD sui test critici del brief**, tutti scritti prima dell'implementazione
e osservati rossi al primo giro naturale (nessuna implementazione ancora
presente), poi verdi. In più, mutation testing su tre invarianti dopo
l'implementazione (rotto apposta, verificato rosso, ripristinato, verificato
verde di nuovo):

1. `touched()` forzato a `(true, None)` anche per campi non toccati
   (`Option::None` del patch) → `a_later_partial_override_does_not_erase_an_earlier_field`
   fallisce (`left: None, right: Some("Titolo")`), come atteso: un
   override parziale azzererebbe i campi non toccati.
2. `restore_previous`: `EXCLUDED.col` sostituito con
   `COALESCE(EXCLUDED.col, asset_overrides.col)` nell'UPSERT di ripristino.
   I due test esistenti sull'undo restavano verdi — **gap reale**: nessuno
   dei due esercita "riga già esistente, campo da riportare a NULL"
   (uno passa dal ramo DELETE per riga mai esistita, l'altro ripristina un
   valore non-NULL). Aggiunto
   `undo_batch_restores_a_null_field_on_a_row_that_already_existed`
   (commit `1949a5e`): fallisce sotto la mutazione
   (`left: Some("Descrizione"), right: None`), verde con l'implementazione
   reale. Vedi `task-4-report.md` per il dettaglio.
3. `FlagRepo::get` con `WHERE asset_id = $1` (filtro `user_id` rimosso) →
   `two_users_rating_the_same_asset_do_not_overwrite_each_other` fallisce
   (`left: Some(Rating(5)), right: Some(Rating(2))`): un utente leggerebbe
   il voto di un altro.

Ruling (Task 4): `AssetRepo::assert_visible` è un nuovo metodo pubblico
(non elencato esplicitamente nel brief, che nomina solo i repo di
overrides/flags) perché sia `apply_batch`/`undo_batch` che `batch_set`
devono verificare la visibilità di fino a 500 id **prima** di scriverli,
e farlo id-per-id userebbe la stessa cascata di round-trip che
`apply_batch` deve evitare. Un `count(DISTINCT ...) = numero di id
richiesti` sul filtro di `VisibilityScope` già esistente copre in una
query sia "id inesistente" sia "id di un altro utente" con lo stesso
`Forbidden`, senza duplicare la logica di scope. Costo se sbagliato:
nessuno strutturale, è un helper puramente additivo su un repo già
esistente.

Ruling (Task 4): `metadata_batches.previous` cattura **l'intera riga**
di `asset_overrides` prima del batch (tutti e 6 i campi + `updated_by`),
non solo i campi toccati dal patch — perché un secondo batch che tocca
un campo diverso deve comunque poter tornare, se annullato, allo stato
esatto lasciato dal primo batch su *tutti* i campi, non solo quello che
ha appena scritto. Se l'asset non aveva alcuna riga di override, il
valore per quell'id nella mappa è `None` (non "tutti i campi NULL"): i
due casi si comportano diversamente in `undo_batch` (`DELETE` contro
`UPSERT` con valori `NULL` espliciti), pur producendo lo stesso
`effective()`. Costo se sbagliato: `previous` più grande di quanto
strettamente necessario per patch mono-campo, accettabile per un JSONB
scritto una volta per batch, non per asset.

Ruling (Task 4): `pending_sidecars` non prende un `AuthContext` — è
documentato nel doc comment seguendo lo stesso pattern già concordato
per `LibraryRepo::mark_scanned` (R nella Fase 0/1): lo chiamerà il job
`WriteSidecar` di Task 5, che attraversa tutte le librerie in
background, non un singolo utente autenticato. Costo se sbagliato:
andrebbe wrappato con uno scope di sistema esplicito quando arriva
Task 5, un cambio locale a quella chiamata.

### Task 5: complete (commit `af00600` media, `51127d7` domain, `6ee8f04` db, `7dcb4e0` jobs, test verdi)

`crates/keeppix-media/src/xmp.rs`: `SidecarData { rating, description,
title, tags, gps, taken_at, label }`, `read_sidecar`/`write_sidecar`,
mappatura dei campi esattamente come da brief/spec §3.4. Scrittura
sempre *leggi-modifica-riscrivi*: il file esistente viene parsato in
uno stream di eventi `quick-xml`, gli attributi/elementi non gestiti
sopravvivono inalterati, solo gli attributi in `MANAGED_ATTRS`
(`xmp:Rating`, `xmp:Label`, `exif:GPSLatitude/Longitude/DateTimeOriginal`)
e gli elementi figli gestiti (`dc:title`, `dc:description`, `dc:subject`)
vengono aggiunti/aggiornati/rimossi. Se il sidecar non esiste, si genera
uno scheletro minimo (`x:xmpmeta`/`rdf:RDF`/`rdf:Description` vuoto) —
mai un file "vuoto" sovrascritto da zero se ne esisteva uno. Scrittura
atomica: `.xmp.tmp` nella stessa cartella, `fsync`, rilettura e
confronto byte-a-byte, poi `rename()`; se un file esistente è
XML malformato, `write_sidecar` ritorna `Err` e non lo tocca affatto
(niente tentativo di "ripararlo" o rigenerarlo).

**TDD sul test critico del brief** (`writing_preserves_fields_we_do_not_manage`,
Lightroom `crs:Exposure2012`): scritto per primo insieme agli altri 11
test di `crates/keeppix-media/tests/xmp.rs`, osservato rosso (funzioni
non ancora implementate → errore di compilazione, poi panic su
`todo!`), poi implementato fino a verde. Mutation testing su due
invarianti dopo l'implementazione (rotti apposta, verificato rosso,
ripristinati, riverificato verde):

1. Rimosso il filtro `MANAGED_ATTRS.contains(...)` in `apply` (tutti
   gli attributi esistenti vengono scartati e solo i gestiti
   riscritti) → `writing_preserves_fields_we_do_not_manage` fallisce
   (`crs:Exposure2012` sparisce), confermando che il test esercita
   davvero la preservazione, non solo l'aggiornamento del rating.
2. `atomic_write` mutato per scrivere direttamente sul path finale
   invece di `.tmp` + `rename` → `writing_to_a_read_only_directory_fails_without_corrupting_anything`
   fallisce (l'errore atteso non arriva più, perché il file esistente
   con permessi normali accetta la scrittura anche se la cartella è
   in sola lettura), confermando che il test dipende davvero dalla
   disciplina tmp+rename, non da un dettaglio del filesystem.

`cargo test -p keeppix-media --test xmp -- --test-threads=1` → 12
passed.

Ruling (Task 5): `quick-xml = "0.41"` nuova dipendenza di
`keeppix-media` — nessuna libreria XML era già nel workspace.
Giustificazione: XMP è RDF/XML e il requisito "leggi-modifica-riscrivi
senza perdere campi sconosciuti" richiede di lavorare sullo stream di
eventi originale (non un DOM che normalizza spazi/ordine/dichiarazioni
di namespace) — `quick-xml` è l'unica libreria comune a Rust che espone
un `Reader`/`Writer` a eventi senza validazione DTD né normalizzazione
implicita, zero dipendenze transitive proprie a parte `memchr` (già nel
grafo). Già annunciata come scelta nel piano (`docs/superpowers/plans/2026-08-15-keeppix-fase-2.md`),
qui solo confermata in pratica. `cargo deny check bans` verde: nessun
arco nuovo verso `keeppix-db`.

Ruling (Task 5): il job `WriteSidecar` non porta l'asset nel payload
(a differenza di `DeriveRaw`) — ad ogni esecuzione rilegge
`OverrideRepo::pending_sidecars(limit=200)` e processa un batch,
ri-accodandosi da solo (stessa dedup key `write_sidecar`) se il batch
era pieno. Motivazione: il brief dice esplicitamente "prende gli asset
da `pending_sidecars`" (plurale) — un job per asset moltiplicherebbe
l'accodamento su un `apply_batch` di 500 righe in 500 job invece di
uno solo. Un batch fallito parzialmente marca `xmp_written_at` solo
per gli asset scritti **e verificati** con successo, poi ritorna `Err`
così lo scheduler ritenta — il prossimo giro rilegge `pending_sidecars`
e trova solo quelli rimasti indietro, non l'intero batch da capo.
Costo se sbagliato: un job singolo per asset sarebbe stato più semplice
da testare in isolamento, ma avrebbe rischiato di intasare la coda su
batch grandi; la reversione (tornare a un job per asset) è un refactor
locale a `xmp.rs`, nessun cambio di schema.

Ruling (Task 5): l'accodamento del sweep vive dentro
`OverrideRepo::apply`/`apply_batch` (in `keeppix-db`), non in
`keeppix-jobs` — architettonicamente valido perché `JobRepo` è nello
stesso crate di `OverrideRepo`, e l'alternativa (un reaper periodico
stile `ReapStale` che non ha ancora nessun trigger temporale cablato
da nessuna parte, verificato cercando nel codice) avrebbe introdotto
latenza non necessaria per un caso già coperto da un accodamento
diretto e deduplicato. **Limite noto e differito**: `FlagRepo::set`
(rating/pick) non tocca `asset_overrides.updated_at` né accoda nulla —
un utente che *solo* vota (senza mai toccare titolo/descrizione/GPS/
data) non fa comparire l'asset in `pending_sidecars` finché un
override successivo non lo tocca. Il rating dell'owner arriverà comunque
sul file alla prossima scrittura utile (via `sidecar_source`, che legge
sempre il voto corrente), ma non "al volo" al solo voto. Non risolto in
questo task perché il brief e la spec §3.3/§3.4 parlano di "override" come
innesco della propagazione, non di flag; estendere il rilevamento di
pending a `asset_flags` è un cambio di schema (serve un `updated_at`
con innesco equivalente) fuori dai confini scritti di Task 5. Costo se
sbagliato: un voto isolato del proprietario resta invisibile su disco
finché non arriva un altro cambiamento di metadati sullo stesso asset —
accettabile per Fase 2, da rivedere se l'uso reale mostra che il rating
da solo deve propagarsi subito.

`crates/keeppix-jobs/src/xmp.rs` + `crates/keeppix-jobs/tests/xmp.rs`:
due test di integrazione end-to-end (`TestDb` reale, non mock) che
provano l'intera pipeline "DB prima, file poi" — `apply` con voto del
proprietario produce un sidecar nuovo con `xmp:Rating`/`xmp:Label`
corretti e l'asset esce da `pending_sidecars`; un sidecar Lightroom
preesistente con `crs:Exposure2012` sopravvive a uno sweep che tocca
solo la descrizione. Mutation test: rimossa temporaneamente la chiamata
a `mark_sidecar_written` in `write_one` → il test sul "non più
pendente" fallisce come atteso, poi ripristinato.

`cargo test -p keeppix-jobs --test xmp -- --test-threads=1` → 5
passed (2 richiesti + 3 di harness). Suite intera per crate (vedi
Ruling Task 3 sullo script rotto — qui il problema è diverso: la
`cleanup_containers` di `scripts/test.sh` assume che se il comando
`docker` esiste anche il demone sia raggiungibile, il che non è vero in
questo sandbox — `docker ps` fallisce, `set -e`+`pipefail` interrompe
lo script dopo il primo crate. Non modificato `scripts/test.sh`, fuori
scope per questo task; rieseguito `cargo test -p <crate> --jobs 1 --
--test-threads=1` con `KEEPPIX_TEST_DATABASE_URL` per ogni crate del
workspace, come da istruzioni d'ambiente "No Docker"):
`keeppix-domain` 42, `keeppix-media` (esclusi `video::*`, vedi sotto)
tutti verdi incluso `xmp` 12/12, `keeppix-db` tutti verdi (incluso
`overrides` 15/15 con `pending_sidecars_only_lists_updates_not_yet_written`),
`keeppix-jobs` tutti verdi incluso `xmp` 5/5, `keeppix-api` tutti verdi
(24+ test), `keeppix-server`/`keeppix-dav`/`keeppix-test-support` zero
test ma compilano. `cargo fmt --check` e `cargo clippy --workspace
--all-targets -- -D warnings` verdi su tutto il workspace.

Fallimento preesistente e indipendente da questo task, non toccato:
`keeppix-media --test video::poster_extracts_one_frame` — ffmpeg non
riesce a scrivere un frame in questo sandbox. Confermato riproducendolo
sul commit precedente a Task 5 (`878418a`, checkout temporaneo di
`crates/keeppix-media` prima di `af00600`, poi ripristinato): fallisce
identicamente, quindi non è una regressione introdotta qui — stessa nota
già presente nel ledger di Task 4.
