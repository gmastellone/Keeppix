# Keeppix — vai fino alla fine

**Aggiornamento 25 agosto: tutte le Fasi 0-11 sono chiuse e mergiate in `main`** (PR #3-#11 per
0-6; merge diretti via git per 7-11, dove le PR GitHub erano rotte, CI reale ottenuta con push su
`fase-*`). Fai `git pull` su `main`: c'è tutto quello che ti serve, IA compresa. Il lavoro aperto
non è più una fase del roadmap, ma il debito di licenza dei modelli IA (vedi §"modelli IA" più
sotto): Task A (volti, YuNet+SFace) chiuso e mergiato; Task B (embedding CLIP, OpenCLIP-XLM-R
int8 potato IT/EN) in corso. Da qui in avanti procedi da solo, con quattro sole eccezioni elencate
in fondo.

---

## 1. L'ordine, e perché è quello (storico — tutte le fasi sotto sono chiuse)

```
Fase 10 (✅ main)  →  Fase 7 (✅ main)  →  Fase 8 (✅ main)  →  Fase 9 (✅ main)  →  Fase 11 (✅ main, A→B→C→D)
```

**La Fase 10 è andata prima della 7, 8 e 9, e non era un'opinione.** Ha fissato l'involucro di
riuscita parziale, la tassonomia chiusa degli errori, `SearchNode` come unico modello di filtro
e gli eventi WebSocket. Le altre tre introducono da sole più di otto operazioni di massa: se la
convenzione fosse arrivata dopo, quelle otto andavano riscritte.

**La Fase 11 è in quattro tranche** che seguono le fasi da cui dipendono: A e B subito dopo la
10, C dopo la 7, D dopo la 8 e la 9. Costruire Persone contro un backend senza volti significa
costruire contro dati finti, e i dati finti nascondono proprio i problemi che l'integrazione
deve trovare.

---

## 2. Cosa leggere, in quest'ordine

1. **`docs/ui/costo-beneficio-funzioni.md`**, sezione **«Decisioni prese»** in fondo.
   **Leggila per prima**: è la fonte che vince su tutte le altre.
2. **`docs/ui/keeppix-mockup.html`** — il prototipo interattivo. Si apre con un doppio click,
   senza server. È la fonte di verità su **come si comporta** ciò che si costruisce: le
   scorciatoie funzionano davvero, gli stati disabilitati sono davvero disabilitati, le durate
   sono quelle vere. Aprilo mentre lavori sulla Fase 11, non leggerlo soltanto.
   **Contiene anche le funzioni tagliate: ignorale.**
3. **`docs/ui/documento-funzionale-ui.md`** — 70 schermate, ogni etichetta alla lettera. Si
   apre con un riquadro di emendamenti: leggilo. E **leggi la Parte XII prima del resto**
   («Assunzioni e domande aperte»): contiene i difetti del prototipo da **non** replicare.
4. **`docs/ui/analisi-gap-backend.md`** — il confronto punto per punto col backend reale.
5. **`docs/ui/caricamento-nuove-foto.md`** — solo se lavori sulla Fase 11 Task 6: l'area di
   caricamento di nuove foto dal browser, aggiunta dopo la consegna del 20 agosto.
6. `AGENTS.md`, il roadmap, e la spec + il piano della fase su cui lavori.

### Decisioni aggiunte dopo la stesura di questo file — 20 agosto sera

- **I RAW entrano in Keeppix solo attraverso il Culling, mai altrove.** Il Culling è un'area
  permanentemente separata dalla libreria: scegliere una foto la sposta in `_taken/` e **lì
  resta** — nessuna promozione automatica. Chi vuole una foto scelta nella libreria vera la
  ricarica **manualmente**: un'azione dell'utente, non una funzione di Keeppix.
- **L'IA esclude l'intero sottoalbero del Culling**, non solo `_taken`/`_skipped`: dato che i RAW
  vivono solo lì, escludere tutto l'albero esclude automaticamente ogni RAW, senza filtro per
  formato. **Non serve più** la regola "un'impronta per pila" — nella libreria non esistono pile
  RAW+JPEG, per costruzione.
- **Il modello IA non resta mai caricato**: si carica solo per lotto/finestra di analisi, si
  scarica subito dopo. **Tetto duro: sotto 1 GB di RSS reale mentre gira** — un candidato che lo
  sfora alla dimensione di lotto minima utile non si sceglie come predefinito, punto.
- **La libreria gestisce anche PNG, TIFF, WebP-sorgente, HEIF 8/10 bit**, non solo JPEG — debito
  reale su codice già in produzione: `derive.rs` oggi decodifica solo JPEG (Fase 10 Task 22).
- **L'area di caricamento di nuove foto dal browser è disegnata e prototipata** (Fase 11 Task 6,
  dettaglio nel piano). Non serve nuovo lavoro di backend: `POST /upload` accetta già
  `target_folder_id` e già distingue `created`/`skipped_duplicate`/`renamed` per file — è wiring
  del frontend su tus, già spedito in Fase 5. Fonte: `docs/ui/caricamento-nuove-foto.md`.

### Decisioni aggiunte il 22 agosto — modelli IA, licenze, ottimizzazione

- **I pesi MobileCLIP2 e InsightFace sono research-only** (verificato sui testi di licenza
  reali): incompatibili con la doppia licenza commerciale. È un **debito aperto e tracciato**,
  non un blocco: piano completo, benchmark e sha256 in
  [`plans/2026-08-22-keeppix-modelli-ai.md`](plans/2026-08-22-keeppix-modelli-ai.md).
- **Volti: YuNet + SFace** (OpenCV Zoo, MIT/Apache, ~9,5 MB totali) al posto di SCRFD/ArcFace —
  da fare **prima di chiudere la roadmap** (Task A del piano): la pipeline volti non ha mai
  girato contro pesi veri, questo la fa girare per la prima volta.
- **Embedding: OpenCLIP XLM-R ViT-B-32 int8, potato a SOLO IT/EN**, al posto di MobileCLIP2 —
  **dopo la Fase 11** (Task B del piano). Deciso su doppio benchmark: qualità pari o migliore
  (IT 0.95/MRR 0.975, EN 1.00), visual ~2,7× più leggero (271 MB di picco contro 744, stesso
  harness) e ~4× più veloce (22,7 ms/foto contro 95,7). Embed dim resta 512: nessuna migrazione.
- **Regole trasversali, valgono per entrambi i task e da qui in avanti**: niente Python a
  runtime — solo negli script di export/download offline; ottimizzazioni e misure si fanno in
  Rust nel crate `ort`; **il codice sostituito si elimina, non si commenta** — niente pesi,
  script, costanti o riferimenti al modello vecchio lasciati in giro.

### La precedenza, quando le fonti divergono

**decisioni → prototipo → documento funzionale → analisi gap.**

Un'eccezione che vale ovunque: **sull'accessibilità da tastiera il prototipo non è fonte di
verità**, perché è dichiaratamente rotta (Parte XII, §2.3) e il documento stesso dice di non
replicarla.

---

## 3. Le decisioni già prese, che non si rimettono in discussione

- **Album dinamici: non esistono.** Un album normale ricorda il filtro con cui è nato e ha un
  pulsante **«Aggiorna album»**. Il costo si paga quando l'utente lo chiede, non a ogni apertura
  della griglia.
- **Conteggi accanto alle righe: tolti**, tranne nel **culling**, dove restano esatti — lì
  «quante me ne restano da vedere» è letteralmente la domanda che l'utente si sta facendo.
- **Video: si tiene, ma minimo.** Solo in background o di notte, **una sola resa**, e **non si
  tocca** un video già piccolo o già riproducibile dal browser. Serve una tessera con badge di
  durata e uno stato «in preparazione» (`PlaybackResponse` ha già `ready`).
- **Audit: spento di default**, si accende col secondo utente.
- **L'IA non entra nel culling — mai, l'intero sottoalbero.** Il Culling non promuove mai
  automaticamente in libreria (dettagli e perché nella sezione più sotto, aggiunta dopo).
- **L'IA legge la miniatura da 240 px**, mai l'originale. Per i volti: rilevamento sulla
  miniatura, impronta sulla preview da 2048 px.

---

## 4. Come lavorare — invariato

- Framework superpowers. Ledger `.superpowers/sdd/<piano>/progress.md` con un
  `Ruling: <cosa> — <perché> — <costo se sbagliato>` per **ogni** decisione presa in corsa.
- Test alla fine di ogni task; `./scripts/test.sh` completo alla fine del branch.
- Aggiorna sempre: `docs/superpowers/README.md`, `docs/CONTINUE.md`, il roadmap,
  `scripts/wired-exceptions.txt`.
- Un branch per fase, PR, **CI reale verde prima del merge**. La repo è pubblica: i minuti
  Actions sono illimitati, non c'è più nessuna scusa.
- **Ogni piano ha una sezione «Cosa esiste già»**: leggila prima di iniziare. Dice cosa non
  reimplementare e cosa non rompere.

---

## 5. Le misure che devi produrre, non stimare

Vanno nel ledger, con il numero.

| Fase | Misura |
|---|---|
| 10 | `EXPLAIN` della timeline **prima e dopo** la taratura di Postgres; peso reale della geometria; import a lotti prima/dopo |
| 7 | ms per inferenza **sul Pi**, non quelli del crate; scansione vettoriale con e senza indice |
| 8 | ms per rilevamento e per impronta; quanti volti per foto sull'archivio vero |
| 9 | tempo di una rinomina di massa su 500 file |
| 11 | bundle iniziale gzip; tessere vive nel DOM durante uno scroll; tempo di calcolo del layout |

---

## 6. Le trappole già identificate

1. ~~`crates/keeppix-db/src/uploads.rs:588` non compilava~~ — **risolto** nel fix pass di Fase 6
   (cast `u64` esplicito, verificato riga per riga). Il Task 8 della Fase 10 può procedere.
2. **Postgres gira con i default** (`random_page_cost = 4.0`, `shared_buffers = 128 MB`). Se
   costruisci gli indici della Fase 10 prima di tararlo, **il pianificatore li ignora** e
   concluderai che non servono. Il **Task 1bis va prima** dei task sugli indici.
3. **Le scorciatoie da tastiera del prototipo si attivano anche dentro i campi di testo**: con
   un lotto aperto, digitare `1` cambia la valutazione della foto sottostante. È un difetto
   dichiarato, da risolvere alla radice: **mai attivare scorciatoie se il focus è in un campo o
   se un dialog è aperto.**
4. **Quattro funzioni in `wired-exceptions.txt` sono marcate `non-rivendicata`**
   (`get_for_user`, `timezone_for`, `timezone_changes`, `apply_taken_at_batch`, origine Fase 4):
   nessuna fase le rivendica. Se tocchi home/timezone, decidile — wired per davvero o rimosse.

---

## 7. Le due cose che valgono più di tutto il resto

Il documento funzionale apre con otto richieste che toccano il backend e dice: *«il punto 1 e il
punto 2 sono quelli da verificare per primi: se uno dei due non fosse realizzabile, cambia il
disegno, non l'implementazione»*. Sono i Task 2 e 1 della Fase 10, e **oggi nessuna delle due
esiste**:

1. **La geometria di tutta la vista in una richiesta.** Serve perché il layout è giustificato e
   virtualizzato: per sapere quanto è alta la pagina bisogna conoscere le proporzioni di *tutto*
   prima di disegnare un pixel. Formato binario da **6 byte per scatto, senza id** — 0,44 MB per
   214.000 foto. Gli id non servono: la geometria **non identifica nulla, descrive altezze**.
2. **Le operazioni di massa devono dire cosa è riuscito e cosa no, foto per foto.** Oggi
   `POST /flags/batch` risponde `204 No Content`, che è esattamente la bugia che il documento
   descrive.

**Se scopri che una delle due non è realizzabile come descritta, fermati e dillo** invece di
consegnare una versione annacquata: è il caso in cui cambia il disegno dell'interfaccia.

---

## 8. Dove ti fermi e chiedi

Quattro punti soltanto, quelli dove sbagliare costa più che aspettare:

1. **Se una delle due richieste del §7 non è realizzabile.**
2. **Prima della prima rinomina o spostamento su file veri** (Fase 9). È l'unica fase che tocca
   il disco dell'utente: la prima volta la guardiamo insieme.
3. **Se una misura ribalta una decisione** — il probe dice che l'inferenza costa dieci volte la
   stima, l'import a lotti non migliora, il layout supera i 50 ms. Porta il numero prima di
   riprogettare intorno.
4. **A fine fase, prima del merge.** Se stai lavorando con supervisione, rivede l'architetto.
   Se stai lavorando **senza revisore esterno** (sessione autonoma), il §10 sotto è la procedura
   che sostituisce quella revisione — non è facoltativa, è ciò che ha impedito due bug reali di
   arrivare in `main` durante lo sviluppo di questo stesso progetto.

Fuori da questi quattro, procedi.

---

## 10. Come si verifica una fase prima di mergiarla — nessuna scorciatoia

**Un test verde non è la prova che un requisito è rispettato.** Due esempi reali, successi in
questo stesso repo, entrambi trovati **solo** rileggendo il codice — mai fidandosi del ledger o
della CI:

- Fase 10: il commit di chiusura dichiarava `./scripts/test.sh — EXIT 0`, ma un test
  (`bootstrap_emits_no_more_queries_than_individual_repos`) era documentato **due volte** nello
  stesso ledger come flaky e mai risolto con un commit dedicato. La CI reale (mai girata prima
  su quel branch) l'ha fatto fallire alla prima run vera.
- Fase 7: il ledger diceva che il modello IA "non resta mai caricato", ma il codice lo
  ricaricava **ogni 16 foto** invece che una volta per finestra di analisi — ~31% di tempo
  sprecato in ricaricamenti, mai misurato perché nessuno aveva chiesto il numero.

**La procedura, la stessa usata per chiudere le Fasi 10 e 7:**

1. **Non verificare sul checkout in uso.** Crea un worktree di sola lettura sul branch da
   verificare: `git worktree add <percorso-scratch> origin/fase-N --detach`. Leggi da lì, non
   toccarlo, rimuovilo a verifica finita (`git worktree remove <percorso> --force`).
2. **Trova le due o tre cose che il piano della fase dichiara più importanti di tutto il resto**
   — ogni piano ne ha una sezione esplicita (vedi Fase 10 §7, Fase 7 i cinque Ruling sul
   ciclo di vita/RAM del modello). Sono il bersaglio prioritario: verificale leggendo
   l'implementazione reale — file e riga — non il riassunto che ne fa il ledger.
3. **Ogni numero nel ledger deve avere una fonte**: un log/output reale incollato, non una stima.
   Se un numero manca dove il piano lo richiede esplicitamente, è un buco, non un dettaglio.
4. **Leggi la cronologia CI del branch** (`gh run list --branch fase-N`), non solo l'ultima run.
   Fallimenti reali seguiti da correzioni sostanziali (non un timeout rilassato a caso, non un
   warning silenziato) sono un segnale positivo, non negativo — vuol dire che la CI ha trovato
   qualcosa di vero. Un rustfmt/clippy isolato non richiede scrutinio ulteriore; un test di
   comportamento sì.
5. **Controlla `scripts/wired-exceptions.txt`**: ogni nuova voce deve avere un rinvio a una fase
   futura reale con un motivo, non una scusa per zittire `check-wired.py` su codice morto.
6. **Se la fase precedente ha introdotto una convenzione** (l'involucro `BulkOutcome`, la
   tabella `operations`, `SearchNode`), controlla che questa fase la riusi invece di reinventarla
   — è esattamente il tipo di errore che il ripasso dei piani 7/8/9 contro la Fase 10 ha corretto
   nove volte prima ancora di scrivere codice.
7. **Solo a questo punto**: `git merge-tree $(git merge-base main origin/fase-N) main
   origin/fase-N` per un controllo conflitti a secco, poi `git merge --no-ff`, push, e conferma
   che la CI sia verde **anche sul commit di merge risultante su `main`**, non solo sul branch —
   non è la stessa cosa, un merge può introdurre conflitti risolti male che nessuna delle due CI
   isolate avrebbe visto.

Se qualcosa non torna a uno di questi punti, il costo di scriverlo nel ledger e sistemarlo prima
del merge è sempre più basso del costo di scoprirlo dopo, mergiato in `main`.

---

## 9. Riferimento rapido dei piani

| Fase | Piano | Task | Nota |
|---|---|---|---|
| 10 | `plans/2026-08-20-keeppix-fase-10.md` | 23 | ✅ chiusa e mergiata |
| 7 | `plans/2026-08-20-keeppix-fase-7.md` | 13 | ✅ chiusa e mergiata (UI review → 11) |
| 8 | `plans/2026-08-20-keeppix-fase-8.md` | 11 | il **Task 1 è il test** che i volti non escano dai link pubblici — scritto prima del codice che potrebbe violarlo |
| 9 | `plans/2026-08-20-keeppix-fase-9.md` | 11 | tocca **file veri**: chiudi le cinque convalide della rinomina prima |
| 11 | `plans/2026-08-20-keeppix-fase-11.md` | 18 | **Task 5bis** (ottimizzazioni client) va letto **prima** di scrivere la prima schermata: cambia la struttura del codice |

**Fatto (21 agosto):** i piani di 7, 8 e 9 — scritti prima che la Fase 10 esistesse — sono stati
ripassati contro il codice vero della Fase 10 mergiata, e corretti su nove punti reali (`Semantic`
contro l'`ORDER BY` fisso di `SearchRepo::run`, l'evento WS che riusa `OperationKind` invece di
inventarne uno, `BulkOutcome` da adattare per persone/volti, il permesso di destinazione sbagliato
nella Fase 9, l'`undo` della rinomina che non toccava il filesystem, e altri — dettagli nei commit
dei tre file). Restano a livello di task e decisioni, non di firme, ma non più scollegati dal
codice reale.
