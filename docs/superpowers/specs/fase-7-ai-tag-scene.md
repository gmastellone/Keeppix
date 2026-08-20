# Fase 7 — Riconoscimento scene, tag e ricerca semantica

**Stato:** specifica di progetto, non ancora pianificata in task
**Dipende da:** Fase 1 (asset indicizzati, coda job, profili energetici), Fase 3
(permessi: un tag non deve rivelare foto che non puoi vedere)
**Chiusa quando:** su un Raspberry Pi 5, con la libreria reale già indicizzata,
cercare «tramonto con casa» restituisce risultati sensati in meno di un secondo,
e un tag creato dall'utente si popola da solo **senza rianalizzare le foto**

---

## 1. La decisione portante: un solo embedding per foto

Tutto ciò che questa fase promette — categorie automatiche, ricerca per
descrizione libera, tag suggeriti — si regge su **un solo numero calcolato una
volta sola per foto**: un vettore CLIP.

### 1.1 Perché non un classificatore

L'istinto è addestrare (o scaricare) un classificatore di scene: «questa foto è
architettura, questa è fauna». Va scartato, e il motivo è il requisito stesso:

> **L'utente crea i tag; l'IA li abbina, non li inventa.**

Un classificatore ha le sue categorie **fissate al momento dell'addestramento**.
Se l'utente crea «Regate», il classificatore non può assegnarlo: quella classe
non esiste nei suoi pesi, e non c'è modo di aggiungerla senza riaddestrare.

CLIP risolve questo per costruzione, non per aggiunta:

| | Classificatore | CLIP (embedding condiviso) |
|---|---|---|
| Categorie | fisse nei pesi | **qualsiasi frase**, decise a runtime |
| Tag nuovo dell'utente | riaddestramento | una query, zero inferenza |
| Ricerca libera («tramonto con casa») | ❌ impossibile | ✅ stessa identica operazione |
| Inferenze per foto | 1 per classificatore | **1 in totale** |
| Può inventare categorie | sì, quelle sue | **no**: senza un tag scritto dall'utente non esiste il vettore da confrontare |

L'ultima riga è la garanzia che il requisito chiede. Non è un controllo
applicativo che si può dimenticare: **il vincolo è fisico**. Per proporre un
tag serve l'embedding testuale di quel tag, e quell'embedding esiste solo se
qualcuno ha scritto quel tag.

### 1.2 Un modello, tre funzionalità

```
                    ┌─────────────────────────┐
   foto ──────────► │  MobileCLIP (immagine)  │ ──► vettore 512 dim ──┐
                    └─────────────────────────┘                        │
                                                                       ▼
                    ┌─────────────────────────┐              ┌──────────────────┐
   «Fauna» ───────► │  MobileCLIP (testo)     │ ──► vettore  │ pgvector         │
   «tramonto con    │  una volta per tag,     │      512 dim │ (stesso Postgres)│
    casa»           │  una volta per ricerca  │              └──────────────────┘
                    └─────────────────────────┘                        │
                                                                       ▼
                                              similarità coseno = tutte e tre le funzioni
```

- **Ricerca per descrizione**: embedding della frase → i K vettori più vicini.
- **Tag automatico**: embedding del nome del tag → le foto sopra soglia.
- **«Foto simili a questa»**: il vettore della foto stessa → gratis, nessun
  calcolo nuovo.

**Il costo di calcolo dell'intera fase è un'inferenza per foto, una volta.**
Tutto il resto sono confronti fra vettori già calcolati: prodotti scalari, non
chiamate al modello.

### 1.3 Il modello scelto

**MobileCLIP2-S2**, esportato in ONNX. Progettato da Apple per hardware edge;
la S2 ottiene risultati migliori di SigLIP ViT-B/16 essendo 2,3× più veloce e
2,1× più piccola.

Il crate [`open-clip-inference-rs`](https://github.com/RuurdBijlsma/open-clip-inference-rs)
(su `ort`, cioè ONNX Runtime) lo supporta esplicitamente e pubblica i tempi:
**~75 ms per immagine, ~19 ms per testo** sulla S2. Le varianti S3 (116/35 ms) e
S4 (192/38 ms) restano disponibili come opzione per chi ha hardware più grosso.

**Quei numeri non sono misurati su un Raspberry Pi.** Sono il punto di partenza,
non una promessa: il primo task della fase è misurarli sull'hardware vero
(§6.1). Se sul Pi la S2 costa 400 ms invece di 75, la fase resta valida — cambia
la stima del backfill, non l'architettura.

### 1.4 Il motore di inferenza

**ONNX Runtime via il crate `ort`.** È C++ sotto, come LibRaw che il progetto
già accetta dove serve.

L'alternativa Rust-puro, `tract`, sarebbe più coerente e più leggera all'avvio
(nessuna libreria dinamica, nessuna arena di memoria generica), ma copre meno
operatori ONNX: il rischio è scoprire a metà fase che un layer del modello non è
supportato. **Va provata per prima** (§6.1): se `tract` carica ed esegue il
modello scelto, si usa quella; altrimenti `ort`. La decisione è misurata, non
dichiarata qui.

Scartato **ncnn** (probabilmente il più veloce in puro calcolo su ARM, ma i
binding Rust sono immaturi) e scartato il **microservizio Python separato** —
che è ciò che fa Immich, e che qui significherebbe un secondo runtime e una
seconda immagine da tenere viva su un Pi.

---

## 2. Dove vivono i vettori

**pgvector, dentro lo stesso PostgreSQL.** Nessun database nuovo, nessun
servizio in più: la stessa scelta già fatta per PostGIS e per GeoNames.

### 2.1 Il problema dell'immagine

`postgis/postgis:17-3.5` **non contiene pgvector**, e `pgvector/pgvector:pg17`
non contiene PostGIS — che serve alle Fasi 4 e 5. Servono entrambe.

Soluzione: un `Dockerfile.db` che parte da `postgis/postgis:17-3.5` e aggiunge
l'estensione. È l'unico pezzo di infrastruttura nuovo della fase, e va
dichiarato come tale: oggi il `compose.yaml` usa un'immagine pubblica senza
`build:`.

**Chi usa un Postgres esterno** (percorso documentato in `DEPLOY.md`) deve
avere pgvector installato. Se manca, l'avvio **non fallisce**: le funzioni AI
si disattivano e il pannello lo dice con una frase leggibile e il comando da
eseguire. Una galleria fotografica non deve rifiutarsi di partire perché non
sa proporre tag.

### 2.2 Schema

```sql
CREATE EXTENSION IF NOT EXISTS vector;

-- Un embedding per asset. `model_version` è parte dell'identità del dato:
-- vettori di modelli diversi NON sono confrontabili fra loro.
CREATE TABLE asset_embeddings (
    asset_id      uuid PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    embedding     vector(512) NOT NULL,
    model_version text NOT NULL,
    computed_at   timestamptz NOT NULL DEFAULT now()
);

-- HNSW: ricerca approssimata, costruzione più lenta di ivfflat ma nessun
-- bisogno di riaddestrare l'indice quando la libreria cresce — su un archivio
-- che si riempie a ondate è la proprietà che conta.
CREATE INDEX asset_embeddings_hnsw
    ON asset_embeddings USING hnsw (embedding vector_cosine_ops);

CREATE INDEX asset_embeddings_model_idx ON asset_embeddings (model_version);

-- I tag li crea l'utente. `embedding` è il vettore del *testo* del tag,
-- calcolato una volta alla creazione: è ciò che rende l'abbinamento una
-- query invece di una rianalisi.
CREATE TABLE tags (
    id            uuid PRIMARY KEY,
    name          text NOT NULL,
    kind          text NOT NULL CHECK (kind IN ('tag','category')),
    parent_id     uuid REFERENCES tags(id) ON DELETE SET NULL,
    -- Frase di aggancio opzionale: «Regate» da solo è ambiguo per il modello,
    -- «barche a vela in regata» no. L'utente vede il nome, il modello usa questa.
    prompt        text,
    embedding     vector(512),
    model_version text,
    color         text,
    -- Soglia **per tag**, non di sistema: la pagina "Tag e categorie"
    -- dell'interfaccia la mostra per ogni riga (78%, 85%, 80%…) ed è
    -- modificabile nel dialog "modifica tag". Un tag largo come «Paesaggi»
    -- e uno stretto come «Fauna selvatica» non possono condividere la
    -- stessa soglia senza che uno dei due diventi inutile.
    -- Semantica vincolante: cambiarla governa le analisi *future* e non
    -- rivaluta mai una foto già decisa (vedi §56 del documento funzionale).
    threshold     real NOT NULL DEFAULT 0.75,
    created_by    uuid NOT NULL REFERENCES users(id),
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (name, kind)
);

-- `parent_id` (categoria → tag figli) senza indice sarebbe una scansione di
-- `tags` per ogni categoria aperta. Tabella piccola (governata da un
-- umano, non dalla scala delle foto) — la differenza si sente solo con
-- centinaia di tag, ma costa nulla averlo da subito.
CREATE INDEX tags_parent_idx ON tags (parent_id) WHERE parent_id IS NOT NULL;

-- Assegnazioni materializzate. Sono una *cache* di un confronto vettoriale,
-- non una seconda verità: si possono ricostruire, e servono a rendere il
-- browsing istantaneo invece di ricalcolare a ogni click.
CREATE TABLE asset_tags (
    asset_id   uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    tag_id     uuid NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    -- Tre stati, espliciti. Dedurli da `source` + `decided_at` è ambiguo:
    -- una coppia rifiutata e una mai proposta si assomigliano troppo, e il
    -- documento funzionale è categorico — «il rifiuto è permanente e
    -- definitivo, una proposta rifiutata non tornerà mai in coda». Con lo
    -- stato esplicito la regola diventa una condizione WHERE invece di una
    -- convenzione da ricordare.
    state      text NOT NULL CHECK (state IN ('proposed','confirmed','rejected')),
    source     text NOT NULL CHECK (source IN ('ai','user')),
    score      real,
    -- Una conferma o un rifiuto umano non viene mai sovrascritto dall'IA.
    decided_by uuid REFERENCES users(id),
    decided_at timestamptz,
    PRIMARY KEY (asset_id, tag_id)
);

CREATE INDEX asset_tags_tag_idx ON asset_tags (tag_id);

-- La coda di revisione (§56) chiede «tutte le proposte in attesa, raggruppate
-- per tag». Senza questo indice è una scansione di una tabella che cresce
-- come foto × tag; le proposte sono una minoranza, quindi indice parziale.
CREATE INDEX asset_tags_proposed_idx ON asset_tags (tag_id, asset_id)
    WHERE state = 'proposed';
```

### 2.3 `model_version` non è un dettaglio

Due embedding prodotti da modelli diversi vivono in spazi diversi: confrontarli
dà numeri privi di significato, non un errore. La colonna esiste perché:

- ogni query di similarità filtra su un solo `model_version`;
- cambiare modello è un'operazione **dichiarata**, che invalida gli embedding
  esistenti e richiede un ricalcolo completo — con avviso esplicito del costo
  («12.400 foto, ~4 ore di notte»), mai silenziosamente;
- durante il ricalcolo la ricerca continua a funzionare sul modello vecchio,
  finché il nuovo non è completo. Nessuna finestra di risultati sbagliati.

---

## 3. Tag e categorie — il modello funzionale

### 3.1 Cosa può fare l'utente

- **Creare** un tag o una categoria. Le categorie sono contenitori (`kind`
  distinto e `parent_id`): «Natura» come categoria, «Fauna selvatica» e
  «Paesaggi» come tag dentro. Un solo livello di annidamento, non un albero
  arbitrario — oltre non serve a un archivio fotografico e complica la ricerca.
- **Correggere il prompt** senza rinominare il tag. Il nome è per gli umani, il
  prompt è per il modello. Cambiare il prompt ricalcola **solo** l'embedding di
  quel tag (19 ms) e rivaluta le assegnazioni con una query — non tocca le foto.
- **Confermare o rifiutare** un'assegnazione automatica. La decisione è
  permanente: `decided_by`/`decided_at` la rendono immune a ogni rivalutazione
  successiva.
- **Assegnare a mano** un tag a una selezione di foto, anche in blocco
  (`source = 'user'`).

### 3.2 Cosa fa l'IA, e cosa non può fare

L'IA **abbina soltanto**. Per ogni foto con embedding e ogni tag con embedding,
calcola la similarità e:

- sopra la soglia del tag → assegna (`state = 'proposed'`, `source = 'ai'`, con
  lo `score`);
- fra `soglia − banda` e la soglia → **suggerisce comunque**, ma con uno `score`
  più basso: la coda di revisione è ordinata per score, quindi le proposte più
  deboli finiscono in fondo;
- sotto `soglia − banda` → niente.

**Un solo numero per tag, visibile e modificabile** (`tags.threshold`): è quello
che la pagina "Tag e categorie" mostra su ogni riga e che il dialog "modifica
tag" lascia cambiare. La **banda** sotto la soglia è invece una costante di
sistema, non esposta: serve a evitare che il taglio sia netto al punto da
perdere proposte che stanno un punto percentuale sotto.

La soglia è per-tag perché è l'unico modo onesto: «Tramonti» e «Foto di gruppo»
non hanno la stessa separabilità.

**Nulla entra in libreria senza una persona.** Anche sopra soglia lo stato è
`proposed`, non `confirmed`: l'IA riempie una coda, non la libreria. È la
traduzione in schema del principio dichiarato dall'interfaccia — *«Tu crei i
tag, l'IA li abbina soltanto alle foto»* — e di SP-12, che non confonde mai un
suggerimento con una decisione umana.

**Non può creare tag.** Non è un divieto scritto in un `if`: senza una riga in
`tags` non esiste un vettore da confrontare.

### 3.3 Quando un tag nuovo viene creato

Questo è il punto in cui il disegno paga:

```
Utente crea «Fauna selvatica»
        │
        ├─ 1 inferenza testo (~19 ms)
        │
        └─ 1 query pgvector sull'indice già costruito
                 ↓
           «412 foto sopra soglia, 89 incerte»   ← nessuna foto rianalizzata
```

Su una libreria da 200.000 foto già indicizzate, creare un tag costa
**un'inferenza di testo e una query**, non 200.000 inferenze.

---

## 4. Ricerca

### 4.1 Come si innesta su quella esistente

La ricerca attuale è un **AST JSON** (`SearchNode` in `keeppix-db`) con nodi
`And/Or/Not/Text/Type/Camera/Lens/Iso/Year/Folder/HasGps`. La fase aggiunge due
varianti, non un secondo motore:

- `Tag { id }` — filtro esatto su `asset_tags`, come `Folder`.
- `Semantic { query, limit }` — embedding della frase, K vicini via pgvector.

Il guadagno: **si combinano con tutto il resto**. «Tramonto con casa, scattate
nel 2024, con la Sony, in Grecia» è un `And` di `Semantic`, `Year`, `Camera` e
(dalla Fase 4) un filtro geografico. Nessuna di queste funzioni è un'isola.

### 4.2 Il vincolo sui permessi

Un risultato semantico passa dalla **stessa** `VisibilityScope` di ogni altra
query. Non è opzionale ed è la parte facile da sbagliare: una ricerca vettoriale
tende a essere scritta come «prendi i primi K globali», che scavalcherebbe i
permessi della Fase 3.

L'ordine corretto è: filtro di visibilità **dentro** la query, poi i K vicini fra
ciò che l'utente può vedere — non i K globali filtrati dopo, che restituirebbe
meno risultati del dovuto (o zero) a chi ha accesso parziale.

### 4.3 Costo

Una query HNSW su 200.000 vettori è nell'ordine dei millisecondi. Il costo vero
è l'inferenza testuale della frase: ~19 ms. **La ricerca semantica è più veloce
della ricerca testuale su trigram** che il progetto già ha.

---

## 5. Il backfill — la parte che gira per ore

Una libreria esistente da 200.000 foto richiede 200.000 inferenze. A 200 ms per
foto su un Pi sono ~11 ore. Non è un problema **se non disturba**.

### 5.1 L'infrastruttura esiste già

Il progetto ha `EnergyProfile` (`Interactive` / `Background` / `Night` /
`Paused`) e `ActivityTracker`: un job a priorità `Background` **non viene
reclamato** finché c'è stata una richiesta autenticata negli ultimi 5 minuti.

Conseguenza diretta, senza scrivere codice nuovo di throttling: **il backfill AI
si ferma da solo appena qualcuno apre la galleria, e riparte quando smette.** Di
notte (finestra 02:00–06:00) va a piena velocità.

I job di embedding si accodano quindi a `JobPriority::Background`, e la fase
eredita gratis un comportamento che altrimenti sarebbe un sottosistema.

### 5.2 Le foto nuove non aspettano

Una foto appena caricata (WebDAV, tus, watcher) accoda l'embedding a
`JobPriority::High`, come già fa la Fase 5 per l'indicizzazione: chi carica
adesso vede i tag adesso.

### 5.3 Cosa vede l'utente

```
Analisi AI                                      12.412 / 200.000 foto
────────────────────────────────────────────────────────────────────
▓▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░  6%    in pausa — stai usando la galleria
                                     riprende automaticamente, o [continua ora]

Stima: ~11 ore di elaborazione notturna
Modello: MobileCLIP2-S2 · 187 ms/foto misurati su questo hardware
```

Onestà nell'interfaccia: il tempo dichiarato è **misurato**, non stimato da una
tabella; la pausa è spiegata invece di sembrare un blocco.

---

## 6. Hardware: misurare invece di indovinare

### 6.1 Il probe attuale non basta

Esiste `keeppix_media::probe()`, ma restituisce `"unprobed"`: non misura nulla,
ed era pensato per l'**accelerazione video** (Fase 6), non per l'inferenza.

Questa fase ha bisogno di sapere altro:

| Cosa | Perché serve |
|---|---|
| Core disponibili e RAM libera | quanti thread dare a ONNX Runtime; sotto una soglia le funzioni AI restano spente |
| **ms per inferenza, misurati** | la stima del backfill mostrata all'utente, e la scelta fra S2/S3/S4 |
| `vector` presente in Postgres | senza, la fase si disattiva con un messaggio, non con un errore |
| Set di istruzioni utile (NEON, INT8) | selezione della variante quantizzata quando disponibile |

Il probe va quindi **esteso**, non sostituito: resta un'unica funzione che
misura le capacità della macchina, con una sezione video (che la Fase 6
riempirà) e una sezione inferenza (che riempie questa).

La misura si fa **una volta**, all'avvio o su richiesta, su un'immagine di
prova inclusa nell'immagine Docker: due o tre inferenze, meno di un secondo. Il
risultato va in `system_settings` (dove il probe già scrive) ed è **visibile e
sovrascrivibile a mano** dal pannello: se l'operatore sa che la sua macchina fa
meglio, deve poterlo dire.

### 6.2 Degradare, non fallire

Tre livelli, decisi dalla misura:

1. **Pieno** — hardware sufficiente: embedding automatico all'ingest + backfill.
2. **Ridotto** — hardware debole: nessun backfill automatico; l'utente può
   avviarlo a mano sapendo che durerà. Le foto nuove vengono comunque analizzate.
3. **Spento** — pgvector assente o RAM insufficiente: la sezione AI dell'interfaccia
   non compare, con una riga che dice perché. Il resto di Keeppix funziona
   identico.

Nessuno di questi stati è un errore: sono configurazioni legittime dello stesso
prodotto su hardware diverso.

### 6.3 Il modello nell'immagine, non scaricato a runtime

I pesi ONNX (~150–300 MB secondo la variante) vengono **cotti nell'immagine
Docker**, come GeoNames in Fase 4. Il crate scelto di default li scaricherebbe
da HuggingFace al primo avvio: va usata l'opzione con file locali.

Il vincolo di progetto è invariato: **zero richieste di rete verso l'esterno**.

---

## 7. Privacy

- Gli embedding sono dati derivati dalle foto e vivono nello stesso database:
  chi può leggere il database poteva già leggere le foto. Nessuna superficie
  nuova.
- **Nessuna inferenza lascia la macchina.** Non c'è un'API esterna, non c'è
  telemetria, i pesi sono locali.
- Su un link pubblico i tag seguono la stessa regola dei metadati: se il link
  nasconde i metadati, nasconde anche i tag — un tag può essere descrittivo
  quanto una didascalia.
- Un tag creato da un utente è visibile agli altri utenti dell'istanza (è un
  vocabolario condiviso della libreria); le **assegnazioni** restano soggette
  ai permessi sulle foto.

---

## 8. Cosa NON è in Fase 7

- **Riconoscimento dei volti**: è la Fase 8. Motore diverso, e soprattutto una
  complessità propria (cluster, correzioni manuali che devono sopravvivere).
- **OCR / testo nelle immagini**: utile, ma è un terzo modello e un terzo indice.
- **Didascalie generate** («un cane corre sulla spiaggia»): richiede un modello
  generativo, un ordine di grandezza più pesante. La ricerca per descrizione
  copre il bisogno reale senza generare testo.
- **Addestramento o fine-tuning sui dati dell'utente**: fuori scope. I pesi sono
  fissi e ispezionabili.
- **Inferenza lato client** (browser, app mobile): possibile in futuro come
  ottimizzazione, mai come sostituto — WebDAV, rclone e il watcher non hanno un
  client che possa calcolare. Il percorso server-locale resta l'unico obbligatorio.


---

## Emendamento — 20 agosto 2026: da dove si legge, e cosa si analizza

La specifica non diceva **da quale immagine** si calcola l'impronta né **quali foto** si
analizzano. Sono le due leve che pesano di più sul costo, più di qualunque scelta di modello.

### A. L'ingresso è la miniatura già generata, non l'originale

Il modello lavora a **224–256 px**. Keeppix genera già una miniatura **WebP da 240 px** per ogni
foto (`THUMB = 240`, `keeppix-media/src/derive.rs:40`), scritta su disco all'ingestione.

**Ruling: l'analisi legge la miniatura, non l'originale.** — Decodificare un RAW costa centinaia
di millisecondi ed è la ragione per cui l'ingestione è lenta; leggere una WebP da 240 px ne costa
uno o due. Rifare quel lavoro per l'IA significherebbe **pagare due volte la parte più cara della
pipeline**, per ottenere un'immagine che poi viene comunque ridotta a 224 px. Con la miniatura,
il costo per foto si riduce all'inferenza e basta. — *Costo se sbagliato:* dettagli fini persi
sotto i 240 px; per riconoscere «tramonto», «montagna» o «ritratto» sono irrilevanti.

Corollario: **l'analisi può girare solo su foto che hanno già la miniatura**, il che la incatena
naturalmente a valle dell'ingestione invece di competerci.

### B. L'IA non entra nel culling. Punto.

**Ruling: si analizza la libreria; i lotti di culling sono fuori.** — Il culling è un'**area di
transito**: le foto ci arrivano dalla scheda, si scelgono, e poi escono — quelle tenute entrano
in libreria, le altre spariscono. Analizzare lì dentro significa lavorare su materiale che per
definizione non è ancora deciso, e nella maggior parte dei casi buttare il lavoro. Una foto viene
analizzata **quando entra in libreria**, che è esattamente il momento in cui qualcuno ha deciso
che vale la pena tenerla. — *Costo se sbagliato:* una foto appena uscita dal culling non è
cercabile per contenuto finché l'analisi non la raggiunge, cosa che accade comunque in
sottofondo entro pochi minuti.

Il confine è **una condizione sulla cartella, non uno stato per foto**: la Fase 9 marca le
cartelle di culling (`folders.culling_role`, sotto `libraries.culling_root_folder_id`). L'analisi
le salta e basta.

Questo cancella tre complicazioni che una versione precedente di questo emendamento si era
inventata: escludere gli scartati uno per uno, buttare l'analisi quando una foto viene scartata,
e rianalizzare quando viene ripescata. **Nessuna delle tre serve più.**

Resta invece valida una sola esclusione, dentro la libreria:

**Una impronta per pila, non per file.** RAW e JPEG affiancati sono **un solo scatto** (richiesta
#4 del documento funzionale): si analizza il primario. È la stessa definizione di «una foto» che
l'interfaccia usa per contare, selezionare ed eliminare — usarne una diversa qui farebbe divergere
i numeri fra due schermate. Su un archivio RAW+JPEG è metà del lavoro in meno.

### C. Le altre leve, in ordine di resa

1. **Inferenza a lotti** — passare N immagini al modello in una volta invece di una alla volta:
   tipicamente 2–4× di throughput, perché ammortizza il costo fisso per invocazione.
2. **Quantizzazione int8** — su CPU ARM tipicamente 2–3× più veloce, con perdita di qualità
   trascurabile per un confronto di somiglianza (non stiamo classificando, stiamo ordinando).
3. **Modello più piccolo** — la scala MobileCLIP ha varianti: la più piccola è molto più veloce e
   perde poco su categorie larghe come quelle di un archivio fotografico. **Da misurare**, è il
   compromesso più soggettivo.
4. **Prima ciò che si guarda** — analizzare per prime le foto viste di recente e la cartella
   aperta, così la ricerca funziona su ciò che interessa mentre il resto arriva. Riusa la stessa
   idea di `POST /viewport`.

### D. L'indice vettoriale: da misurare, non da dare per scontato

La specifica dava per acquisito un indice HNSW. Su 200.000 impronte:

- **HNSW** — ricerca in millisecondi, ma costa RAM e un tempo di costruzione non banale;
- **IVFFlat** — molto più economico da costruire e da tenere in memoria, richiamo leggermente
  inferiore;
- **nessun indice** — una scansione lineare legge ~400 MB; se stanno nella cache di Postgres
  (`shared_buffers` tarato, vedi Fase 10 Task 1bis) può bastare.

**Ruling: si parte senza indice e si misura.** — Su un Pi con 8 GB, un indice HNSW compete per la
RAM con `shared_buffers`, che serve a tutto il resto dell'applicazione. Se la scansione lineare
sta sotto la soglia di interattività, l'indice è complessità e memoria spese per niente.
— *Costo se sbagliato:* si aggiunge l'indice dopo, che è un `CREATE INDEX` e nient'altro.
