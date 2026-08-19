# Fase 6 — Consolidamento

**Stato:** specifica di progetto, non ancora pianificata in task
**Dipende da:** tutte le fasi precedenti
**Chiusa quando:** dalla specifica OpenAPI si genera un client funzionante, e un
ripristino da backup su macchina vuota riporta l'istanza allo stato esatto

Questa fase non aggiunge funzioni visibili: rende il sistema **mantenibile** e
pronto per il client mobile. È la fase che si salta volentieri e che poi manca.

---

## 1. Video — completare quello che la Fase 1b ha impostato

La Fase 1b decide la strategia (direct play, transcodifica on-demand); qui si
completa.

- **Playlist HLS** per i transcodificati, con segmentazione che permette di
  saltare a un punto qualsiasi senza attendere il file intero.
- **Cache delle transcodifiche** su disco, con pulizia notturna di ciò che non
  si apre da 90 giorni.
- **Adattamento alla rete**: opzione «risparmia banda in mobile» — in 4G si
  serve la versione 720p, in casa l'originale.
- **Tone mapping** per HDR (HLG/PQ) in transcodifica.
- Accelerazione hardware secondo quanto misurato dal probe della Fase 1b.

---

## 2. Backup e ripristino

### 2.1 Cosa vale la pena salvare

| Cosa | Come | Perché |
|---|---|---|
| **Database** | `pg_dump` compresso, ogni notte, rotazione | **è l'unica cosa irrecuperabile**: album, condivisioni, rating, utenti, override |
| Sidecar XMP | nella cartella delle foto | vanno nel backup delle foto, e rendono rating e metadati recuperabili anche perdendo il DB |
| Derivati | non serve | si rigenerano dagli originali |
| Mappe | non serve | si riscaricano |
| Originali | **responsabilità dell'utente** | Keeppix non è un sistema di backup e non deve fingere di esserlo |

**Proprietà di resilienza da notare**: poiché rating, descrizioni e posizioni
finiscono anche nei sidecar XMP, **anche perdendo completamente il database** una
nuova scansione restituisce la gran parte del lavoro di catalogazione. Si
perdono utenti, album e condivisioni, non il lavoro sulle foto.

### 2.2 Formato del pacchetto

Un file unico `keeppix-<timestamp>.kpxb`, che è un **tar** con dentro:

```
manifest.json     versione, data, istanza, conteggi, checksum di tutto
database.dump     pg_dump formato custom, compresso
config.toml       configurazione (segreti cifrati)
maps.json         elenco regioni scaricate + versione  ← non i GB di tile
sidecars/         i file .xmp        (opzionale, piccoli e preziosi)
derivatives/      miniature e preview (opzionale, sconsigliato)
originals/        le foto             (opzionale, enorme)
```

Compressione **zstd**, cifratura **`age`** con passphrase o chiave pubblica.

**Una proprietà su cui non si transige:** `age` e `tar` sono strumenti standard.
Il backup resta apribile **anche senza Keeppix**, fra dieci anni, con la CLI
`age` e `tar`. Un formato leggibile solo dal software che l'ha creato non è un
backup, è una scommessa.

### 2.3 Destinazioni

Percorso locale · **S3-compatibile** (AWS, Backblaze B2, Cloudflare R2, MinIO,
Wasabi) · WebDAV remoto · SFTP.

Credenziali cifrate a riposo, test della connessione prima di salvare la
configurazione. Più destinazioni contemporanee.

### 2.4 Wizard

```
Backup                                        [ Esegui adesso ]
─────────────────────────────────────────────────────────────
 Contenuto        ☑ Database        142 MB   irrecuperabile
                  ☑ Configurazione   12 KB
                  ☑ Elenco mappe      2 KB
                  ☑ Sidecar XMP      48 MB   rating e metadati
                  ☐ Derivati       20,1 GB   si rigenerano
                  ☐ Originali       1,02 TB

                  ⚠ Senza "Originali" questo backup NON contiene
                    le tue foto. Contiene il catalogo.

 Quando           ○ Solo manuale
                  ● Automatico   [ogni notte ▾] alle [03:00]

 Conservazione    ● Intelligente: 7 giornalieri, 4 settimanali, 12 mensili
                  ○ Ultimi [ 14 ] backup

 Destinazione     [ Backblaze B2 ▾ ]  keeppix-backup/     [testa]
                  ＋ aggiungi seconda destinazione

 Cifratura        ☑ age  ·  passphrase impostata          [cambia]

 ☑ Verifica il backup dopo la scrittura
 ☑ Prova di ripristino mensile del database in schema temporaneo
─────────────────────────────────────────────────────────────
 Ultimo: 13/08 03:00 · 190 MB · ✅ verificato · B2 + locale
```

**Due dettagli valgono più di tutto il resto:**

1. **L'avviso in grassetto.** Il modo più comune di perdere le foto è credere di
   averle nel backup quando c'è solo il catalogo. Deve essere impossibile
   fraintendere.
2. **La prova di ripristino mensile.** Ripristino automatico del dump in uno
   schema temporaneo per verificare che sia davvero caricabile, poi
   cancellazione. È la differenza fra avere backup e avere **ripristini**.

### 2.5 Wizard di ripristino

Selezione della sorgente con manifest leggibile (data, versione, conteggi),
scelta dei componenti, **anteprima delle modifiche in simulazione** prima di
scrivere, **dump di sicurezza dello stato attuale** prima di sovrascrivere.

Regole:
- Backup **più recente** della versione installata → **rifiutato** con messaggio
  chiaro.
- Più vecchio → migrazioni applicate automaticamente.
- Ripristino delle sole mappe → non tocca il database, utilizzabile a caldo.

Su un server nuovo e vuoto, il ripristino è offerto **come alternativa al wizard
di primo avvio**: «Nuova installazione» oppure «Ripristina da backup».

---

## 3. 2FA — TOTP

**RFC 6238**, standard aperto: nessun costo, nessuna dipendenza da Google o
altri, **funziona offline**. Compatibile con Google Authenticator, Aegis,
FreeOTP, 1Password, Bitwarden, Authy — sono tutti lo stesso algoritmo.

- Provisioning con URI `otpauth://` mostrato come QR code.
- **Segreto cifrato a riposo** con una chiave derivata dal segreto del server —
  qui `SettingsRepo::get_or_create_secret` trova finalmente il suo uso di
  produzione (in Fase 0 era soddisfatto a vuoto: i token opachi non richiedono
  chiave di firma).
- Finestra di tolleranza ±1 intervallo.
- **Protezione contro il riuso dello stesso codice.**
- **10 codici di recupero** monouso salvati come hash.

**WebAuthn/passkey** come evoluzione: anch'esso standard aperto e gratuito, ed è
tecnicamente superiore perché resistente al phishing. Da valutare dopo, non
insieme.

---

## 4. Sincronizzazione incrementale per il mobile

È il pezzo su cui si regge un'app decente: il telefono tiene un database locale
e chiede solo cosa è cambiato.

```
GET /api/v1/sync/delta?cursor=88421
→ { cursor: 91055,
    upserted: [ …asset… ],
    deleted:  [ "018f…", "018f…" ],
    has_more: true }
```

Poggia sul `change_log` **alimentato dalla Fase 1a**: attivarlo dopo avrebbe
significato che tutto ciò che è successo prima è invisibile alla
sincronizzazione.

### 4.1 Il dettaglio che va preso bene

Una transazione con `seq` più basso può committare **dopo** una con `seq` più
alto. Un client che legge fino all'ultimo `seq` visto si perderebbe le righe
committate nel frattempo.

**Il cursore restituito va arretrato al limite delle transazioni certamente
concluse** (`pg_snapshot_xmin`), non all'ultimo `seq` osservato.

Il test che lo dimostra apre **due transazioni sovrapposte**, le committa in
ordine inverso, e verifica che il client non perda righe. Senza quel test, il
difetto si manifesta solo in produzione con un client mobile che perde foto in
modo intermittente.

### 4.2 Idempotenza

Header **`Idempotency-Key`** su tutte le mutazioni. Un'app mobile ritenta di
continuo; senza questo, «aggiungi 300 foto all'album» eseguito due volte fa
danni.

**Chiude tre debiti in uno** (erano stati differiti qui insieme, con questa
ragione): il deadlock `40P01` di due replay concorrenti, il re-login occasionale
su retry di `refresh`, e l'assenza di idempotenza. **Sono un unico problema con
un'unica soluzione**, non tre sviste separate.

---

## 5. Scheduler di manutenzione

Completa i profili energetici della Fase 1b. Nella finestra notturna:

- backlog delle preview e delle transcodifiche;
- **scrubbing d'integrità**: ri-hash a rotazione del 5% della libreria, per
  intercettare bit rot;
- sincronizzazione dei sidecar XMP in sospeso;
- riscansione completa delle librerie su filesystem di rete;
- riscoperta duplicati, backfill geocoding;
- `VACUUM ANALYZE` e **dump del database**;
- pulizia: cestino oltre 30 giorni, cache transcodifiche oltre 90 giorni, job
  `done` oltre 7 giorni, sessioni scadute, upload abbandonati.

---

## 6. Prestazioni — indici mancanti e la cache mai costruita

Verificato sul codice attuale, non nello spec: tre lacune concrete, trovate
mentre si rispondeva alla domanda "tutte le query sono ottimizzate?" con
un'analisi reale invece che a sensazione.

### 6.1 Ricerca per fotocamera/obiettivo: sequenziale, non indicizzata

`asset_exif.camera_model` e `.lens` si cercano con `ILIKE '%...%'` (jolly
davanti), ma li copre solo un indice btree parziale (`asset_exif_camera_idx`)
— che **non può** servire un `ILIKE` con jolly davanti, a differenza di un
indice trigram. Il nome file ha già la soluzione corretta
(`assets_filename_trgm`, GIN trigram): va estesa a camera e obiettivo.

```sql
CREATE INDEX asset_exif_camera_trgm ON asset_exif USING gin (camera_model gin_trgm_ops);
CREATE INDEX asset_exif_lens_trgm ON asset_exif USING gin (lens gin_trgm_ops);
```

Oggi, su una libreria da 200.000 foto, cercare per fotocamera è una scansione
sequenziale — esattamente il tipo di query che il test di scala (§ sotto)
dovrebbe misurare e non fa ancora.

### 6.2 La cache in-process decisa e mai costruita

Lo spec madre del progetto dice esplicitamente: *"Cache in-process (`moka`),
niente Redis"* (`specs/2026-08-13-keeppix-design.md`). Non è mai stata
implementata: nessuna dipendenza `moka` in nessun `Cargo.toml`. Oggi
`VisibilityScope::resolve` e le impostazioni si rileggono dal database **a
ogni richiesta**, senza eccezioni.

Non è un difetto nato per distrazione: è una decisione già presa e mai
portata a termine. Con la sincronizzazione incrementale del mobile (§4) che
interroga spesso, il costo smette di essere teorico.

Da costruire qui: `moka` per permessi effettivi e impostazioni, con
**invalidazione esplicita** quando un permesso o un'impostazione cambia — mai
una cache che può restare indietro su chi può vedere cosa. Una cache di
permessi scaduta è un difetto di sicurezza, non solo di prestazioni: va
trattata con lo stesso rigore.

### 6.3 Indici mancanti su chiavi esterne

Postgres non indicizza automaticamente le colonne di chiave esterna. La
maggior parte delle mancanti sono tabelle di audit/admin a basso traffico —
accettabile così. Due sono più vicine a percorsi frequentati e vale la pena
chiuderle qui:

```sql
CREATE INDEX stacks_primary_asset_idx ON stacks (primary_asset_id);
CREATE INDEX album_assets_added_by_idx ON album_assets (added_by);
```

Va anche rivisto il filtro `status <> 'trashed'` in `AssetRepo` — oggi
scavalca l'indice parziale su `status`, che copre solo `discovered`/`error`.

### 6.4 N+1 nella creazione dei percorsi cartella

`FolderRepo::ensure_path` scorre i segmenti del percorso e per ciascuno fa
un `ensure_child_on` — due andate e ritorno (un upsert più una rilettura)
per segmento, non un'unica query. Confinato in una singola transazione e al
percorso di scrittura dell'ingest/scansione — non tocca timeline o ricerca,
che restano i percorsi di lettura frequentati. Da valutare qui se vale la
riscrittura a una sola query (es. `INSERT ... ON CONFLICT` su tutti i
segmenti insieme) o se il costo reale, misurato, non lo giustifica: un
import è un'operazione rara rispetto a una richiesta di timeline, e non è
detto che l'ottimizzazione paghi il rischio di riscrivere una funzione che
oggi è corretta.

### 6.5 Cosa invece è già corretto, verificato non assunto

- **Un solo pool di connessioni, un solo processo** (`sqlx::PgPool`, 10
  connessioni di default, configurabile) — niente PgBouncer: risolverebbe un
  problema di più processi indipendenti che qui non esiste.
- Timeline, ricerca, modifica in blocco: budget **misurati** su 200.000 righe
  sintetiche con `EXPLAIN ANALYZE` reale (`scale_200k.rs`), non assunti.
- Cache dei derivati e della risoluzione piena, con limite ed espulsione LRU:
  reale e già funzionante.

---

## 7. API pubblica e client generati

- **OpenAPI 3.1 generato dal codice** con `utoipa`: gli handler *sono* la
  specifica.
- Da `/api/openapi.json` si generano i client **Kotlin, Swift, Dart e
  TypeScript**.
- Test in CI che fallisce sui cambiamenti incompatibili.
- **Contratto congelato**: solo aggiunte entro `/api/v1`. Una rottura genera
  `/api/v2`, e la v1 resta viva almeno 12 mesi con header `Deprecation` e
  `Sunset`.

Debiti della Fase 0 da saldare qui: i tre schemi byte-identici
(`LoginResponse`, `MeResponse`, `SetupResponse`), `info.version` che è la
versione del crate invece che dell'API, i rustdoc `# Errors` usati come
`summary`.

---

## 8. PWA

- Installabile, con **Share Target** (Fase 5).
- **Service worker**: shell dell'app e miniature già viste navigabili offline.
- Stati offline progettati, non improvvisati.

**Nota onesta sulle notifiche push**: quando l'app mobile sarà in background,
il WebSocket non funziona — il sistema operativo sospende le connessioni.
Servirebbero FCM/APNs, che richiedono servizi Google/Apple. Fuori dagli
obiettivi dichiarati del progetto; se un giorno servissero, l'API è pronta
perché gli eventi sono già entità serializzate, non messaggi ad hoc.

---

## 9. Debiti della Fase 0 assegnati a questa fase

| Voce | Cosa fare |
|---|---|
| `Password` non azzera il buffer in `Drop`, deriva `Clone` | Va fatto con `zeroize` **su tutta la catena** (corpo JSON, buffer axum, allocazione serde), non solo sull'ultimo anello: azzerare solo quello dà un falso senso di completezza |
| `index.html` ha `lang="en"` hardcoded | Con le impostazioni utente |
| `users.locale` e `UserView.locale` arrivano al frontend e non sono usati | La lingua vive in `localStorage`, non nel profilo come dice lo spec §10.10. Da riconciliare |
| `POST /auth/refresh` non è chiamato da nessun client | Tutta la macchina di rotazione e rilevamento riuso non è collaudata sul campo, benché coperta dai test. Il client mobile la userà davvero |
| ICU MessageFormat (deroga registrata I4) | **Riaprire solo** alla prima lingua con più di due categorie plurali (russo, polacco, arabo). Allora la scelta giusta è un compilatore ICU a **build time** (`@intlify/unplugin-vue-i18n`), non a runtime |

---

## 10. Riconoscimento volti e ricerca semantica: non più fuori scope

Questa sezione diceva, alla prima stesura, che `faces`/`people`/`asset_embeddings`
erano schema predisposto ma "fuori dagli obiettivi dichiarati". Non è più
vero: sono ora la **Fase 7** (scene, tag, ricerca semantica CLIP + pgvector)
e la **Fase 8** (volti), entrambe con spec proprie, pianificate dopo la 6 nel
grafo delle dipendenze — non dentro la 6, e non più "forse mai". Le tabelle
che questa sezione menzionava vanno verificate contro lo schema reale delle
Fasi 7/8 quando si scrive il piano di questa fase: potrebbero non
corrispondere più esattamente a quanto deciso lì.
