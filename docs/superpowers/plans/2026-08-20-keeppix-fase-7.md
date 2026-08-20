# Piano — Fase 7: Scene, tag e ricerca semantica

**Specifica:** `docs/superpowers/specs/fase-7-ai-tag-scene.md` (con i tre emendamenti del
20 agosto 2026: soglia per tag, stato esplicito, e «da dove si legge / cosa si analizza»).
**Base:** dopo la chiusura della Fase 10, di cui riusa involucro di riuscita parziale,
tassonomia degli errori, `SearchNode` e gli eventi WebSocket.
**Branch:** `fase-7`.

> **Nota sul livello di dettaglio.** Questo piano è scritto **prima** che la Fase 10 esista.
> È quindi volutamente a livello di *task e decisioni*, non di firme esatte: un piano che
> inventa firme contro codice non ancora scritto è finzione plausibile. Il primo task di ogni
> gruppo comincia leggendo il codice reale.

---

## Cosa esiste già (da verificare al momento, non assumere)

- **Il probe hardware esiste ma non misura.** `keeppix-media/src/probe.rs` ha un chiamante e
  restituisce `"unprobed"`: è un segnaposto dalla Fase 1b. Il Task 1 lo rende reale.
  **Attenzione:** esiste una seconda `probe()` in `video.rs` che è tutt'altro (ffprobe su un
  file). Non confonderle.
- **`get_json` è in `scripts/wired-exceptions.txt` come rinvio a questa fase**: è il lettore
  delle capacità hardware misurate. Questa fase lo paga.
- **I derivati esistono già su disco**: miniatura WebP **240 px** (`THUMB`) e preview
  **2048 px** (`PREVIEW_LONG_SIDE`), scritte all'ingestione. L'analisi legge quelle.
- **`compose.yaml` usa un'immagine pubblica senza `build:`**: aggiungere pgvector significa
  introdurre il primo `Dockerfile.db` del progetto.
- **`EnergyProfile` e `JobPriority`** governano già chi può girare quando. L'analisi è
  `Background`: non compete con chi naviga.

---

## Gruppo A — Misurare prima di costruire

### Task 1 — Il probe hardware diventa reale
Sostituire `"unprobed"` con una misura vera: core disponibili, RAM libera, e **il tempo di
un'inferenza sul modello scelto**, eseguita davvero all'avvio su un'immagine di prova.

**Ruling: la fase comincia con una misura, non con una stima.** — I numeri della specifica
(75 ms per immagine sulla S2) vengono dal crate, non da un Pi. Se sul Pi costano 400 ms, la
fase resta valida ma cambia tutto il resto: la stima del backfill, la scelta del livello
predefinito, e forse il modello. — *Costo se sbagliato:* si pianificano dodici task su un
numero inventato.

Il risultato va in `system_settings` e lo legge `get_json`, che così smette di essere un rinvio.

### Task 2 — `tract` o `ort`, deciso per prova
Provare **prima `tract`** (Rust puro, nessuna libreria dinamica, avvio più leggero). Se carica
ed esegue il modello scelto, si usa quella. Altrimenti `ort`. **La decisione va nel ledger con
il motivo e i due tempi**, non dichiarata a priori.

---

## Gruppo B — Fondamenta

### Task 3 — L'immagine Postgres con pgvector
`Dockerfile.db` che parte da `postgis/postgis:17-3.5` e aggiunge pgvector. `compose.yaml` passa
da `image:` a `build:`.

**Se pgvector manca** (chi usa un Postgres esterno) **l'avvio non fallisce**: le funzioni AI si
disattivano e l'interfaccia lo dice con una frase leggibile e il comando da eseguire. Una
galleria fotografica non deve rifiutarsi di partire perché non sa proporre tag.

### Task 4 — Schema
`asset_embeddings`, `tags` (con `prompt`, `threshold`, `color`, `parent_id`), `asset_tags` (con
`state` proposed/confirmed/rejected, `source`, `score`, `decided_by`, `decided_at`), più gli
indici della specifica — **tranne quello vettoriale**, vedi Task 11.

---

## Gruppo C — La pipeline di analisi

### Task 5 — Calcolare le impronte
1. **Ingresso: la miniatura da 240 px già su disco.** Non si decodifica mai l'originale.
2. **Il culling è fuori**: si salta tutto ciò che sta sotto una cartella con `culling_role`
   (Fase 9). Se la Fase 9 non è ancora chiusa, il predicato si scrive comunque e resta inerte.
3. **Un primario per pila**: RAW+JPEG è un solo scatto.
4. **Inferenza a lotti**, non una foto alla volta.
5. Mai ricalcolare ciò che ha già un'impronta con lo stesso `model_version`.

### Task 6 — Lo scheduler dell'analisi
- Priorità `Background`: non parte mentre qualcuno naviga.
- **Pausa automatica**, soglia **4000 ms** dall'ultima attività (Fase 10 Task 21), configurabile.
- Finestra notturna a piena velocità. **Attenzione:** `default_night_window()` è 2:00–6:00 ma
  l'interfaccia promette 2:00–7:00 — vanno allineate, e vince l'interfaccia salvo ragioni.
- I tre livelli **Piena / Ridotta / Spenta**, con i tempi **misurati** dal Task 1 mostrati
  all'utente.

**Ruling: i livelli si presentano con i numeri veri, non come «livello di IA».** —
*«Analisi completa: ~2 ore, poi qualche minuto al giorno»* è una scelta informata; un'etichetta
astratta no. — *Costo se sbagliato:* l'utente sceglie a caso e poi si lamenta del risultato.

---

## Gruppo D — Tag

### Task 7 — Tag e categorie
CRUD di tag e categorie (un solo livello di annidamento). Creare o modificare un tag ricalcola
**solo** l'embedding di quel tag, non tocca le foto. Eliminare un tag elimina le sue decisioni —
e il dialog lo dice, mostrando quante foto sono coinvolte, prima di confermare.

### Task 8 — L'abbinamento
Per ogni tag, le foto sopra `threshold` entrano come `state='proposed'`, `source='ai'`, con lo
`score`. La banda sotto soglia produce proposte più deboli, ordinate più in basso.

**Nulla entra in libreria senza una persona.** Anche sopra soglia lo stato è `proposed`.

**Cambiare la soglia non rivaluta nulla di già deciso**: governa le analisi future.

### Task 9 — La coda di revisione
Conferma e rifiuto, singoli e **in blocco** («Conferma tutte» / «Rifiuta tutte» per tag).
Le azioni di massa usano **l'involucro di riuscita parziale della Fase 10**.
**Il rifiuto è permanente**: una coppia rifiutata non torna mai in coda.

---

## Gruppo E — Ricerca

### Task 10 — `SearchNode`: `Tag`, `Category`, `Semantic`
Le prime due sono filtri normali. `Semantic` calcola l'embedding della frase (una inferenza
testuale, ~19 ms) e ordina per somiglianza.

### Task 11 — L'indice vettoriale, **solo se serve**
Partire **senza indice**. Misurare la scansione lineare su 200.000 impronte con
`shared_buffers` tarato (Fase 10 Task 1bis). Se sta sotto la soglia di interattività, non si
aggiunge nulla. Altrimenti valutare **IVFFlat prima di HNSW**: costa molto meno da costruire e
da tenere in RAM, che su un Pi da 8 GB compete con tutto il resto.

**Il numero va nel ledger**, e la decisione presa in base a quello.

---

## Gruppo F — Chiusura

### Task 12 — WebSocket e interfaccia
`analysis.progress` e `suggestions.changed` (Fase 10 Task 20). Gli eventi sono **magri**: un
segnale, non uno stato.

### Task 13 — Documenti e debiti
`get_json` esce dai rinvii. Aggiornare README, CONTINUE, roadmap, `wired-exceptions.txt`.
Ledger con un `Ruling:` per ogni decisione presa in corsa, **e i numeri misurati**.

---

## Chiusura della fase

`cargo fmt`, `clippy -D warnings`, `cargo deny`, `check-wired.py` puliti; `./scripts/test.sh`
verde; CI reale verde. E la prova che conta: **cercare «tramonto con casa» sulla libreria vera,
su un Pi, restituisce risultati sensati in meno di un secondo**, e un tag appena creato si
popola senza rianalizzare nulla.
