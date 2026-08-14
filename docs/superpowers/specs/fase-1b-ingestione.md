# Fase 1b — Pipeline di ingestione

**Stato:** specifica di progetto, non ancora pianificata in task
**Dipende da:** Fase 1a (modello dati, repository, visibilità)
**Chiusa quando:** si punta Keeppix a una cartella reale e la si trova
indicizzata, con le miniature su disco, senza aver toccato un solo originale

Questo documento contiene le **decisioni**. Il piano dei task, quando verrà
scritto, non deve rimetterle in discussione: deve trasformarle in passi
eseguibili.

---

## 1. Il principio che governa tutta la fase

**La timeline deve diventare navigabile in minuti, non a fine scansione.**

Su 1 TB e ~200.000 file, una pipeline monolitica «leggi tutto, poi mostra»
lascia l'utente davanti a una barra di avanzamento per ore. La pipeline è
divisa in quattro fasi a costo crescente, e **ogni fase rende il sistema più
utile della precedente**:

```
FASE 1              FASE 2            FASE 3         FASE 4
Discovery      →    Metadati     →    Hash      →    Derivati
walk dir            EXIF header       blake3         thumb + preview
~3 min/1TB          ~2 ms/file        I/O bound      il grosso del lavoro

↓ navighi          ↓ timeline        ↓ duplicati    ↓ griglia
  le cartelle        con le date       e move         completa
```

Dopo la **fase 2** — circa 15 minuti su 1 TB — l'utente ha già timeline
ordinata per data di scatto, filtri per fotocamera/ISO/obiettivo, e ricerca
funzionante. Le miniature arrivano dopo, e al loro posto c'è il **thumbhash**:
25 byte di sfocatura colorata, mai un rettangolo grigio.

Ogni fase è un **tipo di job distinto**. Non esiste un job «indicizza questo
file» che fa tutto: sarebbe non riprendibile e non prioritizzabile.

---

## 2. Coda dei job

### 2.1 Perché in Postgres e non in Redis

Deciso nello spec generale (§2, D5) e non si rimette in discussione. La ragione
operativa: un worker fa `BEGIN; SELECT … FOR UPDATE SKIP LOCKED; …; COMMIT`, e
**il job e i dati che produce stanno nella stessa transazione**. Un thumbnail
generato ma non registrato non può esistere. Con Redis potresti creare l'asset
e perdere il job, o il contrario — è la classe di bug che fa comparire foto
senza anteprima senza che si capisca perché.

Il carico reale è ~200.000 eventi *in tutta la vita del sistema*. Postgres con
`SKIP LOCKED` regge 5.000 job/sec: tre ordini di grandezza di margine.

### 2.2 Schema

```sql
CREATE TABLE jobs (
    id          bigserial   PRIMARY KEY,
    kind        text        NOT NULL,
    payload     jsonb       NOT NULL,
    -- 0 interattivo · 1 alto · 2 visibile · 3 background
    priority    smallint    NOT NULL DEFAULT 3,
    status      text        NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending','running','done','failed')),
    attempts    int         NOT NULL DEFAULT 0,
    max_attempts int        NOT NULL DEFAULT 3,
    last_error  text,
    run_after   timestamptz NOT NULL DEFAULT now(),
    locked_by   uuid,
    locked_at   timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    -- Deduplica: due scansioni non devono accodare due volte lo stesso lavoro.
    dedup_key   text
);

-- L'indice su cui gira il claim. Parziale: le righe done/failed non lo sporcano.
CREATE INDEX jobs_claim_idx ON jobs (priority, run_after, id)
    WHERE status = 'pending';

-- Un job pendente per chiave logica.
CREATE UNIQUE INDEX jobs_dedup_key ON jobs (dedup_key)
    WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running');

CREATE INDEX jobs_stale_idx ON jobs (locked_at) WHERE status = 'running';
```

`dedup_key` è ciò che rende una riscansione idempotente: la chiave di un job
di hash è `hash:<asset_id>`, quella di un derivato `derive:<content_hash>`.
Rilanciare la scansione mentre la precedente non è finita non raddoppia il
lavoro.

### 2.3 Claim

```sql
UPDATE jobs SET
    status = 'running',
    locked_by = $worker_id,
    locked_at = now(),
    attempts = attempts + 1
WHERE id = (
    SELECT id FROM jobs
     WHERE status = 'pending'
       AND run_after <= now()
       AND priority <= $max_priority      -- il profilo energetico limita qui
     ORDER BY priority, run_after, id
     FOR UPDATE SKIP LOCKED
     LIMIT 1
)
RETURNING *;
```

`priority <= $max_priority` è il punto in cui i profili energetici agiscono:
non si fermano i worker, si restringe ciò che possono prendere.

### 2.4 Ciclo di vita e fallimenti

- **Successo**: `status = 'done'`. Le righe `done` vengono cancellate da un job
  di manutenzione notturna dopo 7 giorni — servono a diagnosticare, non per
  sempre.
- **Fallimento ritentabile**: `status = 'pending'`, `run_after = now() +
  backoff`, con backoff esponenziale `min(2^attempts, 300)` secondi più jitter.
  Il jitter non è ornamentale: senza, 4.000 job che falliscono per un disco
  smontato ritentano tutti nello stesso istante.
- **Esaurito** (`attempts >= max_attempts`): `status = 'failed'`, `last_error`
  popolato. L'asset corrispondente va in `status = 'error'` con `error_detail`,
  e compare nella pagina **Problemi** (Fase 1c).
- **Worker morto**: un job `running` con `locked_at` più vecchio di 10 minuti
  torna `pending`. Lo fa un job di manutenzione, non un trigger.

**Ogni job deve essere idempotente.** Se il processo muore a metà di un
derivato, riprendere non deve produrre un file corrotto: si scrive su un
temporaneo e si fa `rename()` atomico.

### 2.5 Livelli di priorità

| Liv. | Nome | Cosa | Chi lo accoda |
|---|---|---|---|
| 0 | interattivo | preview di una foto in apertura, transcodifica di un video avviato | una richiesta HTTP che sta aspettando |
| 1 | alto | file appena caricati (web, mobile, WebDAV) | upload, watcher |
| 2 | visibile | miniature dei bucket temporali nel viewport | il frontend, via hint |
| 3 | background | il resto della scansione | scanner |

Il livello 2 è quello che si nota: il frontend comunica quali bucket sta
guardando e il backend **riordina la coda** promuovendo quei job. Scorri al
2019 e le foto del 2019 si materializzano davanti a te, invece di aspettare
che finisca il 2024. Il meccanismo di promozione è un `UPDATE jobs SET
priority = 2 WHERE dedup_key = ANY($keys) AND status = 'pending'`.

---

## 3. Worker pool e profili energetici

### 3.1 Dimensionamento

```
worker_count = clamp(cpu_count - 1, 1, 8)
```

Meno uno perché il thread HTTP deve restare reattivo. Il tetto a 8 perché oltre
il collo di bottiglia è l'I/O, non la CPU.

**Limite di memoria, non solo di concorrenza.** Quattro worker su una foto da
61 megapixel mandano in OOM un Raspberry da 4 GB. Ogni job dichiara una stima
di RAM (`width * height * 3` byte per un'immagine, più il buffer di
decodifica), e un semaforo pesato la fa rispettare. Un job che stima più della
memoria disponibile aspetta invece di far cadere il processo.

### 3.2 I quattro profili

| Profilo | Attivazione | Core usati | Priorità ammesse |
|---|---|---|---|
| `interactive` | interfaccia in uso | 50% | 0-2 |
| `background` | inattività da 5 min | 100% | 0-3 |
| `night` | finestra oraria (default 02:00-06:00) | 100% | 0-3 + manutenzione |
| `paused` | manuale | 0 | solo 0 |

«Interfaccia in uso» significa: una richiesta autenticata negli ultimi 5
minuti, esclusi `/health` e le richieste di asset statici. Il passaggio a
`interactive` deve avvenire **entro pochi secondi** da una richiesta, altrimenti
l'utente apre l'app e la trova lenta.

Nella finestra notturna gira ciò che di giorno non ha senso far girare:

- backlog delle preview e delle transcodifiche;
- **scrubbing d'integrità**: ri-hash a rotazione del 5% della libreria, per
  intercettare bit rot prima che sia troppo tardi. Su un archivio di ricordi è
  la cosa più utile che un server possa fare di notte;
- riscansione completa delle librerie su filesystem di rete;
- pulizia dei job `done`, dei temporanei abbandonati, del cestino oltre i 30
  giorni, delle transcodifiche non usate da 90 giorni;
- `VACUUM ANALYZE` e dump del database.

Tutto interrompibile: se qualcuno apre l'interfaccia alle 3 di notte, entro
pochi secondi si torna a `interactive`.

---

## 4. Rilevamento delle capacità hardware

Al primo avvio, e su richiesta da Impostazioni, Keeppix **misura invece di
indovinare**.

Il motivo è che `target_arch` non dice nulla di utile: sotto `aarch64` ci sono
un Raspberry Pi 5 senza encoder H.264, un RK3588 che transcodifica 8K, un
Jetson con NVENC e un Ampere Altra senza alcun media engine. Un binario ARM64
gira su tutti.

**Procedura**: si genera un clip di test di 2 secondi e si prova a codificarlo
con ogni backend candidato, in ordine di preferenza per il SoC rilevato da
`/proc/device-tree/compatible`, `/proc/cpuinfo`, `/dev/dri/*`, `nvidia-smi`.

Candidati, in ordine: `rkmpp`, `nvenc`, `v4l2m2m`, `videotoolbox`, `vaapi`,
`qsv`, `amf`, software.

Il risultato va in `system_capabilities` (tabella già esistente dalla Fase 0),
è mostrato in Impostazioni **con gli fps misurati**, ed è sovrascrivibile a
mano. Costo: ~4 secondi, una volta.

Un driver presente ma rotto — capita spessissimo con V4L2 e VAAPI a metà — deve
fallire **durante il probe**, non sul video di Natale alle 23:00.

---

## 5. `keeppix-media` — elaborazione dei file

Crate senza database, senza rete, senza stato. Funzioni pure `path → dati`.
Questo confine è imposto da `deny.toml` e non è negoziabile: rende la pipeline
testabile con file di esempio e nessun Postgres acceso.

### 5.1 Rilevamento del tipo

**Per contenuto, non per estensione.** Si leggono i primi byte (magic number).
L'estensione è un suggerimento che un utente può sbagliare, e un file
rinominato non deve rompere la pipeline.

| Kind | Formati |
|---|---|
| `image` | JPEG, PNG, WebP, HEIC/HEIF, AVIF, GIF, TIFF |
| `raw_image` | ARW, ARQ (Sony) · NEF, NRW (Nikon) · CR2, CR3 (Canon) · DNG · ORF (Olympus) · RAF (Fuji) |
| `video` | MP4, MOV, MKV, AVI, WebM, M4V, 3GP |
| `unknown` | tutto il resto — non è un errore, semplicemente non si indicizza |

### 5.2 Fase 1 — Discovery

`walkdir` **senza aprire i file**. Per ogni entry si raccoglie `(path, size,
mtime, inode)` e si inserisce a batch da 1.000 righe con `COPY`.

Esclusioni predefinite, non configurabili:
`@eaDir` (Synology), `.DS_Store`, `Thumbs.db`, `#recycle`, `#snapshot`,
`.keeppix-trash/`, `.keeppix-tmp/`, file che iniziano per `.`, file che iniziano
per `._` (AppleDouble).

Più i pattern per libreria (`exclude_patterns`, glob).

I file `.xmp` **non diventano asset**: si associano al file omonimo.

**File in corso di scrittura.** Un `.ARW` da 50 MB copiato via rete appare
subito ma è incompleto. Prima di processarlo si verifica la **stabilità di
`mtime` e `size` su due letture a 5 secondi di distanza**. Se cambia, si
rimanda. Senza questo controllo si indicizza mezzo file e se ne calcola l'hash
sbagliato.

Costo: ~3 minuti su 1 TB.

### 5.3 Fase 2 — Metadati rapidi

Si aprono solo i **primi 128 KB**: bastano per l'header EXIF di JPEG, HEIC e
quasi tutti i RAW. Se ne ricava data di scatto, dimensioni, orientamento,
fotocamera, obiettivo, ISO, tempi, GPS.

Costo: ~2 ms per file. È qui che la timeline prende forma.

**Fuso orario.** Le reflex non registrano il fuso: un `.ARW` scattato a Tokyo
alle 14:00 ha scritto «14:00» e nient'altro, e su un server italiano finirebbe
in timeline alle 06:00. Con il GPS si ricava il fuso dai confini geografici
(dataset semplificato ~8 MB in PostGIS, introdotto in Fase 4) e si normalizza
`taken_at_utc`, conservando `tz_offset_minutes` per la visualizzazione. Senza
GPS si assume il fuso del server e si segnala.

**Ordine di ripiego per la data**: EXIF `DateTimeOriginal` → EXIF
`CreateDate` → `mtime` del file. Mai la data di creazione del filesystem, che
su una copia è la data della copia.

### 5.4 Fase 3 — Hash

`blake3` in streaming, multi-thread. Limitato dal disco, non dalla CPU (~2,5
GB/s multi-thread). Su NVMe ~40 minuti per 1 TB, su USB 3.0 ~2 ore.

**Ricontrollo incrementale**: alle scansioni successive, se `(size, mtime,
inode)` combaciano, l'hash **non si ricalcola**. La riscansione del TB scende
a ~2 minuti.

Da qui nascono tre cose: raggruppamento duplicati, rilevamento spostamenti, e
il nome del file dei derivati.

### 5.5 Fase 4 — Derivati

Percorso: `data/derivatives/ab/cd/<hash>.webp`, sharded sui primi due byte
dell'hash. Cartelle da 200.000 file sono lente su ext4 e disastrose su exFAT.

| Derivato | Dimensione | Uso |
|---|---|---|
| thumbhash | 25 byte, in DB | placeholder immediato |
| thumbnail | 240 px lato lungo, WebP | griglia |
| preview | 1440 px lato lungo, WebP q78 + `sharp_yuv` | visualizzatore, mobile |
| originale | — | download e zoom 1:1, servito dal file di partenza |

#### Le ottimizzazioni, in ordine di applicazione

Sono la differenza fra 2 ore e 20 ore su hardware piccolo. Vanno implementate
tutte.

1. **Thumbnail EXIF incorporato.** Molte foto ne contengono già uno. Se è
   ≥240 px si usa direttamente: **~1 ms invece di ~200 ms**.
2. **Decodifica DCT a scala ridotta.** libjpeg-turbo sa decodificare
   direttamente a 1/2, 1/4, 1/8 lavorando sui coefficienti DCT. Per una preview
   1440 da un JPEG 6000 px si decodifica a 1/4. **~8 volte meno lavoro.**
3. **Una sola decodifica per due derivati.** Il buffer decodificato produce
   preview, thumbnail e thumbhash. Mai decodificare due volte lo stesso file.
   **−45% di tempo.**
4. **Salto della preview quando è inutile.** Se l'originale è già ≤1600 px e
   ≤400 KB (screenshot, immagini WhatsApp), si serve l'originale. Sul TB tipico
   è il **20-30% dei file**. −25% su tempo e spazio.
5. **`sharp_yuv`** nella conversione RGB→YUV: qualità percepita di q85 al costo
   di q78. **−8% di spazio gratis.**
6. **Qualità adattiva** guidata da SSIM su campione: le immagini piatte (cielo,
   ritratti su fondo uniforme) scendono a q68 senza differenza visibile.
   **−12% medio.**
7. **`fast_image_resize`** con Lanczos3, che fa dispatch runtime su NEON e
   AVX2. Non serve `cfg` di compilazione.
8. **Decoder**: `zune-jpeg` o libjpeg-turbo, **non** il decoder JPEG puro-Rust
   del crate `image`, che è ~3x più lento e senza DCT scaling.
9. **Buffer RGB8, non RGBA**, quando non c'è canale alfa.

Risultato atteso su 200.000 asset, RPi 5 + NVMe: **~20 GB, ~2h10m**.

#### Formato configurabile

In `Impostazioni → Media`: formato preview (WebP · AVIF · JPEG), qualità
(adattiva · fissa), risoluzione (1080 · 1440 · 2048 · originale), formato
miniature.

Cambiare formato **rigenera** i derivati con un job di background
interrompibile: i vecchi restano validi finché i nuovi non sono pronti. Non si
resta mai senza miniature a metà conversione.

Default: WebP. AVIF è −30% di spazio ma 8-15x più lento a codificare — su ARM
è escluso di default, su x86 potente è una scelta ragionevole.

### 5.6 Video

Non si transcodifica quasi mai. La regola:

```
ffprobe → codec, contenitore, durata, risoluzione, rotazione, HDR
  ├─ H.264 + AAC in MP4/MOV, ≤1080p  → DIRECT PLAY, nessuna transcodifica
  ├─ H.264 4K                         → direct play su Wi-Fi, 720p su rete mobile
  ├─ HEVC / AV1 / VP9 / MKV           → transcodifica ON DEMAND al primo play,
  │                                     720p H.264, in cache su disco
  └─ HDR (HLG/PQ)                     → tone mapping in transcodifica
```

Il 90% dei video da telefono e reflex è H.264+AAC: il browser lo riproduce
nativamente e si serve il file con range request. **CPU: zero**, anche su
Raspberry, anche in 4K se la rete regge.

Derivati generati sempre: **poster frame** al 10% della durata (non a 0, che
spesso è nero) e **anteprima animata** di 3 secondi in WebP per l'hover.

Se il probe dice che servirebbe transcodifica ma nessuno ha aperto il video,
**non si fa nulla**. Su un TB con qualche centinaio di video HEVC da iPhone,
questa sola scelta risparmia giorni di CPU.

Consegna: direct play come file progressivo con range request; transcodificati
in **HLS**, che permette di iniziare la riproduzione prima del completamento e
di saltare a un punto qualsiasi.

### 5.7 Sandbox dei decoder C

`libraw` e `ffmpeg` sono C e aprono file non fidati: è storicamente il modo in
cui le gallerie vengono bucate.

Ogni decodifica che coinvolge codice C gira in un **processo separato
usa-e-getta**, con `rlimit` su memoria e CPU, filtro **seccomp** che nega rete
e scritture arbitrarie, nessun privilegio. Un crash o un exploit su un `.CR3`
malevolo muore lì dentro.

Costo: pool di processi pre-avviati, ~1-2 ms per file. Trascurabile rispetto ai
40 ms di una preview.

In più: rifiuto di immagini oltre **200 megapixel** (bomba di decompressione),
timeout duri su ffmpeg.

---

## 6. Watcher del filesystem

`notify` con debounce a 2 secondi e coalescing degli eventi.

Due avvertenze che vanno affrontate nel design, non scoperte dopo:

**Limite inotify.** Il default Linux è `max_user_watches=8192`: con migliaia di
cartelle si satura. All'avvio si legge il limite, si stima il fabbisogno, e se
non basta si mostra in interfaccia il comando
(`sysctl fs.inotify.max_user_watches=524288`) **e** si ripiega automaticamente
su scansione periodica. Non si fallisce in silenzio.

**Filesystem di rete.** Su NFS, SMB e mount rclone inotify non funziona
affatto. Si rileva il tipo di mount e si passa a polling programmabile (default
ogni 15 minuti).

I file che arrivano via `PUT` WebDAV (Fase 5) **non passano dal watcher**: si
sa esattamente quando sono completi, quindi l'indicizzazione parte subito senza
i 5 secondi di attesa di stabilità.

---

## 7. Rilevamento spostamenti e duplicati

**Spostamento**: quando il watcher osserva una cancellazione seguita a breve da
una creazione con identico `(content_hash, size)`, la riconosce come *move* e
trasferisce metadati, rating e appartenenza agli album.

È una **regola esplicita, testabile e registrata a log** («rilevato
spostamento: 412 file»), non un effetto collaterale dell'identità. È il motivo
per cui l'identità dell'asset è il percorso e non il contenuto: la
cancellazione resta non ambigua, e lo spostamento è gestito da codice che si
può leggere.

**Duplicati**: raggruppamento per `content_hash` con `count > 1`. I derivati
sono già indicizzati per hash, quindi cinque copie della stessa foto occupano
un solo thumbnail. La pagina Duplicati (Fase 1c) mostra lo spazio recuperabile
e permette «tieni questo, elimina gli altri».

---

## 8. Gestione dei fallimenti — le tre protezioni

Queste tre esistono perché senza di esse il sistema può distruggere dati.

**1. Il disco sparisce.** Se il `root_path` di una libreria non è montato o è
vuoto mentre il database dice che conteneva 180.000 file: la libreria va in
stato `offline`, i job si fermano, **non viene cancellato nulla**. Banner in
interfaccia: «Libreria Foto non raggiungibile».

**2. Sparizione di massa.** Se una scansione rileva la scomparsa di oltre il
**20%** dei file, si ferma e chiede conferma esplicita elencando cosa
sparirebbe. Protegge da mount parziali, permessi cambiati, dischi mezzi
montati.

**3. File corrotto o illeggibile.** `status = 'error'` con il motivo, 3
tentativi con backoff, poi la pagina **Problemi** con azioni: riprova, ignora,
mostra il percorso. **Non blocca mai la coda.**

---

## 9. Cosa NON è in Fase 1b

Endpoint HTTP, WebSocket, `TimelineRepo`, frontend, ricerca: sono Fase 1c.
Estrazione preview RAW e sidecar XMP: Fase 2 — in 1b i RAW vengono
riconosciuti, gli EXIF letti, ma la preview incorporata si estrae in Fase 2.

La verifica di 1b è un test di integrazione che punta la pipeline a una
directory di esempio e controlla il risultato nel database e su disco.

---

## 10. Stime da verificare durante l'esecuzione

Sono **mie stime, non misure**. La Fase 1b produce i numeri veri, e quei numeri
ricalibrano le fasi 2, 4 e 6.

| Fase | Tempo atteso (1 TB, ~200k file, RPi 5 + NVMe) | Utilizzabile |
|---|---|---|
| Discovery | ~3 min | navighi le cartelle |
| Metadati | ~12 min | **timeline, ricerca, mappa** |
| Hash | ~45 min | duplicati, spostamenti |
| Thumbnail | ~2 h | griglia completa |
| Preview | in background | ~20 GB |
| **Riscansione** | **~2 min** | — |

Su USB 3.0 raddoppiano; su VPS x86 con SSD si dimezzano.

Da misurare e riportare nel ledger: throughput reale di discovery, ms/file
della fase metadati, MB/s dell'hash, ms/file dei derivati, e **la percentuale
di file che ricadono nell'ottimizzazione 1 e nell'ottimizzazione 4** — sono le
due che decidono il tempo totale.
