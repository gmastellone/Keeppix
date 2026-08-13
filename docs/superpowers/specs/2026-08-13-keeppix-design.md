# Keeppix — Documento di progettazione

**Data:** 2026-08-13
**Stato:** approvato, pronto per la pianificazione implementativa
**Base di partenza:** [urocissa](https://github.com/hsa00000/urocissa) v3.4.1

---

## 1. Obiettivo

Keeppix è una galleria fotografica self-hosted che serve due utenti nella stessa persona:

- **la famiglia**, che vuole rivedere e condividere i propri ricordi senza pensare a dove sono i file;
- **il professionista**, che carica RAW da remoto, li tiene al sicuro, li seleziona e li mostra ai clienti.

Prende da urocissa lo scorrimento virtuale su grandi collezioni, il parser di ricerca e la pipeline di indicizzazione; aggiunge le quattro cose che urocissa dichiara di non avere — multiutente, mappe, librerie esterne e cartelle esistenti — più condivisione granulare, link pubblici, gestione RAW e WebDAV.

### Vincoli

| Vincolo | Valore |
|---|---|
| Volume iniziale | ~1 TB, ~200.000 asset, di cui ~256 GB di RAW effimeri |
| Hardware minimo | Raspberry Pi 5 8 GB + NVMe |
| Hardware supportato | ARM64 e x86-64, da SBC a VPS |
| Deployment | Docker, servizi esterni opzionali |
| Client futuri | app mobile nativa sulla stessa API |
| Lingue | italiano e inglese dal rilascio |

### Non obiettivi (v1)

Riconoscimento volti, ricerca semantica CLIP, backup automatico da telefono, editing distruttivo delle immagini, watermark, quote per utente, federazione fra istanze.

---

## 2. Decisioni architetturali

Ogni decisione riporta la ragione e l'alternativa scartata, perché il *perché* invecchia meglio del *cosa*.

| # | Decisione | Alternativa scartata | Ragione |
|---|---|---|---|
| D1 | **Hard fork** di urocissa: si riusano concetti e componenti, non la struttura | fork rebasabile | il multiutente attraversa ogni router; il rebase sarebbe comunque impraticabile |
| D2 | **Postgres 17 + PostGIS** unica fonte di verità; niente redb | ibrida redb+Postgres | a 200k asset Postgres risponde in 5-15 ms; due fonti di verità su 8 GB di RAM sono un costo netto |
| D3 | **Axum** al posto di Rocket | restare su Rocket | WebDAV come servizio tower, multipart in streaming, OpenAPI da `utoipa`, manutenzione attiva |
| D4 | **Vue 3 + TypeScript + Tailwind v4 + Reka UI** | Vuetify 4 (usato da urocissa) | bundle iniziale 150 KB gzip contro ~280 KB di sola baseline; controllo totale sull'aspetto mobile-first |
| D5 | **Coda job in Postgres** (`FOR UPDATE SKIP LOCKED`) | Redis | transazionalità con i dati prodotti; un servizio in meno; il carico è tre ordini di grandezza sotto il limite |
| D6 | **Cache in-process** (`moka`), niente Redis | Redis come cache | su nodo singolo la cache in-process è più veloce; Redis serve solo con più istanze |
| D7 | **REST + OpenAPI 3.1** | GraphQL | generazione automatica del client mobile, cache HTTP sui derivati, costo delle query controllato |
| D8 | **WebSocket** per gli eventi | SSE | prestazioni equivalenti; il canale bidirezionale serve alla roadmap (culling collaborativo, client mobile) |
| D9 | **Identità dell'asset = percorso** (`folder_id` + `filename`) | identità = hash del contenuto | cancellazione non ambigua; la cartella resta la verità; la deduplica si sposta sulla presentazione |
| D10 | **Metadati modificati in tabella override**, originali immutabili | riscrittura dei file | i RAW non si riscrivono; batch su 5.000 file istantaneo e reversibile |
| D11 | **Immagine distroless** con ffmpeg statico compilato da noi | debian-slim | nessuna shell, 6 pacchetti invece di 90; costo: ownership delle build ffmpeg |
| D12 | **PMTiles locali** per le mappe | tile server, OpenFreeMap remoto | nessuna richiesta esterna che riveli dove sono state scattate le foto; un file per regione |
| D13 | **Permessi solo-allow** con ereditarietà | allow/deny come NTFS | la UI può sempre spiegare *perché* hai accesso; il deny è la prima causa di permessi incomprensibili |

---

## 3. Architettura di sistema

### 3.1 Forma del deployment

Un solo processo Rust (~80 MB RSS a riposo) con il frontend incorporato via `rust-embed`, più Postgres.

```
┌─────────────────────────────────────────────┐
│  keeppix                                    │
│   Axum ─┬─ /api/v1/*   REST                 │
│         ├─ /api/ws     WebSocket            │
│         ├─ /dav/*      WebDAV               │
│         ├─ /media/*    file serving         │
│         └─ /*          SPA                  │
│   Worker pool (tokio + rayon)               │
│   Watcher filesystem (notify)               │
│   Processi sandbox usa-e-getta (ffmpeg/RAW) │
└───────┬───────────────────────┬─────────────┘
        │                       │
  ┌─────▼──────┐        ┌───────▼─────────┐
  │  Postgres  │        │  Filesystem     │
  │  + PostGIS │        │  originali (ro) │
  └────────────┘        │  derivati (rw)  │
                        └─────────────────┘
```

Gli originali sono montati in sola lettura salvo esplicita necessità di scrittura (upload, WebDAV, sidecar). Keeppix rileva il montaggio read-only e disabilita le azioni corrispondenti spiegando il motivo, invece di far fallire i job.

### 3.2 Workspace

| Crate | Responsabilità | Dipende da |
|---|---|---|
| `keeppix-domain` | entità e tipi puri, nessun I/O | — |
| `keeppix-db` | migrazioni, repository sqlx, query timeline — **unico posto con SQL** | domain |
| `keeppix-media` | decodifica, EXIF/XMP, thumbnail, RAW, video — **nessuna conoscenza del DB** | domain |
| `keeppix-jobs` | coda, worker, definizione job, retry | db, media |
| `keeppix-api` | router Axum, extractor auth, DTO, OpenAPI | db, jobs |
| `keeppix-dav` | adapter WebDAV con permessi applicati | db |
| `keeppix-server` | binario: config, migrazioni, wiring, watcher, embed frontend | tutti |

Invariante: `keeppix-media` non conosce il database e `keeppix-db` non conosce le immagini. La pipeline che li unisce vive in `keeppix-jobs`. Ne consegue che la lettura di un CR3 è testabile senza Postgres e le query della timeline sono testabili senza immagini.

### 3.3 Rilevamento capacità hardware

Al primo avvio (e su richiesta da Impostazioni) Keeppix **misura** invece di indovinare: genera un clip di test e prova ogni backend candidato, ordinato secondo il SoC rilevato da `/proc/device-tree/compatible`, `/proc/cpuinfo`, `/dev/dri/*`, `nvidia-smi`.

Candidati: `rkmpp`, `nvenc`, `v4l2m2m`, `videotoolbox`, `vaapi`, `qsv`, `amf`, software.

Il risultato è persistito in `system_capabilities`, mostrato in Impostazioni con i fps misurati, e sovrascrivibile a mano. Nessun `#[cfg(target_arch)]` nella logica di business: un binario ARM64 gira sia su un Pi 5 privo di encoder sia su un RK3588 che transcodifica 8K, e `target_arch` non distingue i due casi.

Costo: ~4 secondi, una volta sola.

### 3.4 Profili energetici

| Profilo | Attivazione | Core | Job ammessi |
|---|---|---|---|
| `interactive` | interfaccia in uso | 50% | priorità 0-2 |
| `background` | inattività da 5 min | 100% | + scansione, derivati |
| `night` | finestra oraria (default 02:00-06:00) | 100% | + manutenzione pesante |
| `paused` | manuale | 0 | solo priorità 0 |

Nella finestra notturna: backlog preview e transcodifiche, **scrubbing d'integrità** (ri-hash a rotazione del 5% della libreria per intercettare bit rot), sincronizzazione sidecar, riscansione dei filesystem di rete, riscoperta duplicati, backfill geocoding, `VACUUM ANALYZE`, backup, pulizia cestino oltre i 30 giorni e cache transcodifiche non usate da 90 giorni.

Ogni profilo è interrompibile entro pochi secondi.

### 3.5 Autenticazione

- **Browser**: cookie `__Host-`, `HttpOnly`, `Secure`, `SameSite=Lax`, rotazione a ogni refresh, rilevamento del riuso del refresh token con revoca dell'intera famiglia di sessioni.
- **Mobile e WebDAV**: token opachi da 32 byte salvati come hash, con scope (`mobile`, `webdav`), ultimo utilizzo tracciato, revoca individuale.
- **2FA TOTP** (RFC 6238) opzionale: compatibile con Google Authenticator, Aegis, FreeOTP, 1Password, Bitwarden. Segreto cifrato a riposo con chiave derivata dal segreto del server, tolleranza ±1 intervallo, protezione contro il riuso del codice, 10 codici di recupero salvati come hash. Nessun servizio esterno, funziona offline.
- **WebAuthn/passkey** in roadmap post-v1.

---

## 4. Modello dati

### 4.1 Identità e file

```sql
libraries(id, name, owner_id, root_path, scan_enabled, exclude_patterns[], status)

folders(id, library_id, parent_id, name,
        path ltree,              -- label = ID numerici, non nomi
        UNIQUE(parent_id, name))

assets(id uuid, folder_id, filename,           -- UNIQUE(folder_id, filename)
       content_hash bytea,                     -- blake3, indicizzato non-unique
       size_bytes, mtime, inode,
       kind, taken_at, taken_at_utc, tz_offset,
       width, height, duration_ms,
       camera_make, camera_model, lens, iso, f_number, exposure, focal_length,
       location geography(Point,4326), place_id, location_source,
       stack_id, status, created_at, updated_at)
```

`status`: `active` · `offline` (file sparito) · `error` · `trashed`.
`location_source`: `exif` · `user` · `map_pin` · `copied` · `gpx`.

L'identità è il percorso, non il contenuto: cancellare una foto cancella *quel* file in *quella* cartella, senza toccare le altre copie. `content_hash` resta indicizzato e serve a tre cose: indicizzazione dei derivati (generati una volta sola anche per 5 copie), pagina Duplicati, rilevamento spostamenti.

**Rilevamento spostamenti**: quando il watcher osserva una cancellazione seguita a breve da una creazione con identico `(content_hash, size)`, la riconosce come spostamento e trasferisce metadati, rating e appartenenza agli album. È una regola esplicita, testabile e registrata a log, non un effetto collaterale dell'identità.

### 4.2 Albero e permessi

`ltree` con indice GiST rende "tutto ciò che sta sotto questa cartella" una singola condizione `path <@ '1.7'`. Le label sono ID numerici perché i nomi contengono spazi e accenti, che ltree non ammette.

```sql
users(id, username, email, display_name, password_hash, totp_secret_enc,
      is_admin, locale, created_at, disabled_at)
groups(id, name, created_by)
group_members(group_id, user_id, added_at)

permissions(id, subject_type, subject_id,       -- 'user' | 'group'
            object_type, object_id,             -- 'folder' | 'album' | 'asset'
            role,                               -- 'viewer' | 'editor'
            inherit boolean default true)
```

| Azione | viewer | editor | owner | admin |
|---|:-:|:-:|:-:|:-:|
| Vedere, scaricare preview | ✅ | ✅ | ✅ | ✅ |
| Scaricare l'originale | opz. | ✅ | ✅ | ✅ |
| Rating e pick (propri) | ✅ | ✅ | ✅ | ✅ |
| Modificare metadati | ❌ | ✅ | ✅ | ✅ |
| Aggiungere/togliere da album | ❌ | ✅ | ✅ | ✅ |
| Caricare nella cartella | ❌ | ✅ | ✅ | ✅ |
| Cestinare in Keeppix | ❌ | ✅ | ✅ | ✅ |
| **Cancellare dal disco** | ❌ | ❌ | ✅ | ✅ |
| Ri-condividere ad altri | ❌ | ❌ | ✅ | ✅ |

Un editor non può distruggere file: può solo cestinarli in Keeppix, da dove sono recuperabili.

L'admin ha accesso completo in lettura e scrittura. Ogni accesso dell'admin a contenuti non suoi finisce nell'audit log.

**Risoluzione**: vince il permesso più alto fra quelli applicabili (diretti, di gruppo, ereditati). Nessun deny esplicito. L'ereditarietà si interrompe con `inherit = false` su un nodo, ed è visibile in interfaccia. La UI può sempre rispondere a "perché ho accesso a questa foto?" con la catena completa.

**Query di visibilità** — l'unica funzione che costruisce il filtro, in `keeppix-db`:

```sql
WITH allowed AS (
  SELECT f.path FROM folders f
    JOIN libraries l ON l.id = f.library_id
   WHERE l.owner_id = $me
  UNION
  SELECT f.path FROM permissions p JOIN folders f ON f.id = p.object_id
   WHERE p.object_type = 'folder'
     AND (p.subject_id = $me OR p.subject_id = ANY($my_groups))
)
SELECT a.* FROM assets a JOIN folders f ON f.id = a.folder_id
 WHERE f.path <@ ANY(SELECT path FROM allowed)
   AND a.status = 'active'
 ORDER BY a.taken_at_utc DESC, a.id DESC
 LIMIT 200;
```

I prefissi autorizzati sono tipicamente 1-10. Nessuna tabella di visibilità materializzata: cambiare un permesso è un `INSERT` con effetto immediato.

**Ogni repository che legge asset richiede un `AuthContext`**: non esiste un metodo che non lo prenda, e gli handler HTTP non scrivono SQL. Ne consegue che REST, WebDAV, WebSocket e link pubblici condividono lo stesso identico controllo — un link pubblico è un `AuthContext::ShareLink { scope, allow_download, … }`, non una strada parallela con regole proprie.

### 4.3 Timeline

```sql
folder_month_counts(folder_id, month date, asset_count int)  -- PK(folder_id, month)
```

Aggiornata da trigger. Conteggi per cartella e mese, indipendenti dall'utente, sommati al volo sul sottoalbero autorizzato: ~30.000 righe di una tabella minuscola, **3-8 ms**. Il client riceve `[{month, count}]`, calcola l'altezza esatta della scrollbar e richiede i bucket solo quando entrano nel viewport.

### 4.4 Metadati

```sql
asset_exif(asset_id PK, raw jsonb, parsed_at)          -- letto dal file, immutabile
asset_overrides(asset_id PK, title, description, taken_at, location, place_id,
                orientation, updated_by, updated_at, xmp_written_at)
```

Valore mostrato = `COALESCE(override, exif)`. Ne conseguono: editing batch su 5.000 file in millisecondi, "ripristina originale" sempre disponibile, scrittura sidecar come job asincrono ritentabile.

**Propagazione sui file** — non "mai scrivere", ma "DB prima, file poi":

| Tipo | Default | Opzioni |
|---|---|---|
| RAW (ARW, NEF, CR2, CR3, DNG, ORF, RAF) | sidecar `.xmp` | nessuna scrittura nel RAW, mai |
| JPEG, HEIC, PNG | sidecar `.xmp` | ☑ scrittura EXIF nel file, per libreria o su richiesta |
| Video | sidecar `.xmp` | tag QuickTime opzionali |

La scrittura nel file avviene su temporaneo nella stessa cartella → `fsync` → rilettura e verifica → `rename()` atomico. Azione **"Sincronizza metadati sui file"** disponibile su richiesta con barra di avanzamento e report.

Ragioni della scelta: i RAW sono contenitori proprietari poco documentati e una scrittura fallita a metà produce un file irrecuperabile; un batch su 5.000 file come riscrittura costa 40-90 minuti su disco USB contro ~200 ms in DB; la sovrascrittura distrugge il valore originale rendendo irreversibile un errore di fuso orario; riscrivere i file cambia `mtime` e fa ricaricare 1 TB al backup successivo.

### 4.5 Resto dello schema

```sql
albums(id, name, description, owner_id, cover_asset_id, created_at)
album_assets(album_id, asset_id, position, added_by, added_at)  -- PK(album_id, asset_id)

stacks(id, primary_asset_id)          -- RAW+JPEG; assets.stack_id → stacks.id

asset_flags(asset_id, user_id, rating smallint, pick, color_label)  -- PK(asset_id,user_id)

share_links(id, token_hash, object_type, object_id, created_by,
            password_hash, expires_at, max_views, view_count,
            allow_download, allow_original, allow_upload, allow_cdn_cache,
            hide_metadata, revoked_at, last_accessed_at)

places(id, name, ascii_name, country_code, admin1, admin2,
       location geography(Point,4326), population)

jobs(id, kind, payload jsonb, priority smallint, status,
     attempts, last_error, run_after, locked_by, locked_at)

upload_sessions(id, user_id, target_folder_id, filename, expected_size,
                expected_hash, received_bytes, temp_path, expires_at)

trash_entries(asset_id, deleted_by, deleted_at, original_path, disk_action)

dav_locks(token, resource_path, owner, depth, timeout_at)

change_log(seq bigserial, entity, entity_id, op, at)   -- cursore per /sync/delta

audit_log(id, actor_id, action, object_type, object_id, detail jsonb, at)

system_capabilities(key, value jsonb, measured_at)
```

Rating e pick sono **per utente**: nel culling professionale la selezione di ciascuno è la propria. Regole di riconciliazione: nell'XMP finisce il rating del **proprietario della libreria**; un `xmp:Rating` già presente in un sidecar importato viene assegnato al proprietario; l'interfaccia espone un selettore "Selezione di: [utente]"; gli album condivisi possono attivare la **modalità collaborativa** che unisce i pick mostrando l'autore di ciascuno.

**Predisposto e vuoto in v1**: `faces`, `people`, `asset_embeddings(vector)`. Le tabelle esistono dalle migrazioni iniziali così l'attivazione futura non richiede una migrazione dolorosa su 200.000 righe.

**Deliberatamente assente**: nessuna tabella di visibilità per utente; nessuna copia degli originali; nessun path assoluto denormalizzato sugli asset (un `mv` di una cartella con 40.000 foto sarebbe un UPDATE di 40.000 righe).

---

## 5. Pipeline di ingestione

**Principio**: la timeline deve essere navigabile in pochi minuti, non a fine scansione. Le fasi hanno costo crescente e ognuna rende il sistema più utile della precedente.

### 5.1 Fase 1 — Discovery (~3 min su 1 TB)

`walkdir` senza aprire i file; inserimento a batch da 1.000 righe con `COPY`.

Esclusioni predefinite: file nascosti, `@eaDir`, `.DS_Store`, `Thumbs.db`, `#recycle`, `.keeppix-trash/`, `.keeppix-tmp/`, più i pattern per libreria. I file `.xmp` non diventano asset: sono associati al file omonimo.

**File in scrittura**: prima di processare, verifica di stabilità di `mtime` e `size` su due letture a 5 secondi di distanza. Evita di indicizzare file incompleti.

### 5.2 Fase 2 — Metadati rapidi (~12 min)

Lettura dei primi 128 KB: sufficiente per l'header EXIF di JPEG, HEIC e quasi tutti i RAW. ~2 ms per file. Da qui la timeline è ordinata per data, i filtri per fotocamera funzionano, la mappa è popolata, la ricerca è utilizzabile.

### 5.3 Fase 3 — Hash (~45 min su NVMe)

blake3 in streaming multi-thread, limitato dal disco. Alle scansioni successive l'hash non viene ricalcolato se `(size, mtime, inode)` combaciano: la riscansione del TB scende a ~2 minuti.

### 5.4 Fase 4 — Derivati (~2 h)

Percorso `data/derivatives/ab/cd/<hash>.webp`, sharded sui primi due byte per evitare cartelle da 200.000 file.

| Derivato | Dimensione | Uso |
|---|---|---|
| thumbhash | 25 byte, in DB | placeholder immediato |
| thumbnail | 240 px, WebP m6 | griglia |
| preview | 1440 px, WebP m6 q78 + `sharp_yuv` | visualizzatore, mobile |
| originale | — | download e zoom 1:1 |

Ottimizzazioni, in ordine di applicazione:

1. **Thumbnail EXIF incorporato** se ≥240 px: ~1 ms invece di ~200 ms.
2. **Decodifica DCT a scala ridotta**: per una preview 1440 da un JPEG 6000 px si decodifica a 1/4. ~8× meno lavoro.
3. **Una sola decodifica per due derivati**: il buffer decodificato produce preview, thumbnail e thumbhash. −45% di tempo.
4. **Salto della preview** quando l'originale è già ≤1600 px e ≤400 KB: si serve l'originale. −25% su tempo e spazio.
5. **`sharp_yuv`**: qualità percepita di q85 al costo di q78. −8% di spazio.
6. **Qualità adattiva** guidata da SSIM su campione: le immagini piatte scendono a q68 senza differenza visibile. −12% medio.
7. **`fast_image_resize`** Lanczos3 con dispatch runtime NEON/AVX2.
8. **Memoria**: buffer RGB8, decodifica a scala ridotta, worker limitati da semaforo sulla RAM stimata oltre che dal numero di core.

Stima risultante su 200.000 asset: **~20 GB, ~2 h 10 m** su RPi 5 con NVMe.

Formato e risoluzione sono modificabili in Impostazioni (WebP, AVIF, JPEG; 1080/1440/2048/originale; qualità adattiva o fissa). La rigenerazione è un job di background interrompibile: i derivati vecchi restano validi finché i nuovi non sono pronti.

### 5.5 RAW

```
1. Sidecar .xmp presente?         → leggo rating, tag, GPS, descrizione
2. Estrazione preview incorporata
     ARW → JPEG full-size    NEF → JPEG full-size (maggior parte dei corpi)
     CR3 → box PRVW ~1620px  CR2 → IFD preview full-size
     DNG → preview standard
3. Preview ≥1440 px?              → è la preview. Fine. (~40 ms)
4. Piccola o assente?             → demosaic libraw half-size, WB camera (~1,5-4 s su ARM)
5. Fallito?                       → status='error', compare in Problemi
```

Il passo 3 copre il 90-95% dei file Sony, Nikon e Canon: il culling a risoluzione piena costa quasi zero. Il demosaic completo resta come azione esplicita "Genera anteprima alta qualità" e come opzione di libreria. Nessun darktable nell'immagine (+800 MB e lentissimo su ARM).

Formati: ARW/ARQ, NEF/NRW, CR2/CR3, DNG, ORF, RAF.

### 5.6 Video

```
ffprobe → codec, contenitore, durata, risoluzione, rotazione, HDR
  ├─ H.264 + AAC in MP4/MOV, ≤1080p   → DIRECT PLAY, nessuna transcodifica
  ├─ H.264 4K                          → direct play su Wi-Fi, 720p su rete mobile
  ├─ HEVC / AV1 / VP9 / MKV            → transcodifica ON DEMAND al primo play,
  │                                       720p H.264, in cache su disco
  └─ HDR (HLG/PQ)                      → tone mapping in transcodifica
```

Derivati sempre generati: poster frame al 10% della durata e anteprima animata di 3 s in WebP per l'hover. Se il probe indica che servirebbe transcodifica ma nessuno ha aperto il video, non si fa nulla.

**Consegna**: i video in direct play sono serviti come file progressivo con range request (`/media/original/{id}`); i video transcodificati sono serviti in **HLS** (`/media/video/{id}/hls`), che consente di iniziare la riproduzione prima del completamento e di saltare a un punto qualsiasi senza attendere l'intero file.

### 5.7 Priorità

| Livello | Job |
|---|---|
| 0 interattivo | preview di una foto in apertura, transcodifica di un video avviato |
| 1 alto | file appena caricati (web, mobile, WebDAV) |
| 2 visibile | miniature dei bucket nel viewport corrente |
| 3 background | resto della scansione |

Il frontend comunica i bucket visibili e il backend riordina la coda: scorrendo al 2019, le miniature del 2019 vengono generate per prime.

### 5.8 Watcher

`notify` con debounce a 2 s e coalescing.

- **Limite inotify**: all'avvio si legge `max_user_watches`, si stima il fabbisogno e, se insufficiente, si mostra il comando `sysctl fs.inotify.max_user_watches=524288` **e** si ripiega automaticamente su scansione periodica.
- **Filesystem di rete**: su NFS, SMB e mount rclone inotify non funziona; si rileva il tipo di mount e si passa a polling (default 15 min).

### 5.9 Fallimenti

1. **Libreria non raggiungibile** → stato `offline`, job fermati, **nulla viene cancellato**, banner in interfaccia.
2. **Sparizione di massa** → se una scansione rileva la scomparsa di oltre il **20%** dei file, si ferma e chiede conferma elencando cosa sparirebbe. Protegge da mount parziali e permessi cambiati.
3. **File illeggibile** → `status='error'` con motivo, 3 tentativi con backoff esponenziale, poi pagina Problemi con azioni riprova / ignora / mostra percorso. Non blocca la coda.

Ogni job è idempotente: al riavvio i job `running` da oltre 10 minuti tornano `pending`.

### 5.10 Stime (1 TB, ~200k file, RPi 5 + NVMe)

| Fase | Tempo | Utilizzabile |
|---|---|---|
| Discovery | ~3 min | navigazione cartelle |
| Metadati | ~12 min | timeline, ricerca, mappa |
| Hash | ~45 min | duplicati, spostamenti |
| Thumbnail | ~2 h | griglia completa |
| Preview | in background | ~20 GB |
| Riscansione | ~2 min | — |

Su USB 3.0 i tempi raddoppiano; su VPS x86 con SSD si dimezzano.

---

## 6. Condivisione

### 6.1 Oggetti condivisibili

| Oggetto | Cosa vede il destinatario |
|---|---|
| Foto singola | solo quella, senza risalire alla cartella |
| Cartella | il sottoalbero navigabile, inclusi i file aggiunti in seguito |
| Album | l'insieme curato, senza esporre la struttura su disco |

Chi riceve una cartella condivisa non vede mai il percorso reale sul filesystem: vede `Vacanze / 2024 / Grecia`, non `/mnt/nas/foto/…`.

### 6.2 Link pubblici

Token da 32 byte casuali in base64url, **salvato come hash**: un dump del database non apre i link. Opzioni: password (argon2id), scadenza, numero massimo di visite, download consentito, originale consentito, upload da ospite, `hide_metadata`.

- `X-Robots-Tag: noindex, nofollow` e `Referrer-Policy: no-referrer` su tutte le pagine pubbliche.
- Rate limiting per token e per IP sui tentativi di password.
- `hide_metadata` **attivo di default sui link senza password**: preview servite senza EXIF, mappa nascosta, coordinate assenti anche dall'API.
- Impostazione **"nascondi posizioni entro N metri da un punto"**: le foto scattate a casa risultano prive di coordinate nei contenuti condivisi, mantenendo il dato nel database.
- Revoca immediata; pagina "Link attivi" con ultimo accesso e conteggio visite.
- Flag `allow_cdn_cache` per-link: i contenuti pubblici possono essere serviti con `Cache-Control: public` da un URL separato, cacheabile da CDN. Spento di default. Tutto il resto è `Cache-Control: private`.

**Upload da ospite**: i file arrivano nella cartella di destinazione con flag `uploaded_by_guest` e finiscono in una coda di revisione che il proprietario approva o scarta. Limite di dimensione totale configurabile per link.

### 6.3 Pannello permessi

Disponibile su foto, cartella, album e su selezioni multiple. Distingue visivamente permessi **diretti** ed **ereditati**; su un ereditato consente di sovrascrivere il ruolo o escluderlo su quel nodo senza toccare il livello superiore; cliccando un utente mostra la catena del perché ha accesso. Include l'elenco dei link pubblici attivi con copia e revoca.

Pagina globale **"Condivisioni"**: tutto ciò che esce, chi vede cosa, revoca di massa.

### 6.4 Audit log

Registra: creazione e revoca di condivisioni e link, accessi ai link pubblici, cancellazioni dal disco, cambi di ruolo, accessi dell'admin a contenuti altrui.

---

## 7. Upload e WebDAV

### 7.1 Protocollo tus 1.0

Standard aperto con client maturi per JavaScript, Kotlin, Swift e Dart: l'upload riprendibile dell'app mobile è già risolto.

```
① PRE-CHECK       POST /api/v1/upload/check   → hash noti, "carica solo questi 12"
② CREAZIONE       POST /api/v1/upload         Upload-Length, filename,
                                              target_folder_id, client_mtime, blake3
③ RIPRESA         HEAD /api/v1/upload/{id}    → Upload-Offset (verità sul server)
④ INVIO           PATCH …                     chunk + Upload-Checksum, fsync ogni 16 MB
⑤ FINALIZZAZIONE  hash completo → verifica decodificabilità → rename() atomico
                  → job di indicizzazione priorità 1
```

- L'offset lo dichiara **sempre il server**: nessuno stato locale di cui fidarsi.
- Checksum per chunk (errore `460` e reinvio immediato) e checksum end-to-end a file completo.
- Verifica di decodificabilità oltre all'hash: header valido, dimensioni leggibili, `ffprobe` per i video.
- Chunk adattivi: 8 MB su rete buona, fino a 1 MB su rete instabile.
- 3 file in parallelo, chunk sequenziali per file.
- Spazio disco verificato prima di accettare la sessione.
- `mtime` originale preservato.
- Temporanei in `.keeppix-tmp/` **dentro la stessa libreria**: stesso filesystem, `rename()` atomico anche per file da 2 GB. Sessioni abbandonate ripulite dopo 7 giorni.
- Collisioni: stesso nome e stesso hash → duplicato, saltato e segnalato; stesso nome, contenuto diverso → `IMG_1234_1.ARW` con notifica. Mai sovrascrittura silenziosa.

### 7.2 WebDAV

Montato su `/dav/`, espone **solo l'albero delle cartelle**. La radice elenca le librerie accessibili. Autenticazione Basic su HTTPS con **app-password** dedicate (nome, ultimo uso, revoca indipendente).

| Metodo | Comportamento |
|---|---|
| `PROPFIND` | elenco **dal database** |
| `GET` | file originale con range request |
| `PUT` | temporaneo → verifica → `rename()` atomico → indicizzazione priorità 1 |
| `MKCOL` | crea cartella (permesso editor) |
| `MOVE` | `rename()` su disco; il rilevamento spostamenti conserva rating, album, descrizioni |
| `COPY` | copia reale, con avviso sullo spazio |
| `DELETE` | **solo owner e admin**; editor riceve `403`. **Sempre nel cestino** (`.keeppix-trash/`, 30 giorni) |
| `LOCK` / `UNLOCK` | Class 2, obbligatori per Finder e Windows; lock persistiti in Postgres |

**Ottimizzazioni**

1. `PROPFIND` servito da una singola query Postgres: ~40 ms su una cartella con 40.000 file, contro 5-15 s con `stat()` per file.
2. XML generato in **streaming**: pochi KB di RAM invece di decine di MB per una risposta da 14 MB.
3. **`ETag` = content hash**: rclone e Cyberduck scaricano solo ciò che è realmente cambiato.
4. Nessun `stat()` a raffica: il DB è la fonte dei metadati, il filesystem si tocca solo per i byte.
5. `quota-available-bytes` esposto.
6. `PUT` non passa dal watcher: il completamento è noto, l'indicizzazione parte subito.

**Compatibilità client**: macOS Finder richiede Class 2 (implementato) e scrive `.DS_Store` e file `._*` — accettati ma esclusi dall'indicizzazione, con opzione per scartarli. Windows Explorer ha un limite di 50 MB per file e pretende HTTPS valido: documentata la chiave di registro, ma si consiglia rclone o Cyberduck. rclone è il client di riferimento per la sincronizzazione di cartelle locali, con configurazione pronta nella documentazione.

**Limite dichiarato**: per caricare centinaia di RAW l'upload tus dalla web app è più veloce e robusto. WebDAV dà il meglio come disco montato e per la sincronizzazione automatica.

---

## 8. Mappa e geocoding

### 8.1 Tile

**MapLibre GL JS** con **PMTiles locali**: un file per regione in `data/maps/`, servito dal backend con range request. Nessun tile server, nessuna richiesta a terzi — che vedrebbero l'IP e le zone consultate, cioè approssimativamente dove sono state scattate le foto.

Il bundle MapLibre (~230 KB gzip) è caricato pigramente: chi non apre la mappa non lo scarica.

**Gestore regioni** in Impostazioni, granularità paese più aggregati continentali e ritaglio di un'area sulla mappa:

- elenco delle regioni scaricate con dimensione, versione e cancellazione;
- catalogo per continente con dimensioni;
- download riprendibile, verificato con checksum, in background;
- avviso sulle regioni più vecchie di 6 mesi e aggiornamento notturno opzionale.

**Ricerca fuori dalle regioni scaricate**: il database dei luoghi (GeoNames, tutto il mondo) è indipendente dalle tile. Assegnare "Kyoto" a 400 foto **funziona sempre**; manca solo lo sfondo. L'interfaccia avvisa, offre il download della regione in background e **non blocca mai l'assegnazione**.

### 8.2 Punti sulla mappa

Aggregazione a griglia lato server, cella derivata dallo zoom:

```sql
SELECT ST_SnapToGrid(a.location::geometry, $cell) AS cell, count(*) AS n,
       (array_agg(a.id ORDER BY COALESCE(fl.rating, 0) DESC, a.taken_at_utc DESC))[1] AS cover
  FROM assets a
  LEFT JOIN asset_flags fl ON fl.asset_id = a.id AND fl.user_id = $me
 WHERE a.location && ST_MakeEnvelope($bbox) AND <scope di visibilità>
 GROUP BY cell;
```

Indice GiST su `location`; risposta tipica 5-15 KB in 3-9 ms. Oltre lo zoom 14 si restituiscono punti singoli con tetto configurabile. I cluster mostrano la **miniatura della foto migliore del gruppo** secondo il rating di chi guarda (rating e pick sono per utente, §4.5).

**Nessuna regione scaricata**: la vista mappa mostra uno stato vuoto con il catalogo delle regioni e la stima di spazio, invece di una mappa grigia. La ricerca dei luoghi e l'assegnazione delle posizioni restano pienamente funzionanti.

Interazioni: click su cluster per zoom, click su punto per aprire la foto, **disegno di un'area** che filtra la timeline, modalità heatmap, mappa applicabile a libreria, album, cartella o risultato di ricerca (stesso endpoint, un parametro), mini-mappa nel pannello dettagli con "altre foto scattate qui".

### 8.3 Geocoding

Dataset **GeoNames `cities500` + admin1/admin2 + countryInfo**: ~200.000 località, 11 MB compressi, **inclusi nell'immagine**, ~150 MB in Postgres.

**Reverse**: k-NN GiST `ORDER BY location <-> $point LIMIT 1`, <1 ms, con soglia di distanza ponderata sulla popolazione (un paese di 600 abitanti vale entro 3 km, una città di 500.000 anche a 25 km); fuori soglia si ripiega su regione e poi nazione.

**Forward con autocomplete**: GIN `pg_trgm` su nome normalizzato senza accenti, ~4 ms, ordinato per popolazione **e** per vicinanza ai luoghi già frequentati dall'utente. Scelta la località, su tutta la selezione vengono scritti `location`, `place_id` e nome negli override, e da lì nei sidecar.

**Altri modi di geolocalizzare**: trascinamento del pin sulla mappa; **copia posizione da un'altra foto** (una foto col GPS del telefono e 200 RAW senza); **import GPX** — non nella v1 ma predisposto da `location_source = 'gpx'`, dal geotagging batch già parametrico e da `taken_at_utc` normalizzato, senza il quale il matching temporale non funzionerebbe.

### 8.4 Fuso orario

Le reflex non registrano il fuso: un `.ARW` scattato a Tokyo alle 14:00 finirebbe in timeline alle 06:00 su un server italiano. Una versione semplificata dei confini dei fusi (~8 MB in PostGIS) permette di ricavare il fuso dalle coordinate e normalizzare `taken_at_utc`, conservando l'ora locale per la visualizzazione. **Attivo di default.**

Su una libreria già catalogata, l'azione "ricalcola fusi orari" mostra un'anteprima delle modifiche ed è annullabile in blocco. Per le foto senza GPS resta l'azione manuale "sposta di N ore" sulla selezione.

---

## 9. API

### 9.1 Contratto

Tutto sotto `/api/v1`, **contratto congelato**: solo aggiunte. Una rottura genera `/api/v2` e la v1 resta viva almeno 12 mesi con header `Deprecation` e `Sunset`.

**OpenAPI 3.1 generato dal codice** con `utoipa`: gli handler *sono* la specifica. Da `/api/openapi.json` si generano i client Kotlin, Swift, Dart e TypeScript. Un test in CI fallisce sui cambiamenti incompatibili.

| Gruppo | Endpoint |
|---|---|
| Auth | `POST /auth/login` · `/auth/refresh` · `/auth/devices` · `/auth/app-passwords` · `/auth/totp` |
| Timeline | `GET /timeline/buckets` · `GET /timeline?bucket=` |
| Asset | `GET /assets/{id}` · `PATCH /assets/{id}` · `POST /assets/batch` |
| Cartelle | `GET /folders/tree` · `GET /folders/{id}/children` |
| Album | CRUD · `POST /albums/{id}/assets` |
| Ricerca | `POST /search` · `GET /search/suggest` |
| Mappa | `GET /map/clusters?bbox=&zoom=` · `GET /places/suggest?q=` |
| Condivisione | `GET|POST|DELETE /permissions` · `POST /share-links` |
| Upload | tus su `/upload` |
| Sync | `GET /sync/delta?cursor=` |
| Eventi | `GET /ws` (WebSocket) |
| Media | `/media/thumb/{hash}` · `/media/preview/{hash}` · `/media/original/{id}` · `/media/video/{id}/hls` |

### 9.2 Scelte per il client mobile

**Sincronizzazione incrementale**

```
GET /sync/delta?cursor=88421
→ { cursor: 91055, upserted: [...], deleted: ["018f…"], has_more: true }
```

Richiede `change_log` alimentato da trigger e **tombstone** per le cancellazioni. Accorgimento necessario: una transazione con `seq` più basso può committare dopo una con `seq` più alto, quindi il cursore restituito è arretrato al limite delle transazioni certamente concluse (`pg_snapshot_xmin`), non all'ultimo `seq` osservato.

**ID stabili**: UUID v7 generabili dal client, così un asset creato offline non richiede riconciliazione di identità.

**Errori come dati**: RFC 9457 `application/problem+json` con `type` stabili (`keeppix/quota-exceeded`). Il client decide sul codice, mai sul testo — il backend non traduce nulla, la localizzazione avviene nel client.

**Idempotenza**: header `Idempotency-Key` su tutte le mutazioni. Un'app mobile ritenta di continuo.

### 9.3 WebSocket

**Regola architetturale**: il WebSocket è un canale di **notifica**, non la fonte di verità. Alla riconnessione il client chiama sempre `/sync/delta?cursor=`. Se il socket perde messaggi, l'applicazione resta corretta.

1. **Autenticazione con ticket monouso**: `POST /api/v1/ws/ticket` restituisce un ticket valido 30 secondi, passato nel sottoprotocollo. Mai il token in query string, dove finirebbe nei log del reverse proxy. Il client mobile usa l'header `Authorization`.
2. **Verifica dell'`Origin`** all'handshake: `SameSite` non si applica alle connessioni WebSocket, quindi senza questo controllo qualsiasi sito aperto nel browser potrebbe aprire un socket autenticato.
3. **Versionamento nel sottoprotocollo** (`keeppix.v1`): un'app vecchia continua a parlare v1.
4. **Backpressure**: coda per connessione limitata a 256 messaggi; al superamento viene svuotata e sostituita da un singolo `resync`. Un client lento non può gonfiare la RAM del server.
5. **Coalescing**: un messaggio di avanzamento aggregato ogni 250 ms, notifiche di nuovi asset in batch. Il volume del canale è indipendente dal volume del lavoro.
6. **Sottoscrizioni esplicite** (`timeline`, `album:{id}`, `jobs`, `folder:{id}`), con ogni evento filtrato dallo stesso `visibility_scope` delle query REST. Al cambio permessi viene emesso `permissions_changed` e le sottoscrizioni non più valide cadono.
7. **Heartbeat** ogni 30 s, peer morto dopo due pong mancati; riconnessione con backoff esponenziale **e jitter**.
8. **`permessage-deflate` disattivata**: ~300 KB di RAM per connessione con i parametri di default, inutile su messaggi da 200 byte.
9. **Fallback a polling** `/sync/delta` ogni 15 s dopo tre handshake falliti.
10. **Limiti**: 8 connessioni per utente, messaggio in ingresso ≤64 KB, rate limit sui messaggi in entrata, chiusura garbata con codice 1001 al riavvio.

### 9.4 Prestazioni

Non c'è cache esterna. In ordine di impatto:

1. **URL dei derivati contenenti l'hash** → `Cache-Control: public, max-age=31536000, immutable`. Il browser non li richiede mai più: alla seconda visita di una griglia il server riceve zero richieste.
2. **`ETag` + `304`** sugli endpoint JSON.
3. **HTTP/2 obbligatorio**: una griglia carica 200 miniature; su HTTP/1.1 sarebbero 6 connessioni in coda.
4. **Zero-copy** nello streaming dei file.
5. **Brotli sul JSON**, nessuna compressione sulle immagini.
6. **Cache in-process** (`moka`, ~60 MB): scope di visibilità, albero cartelle, conteggi mensili, sessioni, lookup GeoNames, metadati recenti.

Nessun uso di CDN per contenuti privati: `Cache-Control: private` su tutto ciò che è autenticato.

### 9.5 Sicurezza

**Autenticazione**: Argon2id con parametri OWASP verificati sull'hardware al primo avvio; rate limit progressivo per IP e account; messaggi d'errore indistinguibili; confronti a tempo costante.

**Superficie web**: CSP con nonce senza `unsafe-inline`; `frame-ancestors 'none'`; HSTS; `Referrer-Policy: no-referrer`; `Permissions-Policy` che nega camera, microfono e geolocalizzazione. CSRF: `SameSite=Lax` più obbligo di `Content-Type: application/json` e di un header custom sulle mutazioni — un form HTML esterno non può produrli.

**Filesystem**: nessun percorso arriva dal client; l'accesso avviene per `id` o `content_hash` e il path si risolve lato server. Ogni accesso passa da canonicalizzazione e verifica di appartenenza alla radice della libreria; i symlink che escono dalla radice non vengono seguiti. Nessuna richiesta HTTP verso URL forniti dall'utente: i download delle mappe puntano a host in allowlist fissa.

**Decodifica di file non fidati** — la superficie più delicata. I decoder Rust sono memory-safe, ma libraw e ffmpeg sono C:

> Ogni decodifica che coinvolge codice C gira in un **processo separato usa-e-getta**, con `rlimit` su memoria e CPU, filtro **seccomp** che nega rete e scritture arbitrarie, nessun privilegio. Un exploit su un `.CR3` malevolo muore lì dentro.

Costo: pool di processi pre-avviati, ~1-2 ms per file. In più: rifiuto di immagini oltre 200 megapixel, timeout duri su ffmpeg.

**Database**: `sqlx` con query verificate a compile-time. Il parser di ricerca produce un AST che genera query parametrizzate: la stringa dell'utente non tocca mai l'SQL.

**Catena di fornitura**: `cargo-audit` e `cargo-deny` in CI, SBOM a ogni release, immagini firmate, rebuild settimanale automatica con `osv-scanner` sulla versione di ffmpeg.

---

## 10. UI/UX

### 10.1 Principio

**Superficie pulita, potenza a un gesto di distanza.** La sintesi fra Immich e Google Photos non è una media, è **divulgazione progressiva**:

- la timeline non ha barre degli strumenti finché non selezioni;
- i filtri sono chip sotto la ricerca, non un pannello permanente;
- le funzioni professionali vivono in **modalità** che si entrano e si escono.

**Regola dura contro la sovrapposizione**: il visualizzatore normale non diventa mai una modalità; la modalità culling non diventa mai il default. Nel visualizzatore normale entrano solo due azioni atomiche — rating (`1-5`) e preferito (`f`) — e su mobile le stelle stanno nel pannello informazioni, non sopra la foto. Tutto il resto è esclusivo del culling, che ha **un unico punto d'ingresso**.

### 10.2 Navigazione

**Mobile**: barra inferiore `Foto · Cerca · Album · Libreria`. Libreria raccoglie Cartelle (prima voce, in evidenza), Mappa, Preferiti, Condivisi con me, Le mie condivisioni, Cestino, Problemi. Pressione lunga sulla tab Libreria porta direttamente alle Cartelle. In Impostazioni → Aspetto è possibile scambiare Album e Cartelle nella terza posizione.

Motivazione: il lavoro sulle cartelle è lavoro da desktop (organizzare, spostare, condividere); il mobile è consumo. Dedicare il 25% della barra alle cartelle ottimizzerebbe per l'uso raro.

**Desktop**: sidebar sinistra con albero cartelle sempre visibile, griglia più densa.

### 10.3 Timeline

Griglia **giustificata** (righe di altezza costante, larghezze proporzionali all'aspect ratio): i panorami restano panorami.

- Header appiccicosi per giorno e mese con conteggio.
- **Scrubber laterale** con etichette di anno e mese (componente di urocissa, alimentato dai bucket).
- Densità regolabile da 2 a 12 colonne (pinch su mobile, `+`/`−` su desktop), salvata per dispositivo.
- Placeholder **thumbhash**: mai rettangoli grigi, mai layout shift.
- Selezione: click, shift+click, rettangolo di trascinamento su desktop; pressione lunga e trascinamento su mobile. "Seleziona tutto" istantaneo su 200.000 foto (seleziona la query, non gli elementi).
- Barra contestuale in basso alla selezione: condividi, album, tag, posizione, rating, scarica, elimina.
- Chip permanente `[ Tutti | Foto | Video ]`, scelta ricordata; `type:video` funziona anche nella sintassi di ricerca.

### 10.4 Visualizzatore

Schermo intero, swipe, pinch-zoom, doppio tap per 1:1, con le due foto adiacenti precaricate. Pannello informazioni con dati di scatto, percorso cartella, luogo con mini-mappa, tag, rating.

Scorciatoie: `←→` naviga · `i` info · `z` zoom 1:1 · `1-5` rating · `f` preferito · `Canc` elimina · `Spazio` play.

### 10.5 Modalità culling

Foto grande con zoom 1:1, filmstrip in basso, contatori scelte/scarti/da vedere e filtri.

- Tastiera-centrica con **avanzamento automatico** dopo il voto.
- **Zoom 1:1 per il fuoco**: richiede l'originale; si precarica il ritaglio centrale a piena risoluzione delle 3 foto successive.
- **Confronto** affiancato di 2-4 scatti (`c`).
- Filtro sugli scarti ed eliminazione in blocco, che apre il dialogo a tre opzioni (indice / cestino / disco).
- Su album condivisi in modalità collaborativa, i pick del cliente compaiono con il suo avatar.

### 10.6 Ricerca

Una barra che accetta testo libero e **query booleane** (parser Chevrotain di urocissa), con filtri esposti come chip: periodo, fotocamera, obiettivo, ISO, luogo, rating, tipo, cartella. Ogni ricerca è **salvabile** e compare nella sidebar come raccolta viva — sostituisce gli "album intelligenti" senza introdurre un secondo concetto.

### 10.7 Cartelle

Breadcrumb, cartelle come schede con anteprima e conteggio, foto sotto. Su desktop albero in sidebar con drag&drop per spostare (che diventa un `rename()` con conferma). Azioni per cartella: condividi, scansiona adesso, escludi, apri mappa, statistiche.

### 10.8 Upload

Destinazione scelta ogni volta, con creazione di nuova cartella inline e opzione "ricorda per questa sessione". Segnalazione dei file già presenti prima di iniziare. Pannello persistente e minimizzabile: si naviga durante il caricamento, e chiudendo la scheda gli upload interrotti riprendono dal punto esatto.

**PWA con Share Target**: dalla galleria del telefono, "Condividi → Keeppix" avvia il flusso di upload — copre il requisito di caricamento manuale da mobile senza un'app nativa.

### 10.9 Prestazioni frontend

- **Budget bundle iniziale: 150 KB gzip**; mappa, culling, impostazioni e player video in chunk separati.
- Virtual scroll di urocissa, `content-visibility: auto`, dimensioni esplicite sulle immagini.
- Il frontend comunica i bucket visibili, il backend riordina la coda delle miniature.
- UI ottimistica su rating, preferiti e aggiunta ad album; nessuno spinner sotto i 200 ms.
- **Service worker**: shell e miniature già viste navigabili offline.

### 10.10 Trasversali

Tema chiaro e scuro con rilevamento di sistema, scuro sempre nel visualizzatore a schermo intero. Accessibilità: navigazione da tastiera completa, focus visibile, contrasto AA, rispetto di `prefers-reduced-motion`.

**i18n**: `vue-i18n` con formato **ICU MessageFormat** (plurali corretti), date e numeri con l'API `Intl` nativa, file JSON per lingua, controllo in CI su chiavi mancanti. Lingua **rilevata** da `navigator.language` al primo accesso, poi modificabile in Impostazioni e salvata nel profilo. Italiano e inglese completi al rilascio.

**Stati vuoti utili** con l'azione giusta accanto. **Pagina Problemi** come sportello unico: file corrotti, librerie offline, job falliti, sidecar non scrivibili.

---

## 11. Deployment

### 11.1 Immagine

**`gcr.io/distroless/cc-debian12`** multi-arch `amd64` + `arm64`, ~210 MB, con ffmpeg e helper libraw **compilati staticamente da noi**. Utente non-root, root filesystem in sola lettura, `no-new-privileges`, capability azzerate. Tag `:1-debug` identico ma con busybox.

glibc, non musl: l'allocatore di musl è lento sui carichi Rust multi-thread, ed è esattamente il nostro caso.

Guadagno: nessuna shell né package manager (la maggior parte delle tecniche di post-exploitation presuppone `/bin/sh`), ~6 pacchetti da monitorare invece di ~90. Nessuna differenza di prestazioni a runtime.

Costo accettato: la manutenzione delle build ffmpeg è nostra. Mitigato da rebuild settimanale in CI, `osv-scanner` sulla versione ffmpeg con apertura automatica di issue, e dal fatto che ffmpeg gira già in un processo sandbox senza rete né privilegi. **Attenzione**: le build statiche pronte all'uso spesso non includono VAAPI, rkmpp e v4l2m2m — ffmpeg va compilato con i flag di accelerazione, altrimenti il rilevamento hardware non troverebbe nulla.

Healthcheck via sottocomando `keeppix healthcheck` (nessun `curl` disponibile).

### 11.2 Compose

```yaml
services:
  keeppix:
    image: ghcr.io/keeppix/keeppix:1
    environment:
      DATABASE_URL: postgres://keeppix:${DB_PASSWORD}@db/keeppix
    volumes:
      - ./data:/data              # derivati, mappe, backup
      - /mnt/nas/foto:/photos:ro  # originali
    ports: ["5673:5673"]

  db:
    profiles: ["bundled"]         # non parte con un Postgres esterno
    image: postgis/postgis:17-3.5
    volumes: ["./pgdata:/var/lib/postgresql/data"]
```

`docker compose --profile bundled up -d` per l'installazione completa; `docker compose up -d` con `DATABASE_URL` esterno per riusare Postgres esistente.

Configurazione: **variabili d'ambiente → `config.toml` → default**. L'unica variabile obbligatoria è `DATABASE_URL`. Nessun segreto predefinito: la chiave di sessione è generata al primo avvio e persistita.

### 11.3 Primo avvio

1. **Crea l'amministratore** — password forte richiesta, TOTP opzionale.
2. **Rilevamento hardware** — benchmark con risultati mostrati.
3. **Dove sono le tue foto?**
   - *Sono già su questo server* → esplorazione dei percorsi montati con anteprima (conteggi per tipo, spazio, file esclusi) → scansione.
   - *Devo ancora caricarle* → libreria vuota + **wizard WebDAV**: cartella di destinazione, app-password generata e mostrata una sola volta, istruzioni per Finder, Windows/Cyberduck, rclone (blocco di configurazione da copiare) e mobile (QR), con **indicatore live della prima connessione ricevuta**.
   - *Configuro più tardi.*
4. **Scansione** — anteprima prima di partire con conteggi per formato, esclusi, stima dei tempi per fase e dello spazio derivati; opzione "avvia stanotte".
5. **Mappe offline** — scelta delle regioni da scaricare.

Su libreria vuota il watcher è comunque attivo: i file che arrivano via WebDAV vengono indicizzati man mano.

In alternativa al wizard, su un'istanza nuova viene offerto **"Ripristina da backup"**.

### 11.4 Migrazioni

`sqlx` applicate all'avvio in transazione, con dump automatico del database prima di ogni migrazione distruttiva. Il tag `:1` segue la major. Il downgrade è supportato solo via ripristino del dump, e la documentazione lo dichiara.

### 11.5 Backup

**Formato**: un file `keeppix-<timestamp>.kpxb`, tar compresso zstd e cifrato con **`age`**.

```
manifest.json     versione, data, istanza, conteggi, checksum
database.dump     pg_dump formato custom
config.toml       configurazione, segreti cifrati
maps.json         elenco regioni + versione (non i GB di tile)
sidecars/         file .xmp        (opzionale)
derivatives/      miniature        (opzionale, sconsigliato)
originals/        foto             (opzionale)
```

`age` e `tar` sono strumenti standard: **il backup resta apribile senza Keeppix**. Un formato leggibile solo dal software che l'ha creato non è un backup.

**Destinazioni**: percorso locale, S3-compatibile (AWS, B2, R2, MinIO, Wasabi), WebDAV remoto, SFTP. Credenziali cifrate a riposo, test della connessione prima del salvataggio. Più destinazioni contemporanee.

**Wizard di backup**: selezione dei contenuti con dimensioni e note ("il database è irrecuperabile", "i derivati si rigenerano"), pianificazione manuale o automatica nella finestra notturna, conservazione intelligente (7 giornalieri, 4 settimanali, 12 mensili) o a numero fisso, cifratura, verifica dopo la scrittura, **prova di ripristino mensile** del dump in schema temporaneo.

Due elementi non negoziabili:

- **Avviso in evidenza** quando gli originali non sono inclusi: *"questo backup non contiene le tue foto, contiene il catalogo"*. Il modo più comune di perdere le foto è credere il contrario.
- **La prova di ripristino** è ciò che distingue avere backup dall'avere ripristini.

**Wizard di ripristino**: selezione della sorgente con manifest leggibile (data, versione, conteggi), scelta dei componenti (database, configurazione, elenco mappe con riscaricamento, sidecar, originali se presenti), **anteprima delle modifiche in simulazione** prima di scrivere, dump di sicurezza dello stato attuale prima di sovrascrivere. Un backup più recente della versione installata viene rifiutato con messaggio chiaro; uno più vecchio fa applicare le migrazioni. Il ripristino delle sole mappe non tocca il database ed è utilizzabile a caldo.

**Cosa vale la pena salvare**: il database è l'unica cosa irrecuperabile (album, condivisioni, rating, utenti, override). I derivati si rigenerano, le mappe si riscaricano, gli originali sono responsabilità dell'utente — Keeppix non è un sistema di backup e non deve fingere di esserlo. Poiché rating, descrizioni e posizioni finiscono anche nei sidecar XMP, **perdendo il database una nuova scansione restituisce la gran parte del lavoro di catalogazione**: si perdono utenti, album e condivisioni, non il lavoro sulle foto.

### 11.6 Osservabilità

Log strutturati JSON con `tracing`; `/health` e `/metrics` Prometheus opzionale; pagina **Sistema** in interfaccia con job in corso e in coda, throughput, spazio per categoria, stato librerie, storico scansioni, profilo energetico attivo.

### 11.7 CI

Build multi-arch, test unitari e di integrazione con Postgres reale (testcontainers), `cargo-audit`, `cargo-deny`, controllo di compatibilità OpenAPI, controllo delle traduzioni mancanti, SBOM, firma delle immagini, rebuild settimanale.

---

## 12. Decomposizione in fasi

Ogni fase ha spec, piano e implementazione propri.

| Fase | Contenuto | Criterio di completamento |
|---|---|---|
| **0** | Scheletro: workspace, Postgres, migrazioni, Axum, auth, Docker, CI, frontend minimo | login funzionante, immagine pubblicata, test verdi |
| **1** | Ingestione: librerie esterne, discovery, metadati, hash, derivati, watcher, timeline, ricerca | 1 TB indicizzato, timeline navigabile con scrubber |
| **2** | RAW: estrazione preview, EXIF estesi, override, sidecar XMP, editing batch, culling | 11.000 RAW selezionabili con rating e pick |
| **3** | Multiutente: utenti, gruppi, ACL, album, condivisione, link pubblici, audit | condivisione di cartella a un utente esterno |
| **4** | Mappe: PMTiles, gestore regioni, cluster, GeoNames, geocoding, fusi orari | assegnazione posizione in batch |
| **5** | WebDAV: PROPFIND da DB, lock, upload tus, wizard di configurazione | rclone bisync su una cartella reale |
| **6** | Consolidamento: video on-demand, backup e ripristino, 2FA, OpenAPI pubblica, PWA, i18n | client mobile generabile dalla specifica |

---

## 13. Rischi noti

| Rischio | Impatto | Mitigazione |
|---|---|---|
| Manutenzione delle build ffmpeg statiche | CVE non patchate | rebuild settimanale, `osv-scanner`, sandbox seccomp del processo |
| Prima scansione lunga su hardware lento | percezione di lentezza | fasi progressive, timeline utile dopo 15 minuti, stime mostrate in anticipo |
| Limite inotify su alberi grandi | watcher parziale | rilevamento all'avvio, istruzioni, ripiego automatico su polling |
| ltree con alberi molto profondi | query lente | indice GiST, profondità monitorata, prefissi autorizzati tipicamente <10 |
| Preview RAW assenti su corpi rari | demosaic lento | fallback libraw, azione manuale, segnalazione in Problemi |
| Riscrittura EXIF su JPEG | corruzione file | temporaneo + fsync + verifica + rename atomico; opt-in |
| Deriva del contratto API | rottura del client mobile | OpenAPI generato, test di compatibilità in CI, versionamento esplicito |
| Perdita del database | perdita di album e condivisioni | backup automatico verificato, sidecar XMP come rete di sicurezza parziale |
