# Fase 8 — Volti: rilevamento, raggruppamento, correzione

**Stato:** specifica di progetto, non ancora pianificata in task
**Dipende da:** Fase 7 (motore di inferenza, pgvector, probe hardware, backfill
a priorità energetica: tutto riusato, nulla di duplicato), Fase 3 (permessi)
**Chiusa quando:** su un archivio reale, le foto di una persona sono raggruppate
sotto un nome, e le correzioni fatte a mano — unioni, separazioni, «non è un
volto» — **sopravvivono a ogni rianalisi successiva**

---

## 1. Perché è una fase separata dalla 7

Il calcolo è comparabile: un'inferenza in più per foto. La **complessità** no.

La Fase 7 assegna un'etichetta a una foto: l'operazione è senza stato, ripetibile,
e se sbaglia si corregge una foto alla volta. Qui il sistema costruisce
**identità che vivono nel tempo**: un cluster nato oggi deve accogliere le foto
di domani, sopportare che l'utente lo unisca a un altro, e non disfare quella
decisione alla passata successiva.

È quel requisito — *le correzioni umane sono permanenti* — a giustificare una
fase propria. Il resto è la parte facile.

---

## 2. La pipeline

```
foto ──► SCRFD ──► N riquadri + 5 punti ──► allineamento ──► ArcFace ──► N vettori 512
         (rileva)      (occhi, naso, bocca)   (Umeyama, 112×112)  (identità)
                                                                        │
                                                                        ▼
                                              assegnazione incrementale a una persona
```

### 2.1 I modelli

- **Rilevamento: SCRFD**, variante **500MF** — progettata per hardware limitato.
  Restituisce riquadro, confidenza e cinque punti di riferimento.
- **Allineamento: Umeyama** sui cinque punti, verso una posa canonica 112×112.
  Non è un dettaglio: un volto storto produce un embedding peggiore, e la qualità
  del raggruppamento dipende quasi tutta da qui.
- **Identità: ArcFace**, 512 dimensioni — lo standard di fatto.

Il crate [`face_id`](https://docs.rs/face_id/latest/face_id/) incapsula tutte e
tre le fasi (stesso autore e stessa base `ort` del crate CLIP della Fase 7:
un solo stack di inferenza per entrambe le fasi, non due).

Da correggere in integrazione: di default scarica i pesi da HuggingFace al primo
avvio. Vanno usati i **file ONNX locali**, cotti nell'immagine come in Fase 7 e
come GeoNames in Fase 4. Il vincolo «zero rete esterna» non ha eccezioni.

### 2.2 Riuso, non duplicazione

Dalla Fase 7 arrivano già pronti: il motore `ort`, pgvector, il probe hardware
con la misura di inferenza, il backfill che si mette in pausa quando l'utente
naviga. Questa fase aggiunge **un job in più nella stessa coda**, non un
sottosistema parallelo.

### 2.3 Costo

Due inferenze per foto (rilevamento + un embedding per volto trovato), contro
l'una della Fase 7. Su una foto senza volti il costo è il solo rilevamento.

Il numero reale sul Pi si misura, non si assume — stesso primo task della Fase 7,
stesso principio.

---

## 3. Schema

```sql
-- Un volto rilevato. Vive anche senza persona: prima si rileva, poi si raggruppa.
CREATE TABLE faces (
    id           uuid PRIMARY KEY,
    asset_id     uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    -- Riquadro in coordinate relative (0..1): sopravvive a ritagli e derivati
    -- di dimensione diversa, che con i pixel assoluti sarebbe da ricalcolare.
    bbox_x       real NOT NULL,
    bbox_y       real NOT NULL,
    bbox_w       real NOT NULL,
    bbox_h       real NOT NULL,
    landmarks    jsonb,
    embedding    vector(512),
    detect_score real NOT NULL,
    -- Qualità (nitidezza, dimensione, posa): un volto sfocato di 20px non deve
    -- decidere l'identità di un cluster.
    quality      real,
    person_id    uuid REFERENCES persons(id) ON DELETE SET NULL,
    -- Decisione umana su QUESTO volto. NULL = l'automatismo può ancora agire.
    assigned_by  uuid REFERENCES users(id),
    assigned_at  timestamptz,
    -- Falso positivo dichiarato: un disegno, una texture, un volto in un poster.
    rejected_at  timestamptz,
    model_version text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX faces_asset_idx  ON faces (asset_id);
CREATE INDEX faces_person_idx ON faces (person_id) WHERE rejected_at IS NULL;
CREATE INDEX faces_hnsw ON faces USING hnsw (embedding vector_cosine_ops);

-- Una persona. Il nome è opzionale: «Persona 4» con 37 foto è già utile.
CREATE TABLE persons (
    id            uuid PRIMARY KEY,
    name          text,
    cover_face_id uuid REFERENCES faces(id) ON DELETE SET NULL,
    -- Centroide degli embedding dei volti confermati: evita di ricalcolarlo
    -- a ogni confronto. Si aggiorna quando il gruppo cambia.
    centroid      vector(512),
    hidden_at     timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (name)
);

-- Gruppi di PERSONE FOTOGRAFATE. Da non confondere con i `groups` della
-- Fase 3, che sono gruppi di *utenti* per i permessi: nomi simili, concetti
-- distinti, tabelle separate di proposito.
CREATE TABLE person_groups (
    id         uuid PRIMARY KEY,
    name       text NOT NULL UNIQUE,
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE person_group_members (
    group_id  uuid NOT NULL REFERENCES person_groups(id) ON DELETE CASCADE,
    person_id uuid NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, person_id)
);

-- La chiave primaria copre "chi è nel gruppo X" (group_id in testa), non
-- "in quali gruppi sta la persona Y" — query altrettanto naturale (il
-- dettaglio di una persona che mostra i suoi gruppi) e senza questo indice
-- farebbe una scansione della tabella.
CREATE INDEX person_group_members_person_idx ON person_group_members (person_id);

-- LA TABELLA CHE FA LA DIFFERENZA (§4.3): due persone che l'utente ha
-- separato non devono mai essere riunite dall'automatismo.
CREATE TABLE person_separations (
    person_a   uuid NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    person_b   uuid NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    -- Coppia non ordinata: si memorizza sempre con a < b.
    PRIMARY KEY (person_a, person_b),
    CHECK (person_a < person_b)
);
```

---

## 4. Raggruppamento

### 4.1 Incrementale, non a lotti

Il raggruppamento classico (DBSCAN/HDBSCAN su tutti i volti) va rieseguito da
capo a ogni aggiunta, e **ridisegna i cluster ogni volta**: le correzioni
dell'utente verrebbero cancellate a ogni passata. Inadatto.

Qui il modello è incrementale, un volto alla volta:

```
nuovo volto
   │
   ├─ qualità sotto soglia?  ──► resta senza persona, non inquina nessun cluster
   │
   ├─ cerca il centroide più vicino fra le persone esistenti (query pgvector)
   │
   ├─ distanza < soglia_certezza   ──► assegnato, centroide aggiornato
   ├─ distanza < soglia_dubbio     ──► **proposto**, non assegnato: va in revisione
   └─ oltre                        ──► nuova persona (senza nome)
```

Costa una query vettoriale per volto, non un riaggregazione globale. Ed è
**stabile**: aggiungere una foto non ridisegna nulla di ciò che c'è già.

### 4.2 Le tre correzioni

**Unione** — «queste 3 persone sono la stessa»:
tutti i volti passano alla persona di destinazione (si conserva quella con il
nome, se ce n'è una), i centroidi si ricalcolano, le persone svuotate spariscono.

**Separazione** — l'errore opposto, e più insidioso: un cluster che contiene due
persone diverse. L'utente seleziona i volti da estrarre; nasce una persona nuova,
**e si scrive una riga in `person_separations`**.

**«Non è un volto»** — `rejected_at`. Sparisce dall'interfaccia e, soprattutto,
non viene mai riproposto: senza questo, ogni rianalisi ripropone lo stesso
falso positivo e l'utente lo rifiuta all'infinito.

### 4.3 Perché `person_separations` esiste

È la tabella che distingue un sistema utilizzabile da uno frustrante.

Se l'utente separa due gemelli, e domani l'algoritmo trova che i loro centroidi
sono vicinissimi, li riunirà — annullando la correzione. L'utente rifarà la
separazione. Il ciclo si ripete a ogni foto nuova.

Con `person_separations` la regola diventa esplicita: **una coppia separata a
mano non è più candidata all'unione automatica, per sempre.** L'automatismo può
proporre, mai eseguire, su quella coppia.

Stessa logica su `faces.assigned_by`: un volto assegnato a mano non viene
riassegnato da un ricalcolo. **La decisione umana è sempre più forte della
misura.**

---

## 5. Cosa vede l'utente

```
Persone                                                    [gestisci gruppi]
──────────────────────────────────────────────────────────────────────────
  Famiglia                                                        4 persone
   ( Giovanni 1.204 )  ( Marta 890 )  ( Luca 445 )  ( Anna 210 )

  Amici                                                           7 persone
   ( Paolo 88 )  ( Chiara 61 )  …

  Senza gruppo
   ( Persona 12  37 )  ( Persona 13  22 )                     [dai un nome]

──────────────────────────────────────────────────────────────────────────
  Da rivedere                                              23 proposte
  «Questi volti sembrano Giovanni»          [conferma tutti] [uno per uno]
```

Nel dettaglio di una persona: unisci, separa, rinomina, scegli copertina,
nascondi (per gli sconosciuti sullo sfondo che non interessano ma non sono
falsi positivi).

Nel dettaglio di una foto: i riquadri dei volti con il nome, e da lì
l'assegnazione manuale.

### 5.1 Gruppi

CRUD puro sopra persone già identificate: nessun calcolo, nessuna IA. Servono a
navigare («mostrami le foto della Famiglia nel 2024» diventa un filtro come gli
altri) e a nascondere in blocco chi non interessa.

Una persona può stare in più gruppi.

---

## 6. Ricerca

Come in Fase 7, nuove varianti dell'AST esistente, non un motore a parte:

- `Person { id }` — le foto in cui compare.
- `PersonGroup { id }` — le foto in cui compare **almeno una** persona del gruppo.
- `PersonCount { cmp, value }` — «foto con almeno 3 persone», utile per trovare
  le foto di gruppo.

Combinabili con tutto il resto: «Famiglia, in Grecia, nel 2024, con almeno 4
persone» è un `And` di quattro nodi che esistono già o li aggiunge questa fase.

**Stessa `VisibilityScope` di ogni altra query.** Vale identico l'avvertimento
della Fase 7 §4.2: una ricerca per persona non deve mai rivelare l'esistenza di
foto che l'utente non può vedere.

---

## 7. Privacy — la sezione che non è formalità

Gli embedding facciali sono **dati biometrici**. Il regime è diverso da quello
di un tag «tramonto», e il progetto lo tratta come tale.

- **Tutto resta sulla macchina.** Nessun servizio esterno, nessun database di
  identità pubblico, nessun confronto con volti che non siano nelle foto
  dell'utente. Keeppix non sa *chi* è una persona: sa che questi volti si
  somigliano, e il nome lo scrive l'utente.
- **Disattivabile per intero**, e per ogni libreria. Chi non vuole il
  riconoscimento facciale non deve subirlo: è un interruttore, e da spento non
  si rileva nulla — non «si rileva ma non si mostra».
- **Cancellabile**: eliminare una persona elimina i suoi embedding, non solo
  l'etichetta. Serve un'azione esplicita «elimina tutti i dati dei volti» che
  faccia piazza pulita di `faces`, `persons` e gruppi.
- **Sui link pubblici i volti non compaiono mai.** Né riquadri, né nomi, né
  filtri per persona: un link condiviso non deve rivelare chi frequenta chi.
  Non è configurabile.
- **Le foto degli altri utenti non contribuiscono** ai cluster visibili a un
  utente che non ha accesso a quelle foto.

---

## 8. Cosa NON è in Fase 8

- **Riconoscere persone da un database esterno**: mai. Contrario allo scopo.
- **Età, genere, emozioni**: il crate scelto li offre, e restano spenti. Sono
  inferenze su caratteristiche personali che questa galleria non ha motivo di
  fare, spesso imprecise, e con implicazioni che non valgono il beneficio.
- **Animali domestici**: richiede un modello diverso; la ricerca semantica della
  Fase 7 («il mio cane nero») copre una parte del bisogno.
- **Raggruppamento a lotti globale** (HDBSCAN e simili): §4.1 — incompatibile
  con la persistenza delle correzioni manuali.
- **Riconoscimento in tempo reale su video**: fuori scope; i video sono Fase 6.


---

## Emendamento — 20 agosto 2026: stesse regole della Fase 7

Tre vincoli, identici a quelli fissati per l'analisi delle scene, e per le stesse ragioni.

**1. I volti non si cercano nel culling.** I lotti sono un'area di transito: le foto ci arrivano
dalla scheda, si scelgono, e poi escono. Il rilevamento parte **quando una foto entra in
libreria**, non prima. Il confine è una condizione sulla cartella (`folders.culling_role`), non
uno stato per foto.

**2. Un volto per pila, non per file.** Si lavora sul primario: RAW e JPEG affiancati sono lo
stesso scatto e le stesse facce.

**3. L'ingresso è la miniatura, con un'avvertenza.** A differenza delle scene, qui i **240 px
della miniatura possono non bastare**: un volto in secondo piano su uno scatto di gruppo occupa
pochi pixel, e sotto una certa dimensione il rilevamento fallisce o l'impronta è rumorosa.

**Ruling: il rilevamento gira sulla miniatura, l'impronta del volto sulla preview.** — Trovare
*dove* sono i volti è un compito su scala grossa e la miniatura basta; ricavare l'impronta che
distingue Marta da Elena richiede pixel veri. La preview da 2048 px (`PREVIEW_LONG_SIDE`) esiste
già su disco, quindi neanche qui si decodifica l'originale. — *Costo se sbagliato:* i volti molto
piccoli restano non riconosciuti, il che è preferibile a riconoscerli male: un'attribuzione
sbagliata costa all'utente più di un volto mancato, perché va trovata e corretta a mano.

**Conseguenza sui costi.** L'analisi dei volti resta la voce più cara dopo l'ingestione, ma il
lavoro si riduce alla libreria vera — non ai lotti in lavorazione — e non ripaga mai due volte lo
stesso scatto.
