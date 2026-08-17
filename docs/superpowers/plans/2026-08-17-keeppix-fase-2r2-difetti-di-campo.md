# Fase 2R2 — difetti trovati dal field test sull'archivio reale

**Stato:** ✅ **chiuso**. D1–D4 corretti e verificati sul branch `fase-2r2`
(HEAD `7cc3bed`). Terzo field test sull'archivio reale, con la libreria ferma
28 minuti dopo la fine della scansione: `discover_library` resta a **1**
(prima della correzione di D4 erano 482 in 34 minuti), `extract_metadata` e
`hash_asset` restano a 779. Pronto per il merge.
**Branch di partenza:** `fase-2r` (HEAD `716e253`).
**Branch di lavoro:** `fase-2r2`.

Questo documento è **autocontenuto**: contiene il codice difettoso verbatim, le
osservazioni misurate, l'aritmetica che le collega, e la correzione. Non serve
aver visto la sessione in cui i difetti sono emersi.

---

## Come sono stati trovati

`scripts/field-test.sh` eseguito su archivio reale, non su fixture:

| | |
|---|---|
| Archivio | `/Volumes/NVME/Immagini/Vacanze/2026/07/Vacanze Giovanni 2026` |
| Contenuto | **779 file `.ARW`** (Sony RAW), 36 GB, 12 cartelle annidate |
| Montaggio | bind mount **read-only** su `/photos` (verificato con `docker inspect`) |
| Build | release, immagine distroless, Docker Desktop su macOS |
| Durata osservata | 16 minuti prima dello smontaggio manuale |

Tutte le cifre qui sotto vengono da query su quel database, non da stime.

### Un falso allarme, già escluso — non riaprirlo

Un primo conteggio con `find -iname '*.arw'` dava **1558** file e faceva
sembrare che la discovery ne perdesse metà. È sbagliato: macOS aveva scritto un
file AppleDouble `._DSC0xxxx.ARW` accanto a ogni scatto, e `find` li contava.

```
779 file .ARW reali  +  779 sidecar ._*.ARW  =  1558
```

Il walker esclude i nomi che iniziano con `.` (`walk.rs`, `is_excluded_name`),
quindi **li salta correttamente**. La discovery ha trovato 779 ARW su 779.
**Non c'è nessun difetto di copertura del walker.** Se qualcuno riconta i file,
usi `! -name '._*'`.

### Cosa invece funziona, ed è bene sapere che funziona

La 2R ha centrato il suo obiettivo: la scansione è partita da
`POST /api/v1/libraries/{id}/scan` **senza riavviare il container**, e i primi
asset sono comparsi nel database dopo circa 5 minuti di cammino. Il baseline
precedente era 650 secondi e zero asset. Il difetto D1 della 2R è chiuso.

I difetti qui sotto **non sono della 2R**: sono della Fase 1b e della Fase 2, e
sono rimasti invisibili perché nessun test li attraversava end-to-end.

---

## D1 — `detect_kind` non è mai chiamata: l'intera pipeline RAW è codice morto

**Gravità: critica.** È il difetto che rende il prodotto inutilizzabile.

### Osservazione

```
SELECT kind, count(*) FROM assets GROUP BY 1;
 unknown | 808
```

Tutti gli 808 asset hanno `kind = unknown`. Nessuno è `raw_image`, benché 779
siano Sony ARW.

Conseguenza diretta, dalla coda:

```
derive_asset | failed | 2154
```

con, nei log del worker:

```
worker: decode: Error parsing image. Illegal start bytes:4949
worker: decode: Error parsing image. Illegal start bytes:5369
```

`0x4949` è `II`, il magic TIFF little-endian con cui inizia ogni ARW. Il file
RAW sta arrivando al decoder JPEG.

### Causa

`crates/keeppix-jobs/src/discover.rs`, dentro `flush_batch`, riga ~125:

```rust
let asset = assets
    .upsert_discovered(NewAsset {
        folder_id: folder.id,
        filename,
        size_bytes: file.size_bytes,
        mtime: file.mtime,
        inode: file.inode,
        kind: AssetKind::Unknown,   // ← costante, mai calcolata
    })
    .await?;
```

`kind` è scritto a `Unknown` e **nessun altro job lo aggiorna mai**.

La funzione che dovrebbe classificarlo esiste ed è corretta —
`keeppix_media::detect_kind` in `crates/keeppix-media/src/kind.rs`, che
riconosce il magic TIFF e poi cerca `SONY` / `NIKON` / `Canon` nell'header. Ma:

```
$ grep -rn "detect_kind" crates/ --include="*.rs"
crates/keeppix-media/src/lib.rs:19:pub use kind::detect_kind;
crates/keeppix-media/tests/kind.rs:2,6,12,22,30,35     ← solo i suoi test
```

**`detect_kind` è chiamata esclusivamente dai propri test unitari.** Nessun
codice di produzione la invoca.

L'instradamento a valle invece è giusto — `crates/keeppix-jobs/src/hash.rs:49`:

```rust
AssetKind::RawImage => (JobKind::DeriveRaw, "derive_raw"),
_                   => (JobKind::DeriveAsset, "derive"),
```

Con `kind` sempre `Unknown` cade sempre sul ramo `_`, quindi `JobKind::DeriveRaw`
— e con lui `raw_job::run`, il demosaic, la preview incorporata, tutto il lavoro
della Fase 2 — **non viene accodato nemmeno una volta in produzione**.

È lo stesso schema dei due difetti precedenti: *il percorso testato non è il
percorso spedito*. I test della Fase 2 chiamano `derive_raw` direttamente invece
di passare per `detect_kind` → `hash.rs` → dispatch, e per questo passano
mentre la catena reale è interrotta.

### Correzione

Classificare dentro `metadata::run`, che **apre già il file** per l'EXIF: non
costa una seconda `open` per asset, cosa che su 200.000 file conta.

`crates/keeppix-jobs/src/metadata.rs`, stato attuale:

```rust
let path = folder_path.join(asset.filename.as_str());
let exif = read_exif(&path, asset.mtime).map_err(|e| JobError::Worker(e.to_string()))?;
assets.insert_exif(asset_id, &exif).await?;
```

Va aggiunta, **prima** di `read_exif`, la lettura dei primi 4 KB e la
classificazione; e la coda verso `HashAsset` va accodata **solo** se il tipo è
riconosciuto.

- Leggere al più 4096 byte (basta: `looks_like_camera_raw` cerca la stringa del
  produttore nell'header TIFF, che nei file Sony cade ben dentro il primo KB).
- `AssetRepo::set_kind(asset_id, kind)` — nuovo metodo in
  `crates/keeppix-db/src/assets.rs`, unica sede ammessa per SQL.
- Se `kind == AssetKind::Unknown`: **non** accodare `HashAsset`. Un file che non
  sappiamo decodificare non deve generare un job di derivazione destinato a
  fallire. Questo da solo elimina i 29 fallimenti `:5369` di D3.

### Test che devono fallire prima della correzione

1. `keeppix-jobs`, integrazione: dato un file con header TIFF+`SONY` in una
   libreria, dopo `discover` + `metadata` l'asset ha `kind = raw_image` **e** il
   job accodato è `derive_raw`, non `derive`.
2. Dato un file di testo con estensione media, dopo `metadata` l'asset resta
   `unknown` e **nessun** job `hash_asset` viene accodato.
3. Un test che sarebbe fallito allora: partendo da un ARW reale in fixture,
   `detect_kind` viene raggiunta. Se qualcuno rimette `kind: AssetKind::Unknown`
   fisso in `discover.rs`, il test 1 deve diventare rosso.

---

## D2 — ogni riscansione riaccoda tutto: sul Pi è un ciclo di re-hash infinito

**Gravità: critica sull'hardware bersaglio.** È il difetto peggiore per il
vincolo di leggerezza.

### Osservazione

In **16 minuti**, su una libreria di 808 asset che non è mai cambiata (il mount
è read-only):

```
discover_library | done    |    3        finestra 07:21:02 -> 07:33:30
discover_library | running |    1        07:36:49        ← la quarta
extract_metadata | done    | 2631
extract_metadata | pending |  210
hash_asset       | done    | 2583
hash_asset       | pending |   48
```

L'aritmetica è netta: `808 × 3 = 2424`, e i conteggi la superano perché la
quarta passata era già in corso. Ogni discovery riaccoda **l'intero** lavoro di
estrazione metadati e di hashing per **ogni** asset, anche se nessun file è
cambiato. Cadenza osservata: una riscansione ogni 4–5 minuti.

Su questo archivio significa ri-leggere e ri-hashare 36 GB ogni pochi minuti.
Sul bersaglio dichiarato — **Raspberry Pi 5 con 200.000 foto** — significa che
la macchina non smette mai di macinare l'intera libreria, e il disco non si
ferma mai. Viola il vincolo di `AGENTS.md` in modo non marginale.

### Causa

`crates/keeppix-jobs/src/discover.rs`, `flush_batch`: l'accodamento è
incondizionato, non guarda se `upsert_discovered` ha effettivamente cambiato
qualcosa.

```rust
let asset = assets.upsert_discovered(NewAsset { ... }).await?;
jobs.enqueue(
    JobKind::ExtractMetadata,
    serde_json::json!({ "asset_id": asset.id.to_string() }),
    JobPriority::Background,
    Some(&format!("meta:{}", asset.id)),
)
.await?;
```

Il `dedup_key` `meta:{id}` non protegge: deduplica solo contro job **pendenti**.
Quando il precedente è `done`, un nuovo `enqueue` inserisce una riga nuova.

E `upsert_discovered` non ha modo di segnalare "non è cambiato niente" —
`crates/keeppix-db/src/assets.rs:123`:

```rust
INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, inode, kind)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (folder_id, filename) DO UPDATE SET
   size_bytes = EXCLUDED.size_bytes,
   mtime      = EXCLUDED.mtime,
   inode      = EXCLUDED.inode,
   kind       = EXCLUDED.kind,
   updated_at = now()
RETURNING {COLUMNS}
```

Aggiorna sempre e restituisce sempre una riga.

**Nota:** questo `SET kind = EXCLUDED.kind` è anche una seconda mina. Dopo la
correzione di D1, `EXCLUDED.kind` resterebbe `Unknown` e **ogni riscansione
azzererebbe la classificazione di tutti gli asset**, rimettendo in moto la
catena di fallimenti. Va risolto insieme, non dopo.

### Correzione

Rendere l'upsert capace di dire "invariato", e saltare l'accodamento in quel
caso.

```sql
INSERT INTO assets (id, folder_id, filename, size_bytes, mtime, inode, kind)
VALUES ($1, $2, $3, $4, $5, $6, $7)
ON CONFLICT (folder_id, filename) DO UPDATE SET
   size_bytes = EXCLUDED.size_bytes,
   mtime      = EXCLUDED.mtime,
   inode      = EXCLUDED.inode,
   kind       = EXCLUDED.kind,
   updated_at = now()
WHERE assets.mtime      IS DISTINCT FROM EXCLUDED.mtime
   OR assets.size_bytes IS DISTINCT FROM EXCLUDED.size_bytes
RETURNING {COLUMNS}
```

Con la clausola `WHERE` sull'`DO UPDATE`, quando il file è invariato **nessuna
riga viene restituita**. La firma diventa:

```rust
pub async fn upsert_discovered(&self, new: NewAsset) -> Result<Option<Asset>, DbError>
```

e in `flush_batch`:

```rust
let Some(asset) = assets.upsert_discovered(NewAsset { ... }).await? else {
    continue;   // file invariato: niente da rifare
};
```

Questo risolve anche la mina di `kind`: l'`UPDATE` scatta **solo** quando il
contenuto è davvero cambiato, e in quel caso riportare `kind` a `Unknown` è
corretto — `metadata` verrà rieseguito e lo riclassificherà.

Attenzione a un effetto collaterale voluto: un asset invariato la cui
derivazione era fallita non viene ritentato dalla riscansione. È giusto così —
il ritentativo dei job falliti è una responsabilità separata, e va tracciata come
voce differita nel ledger, non risolta qui di straforo.

### Test che devono fallire prima della correzione

1. `keeppix-jobs`, integrazione: eseguire `discover::run` **due volte** sulla
   stessa libreria senza toccare i file. Dopo la seconda, il numero di job
   `extract_metadata` deve essere **identico** a dopo la prima. Oggi raddoppia.
2. Toccare `mtime` di un solo file e rieseguire: deve comparire **esattamente
   un** nuovo `extract_metadata`.
3. `keeppix-db`, unitario: `upsert_discovered` con gli stessi identici valori
   restituisce `None` la seconda volta; con `size_bytes` diverso restituisce
   `Some`.
4. Regressione su D1+D2 insieme: dato un asset già classificato `raw_image`, una
   riscansione che non cambia il file **non** deve riportarlo a `unknown`.

### Da verificare durante l'esecuzione

La cadenza di 4–5 minuti non è stata attribuita con certezza. L'ipotesi è che il
watcher sia caduto in `WatcherMode::Polling` perché gli eventi fsevents non
attraversano il bind mount di Docker Desktop su macOS. Va confermato leggendo il
modo scelto a runtime e, se è polling, va verificato che l'intervallo sia
configurabile e che il default sia sensato per un Pi. Anche in modo nativo,
comunque, la correzione qui sopra resta necessaria: senza, ogni singolo evento
sul filesystem costa una riscansione completa.

---

## D3 — il walker indicizza i sidecar `.DOP`

**Gravità: bassa,** ma è la fonte dei 29 fallimenti `:5369` e sporca la timeline.

### Osservazione

```
SELECT upper(substring(filename from '\.[^.]+$')), count(*) FROM assets GROUP BY 1;
 .ARW | 779
 .DOP |  29
```

`808 = 779 + 29`. I `.DOP` sono i sidecar di DxO PhotoLab: file di testo, non
immagini. Producono 29 job `derive_asset` che falliscono con
`Illegal start bytes:5369`.

### Causa

`crates/keeppix-media/src/walk.rs`, `is_excluded_name`, è una **denylist** che
copre solo `.xmp` e alcuni nomi noti:

```rust
matches!(name, "@eaDir" | "Thumbs.db" | "#recycle" | "#snapshot"
              | ".keeppix-trash" | ".keeppix-tmp")
    || name.starts_with('.')
    || Path::new(name).extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xmp"))
```

Tutto ciò che non è escluso diventa un asset.

### Correzione

**Non** passare a una allowlist di estensioni: romperebbe il principio scritto
nel doc comment di `detect_kind` («il tipo dai magic number, non
dall'estensione») e taglierebbe fuori formati validi ma non previsti.

Due mosse, complementari:

1. La correzione di D1 già impedisce che un `unknown` generi un job di
   derivazione. Da sola basta a togliere i 29 fallimenti.
2. In aggiunta, estendere la denylist alle estensioni di sidecar note —
   `dop`, `pp3`, `arp`, `thm`, `aae`, oltre a `xmp` — così su 200.000 file il Pi
   non paga 200.000 `open` inutili solo per scoprire che non sono immagini.

E la timeline deve escludere gli asset `unknown`: un file che non sappiamo
decodificare non è una foto da mostrare.

### Test

1. Un walk su una directory con `foto.ARW` + `foto.ARW.dop` produce **un** solo
   `WalkedFile`.
2. Un asset `unknown` non compare nella risposta della timeline.

---

## D4 — la riscansione nativa si autoalimenta all'infinito

**Gravità: critica.** Trovato **dopo** i task 1–5 sopra, rieseguendo il field
test sul branch `fase-2r2` (commit `925b70e`) che li implementa. I task 1–4
funzionano — vedi «Cosa è stato verificato» più sotto — ma questo difetto è
nuovo e resta aperto.

### Osservazione

Stesso archivio, stessa libreria, **lasciata ferma**. Query ripetute nel
tempo:

```
09:09:14  discover_library: 482 job, cadenza ~ogni 2-3s, nessun segno di rallentare
09:09:14  extract_metadata: 779   hash_asset: 779        ← fermi, corretto
```

**34 minuti dopo l'avvio**, con la scansione iniziale e tutte le derivazioni
finite da tempo (l'ultimo lavoro pesante risale a **08:41**), il container
continua ad accodare `discover_library` **ogni 2-3 secondi, senza fermarsi
mai**. Non è transitorio: il conteggio cresce linearmente e senza segni di
esaurimento per tutta la finestra osservata.

I task 1–3 (D1+D2) funzionano correttamente: ogni singola riscansione non
riaccoda più `extract_metadata`/`hash_asset` per gli 808 asset invariati — i
loro conteggi restano fermi. **Il problema non è più il costo di una
riscansione. È che la riscansione non si ferma mai.** Su un archivio di
200.000 foto, ogni passata cammina comunque l'intero albero e fa uno `stat()`
per file: anche resa "economica" a valle, farla ogni 2-3 secondi per sempre
è esattamente il tipo di consumo continuo che il vincolo di leggerezza vieta.

### Causa

`crates/keeppix-jobs/src/watch.rs`, `watch_native`, riceve gli eventi dal
watcher `notify` e li filtra con:

```rust
fn interesting(event: &Event) -> bool {
    event.paths.iter().any(|p| !is_hidden_path(p))
}
```

Filtra solo per **percorso**. Non guarda mai `event.kind`. Il tipo
`notify::Event` porta un `EventKind` con cinque famiglie (`notify-types
2.1.0`, `event.rs`): `Access`, `Create`, `Modify`, `Remove`, `Other`/`Any`. Un
evento `Access` — apertura o lettura di un file, **senza alcuna modifica** —
oggi passa il filtro `interesting` esattamente come una scrittura vera.

`std::fs::metadata()` (lo `stat()` fatto su ogni file a ogni `discover::run`,
e anche dentro `hash.rs`/`metadata.rs` per leggere size/mtime) è già di per sé
un accesso. Su un bind mount Docker Desktop via virtiofs, questi accessi sono
noti per essere rispecchiati come eventi filesystem verso l'host. Il quadro
osservato è coerente con un ciclo chiuso:

```
discover cammina l'albero e fa stat() su 779 file
  → il bridge virtiofs genera eventi di accesso sull'host
    → il watcher nativo li riceve, "interesting" li accetta
      → dopo il debounce di 2s, enqueue_rescan
        → una nuova discover cammina l'albero e fa stat() su 779 file
          → (si ripete da capo, indefinitamente)
```

Il timing lo conferma: la cadenza osservata (~2-3s) è compatibile con
`DEFAULT_DEBOUNCE = Duration::from_secs(2)` più il tempo di camminare 779
file. E la tempesta non accenna a fermarsi nemmeno 28 minuti dopo che
l'ultimo lavoro di lettura "genuino" (hash, EXIF, demosaic) è terminato —
l'unica attività filesystem rimasta è la discovery stessa.

### Correzione

Filtrare per tipo di evento, non solo per percorso. Reagire solo a
`Create`, `Modify`, `Remove`; ignorare `Access` e la famiglia generica
`Any`/`Other` quando il backend non sa essere più preciso:

```rust
fn interesting(event: &Event) -> bool {
    use notify::EventKind;
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|p| !is_hidden_path(p))
}
```

Va verificato che questo non rompa il rilevamento di file nuovi/modificati
sui backend reali (inotify su Linux, FSEvents su macOS): `Create` e `Modify`
sono le famiglie che contano per la scoperta di foto nuove o cambiate;
`Access` non lo è mai, per definizione del tipo stesso (il doc comment di
`EventKind::Access` in notify-types lo dice esplicitamente: «non-mutating
access operations»).

Come difesa in profondità, oltre al filtro sul tipo: **limitare la cadenza
minima fra due riscansioni della stessa libreria**, indipendentemente da
quanti eventi arrivano nel frattempo. Anche con `interesting()` corretto, un
editor RAW di terzi che tocca ripetutamente i sidecar, o un futuro storage di
rete più rumoroso di virtiofs, non deve poter causare più di una riscansione
ogni N secondi. Un intervallo minimo (es. non più di una ogni 30s per
libreria) è economico da implementare accanto al debounce esistente e chiude
la classe di problema, non solo l'istanza osservata.

### Test che devono fallire prima della correzione

1. Costruire un `Event` con `EventKind::Access(AccessKind::Open(_))` su un
   percorso non nascosto: `interesting()` deve restituire `false`. Oggi
   restituisce `true`.
2. Costruire un `Event` con `EventKind::Modify(ModifyKind::Data(_))`:
   `interesting()` deve restituire `true` (invariato — non deve regredire il
   rilevamento reale).
3. Un test di integrazione più difficile da isolare senza toccare
   `notify` reale: aprire (senza scrivere) tutti i file di una libreria
   fittizia dopo l'avvio del watcher, e verificare che **non** venga accodata
   nessuna `discover_library` aggiuntiva entro il periodo di debounce più un
   margine. Se l'ambiente CI non genera eventi `Access` in modo affidabile
   (molti filesystem Linux in container non li emettono senza `mount
   -o strictatime` e flag dedicati), documentare l'esito nel ledger invece di
   far finta che il test provi qualcosa che non può verificare in quel
   contesto.

### Da verificare durante l'esecuzione

Il timing osservato è coerente con la teoria del ciclo chiuso, ma non è stato
isolato con un test che disattiva la discovery e osserva se la tempesta
cessa da sola. Se dopo la correzione del filtro la tempesta persistesse,
significherebbe che la causa non è (solo) `Access`, e andrebbe ripresa
l'indagine guardando `event.kind` effettivo ricevuto in produzione (loggarlo
temporaneamente al livello `trace`).

---

## Ordine di esecuzione

I difetti sono intrecciati: D2 va fatto **insieme** a D1, non dopo, perché la
riga `SET kind = EXCLUDED.kind` annullerebbe D1 alla prima riscansione.

| # | Task | File principali |
|---|---|---|
| 1 | `AssetRepo::set_kind` + `upsert_discovered -> Option<Asset>` con `WHERE` | `crates/keeppix-db/src/assets.rs` |
| 2 | `flush_batch` salta gli invariati | `crates/keeppix-jobs/src/discover.rs` |
| 3 | `metadata::run` legge 4 KB, chiama `detect_kind`, persiste, e accoda `HashAsset` solo se il tipo è noto | `crates/keeppix-jobs/src/metadata.rs` |
| 4 | Denylist estesa ai sidecar; timeline esclude `unknown` | `crates/keeppix-media/src/walk.rs`, timeline |
| 5 | Verifica del modo watcher e dell'intervallo di polling | `crates/keeppix-jobs/src/watch.rs` |
| 6 | `interesting()` filtra per `EventKind`, non solo per percorso; cadenza minima fra riscansioni della stessa libreria | `crates/keeppix-jobs/src/watch.rs` |

Task 1–3 vanno in un unico ciclo di verifica: separarli lascia il repository in
uno stato in cui i test di D1 passano e quelli di D2 no, o viceversa. Task 6
(D4) è stato scoperto **dopo** l'esecuzione di 1–5 e non dipende da loro, ma
va comunque chiuso prima del merge: senza, l'istanza non si ferma mai di
camminare l'albero.

## Cosa è stato verificato — field test su `fase-2r2` (commit `925b70e`)

Rieseguito lo stesso field test (779 ARW reali, 36 GB) su branch pulito. Stato
al momento della verifica, con la libreria ferma:

| Criterio | Esito |
|---|---|
| `kind = raw_image` per tutti gli asset RAW, zero `.DOP` indicizzati | ✅ `raw_image \| 779`, nessuna estensione `.DOP` fra gli asset |
| `derive_raw` presente, zero `derive_asset \| failed` | ✅ `derive_raw \| done \| 761`, nessun fallimento di alcun tipo |
| Timeline con miniature reali nel browser | ✅ verificato con login e screenshot: foto reali, non placeholder |
| Stabilità dei job pesanti | ✅ `extract_metadata` e `hash_asset` restano fermi a **779** anche dopo **482 riscansioni** in 34 minuti — D1+D2 tengono sotto stress ben oltre lo scenario previsto |
| Nessuna riscansione oltre la prima entro 15 minuti a libreria ferma | ❌ **482 riscansioni** in 34 minuti, cadenza ~2-3s, nessun segno di fermarsi — vedi **D4** |

I `761` job `derive_raw` contro `689` hash `content_hash` distinti (779 asset,
alcune foto duplicate fra cartelle) non sono un difetto: il codice ha già un
guard di idempotenza in `raw.rs` (`if thumb_path.is_file() { return Ok(()) }`)
che salta il demosaic — l'unico passo davvero costoso — quando il derivato
esiste già. I job in più costano uno `stat()`, non un demosaic.

## Criterio di chiusura

Non è «i test passano». È il field test rieseguito sullo stesso archivio:

- [x] `SELECT kind, count(*) FROM assets` restituisce `raw_image | 779`, e
      nessun `.DOP` fra gli asset;
- [x] `SELECT kind, status, count(*) FROM jobs` mostra job `derive_raw`, e
      **zero** `derive_asset | failed`;
- [ ] lasciando girare l'istanza **15 minuti a libreria ferma**, i conteggi di
      `extract_metadata` e `hash_asset` **non aumentano** — verificato, tengono;
      **ma la riga sotto è nuova e blocca comunque la chiusura**:
- [x] lasciando girare l'istanza **15 minuti a libreria ferma**, il conteggio
      di `discover_library` **non cresce oltre la prima passata** — verificato
      con la correzione di D4 (`9badcb8`): **28 minuti** a libreria ferma,
      `discover_library` resta a **1** (era 482 in 34 minuti prima della
      correzione);
- [x] aprendo la timeline nel browser si vedono le miniature, non riquadri vuoti.

Tutti i criteri sono soddisfatti. Il criterio della Fase 2R — «una persona, da
istanza vuota e usando solo il browser, crea l'admin, aggiunge una libreria,
avvia la scansione e vede le foto» — è soddisfatto, e la macchina smette di
lavorare quando non c'è nulla da fare.

## Storico dei field test

| Report | Esito |
|---|---|
| `.superpowers/field-test-20260817-0918.md` | Primo run 2R2, prima di D1-D3: scoperti D1/D2/D3 |
| `.superpowers/field-test-20260817-1034.md` | Dopo D1-D3 (`925b70e`): D1-D3 confermati, scoperto D4 |
| `.superpowers/field-test-20260817-1146.md` | Dopo D4 (`7cc3bed`): tutti i criteri soddisfatti, chiusura |

## Lezione di processo, da non perdere

Tre difetti su tre, in tre fasi diverse, sono dello stesso tipo: **una funzione
scritta, testata, e mai collegata al percorso reale.** `restat_if_stable` con lo
sleep, la scansione che richiedeva il riavvio, e ora `detect_kind`.

I test unitari non li vedono per costruzione, perché invocano la funzione
direttamente — che è esattamente ciò che la produzione non fa.

La contromisura non è «più test unitari». È che **ogni fase si chiude con un
passaggio end-to-end su dati reali**, e che ogni funzione pubblica di
`keeppix-media` abbia almeno un chiamante di produzione — verificabile con un
`grep`, e quindi automatizzabile in CI.
