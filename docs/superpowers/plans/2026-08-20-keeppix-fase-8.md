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
SCRFD (rilevamento) e ArcFace (impronta), via il crate `face_id` — stesso autore e stesso stack
`ort` di quello della Fase 7. **Misurare su hardware vero** come nel Task 1 della Fase 7, e
mettere il numero nel ledger.

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

---

## Gruppo D — Chiusura

### Task 9 — `SearchNode::Person`
È il chip «Persona» che nel prototipo è **disabilitato di proposito** in attesa di questa fase.

### Task 10 — Interruttore e cancellazione
Due comandi **distinti**, e la differenza va rispettata: **spegnere** il riconoscimento smette di
calcolare e conserva i dati; **«Elimina tutti i dati dei volti»** li cancella. È la risposta alla
domanda aperta n.6.

### Task 11 — WebSocket, documenti, e il test del Task 1 che ora deve passare
`suggestions.changed` include i volti. Il test della regola dei link pubblici **deve essere
verde**, ed è la condizione di chiusura della fase.
