# Fase 5 — WebDAV e upload riprendibili

**Stato:** specifica di progetto, non ancora pianificata in task
**Dipende da:** Fase 1 (cartelle, watcher, coda job). Fase 3 se già fatta, per i
permessi; altrimenti solo il proprietario.
**Chiusa quando:** `rclone bisync` completa un ciclo su una cartella reale e i
file caricati compaiono in timeline entro pochi secondi

Due porte sullo stesso magazzino: entrambe scrivono file veri in cartelle vere,
entrambe passano dallo stesso controllo permessi, entrambe finiscono nella
stessa coda di indicizzazione.

---

## 1. Upload riprendibili — protocollo tus 1.0

### 1.1 Perché uno standard e non un protocollo nostro

**tus** è lo standard usato da Vimeo e Cloudflare Stream, con client maturi per
**JavaScript, Kotlin, Swift e Dart**. Il giorno in cui si farà l'app mobile,
l'upload riprendibile — la parte più fastidiosa da scrivere — è già risolta.

### 1.2 Il flusso

```
① PRE-CHECK       POST /api/v1/upload/check
   client → hash blake3 dei file da caricare (calcolati localmente)
   server → "questi 47 li ho già, caricami gli altri 12"
            ↳ ricaricare 300 foto dal telefono costa quasi zero

② CREAZIONE       POST /api/v1/upload
   Upload-Length: 52428800
   Upload-Metadata: filename, target_folder_id, client_mtime, blake3
   ← 201  Location: /api/v1/upload/8f2a…

③ RIPRESA         HEAD /api/v1/upload/8f2a…
   ← Upload-Offset: 18874368      ← la verità sta SEMPRE sul server

④ INVIO           PATCH /api/v1/upload/8f2a…
   Upload-Offset: 18874368 · Upload-Checksum: blake3 <chunk>
   [ chunk ]  → append su file temporaneo, fsync ogni 16 MB

⑤ FINALIZZAZIONE  verifica hash completo → verifica decodificabilità
                  → rename() atomico nella cartella di destinazione
                  → job di indicizzazione a priorità 1
```

### 1.3 I dettagli che separano «funziona in ufficio» da «funziona in treno»

- **L'offset lo dichiara il server, non il client.** Dopo qualsiasi
  disconnessione il client fa `HEAD` e riparte esattamente da lì. Non esiste
  stato locale di cui fidarsi.
- **Checksum per chunk.** Un chunk corrotto viene rifiutato con `460` e
  rispedito subito, invece di scoprirlo dopo 2 GB.
- **Checksum end-to-end.** A file completo, blake3 del server contro quello del
  client: se non coincidono il file **non entra mai in libreria**.
- **Verifica di decodificabilità.** Oltre all'hash, si prova ad aprire il file:
  header valido, dimensioni leggibili, `ffprobe` per i video. Un file integro ma
  illeggibile va segnalato prima di finire nell'archivio.
- **Chunk adattivi**: 8 MB su rete buona, fino a 1 MB se si rileva latenza alta
  o errori. Su 4G ballerino cambia tutto.
- **3 file in parallelo**, chunk sequenziali per file.
- **Spazio verificato prima** di accettare la sessione: niente upload che
  muoiono a metà per disco pieno.
- **`mtime` originale preservato**, così se una foto non ha EXIF la data resta
  quella vera.
- **Temporanei in `.keeppix-tmp/` dentro la stessa libreria** — stesso
  filesystem, quindi il `rename()` finale è atomico e istantaneo anche per un
  file da 2 GB. Sessioni abbandonate ripulite dopo 7 giorni.

### 1.4 Collisioni di nome

- Stesso nome **e** stesso hash → è un duplicato: si salta e lo si segnala.
- Stesso nome, contenuto diverso → si salva come `IMG_1234_1.ARW` e lo si
  segnala.
- **Mai una sovrascrittura silenziosa.**

### 1.5 Schema

```sql
upload_sessions (
    id              uuid PRIMARY KEY,
    user_id         uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    share_link_id   uuid REFERENCES share_links(id),  -- upload da ospite
    target_folder_id uuid NOT NULL REFERENCES folders(id),
    filename        text NOT NULL,
    expected_size   bigint NOT NULL,
    expected_hash   bytea,
    received_bytes  bigint NOT NULL DEFAULT 0,
    temp_path       text NOT NULL,
    client_mtime    timestamptz,
    expires_at      timestamptz NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);
```

---

## 2. WebDAV

Montato su `/dav/`, espone **solo l'albero delle cartelle** — niente timeline,
niente album virtuali. La radice elenca le librerie accessibili; sotto, l'albero
reale.

La ragione: WebDAV serve a **caricare e sincronizzare cartelle locali**. Una
timeline esposta via WebDAV sarebbe una finzione lenta.

### 2.1 Autenticazione

**Basic auth su HTTPS con app-password dedicate.** I client WebDAV memorizzano
la credenziale in chiaro o quasi, quindi non ci va mai la password di login.

Ogni app-password ha nome, data di ultimo uso e revoca indipendente:
«MacBook Finder», «rclone NAS», «telefono».

### 2.2 Metodi e permessi

| Metodo | Comportamento |
|---|---|
| `PROPFIND` | Elenco **dal database**, non dal filesystem |
| `GET` | File originale con range request |
| `PUT` | Temporaneo → verifica → `rename()` atomico → indicizzazione a priorità 1 |
| `MKCOL` | Crea cartella (permesso editor) |
| `MOVE` | `rename()` su disco; la rilevazione spostamenti **conserva rating, album, descrizioni** |
| `COPY` | Copia reale, con avviso sullo spazio |
| `DELETE` | **Solo owner e admin.** Un editor riceve `403`. **Sempre nel cestino** (30 giorni) |
| `LOCK` / `UNLOCK` | Class 2, obbligatori: senza, Finder e Windows non scrivono |

Il `DELETE` limitato è la **stessa regola della Fase 3, applicata dallo stesso
codice**: WebDAV non è una scorciatoia per aggirare i permessi.

Il `DELETE` sempre nel cestino è deliberato: il protocollo non ha modo di fare
domande, e trascinare una cartella nel cestino del Finder per sbaglio non deve
essere irreversibile.

### 2.3 Le ottimizzazioni che rendono WebDAV usabile

**1. `PROPFIND` dal database.** È la più importante. Un `PROPFIND Depth:1` su
una cartella con 40.000 file, servito con `stat()` uno per uno, richiede 5-15
secondi su Raspberry e Finder va in timeout. Servendolo da una singola query
Postgres — che ha già nome, dimensione, mtime, hash — sono **~40 ms**.

**2. XML in streaming.** La risposta a 40.000 file è ~14 MB di XML. Non si
costruisce in memoria: si genera e si spedisce a flusso costante. RAM occupata:
pochi KB invece di decine di MB per client.

**3. `ETag` = content hash.** È la chiave della sincronizzazione: rclone e
Cyberduck confrontano gli ETag e scaricano **solo ciò che è cambiato davvero**.
Una sync di controllo su 200.000 file diventa una manciata di secondi.

**4. Niente `stat()` a raffica.** Il database è la fonte di verità per i
metadati; il filesystem si tocca solo quando si leggono o scrivono byte.

**5. `quota-available-bytes` esposto**, così Finder mostra lo spazio libero e
non tenta copie destinate a fallire.

**6. `PUT` non passa dal watcher.** Si sa esattamente quando il file è completo,
quindi l'indicizzazione parte subito, senza i 5 secondi di attesa di stabilità.

### 2.4 Lock

```sql
dav_locks (
    token         text PRIMARY KEY,
    resource_path text NOT NULL,
    owner         text,
    depth         text NOT NULL,
    timeout_at    timestamptz NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);
```

Persistiti in Postgres, non in memoria: sopravvivono a un riavvio. Finder e
Windows Explorer **richiedono** WebDAV Class 2 per scrivere.

### 2.5 Compatibilità client — le trappole note

- **macOS Finder**: richiede Class 2 (`LOCK`/`UNLOCK`). Scrive anche `.DS_Store`
  e file `._nome`: si accettano ma si escludono dall'indicizzazione, con
  opzione per scartarli del tutto.
- **Windows Explorer**: il servizio WebClient ha un limite predefinito di 50 MB
  per file e pretende HTTPS con certificato valido. Va documentata la chiave di
  registro, ma **si consiglia rclone o Cyberduck**: il client nativo è lento e
  capriccioso.
- **rclone**: è il client migliore per «sincronizzare cartelle locali».
  `rclone bisync` bidirezionale, `rclone sync` unidirezionale, entrambi da
  verificare contro l'implementazione. Va fornito un file di configurazione già
  pronto nella documentazione.
- **Mountain Duck / Cyberduck**: montaggio come disco, con cache locale. Il caso
  «trascino i RAW dalla scheda e vanno sul server».

### 2.6 Limite dichiarato

WebDAV resta un protocollo verboso: **per caricare 500 RAW da 50 MB, l'upload
tus dalla web app è più veloce e più robusto**. WebDAV dà il meglio come disco
montato e per la sincronizzazione automatica di cartelle.

Va scritto nella documentazione, non lasciato scoprire.

---

## 3. Wizard di configurazione

Nel primo avvio (o da Impostazioni), quando si sceglie «devo ancora caricare le
foto»:

```
Carica le tue foto — WebDAV
──────────────────────────────────────────────────────
Cartella di destinazione   📁 /photos/Da ordinare
App-password generata      MacBook · kpx_7Fq2…9dLm   [copia]
                           ⚠ mostrata una sola volta

▸ macOS Finder      ⌘K → https://keeppix.local/dav/
▸ Windows           consigliato Cyberduck  [scarica config]
▸ rclone            [copia blocco di configurazione]
▸ iPhone / Android  app WebDAV → [mostra QR]

   ⏳ In attesa della prima connessione…
   ✅ Connesso da MacBook · 14 file ricevuti
──────────────────────────────────────────────────────
```

**L'indicatore live in fondo è la parte che fa la differenza**: si sa subito se
la configurazione funziona, invece di scoprirlo mezz'ora dopo.

---

## 4. Frontend

### 4.1 Pannello upload

Destinazione scelta **ogni volta** (decisione presa), con creazione di nuova
cartella inline e opzione «ricorda per questa sessione». Segnalazione dei file
già presenti prima di iniziare.

Il pannello è **persistente e minimizzabile**: si naviga la libreria mentre si
carica. Chiudendo la scheda e riaprendola, gli upload interrotti sono ancora lì
e riprendono dal punto esatto.

```
Carica 47 file · 1,2 GB
────────────────────────────────────
Destinazione
  📁 Foto / 2026 / Grecia / [Santorini ▾]     [nuova cartella]
  ☑ Ricorda per questa sessione
  ⚠ 3 file già presenti in libreria — verranno saltati

[ Annulla ]                        [ Carica ]
────────────────────────────────────
▓▓▓▓▓▓▓▓▓░░░░░  DSC_4412.ARW   62%   4,2 MB/s
✓ DSC_4411.ARW      ✕ DSC_4409.ARW  rete persa  [riprova]
```

### 4.2 PWA con Share Target

Dalla galleria del telefono: «Condividi → Keeppix» e le foto selezionate entrano
nel flusso di upload.

**È la risposta al requisito «selezionarle e caricarle a mano» senza dover
scrivere un'app nativa.** Android supporta Web Share Target; su iOS il supporto
è più limitato e va verificato.

---

## 5. Cosa NON è in Fase 5

Backup automatico da telefono: escluso dagli obiettivi. Sincronizzazione
bidirezionale gestita da Keeppix: no — la fa rclone, che lo fa meglio. SFTP: non
previsto.
