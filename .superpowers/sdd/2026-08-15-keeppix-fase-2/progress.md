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
| 6 | Stack RAW+JPEG | complete | `8a3308f` |
| 7 | Cestino a tre opzioni | complete | `3edb207`, `a35c1a4`, `b82380a`, `04e8cb6` |
| 8 | Duplicati + batch | complete | `49a2068`, `e62d817`, `75a84ea`, `51378bf`, `91d8ed1` |
| 9 | Frontend culling | complete | (vedi sotto) |

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

### Task 6: complete (commit `8a3308f`, test verdi)

Migrazione `0013_stacks.sql`: tabella `stacks (id, primary_asset_id NOT
NULL, created_at)` e `ALTER TABLE assets ADD CONSTRAINT ... FOREIGN KEY
(stack_id) REFERENCES stacks (id) ON DELETE SET NULL` — la colonna
`assets.stack_id` esisteva già dalla 0005 (nullable, senza FK), come
segnalato nel brief. `StackRepo::regroup_folder(folder_id)` (nessun
`AuthContext`, stessa giustificazione di `LibraryRepo::mark_scanned`:
lo chiamerà lo scanner) raggruppa per nome base case-insensitive nella
stessa cartella (spec §5, regola 1): stesso stack per stesso nome base,
RAW primario quando presente, un file solo non forma mai uno stack.

Ruling (Task 6): la regola 2 dello spec (scatti entro 2 secondi, stesso
corpo macchina, stesso numero di scatto) **non è implementata** — come
il brief permette esplicitamente ("se non fattibile senza più schema,
documentare come Ruling e implementare solidamente la regola 1"). Lo
schema di `asset_exif` (`camera_make`, `camera_model`, `lens`, `iso`,
`f_number`, `exposure`, `focal_length`, più `raw jsonb`) non ha un
campo "numero di scatto": la sequenza di scatto della fotocamera non è
un tag EXIF standard — vive in blocchi MakerNote proprietari e diversi
per marca (Sony `0x9050`, Canon nei tag custom, Nikon in un blocco
cifrato su alcuni corpi), quindi va oltre il parsing EXIF generico già
fatto in Fase 1 e richiederebbe un parser per-vendor dentro
`asset_exif.raw` — un cambio di scope che il piano vieta esplicitamente
("Non implementare cose di fasi successive perché tanto ci vuole
poco"). Un'implementazione più debole basata solo su
`taken_at_utc`+`camera_model` (senza numero di scatto) rischierebbe
falsi positivi concreti: due scatti diversi a raffica entro 2 secondi
con la stessa fotocamera verrebbero stackati insieme senza che siano
lo stesso soggetto. Costo se sbagliato: gli utenti che scattano
RAW+JPEG con nomi file disallineati (rari: succede solo se il firmware
numera i due formati con contatori diversi, che nessuno dei corpi
comuni fa) non vedono lo stack; la regola 1 da sola copre il caso
d'uso descritto nella spec (`DSC_0042.ARW`+`DSC_0042.JPG`, stesso nome
di entrambi i file scritti dalla fotocamera).

Ruling (Task 6): la promozione del primario e la pulizia dello stack
orfano vivono in un **trigger SQL** (`assets_promote_stack_primary`,
migrazione 0013), non in un metodo di `StackRepo` — motivazione
esplicita nel commento della migrazione: deve reggere qualunque via
porti alla rimozione di un asset (il cestino di Task 7, un `DELETE`
fatto a mano nei test, un futuro comando batch), e un invariante di
schema non si può dimenticare di richiamare mentre un metodo di
repository sì. Il precedente c'è già nel codebase (`assets_month_counts`,
0009). **Scoperta empirica durante l'implementazione**: il trigger deve
essere `AFTER`, non `BEFORE` come tentato per primo — con `BEFORE`, il
`DELETE FROM stacks` per uno stack rimasto senza membri innesca, tramite
il cascade `ON DELETE SET NULL` della FK gemella
(`assets.stack_id -> stacks.id`), un tentativo di modificare di nuovo
la riga `assets` che l'`UPDATE`/`DELETE` esterno sta ancora processando
in quello stesso istante, e Postgres rifiuta l'auto-modifica con
`tuple to be updated was already modified by an operation triggered by
the current command`. Riprodotto verificandolo empiricamente (non solo
per ragionamento) sostituendo temporaneamente `AFTER` con `BEFORE`
prima di arrivare alla versione finale: `deleting_the_primary_...`
falliva con esattamente quell'errore Postgres. Con `AFTER`, quando il
trigger legge lo stato di `assets` la riga OLD è già sparita (`DELETE`)
o già sul nuovo `stack_id` (`UPDATE OF stack_id`), quindi il conteggio
dei membri rimasti è accurato senza toccare di nuovo la riga in corso.
La FK `stacks.primary_asset_id -> assets.id` è `DEFERRABLE INITIALLY
DEFERRED` per lo stesso motivo, dal lato opposto: senza differirla,
l'ordine fra il nostro trigger `AFTER` e il trigger interno di
Postgres che applica quella FK (entrambi `AFTER` sulla stessa tabella
per lo stesso evento) dipenderebbe dall'ordine alfabetico dei nomi dei
trigger — fragile e dipendente dalla versione di Postgres. Differendo
il controllo a fine transazione, il nostro trigger ha tutto il tempo
di riassegnare `primary_asset_id` prima che il vincolo venga
verificato. Costo se sbagliato: un `DELETE` del primario di uno stack
a membro singolo fallirebbe con un errore Postgres invece di
completarsi — verificato che *non* accade con la versione attuale.

**Idempotenza** (il test che il brief segnala come critico):
`regroup_folder` riusa lo `stack_id` già presente sui membri del
gruppo quando è unico, invece di crearne uno nuovo a ogni chiamata.
Verificato con un test di mutazione: rimossa la logica di riuso
(sempre `INSERT INTO stacks` con un nuovo id) →
`regrouping_the_same_folder_twice_is_idempotent` fallisce
(`left: <uuid1>, right: <uuid2>`, id diversi al secondo giro), poi
ripristinata e riverificata verde.

**Altri due mutation test** sui requisiti del brief, entrambi
osservati rossi poi ripristinati verdi:
1. Rimossa la preferenza per il RAW (`primary = group[0].id` invece di
   cercare `kind == "raw_image"`) → `the_raw_is_the_primary_asset_when_present`
   fallisce. **Nota**: la prima versione di questo test usava
   `DSC_0043.ARW`/`DSC_0043.JPG`, e "ARW" ordina alfabeticamente prima
   di "JPG" — la mutazione passava comunque, perché il fallback "primo
   per nome" sceglieva per coincidenza lo stesso file del RAW. Corretto
   usando `.NEF` (che ordina dopo sia `.HEIC` sia `.JPG`) nei due test
   che asseriscono sul primario, così la mutazione fallisce davvero.
   Esattamente il tipo di test "verde ma che non prova nulla" contro
   cui mette in guardia l'AGENTS.md.
2. Disabilitato il trigger (`CREATE TRIGGER` commentato in una copia
   locale della migrazione, poi ripristinato) →
   `deleting_the_primary_promotes_another_member_...` fallisce, non con
   uno stack orfano ma con un errore Postgres
   (`violates foreign key constraint "stacks_primary_asset_id_fkey"`):
   la FK `NOT NULL` + `DEFERRABLE` impedisce comunque la corruzione,
   trasformando l'assenza del trigger in un errore rumoroso invece che
   in uno stack silenziosamente rotto.

`cargo test -p keeppix-db --test stacks -- --test-threads=1` → 9
passed (6 richiesti dal brief + 3 di harness). Suite intera eseguita
crate per crate (`keeppix-domain` 42, `keeppix-db` tutti verdi,
`keeppix-jobs` tutti verdi inclusi `raw`/`xmp`, `keeppix-api`/
`keeppix-server`/`keeppix-dav`/`keeppix-test-support` verdi) più
`cargo build --workspace --all-targets`. `cargo fmt --check` e `cargo
clippy --workspace --all-targets -- -D warnings` verdi su tutto il
workspace. Fallimento preesistente confermato ancora presente e non
toccato: `keeppix-media --test video::poster_extracts_one_frame`
(stessa causa già annotata nei Task 4/5).

`cargo deny` non è installato in questo ambiente (`error: no such
command: 'deny'`) — non eseguibile, non aggirato: questo task non
aggiunge dipendenze né tocca `Cargo.toml` di alcun crate, quindi non
c'è un nuovo arco `keeppix-media`↔`keeppix-db` da verificare.

Non cablato in `discover`/`hash` (piano Task 6: "Files" elenca solo
migrazione + `stacks.rs` + test, nessuna modifica a `keeppix-jobs"):
`StackRepo::regroup_folder(folder_id)` è pronto per essere chiamato
dallo scanner una volta per cartella dopo la scrittura degli asset
(stesso punto in cui `discover.rs::run` oggi chiama `ensure_path` per
cartella), ma il brief permette esplicitamente di lasciare questo
cablaggio a un task successivo ("StackRepo methods that discover can
call later").

### Task 7: complete (commit `3edb207` domain, `a35c1a4` db, `b82380a` media, `04e8cb6` api, test verdi)

Migrazione `0014_trash.sql`: tabella `trash_entries` (audit/ripristino,
non FK verso `assets.id` — vedi commento in migrazione sul perché un
`ON DELETE CASCADE`/`SET NULL` distruggerebbe l'audit insieme
all'asset). `DiskAction`/`TrashEntry` in `keeppix-domain::trash`.
`TrashRepo::choose(ctx, asset_id, action)` applica una delle tre
opzioni (spec §6): `kept` cancella la riga `assets` lasciando il file;
`purged` cancella file e riga; `moved_to_trash` fa `rename()` in
`<library_root>/.keeppix-trash/<sottopercorso>/<entry_id>__<filename>`
e marca l'asset `trashed`. `TrashRepo::restore` rimette il file al
percorso originale e l'asset a `indexed`, senza sovrascrivere se il
percorso è di nuovo occupato (`Conflict`). `TrashRepo::cleanup_expired(before)`
cancella dal disco e dalla tabella i `moved_to_trash` pendenti più
vecchi del cutoff, tollerando il fallimento di un singolo file (lo
riprova al giro dopo invece di abortire tutto il batch). Rotte API
`DELETE /api/v1/assets/{id}` (body `{"disk_action": ...}`, nessun
default) e `POST /api/v1/assets/{id}/restore` in
`crates/keeppix-api/src/routes/trash.rs`, con `docs/api/openapi.json`
rigenerato per le due nuove operazioni.

`cargo test -p keeppix-db --test trash -- --test-threads=1` → 12
passed (6 richiesti dal brief + 6 di harness/copertura aggiuntiva:
conflitto su ripristino senza cestinamento pendente, probing Forbidden
non NotFound). `cargo test -p keeppix-api --test trash` → 3 passed.
`cargo test -p keeppix-media --test walk` → 3 passed (incluso il nuovo
`walker_excludes_the_keeppix_trash_directory`). Suite completa
eseguita crate per crate con `KEEPPIX_TEST_DATABASE_URL` puntato a
Postgres locale (niente Docker in questo ambiente, coerente con Task
3/5): `keeppix-domain` 44, `keeppix-db` tutti verdi, `keeppix-media`
tutti verdi eccetto il fallimento preesistente e indipendente
`video::poster_extracts_one_frame` (stessa causa annotata nei Task
4/5/6, non toccato), `keeppix-api` tutti verdi (incluso
`openapi_snapshot_matches_the_committed_file` dopo la rigenerazione).
`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi su tutto il workspace. `cargo deny` non installato in
questo ambiente (stessa nota di Task 6); nessuna dipendenza nuova
aggiunta da questo task.

**TDD sui sei requisiti pinnati dal brief**, tutti scritti come test
che falliscono prima dell'implementazione (in un caso, l'implementazione
e i test sono stati scritti nella stessa iterazione — verificato con
mutation testing invece che con un rosso naturale, per lo stesso motivo
già discusso in Task 3/6: un design esplorato per intero prima di battere
tastiera avrebbe reso il primo giro dei test verde per costruzione, non
per fortuna). Mutation testing su ciascuno dei sei, rotto apposta,
osservato rosso, ripristinato, riverificato verde:

1. `rename()` sostituito con `copy()` + `remove_file()` in
   `move_into_trash` → `moving_to_trash_is_a_rename_that_keeps_the_inode`
   fallisce sull'assert sull'inode (`left != right`): un `copy()` alloca
   un nuovo inode anche sullo stesso filesystem, mentre `rename()` no.
2. Esclusione di `.keeppix-trash` rimossa da
   `keeppix_media::walk::is_excluded_name` →
   `walker_excludes_the_keeppix_trash_directory` fallisce (il file
   cestinato torna a comparire nell'elenco del walker) — è il rischio
   esplicitamente segnalato dal brief come "quello che si dimentica e
   produce un ciclo infinito visibile solo su una libreria grande".
3. Il controllo `original.exists()` rimosso in `TrashRepo::restore` →
   `restore_does_not_overwrite_a_file_that_now_occupies_the_original_path`
   fallisce: il contenuto del file che occupava il percorso verrebbe
   sovrascritto dal ripristino invece di restare intatto.
4. `may_purge` mutata per tornare sempre `true` →
   `only_owner_and_admin_can_purge_an_editor_gets_forbidden` fallisce
   (l'utente senza libreria propria riesce a fare `purged`).
5. La `WHERE` di `restore` allargata a includere anche `restored_at IS
   NOT NULL` (query sempre vera) → `restoring_an_asset_that_is_not_in_the_trash_is_a_conflict`
   fallisce: un asset mai cestinato verrebbe "ripristinato" da una riga
   già chiusa invece di tornare `Conflict`.
6. Il filtro `deleted_at < before` rimosso da `cleanup_expired` →
   `cleanup_expired_deletes_the_file_and_the_row_past_the_cutoff`
   fallisce (`cleaned == 2` invece di `1`): il cestinamento recente
   verrebbe cancellato insieme a quello scaduto.

Ruling (Task 7): l'autorizzazione su `Purged` è estratta in una
funzione pura `may_purge(ctx: &AuthContext, library_owner: UserId) ->
bool`, separata dalla risoluzione async di libreria/visibilità dentro
`TrashRepo::choose`. Motivazione: nel modello di visibilità di questa
fase (nessuna condivisione prima della Fase 3) chiunque veda un asset
è già owner o admin della sua libreria — un test end-to-end su
"editor riceve Forbidden" non potrebbe distinguere il cancello dedicato
a `Purged` dal controllo di visibilità che lo precede, perché
coincidono sempre. La funzione pura si pinna con tre unit test diretti
(admin estraneo, owner non-admin, utente né owner né admin) che non
passano dal database, indipendenti da come la visibilità evolverà in
Fase 3. Costo se sbagliato: quando la Fase 3 introduce la condivisione,
`may_purge` andrà comunque richiamata da `choose` con l'owner reale
della libreria (già lo fa) — nessun refactor previsto, solo più casi
coperti dagli stessi tre test.

Ruling (Task 7): `trash_entries.asset_id` non porta una foreign key
verso `assets.id` (documentato nel commento della migrazione). Sia
`kept` sia `purged` cancellano la riga `assets` nella stessa
transazione in cui scrivono la riga di audit: un `ON DELETE CASCADE`
distruggerebbe l'audit insieme all'asset (perdendo la prova di "chi ha
cancellato cosa e quando"), un `ON DELETE SET NULL` renderebbe NULL
una colonna che ha senso solo se popolata. L'id resta un riferimento
storico valido anche quando l'asset sottostante non esiste più. Costo
se sbagliato: nessun controllo di integrità referenziale automatico su
`asset_id` — accettabile perché la tabella esiste per audit, non per
letture che assumono l'asset ancora vivo.

Ruling (Task 7): `TrashRepo::cleanup_expired(before: DateTime<Utc>)` è
scritta e testata ma **non ancora agganciata a un job schedulato** — il
brief di questo task non lo richiede esplicitamente (elenca solo
migrazione, `trash.rs`, rotta API; il piano generale non menziona un
`JobKind` per la pulizia in questa fase). Chi la chiamerà passerà
`Utc::now() - Duration::days(30)`; il codice è pronto, il cablaggio a
uno scheduler è fuori dai confini scritti di Task 7. Costo se
sbagliato: senza un job che la richiami periodicamente, il cestino
cresce indefinitamente su disco finché qualcosa non invoca il metodo —
da annotare come lavoro futuro se non compare in un task successivo di
questa fase.

Fallimento preesistente e indipendente da questo task, non toccato:
`keeppix-media --test video::poster_extracts_one_frame` (stessa causa
annotata nei Task 4/5/6).

### Task 8: complete (commit `49a2068`, `e62d817` db; `75a84ea`, `51378bf`, `91d8ed1` api, test verdi)

`DuplicateRepo` (`crates/keeppix-db/src/duplicates.rs`) sostituisce
`ProblemsRepo::duplicates` della Fase 1c: `groups()` esclude gli asset
`trashed` dal conteggio (`a.status <> 'trashed'`, spec — un duplicato
già in coda per sparire non è "recuperabile" nello stesso senso),
`reclaimable_bytes()` è `size_bytes * (count - 1)` non la somma totale
(la prima copia è la foto, non spazio da recuperare), `members()`
elenca i singoli asset di un gruppo per scegliere quale tenere, e
`resolve()` applica una delle tre opzioni di cancellazione (spec §6) a
ogni membro non tenuto **riusando** `TrashRepo::choose` invece di
reimplementarle. `OverrideRepo::shift_taken_at` (in
`crates/keeppix-db/src/overrides.rs`, accanto ad `apply`/`apply_batch`
già esistenti da Task 4) somma `N` ore a `COALESCE(override,
exif).taken_at` con `make_interval(hours => $2)` in un solo statement,
registra un batch di annullamento come `apply_batch`, e lascia senza
data un asset che non ne aveva nessuna (uno scostamento non può
inventare un'origine). `undo_batch` ora rifiuta con `Conflict` se il
sidecar di anche un solo asset del batch è già stato scritto con i
suoi valori (`xmp_written_at >= metadata_batches.applied_at`).

Rotte API nuove: `crates/keeppix-api/src/routes/duplicates.rs` (`GET
/api/v1/duplicates`, `GET /api/v1/duplicates/{content_hash}`, `POST
/api/v1/duplicates/{content_hash}/resolve` — riusa
`routes::trash::parse_action`, reso `pub(crate)`), `metadata.rs` (`GET`/
`PATCH /api/v1/assets/{id}/metadata`, `POST /api/v1/metadata/batch`,
`POST /api/v1/metadata/batch/shift-taken-at`, `POST
/api/v1/metadata/batch/{batch_id}/undo`), `flags.rs` (`GET`/`PUT
/api/v1/assets/{id}/flags`, `POST /api/v1/flags/batch` — espone il
`FlagRepo` già scritto in Task 4, che non aveva ancora una rotta HTTP).
`docs/api/openapi.json` rigenerato: 26 → 34 operazioni.

**MISURATO** (`cargo test -p keeppix-db --release --test overrides
apply_batch_on_five_thousand_assets_stays_under_a_second -- --nocapture`,
5.000 righe seedate con un `INSERT ... SELECT ... FROM unnest(...)` di
massa, non 5.000 round-trip): `apply_batch` **57ms**, `undo_batch`
**11ms** — due ordini di grandezza sotto il vincolo "sotto un secondo"
del brief. In debug (`cargo test` senza `--release`): 75ms / 27ms,
ancora ampiamente sotto soglia. Il limite nel test resta permissivo (3s,
non 1s) per non renderlo instabile su una macchina condivisa/lenta — la
cifra vera è quella stampata e registrata qui e in
`task-8-report.md`, non l'asserzione.

Ruling (Task 8): `parse_action` in `routes::trash` cambiato da privato
a `pub(crate)` invece di duplicarne una copia in `duplicates.rs` — la
stessa mappa stringa→`DiskAction` con lo stesso errore 400 serve a
`resolve()` per applicare l'azione scelta a ogni membro non tenuto del
gruppo. Costo se sbagliato: nessuno, è una visibilità più ampia dentro
lo stesso crate, non un'API pubblica nuova.

Ruling (Task 8): `resolve()` **non** è un'operazione tutto-o-niente —
itera i membri del gruppo chiamando `TrashRepo::choose` uno alla volta,
e se un membro fallisce (es. `Forbidden` su `Purged` per un non-owner)
i membri già processati restano cestinati/eliminati, non tornano
indietro. Motivazione: un rollback esigerebbe di "ri-materializzare"
file già spostati o cancellati sul filesystem, un'operazione che può
essa stessa fallire — più fragile del comportamento scelto. Costo se
sbagliato: un gruppo di duplicati grande con permessi misti fra i
membri (impossibile nel modello di visibilità di questa fase, dove
tutti i membri di uno stesso gruppo appartengono alla stessa libreria
quindi allo stesso owner) potrebbe lasciarsi a metà — da rivedere se la
Fase 3 introduce condivisione fra utenti diversi sulla stessa libreria.

Ruling (Task 8): `MetadataPatchRequest` deserializza `Option<Option<T>>`
con una funzione scritta a mano (`double_option`, in
`routes/metadata.rs`) invece di aggiungere `serde_with` come
dipendenza — un solo usarlo in tutto il crate. Stesso problema che
`serde_with::double_option` risolve (distinguere "campo assente" da
"campo presente con `null`"), stessa soluzione (`#[serde(default,
deserialize_with = ...)]` con `Option::<T>::deserialize(de).map(Some)`),
zero dipendenze in più. Costo se sbagliato: se servisse altrove in
futuro, vale la pena promuoverla a dipendenza condivisa — al momento
non ce n'è un secondo punto d'uso.

`cargo test -p keeppix-db --test duplicates -- --test-threads=1` → 8
passed (5 di dominio + 3 di harness). `cargo test -p keeppix-db --test overrides -- --test-threads=1`
→ 21 passed (15 precedenti da Task 4 + 6 nuovi: misura sui 5.000,
`shift_taken_at` × 3, `undo` rifiutato/non rifiutato dal sidecar × 2).
`cargo test -p keeppix-api --test duplicates --test metadata --test flags -- --test-threads=1`
→ 14 passed (4 + 6 + 4). `cargo test -p keeppix-api --test openapi --
--test-threads=1` → 6 passed, incluso
`openapi_snapshot_matches_the_committed_file` dopo la rigenerazione.
Suite completa eseguita crate per crate (`./scripts/test.sh` si ferma
al primo crate in questo sandbox — stessa causa già annotata nei
Ruling di Task 3/5: `cleanup_containers` chiama `docker ps` assumendo
che il demone sia raggiungibile solo perché il binario esiste, `set -e`
interrompe lo script): tutti i crate verdi eccetto il solito
`keeppix-media --test video::poster_extracts_one_frame` (preesistente,
non toccato da questo task, annotato nei ledger di Task 4/5/6/7).
`cargo fmt --check` e `cargo clippy --workspace --all-targets -- -D
warnings` verdi su tutto il workspace. `cargo deny` non installato in
questo ambiente (stessa nota dei task precedenti); nessuna dipendenza
nuova aggiunta da questo task.

Vedi `task-8-report.md` per il dettaglio completo, inclusi i numeri
misurati e i test critici del brief.

### Task 9: complete (test verdi, non committato al momento della scrittura di questa voce — vedi commit qui sotto una volta fatto)

Frontend-only, nessun crate Rust toccato (confermato con `git status`
prima del commit). File nuovi: `frontend/src/api/culling.ts` (client
per `GET`/`PUT /api/v1/assets/{id}/flags`, `DELETE
/api/v1/assets/{id}` con `disk_action`, tutti già esposti da Task 7/8),
`frontend/src/stores/culling.ts` (Pinia setup store: `order`/`position`
per la navigazione filtrata, `flagsById` ottimistico, `queue` per i
voti in attesa di conferma dal server), `frontend/src/components/
RatingStars.vue`, `frontend/src/components/Filmstrip.vue`,
`frontend/src/views/CullingView.vue`. Modificati: `router.ts` (rotta
`/culling` lazy), `i18n/{it,en}.json` (namespace `culling`),
`TimelineView.vue` (pulsante *Culling* nell'header).

**TDD sui quattro requisiti del brief**, tutti scritti come test che
falliscono prima dell'implementazione (import non risolvibile —
`unresolved import`, osservato con `npx vitest run` prima di scrivere
`culling.ts`/`CullingView.vue`), poi verdi:

1. `l'avanzamento automatico dopo il voto porta alla foto successiva`
   → `culling store — navigation > advances to the next photo after
   voting on the current one` (più due varianti: non supera l'ultima
   foto, non avanza se si vota una foto diversa da quella corrente).
2. `le scorciatoie non sparano mentre l'utente scrive` →
   `CullingView keyboard shortcuts > does not fire shortcuts while the
   user is typing in a text field` + una terza su textarea/
   contenteditable. **Scoperta durante l'implementazione**: jsdom non
   implementa `HTMLElement.isContentEditable` (resta sempre
   `undefined`, verificato isolando il comportamento con `node -e`
   prima di incolpare il codice) — `isTypingTarget` controlla anche
   l'attributo `contenteditable` direttamente, non solo la proprietà,
   altrimenti il test sul contenteditable sarebbe stato verde per il
   motivo sbagliato (o rosso per un bug di jsdom, non del codice).
3. `il filtro «solo scarti» mostra ciò che dichiara` → tre test in
   `culling store — filters`: `rejects` mostra esattamente e solo i
   respinti, `picks` non mostra mai un respinto, `all` li mostra
   entrambi indipendentemente dal voto.
4. `lo store non perde i voti se la rete cade: si accodano e si
   ritentano` → `culling store — resilient queue`: un `setFlags` che
   fallisce una volta lascia il voto in `store.queue` (non lo perde),
   `retryQueue()` lo reinvia con successo; un secondo test verifica che
   un voto arrivato mentre il primo è ancora in volo non scavalchi né
   perda quello precedente (coda FIFO, un tentativo alla volta).

Ruling (Task 9): **nessuna vista per cartella o selezione multipla
esiste ancora nel frontend** (il frontend fin qui ha solo timeline
piatta, ricerca, problemi). Il brief e la spec §4.2 chiedono "il
pulsante Culling nella barra di una cartella o di una selezione" come
unico punto d'ingresso — qui il pulsante *Culling* vive nell'header di
`TimelineView.vue` e avvia una sessione sull'insieme già caricato in
timeline (`flatAssets`, tutto ciò che l'utente ha scorso finora), non
su una cartella o una selezione esplicita. Resta un **unico** punto
d'ingresso (il vincolo duro rispettato), solo il "su cosa" è più
grezzo di quanto la spec immagini. Costo se sbagliato: quando Fase 3+
introduce selezione multipla o vista per cartella, il punto giusto per
il pulsante cambia — `cullingStore.start(list)` accetta già qualunque
array di `TimelineAsset`, quindi il collegamento è un cambio locale a
dove viene chiamato, non allo store o alla vista.

Ruling (Task 9): **zoom 1:1 senza un endpoint di ritaglio lato
server**. La spec dice "si precarica il ritaglio centrale a piena
risoluzione delle 3 foto successive" — non esiste (né questo task lo
introduce: fuori dai file elencati dal brief, che sono solo frontend)
un endpoint che ritagli un'immagine sul server. Realizzato come tecnica
di sola presentazione: si precarica l'intero originale
(`/media/original/{id}`, come indicato esplicitamente dalle istruzioni
del task) in cache browser con `new Image()` per le 3 foto successive
nell'ordine filtrato corrente, e allo zoom si mostra quell'immagine a
piena risoluzione (`max-width: none`) dentro un contenitore
`overflow: hidden` centrato — il "ritaglio" è il contenitore che
nasconde tutto tranne il centro, non un file più piccolo generato ad
hoc. **Limite noto**: per un asset RAW, `/media/original/{id}` in
questa fase restituisce il file RAW originale (ARW/NEF/CR3/…), che i
browser non sanno decodificare come `<img>` — lo zoom 1:1 funziona per
JPEG/HEIC/PNG ma fallisce silenziosamente (l'`<img>` semplicemente non
si carica) su un RAW puro. Il caso d'uso reale del culling per RAW è
mitigato dal fatto che l'utente normalmente guarda la preview derivata
(fino a 1440px, Task 1-3) e lo zoom 1:1 vale soprattutto per gli asset
non-RAW o per uno stack RAW+JPEG dove il JPEG è disponibile — non
risolto qui perché richiederebbe un endpoint di crop lato server
(rasterizzare la preview embedded del RAW oltre i 1440px attuali), un
cambio di scope su `keeppix-media`/`keeppix-api` non incluso nei file
che il brief elenca per Task 9. Da rivedere se l'uso reale mostra che
serve davvero un crop ad alta risoluzione per i RAW.

Ruling (Task 9): **`AssetViewer.vue` non è stato toccato.** La regola
dura ("il visualizzatore normale non diventa una modalità: solo rating
1-5 e preferito `f`") vieta di aggiungere lì le scorciatoie del
culling — non impone di aggiungere rating/preferito se non ci sono già
(`AssetViewer.vue` oggi ha solo `Escape`/`i`/frecce, niente rating né
preferito: quello è un debito della Fase 1c/design §10.4, fuori dai
file che il brief di Task 9 elenca). Rispettato per omissione: nessuna
sovrapposizione introdotta, nessuna nuova funzionalità nel
visualizzatore normale.

Ruling (Task 9): **niente fetch collettivo dei voti pre-esistenti**
all'avvio di una sessione — non esiste un endpoint `GET` in batch per
`asset_flags` (Task 8 ha esposto solo `POST /api/v1/flags/batch` per
*scrivere*, non per leggere più asset in una chiamata). `culling.ts`
espone `ensureFlagsLoaded(id)`, chiamato pigramente quando un asset
diventa quello corrente (un `GET` alla volta, non N in parallelo
all'avvio), che non sovrascrive mai un voto già in coda o già noto
localmente — così un voto appena dato non viene rimpiazzato da una
risposta di rete arrivata in ritardo. Costo se sbagliato: N richieste
sparse nel tempo invece di 1 sola all'avvio, accettabile per una
sessione interattiva (l'utente vede una foto alla volta comunque), da
rivedere se serve mostrare "già votate" nel filmstrip prima che
l'utente ci arrivi.

Ruling (Task 9): **`p`/`x` sono toggle**, non set-only — premerli due
volte sulla stessa foto torna a "nessun voto". Non richiesto
esplicitamente dal brief, ma coerente con l'UX standard del culling
professionale (Lightroom fa lo stesso: `P`/`X` alternano). Costo se
sbagliato: nessuno strutturale, è un dettaglio di `pick()`/`reject()`
nello store.

Ruling (Task 9): confronto affiancato (`c`) limitato a mostrare la
foto corrente più le successive nell'ordine filtrato **fino a 4**
(spec dice "2-4"): con 2+ foto disponibili mostra tutte quelle
disponibili fino al tetto di 4, con una sola foto mostra solo quella.
Nessun controllo esplicito per scegliere quali 2-4 (es. selezione
manuale nel filmstrip) — implementato il caso base "le prossime N",
sufficiente per confrontare scatti quasi identici in sequenza (il caso
d'uso citato dalla spec). Da estendere se serve confrontare foto non
adiacenti.

**Verifica** (comandi del brief, frontend-only):

```
npx vue-tsc --noEmit                    → pulito
npx vitest run                          → 11 file, 35 test, tutti verdi
npm run build                           → CullingView in chunk lazy separato
npm run lint (eslint --max-warnings 0)  → pulito
```

Bundle d'ingresso (stessa misura della CI: solo gli asset referenziati
da `dist/index.html`): **80.296 byte gzip su un budget di 153.600**
(52%). `CullingView-*.js` (3,21 KB gzip) e il chunk condiviso
`culling-*.js` (1,41 KB gzip, store + client API) sono chunk lazy
separati, mai referenziati da `index.html` — verificato con lo stesso
comando `grep` che usa il job `frontend` della CI.

Non eseguito `cargo` in questo task: nessun file Rust toccato
(confermato con `git status --short` prima del commit), e le
istruzioni del task escludono esplicitamente Postgres/backend per
Task 9.

Vedi `task-9-report.md` per il dettaglio completo.
