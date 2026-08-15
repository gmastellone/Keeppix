# Fase 1c — Timeline, API e frontend

**Stato:** ✅ **chiusa sul branch `fase-1`** — vedi
[`../plans/2026-08-14-keeppix-fase-1c-STATO.md`](../plans/2026-08-14-keeppix-fase-1c-STATO.md)
**Dipende da:** Fase 1a (modello dati), Fase 1b (pipeline che riempie i dati)
**Chiusa quando:** si naviga il TB reale dal browser, la timeline scorre fluida
su Raspberry, la ricerca risponde e le miniature dei bucket visibili vengono
generate per prime

---

## 1. La timeline — il problema e la soluzione

**Il problema.** La scrollbar deve conoscere l'altezza totale *prima* di
caricare le foto, altrimenti salta mentre scorri. Ma i conteggi dipendono da
chi guarda, e su 200.000 foto non si può caricare l'elenco degli id.

**La soluzione: bucket per mese, indipendenti dall'utente.**

```sql
folder_month_counts (folder_id, month, asset_count)  -- PK (folder_id, month)
```

Aggiornata da trigger su `assets`. Il conteggio è **per cartella e mese**,
quindi non dipende dall'utente; la visibilità si applica sommando solo il
sottoalbero autorizzato:

```sql
SELECT month, sum(asset_count) AS n
  FROM folder_month_counts fmc
  JOIN folders f ON f.id = fmc.folder_id
 WHERE f.path <@ ANY($allowed_prefixes)
 GROUP BY month
 ORDER BY month DESC;
```

Un utente con 3.000 cartelle e 15 anni di foto somma ~30.000 righe di una
tabella minuscola: **3-8 ms**.

Il client riceve `[{month: "2024-07", count: 412}, …]`, calcola l'altezza
esatta e richiede i bucket solo quando entrano nel viewport.

**Nessuna tabella di visibilità materializzata per utente.** È il punto in cui
si evita la trappola: cambiare un permesso è un `INSERT` e ha effetto
immediato, non innesca la ricostruzione di niente.

### 1.1 Paginazione dentro il bucket

Keyset, mai `OFFSET`:

```sql
SELECT … FROM assets a JOIN folders f ON f.id = a.folder_id
 WHERE f.path <@ ANY($allowed)
   AND a.status = 'indexed'
   AND a.taken_at_utc < $cursor_time
    OR (a.taken_at_utc = $cursor_time AND a.id < $cursor_id)
 ORDER BY a.taken_at_utc DESC, a.id DESC
 LIMIT 200;
```

L'indice `assets_timeline_idx (taken_at_utc DESC, id DESC) WHERE status =
'indexed'` è costruito per questa query. `OFFSET 100000` degraderebbe
linearmente; il keyset no.

### 1.2 Il trigger dei conteggi

Deve gestire tutti e tre i casi, e il terzo è quello che si dimentica:

- `INSERT` → incrementa il bucket.
- `DELETE` → decrementa.
- `UPDATE` che cambia `taken_at_utc` o `folder_id` → decrementa il vecchio
  bucket e incrementa il nuovo. Succede sempre: la fase 2 della pipeline scrive
  `taken_at_utc` su un asset inserito con `NULL` dalla fase 1.

Un test deve verificare che dopo una scansione completa la somma dei conteggi
coincida con `count(*)` sugli asset. Se diverge, il trigger ha un buco.

---

## 2. Ricerca

### 2.1 Sintassi

Una sola barra che accetta testo libero **e** query booleane. Il parser è
quello di urocissa (Chevrotain, TypeScript), da portare nel frontend.

```
grecia and (tramonto or mare) not rating:0
camera:"Sony α7 IV" and iso:>3200
type:video and 2024
```

Chi non conosce la sintassi usa i chip; chi la conosce scrive.

**Il parser produce un AST**, e l'AST genera una query parametrizzata. La
stringa dell'utente **non tocca mai l'SQL**. Questo non è negoziabile.

### 2.2 Campi filtrabili

| Campo | Sorgente | Note |
|---|---|---|
| testo libero | filename, descrizione, nome luogo | `pg_trgm` |
| `type:` | `assets.kind` | image · raw · video |
| `camera:` | `asset_exif.camera_model` | |
| `lens:` | `asset_exif.lens` | |
| `iso:` | `asset_exif.iso` | supporta `>`, `<`, range |
| `rating:` | `asset_flags.rating` | per utente (Fase 2) |
| anno / periodo | `taken_at_utc` | |
| `folder:` | sottoalbero `ltree` | |
| `has:gps` | `location IS NOT NULL` | |

### 2.3 Ricerche salvate

Ogni ricerca è **salvabile** e compare nella sidebar come raccolta viva. È il
sostituto degli «album intelligenti»: un solo concetto invece di due.

```sql
saved_searches (id, owner_id, name, query_text, created_at)
```

---

## 3. Endpoint

Tutti sotto `/api/v1`. Contratto congelato: solo aggiunte.

| Metodo | Percorso | Note |
|---|---|---|
| `GET` | `/timeline/buckets` | `[{month, count}]`, la base della scrollbar |
| `GET` | `/timeline?bucket=2024-07&cursor=…` | asset + flag + luogo in **una** risposta |
| `GET` | `/folders/tree` | albero completo, compatto |
| `GET` | `/folders/{id}/children` | figli diretti + asset |
| `POST` | `/search` | query strutturata. **POST, non GET**: la query non finisce nei log del reverse proxy né nella cronologia |
| `GET` | `/search/suggest?q=` | autocomplete |
| `POST` | `/viewport` | hint dei bucket visibili → promozione job a priorità 2 |
| `GET` | `/media/thumb/{hash}` | `Cache-Control: immutable` |
| `GET` | `/media/preview/{hash}` | `Cache-Control: immutable` |
| `GET` | `/media/original/{id}` | range request |
| `GET` | `/media/video/{id}/hls` | playlist HLS per i transcodificati |
| `GET` | `/problems` | file corrotti, librerie offline, job falliti |
| `GET` | `/duplicates` | gruppi per `content_hash`, spazio recuperabile |

### 3.1 Il fallback SPA deve escludere `/media/*`

Segnalato nella review finale della Fase 0 e **da fare subito in 1c**:
`embed.rs` esclude dal fallback solo i percorsi che iniziano per `api/`. Una
miniatura mancante restituirebbe `index.html` con `200` a un tag `<img>` — un
sintomo illeggibile. Vanno esclusi anche `media/` e `dav/`.

### 3.2 Prestazioni: dove si gioca davvero

In ordine di impatto, non di eleganza:

1. **URL dei derivati con l'hash del contenuto** →
   `Cache-Control: public, max-age=31536000, immutable`. Il browser **non li
   richiede mai più**. Alla seconda visita di una griglia il server riceve zero
   richieste. Nessuna cache server-side compete con una richiesta che non parte.
2. **`ETag` + `304`** sugli endpoint JSON: un bucket invariato risponde 30 byte
   invece di 80 KB.
3. **HTTP/2 obbligatorio.** Una griglia carica 200 miniature: su HTTP/1.1 sono
   6 connessioni in coda.
4. **Zero-copy** nello streaming dei file.
5. **Brotli sul JSON**, nessuna compressione sulle immagini (già compresse).
6. **Cache in-process** (`moka`, ~60 MB): scope di visibilità, albero cartelle,
   conteggi mensili, sessioni, metadati recenti. **Non Redis**: su nodo singolo
   la cache in-process è più veloce, senza serializzazione né hop di rete.

### 3.3 La cache dell'autenticazione

`Auth::from_request_parts` fa una query per ogni richiesta autenticata.
Irrilevante oggi, non in una griglia da centinaia di richieste.

La buona notizia: **è l'unico punto da cui passa l'autenticazione**, quindi la
cache si inserisce lì e da nessun'altra parte. Va progettata con
l'invalidazione esplicita in `revoke`/`rotate` — una sessione revocata non deve
sopravvivere nella cache — oppure con TTL molto corto (30 s) accettando quella
finestra.

---

## 4. WebSocket

### 4.1 La regola che tiene in piedi tutto

> **Il WebSocket è un canale di notifica, non la fonte di verità.** Nessun dato
> esiste solo perché è passato di lì. Alla riconnessione il client chiama
> sempre `/sync/delta?cursor=`. Se il socket perde messaggi, l'applicazione
> resta corretta.

Questo trasforma i bug da «dati mancanti misteriosi» a «aggiornamento con
qualche secondo di ritardo».

### 4.2 Autenticazione — l'errore numero uno

Il browser non può impostare header su una connessione WebSocket, quindi quasi
tutti mettono il token in query string, dove finisce nei log di nginx e nella
cronologia. **Noi no:**

```
POST /api/v1/ws/ticket   → { ticket: "…", expires_in: 30 }   monouso
GET  /api/v1/ws          Sec-WebSocket-Protocol: keeppix.v1, ticket.<ticket>
```

Ticket monouso da 30 secondi, consumato all'handshake. Il client mobile usa
direttamente l'header `Authorization`, che nativamente può.

### 4.3 Verifica dell'`Origin` — l'errore numero due

`SameSite` sui cookie **non si applica** alle connessioni WebSocket. Senza
controllo dell'`Origin`, qualsiasi sito aperto nel browser può aprire un socket
autenticato verso Keeppix. Origin validato contro l'allowlist all'handshake,
connessione rifiutata altrimenti.

### 4.4 Backpressure — l'errore numero tre, e il più costoso

Durante la scansione iniziale si indicizzano 200.000 asset. Se un telefono in
3G è lento, la coda di invio verso di lui cresce senza limite e mangia la RAM
del server.

> Coda per connessione limitata (default 256 messaggi). Al superamento, la coda
> viene **svuotata e sostituita da un singolo messaggio `resync`**: il client
> rifà una delta sync e riparte allineato.

Il server non può mai gonfiarsi per colpa di un client lento.

### 4.5 Coalescing

Non 200.000 messaggi durante lo scan: **un messaggio di avanzamento aggregato
ogni 250 ms**, e le notifiche di nuovi asset raggruppate in batch. Il volume
del canale è indipendente dal volume del lavoro.

### 4.6 Protocollo

Sottoprotocollo `keeppix.v1`, negoziato nell'handshake. Quando evolverà,
un'app vecchia continuerà a parlare v1.

Buste tipizzate:

```json
{ "v": 1, "type": "scan.progress", "payload": { … } }
```

| Tipo | Direzione | Payload |
|---|---|---|
| `scan.progress` | S→C | `{library_id, phase, done, total, eta_seconds}` |
| `assets.upserted` | S→C | `{ids: [...], count}` (batch) |
| `assets.deleted` | S→C | `{ids: [...]}` |
| `job.failed` | S→C | `{kind, asset_id, error}` |
| `library.status` | S→C | `{library_id, status}` |
| `permissions.changed` | S→C | `{}` — il client rifà lo scope |
| `resync` | S→C | `{}` — la coda è traboccata |
| `subscribe` | C→S | `{topics: ["timeline", "jobs", "folder:42"]}` |
| `viewport` | C→S | `{buckets: ["2024-07", "2024-06"]}` |

**Ogni evento passa dal `visibility_scope`** prima di uscire — lo stesso delle
query REST, non una copia. Al cambio permessi si emette `permissions.changed` e
le sottoscrizioni non più valide cadono.

### 4.7 Il resto

- **Heartbeat**: ping ogni 30 s, peer morto dopo due pong mancati.
- **Riconnessione**: backoff esponenziale **con jitter**. Senza jitter, dopo un
  riavvio del server tutti i client tornano nello stesso millisecondo e lo
  riabbattono.
- **`permessage-deflate` disattivata**: ~300 KB di RAM per connessione con i
  parametri di default, inutile su messaggi da 200 byte.
- **Fallback a polling** `/sync/delta` ogni 15 s dopo tre handshake falliti.
  Elimina un'intera categoria di segnalazioni da reti aziendali.
- **Limiti**: 8 connessioni per utente, messaggio in ingresso ≤64 KB, rate
  limit sui messaggi in entrata, chiusura garbata con codice 1001 al riavvio.

**Configurazione nginx necessaria** (va in `docs/DEPLOY.md`):

```nginx
location /api/v1/ws {
    proxy_pass http://keeppix;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_read_timeout 3600s;
}
```

---

## 5. Frontend

### 5.1 Il principio

**Superficie pulita, potenza a un gesto di distanza.** La sintesi fra Immich e
Google Photos non è una media, è divulgazione progressiva:

- la timeline non ha barre degli strumenti finché non selezioni;
- i filtri sono chip sotto la ricerca, non un pannello permanente;
- le funzioni professionali vivono in **modalità** che si entrano e si escono.

**Regola dura contro la sovrapposizione:** il visualizzatore normale non
diventa mai una modalità; la modalità culling (Fase 2) non diventa mai il
default.

### 5.2 Navigazione

**Mobile** — barra inferiore a 4 voci: `Foto · Cerca · Album · Libreria`.
Libreria raccoglie Cartelle (prima voce, in evidenza), Mappa, Preferiti,
Condivisi, Cestino, Problemi. Pressione lunga sulla tab Libreria → Cartelle.

Il motivo: **il lavoro sulle cartelle è lavoro da desktop**; il mobile è
consumo. Dedicare il 25% della barra alle cartelle ottimizzerebbe per l'uso
raro. In Impostazioni si può scambiare Album e Cartelle.

**Desktop** — sidebar sinistra con albero cartelle sempre visibile.

### 5.3 Griglia

Griglia **giustificata**: righe di altezza costante, larghezze proporzionali
all'aspect ratio. I panorami restano panorami — niente ritagli quadrati.

- Header appiccicosi per giorno e mese con conteggio.
- **Scrubber laterale** con etichette di anno e mese: si trascina e si è nel
  2019 in mezzo secondo. È il componente di urocissa da portare da Vuetify a
  Tailwind — la logica è TypeScript quasi puro, va isolata e testata a parte.
- **Densità regolabile** da 2 a 12 colonne (pinch su mobile, `+`/`−` su
  desktop), salvata per dispositivo.
- Placeholder **thumbhash**: mai rettangoli grigi, mai layout shift.
- Selezione: click, shift+click, rettangolo di trascinamento su desktop;
  pressione lunga e trascinamento su mobile.
- **«Seleziona tutto» istantaneo** su 200.000 foto: seleziona la *query*, non
  gli elementi.
- Chip permanente `[ Tutti | Foto | Video ]`, scelta ricordata.

### 5.4 Visualizzatore

Schermo intero, swipe, pinch-zoom, doppio tap per 1:1, con le **due foto
adiacenti precaricate** — lo swipe non aspetta mai.

Pannello informazioni: dati di scatto, percorso cartella, luogo con mini-mappa,
tag, rating.

Scorciatoie: `←→` naviga · `i` info · `z` zoom 1:1 · `1-5` rating · `f`
preferito · `Canc` elimina · `Spazio` play.

Nel visualizzatore normale entrano **solo due azioni atomiche**: rating e
preferito. Tutto il resto è modalità culling (Fase 2).

### 5.5 Prestazioni frontend

- **Budget bundle iniziale: 150 KB gzip**, verificato in CI. Mappa, culling,
  impostazioni e player video sono chunk separati.
- Virtual scroll di urocissa, `content-visibility: auto`, dimensioni esplicite
  sulle immagini (zero layout shift).
- Il frontend **comunica i bucket visibili**; il backend riordina la coda.
- UI ottimistica su rating, preferiti, album. Nessuno spinner sotto i 200 ms.
- **Service worker**: shell e miniature già viste navigabili offline.

### 5.6 Stati che vanno progettati, non improvvisati

- **Caricamento**: thumbhash, mai spinner sulla griglia.
- **Vuoto**: «Nessuna foto in questa cartella» con l'azione giusta accanto.
- **Errore di rete**: con I5 il backend distingue `503` da `401`, quindi il
  frontend **può** distinguere «riprova» da «sessione scaduta». Va fatto: oggi
  `bootstrap()` propaga l'errore dentro `router.beforeEach` e la pagina resta
  bianca.
- **Pagina Problemi**: file corrotti, librerie offline, job falliti, sidecar non
  scrivibili — tutto in un posto, con l'azione per rimediare.

---

## 6. Cosa NON è in Fase 1c

Rating e pick persistenti, culling, RAW, sidecar XMP: Fase 2. Condivisione,
album condivisi, link pubblici: Fase 3. Mappa: Fase 4 (in 1c la mini-mappa nel
pannello dettagli può essere un segnaposto). Upload: Fase 5.

Gli album **esistono** come entità dalla Fase 3; in 1c la voce di menu può
esserci disabilitata, oppure non esserci.
