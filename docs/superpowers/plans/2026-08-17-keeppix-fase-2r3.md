# Fase 2R3 — leggerezza dei derivati e debiti di processo

**Stato:** ✅ **chiusa e mergiata in `main`** (PR #7, `81b09e5`). Task 1-8 più i
rilievi R1-R6 tutti verificati sull'archivio reale; vedi la sezione finale di
questo documento per le misure.
**Branch di partenza:** `main`, dopo il merge di `fase-2r2`.
**Branch di lavoro:** `fase-2r3`.

Questo documento è **autocontenuto**: codice verbatim, misure reali, e la
correzione. Non serve aver visto le sessioni in cui i difetti sono emersi.

## Perché una fase a sé

La Fase 3 porta multiutente, condivisione e link pubblici: funzioni nuove. Le
voci qui dentro sono **debiti e peso**, e mescolarle avrebbe reso la 3
impossibile da rivedere. Nessuna di queste è una funzione nuova: sono cose
che esistono e non funzionano, o funzionano costando troppo.

Il vincolo che le governa è quello di `AGENTS.md`: **Raspberry Pi 5, 8 GB,
200.000 foto.** Ogni numero qui sotto va pesato contro quella macchina.

## Il numero che giustifica la fase

Field test reale (779 ARW Sony, 36 GB, tre esecuzioni concordi): **1,2 GB di
derivati**, cioè **~1,54 MB per foto**.

I derivati hanno dimensioni **fisse** (240 px e 1440 px): scalano col
**numero** di foto, non col peso degli originali. Quindi su 200.000 foto sono
**~308 GB**, che gli originali siano RAW o JPEG.

Non è una stima: è il rapporto misurato moltiplicato per il bersaglio
dichiarato.

---

## Task 1: codifica con perdita — il difetto che costa 268 GB

### Causa

`crates/keeppix-media/src/derive.rs:223`:

```rust
fn write_webp_atomic(path: &Path, rgb: &[u8], w: u32, h: u32) -> Result<(), DeriveError> {
    let tmp = path.with_extension(format!("webp.{}.tmp", std::process::id()));
    let mut buf = Vec::new();
    WebPEncoder::new(&mut buf)
        .encode(rgb, w, h, image_webp::ColorType::Rgb8)
        .map_err(|e| DeriveError::Decode(e.to_string()))?;
    …
}
```

`image-webp = "0.2"`. Il sorgente del crate, `encoder.rs:631`, dichiara:

```
/// Only supports "VP8L" lossless encoding.
```

e scrive chunk `VP8L`. **Ogni derivato è WebP senza perdita** — l'equivalente
di un PNG, per immagini destinate alla visualizzazione a schermo.

Nessuno l'ha deciso: discende da quale crate è stato scelto per scrivere WebP.
Ed è il caso peggiore, perché il lossless a 1440 px conserva dettaglio che il
**ridimensionamento ha già buttato via**: si paga ~8× lo spazio per
informazione che non c'è più.

### Correzione

Passare a **WebP con perdita via libwebp** (decisione dell'utente, presa il
2026-08-17). Il crate `webp` espone i binding; va scelta e **fissata** la
versione, annotandola nel ledger insieme al motivo della dipendenza, come
chiede la regola sulle dipendenze in `AGENTS.md`.

**Sulla regola dei decoder C.** `AGENTS.md` impone che «i decoder scritti in C
girano in un processo separato con `rlimit` e seccomp». Quella regola nasce dal
**decodificare input non fidato**. Qui si **codifica** un buffer RGB che
abbiamo già decodificato noi: l'input di libwebp sono i nostri byte, non i
byte dell'utente. Profilo di rischio diverso, quindi **niente sandbox per
l'encoder** — ma la deroga va scritta nel ledger, non lasciata implicita.

**Qualità configurabile**, default **82**. Sotto 75 si guadagna poco e si
inizia a vedere; sopra 88 si paga molto per una differenza invisibile.
Miniatura e anteprima possono avere qualità diverse: la miniatura da 240 px
regge bene un valore più basso.

### Verifica del build

libwebp si compila da sorgente. `rust:1.88-bookworm` (lo stage `backend` del
`Dockerfile`) ha già gcc, e il runtime `distroless/cc-debian12` porta libc e
libgcc: la catena regge. **Va comunque verificato costruendo l'immagine**, non
dato per buono — e va misurato quanto allunga il build in CI, perché è un costo
ricorrente.

### Test

1. Un'anteprima derivata da un'immagine di prova pesa **meno di un terzo**
   dell'equivalente lossless odierno. Soglia larga di proposito: il test
   protegge dal ritorno al lossless, non certifica un rapporto esatto.
2. Il file prodotto è WebP **con perdita** — non basta l'estensione: va
   verificato che il chunk sia `VP8 ` e non `VP8L`.
3. La qualità è configurabile, il default è documentato in `docs/DEPLOY.md`.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 2: risoluzione dell'anteprima, e lo zoom che oggi non funziona sui RAW

### Il difetto

`frontend/src/views/CullingView.vue` usa tre sorgenti:

| Vista | Sorgente |
|---|---|
| Griglia timeline, ricerca, filmstrip | `/media/thumb/{hash}` — 240 px |
| Apertura foto, culling normale, confronto | `/media/preview/{hash}` — 1440 px |
| **Zoom del culling (`z`)** | **`/media/original/{id}`** |

E `crates/keeppix-api/src/routes/media.rs:87` serve l'originale così com'è:

```rust
let path = folder_path.join(asset.filename.as_str());
stream_file(&path, …, mime_for_name(asset.filename.as_str()), false).await
```

Su un archivio RAW quel percorso mette i byte di un `.ARW` dentro un `<img>`.
**Nessun browser lo disegna.** Lo zoom — la funzione pensata per il fotografo
che scarta a fuoco, cioè l'utente di riferimento della Fase 2 — non mostra
nulla sui RAW.

E `preloadOriginal` precarica l'originale per rendere lo zoom istantaneo: su
questo archivio sono **46 MB per foto** trasferiti in anticipo per
un'immagine che poi non si vede.

> **Da verificare per primo, prima di correggere.** Questo difetto è stato
> trovato **per ispezione**, non nel browser. Il percorso di codice è univoco,
> ma va confermato aprendo un RAW nel culling e premendo `z`. Se si vedesse,
> l'analisi è sbagliata e va rifatta — non si correggono difetti non
> riprodotti.

### Il progetto — e perché è quasi gratis

Gli ARW Sony incorporano un JPEG **a dimensione piena**.
`keeppix_media::extract_embedded_preview` lo estrae già a ogni derivazione, e
`raw.rs:126` lo usa quando il lato lungo è ≥ `MIN_PREVIEW_LONG_SIDE` (1440).
Quindi l'immagine ad alta risoluzione **è già estratta e già decodificata** a
ogni derivazione RAW — e poi buttata via.

Ma scriverla per tutti sarebbe il difetto del Task 1 al contrario: un derivato
a piena risoluzione, anche con perdita, costa ~1,5-2,5 MB a foto, cioè
**300-500 GB su 200.000 foto**. Inaccettabile.

**Tre livelli, con politiche diverse:**

| Livello | Dimensione | Quando |
|---|---|---|
| `thumb` | 240 px | subito, per tutti |
| `preview` | **2048 px** (era 1440) | subito, per tutti |
| `full` | piena risoluzione | **pigro**: alla prima richiesta di zoom |

**Perché alzare l'anteprima a 2048.** A 1440 px, aperta a tutto schermo su un
monitor 4K, l'immagine è già interpolata e morbida: è un limite di
risoluzione, che il lossless non risolveva. Con la codifica con perdita del
Task 1, **2048 px pesano comunque meno dei 1440 px lossless di oggi**: si
guadagna qualità *e* spazio nello stesso passaggio.

**Perché il livello `full` è pigro.** La stragrande maggioranza delle foto non
viene mai zoomata. Generarlo su richiesta e conservarlo in cache è l'unica
politica compatibile col Pi. Serve un **tetto alla dimensione della cache**,
con sfratto del meno usato di recente: senza tetto è una perdita di spazio
lenta, che è esattamente il difetto del cestino (Task 4) in un'altra forma.

`preloadOriginal` nel frontend **non deve più precaricare il RAW**: deve
puntare al livello `full`.

### Test

1. Aperto un asset `raw_image` nel culling e premuto `z`, la risposta ha un
   content-type che il browser sa disegnare, e **non** è il file RAW.
2. Il livello `full` **non** viene generato dalla derivazione iniziale: dopo
   una scansione completa, su disco ci sono solo `thumb` e `preview`.
3. Alla prima richiesta di zoom il livello `full` viene generato; alla seconda
   viene servito dalla cache senza rigenerarlo.
4. Superato il tetto, la cache sfratta e non cresce oltre.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 3: il thumbhash si perde sulle foto duplicate

### Osservazione

Due esecuzioni identiche dello stesso field test, stesso commit, danno numeri
**diversi**:

```
run 10:34   Con thumbhash | 707 / 779
run 11:46   Con thumbhash | 700 / 779
```

779 asset, **689 `content_hash` distinti**: 90 foto sono la stessa immagine in
due cartelle. Gli asset senza thumbhash sono un sottoinsieme di quei 90, e
quali siano cambia a ogni esecuzione.

### Causa

Il guard di idempotenza in `crates/keeppix-jobs/src/raw.rs`:

```rust
let (thumb_path, _) = derivative_paths(data_dir, &hash);
if thumb_path.is_file() {
    return Ok(());          // ← esce senza propagare il thumbhash
}
```

e la propagazione avviene per `content_hash`
(`crates/keeppix-db/src/assets.rs:391`):

```sql
UPDATE assets SET thumbhash = $2, updated_at = now() WHERE content_hash = $1
```

La corsa:

```
asset A e asset B sono la stessa foto (hash H) in cartelle diverse
  hash_job(A) → set_hash(A, H) → accoda derive_raw:H
    derive_raw(H) → deriva, scrive il file, UPDATE ... WHERE content_hash = H
                     → aggiorna solo A: B non ha ancora content_hash
  hash_job(B) → set_hash(B, H) → accoda di nuovo derive_raw:H
    derive_raw(H) → il file c'è già → return Ok(()) → B resta senza thumbhash
```

**Impatto contenuto ma reale:** il derivato esiste, quindi la foto si vede;
manca il placeholder sfocato del caricamento progressivo su circa il 10% degli
asset.

### Correzione

Nel ramo di uscita anticipata, propagare il thumbhash già noto a chi non ce
l'ha, senza rifare nulla:

```sql
UPDATE assets SET thumbhash = src.thumbhash, updated_at = now()
  FROM (SELECT thumbhash FROM assets
         WHERE content_hash = $1 AND thumbhash IS NOT NULL LIMIT 1) src
 WHERE content_hash = $1 AND thumbhash IS NULL
```

Una query, nessun ricalcolo, nessuna lettura di file.

### Test

Due asset in cartelle diverse con lo stesso contenuto; far completare
`derive_raw` per il primo, poi assegnare `content_hash` al secondo ed eseguire
di nuovo `derive_raw` con lo stesso hash. **Entrambi** devono avere `thumbhash`
non nullo. Oggi il secondo resta `NULL`.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 4: `TrashRepo::cleanup_expired` non ha chiamanti di produzione

### Osservazione

```
$ grep -rn "cleanup_expired" --include="*.rs" crates/
crates/keeppix-db/src/trash.rs:403:    pub async fn cleanup_expired(...)   ← definizione
crates/keeppix-db/tests/trash.rs:383,431                                    ← solo test
```

**Zero** chiamanti di produzione. Esiste la rotta manuale `/trash/empty`, ma
**la scadenza automatica non avviene mai**: le foto cancellate restano su disco
per sempre.

Su un Pi con storage limitato è una perdita di capacità silenziosa, e rompe la
promessa di conservazione a termine fatta all'utente.

### Correzione

Schedulare la potatura come job periodico, con la disciplina degli altri job di
manutenzione: priorità bassa, cadenza da operazione di pulizia e non
interattiva. La finestra di conservazione si legge dalla configurazione, non si
incide nel codice.

### Test

Una riga in cestino più vecchia della finestra sparisce **senza intervento
manuale**; una più recente resta.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 5: ritentativo dei job di derivazione falliti

Voce differita correttamente nel ledger della 2R2 («il ritentativo dei derive
falliti non passa dalla riscansione»), che **nessun piano possedeva**.

Oggi un fallimento transitorio — disco occupato, processo di demosaic ucciso
dal gate della RAM — lascia la foto **senza miniatura per sempre**: la
riscansione non la ritenta, perché la 2R2 salta correttamente gli asset
invariati.

Serve un ritentativo con **backoff** e un **numero massimo di tentativi**, che
non passi dalla riscansione. La rotta `/problems` esiste già e mostra gli asset
in errore: il ritentativo va reso visibile lì.

**Test.** Un job fallito viene ritentato dopo il backoff; superato il tetto
smette e resta visibile in `/problems`; un fallimento permanente non genera
tentativi infiniti.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 6: il WebSocket esiste nel backend e nessuno lo usa

### Osservazione

```
$ grep -rn "WebSocket" frontend/src/         → nessun risultato
$ grep -rn "ws::" crates/keeppix-api/src/lib.rs
  .route("/ws/ticket", post(routes::ws::ticket))
  .route("/ws",        get(routes::ws::connect))
```

Il backend ha l'implementazione **completa e montata**: ticket monouso
consumato prima dell'upgrade, validazione dell'`Origin`, backpressure con
`resync`, test in `crates/keeppix-api/tests/ws.rs`, voci in OpenAPI. Il
frontend non ci si collega **mai**.

Il ledger della 2R lo registra di sfuggita (`fase-2r/progress.md:91`):
«avanzamento scansione via **polling** […] non WebSocket — il piano cita WS ma
il task chiede polling per semplicità; **WS non è cablato nel frontend**». La
ripiegatura è stata scritta; la lacuna che la rendeva necessaria no.

**Conseguenza:** la timeline **non si aggiorna in diretta**. Mentre una
scansione lavora, le foto nuove non compaiono finché non si ricarica — che è
esattamente ciò che il WebSocket doveva risolvere, e la ragione per cui fu
scelto rispetto a SSE.

### Correzione

Cablare il client WebSocket nel frontend e usarlo per l'aggiornamento in
diretta della timeline. Il polling del wizard di setup **può restare** — è un
uso una tantum su una pagina che l'utente sta guardando — ma va allora
dichiarato come scelta nel ledger, non lasciato come ripiego.

Il client deve riconnettersi da solo alla caduta, e usare il `resync` già
previsto dal protocollo quando è rimasto indietro.

### Test

Con una scansione in corso, la timeline mostra le foto nuove **senza
ricaricare la pagina**. Caduta la connessione, il client si riconnette e
riallinea.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 7: la guardia in CI contro la sesta occorrenza

Cinque difetti di questo progetto sono **lo stesso difetto**: una funzione
scritta, testata, e mai collegata al percorso reale.

| # | Cosa | Trovato da |
|---|---|---|
| 1 | `restat_if_stable` dormiva 5 s per file | field test |
| 2 | la scansione richiedeva il riavvio del container | field test |
| 3 | `detect_kind` mai chiamata → pipeline RAW morta | field test |
| 4 | `TrashRepo::cleanup_expired` mai chiamata (Task 4) | `grep` |
| 5 | il WebSocket mai usato dal frontend (Task 6) | `grep` |

Tutti si trovano con un `grep`. Nessuno è stato trovato dai test unitari, **per
costruzione**: un test unitario invoca la funzione direttamente, che è
esattamente ciò che la produzione non fa.

### Correzione — in due metà, entrambe necessarie

1. **Lato Rust:** fallisce la CI se una funzione pubblica di `keeppix-media` o
   `keeppix-db` non ha almeno un chiamante fuori dai test.
2. **Lato frontend:** fallisce la CI se una rotta montata in `keeppix-api` non
   ha un consumatore in `frontend/src`.

**La seconda metà non è un extra.** Il difetto 5 sarebbe passato con la sola
prima: le rotte `/ws` e `/ws/ticket` *hanno* chiamanti lato Rust — sono montate
nel router. La lacuna stava fra backend e frontend, dove nessun `grep` sul solo
Rust può vederla.

Serve una **lista di eccezioni dichiarate**: una funzione o una rotta può
legittimamente esistere in attesa della fase che la userà. Ma l'eccezione va
**scritta, con la fase che la consumerà**. È esattamente la differenza fra una
scelta e una dimenticanza.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Task 8: la prova di scala

**Finora non abbiamo mai provato il vincolo che governa il progetto.**

`AGENTS.md` dichiara il bersaglio: **200.000 foto**. Il field test più grande
mai eseguito ne ha **779**: lo 0,4%. Provano la **correttezza**, non la
**scala**.

### Cosa fare

Una prova di scala **sintetica**: generare 200.000 righe in `assets` con date,
cartelle e permessi realistici, **senza file veri** — non servono, perché ciò
che va misurato sono le query, non l'I/O di ingestione. Costa minuti, si
rilancia a ogni fase.

Misurare, con `EXPLAIN ANALYZE` nel ledger:

| Query | Budget |
|---|---|
| `GET /timeline` prima pagina | < 300 ms |
| `GET /timeline` pagina profonda (keyset, mesi indietro) | < 300 ms |
| Conteggi per mese (intestazioni dei bucket) | < 300 ms |
| Ricerca testuale (`pg_trgm`) | < 500 ms |

**Perché adesso e non in Fase 3.** La Fase 3 mette l'ereditarietà dei permessi
dentro la query più calda del prodotto. Se il piano di query non regge a
200.000 asset, va scoperto **prima** di costruirci sopra — e questa impalcatura
diventa lo strumento con cui la Fase 3 misura il proprio budget, invece di
doverla scrivere allora.

**Se un budget non è raggiungibile**, la risposta non è alzarlo in silenzio: è
scriverlo nel ledger col numero misurato e la ragione.

### Nota onesta sui numeri che abbiamo

Tutte le misure di prestazione esistenti vengono da Docker Desktop su macOS,
dove il bind mount passa da virtiofs. La camminata dell'albero ha impiegato
~5 minuti per ~1.600 voci di directory: **~190 ms per `stat`**, quando su un
filesystem nativo sta nei microsecondi. È il costo di virtiofs, non del codice.

**Da quei numeri non si estrapola il Pi.** Questa prova misura le query, che
dipendono da Postgres e dagli indici e non dal filesystem: è l'unica metà delle
misure che si trasferisce onestamente al bersaglio.

- [ ] **Step 1-3: Scrivere, verificare, committare**

---

## Ordine di esecuzione

| # | Task | Dipendenze |
|---|---|---|
| 1 | Codifica con perdita | — |
| 2 | Anteprima 2048 + livello `full` pigro | **dopo il Task 1** |
| 3 | Thumbhash sui duplicati | — |
| 4 | Potatura automatica del cestino | — |
| 5 | Ritentativo dei derive falliti | — |
| 6 | Cablaggio del WebSocket | — |
| 7 | Guardia in CI | **dopo 4 e 6**, che sono le eccezioni che deve prendere |
| 8 | Prova di scala | — |

Il Task 2 va **dopo** il Task 1: alzare l'anteprima a 2048 px restando sul
lossless triplicherebbe lo spazio invece di ridurlo.

Il Task 7 va **dopo** 4 e 6: la guardia va scritta quando i due difetti che
deve intercettare sono ancora nel repository, così si vede fallire su qualcosa
di reale invece che su un caso costruito.

## Criteri di completamento

Non è «i test passano». È il field test rieseguito sull'archivio reale, più i
`grep`.

- [ ] Rapporto derivati/originali **sotto l'1%** (oggi 3,3%), e peso per foto
      sotto **0,4 MB** (oggi 1,54 MB).
- [ ] I derivati sono WebP **con perdita**: chunk `VP8 `, non `VP8L`.
- [ ] Dopo una scansione completa, su disco ci sono **solo** `thumb` e
      `preview`: nessun livello `full` generato in anticipo.
- [ ] Zoom su un RAW nel culling: si vede l'immagine, e non viene scaricato
      il file RAW.
- [ ] Due esecuzioni consecutive del field test danno lo **stesso** numero di
      `thumbhash IS NOT NULL`, pari al totale degli asset RAW derivati.
- [ ] Il cestino si svuota **da solo** oltre la finestra.
- [ ] La timeline si aggiorna **in diretta** durante una scansione, senza
      ricaricare.
- [ ] La CI fallisce se si aggiunge una funzione pubblica senza chiamanti o una
      rotta senza consumatore — **verificato provandoci**, non per fiducia.
- [ ] I budget di query retti a **200.000 asset** sintetici, con `EXPLAIN
      ANALYZE` nel ledger.
- [ ] **Misurato e registrato**: tempo di derivazione e RAM di picco prima e
      dopo il Task 1. L'attesa è che la codifica con perdita sia **più veloce**
      del lossless `VP8L` e che la RAM non cambi (il picco è il buffer RGB a
      piena risoluzione, non l'encoder). **Se l'attesa è smentita, vince la
      misura** e va scritta.
- [ ] Tempo di build dell'immagine Docker prima e dopo l'aggiunta di libwebp.

## Cosa NON è in questa fase

Niente di multiutente: permessi, gruppi, album, link pubblici sono la Fase 3.
Niente AVIF: comprime meglio del WebP ma costa troppo in codifica per un Pi con
200.000 foto — se un giorno si vorrà valutare, servirà una misura, non
un'intuizione.

Niente decodifica JPEG a scala ridotta (DCT scaling). Sarebbe il guadagno
grosso successivo — decodificare l'anteprima incorporata a 1/2 o 1/4 invece che
a piena risoluzione taglierebbe RAM e tempo di un fattore 4-16 — ma richiede di
cambiare decoder, e **non si cambia decoder senza prima aver misurato che il
decoder è il collo**. Voce differita, da riprendere con i numeri del Task 8 in
mano.

---

# Rilievi del field test — da chiudere prima del merge

Field test sull'archivio reale (779 ARW Sony, 36 GB) eseguito sul branch
`fase-2r3` a `1e31fb4`. Due criteri su tre pieni, il terzo a metà.

| Criterio | Esito |
|---|---|
| Rapporto derivati sotto l'1% | ✅ **0,4%** — 139 MB, 178 KB/foto (era 1,54 MB). Chunk `VP8 ` verificato sui byte |
| Thumbhash stabile fra due esecuzioni | ✅ **779/779**, zero mancanti; i **177** asset con hash duplicato — la popolazione che prima falliva — ce l'hanno tutti |
| Zoom sui RAW | ⚠️ **parziale**: si vede (prima `naturalWidth=0`) e serve `/media/full`, ma alla stessa risoluzione dell'anteprima |

`full: 0` file su disco dopo la scansione: la generazione pigra funziona.

## R1 — `full` ha la stessa risoluzione di `preview`: non serve a niente

### L'assunzione sbagliata, e di chi era

Il Task 2 è stato progettato su un'affermazione **mia**: «gli ARW Sony
incorporano un JPEG a dimensione piena, quindi il livello a piena risoluzione è
quasi gratis». **È falsa per questa fotocamera.**

Misurato su tutti i 689 derivati dell'archivio:

```
thumb:    240x160     8 KB
preview:  1616x1080  193 KB     ← mai 2048: l'incorporata è 1616, e il codice
full:     1616x1080  150 KB       fa bene a non ingrandire
```

Il JPEG incorporato più grande in questi ARW è **1616×1080**.
`extract_embedded_preview` già sceglie il più grande disponibile: non è un
difetto di estrazione, è che non c'è niente di più grande.

Quindi `full` è, su questo archivio, **un secondo file con gli stessi pixel**:
un'altra codifica, una cache da gestire, zero dettaglio in più. E lo zoom del
culling — che per la spec §4.2 serve a controllare la messa a fuoco al 100% —
mostra esattamente ciò che si vedeva già.

### Correzione

`full` deve essere **sensibilmente più dettagliato di `preview`, oppure non
esistere**. Regola:

1. anteprima incorporata **se il lato lungo supera quello di `preview`**;
2. altrimenti **demosaic** — la macchina esiste già (`demosaic_half`,
   `SandboxDemosaic`), oggi usata da `derive_raw` solo quando l'incorporata sta
   sotto `MIN_PREVIEW_LONG_SIDE`;
3. se il demosaic non è disponibile (`dcraw_emu` assente), **dirlo**: niente
   livello `full`, e il frontend degrada all'anteprima senza fingere uno zoom
   che non c'è.

`demosaic_half` dà metà sensore: su 6000×4000 sono 3000×2000, quasi il doppio
lineare di adesso. **Non è 1:1.** Se il 100% vero è un requisito, va valutato
il demosaic pieno e ne va misurato il costo — ma la decisione va scritta, non
lasciata implicita nel nome della funzione.

### Attenzione al precaricamento

Il demosaic costa **secondi**, non millisecondi, e gira in un processo separato
col gate della RAM. `preloadOriginal` nel culling precaricava in modo
aggressivo: se ora precarica `/media/full`, navigare veloce accoda una fila di
demosaic e satura il gate.

Serve: precaricamento **conservativo** (l'immagine corrente, al più la
successiva), uno stato di caricamento visibile, e nessuna richiesta in volo
quando l'utente ha già cambiato foto.

### Test

1. Su un ARW la cui incorporata è 1616×1080 e la cui `preview` è 1616,
   `/media/full` restituisce un'immagine **strettamente più grande**.
2. Su un file la cui incorporata supera già `preview`, `full` usa
   l'incorporata e **non** fa demosaic (verificabile contando le invocazioni).
3. Senza `dcraw_emu`, la richiesta non fallisce con un errore opaco: degrada in
   modo dichiarato.
4. Il tempo di demosaic su un ARW reale è **misurato e scritto nel ledger**.

## R2 — la cache scandisce l'intero albero a ogni zoom

`enforce_full_cache_cap` (`crates/keeppix-media/src/derive.rs:334`) fa un
`WalkDir` su **tutta** `data/derivatives` a **ogni** richiesta di `/media/full`.

Su 200.000 foto sono ~400.000 file in 65.536 directory, percorsi ogni volta che
l'utente preme `z`. Nel culling, dove si zooma in sequenza, è uno stallo
ripetuto — e viola il principio di `AGENTS.md` per cui niente percorre l'intero
insieme dei dati su un percorso caldo.

**Correzione.** Il costo di far rispettare il tetto deve essere indipendente
dal numero totale di derivati: un totale mantenuto in modo incrementale, oppure
lo sfratto spostato su un job periodico (come la potatura del cestino del
Task 4, che è lo stesso problema risolto bene).

`touch_accessed` va tenuto: aggiornare l'atime esplicitamente invece di
affidarsi al filesystem è la scelta giusta, perché molti mount sono `relatime`
o `noatime`.

**Test.** Con N livelli `full` in cache, far rispettare il tetto non percorre
l'albero completo — verificato con un budget che non cresce col numero di
`thumb`/`preview` presenti.

## R3 — la leva sulla velocità di libwebp non è stata toccata

Misura riportata nel ledger: il lossy è più lento del lossless (136 ms contro
45 ms). **La misura è giusta e va rispettata** — ma la causa non è «lossy
contro lossless»: è che `WebPEncoder::from_rgb(...).encode(q)`
(`derive.rs:249`) usa l'API semplice di libwebp, che significa **`method = 4`**,
il default orientato alla compressione.

`method` va da 0 a 6 e scambia velocità con dimensione. A 1-2 la codifica
accelera parecchio perdendo poco.

Sul totale l'impatto è oggi piccolo — 7m52s contro 7m30s, perché dominano hash
e I/O — ma su 200.000 foto sono ore di differenza in ingestione.

**Da fare.** Esporre `method`, misurare la curva a 0, 2, 4 su un campione
reale (tempo **e** dimensione), scegliere il default e **scrivere i numeri nel
ledger**. Vincolo: il rapporto derivati deve restare **sotto l'1%**.

## R4 — la lista di eccezioni dice «fase futura» dove sono debiti passati

Il Task 7 ha funzionato: la guardia ha scoperto molta superficie spedita e mai
raggiungibile. Verificato nel frontend:

| Rotta | Consumatori nel frontend |
|---|---|
| `/users`, `/users/me/password`, `/users/{id}`, `/users/{id}/disable` | **0** |
| `/trash`, `/trash/empty` | **0** (i riferimenti a "trash" sono etichette di culling e i18n) |
| `/metadata/batch*`, `/flags/batch` | **0** |
| `/folders/tree`, `/folders/{id}/children` | **0** |
| `/search/suggest`, `/saved-searches` | **0** |
| `/auth/refresh` | **0** |

`scripts/wired-exceptions.txt` si presenta come «the phase that will consume
them», ma quasi tutte le voci puntano a fasi **già chiuse** (`fase-0`,
`fase-1a`, `fase-1b`, `fase-1c`, `fase-2`). Non sono rinvii: sono **debiti di
fasi dichiarate complete**.

Metterli in lista per sbloccare la guardia è legittimo. Etichettarli come
attese future non lo è: nasconde che la 2R aveva scritto «una funzione che
l'utente non può raggiungere non esiste» e poi ha spedito la gestione utenti
senza interfaccia.

**Correzione.** Due sezioni distinte e dichiarate:

- **Rinvii**: consumatore previsto in una fase **non ancora eseguita**.
- **Debiti**: spediti in una fase **già chiusa** senza consumatore, con la fase
  che li salderà.

E nel README/ledger vada scritto che il backlog esiste, invece di viverne solo
in un file di eccezioni.

## R5 — `/auth/refresh` non è chiamato da nessuno: verificare cosa comporta

Emerge da R4 e merita una verifica a sé, perché non è superficie mancante ma
possibile difetto funzionale: se la SPA non rinnova mai la sessione, questa
scade durante l'uso e l'utente viene buttato fuori.

**Da fare.** Verificare il comportamento reale alla scadenza. Se l'utente viene
espulso, è un difetto di questa fase; se una rotazione avviene per altra via, è
un rinvio legittimo e va scritto quale.

## R6 — il rilevamento hardware non rileva niente

Trovato rispondendo alla domanda «l'accelerazione hardware funziona?».

**Non funziona, e non è mai stata scritta.** `crates/keeppix-media/src/probe.rs`:

```rust
pub fn probe() -> Capabilities {
    Capabilities {
        backend: "software".to_owned(),   // costante
        decode_fps: None,
    }
}
```

Nel workspace, `grep -rni "vaapi|hwaccel|qsv|nvenc|videotoolbox|v4l2"` non
restituisce **nulla**.

### Perché è un debito e non un rinvio

La spec della **Fase 1b §4** — fase dichiarata completa e già in `main` — lo
specifica per esteso:

> «Al primo avvio, e su richiesta da Impostazioni, Keeppix **misura invece di
> indovinare**.» Clip di test di 2 secondi, backend candidati in ordine
> (`rkmpp`, `nvenc`, `v4l2m2m`, `videotoolbox`, `vaapi`, `qsv`, `amf`,
> software), SoC rilevato da `/proc/device-tree/compatible`, `/proc/cpuinfo`,
> `/dev/dri/*`, `nvidia-smi`; risultato in `system_capabilities` con **gli fps
> misurati**, mostrato in Impostazioni e sovrascrivibile a mano.
>
> «Un driver presente ma rotto — capita spessissimo con V4L2 e VAAPI a metà —
> deve fallire **durante il probe**, non sul video di Natale alle 23:00.»

Niente di tutto questo esiste.

### La forma nuova del solito difetto

Le cinque occorrenze precedenti erano *funzioni senza chiamante*. Questa è
peggio: **`persist_capabilities` chiama `probe()` regolarmente e ne salva il
risultato.** Il chiamante c'è, il dato finisce in `settings`, tutto sembra
funzionare — solo che il valore è una costante.

**La guardia del Task 7 non può prenderla**, perché cerca chiamanti, non
verifica che una funzione faccia qualcosa. Vale la pena scriverlo: la guardia
copre una classe, non tutte.

### Cosa NON fare adesso

**Non implementare il probe completo in questa fase.** L'accelerazione hardware
serve alla transcodifica video, che è Fase 6 — e la spec della Fase 6 dice
esplicitamente «accelerazione hardware secondo quanto misurato dal probe della
Fase 1b». La pipeline fotografica di oggi non ha alcun percorso GPU: decodifica
JPEG e codifica WebP sono CPU. **La sua assenza non ci è costata nulla finora**,
e costruirlo qui sarebbe lavoro di Fase 6 fatto fuori posto, per giunta
dipendente da build ffmpeg con i flag giusti (la spec generale avverte che le
build statiche pronte spesso non includono VAAPI, rkmpp e v4l2m2m).

### Cosa fare adesso

**Smettere di fingere.** Oggi il valore salvato afferma `"software"` come se
fosse stato misurato. Deve dichiarare di **non essere stato rilevato** — un
backend `"unprobed"` o equivalente, con il doc comment che dice che il
rilevamento arriva in Fase 6 — così nessuno costruisce sopra un dato inventato,
e chi legge `settings` distingue «misurato: software» da «mai misurato».

E la voce va nel backlog dei debiti (vedi R4), attribuita alla Fase 1b come
origine e alla Fase 6 come saldo.

## Criterio di chiusura dei rilievi

- [ ] `/media/full` su un ARW restituisce un'immagine **strettamente più
      grande** dell'anteprima, verificato nel browser sull'archivio reale.
- [ ] Il tempo di demosaic su un ARW reale è misurato e nel ledger.
- [ ] Il tetto della cache si fa rispettare senza percorrere l'albero completo.
- [ ] `method` di libwebp misurato a 0/2/4 con tempo e dimensione, default
      scelto e motivato; rapporto derivati ancora **sotto l'1%**.
- [ ] `wired-exceptions.txt` distingue rinvii da debiti.
- [ ] Il comportamento alla scadenza della sessione è verificato e dichiarato.
- [ ] `probe()` non afferma più `"software"` come se l'avesse misurato, e il
      debito è registrato con origine Fase 1b e saldo Fase 6.
