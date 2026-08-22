# Piano — Fase 8: Volti

**Specifica:** `docs/superpowers/specs/fase-8-volti.md` (con l'emendamento del 20 agosto 2026).
**Base:** dopo la Fase 7, di cui **riusa** motore di inferenza, pgvector, probe hardware,
scheduler a livelli e coda di revisione. Da sola dovrebbe costruirseli tutti.
**Branch:** `fase-8`.

> Piano a livello di task e decisioni: è scritto prima che le Fasi 10 e 7 esistano.

---

## La regola che viene prima di tutto

> **I volti sono dati biometrici. Non compaiono mai su un link pubblico condiviso. Non è
> configurabile, vale sempre.**

È l'unica regola del documento funzionale dichiarata **non negoziabile**, ed è la richiesta n.8
delle otto che toccano il backend. Il documento è esplicito su dove va garantita: *«dove i link
pubblici vengono serviti — non solo nell'interfaccia»*, cioè in
`crates/keeppix-api/src/routes/share.rs` (`public_info`, `public_assets`).

**Ruling: la regola si chiude con un test, non con una revisione del codice.** — Una regola
garantita dall'attenzione di chi scrive si rompe alla prima aggiunta distratta. Serve un test che
costruisca una foto con volti confermati, la esponga via link pubblico, e **asserisca che nessun
campo di volto attraversa la risposta**. — *Costo se sbagliato:* si esporta un dato biometrico
da un link pubblico, che è il difetto peggiore che questo progetto possa produrre.

Il test va scritto nel **Task 1**, prima del codice che potrebbe violarlo.

---

## Gruppo A — Fondamenta

### Task 1 — Il test della regola, prima di tutto
Scrivere il test descritto sopra. Fallirà per assenza di volti: va bene, deve esistere prima.

### Task 2 — Modelli e misura
**Emendamento 22 agosto: YuNet (rilevamento) e SFace (impronta)**, non SCRFD/ArcFace — i pesi
InsightFace sono research-only, incompatibili con la doppia licenza commerciale. Fonti, sha256
e dettagli in `plans/2026-08-22-keeppix-modelli-ai.md` (Task A). Stesso stack `ort` della
Fase 7. **Misurare su hardware vero** come nel Task 1 della Fase 7, e mettere il numero nel
ledger.

### Task 3 — Schema
`faces` (bbox in coordinate **relative**, `rejected_at` per i falsi positivi, `assigned_by` /
`assigned_at` per le decisioni umane), `persons`, `person_groups`, `person_group_members` (con
l'indice su `person_id`), `person_separations` (con `CHECK (person_a < person_b)`).

---

## Gruppo B — La pipeline

### Task 4 — Rilevare e riconoscere
1. **Il culling è fuori**, come per la Fase 7: si lavora sulla libreria.
2. **Un primario per pila.**
3. **Rilevamento sulla miniatura da 240 px; impronta del volto sulla preview da 2048 px.**
   Trovare *dove* sono i volti è un compito a scala grossa; distinguere Marta da Elena richiede
   pixel veri. Nessuna delle due decodifica l'originale.
4. I volti troppo piccoli **restano non riconosciuti**, di proposito: un'attribuzione sbagliata
   costa all'utente più di un volto mancato, perché va trovata e corretta a mano.

**Registrare la passata come `operations` (Fase 10), non lasciarla senza avanzamento/cancel.**
`OperationKind` (`crates/keeppix-domain/src/operation.rs`) ha oggi una sola variante,
`LibraryScan`, pensata apposta per crescere: aggiungere una variante per il rilevamento volti,
creare una riga via `OperationsRepo::create` per la passata su tutta la libreria, aggiornarla con
`record_success_many`/`finish_done`. Il polling WS esistente (`drain_operations`) emette
`operation.progress` da solo — nessun evento nuovo, stesso pattern richiesto alla Fase 7 per la
sua finestra di analisi. Senza questo, un amministratore che lancia il rilevamento su una
libreria intera non ha modo di vederne l'avanzamento né di annullarlo.

### Task 5 — Raggruppamento incrementale
**Non** un raggruppamento globale rifatto da capo a ogni ondata: distruggerebbe le correzioni
manuali. I volti nuovi si agganciano ai gruppi esistenti; solo ciò che non si aggancia forma
gruppi nuovi.

`person_separations` **blocca permanentemente** il riaccorpamento automatico di due persone che
un umano ha separato.

---

## Gruppo C — Le persone

### Task 6 — Persone e gruppi
CRUD, rinomina (con il campo vuoto **rifiutato**: il prototipo non lo controlla ed è un difetto
segnalato), flag «nascosta», scelta della copertina. Gruppi di persone — **distinti** dai gruppi
di permessi della Fase 3, che sono un'altra cosa e non vanno confusi nel modello.

### Task 7 — Unisci e separa
**Unire** riassegna tutti i volti alla persona sopravvissuta ed elimina le assorbite.
**Separare** crea una persona nuova: **non ripristina lo stato precedente**. È la risposta alla
domanda aperta n.5 del documento funzionale, e va scritta nell'interfaccia perché l'utente non
si aspetti un annullamento.

### Task 8 — La coda di revisione volti
Stessa forma della coda tag (SP-10). Tre esiti: conferma, nuova persona, **«non è un volto»**
(falso positivo permanente). Azioni in blocco con l'involucro di riuscita parziale.

**`BulkOutcome`/`BulkFailure` (Fase 10, `crates/keeppix-api/src/bulk.rs`) sono tipizzati su
`AssetId`**, non generici — confermato su tutti gli 8 punti in cui sono usati oggi. Le azioni in
blocco su questa coda lavorano su persone/volti, non su asset: serve un tipo gemello (es.
`BulkOutcome<PersonId>`/`BulkOutcome<FaceId>`, o un piccolo refactor generico di `bulk.rs`) che
riusi la stessa forma e la stessa tassonomia di `FailureReason` — tenendo conto che
`FailureReason::FileMissing` non ha senso qui (`Unknown`/`PermissionDenied`/`Timeout` bastano).

**Il conteggio di questa coda alimenta `BootstrapResponse.badges.revision`**
(`crates/keeppix-api/src/routes/bootstrap.rs`), lo stesso campo che la Fase 7 collega al
conteggio delle proposte di tag — il commento nel codice dice apposta *"Proposte tag/volti in
attesa (Fasi 7/8)"*: è un conteggio combinato, questa fase aggiunge la propria metà, non un
campo separato.

---

## Gruppo D — Chiusura

### Task 9 — `SearchNode::Person`
È il chip «Persona» che nel prototipo è **disabilitato di proposito** in attesa di questa fase.

### Task 10 — Interruttore e cancellazione
Due comandi **distinti**, e la differenza va rispettata: **spegnere** il riconoscimento smette di
calcolare e conserva i dati; **«Elimina tutti i dati dei volti»** li cancella. È la risposta alla
domanda aperta n.6.

**L'interruttore è per libreria** (spec §7): `crates/keeppix-db/src/preferences.rs` è per
utente, con una lista chiusa di chiavi che rifiuta campi sconosciuti — non è il posto giusto. Il
precedente reale è `libraries.scan_enabled: bool` (`crates/keeppix-db/src/libraries.rs`): una
nuova colonna sulla stessa riga, stesso pattern, non una voce di preferenze utente.

### Task 11 — WebSocket, documenti, e il test del Task 1 che ora deve passare
`suggestions.changed` include i volti. Il test della regola dei link pubblici **deve essere
verde**, ed è la condizione di chiusura della fase.
