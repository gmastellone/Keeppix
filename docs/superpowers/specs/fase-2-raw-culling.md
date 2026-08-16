# Fase 2 — RAW, metadati e culling

**Stato:** in esecuzione sul branch `fase-2`
**Dipende da:** Fase 1 completa (1a + 1b + 1c)
**Chiusa quando:** una sessione di culling su 800 scatti si completa da tastiera
senza attese percepibili, e l'editing batch dei metadati su 5.000 RAW è
istantaneo e reversibile

---

## 1. Il caso d'uso reale

11.000 RAW da rivedere, votare, correggere nei metadati, e infine cancellare
per la maggior parte. I RAW sono **effimeri**: si processano e si eliminano.
Questo cambia il design — la pipeline RAW è un **flusso di culling**, non un
archivio permanente.

---

## 2. RAW — la buona notizia

**ARW (Sony), NEF (Nikon), CR2/CR3 (Canon) e DNG contengono già un JPEG
full-size incorporato**, scritto dalla fotocamera. Estrarlo costa tipicamente
**1–6 ms** su file già in cache (misurato in Fase 2 sui fixture CC0 di
raw.pixls.us; la stima precedente 30–80 ms includeva probabilmente I/O a
freddo di RAW full-size) e **zero demosaic**.

Per il caso d'uso — *vederli a risoluzione decente per fare review e
cancellare* — questo copre tutto.

### 2.1 La pipeline

```
1. C'è un sidecar .xmp?    → leggi rating, tag, GPS, descrizione già presenti
2. Estrai la preview JPEG incorporata
     ARW  → JPEG full-size (spesso 6000 px)
     NEF  → JPEG full-size sulla maggior parte dei corpi
     CR3  → box PRVW, ~1620 px
     CR2  → IFD preview full-size
     DNG  → preview definita dallo standard, dimensione variabile
     ORF  → preview, dimensione variabile
     RAF  → JPEG full-size
3. Preview trovata e ≥1440 px?  → è la preview. Fine. (~1–6 ms in-memory)
4. Preview piccola o assente?   → demosaic con libraw, half-size,
                                   bilanciamento del bianco della fotocamera
                                   (~1,5-4 s su ARM)
5. Fallita anche quella?        → status='error', compare in Problemi
```

Il passo 3 copre il **90-95%** dei file Sony, Nikon e Canon (stima ancora
da verificare su librerie reali; sui 5 fixture di Fase 2 è stato 5/5).

**Misura Fase 2 (ledger):** sui fixture CC0, 100% hanno raggiunto una preview
embedded utilizzabile senza demosaic; tempi 1.1–5.4 ms (release, file in
cache). La percentuale per corpo macchina su archivi reali resta da misurare
in produzione.

### 2.2 Cosa NON si fa

- **Niente darktable nell'immagine.** Aggiunge ~800 MB e su ARM è lentissimo.
- **Il demosaic completo non è il default.** Resta disponibile come azione
  esplicita «Genera anteprima alta qualità» su una singola foto, e come opzione
  di libreria per chi non si accontenta della preview della fotocamera.
- **Un file RAW non si riscrive mai.** Vedi §3.

### 2.3 Formati

ARW, ARQ · NEF, NRW · CR2, CR3 · DNG · ORF · RAF. Il rilevamento è per magic
number, non per estensione.

---

## 3. Metadati — originali immutabili, modifiche accanto

### 3.1 Perché non si riscrive il file

**La ragione forte, che da sola decide:** ARW, NEF e soprattutto CR3 sono
contenitori proprietari poco documentati. Le librerie capaci di riscriverli in
sicurezza sono poche e fragili. Un fallimento a metà scrittura su un `.CR3` è
un file **irrecuperabile**, e non esiste il negativo.

Non è paranoia: Lightroom, Capture One e darktable si rifiutano tutti di
scrivere dentro un RAW e usano sidecar XMP, esattamente per questo.

Quattro ragioni pratiche che valgono anche per i JPEG:

1. **Velocità.** «Seleziono 5.000 foto e metto la posizione» come UPDATE in DB:
   ~200 ms. Come riscrittura di 5.000 file su disco USB: 40-90 minuti, con
   5.000 finestre di corruzione e il watcher che si sveglia 5.000 volte.
2. **Reversibilità.** L'errore più comune del mondo è correggere in blocco il
   fuso orario sbagliando di un'ora. Con l'override si fa «ripristina
   originale». Con la sovrascrittura la data di scatto originale **non esiste
   più da nessuna parte**.
3. **Backup.** Riscrivere i file cambia `mtime` e contenuto: al backup
   successivo rsync/Borg/Backblaze ricaricano **1 TB**. Con i sidecar, qualche
   megabyte.
4. **Disponibilità.** NAS spento, mount read-only, disco pieno: l'app continua
   a funzionare e la modifica non va persa; il job di scrittura riparte quando
   il disco torna.

### 3.2 Lo schema

```sql
asset_overrides (
    asset_id      uuid PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    title         text,
    description   text,
    taken_at      timestamptz,
    location      geography(Point, 4326),
    place_id      bigint,
    orientation   smallint,
    updated_by    uuid REFERENCES users(id),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    xmp_written_at timestamptz
);
```

Il valore mostrato è **`COALESCE(override, exif)`**. `asset_exif` non si
riscrive mai.

`xmp_written_at` è ciò che permette al job di sincronizzazione di sapere cosa
resta da riversare: `WHERE updated_at > COALESCE(xmp_written_at, '-infinity')`.

### 3.3 Propagazione sui file — «DB prima, file poi»

Non è «mai scrivere»:

| Tipo file | Default | Opzioni |
|---|---|---|
| RAW | sidecar `.xmp` accanto al file | **nessuna scrittura nel RAW, mai** |
| JPEG · HEIC · PNG | sidecar `.xmp` | ☑ «scrivi anche negli EXIF del file», per libreria o su richiesta |
| Video | sidecar `.xmp` | tag QuickTime opzionali |

La scrittura nel file, quando attivata, è: **temporaneo nella stessa cartella →
`fsync` → rilettura e verifica dei metadati → `rename()` atomico**. Se qualcosa
va storto, l'originale non è mai stato toccato.

Azione esplicita **«Sincronizza metadati sui file»**, da lanciare prima di
esportare o consegnare una cartella a un cliente: riversa tutti gli override in
sospeso, con barra di avanzamento e report.

### 3.4 Il sidecar XMP

Formato: `IMG_1234.ARW.xmp` accanto al file. È lo standard che darktable,
Lightroom, Capture One e Bridge leggono.

Campi mappati:

| Keeppix | XMP |
|---|---|
| rating (owner) | `xmp:Rating` |
| description | `dc:description` |
| title | `dc:title` |
| tag | `dc:subject` |
| GPS | `exif:GPSLatitude` / `exif:GPSLongitude` |
| taken_at | `exif:DateTimeOriginal` |
| pick/reject | `xmp:Label` (convenzione darktable) |

**Lettura**: se un `.xmp` esiste già all'indicizzazione, i suoi valori vanno
negli override attribuiti al **proprietario della libreria**. Un archivio che
arriva da Lightroom non perde il lavoro fatto.

**Scrittura**: il sidecar si scrive da zero, non si fonde. Se contiene campi
che Keeppix non gestisce, vanno **preservati** — leggere, modificare i propri
campi, riscrivere. Perdere metadati altrui è peggio che non scrivere.

---

## 4. Culling

### 4.1 Flag per utente

```sql
asset_flags (
    asset_id   uuid,
    user_id    uuid,
    rating     smallint CHECK (rating BETWEEN 0 AND 5),
    pick       text CHECK (pick IN ('none','pick','reject')),
    color_label text,
    PRIMARY KEY (asset_id, user_id)
);
```

**Per utente**, non per asset: nel culling professionale la selezione di
ciascuno è la propria. Il tuo 5 stelle non è il 5 stelle di tua moglie.

Tre conseguenze da gestire, tutte già decise:

1. **Chi finisce nell'XMP?** `xmp:Rating` è un valore singolo. Regola: vince il
   rating del **proprietario della libreria**. Gli altri restano solo in
   Keeppix. In UI, sotto il rating di un non-proprietario compare «non
   sincronizzato sul file».
2. **Import da Lightroom**: un `xmp:Rating` esistente va al proprietario.
3. **Confusione visiva**: selettore esplicito in barra filtri, **«Selezione di:
   [Io ▾]»**, con l'elenco di chi ha accesso.

Per il culling a quattro mani con un cliente (Fase 3), un album condiviso può
attivare la **modalità selezione collaborativa**: i pick vengono uniti e
mostrati con l'avatar di chi li ha messi — «scelte dal cliente: 47».

### 4.2 La modalità

Modalità a sé, **un unico punto d'ingresso**: il pulsante *Culling* nella barra
di una cartella o di una selezione. Non tre scorciatoie sparse.

```
┌────────────────────────────────────────────────────┐
│  Culling — Matrimonio Rossi         142/856   ✕    │
├────────────────────────────────────────────────────┤
│              [ foto grande, zoom 1:1 ]      ⌕ 100% │
├────────────────────────────────────────────────────┤
│ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ │  filmstrip
├────────────────────────────────────────────────────┤
│ ⭐⭐⭐⭐○   ✓ scelte 118   ✕ scarti 47   ○ da vedere │
│ [Tutte] [Da vedere] [Scelte] [Scarti]              │
└────────────────────────────────────────────────────┘
```

- **Tastiera-centrica con avanzamento automatico**: premi `3`, passa alla
  successiva. Si smaltiscono 800 scatti in venti minuti.
- **Zoom 1:1 per il fuoco.** Qui serve l'originale, non la preview: si
  precarica **il ritaglio centrale a piena risoluzione delle 3 foto
  successive**, così il controllo di messa a fuoco è istantaneo. È l'unico
  posto in cui si legge l'originale in modo aggressivo.
- **Confronto affiancato** di 2-4 scatti (`c`), per gli scatti quasi identici.
- Filtro sugli scarti + **«Elimina i 47 scarti»**.

Scorciatoie: `1-5` rating · `p` pick · `x` reject · `←→` naviga · `z` zoom 1:1
· `c` confronto · `Canc` elimina.

---

## 5. Stack RAW+JPEG

Chi scatta in RAW+JPEG ha due file per ogni scatto e non vuole vedere due
miniature.

```sql
stacks (id uuid PRIMARY KEY, primary_asset_id uuid);
-- assets.stack_id → stacks.id
```

**Raggruppamento automatico**: stesso nome base nella stessa cartella
(`DSC_0042.ARW` + `DSC_0042.JPG`), oppure scatti entro 2 secondi con stesso
corpo macchina e stesso numero di scatto.

Nella griglia si vede **un elemento con badge «RAW+JPEG»**. Cancellando lo
stack si sceglie cosa eliminare: solo il JPEG, solo il RAW, entrambi.

L'asset primario è il RAW se presente, perché è quello con più informazione.

---

## 6. Cancellazione — «chiedi ogni volta»

Decisione presa: nessun comportamento implicito.

```
Elimina 47 foto
────────────────────────────────────────
○  Rimuovi solo dall'indice
   I file restano sul disco. Keeppix li reindicizzerà
   alla prossima scansione.

●  Sposta nel cestino di Keeppix
   Recuperabili per 30 giorni. I file vengono spostati
   in .keeppix-trash/ dentro la stessa libreria.

○  Elimina dal disco adesso
   ⚠ Irreversibile.
────────────────────────────────────────
```

Il cestino è una cartella `.keeppix-trash/` **dentro la stessa libreria**: è un
`rename()` istantaneo, non una copia di 25 GB attraverso i filesystem.

```sql
trash_entries (
    asset_id      uuid,
    deleted_by    uuid,
    deleted_at    timestamptz,
    original_path text,
    disk_action   text CHECK (disk_action IN ('kept','moved_to_trash','purged'))
);
```

Pulizia del cestino oltre i 30 giorni: job di manutenzione notturno.

**Da WebDAV** (Fase 5) un `DELETE` va **sempre** nel cestino: il protocollo non
ha modo di fare domande, e trascinare una cartella nel cestino del Finder per
sbaglio non deve essere irreversibile.

---

## 7. Duplicati

Pagina che elenca i gruppi con `content_hash` uguale e `count > 1`, con lo
spazio recuperabile e l'azione «tieni questo, elimina gli altri».

Deduplica **esatta per hash**, non per similarità: niente ML in questa fase.
Immich usa anche la similarità CLIP, che è utile ma è un'altra cosa e va con il
resto dell'AI (fuori dalla v1).

I derivati sono già indicizzati per hash, quindi cinque copie occupano un solo
thumbnail: il guadagno del deduplicare è sugli originali.

---

## 8. Editing batch dei metadati

Il caso d'uso: «seleziono 5.000 foto e metto descrizione o posizione a tutte».

- **`INSERT … ON CONFLICT UPDATE` su 5.000 righe**: istantaneo.
- Nessun file toccato in quel momento.
- Il job di scrittura sidecar è **asincrono, a priorità 3, ritentabile**.
- **Annullabile**: l'operazione batch registra cosa ha cambiato, e «annulla»
  ripristina i valori precedenti finché il sidecar non è stato scritto.

Campi editabili in batch: titolo, descrizione, tag, posizione, data di scatto
(assoluta o con scostamento «sposta di N ore»), rating, pick.

Lo **scostamento di N ore** è il rimedio classico quando torni da un viaggio e
ti accorgi di non aver cambiato l'orologio della macchina. Va offerto
esplicitamente, non lasciato all'utente da calcolare.

---

## 9. Cosa NON è in Fase 2

Condivisione e permessi: Fase 3. Mappa e geocoding: Fase 4 — in Fase 2 si può
già scrivere `location` negli override, ma l'interfaccia per sceglierla arriva
dopo. Import GPX: previsto ma non implementato (vedi `location_source = 'gpx'`,
già nell'enum dalla Fase 1a).
