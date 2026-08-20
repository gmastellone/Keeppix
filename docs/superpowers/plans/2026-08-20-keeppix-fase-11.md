# Piano — Fase 11: Interfaccia

**Specifica:** `docs/superpowers/specs/fase-11-interfaccia.md`
**Fonte di verità del comportamento:** `docs/ui/keeppix-mockup.html` (prototipo interattivo).
**Fonte di verità della descrizione:** `docs/ui/documento-funzionale-ui.md`.
**Marchio:** `docs/ui/brand-sheet.png`.
**Branch:** `fase-11` (in quattro tranche, vedi §Tranche).

---

## Cosa esiste già (verificato, non assunto)

- **Il frontend non è vuoto.** Vue 3.5, Pinia 4, `vue-router` 5, `vue-i18n` 11, Tailwind 4,
  Vite 8, Vitest 4, `reka-ui` 2. Sedici viste già scritte — Timeline, Culling, Map, Search,
  Albums, Shares, Trash, Users, Groups, Problems, Setup, Login, Player, BatchEdit, Folders,
  ShareTarget — diverse con i loro `*.spec.ts`.
- **Componenti esistenti da riusare, non rifare:** `AssetViewer`, `Filmstrip`, `RatingStars`,
  `PlacePicker`, `MapClusterLayer`, `UploadPanel`, `SharePanel`, più `components/ui/`.
- **`reka-ui` è già una dipendenza**, ed è la ragione per cui non serve nessuna libreria di
  componenti: dialog, menu, popover, tooltip e switch con focus trap e ARIA corretti.
- **Il budget di bundle è già un test in CI**: 150 KB gzip per gli asset che `index.html` carica
  subito (`.github/workflows/ci.yml`, job `frontend`). Non va allentato.
- **`maplibre-gl` e `hls.js` sono già isolati** in chunk per-rotta via `import()` pigro in
  `src/router.ts`: da soli sfonderebbero il budget tre volte.
- **`POST /api/v1/viewport` esiste già** e serve esattamente a promuovere la generazione delle
  miniature che l'utente sta guardando. Va **usato**, non reinventato.

---

## Il vincolo che governa tutto il resto

> *«Il prototipo dice cosa deve fare ogni comando; non è un modello di come esporlo.»*

Il documento è onesto su un punto e va preso alla lettera: **l'accessibilità da tastiera del
prototipo è sistematicamente rotta e non va replicata.** Convivono tre livelli senza criterio —
comandi completi, comandi attivabili ma non raggiungibili con Tab, comandi solo cliccabili — e
interi blocchi ricadono nell'ultimo: quasi tutta la navigazione, quasi tutto il culling, il
Cestino (*«da sola tastiera è di sola lettura»*), le Condivisioni, le schede album, le schede
persona, i pin della mappa.

**Ruling: ogni cosa cliccabile è un pulsante vero.** — Raggiungibile con Tab, attivabile con
Invio e Spazio, con anello di focus visibile; i dialog intrappolano il focus e lo restituiscono
al trigger; Esc chiude ovunque; i `radiogroup` hanno roving tabindex **e** frecce. Costruirlo
una volta sola nei componenti condivisi costa meno che replicare la versione rotta. — *Costo se
sbagliato:* è l'unico punto in cui si diverge deliberatamente dal prototipo, e va scritto nel
ledger perché non sembri una svista.

---

## Tranche

Le schermate si consegnano seguendo le fasi da cui dipendono, non tutte insieme.

| Tranche | Contenuto | Dipende da |
|---|---|---|
| **A** | Fondamenta: pattern condivisi, shell, router, timeline a scala reale | Fase 10 |
| **B** | Il grosso delle schermate: libreria, dettaglio, ricerca, mappa, condivisioni, album, manutenzione, impostazioni | Fase 10 |
| **C** | Tag, Revisione, Analisi libreria, semantica in Cerca | Fase 7 |
| **D** | Persone e volti · Culling e rinomina | Fasi 8 e 9 |

---

## Tranche A — Fondamenta

### Task 1 — I token di stile e le due mappe di tema

Dal prototipo, tre cose vanno estratte **come token** e non ricopiate a mano schermata per
schermata:

- **Palette dei tempi**, misurata sul documento e sorprendentemente piccola: `.12s` (54
  occorrenze — tooltip, comparsa dei comandi sulla tessera), `.2s` (53 — toast, transizioni
  generiche), `.15s` (14 — rotazione della freccia dei gruppi), `.25s` (2 — cambio di tema),
  più `.1s`/`.18s`/`.3s` in cinque casi isolati. Curva `ease` in **108 casi su 111**.
  **Tre valori coprono il 92% delle animazioni**: sono token, non numeri da riscrivere.

- **Ritardi che non sono animazioni** e che vanno rispettati alla lettera: toast a **10 ms**
  di ritardo, visibile **2400 ms** (successo) / **4,2 s** (errore e riuscita parziale) /
  **6,5 s** se ha un'azione — e in quest'ultimo caso **il timer si ferma mentre il puntatore è
  sopra**, senza il quale sparirebbe proprio mentre si decide se premerlo; rimozione dal DOM
  dopo **250 ms**; tocco prolungato mobile a **500 ms** con **vibrazione di 15 ms**;
  pulsazione dell'indicatore di analisi a **1,4 s**.

  I **700 ms** che compaiono nel prototipo sono scaffolding (ritardo simulato fra avvio ed
  esito), ma il documento nota che *«durante i 700 ms si può premere di nuovo, e il codice non
  lo impedisce»*: nel prodotto vero quel caso è coperto da **SP-30**, il pulsante occupato.
- **Colori semantici**, che nel documento significano qualcosa e non sono decorazione: il verde
  `#2E9E5B` è **lo stesso** per "Scelta" nel culling e per "In linea" nel piede utente — scelta
  deliberata per non introdurre una terza tonalità; il rosso è riservato a "elimina dal disco" e
  ai badge numerici.
- **L'accento non è mai uno sfondo di voce selezionata** (SP-26): "sei qui" è bordino verticale
  a sinistra più fondo tenue; l'accento pieno è riservato ai badge.

Aggiungere `prefers-reduced-motion`: il documento lo cita una volta sola, ma con animazioni
dichiarate ovunque va gestito centralmente.

**Verifica:** un test che nessun componente dichiari una durata fuori dalla palette.

### Task 2 — I trenta pattern condivisi come componenti

Vanno **prima** delle schermate: il documento lo dice esplicitamente — *«costruirli prima delle
schermate evita che dodici viste divergano ognuna per conto suo»*.

| Componente | Pattern | Note vincolanti |
|---|---|---|
| `PhotoTile` | SP-1, SP-15 | tre stop di tabulazione (apri → spunta → cuore); `:focus-within` rivela i comandi come l'hover — è voluto |
| `SelectionBar` + store | SP-2 | **due pool distinti e paralleli**: libreria e lotto di culling. Non si parlano |
| `QuickFilter` | SP-3 | OR dentro una dimensione, AND fra dimensioni |
| `SelectAllVisible` | SP-4 | seleziona **ciò che è visibile**, non la libreria; **scompare** quando non c'è nulla, non si disabilita |
| `Dialog` (su `reka-ui`) | SP-5 | focus trap **sì** (il prototipo non ce l'ha), focus alla prima opzione, ritorno al trigger, Esc chiude |
| `ToastHost` | SP-6, SP-28, SP-29 | tre nature: successo neutro 2,4 s · errore 4,2 s · riuscita parziale 4,2 s. **Con azione: 6,5 s e il timer si ferma al passaggio del mouse** |
| `Tooltip` | SP-7 | `.12s`, nessun ritardo, **disattivato su mobile** |
| `RatingStars` (esiste) | SP-9, SP-20 | ricliccare la stella attiva **azzera** |
| `SuggestionQueue` | SP-10 | tag e volti, stessa forma |
| `ProvenanceBadge` | SP-12 | IA e umano mai indistinguibili, in nessun punto |
| `Avatar` | SP-16 | iniziali **sempre bianche**; colore dell'utente corrente da preferenze, sincronizzato ovunque |
| `AppShell` | SP-17 | commuta **per larghezza**, non per interruttore |
| `DeleteDialog` | SP-18 | **focus sulla prima opzione, la meno distruttiva** — deliberato: chi preme Invio d'istinto fa la cosa innocua |
| `ConfirmDialog` | SP-5 | focus su **"Annulla"**, stessa filosofia |
| `SegmentedControl` | SP-24 | roving tabindex **e frecce** (il prototipo non le ha); nei filtri della modifica in blocco include sempre `"Non modificare"` |
| `NavGroup` | SP-25 | freccia che ruota in `.15s`; si apre da solo e **non si chiude** se la vista corrente è dentro |
| `BusyButton` | SP-30 | mantiene la dimensione, blocca il doppio invio |
| `LoadingSkeleton` | SP-27 | ha **la forma del contenuto**, mai uno spinner centrato |
| `Popover` (su `reka-ui`) | SP-14 | click fuori chiude, **Esc chiude** (il prototipo lo fa solo a metà), Esc **a livelli** quando è annidato |

**I ventiquattro dialog, menu e popover del documento si costruiscono sopra due soli
componenti** — `Dialog` (SP-5) e `Popover` (SP-14) — non uno per uno:

- **menu a comparsa (6):** account desktop · account mobile · «altre azioni» ⋯ del lightbox ·
  selettore rapido di lotto · menu sul riquadro del volto · popover della mappa · picklist
  della creazione album;
- **dialog modali (18):** cartella radice di culling · imposta posizione · ricerca di regione ·
  condividi selezione · scegli copertina · assegna a gruppo · unisci persone · separa persona ·
  selettore di persona · selettore di tag · aggiungi ad album · file con problemi ·
  **eliminazione a 3 opzioni** · informazione · conferma · modifica tag · modifica categoria ·
  rinomina con formula · inserimento testo generico.

Due eccezioni deliberate da **preservare**, non da uniformare: nel dialog di eliminazione il
focus va sulla **prima opzione, la meno distruttiva**; nel dialog di conferma va su
**"Annulla"**. Chi preme Invio d'istinto compie l'azione innocua.

**Verifica:** un `*.spec.ts` per componente, con le etichette esatte asserite.

### Task 3 — Router: ogni schermata ha un indirizzo

Il prototipo non ha rotte; il documento lo marca come *«decisione da prendere consapevolmente,
non da ereditare»*, e indica il caso che decide: *«mando a un collega il link a questa foto»*.

1. Una rotta per vista, con i parametri nell'URL: cartella, album, persona, lotto, **foto aperta
   nel lightbox**, e i filtri attivi.
2. Le regole di ripristino di §7 del documento vanno rispettate **alla lettera**, comprese le
   incoerenze dichiarate: cliccare una voce di sidebar azzera i filtri rapidi, cliccare una
   cartella no. Non sono sviste da correggere qui — sono comportamento documentato, e cambiarle
   è una decisione di prodotto separata (è la domanda aperta n.9).
3. Ripristino della posizione di scorrimento tornando in una vista (il prototipo non ce l'ha).

**Verifica:** test che il tasto Indietro del browser funziona; test che ricaricare la pagina su
un album aperto lo riapre.

### Task 4 — La timeline a scala reale

È **il calcolo più delicato dell'intera fase**.

1. Consumare `GET /timeline/geometry` (Fase 10) in un `ArrayBuffer`, letto con una `DataView`
   incapsulata in una classe di ~30 righe. **Mai** 214.000 oggetti JavaScript: costerebbero
   ~50 MB di heap e metterebbero sotto pressione il GC a ogni scroll.
2. Layout giustificato: accumulare scatti finché la somma dei rapporti d'aspetto per l'altezza
   obiettivo supera la larghezza, poi scalare la riga perché la riempia esattamente.
   **L'ultima riga non si stira.** Deterministico, `O(n)`, nessuna misura del DOM.
3. Virtualizzazione: somme prefisse delle altezze di riga più ricerca binaria su `scrollTop`.
   ~120 righe, **nessuna libreria** — vedi il Ruling nella spec §2.
4. Ricalcolo su `ResizeObserver` e al cambio di densità, **mai** su `scroll`.
5. Barra dei mesi (scrubber): tick equidistanti per mese presente, targhetta col mese esteso
   durante il trascinamento, sincronizzazione inversa allo scroll. **Da rendere raggiungibile da
   tastiera**, cosa che il prototipo non fa.
6. Miniature solo per le righe visibili più una schermata di margine, con `IntersectionObserver`,
   `loading="lazy"`, `decoding="async"`, e `POST /viewport` per la priorità di generazione.
7. **Il primo fotogramma non scarica nessuna miniatura.** `AssetView` porta già `thumbhash`
   (`routes/timeline.rs:56`), l'impronta da ~25 byte da cui si ricostruisce un'anteprima
   sfocata: le tessere si dipingono subito da quella, arrivata con la pagina, e le miniature
   vere la sostituiscono man mano. Su una griglia da 60 tessere sono **60 richieste tolte dal
   percorso critico**, non rimandate.

   È l'altra metà della richiesta n.1: la geometria dà le **proporzioni** prima di disegnare,
   `thumbhash` dà il **colore**. Insieme il primo fotogramma è completo e corretto senza aver
   scaricato una sola immagine.

**Verifica:** test su una geometria finta da 200.000 record che il numero di tessere montate
resta sotto una soglia esplicita durante uno scroll simulato; test che l'altezza totale calcolata
combacia con la somma dei `count` di `/timeline/buckets`.

### Task 5 — Le tre macchine a stati

Ogni insieme di dati ha tre stati: in caricamento, pronto, errore. **Nessuna schermata assume
che i dati ci siano.**

- Lo scheletro ha la forma del contenuto (SP-27). Per la timeline questo è gratis: la geometria
  arriva prima delle miniature, quindi lo scheletro è già nella posizione e nelle proporzioni
  giuste. È il vantaggio diretto della richiesta #1.
- L'errore dice tre cose **in quest'ordine**: cosa non è riuscito, **cosa non è successo** (i
  file sono intatti), come riprovare. Mai "qualcosa è andato storto". Tre forme: a piena vista,
  in riga, come messaggio temporaneo (SP-28).
- `"Riprova"` compare **solo** per `unreachable` e `permission-denied` (Fase 10 §7).
- La riuscita parziale ha una schermata sua (SP-29): dichiara i numeri veri e offre di ritentare
  **solo ciò che è rimasto indietro**.

**Verifica:** un test per natura di errore che asserisce presenza/assenza di "Riprova".

---

## Tranche B — Il grosso delle schermate

Un task per blocco del documento, ognuno chiuso confrontando la schermata col prototipo aperto
di fianco:

- **Task 6** — Shell desktop e mobile, sidebar, topbar, menu account, pagina "Altro".
- **Task 7** — Foto/Timeline (composizione finale), Preferiti, filtro rapido, selezione multipla,
  modifica in blocco.
- **Task 8** — Lightbox, pannello informazioni, menu ⋯. Scorciatoie: `Esc` **a due livelli**
  (prima chiude il menu ⋯, poi il lightbox), `←` `→`, `i`, `f`.
- **Task 9** — Cerca: barra, pillole (SP-19), risultati, ricerche salvate. Da colmare rispetto al
  prototipo: Invio promuove il testo a pillola, Backspace su campo vuoto rimuove l'ultima
  pillola, `↑`/`↓` fra i suggerimenti — tutte **non implementate** nel prototipo e dichiarate
  come lacune.
- **Task 10** — Mappa e popover, dialog posizione. Il prototipo qui non ha **nessuna**
  accessibilità da tastiera: va costruita da zero.
- **Task 11** — Condivisioni e dialog di condivisione. *«La schermata meno accessibile del
  blocco: da ricostruire con elementi nativi.»*
- **Task 12** — Album: griglia, dettaglio, creazione (manuale **e** dinamico), aggiungi ad album.
- **Task 13** — Manutenzione: Cestino, Duplicati, Problemi, dialog file con problemi. Il Cestino
  è *«di sola lettura da tastiera»* nel prototipo: da rifare.
- **Task 14** — Impostazioni e Profilo. **Nessun "Salva"** in nessuna pagina di preferenze
  (SP-23): ogni modifica è immediata. Unica eccezione: "Dati account" del Profilo.

---

## Tranche C — Fase 7

**Task 15** — Tag e categorie, dialog modifica tag (con la **soglia per tag**) e categoria,
selettore di tag, Revisione–tag (SP-10), Analisi libreria, i tre livelli IA (SP-11), provenienza
IA/utente (SP-12), la semantica in Cerca.

## Tranche D — Fasi 8 e 9

**Task 16** — Persone: griglia, dettaglio, copertina, gruppo, unisci, separa, selettore persona,
menu sul riquadro del volto, Revisione–volti.

**Task 17** — Culling: griglia lotti, lotto aperto, selettore di lotto, dialog cartella radice,
dialog "Rinomina con formula". Scorciatoie del lotto aperto, esatte:

| Tasto | Effetto |
|---|---|
| `←` `→` | **azzerano la selezione multipla**, poi spostano di uno. Non tornano in cerchio |
| `Shift+←` `Shift+→` | intervallo dall'ancora; **ricalcolato ogni volta, non additivo** |
| `P` | Scelta; ripremuto sulla stessa foto la riporta a "da valutare" |
| `X` e `Canc` | Scarta (identici); ripremuti riportano a "da valutare" |
| `1`–`5` | valutazione; **lo stesso numero ripremuto azzera** (SP-20) |

`P`, `X`, `1`–`5` agiscono **solo sulla foto corrente**, mai sulla selezione.

**Ruling: le scorciatoie non si attivano se il focus è in un campo di testo o se un dialog è
aperto.** — Nel prototipo è un difetto vero e dichiarato: con un lotto aperto, digitare `1` in un
campo cambia la valutazione della foto sottostante, `p` e `x` la scelgono o la scartano. Solo il
lightbox è schermato correttamente. — *Costo se sbagliato:* l'utente modifica dati mentre crede
di scrivere.

---

## Cosa va disegnato perché il backend ce l'ha e l'interfaccia no

Il documento dichiara che *«l'intero disegno assume fotografie»*, ma la Fase 6 ha spedito la
pipeline **video** completa (probe, HLS, poster, player). Stesso caso per: importazione iniziale
di una libreria, registro di controllo, app-password, vista pubblica di un link condiviso.

**Ruling: si disegnano seguendo i pattern esistenti, non inventando un linguaggio nuovo.** — Un
video è una tessera con badge di durata invece del badge RAW (SP-15 ha già la forma); la vista
pubblica di un link è la griglia con la barra di selezione tolta; l'importazione è una pagina di
impostazioni (SP-23) più un avanzamento (Fase 10 Task 16). Nessuno di questi richiede un
componente nuovo. — *Costo se sbagliato:* si aggiungono schermate non validate dal disegno; vanno
mostrate prima di darle per buone.

---

## Chiusura della fase

- Ogni schermata del documento esiste, e le **etichette combaciano alla lettera** (sono asserite
  nei test tramite i file di traduzione).
- Bundle iniziale sotto i 150 KB gzip: il test in CI c'è già.
- `vue-tsc --noEmit`, `vitest run`, build, e CI reale verde.
- Documenti aggiornati: `docs/superpowers/README.md`, `docs/CONTINUE.md`, roadmap.
- Ledger `.superpowers/sdd/2026-08-20-keeppix-fase-11/progress.md`, con un `Ruling:` per ogni
  punto in cui si è divergiuto deliberatamente dal prototipo — soprattutto in accessibilità.
