# Fase 10 — Superficie API per l'interfaccia

**Stato:** specifica. Nessuna riga è stata implementata.
**Origine:** confronto punto per punto fra `docs/ui/documento-funzionale-ui.md` (10.823 righe,
70 schermate, SP-1…SP-30), il prototipo interattivo `docs/ui/keeppix-mockup.html` navigato dal
vivo, e la superficie API reale al tip di `origin/fase-6`.

---

## 1. Perché questa fase esiste, e perché viene prima della 7

Il documento funzionale si chiude, per ogni schermata, con un paragrafo *"Dati necessari"*
scritto in termini di cose e non di endpoint. Confrontando quei 64 paragrafi con le 81
operazioni realmente esposte emergono tre categorie ben distinte:

1. **Cose che il backend già fa** e che la UI può usare così com'è (la maggioranza).
2. **Cose che sono già specificate nelle Fasi 7, 8 e 9** e che arriveranno con quelle
   (tag, ricerca semantica, volti, culling, rinomina).
3. **Cose che non sono coperte da nessuna fase**, e che sono *trasversali*: non appartengono
   all'IA, ai volti o all'organizzazione, ma al modo in cui **qualunque** schermata parla con
   il server.

Questa fase è la terza categoria. Contiene le due richieste che il documento stesso marca come
prioritarie — *"il punto 1 e il punto 2 sono quelli da verificare per primi: se uno dei due non
fosse realizzabile, cambia il disegno, non l'implementazione"* — e nessuna delle due è oggi
realizzabile.

**Ruling: la Fase 10 precede la 7, la 8 e la 9.** — Perché l'involucro di riuscita parziale (§3)
e la tassonomia degli errori (§7) sono convenzioni che *ogni* endpoint di massa deve rispettare.
Le Fasi 7, 8 e 9 introducono da sole almeno otto nuove operazioni di massa (conferma tag in
blocco, rifiuta tutte, conferma volti, unisci persone, sposta lotto, rinomina con formula,
svuota scartati, riassegna volti). Se la convenzione arriva dopo, quelle otto vanno riscritte.
— *Costo se sbagliato:* si ritarda l'IA di ~2 settimane per pagare un debito che altrimenti si
paga tre volte.

---

## 2. La geometria della timeline (richiesta #1 del documento)

### 2.1 Il problema, nei termini del documento

> *"Serve poter conoscere le proporzioni di tutti gli scatti di una vista senza doverne caricare
> le miniature. Per ogni foto della vista: un identificativo, la larghezza e l'altezza (o
> direttamente il rapporto), e il mese di appartenenza. Nient'altro."* (§66)

Serve perché il layout è **giustificato** (righe ad altezza comune, larghezza proporzionale al
lato lungo) e **virtualizzato**: per sapere quanto è alta la pagina — e quindi dove va il
cursore della barra di scorrimento, e quali righe ricadono nella finestra — bisogna conoscere la
geometria di *tutta* la vista prima di aver disegnato un pixel.

### 2.2 Perché oggi non è possibile

`GET /api/v1/timeline/buckets` dà `{month, count}`: il conteggio sì, le proporzioni no.
`GET /api/v1/timeline` dà `AssetView`, che contiene `width`/`height` ma anche `content_hash`,
`size_bytes`, `kind`, `status`, `taken_at_utc`, `thumbhash`, `location` — ed è paginata a
massimo 200 elementi (`crates/keeppix-api/src/routes/timeline.rs`, `limit.clamp(1, 200)`).

Su una libreria da 214.000 scatti — la scala dichiarata dal prototipo — costruire la geometria
significherebbe **1.070 richieste** che trasportano decine di megabyte per estrarne tre campi.

### 2.3 La forma proposta

```
GET /api/v1/timeline/geometry?library={id}&bbox=…&filter=…
→ 200 application/octet-stream  (oppure JSON, vedi Ruling)
```

Un solo record per scatto, tre campi:

| campo | tipo | byte |
|---|---|---|
| `id` | uuid | 16 |
| `w`, `h` | u16, u16 | 4 |
| `month` | u16 (anni*12+mese) | 2 |

**22 byte per scatto.** Per 214.000 scatti: **4,7 MB** non compressi, ~1,5 MB con gzip — una
sola richiesta, cacheabile con `ETag`.

**Ruling: risposta binaria compatta, non JSON.** — Lo stesso contenuto in JSON
(`{"id":"…36 char…","w":6000,"h":4000,"m":24318}`) pesa ~75 byte per scatto: 16 MB invece di
4,7, e costringe il browser a costruire 214.000 oggetti JavaScript solo per leggerli. Il formato
binario si legge in un `ArrayBuffer` con una sola `DataView`, senza allocazioni per elemento, e
alimenta direttamente i `Float64Array` del calcolo di layout. — *Costo se sbagliato:* il
frontend deve scrivere un lettore binario (~40 righe) invece di `await res.json()`.

**Ruling: gli id restano uuid a 16 byte, non indici densi.** — Un indice denso (0…N) sarebbe più
compatto, ma legherebbe la geometria all'ordinamento esatto della pagina successiva: qualunque
foto aggiunta o cestinata fra le due richieste sfalserebbe tutti gli indici. Con l'uuid la
geometria e le pagine si riconciliano per identità. — *Costo se sbagliato:* 4,7 MB invece di 1,3.

**Ruling: la geometria rispetta gli stessi filtri e la stessa visibilità della timeline.** —
Deve descrivere *la vista corrente*, non la libreria: se l'utente ha una cartella aperta o un
filtro attivo, la geometria è quella. Riusa `VisibilityScope::resolve` e il compilatore di
`SearchNode` già esistenti. — *Costo se sbagliato:* la barra di scorrimento mente ogni volta che
c'è un filtro, che è il caso normale.

### 2.4 Costo lato database

Serve una sola query, senza join e senza ordinamenti costosi:

```sql
SELECT a.id, a.width, a.height, date_trunc('month', a.taken_at_utc)
FROM assets a JOIN folders f ON f.id = a.folder_id
WHERE <visibility> AND a.status <> 'trashed' AND <filtro>
ORDER BY a.taken_at_utc DESC, a.id DESC;
```

**Serve un indice di copertura**, altrimenti è un seq scan su tutta la tabella:

```sql
CREATE INDEX assets_geometry_idx
    ON assets (folder_id, taken_at_utc DESC, id DESC)
    INCLUDE (width, height)
    WHERE status <> 'trashed';
```

Con `INCLUDE` la query si serve **solo dall'indice** (index-only scan), senza toccare la heap:
è la differenza fra ~200 ms e ~2 s su 214.000 righe.

---

## 3. La riuscita parziale (richiesta #2 del documento)

### 3.1 Il problema

> *"Un'operazione di massa non può rispondere 'fatto' o 'non fatto'. Deve rispondere con l'elenco
> di cosa è riuscito e l'elenco di cosa no, foto per foto, e per ciascun fallimento una ragione."*
> (§69)

Oggi `POST /api/v1/flags/batch` risponde `204 No Content`. `POST /api/v1/metadata/batch`
risponde `{batch_id}`. Su un'operazione che tocca 400 file, dove tre sono su una cartella in
sola lettura, l'interfaccia può solo mentire: dire "fatto" nascondendo tre fallimenti, o dire
"errore" facendo rifare tutto.

### 3.2 L'involucro, uguale per ogni operazione di massa

```json
{
  "succeeded": ["01J...a", "01J...b"],
  "failed": [
    { "id": "01J...c", "reason": "permission-denied",
      "detail": "La cartella /volume1/Foto/Urbino non è scrivibile" },
    { "id": "01J...d", "reason": "file-missing" }
  ],
  "batch_id": "01J...z"
}
```

- **Stato HTTP `200`** anche con fallimenti parziali. `207 Multi-Status` sarebbe semanticamente
  più preciso ma è mal gestito da diversi client HTTP e non porta nulla che il corpo non dica.
- `reason` viene dalla **stessa tassonomia** della §7, così il frontend ha un solo `switch`.
- `batch_id` resta, e resta annullabile: `POST /metadata/batch/{batch_id}/undo` esiste già e
  annulla **solo ciò che è riuscito**.

**Ruling: l'operazione non è transazionale sull'intero lotto.** — Il documento chiede
esplicitamente di poter "ritentare solo le rimanenti", che è incompatibile con un rollback
totale: se 397 su 400 riescono, buttarle via per tre fallimenti è il comportamento che la UI
sta cercando di evitare. Ogni elemento è una transazione a sé. — *Costo se sbagliato:* un lotto
può restare a metà; l'annullamento per `batch_id` è la rete di sicurezza.

**Ruling: il tetto per lotto resta quello già in vigore** (`crate::batch::reject_oversized_batch`).
— *Costo se sbagliato:* nessuno, è già la convenzione.

### 3.3 Endpoint che adottano l'involucro

| Endpoint | Oggi | Dopo |
|---|---|---|
| `POST /flags/batch` | `204` | involucro |
| `POST /metadata/batch` | `{batch_id}` | involucro (+`batch_id`) |
| `POST /metadata/batch/shift-taken-at` | `{batch_id}` | involucro |
| `POST /metadata/batch/copy-location` | `{batch_id}` | involucro |
| `POST /metadata/batch/import-gpx` | `{batch_id}` | involucro |
| `POST /metadata/batch/recalculate-timezones` | `{batch_id}` | involucro |
| **`POST /assets/batch/delete`** | *non esiste* | involucro (§4) |
| **`POST /albums/{id}/assets/batch`** | *non esiste* | involucro (§5) |

È un **cambio non retrocompatibile** su sei endpoint. Il contratto congelato dice che
`/api/v1` è additivo: da `204` a `200`+corpo un client vecchio non si rompe (ignora il corpo),
ma da `{batch_id}` a un oggetto più ricco nemmeno, perché `batch_id` resta al suo posto.
**Nessun campo viene rimosso o rinominato.**

---

## 4. Eliminazione: da singola a di massa, mantenendo le tre vie

`DiskAction::{Kept, MovedToTrash, Purged}` (`crates/keeppix-domain/src/trash.rs:11`) è già
esattamente il modello del documento (SP-18): solo indice / cestino di Keeppix 30 giorni /
disco, senza default implicito, con `Purged` ristretto a owner e admin. **Non va toccato.**

Manca solo la forma di massa, che la UI usa in tre punti: la barra di selezione (SP-2), il
dialog di eliminazione a tre opzioni applicato a N elementi (§49), e la risoluzione di un gruppo
di duplicati (§46, dove la modalità scelta va propagata a tutte le copie non tenute).

```
POST /api/v1/assets/batch/delete
{ "asset_ids": [...], "disk_action": "kept" | "moved_to_trash" | "purged" }
→ involucro §3
```

**Ruling: `disk_action` resta obbligatorio anche nella forma di massa.** — È il punto n.6 delle
otto richieste, ed è dichiarato non negoziabile: *"Keeppix chiede sempre quale delle tre"*.
Un default renderebbe possibile cancellare 400 file dal disco per omissione. — *Costo se
sbagliato:* nessuno; un campo in più nel corpo.

---

## 5. Album: dinamici, condivisi, con copertina

### 5.1 Cosa manca

`0016_albums.sql` ha `albums(id, name, description, owner_id, cover_asset_id, …)` e
`album_assets(album_id, asset_id, position, added_by, added_at)`. Le schermate §41, §42 e §43
chiedono in più:

- **album dinamici**: condizioni di filtro + operatore (*tutte* / *almeno una*), i cui membri
  sono **calcolati**, mai materializzati;
- **flag `condiviso`** (badge nella griglia);
- **tinta della copertina** e flag monocromatico;
- **intervallo di date testuale** ("Gen 2026 – Lug 2026") e conteggio membri, come aggregati.

### 5.2 Schema

```sql
ALTER TABLE albums
    ADD COLUMN kind        text NOT NULL DEFAULT 'manual'
                           CHECK (kind IN ('manual','dynamic')),
    ADD COLUMN rule        jsonb,          -- SearchNode serializzato, solo se kind='dynamic'
    ADD COLUMN is_shared   boolean NOT NULL DEFAULT false,
    ADD COLUMN cover_tint  text,           -- es. '#7A8B6F'
    ADD COLUMN monochrome  boolean NOT NULL DEFAULT false;

ALTER TABLE albums
    ADD CONSTRAINT albums_rule_matches_kind
    CHECK ((kind = 'dynamic') = (rule IS NOT NULL));
```

**Ruling: `rule` è un `SearchNode` serializzato, non un linguaggio nuovo.** — Un album dinamico
*è* una ricerca salvata con un nome e una copertina. Riusare `SearchNode` significa che ogni
asse aggiunto alla ricerca (§6) diventa automaticamente disponibile agli album dinamici, e che
il compilatore SQL è uno solo. — *Costo se sbagliato:* le condizioni della UI (cartella, data,
fotocamera, obiettivo, tipo file, preferito, valutazione, pick) devono esistere come
`SearchNode`, il che è esattamente ciò che la §6 fa.

**Ruling: i membri di un album dinamico non sono mai scritti in `album_assets`.** — Il documento
li descrive come raccolte *"vive": si aggiornano da sole quando arrivano nuove foto che
corrispondono*. Materializzarli significherebbe doverli ricalcolare a ogni import, a ogni
modifica di metadato e a ogni cestinamento. — *Costo se sbagliato:* il conteggio dei membri
costa una `COUNT` a ogni apertura della griglia; si mitiga con la cache in-process già
introdotta in Fase 6.

`GET /albums` restituisce per ciascun album `member_count` e `date_range` (min/max `taken_at`)
come aggregati calcolati, per entrambi i tipi.

---

## 6. Assi di ricerca mancanti

`SearchNode` (`crates/keeppix-db/src/search.rs:27`) ha oggi:
`And, Or, Not, Text, Type, Camera, Lens, Iso, Year, Folder, HasGps`.

Servono, e **nessuno dipende da IA, volti o culling**:

| Variante nuova | Serve a |
|---|---|
| `Rating { cmp, value }` | album dinamici (§43), filtri |
| `Favorite` | chip "Preferiti" in Cerca (§23), album dinamici |
| `DateRange { from, to }` | il placeholder della topbar dice *"Cerca per data…"*; §43 |
| `Day { value }` / `Month { value }` | raggruppamenti e filtri di §43 |
| `Country { value }` | pillola "Paese" di §24, oggi si crea ma non filtra |
| `Aperture { cmp, value }` / `Shutter { cmp, value }` | condizioni di §43 |

Restano fuori, perché arrivano con le rispettive fasi:
`Tag`/`Category` (Fase 7), `Person` (Fase 8), `Pick` (Fase 9), `Semantic` (Fase 7).

**Ruling: le varianti nuove si aggiungono a `SearchNode`, non a un secondo modello di filtro.** —
Il filtro rapido a chip (SP-3), le pillole di Cerca (§24) e le condizioni degli album dinamici
(§43) sono tre interfacce diverse sopra la **stessa** domanda. Tenere un solo AST significa un
solo compilatore SQL, un solo punto dove aggiungere un indice, un solo posto da testare. —
*Costo se sbagliato:* l'AST cresce; è già progettato per farlo (è ricorsivo con `And`/`Or`/`Not`).

### 6.1 Indici richiesti dai nuovi assi

```sql
CREATE INDEX assets_rating_idx   ON asset_flags (rating)   WHERE rating > 0;
CREATE INDEX assets_favorite_idx ON asset_flags (asset_id) WHERE favorite;
CREATE INDEX assets_taken_day_idx ON assets (taken_at_utc) WHERE status <> 'trashed';
```

L'indice parziale su `favorite` è quello che conta: i preferiti sono per definizione una
piccola frazione della libreria (71 su 900 nel prototipo, ~8%), e un indice parziale è
un ventesimo di quello pieno.

---

## 7. Tassonomia degli errori (richiesta #7 del documento)

> *"Distinguere almeno quattro nature di fallimento: server irraggiungibile, permessi mancanti,
> file assente, tempo scaduto. `Riprova` ha senso nei primi due e non negli altri due."* (§68)

Il tipo `Problem` (RFC 7807) esiste già. Manca la **garanzia** che il campo `type` appartenga a
un insieme chiuso e che il frontend possa deciderci sopra. Si fissano quattro nature, con la
politica di ritentativo attaccata a ciascuna:

| `reason` | Significato | `Riprova` ha senso? |
|---|---|---|
| `unreachable` | il server o la libreria di rete non risponde | **sì** |
| `permission-denied` | percorso non leggibile/scrivibile | **sì** (dopo intervento) |
| `file-missing` | il file non è più sul disco | no — serve una scansione |
| `timeout` | l'operazione ha superato il tempo massimo | no — serve frazionare |

**Ruling: `reason` è un insieme chiuso e versionato, non testo libero.** — È l'unico modo perché
il frontend possa scegliere il messaggio *e* decidere se mostrare "Riprova". Un `reason` non
riconosciuto dal frontend ricade su un messaggio generico, non su un crash. — *Costo se
sbagliato:* aggiungere una quinta natura è un cambio additivo, non rotto.

---

## 7bis. Due concetti che nel backend non esistono affatto

Non sono dettagli di forma: sono nozioni di prima classe che attraversano molte schermate, e il
primo passaggio di analisi le aveva mancate.

### 7bis.1 «Preferito»

Zero occorrenze di `favorite` in `keeppix-domain`, `keeppix-db`, `keeppix-api`.
`AssetFlagsBody` (`crates/keeppix-api/src/routes/flags.rs:19`) ha **solo** `rating`, `pick`,
`color_label`.

L'interfaccia lo usa in sette punti: il cuoricino su **ogni** tessera (SP-1), la sezione
"Preferiti" che è una vista intera (§9, *"71 foto, da tutte le cartelle"*), l'azione di massa
della barra di selezione (SP-2), il chip "Preferiti" di Cerca (§23), una condizione degli album
dinamici (§43), la modifica in blocco (§13), e il pannello informazioni del lightbox (§19).

**Ruling: `favorite` è un campo nuovo di `asset_flags`, non un riuso di `Pick`.** — `Pick` è lo
stato di una foto *dentro un lotto di culling*, e il glossario del documento è esplicito: *"sono
stati del culling, non della libreria: una foto scartata in un lotto non è una foto eliminata"*.
Una foto può essere `Pick` e non preferita, e viceversa: sono assi indipendenti. Riusarlo
significherebbe che scartare uno scatto nel culling lo toglie dai preferiti. — *Costo se
sbagliato:* una colonna e un indice parziale in più.

`favorite` è **per utente**, come il resto di `asset_flags`: due persone sulla stessa libreria
hanno preferiti diversi.

### 7bis.2 Conteggio di foto per cartella

`FolderView` (`crates/keeppix-api/src/routes/folders.rs:14`) è
`{id, library_id, parent_id, name, depth}`. La sidebar mostra `Urbino 556`,
`Lago di Braies 110`, `Chioggia e Venezia 246` accanto a **ogni** cartella (§2), e la
sotto-pagina mobile "Cartelle" mostra `"556 foto"` su ogni scheda (§6).

È il primo di una famiglia: gli aggregati che l'interfaccia mostra **per riga di un elenco**, e
che diventano N+1 se non si progettano come una sola query.

| Aggregato | Dove | Fase |
|---|---|---|
| foto per cartella | sidebar, a ogni render | **10** |
| membri per album | griglia album | **10** (§5) |
| elementi per link pubblico | Condivisioni (§29) | **10** |
| foto per tag | Tag e categorie (§52) | 7 |
| foto per persona | Persone (§31) | 8 |
| da valutare per lotto | badge sidebar, selettore lotto (§14, §16) | 9 |

**Ruling: un `GROUP BY` solo per elenco, mai un `COUNT` per riga; risultato in cache `moka` con
invalidazione esplicita.** — La cache di Fase 6 è senza TTL apposta: qui un conteggio scaduto non
è un rallentamento, è un numero sbagliato mostrato all'utente. Le invalidazioni vanno agganciate a
import, cestinamento e spostamento. — *Costo se sbagliato:* la sidebar fa N query a ogni render.

---

## 8. Altre cose che nessuna fase copre

### 8.1 Sessioni attive (§61 Profilo)
La UI elenca i dispositivi collegati con *tipo di dispositivo/browser*, *ultimo accesso
leggibile*, *quale è la sessione corrente*, e offre "Esci" per singola sessione e "Esci da tutti
gli altri dispositivi". La tabella `sessions` esiste dalla Fase 0; mancano l'elenco e la revoca.

```
GET    /api/v1/users/me/sessions      → [{id, device_label, last_seen_at, current}]
DELETE /api/v1/users/me/sessions/{id}
POST   /api/v1/users/me/sessions/revoke-others
```

`device_label` si deriva dallo `User-Agent` al login e si **memorizza** allora: derivarlo a ogni
lettura significherebbe conservare lo User-Agent completo, che è più dato personale del
necessario.

### 8.2 Spazio su disco (sidebar, §2)
La sidebar mostra "Spazio libero — 1,4 TB su 2 TB". Nel prototipo è testo statico. Serve
davvero, per libreria:

```
GET /api/v1/libraries/{id}/storage → { free_bytes, total_bytes }
```

`statvfs` è già usato in `crates/keeppix-db/src/uploads.rs` (ed è, oggi, il punto che **non
compila** — vedi il debito aperto sulla Fase 6): la stessa primitiva, esposta.

### 8.3 Preferenze utente persistite (§60 Impostazioni)
Tema, densità griglia (due valori distinti desktop/mobile), tre preferenze di notifica, lingua.
Oggi vivono solo in memoria nel prototipo. `system_settings` esiste ma è globale: queste sono
**per utente**.

```
GET   /api/v1/users/me/preferences
PATCH /api/v1/users/me/preferences
```

**Ruling: un solo documento `jsonb` per utente, non una colonna per preferenza.** — Sono
preferenze di presentazione, lette tutte insieme all'avvio e mai interrogate singolarmente in
SQL. Una colonna per preferenza significa una migrazione per ogni preferenza nuova. — *Costo se
sbagliato:* non si possono filtrare gli utenti per preferenza, cosa che nessuna schermata chiede.

### 8.4 Lo stack nelle viste di browse (richiesta #4)
`stacks` esiste e `GET /assets/{id}/stack` funziona, ma `AssetView` **non espone lo stack**: la
timeline restituisce RAW e JPEG come due asset, e la UI disegnerebbe due tessere per uno scatto.

Si aggiungono ad `AssetView` due campi, entrambi additivi:

```
stack_size: u16          // 1 se non impilato
raw_kind: "raw" | "raw+jpeg" | "jpeg" | null    // per il badge SP-15
```

e la timeline restituisce **solo il primario** di ogni pila.

**Ruling: il collasso avviene nella query, non nel frontend.** — Se il collasso fosse a valle,
`count` per mese (richiesta #3), la geometria (§2) e la paginazione conterebbero tutte gli
elementi non collassati: tre numeri sbagliati e un layout che salta. — *Costo se sbagliato:* la
query di timeline prende un `WHERE (stack_id IS NULL OR is_primary)`, che l'indice deve coprire.

---

## 9. Cosa questa fase **non** fa

- Non implementa tag, ricerca semantica o analisi: è la **Fase 7**.
- Non implementa volti, persone o gruppi di persone: è la **Fase 8**.
- Non implementa lotti di culling, spostamento file o rinomina con formula: è la **Fase 9**.
- Non costruisce nessuna schermata: è la **Fase 11**.

Lascia però pronte, per tutte e quattro, le convenzioni che dovranno rispettare: l'involucro di
riuscita parziale, la tassonomia degli errori, e `SearchNode` come unico modello di filtro.

---

## 10. Emendamenti che questa analisi impone alle fasi già specificate

Sono divergenze **reali** fra le specifiche scritte e ciò che il prototipo mostra. Vanno
applicate ai rispettivi documenti, non a questo.

### 10.1 Fase 7 — la soglia è **per tag**, non globale
La spec (`fase-7-ai-tag-scene.md`, §225–228) descrive due soglie di sistema, alta e bassa.
Il prototipo mostra una soglia **per singolo tag**, visibile nella pagina "Tag e categorie"
(78%, 85%, 80%, 75%, 72%, 82%) e modificabile nel dialog "modifica tag" (§53).

→ `tags` prende `threshold real NOT NULL DEFAULT 0.75`.
→ La nota semantica di §53 va rispettata alla lettera: *cambiare la soglia non rivaluta nessuna
foto già decisa; governa le analisi future, mai riscrive decisioni esistenti.*

### 10.2 Fase 7 — serve uno **stato esplicito** della coppia (tag, foto)
`asset_tags` ha `source ('ai','user')`, `score`, `decided_by`, `decided_at`. Mancano i tre stati
che la UI distingue ovunque (SP-12, §56): **proposto**, **confermato**, **rifiutato**.
Dedurli è ambiguo: una riga `source='ai'` senza `decided_at` è "proposta", ma una rifiutata
com'è fatta? Il documento è netto: *"il rifiuto è permanente e definitivo, una proposta rifiutata
non tornerà mai in coda"*.

→ `asset_tags` prende `state text NOT NULL CHECK (state IN ('proposed','confirmed','rejected'))`.

### 10.3 Fase 9 — `Pick` come variante di `SearchNode`
Già annotato durante il brainstorming della Fase 9: il filtro "cartella + stato (presa /
scartata / da valutare)" richiede una variante `Pick` in `SearchNode`. Con la §6 di questo
documento il posto dove aggiungerla è definito.

### 10.4 Fase 8 — la regola dei volti va imposta dove i link sono serviti
La richiesta #8 dice *"va garantita dove i link pubblici vengono serviti — non solo
nell'interfaccia"*. Il punto è `crates/keeppix-api/src/routes/share.rs`
(`public_info`, `public_assets`): la Fase 8 deve aggiungervi un test che dimostri che nessun
dato di volto attraversa quelle due funzioni, non solo che la UI non li disegna.
