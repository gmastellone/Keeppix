# Keeppix — Documento funzionale UI

**Versione:** 1.0 — 20 agosto 2026
**Stato:** disegno d'interfaccia concluso e verificato su prototipo interattivo. Nessuna riga di
questo documento descrive funzionalità non prototipate: dove il prototipo non copre un caso, il
documento lo dichiara invece di riempirlo.

---

## A chi serve questo documento

Ha due lettori, con due bisogni diversi, e prova a servirli con lo stesso testo.

**All'architetto che lavora sul backend reale.** Serve a confrontare, punto per punto, quello che
l'interfaccia chiede con quello che il sistema già fa o ha già in programma. Per questo ogni
schermata si chiude con un paragrafo **"Dati necessari a questa schermata"** scritto in termini
di *cose*, non di endpoint: "l'elenco delle foto del lotto, con miniatura, nome file, stato
scelto/scartato/da valutare e valutazione 0–5", non `GET /api/v1/batches/{id}/photos`. La forma
che quei dati prenderanno sul filo è una decisione dell'architetto, non di questo documento. Il
paragrafo distingue sempre ciò che la schermata **legge** da ciò che **scrive**, perché è la
seconda metà quella che di solito costa.

**Allo sviluppatore frontend che deve costruire l'interfaccia.** Serve come inventario esecutivo:
ogni pulsante con la sua etichetta esatta, ogni scorciatoia, ogni stato disabilitato con la
ragione per cui è disabilitato in quel momento, ogni durata di transizione. È volutamente noioso
e ripetitivo. Il livello di dettaglio non è pedanteria: è la forma in cui i dettagli mancanti si
scoprono adesso invece che a integrazione fatta.

## Le otto richieste che toccano il backend

Il resto del documento descrive l'interfaccia, e quasi tutto quello che contiene il backend può
ignorarlo. **Otto punti no.** Sono quelli in cui l'interfaccia impone davvero una forma ai dati o
alle risposte, e in cui scoprirlo tardi costa caro.

Stanno qui in fondo a una pagina, all'inizio, invece che sparsi nelle sezioni dove logicamente
appartengono — perché lì un architetto li troverebbe per ultimi, o non li troverebbe affatto.
Ognuno rimanda al punto dove è spiegato per esteso.

| # | Richiesta | Perché | Dove |
|---|---|---|---|
| 1 | **Le proporzioni di tutti gli scatti di una vista, senza caricarne le miniature.** Per ogni foto: identificativo, larghezza e altezza (o il rapporto), mese. Nient'altro. | È ciò che permette alla timeline di conoscere la propria geometria — e quindi la propria altezza — prima di aver disegnato un pixel. Senza questa separazione fra "proporzioni di tutto" e "immagini di poco", il disegno della timeline va ripensato da capo. | Parte X, §*La timeline a scala reale* |
| 2 | **Le operazioni di massa non possono rispondere "fatto" o "non fatto":** serve l'elenco di cosa è riuscito e di cosa no, **foto per foto**, con una ragione per ogni fallimento. | Su un'operazione che tocca centinaia di file, la riuscita parziale è l'esito **più probabile**. Senza questi elenchi l'interfaccia non può dire i numeri veri né ritentare solo le rimanenti, e ricade per forza in una bugia: "fatto" nascondendo 183 foto non toccate, o "errore" facendo rifare tutto. Ha conseguenze sulla forma delle risposte e forse sulle transazioni. | Parte X, §*Riuscita parziale* |
| 3 | **Il conteggio reale di foto per mese**, come dato aggregato indipendente dall'elenco caricato. | L'intestazione di ogni mese dichiara il totale vero, che non deriva da ciò che è stato caricato. | Parte X, §*La timeline a scala reale* |
| 4 | **Una foto è una pila di file, non un file.** RAW e JPEG affiancati sono **un solo scatto**, non due foto. | Attraversa tutto il modello: conteggi, selezione, eliminazione, rinomina, e la scelta di quale dei due guardare nel dettaglio. | **SP-15** |
| 5 | **La provenienza di ogni etichetta va conservata**, non dedotta: chi ha assegnato questo tag, il riconoscimento automatico o una persona? | L'interfaccia non confonde mai le due cose, in nessun punto. È un principio di prodotto prima che un dettaglio visivo. | **SP-12** |
| 6 | **Eliminare ha tre destinazioni distinte** — solo dall'indice, nel cestino di Keeppix, dal disco — e **nessun comportamento predefinito implicito**. | Keeppix chiede sempre quale delle tre, perché le conseguenze sono incomparabili: la prima si annulla da sola alla scansione successiva, l'ultima è irreversibile. | **SP-18** |
| 7 | **Distinguere almeno quattro nature di fallimento**: server irraggiungibile, permessi mancanti, file assente, tempo scaduto. | `"Riprova"` ha senso nei primi due e non negli altri due, e il messaggio giusto cambia in ognuno. Il prototipo ne distingue una sola: è una semplificazione da non ereditare. | Parte X, §*Errore* |
| 8 | **I volti sono dati biometrici** e non compaiono **mai** su un link pubblico condiviso. Non è configurabile. | È l'unica regola del documento dichiarata come non negoziabile, e va garantita dove i link pubblici vengono serviti — non solo nell'interfaccia. | Parte IX, §*Impostazioni* |

Il punto 1 e il punto 2 sono quelli da verificare per primi: se uno dei due non fosse realizzabile,
cambia il disegno, non l'implementazione.

## Che cos'è la fonte di questo documento

Tutto il contenuto è stato estratto da un **prototipo interattivo funzionante** — un singolo file
HTML/CSS/JavaScript autonomo, senza framework, costruito e rivisto iterativamente. Il prototipo
non è un mockup grafico statico: le schermate sono navigabili, i filtri filtrano davvero, le
scorciatoie da tastiera funzionano, i dialog si aprono e si chiudono. Le durate di animazione
citate in questo documento sono le durate reali scritte nel CSS del prototipo, non intenzioni.

Questo ha una conseguenza importante e va detta subito: **il prototipo è la fonte di verità del
comportamento, non un'illustrazione di questo testo.** Dove i due divergessero, ha ragione il
prototipo — ed è un difetto da correggere qui.

Ha anche il rovescio della medaglia: il prototipo contiene sia scelte deliberate sia scorciatoie
prese per andare avanti. La sezione finale **"Assunzioni e domande aperte"** separa le une dalle
altre in modo esplicito. Non è un'appendice di cortesia: è la parte del documento con il rapporto
più alto fra righe lette e problemi evitati, e va letta per prima da entrambi i lettori.

### Dove aprire il prototipo

Il prototipo è pubblicato come artefatto su claude.ai con l'identificativo
**`keeppix-mockup-ui`** ("Keeppix — Mockup UI"), ed è anche distribuito come singolo file
`index.html`: si apre con un doppio click in qualunque browser, senza server, senza dipendenze e
senza connessione.

**Vale la pena tenerlo aperto accanto a questo documento.** Ogni sezione descrive una schermata
che si può raggiungere e provare: le scorciatoie da tastiera elencate funzionano davvero, gli
stati disabilitati sono davvero disabilitati, e le durate di animazione citate sono quelle che si
vedono. Leggere le sezioni 4, 5 e 6 (mouse, tastiera, animazioni) senza avere lo schermo davanti è
possibile ma spreca metà del lavoro.

Due avvertenze per chi lo apre:

- **In cima alla pagina c'è uno switch Desktop / Mobile.** Non è un elemento del prodotto: serve a
  rivedere le due impaginazioni senza ridimensionare la finestra. Nel prodotto vero il passaggio
  avviene per larghezza dello schermo.
- **In fondo a Impostazioni c'è il pannello "Anteprima stati"**, con tre interruttori spenti di
  partenza. Servono a far comparire gli stati di caricamento, errore e riuscita parziale, che
  altrimenti non si vedrebbero mai — nel prototipo i dati sono già in memoria e ogni operazione
  riesce all'istante. Anche questo è scaffolding, non prodotto: vedi la **Parte X**.

I dati dentro il prototipo sono inventati: circa 900 scatti finti, tre cartelle, alcuni lotti di
culling, persone e album di comodo. Servono a dare volume realistico alle schermate, non a
rappresentare una libreria vera — nomi di file, luoghi e conteggi non vanno letti come requisiti.

## Cosa questo documento non è

- **Non è una specifica di API.** Non contiene endpoint, verbi HTTP, forme di payload, codici di
  errore. Vedi sopra.
- **Non è un design system.** Non contiene la scala tipografica, la griglia spaziale, la palette
  completa o le regole del marchio: quelle vivono nel documento di brand a parte. Qui i colori
  compaiono solo dove *significano* qualcosa (il verde di "scelta", il rosso di "elimina dal
  disco") o dove una durata di transizione comunica un cambio di stato.
- **Non è un documento di requisiti di prodotto.** Non argomenta *perché* Keeppix debba avere il
  culling. Assume che la decisione sia presa e descrive com'è fatto.
- **Non copre l'importazione iniziale né l'amministrazione del server** (utenti multipli, backup,
  aggiornamenti): non sono state disegnate in questa fase.

## Nota sullo stack di destinazione

Il prototipo è scritto senza framework — un unico file, stato in un oggetto globale, e un
`renderAll()` che ridisegna la vista corrente da stringhe di template. **Questa architettura non
va replicata.** È stata scelta perché rende il prototipo apribile con un doppio click e
modificabile in un'unica passata, e va letta come una descrizione del *comportamento*, non
dell'implementazione.

Il frontend reale di Keeppix è in **Vue.js**. Alcune conseguenze pratiche che vale la pena
anticipare, perché ricorrono in tutto il documento:

- Dove il prototipo dice "a ogni render la funzione riscrive il DOM della sezione", in Vue si
  legge semplicemente **stato reattivo**: il documento descrive *cosa deve essere vero dopo il
  cambiamento*, non come arrivarci.
- Le regole di reattività sono però significative. Diversi comportamenti descritti qui
  (la sincronizzazione del colore dell'avatar su tre punti diversi dell'interfaccia, il
  ricalcolo del layout giustificato delle griglie, la barra di selezione che sostituisce la
  toolbar) nel prototipo sono espliciti perché *devono* esserlo; in Vue diventano quasi gratuiti.
  Dove il prototipo ha una scorciatoia dovuta all'assenza di reattività, il documento lo segnala.
- **Il layout giustificato delle griglie fotografiche** (righe ad altezza comune, larghezza
  proporzionale al lato lungo dello scatto) è calcolato in JavaScript, e insieme a esso è
  calcolata la **virtualizzazione** della timeline. È il punto in cui il frontend reale dovrà fare
  il lavoro più delicato — CSS da solo non basta — ed è documentato per intero nella **Parte X**.
- **Non esiste routing basato su URL nel prototipo**: lo stato della vista è una proprietà in
  memoria, il tasto "indietro" del browser non funziona e nessuna schermata è indirizzabile con un
  link. Nel frontend reale questa è una decisione da prendere consapevolmente, non da ereditare —
  vedi le domande aperte.
- **Gli stati di caricamento, errore e riuscita parziale sono disegnati e prototipati**, ma nel
  prototipo vanno accesi a mano dal pannello "Anteprima stati" in fondo a Impostazioni, perché con
  i dati già in memoria non si verificherebbero mai da soli. Anche questo sta nella **Parte X**.
  Nelle sezioni delle singole schermate il sottotitolo 7 riporta ancora, in diversi punti, che uno
  stato di caricamento "non è implementato": va letto come "quella schermata non ha ancora un suo
  scheletro dedicato", non come "l'app non ha stati di caricamento".

## Come leggere una sezione

Ogni schermata, vista o dialog ha una sezione con lo stesso scheletro fisso, sempre nove
sottotitoli nello stesso ordine. Lo scheletro è ripetitivo apposta: rende il documento
confrontabile punto per punto invece che leggibile una volta sola.

| # | Sottotitolo | Cosa ci trovi |
|---|---|---|
| 1 | **Nome e scopo** | Una frase. |
| 2 | **Cosa mostra** | Ogni singolo dato visibile, elemento per elemento. |
| 3 | **Ogni controllo, uno per uno** | Inventario: etichetta esatta, tipo, effetto, validazione. |
| 4 | **Interazioni da mouse** | Click, doppio click, tasto destro, hover, trascinamento, scroll — comprese quelle **non** implementate, dichiarate come tali. |
| 5 | **Interazioni da tastiera** | Ogni scorciatoia tasto per tasto, ordine del focus, Invio, Escape, modificatori. |
| 6 | **Animazioni e transizioni** | Cosa si anima, cosa lo innesca, cosa comunica, con durata e curva. |
| 7 | **Stati per ogni controllo** | Normale, hover, focus, premuto, **disabilitato e perché**, caricamento, errore, vuoto. |
| 8 | **Da dove ci si arriva e dove si va** | La navigazione in ingresso e in uscita. |
| 9 | **Dati necessari a questa schermata** | Cosa la schermata legge e cosa scrive, in termini di cose. |

Le etichette dell'interfaccia sono sempre riportate **alla lettera e fra virgolette**:
`"Svuota scartati"` è il testo che compare a schermo, non una parafrasi.

I comportamenti ricorrenti in più schermate sono descritti **una volta sola** in
[§ Pattern condivisi](#pattern-condivisi-fra-più-schermate) e richiamati altrove con un codice
(**SP-1**, **SP-2**, …). Quando una schermata devia dal pattern, la deviazione è scritta lì e
solo lì. Se leggi "SP-2, ma senza il pulsante Elimina", il resto di SP-2 vale come scritto.

## Glossario minimo

Sei termini che nel documento hanno un significato preciso e non intercambiabile.

- **Culling** — la prima passata su uno scarico di schede: guardare gli scatti appena importati
  uno per uno e decidere quali tenere. È un'attività distinta dallo sfogliare la libreria, e ha
  una sezione dell'app tutta sua.
- **Lotto** — l'unità di lavoro del culling: una cartella di scatti importati insieme (una
  sessione, una giornata, una scheda). Il culling è organizzato per lotti, non per cartelle della
  libreria.
- **Presa / Scartata / Da valutare** — i tre stati di una foto dentro un lotto. Sono stati del
  *culling*, non della libreria: una foto "scartata" in un lotto non è una foto eliminata.
- **Indice / disco** — distinzione centrale in tutta l'app. L'*indice* è ciò che Keeppix sa delle
  foto; il *disco* sono i file veri. Molte azioni distruttive chiedono esplicitamente a quale
  dei due applicarsi, e Keeppix non ha mai un comportamento predefinito implicito su questo punto
  (vedi **SP-18**, il dialog di eliminazione a tre opzioni).
- **Tag / Categoria / Scena** — tre cose diverse. Una **scena** è ciò che il riconoscimento
  automatico crede di vedere ("tramonto", "montagna"); un **tag** è un'etichetta della libreria;
  una **categoria** raggruppa i tag. Vedi la sezione Tag e categorie.
- **Suggerito / Confermato** — un'etichetta proposta dal riconoscimento automatico non è
  equivalente a una messa da una persona, e l'interfaccia non le confonde mai (**SP-12**).

## Indice delle schermate


**Struttura dell'applicazione**

- **1** · Struttura generale della finestra (shell desktop: sidebar + topbar + area contenuto)
- **2** · Sidebar di navigazione
- **3** · Menu account
- **4** · Barra superiore / breadcrumb
- **5** · Shell mobile: header, tab bar in basso, menu account mobile
- **6** · Pagina "Altro" / Libreria su mobile
- **7** · Router e regole di ripristino dello stato quando si cambia sezione

**Sfogliare la libreria**

- **8** · Foto / Timeline
- **9** · Preferiti
- **10** · Il tile fotografico
- **11** · Filtro rapido a chip
- **12** · Selezione multipla e barra azioni
- **13** · Modifica in blocco

**Culling**

- **14** · Culling — scelta del lotto (la griglia dei lotti)
- **15** · Culling — lotto aperto (la schermata di valutazione)
- **16** · Il selettore rapido di lotto
- **17** · Dialog "Scegli la cartella radice di culling"

**Vista dettaglio**

- **18** · Lightbox — struttura e barra superiore
- **19** · Pannello informazioni (pannello laterale destro)
- **20** · Menu "altre azioni" (⋯)
- **21** · Differenze fra lightbox aperto da libreria e lightbox aperto da un lotto di culling
- **22** · Note su mobile (`#app.device-mobile`)

**Ricerca, mappa, condivisione**

- **23** · Cerca — la barra e i suggerimenti
- **24** · Cerca — i filtri strutturati (le "pillole" nella barra)
- **25** · Cerca — l'area dei risultati
- **26** · Mappa
- **27** · Popover della mappa (quando si clicca un gruppo di foto)
- **28** · Dialog "Imposta posizione" / ricerca di regione
- **29** · Condivisioni
- **30** · Dialog "Condividi selezione"

**Persone e volti**

- **31** · Persone — la griglia
- **32** · Persone — dettaglio di una persona
- **33** · Dialog "scegli copertina"
- **34** · Dialog "assegna a gruppo"
- **35** · Dialog "unisci persone"
- **36** · Dialog "separa persona"
- **37** · Selettore di persona (usato per assegnare un volto)
- **38** · Menu sul riquadro del volto
- **39** · Revisione — volti (la coda di conferma dei volti suggeriti)
- **40** · Riferimenti ai pattern condivisi usati in questo blocco

**Album e manutenzione**

- **41** · Album — la griglia
- **42** · Album — dettaglio
- **43** · Creazione di un album
- **44** · Dialog "Aggiungi ad album"
- **45** · Cestino
- **46** · Duplicati
- **47** · Problemi
- **48** · Dialog "file con problemi"
- **49** · Dialog di eliminazione a 3 opzioni
- **50** · Dialog generici riutilizzati: informazione e conferma

**Organizzazione automatica**

- **51** · Premessa al modello dati: tag, categoria e scena sono tre cose diverse
- **52** · Tag e categorie — la pagina
- **53** · Dialog "modifica tag"
- **54** · Dialog "modifica categoria"
- **55** · Selettore di tag (assegnare tag a delle foto)
- **56** · Revisione — tag (coda di conferma; definizione canonica di SP-10)
- **57** · Analisi libreria
- **58** · I livelli IA "Pieno" / "Ridotto" / "Spento"
- **59** · Provenienza IA vs utente

**Preferenze e organizzazione dei file**

- **60** · Impostazioni
- **61** · Profilo
- **62** · Dialog "Rinomina con formula"
- **63** · Dialog generico di inserimento testo
- **64** · Appendice — Dialog "Scegli la cartella radice di culling" (dal punto di vista dell'impostazione)

**Scala, caricamento ed errore**

- **65** · Perché questa parte esiste
- **66** · La timeline a scala reale
- **67** · Caricamento
- **68** · Errore
- **69** · Riuscita parziale
- **70** · Il pannello "Anteprima stati"

**Pattern condivisi fra più schermate**


**Assunzioni e domande aperte**



**In fondo**

- Pattern condivisi fra più schermate (SP-1 … SP-30)
- Assunzioni e domande aperte

---

# Parte I — Struttura dell'applicazione

> Tutto ciò che segue è letto da `/home/claude/keeppix/index.html` (CSS righe 40–1190, JS righe
> 1190–6355). Le etichette sono riportate alla lettera. Dove il mockup non implementa qualcosa,
> è scritto esplicitamente.
>
> Pattern condivisi richiamati per codice: **SP-6** (toast), **SP-7** (tooltip `[data-tip]`),
> **SP-8** (`bindActivatable`), **SP-14** (menu a comparsa), **SP-16** (avatar), **SP-17**
> (shell mobile).

---

## 1. Struttura generale della finestra (shell desktop: sidebar + topbar + area contenuto)

### 1. Nome e scopo
È il telaio fisso dell'applicazione: una colonna di navigazione a sinistra, una barra superiore
con briciole di pane e ricerca, e un'unica area di contenuto in cui viene montata la vista
corrente.

### 2. Cosa mostra
Dall'esterno verso l'interno:

- **`.wrap`** — contenitore di pagina largo al massimo `1220px`, centrato. È la pagina di
  presentazione del mockup, non l'app.
- **Barra di scelta dispositivo** (`.device-switch-bar` → `.seg-control#deviceSwitch`) —
  sopra la finestra, centrata, `margin:0 0 14px`. Contiene due opzioni: **`"Desktop"`** (attiva
  di default) e **`"Mobile"`**. Non fa parte dell'app: è il commutatore di form factor del
  mockup.
- **Cornice finta del browser** (`.frame-outer#frameOuter`) — sfondo `#ececec`, raggio `14px`,
  bordo `1px solid #ddd`, ombra `0 1px 3px rgba(0,0,0,.08), 0 12px 32px rgba(0,0,0,.08)`.
  - **`.frame-bar`** alta `38px`: tre `.frame-dot` da `10px` grigi `#d0d0d0` e un campo
    `.frame-url` con il testo **`"keeppix.local"`** (12px, grigio `#8a8a8a`, su fondo bianco).
- **`#app`** — il contenitore vero dell'applicazione: `display:flex`, **altezza fissa `680px`**,
  `overflow:hidden`. Porta l'attributo `data-theme` (`light` di default, valori `light`/`dark`)
  da cui discendono tutte le variabili CSS di colore.
- **`.sidebar`** — larghezza fissa `216px`, fondo `var(--sidebar-bg)`, bordo destro `1px solid
  var(--border)`, `padding:18px 14px 14px`, `overflow-y:auto`. Vedi sezione 2.
- **`.main`** — colonna flessibile che occupa lo spazio restante (`flex:1`, `min-width:0`,
  `position:relative`) e contiene, nell'ordine del DOM:
  1. `.topbar` (sezione 4) — visibile solo su desktop;
  2. `.mobile-header#mobileHeaderHost` (sezione 5) — `display:none` su desktop;
  3. `.view-root#viewRoot` — l'area di contenuto;
  4. `.mobile-tabbar#mobileTabbarHost` (sezione 5) — `display:none` su desktop.
- **`#lightboxRoot`** — contenitore vuoto in cui viene montato il visore a schermo intero
  (fuori dal perimetro di questo documento).
- **`#selectionLiveRegion`** — `<div class="sr-only" aria-live="polite" aria-atomic="true">`:
  regione annunciata dagli screen reader per la selezione multipla. Invisibile.

**Area di contenuto (`.view-root`)** — `flex:1`, `overflow-y:auto`, `padding:22px 24px 40px`,
`position:relative`. Ha tre modificatori applicati dal router (vedi sezione 7):
- `.no-pad` → `padding:0`;
- `.hide-native-scrollbar` → nasconde la scrollbar di sistema
  (`scrollbar-width:none` + `::-webkit-scrollbar{width:0;height:0;display:none}`);
- `.has-scrubber` → `overflow:hidden;padding:0`.

Il commento nel CSS spiega **perché**: su Foto lo scrubber personalizzato e la scrollbar di
sistema, sovrapposte sullo stesso bordo destro, si intralciavano (il trascinamento non
rispondeva); lo scroll è quindi stato spostato in un contenitore interno `.foto-scroll`
(`position:absolute;inset:0;overflow-y:auto;padding:22px 54px 40px 24px`) mentre `#viewRoot`
resta la cornice ferma su cui lo scrubber è ancorato.

### 3. Ogni controllo, uno per uno
| Etichetta / elemento | Tipo | Cosa fa |
|---|---|---|
| `"Desktop"` (`#deviceSwitch [data-device="desktop"]`) | opzione di un controllo segmentato | chiama `setDevice('desktop')`: toglie `.device-mobile` da `#frameOuter` e `#app`, sposta `.active` sull'opzione, poi `renderAll()` |
| `"Mobile"` (`#deviceSwitch [data-device="mobile"]`) | opzione di un controllo segmentato | chiama `setDevice('mobile')`: aggiunge `.device-mobile` a `#frameOuter` e `#app`, poi `renderAll()` |
| Puntini e URL della cornice (`.frame-dot`, `.frame-url`) | decorazione | nessuna interazione, non cliccabili |

Non esiste nessun altro controllo a livello di telaio: niente pulsante di comprimi-sidebar,
niente barra del titolo dell'applicazione, niente area di notifiche.

### 4. Interazioni da mouse
- **Click** sulle due opzioni del commutatore dispositivo: unico controllo del telaio.
- **Hover**: `.seg-option` non ha regola `:hover` — nessun feedback al passaggio del mouse,
  solo il cambio di stato `.active` al click.
- **Tasto destro**: nessun menu contestuale — **non previsto nel mockup** in nessun punto della
  shell.
- **Trascinamento**: nessun drag&drop nella shell — **non previsto nel mockup** (né per
  riordinare le voci, né per ridimensionare la sidebar, né per spostare foto sulle cartelle).
- **Rotellina**: scroll verticale nativo su `.view-root` (o su `.foto-scroll` nella vista Foto)
  e, indipendentemente, su `.sidebar` se il suo contenuto trabocca.

### 5. Interazioni da tastiera
- **Nessuna scorciatoia globale di applicazione.** Non esiste un `document.addEventListener
  ('keydown', …)` di livello shell: gli unici ascoltatori di tastiera globali sono quelli
  temporanei dei dialog (Esc chiude, vedi SP-5) e quello della vista Culling (frecce, `P`, `X`,
  `Delete`, `1`–`5`).
- **Ordine di Tab**: nella shell desktop l'unico elemento raggiungibile con Tab è
  `#topSearch` (è un `<input>`, quindi focusabile anche se `readonly`). Le opzioni
  `Desktop`/`Mobile` sono `<div>` senza `tabindex`: **non raggiungibili da tastiera**, pur
  esistendo una regola `.seg-option:focus-visible` (vedi ambiguità in sezione 7).
- **Escape**: non chiude nulla a livello di shell (non c'è un gestore Esc per i menu; vedi
  sezione 3 di questo documento e la deviazione da SP-14).

### 6. Animazioni e transizioni
- `#app` ha `transition:background .25s ease, color .25s ease`: **il cambio di tema sfuma** lo
  sfondo e il colore del testo del contenitore. Comunica che è la stessa schermata che cambia
  aspetto, non un ricaricamento. Nota: la transizione è dichiarata solo su `#app`, quindi i
  figli (sidebar, tile, card) cambiano colore **istantaneamente** — l'effetto complessivo è una
  dissolvenza parziale.
- Il passaggio Desktop↔Mobile **non è animato**: `.frame-outer.device-mobile` cambia
  `width`, `border-radius` e `border` senza `transition` dichiarata, ed è comunque seguito da un
  `renderAll()` che ricostruisce il DOM.
- Nessuna animazione di entrata/uscita delle viste: `root.innerHTML=''` seguito dal render è un
  sostituzione secca — **nessuna dissolvenza fra schermate, non prevista nel mockup**.

### 7. Stati per ogni controllo
- **`.seg-option`** — *normale*: `color:var(--text-secondary)`, `font-size:12.5px`,
  `padding:6px 14px`, `border-radius:7px`. *Attiva* (`.active`): `background:var(--card-bg)`,
  `color:var(--text)`, `font-weight:600`, `box-shadow:var(--shadow)`. *Hover*: nessuno.
  *Focus*: regola `:focus-visible` presente (`outline:2.5px solid var(--accent)`,
  `outline-offset:2px`) ma irraggiungibile perché l'elemento non è focusabile.
  *Disabilitata / in caricamento / in errore*: **non previste**.
- **`#app`** — due soli stati visivi: `data-theme="light"` e `data-theme="dark"`, più la classe
  `.selection-active` (aggiunta dal router quando `state.selectionMode` è vero: rende sempre
  visibili le spunte di selezione sulle tile, `#app.selection-active .tile-check{opacity:1;
  pointer-events:auto}`) e `.device-mobile`.
- **Stato vuoto della shell**: non esiste — c'è sempre una vista montata (fallback su `renderFoto`).
- **Stato di caricamento**: non esiste — i dati sono generati in memoria, nessuno scheletro di
  caricamento è previsto nel mockup.

### 8. Da dove ci si arriva e dove si va
La shell è sempre presente: non si "entra" e non si "esce". È il contenitore di ogni vista.
L'unica uscita è la voce `"Esci"` del menu account, che nel mockup mostra solo un toast.

### 9. Dati necessari a questa schermata
- **Legge**: il tema corrente (`state.theme` → attributo `data-theme`), il form factor
  (`state.device`), se la selezione multipla è attiva (`state.selectionMode`), la vista corrente
  (`state.view`) per decidere le classi di `.view-root`.
- **Scrive**: nulla di persistente. `setDevice()` scrive `state.device`; `applyTheme()` scrive
  solo l'attributo DOM.

---

## 2. Sidebar di navigazione

### 1. Nome e scopo
Colonna fissa di sinistra (solo desktop) che contiene marchio, tutte le sezioni
dell'applicazione raggruppate, l'elenco delle cartelle su disco, l'indicatore di spazio libero e
il piede con l'utente.

### 2. Cosa mostra
Dall'alto in basso, tutto dentro `.sidebar` (larga `216px`, scorrevole se il contenuto trabocca):

**a) Marchio (`.brand`)** — `padding:2px 6px 18px`, `gap:10px`:
- `.brand-mark` 26×26 px: SVG con un **anello** (`circle r=62`, `stroke-width=22`, colore
  `currentColor` = `var(--text)`) e un **pallino** centrale (`circle r=24`, `fill:var(--accent)`).
  Commento nel codice (brand guidelines v4): *il pallino è l'unico elemento a colore in tutto il
  sistema*; l'anello segue il colore del testo così da funzionare in entrambi i temi senza due
  asset. In tema chiaro il pallino riceve un contorno `stroke:#3A3A3A;stroke-width:3` — **fix di
  accessibilità dall'audit**: arancione pieno su bianco è 2.64:1, sotto la soglia 3:1 per un
  elemento grafico; col contorno sale a 11.37:1.
- `.brand-name` con il testo **`"Keeppix"`**, `font-weight:700`, `15.5px`,
  `letter-spacing:-.01em`.

**b) Gruppo principale (`#nav-top`, senza etichetta di gruppo)** — le sei voci di `NAV_TOP`,
nell'ordine:

| # | Etichetta esatta | Icona | Badge |
|---|---|---|---|
| 1 | **`"Foto"`** | `photo` (16px) | nessuno |
| 2 | **`"Cerca"`** | `search` (16px) | nessuno |
| 3 | **`"Culling"`** | `funnel` (16px) | **sì, sempre** — `.nav-badge` con `cullingQueueCount()` |
| 4 | **`"Persone"`** | `user` (16px) | nessuno |
| 5 | **`"Mappa"`** | `map` (16px) | nessuno |
| 6 | **`"Condivisioni"`** | `share` (16px) | nessuno |

`cullingQueueCount()` somma le foto ancora "da valutare" (`cullState==='root'`) di **tutti** i
lotti di `CULLING_BATCHES` (`"Dolomiti"`, `"Toscana — Val d'Orcia"`, `"Cinque Terre"`). Il
commento chiarisce il perché del cambio: *non più solo la cartella di libreria attualmente
aperta, ora che Culling è organizzato per lotti*.

**c) Gruppo `"Libreria"` (`.nav-label` + `#nav-library`)** — l'etichetta di gruppo è
`"LIBRERIA"` (resa maiuscola dal CSS `text-transform:uppercase`; nel markup è `Libreria`).
Contiene, nell'ordine:

| # | Etichetta esatta | Icona | Livello | Badge |
|---|---|---|---|---|
| 1 | **`"Preferiti"`** | `heart` (16px) | primo livello | nessuno |
| 2 | **`"Album"`** | `album` (16px) | primo livello | nessuno |
| 3 | **`"Manutenzione"`** | `settings` (16px) + `chevronDown` (13px) a destra | gruppo a scomparsa | nessuno |
| 3a | **`"Cestino"`** | `trash` (15px) | sotto-voce | nessuno |
| 3b | **`"Duplicati"`** | `copy` (15px) | sotto-voce | nessuno |
| 3c | **`"Problemi"`** | `alert` (15px) | sotto-voce | nessuno |
| 4 | **`"IA"`** | `cpu` (16px) + `chevronDown` (13px) a destra | gruppo a scomparsa | nessuno |
| 4a | **`"Tag e categorie"`** | `tag` (15px) | sotto-voce | nessuno |
| 4b | **`"Revisione"`** | `inbox` (15px) | sotto-voce | **sì, solo se > 0** — `pendingSuggestionCount()` |
| 4c | **`"Analisi libreria"`** | `activity` (15px) | sotto-voce | nessuno |

Il commento sopra `NAV_MAINT` spiega **perché** esiste il raggruppamento: *Cestino/Duplicati/
Problemi non sono raccolte curate come Album/Preferiti, sono tutte pagine di "manutenzione".
Raggruppate sotto una singola voce a scomparsa invece che tre righe fisse — con la sidebar già a
680px di altezza fissa, ogni riga fissa in più la fa traboccare in una scrollbar interna (è
esattamente quello che è successo aggiungendo Duplicati come riga a sé).*
Il commento sopra `NAV_AI` aggiunge: *stesso pattern, gruppo a parte perché concettualmente
diverso (organizzazione via IA, non pulizia della libreria).*

`pendingSuggestionCount()` = suggerimenti di tag in attesa su tutti i tag **+** volti in attesa,
questi ultimi contati **solo se** `state.faceRecognitionEnabled` è vero.

**d) Gruppo `"Cartelle"` (`.nav-label` + `#nav-folders`)** — una riga `.folder-item` per ogni
voce di `FOLDERS`, con **nome a sinistra e conteggio a destra**:

| Etichetta esatta | Conteggio (`.folder-count`) |
|---|---|
| **`"Urbino"`** | `556` |
| **`"Lago di Braies"`** | `110` |
| **`"Chioggia e Venezia"`** | `246` |

Non c'è albero/gerarchia: l'elenco è piatto, senza cartelle annidate — **non previsto nel
mockup**. Non c'è un pulsante "aggiungi cartella" qui.

**e) Spaziatore (`.sidebar-spacer`, `flex:1`)** — spinge in basso i due blocchi seguenti.

**f) Indicatore di spazio (`.storage-card`)** — riquadro su fondo `var(--chip-bg)`,
`border-radius:10px`, `padding:11px 12px`, `margin:6px 2px 12px`. Tre elementi, **tutti testo
statico nel markup HTML**:
- `.storage-label`: **`"Spazio libero"`** (11px, `var(--text-secondary)`);
- `.storage-value`: **`"1,4 TB su 2 TB"`** (12.5px, `font-weight:600`);
- `.storage-bar` alta `5px`, raggio `3px`, fondo `var(--border-strong)`, con
  `.storage-bar-fill` `width:12.75%` in `var(--accent)`.

**g) Piede utente (`.user-footer#userFooter`)** — `display:flex`, `gap:9px`,
`padding:8px 6px 2px`, `cursor:pointer`, `border-radius:8px`, `position:relative`:
- `.avatar` (SP-16): cerchio `28px`, fondo `var(--accent)` di default, testo **`"GM"`** in
  bianco `12px/700`. Il commento CSS spiega la scelta del bianco: *l'avatar resta leggibile
  sopra qualunque sfondo scelto […] confermato da Giovanni: bianco su arancione va bene qui,
  trattato come elemento di marca più che testo da leggere a lungo.*
- `.user-name`: **`"Giovanni"`** (13px/600);
- `.user-status`: `.status-dot` (cerchio `6px`, `background:#2E9E5B`) + testo **`"In linea"`**
  (11px, `var(--text-secondary)`). Commento CSS: il verde è più scuro dell'originale `#3FB65C`
  (2.61:1, sotto la soglia grafica 3:1) ed è **lo stesso verde semantico già usato per
  "Scelta"/pick nel culling** (`#2E9E5B`, 3.4:1), riusato invece di introdurre una terza
  tonalità.
- `#userMenuHost`: contenitore vuoto in cui viene montato il menu account (sezione 3).

Nota di implementazione dal codice: **l'avatar del piede è markup statico**, mai rigenerato da
un template, quindi a ogni `renderSidebar()` il colore scelto in Profilo va sincronizzato a mano
(`av.style.background = state.avatarColor || ''`).

### 3. Ogni controllo, uno per uno
1. **Marchio `"Keeppix"`** — decorazione, **non cliccabile**: nessun handler. Non riporta a Foto.
2. **`"Foto"`** — riga di navigazione. Imposta `state.view='foto'` **e azzera
   `state.currentFolder`** (commento: *"Foto" torna sempre alla timeline combinata*), poi applica
   il reset comune (sezione 7).
3. **`"Cerca"`** — va alla pagina Cerca. Non porta il focus nel campo di ricerca (a differenza del
   click su `#topSearch`).
4. **`"Culling"`** — va a Culling. **Non azzera `state.cullingBatchId`**: se un lotto era aperto,
   si rientra dentro quel lotto.
5. **`"Persone"`** — va alla griglia Persone (azzera `state.openPerson` e
   `state.personSelectedIds`).
6. **`"Mappa"`** — va alla Mappa.
7. **`"Condivisioni"`** — va a Condivisioni. Non tocca `state.shareTab` (la scheda
   `"Le mie condivisioni"` / `"Condivisi con me"` resta quella di prima).
8. **`"Preferiti"`** — va a Preferiti.
9. **`"Album"`** — va alla griglia Album (azzera `state.openAlbum`).
10. **`"Manutenzione"`** — **interruttore di gruppo**, non una destinazione: inverte
    `state.navMaintOpen` e richiama **solo** `renderSidebar()` (non `renderAll()`), quindi la
    vista corrente non cambia e non si perde nulla.
11. **`"Cestino"`**, 12. **`"Duplicati"`**, 13. **`"Problemi"`** — sotto-voci, navigano alla
    rispettiva pagina con il reset comune.
14. **`"IA"`** — interruttore di gruppo, inverte `state.navAiOpen`, come sopra.
15. **`"Tag e categorie"`**, 16. **`"Revisione"`**, 17. **`"Analisi libreria"`** — sotto-voci.
18. **`"Urbino"` / `"Lago di Braies"` / `"Chioggia e Venezia"`** — righe cartella. Impostano
    `state.currentFolder = <id>`, `state.view='foto'`, `state.userMenuOpen=false`, poi
    `renderAll()`. **Non applicano il reset comune** (vedi sezione 7).
19. **Riquadro `"Spazio libero"`** — **puramente informativo, non cliccabile**: nessun handler,
    nessun link a Impostazioni.
20. **Piede utente** — apre/chiude il menu account (`state.userMenuOpen = !state.userMenuOpen`
    seguito da `renderUserMenu()`); l'handler fa `e.stopPropagation()` per non essere richiuso
    subito dall'ascoltatore di click sul documento. Tutta la riga è la zona cliccabile (avatar,
    nome e "In linea" insieme).

**Nessun campo di testo, nessun interruttore, nessun chip nella sidebar.** Nessuna voce ha
tooltip `[data-tip]` (SP-7): le etichette sono sempre visibili per esteso.

### 4. Interazioni da mouse
- **Click singolo** su qualsiasi `.nav-item`, `.nav-subitem`, `.folder-item`, sul piede utente e
  sui due interruttori di gruppo: unica interazione prevista.
- **Doppio click**: nessun comportamento distinto (viene trattato come due click).
- **Tasto destro**: **nessun menu contestuale — non previsto nel mockup** (né sulle sezioni né
  sulle cartelle: non si può rinominare o rimuovere una cartella da qui).
- **Hover**: cambio di fondo immediato (nessun `transition` dichiarata su `.nav-item` o
  `.folder-item`, quindi **0 ms**) — `.nav-item:hover{background:var(--chip-bg)}`;
  `.folder-item:hover{background:var(--chip-bg);color:var(--text)}`.
  Nessun tooltip, nessun ritardo di comparsa.
- **Trascinamento**: **non previsto** — non si riordinano le voci, non si trascinano foto sulle
  cartelle, la sidebar non si ridimensiona.
- **Rotellina**: la sidebar è `overflow-y:auto`, quindi scrolla se il contenuto supera i 680px
  di altezza dell'app — è esattamente il traboccamento che il raggruppamento a scomparsa serve a
  evitare.
- **Testo non selezionabile**: `.nav-item{user-select:none}` (le `.folder-item` invece **non**
  hanno `user-select:none` — piccola incoerenza).

### 5. Interazioni da tastiera
- **Nessuna voce della sidebar è raggiungibile da tastiera.** `.nav-item`, `.nav-subitem` e
  `.folder-item` sono `<div>` senza `tabindex`, senza `role` e senza `bindActivatable` (SP-8).
  Non entrano nell'ordine di Tab, non rispondono a Invio o Spazio.
- Esistono comunque le regole `.nav-item:focus-visible` e `.folder-item:focus-visible`
  (`outline:2.5px solid var(--accent); outline-offset:2px`): sono **codice morto** allo stato
  attuale, e insieme documentano l'intenzione di rendere queste voci focusabili.
- Gli interruttori di gruppo espongono `aria-expanded="true|false"`, quindi sono già
  semanticamente etichettati come espandibili, ma non hanno `role="button"` né `tabindex`.
- Nessuna navigazione con frecce dentro la sidebar. Nessuna scorciatoia per saltare a una
  sezione.

### 6. Animazioni e transizioni
- **Chevron dei gruppi a scomparsa**: `.nav-maint-toggle .ico:last-child{transition:transform
  .15s ease; opacity:.6}` e `.nav-maint-toggle.open .ico:last-child{transform:rotate(180deg)}` —
  la freccia ruota di mezzo giro in **`.15s ease`**. Comunica apertura/chiusura del gruppo.
- **Le sotto-voci non hanno animazione di apertura**: vengono aggiunte/rimosse dal markup
  (`maintOpen ? … : ''`), quindi compaiono e spariscono **di colpo**. Non c'è espansione in
  altezza — **non prevista nel mockup**.
- **Hover e stato attivo**: nessuna transizione dichiarata, il cambio è immediato.
- **Barra dello spazio**: `.storage-bar-fill` ha larghezza fissa, **nessuna animazione di
  riempimento**.

### 7. Stati per ogni controllo
**`.nav-item` (voci di primo livello)**
- *Normale*: `padding:8px 10px`, `border-radius:8px`, `font-size:14px`, `color:var(--text)`,
  `border-left:2.5px solid transparent`, `margin-bottom:1px`; icona a `opacity:.85`.
- *Hover*: `background:var(--chip-bg)`.
- *Attiva* (`.active`, quando `state.view === n.id`): `background:var(--chip-bg)`,
  `border-left-color:var(--accent)`, `font-weight:600`.
  Il commento CSS spiega la regola di linguaggio: *stesso linguaggio ovunque nella sidebar (voci
  di primo livello, sotto-voci, cartelle): grigio + bordino a sinistra arancione — **mai il pieno
  arancione**, che qui è riservato al pallino di notifica.*
- *Focus*: contorno arancione `2.5px` (irraggiungibile, vedi sopra).
- *Premuta*: nessuno stato `:active` dichiarato.
- *Disabilitata*: **nessuna voce è mai disabilitata** — tutte le sezioni sono sempre
  raggiungibili, anche quando vuote (es. Cestino vuoto).
- *In caricamento / errore*: non previsti.

**`.nav-subitem`** — come sopra ma `padding-left:26px`, `font-size:13px`, icone `15px`,
`gap:8px`. Il commento chiarisce l'intento: *sotto-voci leggermente più piccole e rientrate, per
restare leggibili come "dentro" il gruppo senza sembrare una sezione completamente nuova.*

**`.nav-maint-toggle` (le righe `"Manutenzione"` e `"IA"`)**
- *Normale*: identica a una `.nav-item`, ma `justify-content:space-between` per spingere la
  chevron a destra.
- *Aperta* (`.open`): chevron ruotata di 180°.
- *"Contiene la vista corrente"* (`.parent-active`): **solo** `font-weight:600` e icona a
  `opacity:1` — **niente sfondo, niente bordino**. Il commento spiega il perché: *il gruppo va
  segnalato quando una delle sue sotto-voci è la vista corrente, ma non con lo stesso
  trattamento della sotto-voce stessa (altrimenti sembrano entrambe "qui sei") […] sfondo e
  bordino restano un segnale esclusivo di dove ci si trova davvero.*
- *Stato bloccato*: `maintOpen = state.navMaintOpen || maintActive`. Quindi **quando la vista
  corrente è dentro il gruppo, il gruppo non si può chiudere**: il click sulla chevron mette
  `navMaintOpen=false` ma `maintActive` lo tiene aperto e visivamente non succede nulla.

**`.nav-badge`** — `background:var(--danger)`, `color:#fff`, `font-size:10.5px`,
`font-weight:700`, `border-radius:9px`, `padding:1px 6px`, `min-width:16px`, `text-align:center`.
- Badge Culling: **renderizzato sempre**, anche con valore `0`.
- Badge Revisione: renderizzato **solo se** `pendingCount > 0` (quindi sparisce a coda vuota).

**`.folder-item`** — *normale*: `padding:7px 10px`, `font-size:13.5px`,
`color:var(--text-secondary)`, bordino sinistro trasparente `2.5px`. *Hover*:
`background:var(--chip-bg)` + `color:var(--text)`. *Attiva*: solo quando
`state.view==='foto' && state.currentFolder===f.id` → `color:var(--text)`, `font-weight:600`,
`border-left-color:var(--accent)`, `background:var(--chip-bg)`. Il conteggio `.folder-count`
resta sempre `11.5px` `var(--text-tertiary)`.
*Stato vuoto dell'elenco*: se `FOLDERS` fosse vuoto il gruppo mostrerebbe solo l'etichetta
`"Cartelle"` — **nessun messaggio di elenco vuoto previsto**.

**`.user-footer`** — *normale* trasparente; *hover* `background:var(--chip-bg)`; nessuno stato
attivo/premuto/disabilitato.

### 8. Da dove ci si arriva e dove si va
- **In ingresso**: la sidebar è sempre visibile su desktop; non ha una schermata di provenienza.
  Scompare del tutto passando a Mobile (`#app.device-mobile .sidebar{display:none}`), dove il suo
  ruolo è svolto dalla tab bar in basso più la pagina `"Altro"` (sezioni 5 e 6).
- **In uscita**: ogni voce porta alla vista omonima; le righe cartella portano a **Foto filtrata
  su quella cartella**; il piede utente apre il menu account.

### 9. Dati necessari a questa schermata
**Legge**
- Vista corrente (`state.view`) per gli stati attivo/parent-active.
- Cartella corrente (`state.currentFolder`) per evidenziare la riga cartella.
- Stato di apertura dei due gruppi (`state.navMaintOpen`, `state.navAiOpen`).
- Elenco delle cartelle: per ciascuna **nome** e **numero totale di foto**.
- Numero di foto ancora da valutare in tutti i lotti di culling (per il badge).
- Numero di suggerimenti IA in attesa: tag suggeriti + volti da confermare, **questi ultimi solo
  se il riconoscimento volti è attivo**.
- Colore avatar scelto dall'utente (`state.avatarColor`).
- Spazio libero e totale del server — nel mockup **testo statico**, non un dato.

**Scrive**
- `state.view`, `state.currentFolder`, `state.navMaintOpen`, `state.navAiOpen`,
  `state.userMenuOpen`.
- Il reset di sezione: `state.openAlbum`, `state.openPerson`, `state.personSelectedIds`,
  `state.browseFilterOpen`, `state.browseFilters`, `state.browseFilterQuery` (dettaglio in
  sezione 7).

---

## 3. Menu account

### 1. Nome e scopo
Piccolo menu a comparsa che si apre dal piede utente della sidebar e raccoglie le tre azioni di
account: profilo, impostazioni, uscita.

### 2. Cosa mostra
Un pannello `.user-menu` largo `190px`, ancorato **sopra** il piede utente
(`position:absolute; bottom:calc(100% + 6px); left:2px`), su `var(--card-bg)`, bordo `1px solid
var(--border-strong)`, `border-radius:10px`, ombra `0 8px 24px rgba(0,0,0,.18)`, `padding:6px`,
`z-index:20`. Contiene esattamente tre voci e un separatore:

| Ordine | Etichetta esatta | Icona (15px) | Trattamento |
|---|---|---|---|
| 1 | **`"Profilo"`** | `user` | normale |
| 2 | **`"Impostazioni"`** | `settings` | normale |
| — | separatore `.user-menu-sep` | — | linea `1px` `var(--border)`, `margin:5px 2px` |
| 3 | **`"Esci"`** | `close` | `.danger` → `color:var(--danger)` |

Nessun'altra informazione: **niente e-mail dell'utente, niente nome del server, niente voce
"Cambia utente" o "Aiuto"** — non previste nel mockup.

### 3. Ogni controllo, uno per uno
1. **`"Profilo"`** (`[data-usermenu="profilo"]`) — chiude il menu e imposta `state.view='profilo'`,
   poi `renderAll()`.
2. **`"Impostazioni"`** (`[data-usermenu="impostazioni"]`) — chiude il menu e imposta
   `state.view='impostazioni'`, poi `renderAll()`.
3. **`"Esci"`** (`[data-usermenu="esci"]`) — chiude il menu e mostra un toast (SP-6) con il testo
   esatto: **`"Solo demo — il logout reale disconnetterebbe la sessione."`**. **Non cambia vista,
   non chiede conferma, non pulisce nulla.**

Nota importante: le voci `"Profilo"` e `"Impostazioni"` funzionano assegnando direttamente
`state.view = action`, dove `action` è il valore di `data-usermenu`. Non passano dalla logica di
reset della sidebar: **filtri rapidi, album/persona aperti e selezione restano intatti** entrando
in Profilo o Impostazioni.

### 4. Interazioni da mouse
- **Click sul piede utente**: apre/chiude (comportamento a interruttore).
- **Click su una voce**: la esegue (con `e.stopPropagation()` per non far intervenire anche il
  gestore di chiusura globale).
- **Click fuori** (in qualsiasi punto del documento): chiude il menu —
  `document.addEventListener('click', …)` che azzera `state.userMenuOpen` e richiama solo
  `renderUserMenu()`. Conforme a SP-14 per la parte "click fuori chiude".
- **Hover su una voce**: `background:var(--chip-bg)`, immediato (nessuna transizione).
- **Tasto destro / trascinamento / rotellina**: nessun comportamento — **non previsti**.

### 5. Interazioni da tastiera
- **Nessuna.** Le voci sono `<div>` senza `tabindex`, senza `role="menuitem"` e senza
  `bindActivatable` (SP-8): non si raggiungono con Tab, non rispondono a Invio/Spazio.
- **Escape non chiude il menu**: non c'è nessun gestore `keydown` associato.
  **Deviazione da SP-14**, che prevede sia "click fuori chiude" sia "Esc chiude" — qui è
  implementata solo la prima metà.
- Il pannello non ha `role="menu"`, non intrappola il focus e non lo restituisce a chi lo ha
  aperto.

### 6. Animazioni e transizioni
- **Nessuna animazione di apertura/chiusura**: il menu viene scritto o cancellato via
  `host.innerHTML`, quindi appare e sparisce istantaneamente. Nessuna dissolvenza o scivolamento
  — **non previsti nel mockup**.
- L'unico effetto animato collegato è il **toast** di `"Esci"` (SP-6): `.toast` parte a
  `opacity:0` con `transform:translateX(-50%) translateY(10px)` e `transition:opacity .2s ease,
  transform .2s ease`; la classe `.show` viene aggiunta dopo **10 ms**, tolta dopo **2400 ms**, e
  l'elemento è rimosso dal DOM **250 ms** più tardi.

### 7. Stati per ogni controllo
- **`.user-menu-item`** — *normale*: `padding:8px 9px`, `border-radius:7px`, `font-size:13px`,
  `color:var(--text)`, `gap:9px` fra icona ed etichetta. *Hover*: `background:var(--chip-bg)`.
  *Focus / premuto / disabilitato / in caricamento*: **nessuno stato dichiarato né raggiungibile**.
- **`"Esci"`** (`.danger`) — colore `var(--danger)` a riposo; in hover mantiene il rosso e prende
  lo stesso fondo grigio delle altre voci (nessun fondo rosso).
- **Menu chiuso** — `#userMenuHost` è letteralmente vuoto (`innerHTML=''`): non esiste nel DOM.
- **Stato vuoto**: non applicabile, le tre voci sono fisse e sempre presenti.

### 8. Da dove ci si arriva e dove si va
- **In ingresso**: solo dal click sul piede utente della sidebar (desktop). L'equivalente mobile è
  il menu dell'avatar nell'header (sezione 5), con le **stesse identiche tre voci**.
- **In uscita**: Profilo, Impostazioni, oppure si resta dov'era con un toast (`"Esci"`).
- Il menu si chiude anche implicitamente ogni volta che si clicca una voce di sidebar o una
  cartella, perché entrambi gli handler impostano `state.userMenuOpen=false`.

### 9. Dati necessari a questa schermata
- **Legge**: solo `state.userMenuOpen`. Nessun dato utente è mostrato dentro il menu.
- **Scrive**: `state.userMenuOpen`, `state.view`. `"Esci"` non scrive nulla.

---

## 4. Barra superiore / breadcrumb

### 1. Nome e scopo
Barra fissa in cima all'area di contenuto (solo desktop) che dice **dove ci si trova** e offre la
scorciatoia alla ricerca.

### 2. Cosa mostra
`.topbar`: altezza `56px`, `padding:0 20px`, bordo inferiore `1px solid var(--border)`,
`gap:16px`, contenuto allineato agli estremi. Due soli elementi:

**a) Briciole di pane (`.breadcrumb#breadcrumb`)** — `font-size:14.5px`,
`color:var(--text-secondary)`, riga singola con ellissi (`white-space:nowrap; overflow:hidden;
text-overflow:ellipsis`). La parte in `<b>` è l'elemento corrente: `color:var(--text)`,
`font-weight:600`. Testo esatto per ogni vista:

| `state.view` | Testo mostrato |
|---|---|
| `foto` (nessuna cartella) | **`Tutte le foto`** (in grassetto) |
| `foto` (cartella aperta) | **`Cartelle / <nome cartella>`** — solo il nome in grassetto |
| `culling` (nessun lotto) | **`Culling`** |
| `culling` (lotto aperto) | **`Culling / <nome lotto>`** |
| `cerca` | **`Cerca`** |
| `mappa` | **`Mappa`** |
| `condivisioni` | **`Condivisioni`** |
| `preferiti` | **`Preferiti`** |
| `album` (griglia) | **`Album`** |
| `album` (album aperto) | **`Album / <nome album>`** |
| `persone` (griglia) | **`Persone`** |
| `persone` (persona aperta) | **`Persone / <nome persona>`** — il nome viene da `personDisplayName()`, che ripiega su **`"Persona <numero>"`** se la persona non è stata battezzata |
| `cestino` | **`Cestino`** |
| `duplicati` | **`Duplicati`** |
| `problemi` | **`Problemi`** |
| `tagManager` | **`Tag e categorie`** |
| `revisione` | **`Revisione`** |
| `analisiLibreria` | **`Analisi libreria`** |
| `profilo` | **`Profilo`** |
| `impostazioni` | **`Impostazioni`** |
| `bulkEdit` | **`Modifica multipla`** |
| `createAlbum`, `libreria`, `cartelle` | **nessuna voce nella mappa → breadcrumb vuoto** |

**b) Zona destra (`.topbar-right`, `gap:14px`)** — contiene **un solo elemento**:
- `input.search-box#topSearch`, largo `230px`, `font-size:13px`,
  `color:var(--text-secondary)`, fondo `var(--chip-bg)`, bordo `1px solid var(--border)`,
  `border-radius:9px`, `padding:8px 12px`, `cursor:text`, attributo **`readonly`**, placeholder
  esatto: **`"Cerca per data, luogo, persona…"`**.

**L'interruttore di tema non è più qui.** Il commento nel codice lo dice esplicitamente: *Il
controllo rapido in alto è stato rimosso: il tema si imposta da Impostazioni → Aspetto (Chiaro /
Scuro / Sistema), un solo posto invece di due controlli ridondanti.* Le regole CSS
`.theme-toggle` e `.theme-toggle-knob` (con `transition:left .18s ease`) sono rimaste nel foglio
di stile ma **nessun markup le usa più**.

### 3. Ogni controllo, uno per uno
1. **Briciole di pane** — testo **non cliccabile**: nessun handler è associato a `#breadcrumb`.
   Anche la parte "genitore" (`Cartelle /`, `Album /`, `Persone /`, `Culling /`) è **inerte**: non
   riporta al livello superiore. **Non previsto nel mockup.**
2. **Campo di ricerca `#topSearch`** — al click esegue: `state.view='cerca'` → `renderAll()` →
   `setTimeout(…, 0)` che porta il focus su `#cercaInput`, il campo vero della pagina Cerca.
   È `readonly`: **non si può digitare qui**; funziona come pulsante travestito da campo. Non ha
   validazione né stato vuoto proprio: il placeholder è sempre visibile perché il valore è sempre
   vuoto.

### 4. Interazioni da mouse
- **Click** su `#topSearch`: apre Cerca e mette il focus nel campo di quella pagina.
- **Hover** su `#topSearch`: `background:var(--chip-bg-hover)`, immediato (nessuna transizione
  dichiarata). Il cursore è `text` — suggerisce (in modo un po' ingannevole) che si possa
  scrivere.
- **Click sulle briciole**: nessun effetto.
- **Tasto destro, doppio click, trascinamento, rotellina**: nessun comportamento — **non
  previsti**.

### 5. Interazioni da tastiera
- `#topSearch` è l'**unico elemento della shell desktop raggiungibile con Tab** (è un `<input>`).
  Riceve il contorno di focus `outline:2.5px solid var(--accent); outline-offset:2px` (regola
  `input:focus-visible`).
- **Ma non ha nessun gestore da tastiera**: premere Invio o Spazio con il focus sul campo **non
  fa nulla** — solo il `click` del mouse apre Cerca. Deviazione da SP-8.
- Nessuna scorciatoia globale per aprire la ricerca (niente `/`, niente `Ctrl+K`) — **non
  prevista nel mockup**.
- Escape non fa nulla nella topbar.

### 6. Animazioni e transizioni
- **Nessuna transizione dichiarata** su `.topbar`, `.breadcrumb` o `.search-box`: il cambio di
  breadcrumb e il fondo in hover sono istantanei.
- Il breadcrumb viene riscritto interamente a ogni `renderAll()` — nessuna dissolvenza di
  cambio testo.
- Residuo non usato: `.theme-toggle-knob{transition:left .18s ease}`.

### 7. Stati per ogni controllo
- **`.search-box`** — *normale*: fondo `var(--chip-bg)`, testo `var(--text-secondary)`.
  *Hover*: fondo `var(--chip-bg-hover)`. *Focus*: contorno arancione `2.5px` (raggiungibile via
  Tab). *Premuto / disabilitato / in caricamento / in errore*: **non previsti**; il campo è
  `readonly` ma non `disabled`, quindi non è mai visivamente spento.
- **`.breadcrumb`** — due soli trattamenti: grigio per il livello genitore, `var(--text)` in
  grassetto `600` per il livello corrente. *Stato vuoto*: stringa vuota per le viste non mappate
  (`createAlbum`, `libreria`, `cartelle`) — la topbar resta con il solo campo di ricerca a destra.
- **`.topbar` intera** — *nascosta* su mobile (`#app.device-mobile .topbar{display:none}`).

### 8. Da dove ci si arriva e dove si va
- **In ingresso**: sempre presente su desktop, sopra qualsiasi vista.
- **In uscita**: l'unico percorso di uscita è il click sul campo di ricerca → pagina **Cerca**,
  con il focus già nel campo di quella pagina.
- Su mobile la topbar è sostituita dall'header mobile (sezione 5), che **non contiene un campo di
  ricerca**: la ricerca lì si raggiunge dalla tab `"Cerca"` in basso.

### 9. Dati necessari a questa schermata
**Legge**
- Vista corrente (`state.view`).
- Nome della cartella corrente, se aperta.
- Nome del lotto di culling corrente, se aperto.
- Nome dell'album corrente, se aperto.
- Nome (o numero automatico) della persona corrente, se aperta.

**Scrive**
- Solo `state.view = 'cerca'` al click sul campo di ricerca. Nessun altro dato.

---

## 5. Shell mobile: header, tab bar in basso, menu account mobile

### 1. Nome e scopo
Telaio alternativo attivato da `state.device === 'mobile'` (SP-17): sostituisce sidebar e topbar
con un header compatto in alto e una barra di quattro schede in basso.

### 2. Cosa mostra
**Cornice** — `.frame-outer.device-mobile`: larghezza `390px`, centrata, `border-radius:36px`,
bordo `7px solid #1c1c1c` (la scocca del telefono), ombra
`0 1px 3px rgba(0,0,0,.1), 0 24px 48px rgba(0,0,0,.24)`; la `.frame-bar` del browser sparisce e
`#app` passa ad altezza **`812px`**.
`#app.device-mobile`: `overflow:hidden`, sidebar e topbar `display:none`, `.main` a larghezza
piena, `.view-root` con `padding:14px 12px 18px`.

**a) Header (`.mobile-header#mobileHeaderHost`)** — `display:flex`, altezza `52px`,
`padding:0 14px`, bordo inferiore `1px solid var(--border)`, contenuto agli estremi.
- **Sinistra (`.mobile-header-left`)**:
  - Freccia indietro `.mobile-back#mobileBackBtn` (28×28, `border-radius:8px`,
    `color:var(--text-secondary)`), icona `chevronLeft` 18px — **presente solo in certe viste**
    (vedi sotto).
  - Titolo `.mobile-header-title`: `15.5px`, `font-weight:700`, riga singola con ellissi.
- **Destra (`.mobile-header-right`, `gap:8px`, `position:relative`)**:
  - Pulsante imbuto `.mobile-icon-btn#mobileCullingBtn` (32×32, raggio `8px`), icona `funnel`
    18px, `aria-label="Culling"` — **visibile solo quando `state.view === 'foto'`**. Se ci sono
    foto da valutare porta un `.nav-badge` posizionato in assoluto a `top:-3px;right:-3px` con il
    conteggio; **se il conteggio è 0 il badge non viene disegnato**.
  - Avatar `.mobile-account-btn#mobileAccountBtn`: `.avatar` forzato a `28px`, `font-size:11px`,
    iniziali **`"GM"`**, colore da `myAvatarStyle()` (cioè `state.avatarColor`, altrimenti
    `var(--accent)`).
  - `#mobileAccountMenuHost`: contenitore del menu account mobile.

**Titolo per vista** (`mobileTitleFor()`), testo esatto:

| `state.view` | Titolo |
|---|---|
| `foto` | nome della cartella aperta, oppure **`"Tutte le foto"`** |
| `cerca` | **`"Cerca"`** |
| `culling` | nome del lotto aperto, oppure **`"Culling"`** |
| `mappa` | **`"Mappa"`** |
| `condivisioni` | **`"Condivisi con me"`** se `shareTab==='withme'`, altrimenti **`"Le mie condivisioni"`** |
| `preferiti` | **`"Preferiti"`** |
| `cartelle` | **`"Cartelle"`** |
| `album` | nome dell'album aperto, oppure **`"Album"`** |
| `persone` | nome della persona aperta, oppure **`"Persone"`** |
| `cestino` | **`"Cestino"`** |
| `duplicati` | **`"Duplicati"`** |
| `problemi` | **`"Problemi"`** |
| `tagManager` | **`"Tag e categorie"`** |
| `revisione` | **`"Revisione"`** |
| `analisiLibreria` | **`"Analisi libreria"`** |
| `profilo` | **`"Profilo"`** |
| `impostazioni` | **`"Impostazioni"`** |
| `libreria` | **`"Altro"`** |
| `bulkEdit` | **`"Modifica multipla"`** |
| qualsiasi altro (es. `createAlbum`) | **`"Keeppix"`** (fallback) |

**b) Tab bar (`.mobile-tabbar#mobileTabbarHost`)** — `display:flex`, bordo superiore `1px solid
var(--border)`, fondo `var(--bg-elevated)`, `padding:6px 4px 8px`. Quattro schede a larghezza
uguale (`.mobile-tab{flex:1}`), ognuna con icona `21px` sopra ed etichetta sotto
(`font-size:10.5px`, `font-weight:600`, `gap:3px`, `user-select:none`):

| Ordine | Etichetta esatta | Icona | `state.view` impostata |
|---|---|---|---|
| 1 | **`"Foto"`** | `photo` | `foto` |
| 2 | **`"Cerca"`** | `search` | `cerca` |
| 3 | **`"Album"`** | `album` | `album` |
| 4 | **`"Altro"`** | `more` (tre puntini orizzontali) | `libreria` |

**c) Menu account mobile** — stesso componente `.user-menu` della sidebar, con in più la classe
`.mobile-account-menu` che lo riancora **sotto** l'avatar:
`position:absolute; top:calc(100% + 8px); bottom:auto; left:auto; right:0`. Voci **identiche** a
quelle desktop: **`"Profilo"`**, **`"Impostazioni"`**, separatore, **`"Esci"`** (in rosso), con le
stesse icone `user`/`settings`/`close` a 15px e lo stesso toast **`"Solo demo — il logout reale
disconnetterebbe la sessione."`**.

**d) Barra strumenti appiccicosa** — su mobile `.grid-toolbar` diventa
`position:sticky; top:0; z-index:5`, con fondo `var(--bg)` e `margin:0 -12px 10px;
padding:10px 12px 8px`. Il commento spiega il **perché**: *il trigger dei filtri deve restare
raggiungibile mentre si scrolla una timeline lunga — niente più bisogno di risalire in cima.
Sticky invece di spostarlo nell'header: resta nel flusso di ogni vista (Foto, Preferiti, Album,
Persona), senza dover duplicare lì lo stato/i dati di scoping.*

### 3. Ogni controllo, uno per uno
1. **Freccia indietro** (`#mobileBackBtn`) — compare quando `showBack` è vero, cioè:
   dettaglio album aperto **oppure** vista `culling` **oppure** vista `bulkEdit` **oppure** la
   vista non è una delle radici `['foto','cerca','libreria']` e non è `album`.
   Comportamento al click, in ordine di priorità:
   - se è il **dettaglio di un album** → `state.openAlbum=null` (torna alla griglia Album,
     restando sulla tab Album);
   - se è **`culling`** o **`bulkEdit`** → `state.view='foto'`;
   - **in tutti gli altri casi** → `state.view='libreria'` (torna alla pagina "Altro").
2. **Titolo** — testo, non cliccabile.
3. **Pulsante imbuto** (`#mobileCullingBtn`) — solo su Foto; porta a `state.view='culling'`.
   **Non azzera `cullingBatchId`**: se un lotto era aperto ci si ritrova dentro.
4. **Avatar account** (`#mobileAccountBtn`) — apre/chiude il menu account
   (`state.mobileAccountOpen`), con `e.stopPropagation()`.
5. **`"Profilo"`** (`[data-mobaccount="profilo"]`) — chiude il menu, `state.view='profilo'`.
6. **`"Impostazioni"`** (`[data-mobaccount="impostazioni"]`) — chiude il menu,
   `state.view='impostazioni'`.
7. **`"Esci"`** (`[data-mobaccount="esci"]`) — chiude il menu, mostra il toast, resta dov'era.
8. **Scheda `"Foto"`** — `state.currentFolder = null` (commento: *"Foto" torna sempre alla
   timeline combinata*) e `state.view='foto'`.
9. **Scheda `"Cerca"`** — `state.view='cerca'`. Non porta il focus in nessun campo.
10. **Scheda `"Album"`** — `state.openAlbum = null` e `state.view='album'`.
11. **Scheda `"Altro"`** — `state.view='libreria'`.

Nessun altro controllo: **niente menu a tre puntini nell'header, niente pulsante di caricamento
foto, niente barra di ricerca in cima** — non previsti nel mockup.

### 4. Interazioni da mouse (e da tocco)
- **Tap** su freccia indietro, imbuto, avatar, voci del menu e schede.
- **Tap fuori** dal menu account: lo chiude
  (`document.addEventListener('click', …)` su `state.mobileAccountOpen`).
- **Hover**: `.mobile-back:hover` e `.mobile-icon-btn:hover` prendono
  `background:var(--chip-bg)` — visibile solo con un mouse, ininfluente al tocco. `.mobile-tab`
  **non ha regola hover**.
- **Tocco prolungato**: **non implementato sui controlli della shell**. Esiste solo sulle tile
  foto (in `bindTile`, ramo `state.device==='mobile'`, per entrare in selezione multipla).
- **Scorrimento (swipe)**: **non previsto** — non si cambia scheda scorrendo lateralmente, non si
  torna indietro con lo swipe dal bordo.
- **Tasto destro / doppio tap / trascinamento**: nessun comportamento — **non previsti**.
- **Tooltip `[data-tip]` (SP-7)**: assenti su mobile per definizione; il pulsante imbuto si
  affida al solo `aria-label="Culling"`.

### 5. Interazioni da tastiera
- **Nessuna.** Freccia indietro, pulsante imbuto, avatar, voci di menu e schede sono tutti
  `<div>` senza `tabindex`, senza `role` e senza `bindActivatable`.
- La regola `.mobile-tab:focus-visible{outline:2.5px solid var(--accent);outline-offset:2px}`
  esiste ma è **irraggiungibile**.
- **Escape non chiude il menu account mobile**: nessun gestore `keydown`. Stessa deviazione da
  SP-14 già segnalata per il menu desktop.

### 6. Animazioni e transizioni
- **Nessuna animazione nell'header, nella tab bar o nel menu account mobile**: tutto è
  riscritto via `innerHTML` a ogni render, quindi i cambi sono istantanei. Non c'è scivolamento
  laterale tra schede, non c'è dissolvenza del menu — **non previsti nel mockup**.
- Il cambio Desktop↔Mobile non è animato (vedi sezione 1).
- L'unica cosa animata raggiungibile da qui è il **toast** di `"Esci"` (SP-6, `.2s ease`,
  visibile 2400 ms).

### 7. Stati per ogni controllo
- **`.mobile-back`** — *normale*: `color:var(--text-secondary)`, sfondo trasparente.
  *Hover*: `background:var(--chip-bg)`. *Assente*: quando la vista è una radice
  (`foto`, `cerca`, `libreria`) o la griglia Album — la freccia **non viene proprio disegnata**,
  non è una versione disabilitata.
- **`.mobile-icon-btn`** — *normale* `color:var(--text-secondary)`; *hover* fondo grigio;
  *con badge* quando ci sono foto da valutare; *assente* fuori dalla vista Foto.
- **`.mobile-account-btn`** — nessuno stato visivo proprio oltre a `cursor:pointer`; l'avatar non
  cambia aspetto quando il menu è aperto (**nessun indicatore "menu aperto"**).
- **`.mobile-tab`** — *normale*: `color:var(--text-tertiary)`.
  *Attiva* (`.active`): `color:var(--accent)` — testo **e** icona arancioni, **senza** fondo né
  bordino (trattamento diverso da quello della sidebar desktop, che usa fondo grigio + bordino).
  *Hover / premuta / disabilitata*: non previste. Nessuna scheda è mai disabilitata.
  **Come si calcola l'attiva**: `album` se `state.view==='album'`; `libreria` se la vista è una di
  `['libreria','cartelle','persone','mappa','preferiti','condivisioni','cestino','duplicati',
  'problemi','tagManager','revisione','analisiLibreria','profilo','impostazioni']`;
  `foto` se la vista è `culling` o `bulkEdit`; altrimenti la vista stessa. Con
  `state.view==='createAlbum'` nessuna scheda risulta attiva.
- **Menu account mobile** — stessi stati del menu desktop (sezione 3): solo *normale* e *hover*.

### 8. Da dove ci si arriva e dove si va
- **In ingresso**: si entra nella shell mobile solo con il commutatore `"Mobile"` sopra la
  finestra. Il cambio conserva `state.view`: si resta nella stessa vista, cambia solo il telaio.
- **In uscita**: le quattro schede coprono Foto, Cerca, Album e "Altro"; da "Altro" si raggiunge
  tutto il resto (sezione 6); l'avatar apre le pagine di account; la freccia indietro risale di
  un livello secondo le tre regole descritte.
- **Attenzione**: le viste `libreria` e `cartelle` **esistono solo nella shell mobile** ma restano
  montate anche se si torna a Desktop — in quel caso si vedono senza breadcrumb e senza voce di
  sidebar attiva.

### 9. Dati necessari a questa schermata
**Legge**
- Vista corrente, cartella corrente, lotto di culling corrente, album corrente, persona corrente
  (per titolo e freccia indietro).
- Scheda di condivisione corrente (`state.shareTab`) per scegliere fra i due titoli.
- Numero di foto da valutare in tutti i lotti (badge dell'imbuto).
- Iniziali dell'utente (**`"GM"`**, statiche nel mockup) e colore avatar scelto.
- Stato di apertura del menu account mobile.

**Scrive**
- `state.view`, `state.currentFolder` (azzerata dalla scheda "Foto"), `state.openAlbum` (azzerata
  dalla scheda "Album" e dalla freccia indietro nel dettaglio album),
  `state.mobileAccountOpen`.
- **Non scrive** filtri, selezione, persona aperta: vedi le divergenze in sezione 7.

---

## 6. Pagina "Altro" / Libreria su mobile

### 1. Nome e scopo
Pagina di sola navigazione, raggiunta dalla quarta scheda **`"Altro"`**, che elenca tutte le
sezioni non presenti nella tab bar, raggruppate come nella sidebar desktop. Include anche la
sotto-pagina **`"Cartelle"`** che ne discende.

### 2. Cosa mostra
Un elenco piatto diviso in tre gruppi. Ogni gruppo ha un'etichetta
`.libreria-section-label` (`11px`, `font-weight:700`, `text-transform:uppercase`,
`letter-spacing:.04em`, `color:var(--text-tertiary)`, `margin:18px 2px 8px`; il primo ha
`margin-top:2px`) e una `.libreria-list` (bordo `1px solid var(--border)`, `border-radius:12px`,
`overflow:hidden`, `margin-bottom:18px`).

Ogni riga `.libreria-row` (`padding:13px 14px`, `gap:12px`, bordo inferiore `1px`, l'ultima senza)
contiene, da sinistra a destra:
- `.libreria-row-ico` — riquadro `30×30`, `border-radius:8px`, fondo `var(--chip-bg)`,
  `color:var(--text-secondary)`, con l'icona a `16px`;
- `.libreria-row-label` — etichetta, `flex:1`, `13.5px`, `font-weight:600`;
- `.libreria-row-sub` — valore secondario opzionale (`11.5px`, `var(--text-tertiary)`);
- `.nav-badge` — badge rosso opzionale;
- `.libreria-row-chev` — chevron `chevronRight` 15px, `color:var(--text-tertiary)`.

**Contenuto esatto, gruppo per gruppo:**

**Gruppo `"Libreria"`**
| Etichetta | Icona | Valore secondario | Badge |
|---|---|---|---|
| **`"Cartelle"`** | `folder` | **`3`** (= numero di cartelle) | — |
| **`"Persone"`** | `user` | — | — |
| **`"Mappa"`** | `map` | — | — |
| **`"Preferiti"`** | `heart` | — | — |
| **`"Condivisi con me"`** | `share` | — | — |
| **`"Le mie condivisioni"`** | `share` | — | — |

**Gruppo `"Manutenzione"`**
| Etichetta | Icona |
|---|---|
| **`"Cestino"`** | `trash` |
| **`"Duplicati"`** | `copy` |
| **`"Problemi"`** | `alert` |

**Gruppo `"IA"`**
| Etichetta | Icona | Badge |
|---|---|---|
| **`"Tag e categorie"`** | `tag` | — |
| **`"Revisione"`** | `inbox` | **sì, solo se > 0** — `pendingSuggestionCount()` |
| **`"Analisi libreria"`** | `activity` | — |

Il commento in testa a `renderLibreriaMenu` spiega tre scelte:
*(1) SOLO navigazione, raggruppata in sezioni etichettate che rispecchiano 1:1 i gruppi del
sidebar desktop; (2) niente più riga "profilo" qui sopra — duplicava l'avatar/account già sempre
presente in alto a destra su ogni schermata mobile; (3) niente più accordion da aprire: da mobile
la pagina scorre normalmente, un elenco piatto e ben etichettato è più rapido da scandire di voci
che si espandono una alla volta.*

**Sotto-pagina `"Cartelle"`** (`renderCartelle`), raggiunta dalla prima riga:
- Titolo `.section-title`: **`"Le tue cartelle"`**;
- Sottotitolo `.section-sub`: **`"Struttura reale su disco — la fonte di verità, non una
  selezione curata"`**;
- Griglia `.folder-cards` (su mobile forzata a **2 colonne**), una `.folder-card` per cartella con:
  - copertina `.folder-card-cover` — non è una foto vera ma un gradiente
    `linear-gradient(135deg, colorA, colorB)` preso dalla **prima foto** della cartella;
  - `.folder-card-title` — il nome: **`"Urbino"`**, **`"Lago di Braies"`**,
    **`"Chioggia e Venezia"`**;
  - `.folder-card-sub` — **`"556 foto"`**, **`"110 foto"`**, **`"246 foto"`**.

### 3. Ogni controllo, uno per uno
Ogni riga è un unico bersaglio cliccabile (`[data-lib]`). Effetto al click:
1. **`"Cartelle"`** → `state.view='cartelle'` (la sotto-pagina qui sopra).
2. **`"Persone"`** → `state.view='persone'`.
3. **`"Mappa"`** → `state.view='mappa'`.
4. **`"Preferiti"`** → `state.view='preferiti'`.
5. **`"Condivisi con me"`** → **due scritture**: `state.shareTab='withme'` **e**
   `state.view='condivisioni'`.
6. **`"Le mie condivisioni"`** → `state.shareTab='mine'` **e** `state.view='condivisioni'`.
7. **`"Cestino"`** → `state.view='cestino'`.
8. **`"Duplicati"`** → `state.view='duplicati'`.
9. **`"Problemi"`** → `state.view='problemi'`.
10. **`"Tag e categorie"`** → `state.view='tagManager'`.
11. **`"Revisione"`** → `state.view='revisione'`.
12. **`"Analisi libreria"`** → `state.view='analisiLibreria'`.
13. **Scheda cartella** (`[data-jumpfolder]`, nella sotto-pagina "Cartelle") →
    `state.currentFolder = <id>` e `state.view='foto'`.

**Nessuna riga applica il reset di sezione** della sidebar desktop: `state.view = id` e basta
(più `shareTab` per le due voci di condivisione). Filtri rapidi, selezione multipla e persona
aperta sopravvivono.

Non c'è nessun campo di ricerca in questa pagina, nessun interruttore, nessuna voce di account
(rimossa apposta), nessuna voce "Esci".

### 4. Interazioni da mouse (e da tocco)
- **Tap/click** sulla riga intera (non solo sull'etichetta o sulla chevron).
- **Hover**: `.libreria-row:hover{background:var(--chip-bg)}` — immediato, nessuna transizione.
  `.folder-card` non ha regola hover dichiarata in questa pagina.
- **Tasto destro, doppio click, trascinamento, tocco prolungato, rotellina**: nessun
  comportamento specifico — **non previsti** (solo lo scorrimento verticale nativo della pagina).

### 5. Interazioni da tastiera
- **Nessuna.** `.libreria-row` e `.folder-card` sono `<div>` senza `tabindex`, senza `role` e
  senza `bindActivatable`: non entrano nell'ordine di Tab e non rispondono a Invio/Spazio.
  A differenza di `.nav-item` e `.mobile-tab`, qui **non esiste nemmeno una regola
  `:focus-visible`**.

### 6. Animazioni e transizioni
- **Nessuna animazione attiva in questa pagina.** Le righe compaiono già tutte aperte.
- Residui CSS dell'accordion rimosso, **oggi non usati da nessun markup**:
  `.libreria-row-toggle .libreria-row-chev .ico{transition:transform .15s ease}`,
  `.libreria-row-toggle.open .libreria-row-chev .ico{transform:rotate(180deg)}`,
  `.libreria-subrow{padding-left:30px}` (con icona `26×26` ed etichetta `13px/500`) e
  `.libreria-row.danger .libreria-row-label{color:var(--danger)}`.

### 7. Stati per ogni controllo
- **`.libreria-row`** — *normale* come descritto sopra; *hover* fondo `var(--chip-bg)`;
  *nessuno stato "attivo"*: la riga della sezione in cui ci si trova **non è evidenziata** (a
  differenza della sidebar desktop) — del resto entrando in una sezione si lascia questa pagina.
  *Focus / premuta / disabilitata / in caricamento*: non previste.
- **`.libreria-row-sub`** — presente **solo** su `"Cartelle"`; mostra il numero di cartelle.
- **`.nav-badge` di `"Revisione"`** — presente solo con coda > 0; sparisce a coda vuota.
- **Stato vuoto** — la pagina è un elenco statico: non ha mai stato vuoto. La sotto-pagina
  "Cartelle" mostrerebbe titolo e sottotitolo con griglia vuota se non ci fossero cartelle
  (**nessun messaggio di elenco vuoto previsto**); inoltre `renderCartelle` legge
  `photosFor(f.id)[0]` senza guardia: **una cartella senza foto farebbe fallire il render**.

### 8. Da dove ci si arriva e dove si va
- **In ingresso**: solo dalla scheda **`"Altro"`** della tab bar mobile, oppure dalla freccia
  indietro dell'header quando ci si trova in una vista che non è una radice (che porta sempre a
  `libreria`). La sotto-pagina `"Cartelle"` si raggiunge dalla prima riga; da lì la freccia
  indietro riporta ad "Altro".
- **In uscita**: verso una qualsiasi delle dodici sezioni elencate, o (dalle schede cartella)
  verso Foto filtrata su quella cartella — che è una **vista radice**, quindi da lì la freccia
  indietro sparisce.
- Su desktop non esiste un equivalente: il ruolo è svolto direttamente dalla sidebar.

### 9. Dati necessari a questa schermata
**Legge**
- Numero di cartelle (valore secondario della riga `"Cartelle"`).
- Numero di suggerimenti IA in attesa (badge `"Revisione"`).
- Per la sotto-pagina "Cartelle": per ogni cartella **nome**, **numero totale di foto** e i due
  colori della prima foto (surrogato della copertina; nel prodotto reale sarebbe la miniatura di
  copertina).

**Scrive**
- `state.view` e, per le due voci di condivisione, `state.shareTab`.
- Dalla sotto-pagina "Cartelle": `state.currentFolder` e `state.view='foto'`.

---

## 7. Router e regole di ripristino dello stato quando si cambia sezione

### 1. Nome e scopo
`renderAll()` è il router e l'unico punto di ridisegno dell'applicazione: decide quale vista
montare in `#viewRoot` in base a `state.view`, ridisegna sempre tutta la navigazione, e —
insieme agli handler di navigazione — definisce **cosa viene azzerato e cosa sopravvive** al
cambio di sezione.

### 2. Cosa mostra
`renderAll()` non ha una resa propria; esegue in ordine fisso:

1. **Rilevamento del cambio di vista** — se `state.view !== _lastRenderedView`, scrive
   `state.lastNavAt = Date.now()` e `state.forceRunningOnce = false`. Il commento spiega il
   perché: *traccia i cambi di VISTA (non ogni render) per la pausa automatica del pannello
   "Analisi libreria": ogni volta che l'utente naviga altrove, l'analisi si mette in pausa e
   riprende da sola dopo qualche secondo di inattività.*
2. `renderSidebar()` → 3. `renderTopbar()` → 4. `renderMobileHeader()` → 5.
   `renderMobileTabbar()`. **Vengono ridisegnati sempre tutti e quattro**, anche quelli nascosti
   dal form factor corrente.
6. `#app` riceve/perde la classe `.selection-active` a seconda di `state.selectionMode`.
7. `#viewRoot` riceve/perde tre classi:
   - `.no-pad` se `view==='mappa'` **oppure** (`view==='culling'` **e** un lotto è aperto);
   - `.hide-native-scrollbar` se `view==='foto'`;
   - `.has-scrubber` se `view==='foto'` **e** `state.device !== 'mobile'`.
8. `root.innerHTML = ''` — la vista precedente viene **buttata via interamente**, senza
   animazione e senza conservare la posizione di scorrimento.
9. Viene invocato il renderer della vista.
10. `layoutJustifiedGrids(root)` — ricalcola la geometria delle griglie foto (giustificata su
    desktop, colonne fisse su mobile).
11. Se `state.lightbox` è valorizzato monta il visore, altrimenti svuota `#lightboxRoot`.

**Tabella di instradamento completa** (`state.view` → renderer):

| `state.view` | Vista montata |
|---|---|
| `foto` | `renderFoto` |
| `culling` | `renderCulling` |
| `cerca` | `renderCerca` |
| `mappa` | `renderMappa` |
| `condivisioni` | `renderCondivisioni` |
| `preferiti` | `renderPreferiti` |
| `album` | `renderAlbumDetail` se `state.openAlbum` è valorizzato, altrimenti `renderAlbum` |
| `persone` | `renderPersonDetail` se `state.openPerson` è valorizzato, altrimenti `renderPersone` |
| `cestino` | `renderCestino` |
| `duplicati` | `renderDuplicati` |
| `problemi` | `renderProblemi` |
| `tagManager` | `renderTagManagement` |
| `revisione` | `renderRevisione` |
| `analisiLibreria` | `renderAnalisiLibreria` |
| `profilo` | `renderProfilo` |
| `impostazioni` | `renderImpostazioni` |
| `libreria` | `renderLibreriaMenu` (solo-mobile per progetto) |
| `cartelle` | `renderCartelle` (solo-mobile per progetto) |
| `bulkEdit` | `renderBulkEdit` |
| `createAlbum` | `renderCreateAlbum` |
| **qualsiasi valore non mappato** | **`renderFoto`** (fallback silenzioso, `renderers[state.view] || renderFoto`) |

**Stato di partenza** (`state` alla riga 2108): `theme:'light'`, `view:'foto'`,
`currentFolder:null`, `device:'desktop'`, `lbInfoOpen:true`, `lbRawMode:'raw'`,
`cullingFilter:'all'`, `cullingRootFolder:'/volume1/Foto/Culling'`, `personGroupFilter:'all'`,
`revisioneTab:'tag'`, `faceRecognitionEnabled:true`, `aiTier:'pieno'`,
`analysisProgress:{done:128450, total:214000}`. Il commento su `currentFolder` chiarisce il
modello mentale: *null = "tutte le foto" (Foto in nav mostra sempre questo di default, combinando
le cartelle) — diventa l'id di una cartella specifica solo scegliendola da "Cartelle" nella
sidebar, o da un altro punto dell'app che salta a una cartella precisa.*
All'avvio del file vengono eseguiti in sequenza `applyTheme()` e `renderAll()`.

### 3. Ogni controllo, uno per uno — **le regole di reset**

**A) Click su una voce di sidebar (`[data-nav]`) — questa è la regola importante.**
```
state.view = <id della voce>
se id === 'foto'  →  state.currentFolder = null
state.openAlbum = null
state.openPerson = null
state.personSelectedIds.clear()
state.userMenuOpen = false
state.browseFilterOpen = false
resetBrowseFilters()          // type, personIds, tagIds, categoryIds, cameras, folderIds → []
state.browseFilterQuery = {}
renderAll()
```
In parole semplici: **cliccare una voce di sidebar riporta la sezione a uno stato pulito** —
chiude l'album o la persona che erano aperti, svuota la selezione delle persone, chiude il
pannello imbuto e **azzera tutti i filtri rapidi** (SP-3) insieme alle ricerche testuali interne
al pannello.

Il commento nel codice motiva esplicitamente l'azzeramento dei filtri: *il filtro rapido è scoped
alla vista: cambiando sezione dalla sidebar riparte pulito, così non resta un filtro dimenticato
ad "assottigliare" silenziosamente un'altra vista.*

**Cosa invece NON viene azzerato** (sopravvive al cambio di sezione):
- `state.selectedIds` e `state.selectionMode` → **la selezione multipla di foto e la barra
  "N selezionate" (SP-2) restano attive passando a un'altra sezione**;
- `state.cullingBatchId`, `state.cullingFilter`, `state.cullingIdx`,
  `state.cullingSelectedIds`, `state.cullingSelectAnchor` → rientrando in Culling si riapre
  l'ultimo lotto, con lo stesso filtro e la stessa posizione;
- `state.cercaQuery`, `state.cercaFilters`, `state.searchPills` → la ricerca resta scritta;
- `state.personGroupFilter`, `state.revisioneTab`, `state.shareTab` → ogni pagina ricorda la sua
  scheda/filtro interno;
- `state.lightbox` → in teoria il visore resterebbe aperto sopra la nuova vista;
- `state.theme`, `state.device`, `state.aiTier`, `state.avatarColor`,
  `state.faceRecognitionEnabled`, `state.gridDensity` → preferenze, per definizione persistenti.

**B) Click su una riga cartella (`[data-folder]`) — regola diversa.**
```
state.currentFolder = <id>
state.view = 'foto'
state.userMenuOpen = false
renderAll()
```
**Nessun azzeramento di filtri, album, persona o selezione.** Entrare in una cartella conserva
quindi il filtro rapido eventualmente attivo su Foto.

**C) Click su una scheda della tab bar mobile (`[data-mobtab]`).**
```
se id === 'album' → state.openAlbum = null
se id === 'foto'  → state.currentFolder = null
state.view = id
renderAll()
```
**Nessun reset di filtri, `openPerson`, `personSelectedIds` o selezione multipla.**

**D) Click su una riga della pagina "Altro" (`[data-lib]`).**
`state.view = id` (più `state.shareTab` per `"Condivisi con me"` / `"Le mie condivisioni"`).
**Nessun reset di alcun genere.**

**E) Click su una scheda cartella nella sotto-pagina "Cartelle" (`[data-jumpfolder]`).**
`state.currentFolder = <id>`, `state.view='foto'`. **Nessun reset.**

**F) Voci del menu account (desktop e mobile).**
`state.view = 'profilo' | 'impostazioni'`. **Nessun reset.**

**G) Freccia indietro mobile.** Le tre regole già viste in sezione 5: `openAlbum=null`, oppure
`view='foto'`, oppure `view='libreria'`. **Nessun altro reset.**

**H) `setDevice(d)`.** Scrive `state.device`, aggiorna le classi di `#frameOuter`/`#app` e
l'`.active` del controllo segmentato, poi `renderAll()`. **La vista corrente non cambia mai.**

**I) `applyTheme()`.** Scrive solo `document.getElementById('app').setAttribute('data-theme',
state.theme)`. Viene chiamata all'avvio e da Impostazioni → **`"Aspetto"`**, il cui controllo
segmentato ha le tre opzioni **`"Chiaro"`**, **`"Scuro"`**, **`"Sistema"`**; `"Sistema"` legge
`window.matchMedia('(prefers-color-scheme: dark)')` una volta sola al click (**non si aggiorna se
il sistema cambia tema dopo** — non previsto nel mockup). `state.themePref` conserva la scelta
(`chiaro` di default), `state.theme` il valore effettivo (`light`/`dark`).

**J) Gestori di click globali sul documento** (tre, tutti registrati a livello di modulo):
1. chiude il menu account desktop (`renderUserMenu()`);
2. chiude il menu "altre azioni" del visore (`state.lbMoreOpen` → `renderAll()`);
3. chiude la picklist "Cartella" della creazione album, **solo se `state.view==='createAlbum'`**
   (commento: *si chiude cliccando fuori o con Esc, come il resto dei pannelli a tendina
   dell'app*);
   più un quarto, definito accanto alla shell mobile, che chiude il menu account mobile.

### 4. Interazioni da mouse
Il router non ha una superficie propria: risponde solo alle chiamate degli handler descritti
sopra. Nessun gesto del mouse lo raggiunge direttamente. **Non c'è cronologia di navigazione del
browser**: niente `history.pushState`, niente URL per vista, quindi **il tasto Indietro del
browser non torna alla sezione precedente** — non previsto nel mockup.

### 5. Interazioni da tastiera
- Nessuna scorciatoia di navigazione globale (né per cambiare sezione, né per tornare indietro).
- L'unica navigazione da tastiera "di sistema" implementata è nelle viste specifiche (Culling,
  visore), non nella shell.

### 6. Animazioni e transizioni
- **Il cambio di vista non è animato**: `root.innerHTML=''` più il render sostituiscono il
  contenuto in un solo fotogramma. Non c'è dissolvenza né scorrimento; **non previsto nel
  mockup**.
- Il ripristino della posizione di scorrimento **non è implementato**: tornando in una sezione si
  riparte dall'alto.
- L'unico effetto legato al cambio di vista è indiretto e non visivo nella shell: la **pausa
  automatica** del pannello "Analisi libreria" (via `state.lastNavAt`).

### 7. Stati per ogni controllo
- **`#viewRoot`** — quattro combinazioni: normale (padding `22px 24px 40px`, scroll proprio),
  `.no-pad` (Mappa, lotto di culling aperto), `.hide-native-scrollbar` (Foto),
  `.has-scrubber` (Foto su desktop: smette di scrollare e cede lo scroll a `.foto-scroll`).
- **`#app.selection-active`** — attivo mentre `state.selectionMode` è vero: rende permanenti le
  spunte sulle tile.
- **Stato di errore del router** — non esiste come stato visibile: un `state.view` sconosciuto
  ricade silenziosamente su Foto.
- **Stato di caricamento** — non previsto: nessun renderer è asincrono.

### 8. Da dove ci si arriva e dove si va
`renderAll()` è chiamato da **ogni** handler dell'applicazione che modifica lo stato (oltre 60
punti di chiamata) più una volta all'avvio. È il collo di bottiglia unico attraverso cui passa
qualunque transizione fra schermate. `renderSidebar()` e `renderMobileAccountMenu()` sono gli
unici render invocabili **da soli**, per aggiornamenti che non devono ricostruire la vista
(apertura/chiusura dei gruppi a scomparsa e dei menu account).

### 9. Dati necessari a questa schermata
**Legge**
- `state.view` (quale schermata montare), `state.device` (regole desktop/mobile),
  `state.openAlbum` e `state.openPerson` (griglia o dettaglio), `state.cullingBatchId`
  (padding della vista Culling), `state.selectionMode`, `state.lightbox`,
  `state.gridDensity[device]` (numero di colonne per la geometria delle griglie).

**Scrive**
- `state.lastNavAt` (momento dell'ultima navigazione reale) e `state.forceRunningOnce=false` a
  ogni cambio di vista;
- indirettamente, tramite gli handler di navigazione: `state.view`, `state.currentFolder`,
  `state.openAlbum`, `state.openPerson`, `state.personSelectedIds`, `state.browseFilters`,
  `state.browseFilterOpen`, `state.browseFilterQuery`, `state.shareTab`, `state.userMenuOpen`,
  `state.mobileAccountOpen`, `state.navMaintOpen`, `state.navAiOpen`, `state.device`,
  `state.theme` / `state.themePref`.

---

# Parte II — Sfogliare la libreria

Tutto ciò che segue è letto da `/home/claude/keeppix/index.html`. Dove una cosa **non** è
implementata nel mockup, è scritto esplicitamente.

Tre pattern condivisi **nascono in queste schermate** e sono quindi documentati qui per esteso,
come definizione canonica: **SP-1** (il tile fotografico), **SP-2** (selezione multipla e barra
azioni), **SP-3** (filtro rapido a chip). Gli altri pattern che compaiono qui — **SP-4**
(seleziona tutto quello che vedi), **SP-5** (dialog modale), **SP-6** (toast), **SP-7** (tooltip
`[data-tip]`), **SP-8** (attivabile da tastiera), **SP-9** (stelle), **SP-15** (badge RAW),
**SP-16** (avatar), **SP-17** (shell mobile) — sono solo richiamati per codice.

---

## 8. Foto / Timeline

### 1. Nome e scopo

La vista d'ingresso dell'app (`state.view === 'foto'`, è anche il fallback quando la vista
richiesta non esiste): la timeline delle foto raggruppate per mese, o di **tutte** le cartelle
insieme (`state.currentFolder === null`, caso predefinito) o di **una sola** cartella scelta da
"Cartelle" nella sidebar.

### 2. Cosa mostra

Dall'alto verso il basso, su desktop:

- **Barra strumenti** (`.grid-toolbar`). Fuori dalla selezione multipla è una riga a due estremi
  (`.grid-toolbar-row`, `justify-content:space-between`):
  - a **sinistra**, solo se è aperta una cartella specifica, il pulsante `"Rinomina cartella…"`;
    se si sta guardando "tutte le foto" al suo posto c'è uno `<span>` vuoto che tiene
    l'allineamento;
  - a **destra** (`.grid-toolbar-right`, gap 6px) la coppia di azioni rapide della griglia:
    "Seleziona tutto quello che vedi" (SP-4) e il pulsante imbuto del filtro rapido (SP-3).
    L'intero blocco a destra sparisce se la lista di partenza è vuota; il solo pulsante
    "Seleziona tutto" sparisce se la lista *visibile* (dopo i filtri) è vuota.
  In selezione multipla l'intera riga è sostituita dalla barra "N selezionate" (SP-2).
- **Un blocco per ogni mese** presente (`.month-block`, `data-month-idx`), in ordine cronologico
  decrescente. Ogni blocco contiene:
  - **intestazione di gruppo** (`.month-head`): il **nome esteso del mese seguito dall'anno** —
    `"Luglio 2026"`, `"Giugno 2026"`, … (`MONTHS_FULL[monthIdx] + ' 2026'`, 16px, peso 700) — e
    accanto, allineato alla stessa baseline, il **conteggio degli scatti del mese**: `"137
    scatti"` (12.5px, colore terziario). L'anno è **fisso a 2026**, non deriva da un dato della
    foto.
  - la **griglia giustificata** delle foto del mese (`.photo-grid`), tile SP-1.
- **Barra laterale dei mesi** (scrubber) sul bordo destro, solo desktop — vedi sotto.
- Nessuna intestazione di sezione ("Foto") e nessun sottotitolo: la timeline parte direttamente
  dalla barra strumenti. Nessuna indicazione del totale complessivo di foto in vista (il totale
  compare solo nel piede del pannello filtri).

**Raggruppamento** (`monthBlocksHTML`): le foto sono raggruppate per `p.monthIdx` — il **mese di
calendario reale** (0 = Gennaio … 11 = Dicembre) — e i gruppi sono ordinati per
`monthDistance(monthIdx) = (6 - monthIdx + 12) % 12`, cioè per distanza dal mese "corrente" della
demo (Luglio, indice 6): prima Luglio, poi Giugno, Maggio… Il commento nel codice spiega il
perché: `monthOffset` è relativo alla singola cartella in cui è stato generato il catalogo demo,
quindi due cartelle diverse possono avere lo stesso `monthOffset` per mesi reali diversi e non è
confrontabile quando la timeline combina più cartelle; `monthIdx` invece lo è sempre.

**Nessun raggruppamento per giorno**: le foto sono raggruppate solo per mese, e dentro il mese
sono nell'ordine in cui arrivano dalla lista (nessun ordinamento esplicito per `p.day`).

**Nessun tetto: la timeline è completa e virtualizzata.** Ogni mese contiene tutte le sue foto e
l'intestazione dichiara il conteggio reale. A restare piccolo non è la libreria ma il numero di
tessere vive nel documento: la geometria è calcolata in anticipo per l'intera libreria, e solo le
righe che ricadono nella finestra visibile esistono davvero. La descrizione completa del
meccanismo — e cosa comporta per il backend — è nella **Parte X, "Scala, caricamento ed
errore"**. Qui basti sapere che scorrendo non c'è nessun "mostra altre" da premere, nessun salto
della barra di scorrimento e nessun limite raggiungibile.

**Stato vuoto**: se i filtri escludono tutto, al posto della griglia compare
`empty-state` con icona imbuto, titolo `"Nessuna foto corrisponde ai filtri"` e sottotitolo
`"Prova ad allargare i filtri, o cancellali dal pannello qui sopra."`. Non esiste uno stato
vuoto per "libreria vuota": nel mockup la libreria ha sempre foto.

### 3. Ogni controllo, uno per uno

| Controllo | Tipo | Cosa fa |
|---|---|---|
| `"Rinomina cartella…"` (icona matita 13px, `.btn.btn-sm.btn-ghost`) | pulsante | apre il dialog di rinomina con formula su tutte le foto della cartella corrente (`openRenameDialog({kind:'folder', …})`). Compare **solo** con una cartella aperta. |
| Pulsante "Seleziona tutto quello che vedi" (icona `selectAll` 15px) | pulsante icona | SP-4: entra in selezione multipla con tutte le foto **attualmente visibili** già spuntate. |
| Pulsante imbuto "Filtra" | pulsante icona + pannello | SP-3, definizione canonica alla sezione 4 di questo documento. |
| Intestazione di mese (`.month-head`) | testo | **non è un controllo**: nessun click, nessun collasso del gruppo, nessuna selezione "tutte le foto di questo mese". |
| Tile foto | vedi SP-1 (sezione 3) | apre il lightbox / seleziona / preferito. |
| Scrubber dei mesi (`.scrubber`) | barra di scorrimento personalizzata | salta al mese, vedi sotto. |
| Barra "N selezionate" | barra azioni | SP-2, definizione canonica alla sezione 5. |

**La barra dei mesi sul bordo destro** (`setupScrubber`), solo desktop:

- È un elemento `.scrubber` alto quanto la vista, largo **34px**, ancorato in alto a destra di
  `#viewRoot` (`position:absolute;top:0;right:0;bottom:0`), `z-index:5`, `cursor:ns-resize`,
  `user-select:none`, padding verticale 8px.
- Contiene:
  - `.scrubber-rail` — il binario: 2px di larghezza, `right:6px`, da 8px dall'alto a 8px dal
    basso, colore `--border-strong`;
  - una `.scrubber-tick` **per ogni mese effettivamente presente** nella lista filtrata,
    etichettata con l'**abbreviazione** del mese (`MONTHS` → `"Lug"`, `"Giu"`, `"Mag"`…), 8.5px,
    colore terziario, scritta in verticale (`writing-mode:vertical-rl`). Le tick sono distribuite
    su tutta l'altezza (`justify-content:space-between`): il commento CSS spiega che prima erano
    impacchettate in alto e "le date non erano ben distanziate";
  - `.scrubber-tooltip` — la targhetta con il mese, `right:26px`, sfondo `var(--text)` su testo
    `var(--bg)` (invertito rispetto al tema), 11.5px peso 600, padding 4px 9px, raggio 6px;
  - `.scrubber-thumb` — il cursore trascinabile, 10×26px, raggio 5px, colore accento,
    `right:2px`, posizione iniziale `top:8px`.
- **È cliccabile**: `rail.onmousedown` è sull'intero elemento `.scrubber`, e la funzione di drag
  viene eseguita **subito al mousedown** — quindi un semplice click a una certa altezza salta
  immediatamente al mese corrispondente, non serve trascinare.
- **Mostra un tooltip, ma solo mentre si trascina/clicca**, non al semplice passaggio del mouse:
  la targhetta contiene il **nome esteso del mese più l'anno** — `"Luglio 2026"` — e viene
  posizionata all'altezza del puntatore. Compare con `.show` (`opacity 0 → 1`, `transition:
  opacity .1s`) al mousedown e sparisce al mouseup.
- Le tick **non sono cliccabili individualmente**: non hanno né handler né attributi dati. Il
  click "sulla tick" funziona solo perché cade dentro l'area della barra e viene interpretato
  posizionalmente.
- **Calcolo**: il rapporto verticale (`clientY` meno il bordo superiore, meno 8px, diviso
  l'altezza utile) viene limitato a 0–1 e trasformato in un indice di mese con
  `Math.round(ratio*(offsets.length-1))`. Non è quindi proporzionale al *numero di foto*: i mesi
  sono equidistanti sulla barra anche se uno contiene 5 foto e un altro 300.
- **Salto**: `scroller.scrollTop = block.offsetTop - 4` — salto **istantaneo**, senza scroll
  animato (`behavior:'smooth'` non è usato da nessuna parte).
- **Sincronizzazione inversa** (`syncThumbToScroll`, agganciata a `scroller.onscroll`): il cursore
  si riposiziona anche quando si scrolla con rotellina, trackpad o tastiera, in proporzione a
  `scrollTop / (scrollHeight - clientHeight)`. Il commento italiano dice perché: *"prima restava
  fermo in alto, sembrava rotto"*. La sincronizzazione è sospesa durante il trascinamento.
- **Non è raggiungibile da tastiera**: la barra non ha `tabindex`, né `role`, né `aria-label`, né
  handler di tastiera.
- **Non ha handler touch**: `mousedown`/`mousemove`/`mouseup` soltanto. Su mobile la barra non
  viene nemmeno costruita (`if(!isMobile && photosList.length) setupScrubber(...)`), e non viene
  costruita neppure se la lista filtrata è vuota.
- I listener sono assegnati per **proprietà diretta** (`rail.onmousedown`, `window.onmousemove`,
  `window.onmouseup`, `scroller.onscroll`) e non con `addEventListener`: il commento spiega che è
  voluto, così ogni nuovo render sostituisce i listener precedenti invece di accumularli — c'era
  un leak che *"rendeva il drag via via meno reattivo dopo diverse interazioni"*.

### 4. Interazioni da mouse

- **Click su una tile** → apre il lightbox della foto; se la selezione multipla è attiva, invece,
  aggiunge/toglie quella foto dalla selezione (SP-1).
- **Click sul cerchietto in alto a sinistra** → seleziona/deseleziona (SP-1/SP-2).
- **Click sul cuoricino in alto a destra** → aggiunge/rimuove dai preferiti, subito, senza
  conferma né toast.
- **Hover su una tile** → compaiono cerchietto di selezione e cuoricino (opacità .12s) e sparisce
  il badge RAW. Nessun ritardo: la transizione parte immediatamente.
- **Hover sui pulsanti icona della barra strumenti** → tooltip SP-7 dopo nessun ritardo
  (`transition:opacity .12s ease, transform .12s ease`).
- **Doppio click**: non implementato in nessun punto della griglia.
- **Tasto destro / menu contestuale**: **non previsto nel mockup**. L'unico `contextmenu`
  intercettato è quello del tap prolungato su mobile, e serve solo a sopprimerlo.
- **Trascinamento di foto (drag & drop)**: **non previsto nel mockup** — non si trascinano foto
  né su cartelle né su album, e non c'è selezione a rettangolo.
- **Trascinamento dello scrubber**: sì, vedi sopra.
- **Rotellina / scroll**: scorre `#fotoScroll` (su desktop) o `#viewRoot` (su mobile) e
  ri-sincronizza il cursore dello scrubber. Nessun caricamento progressivo, nessuna intestazione
  di mese "appiccicata" in alto durante lo scroll (le `.month-head` scorrono via normalmente).
  Su mobile invece la **barra strumenti è sticky** in cima (`position:sticky;top:0;z-index:5`),
  proprio perché il trigger dei filtri resti raggiungibile mentre si scorre.

### 5. Interazioni da tastiera

- **Tab / Shift+Tab**: ordine del DOM. Barra strumenti (rinomina cartella → seleziona tutto →
  imbuto), poi, per ogni tile in ordine di griglia: area di apertura (`.tile-open`), cerchietto di
  selezione (`.tile-check`), cuoricino. Sono tutti `tabindex="0"`: **ogni foto costa 3 tabulazioni**.
- **Invio / Spazio** sull'elemento a fuoco: equivale al click (SP-8).
- **Esc**: chiude il pannello del filtro rapido se aperto (gestore globale). Se il pannello è
  chiuso, Esc nella timeline non fa nulla.
- **Frecce**: **nessuna navigazione con le frecce dentro la griglia** — le frecce ← → sono
  gestite solo dentro il lightbox e dentro il culling. Nella timeline non spostano il fuoco fra
  tile.
- **Nessuna scorciatoia** di selezione: niente Ctrl/Cmd+A, niente Shift+click o Shift+freccia per
  selezionare un intervallo (l'intervallo con Shift esiste **solo** nel filmino del culling,
  `cullingSelectRange`), niente Ctrl/Cmd+click per aggiungere alla selezione.
- **Nessuna scorciatoia** per valutare a stelle, marcare pick/scarta o eliminare dalla timeline
  (i tasti `1`–`5`, `P`, `X`, `Canc` funzionano solo dentro il culling).
- Anello di fuoco: `outline:2.5px solid var(--accent); outline-offset:2px` su tutti gli elementi
  interattivi (`[role="button"]`, `[role="checkbox"]`, `.tile-open`, `.chip`, input…).

### 6. Animazioni e transizioni

- **Cerchietto di selezione** che appare/scompare all'hover o al fuoco: `opacity .12s ease`,
  insieme a `background .12s ease` e `border-color .12s ease` (che servono al passaggio
  non-selezionato → selezionato). *Comunica*: "questa foto si può selezionare".
- **Cuoricino** che appare all'hover/fuoco: `opacity .12s ease`. *Comunica*: azione rapida
  disponibile senza aprire la foto.
- **Badge RAW** che sparisce all'hover/fuoco/selezione: `opacity .12s ease`. *Comunica*: cede il
  posto ai comandi, che occupano lo stesso angolo.
- **Tooltip dello scrubber**: `opacity .1s` (senza curva esplicita → `ease` di default).
  *Comunica*: dove stai atterrando mentre trascini.
- **Tooltip dei pulsanti icona** (SP-7): `opacity .12s ease, transform .12s ease`, con una
  micro-risalita di 3px.
- **Transizione globale di tema**: `#app *{transition:background-color .2s ease, border-color .2s
  ease, color .2s ease}` — riguarda anche tile e chip quando si cambia tema.
- **Nessuna animazione di entrata delle tile** (nessun fade-in progressivo), nessuna transizione
  fra un filtro e l'altro: la griglia viene ridisegnata di colpo. Nessuna animazione sullo scroll
  guidato dallo scrubber.

### 7. Stati per ogni controllo

- **Tile**: normale / hover (comandi visibili, badge nascosto) / focus-within (uguale all'hover) /
  selezionata (contorno accento 2.5px verso l'interno + cerchietto pieno + comandi rimossi dal
  DOM) / in modalità selezione (tutti i cerchietti visibili, `#app.selection-active`). Nessuno
  stato "in caricamento" (la miniatura è un gradiente CSS, non un'immagine) e nessuno stato di
  errore per miniatura mancante.
- **Cerchietto di selezione**: nascosto (opacità 0 e `pointer-events:none`, così non ruba click al
  resto della foto) / visibile / spuntato (`.on`: sfondo accento, bordo bianco, icona di spunta
  12px) / a fuoco.
- **Cuoricino**: nascosto / visibile contorno / visibile pieno (`.on` → `fill:currentColor`,
  resta visibile anche senza hover).
- **"Rinomina cartella…"**: normale / hover (sfondo `--chip-bg`) / focus. Non ha stato
  disabilitato: semplicemente non esiste quando non c'è una cartella aperta.
- **"Seleziona tutto quello che vedi"**: normale / hover / focus. Non viene disegnato quando la
  lista visibile è vuota — non esiste una variante "disabilitata". Il click su lista vuota
  sarebbe comunque un no-op (`if(!list.length) return`).
- **Pulsante imbuto**: normale / hover / **attivo** (`.active`: sfondo `--accent-tint`, colore
  accento, più il pallino con il conteggio) / aperto (`aria-expanded="true"`).
- **Scrubber**: fermo / in trascinamento (tooltip visibile). Nessuno stato di fuoco (non è
  focalizzabile). Non viene renderizzato affatto se la lista filtrata è vuota o su mobile.
- **Griglia**: piena / vuota per filtri (empty state imbuto). Nessuno stato "in caricamento":
  i dati sono generati sincronamente.

### 8. Da dove ci si arriva e dove si va

**In ingresso:**

- È la vista iniziale all'avvio (`state.view:'foto'`) ed è il fallback per qualunque vista non
  mappata.
- Voce `"Foto"` in cima alla sidebar → azzera sempre `state.currentFolder` ("Foto torna sempre
  alla timeline combinata") e azzera anche filtro rapido, ricerche nel pannello e stato aperto
  del pannello.
- Click su una cartella nell'elenco "Cartelle" della sidebar → timeline della sola cartella
  (`state.currentFolder = id`). **Attenzione**: questo percorso **non** azzera il filtro rapido.
- Tab `"Foto"` della tab bar mobile (SP-17) → stessa cosa, `currentFolder = null`.
- Ritorno da "Modifica multipla" (sia con "Applica" sia con "Annulla") → si torna sempre qui.
- Chiusura del lightbox.

**In uscita:**

- Click su una tile → lightbox della foto.
- "Modifica" nella barra di selezione → vista "Modifica multipla".
- "Rinomina cartella…" → dialog di rinomina (modale, si resta nella timeline).
- Pulsanti "Album" / "Condividi" / "Elimina" della barra di selezione → i rispettivi dialog
  modali (SP-5).
- Qualunque voce di navigazione della sidebar o della tab bar.

### 9. Dati necessari a questa schermata

**Legge**, per ogni foto: identificativo; cartella di appartenenza; mese di calendario dello
scatto e giorno del mese (per l'etichetta accessibile); nome del file; proporzione
larghezza/altezza (serve al layout giustificato); colori della miniatura (nel mockup sostituiscono
l'immagine vera); se è RAW e di che tipo (RAW puro o RAW+JPEG, per il badge); se è nei preferiti.
Per i filtri servono in più: fotocamera, tag confermati con la loro categoria, volti confermati
con la persona associata.

Serve inoltre: l'elenco delle cartelle con nome e conteggio; l'elenco dei mesi presenti nella
selezione corrente; la densità di griglia scelta in Impostazioni (numero di colonne desktop e
mobile); l'interruttore globale del riconoscimento volti.

**Scrive**: lo stato "preferita" della singola foto (cuoricino); l'insieme delle foto selezionate
(stato di sessione, non persistito sulla foto). Tutto il resto che parte da qui (valutazione,
pick/scarta, album, tag, titolo, cartella, eliminazione, rinomina) è scritto dalla barra di
selezione, dalla modifica in blocco o dai dialog, non dalla timeline in sé.

---

## 9. Preferiti

### 1. Nome e scopo

Raccolta trasversale di tutte le foto marcate come preferite, da **tutte** le cartelle insieme
(`state.view === 'preferiti'`).

### 2. Cosa mostra

- Titolo di sezione `"Preferiti"` (15px, peso 700).
- Sottotitolo: `"N foto, da tutte le cartelle"` — dove N è il numero **totale** di preferiti,
  **prima** dei filtri rapidi.
- Barra strumenti: a sinistra uno `<span>` vuoto (qui **non** c'è il pulsante "Rinomina
  cartella…" della timeline, perché la vista non è legata a una cartella), a destra la coppia
  "Seleziona tutto" + imbuto. In selezione multipla, la barra "N selezionate" (SP-2).
- Una **griglia piatta** di tile SP-1: **nessun raggruppamento per mese**, nessuna intestazione di
  mese, **nessuno scrubber**. Vale lo stesso layout giustificato della timeline e la stessa
  virtualizzazione (**SP-22**): è la timeline con una sola sezione e senza titolo.

**Due stati vuoti distinti:**

1. **Nessun preferito in assoluto** → si esce subito con l'empty state: icona cuore, titolo
   `"Nessun preferito ancora"`, sottotitolo `"Premi il cuore su una foto per ritrovarla qui."`
   In questo caso **non viene disegnata nemmeno la barra strumenti**, quindi non c'è modo di
   aprire il pannello filtri.
2. **Ci sono preferiti ma i filtri li escludono tutti** → titolo, sottotitolo e barra strumenti
   restano, e al posto della griglia compare l'empty state imbuto: `"Nessuna foto corrisponde ai
   filtri"` / `"Prova ad allargare i filtri, o cancellali dal pannello qui sopra."`

### 3. Ogni controllo, uno per uno

| Controllo | Tipo | Cosa fa |
|---|---|---|
| "Seleziona tutto quello che vedi" | pulsante icona | SP-4, sui preferiti **visibili dopo i filtri**. |
| Pulsante imbuto "Filtra" | pulsante icona + pannello | SP-3; le stesse sei dimensioni della timeline, ma il conteggio nel piede del pannello è calcolato sui soli preferiti. |
| Tile foto | SP-1 | click apre il lightbox; il cuoricino qui **toglie la foto dalla vista**: `p.isFav = false` seguito da `renderAll()` la fa sparire immediatamente dalla griglia, senza conferma, senza toast e senza annulla. |
| Barra "N selezionate" | barra azioni | SP-2 completa, tutti e cinque i pulsanti compresi "Preferiti" (che qui, su una selezione già tutta preferita, **rimuove** tutto dalla vista). |

### 4. Interazioni da mouse

Identiche alla timeline (SP-1): click apre il lightbox o seleziona, hover mostra cerchietto e
cuoricino, nessun doppio click, **nessun menu col tasto destro**, **nessun drag & drop**, nessun
riordino manuale dei preferiti. Lo scroll è quello normale di `#viewRoot` (qui `#viewRoot` **non**
riceve `has-scrubber`, quindi la scrollbar è quella di sistema… con una eccezione: la classe
`hide-native-scrollbar` viene tolta perché è applicata solo quando `state.view==='foto'`).

### 5. Interazioni da tastiera

Identiche alla timeline: Tab attraversa barra strumenti e poi i tre elementi focalizzabili di ogni
tile; Invio/Spazio attivano (SP-8); Esc chiude il pannello filtri. **Nessuna navigazione con le
frecce**, nessuna scorciatoia dedicata alla vista.

### 6. Animazioni e transizioni

Le stesse di SP-1 (`opacity .12s ease` su cerchietto, cuoricino e badge) e di SP-7 per i tooltip.
Nessuna animazione di uscita quando una foto smette di essere preferita: sparisce di colpo al
render successivo.

### 7. Stati per ogni controllo

Come nella timeline. In più: se non ci sono preferiti la schermata è interamente nello stato
"vuoto" e **nessun controllo è presente**; nessun controllo ha uno stato disabilitato esplicito.

### 8. Da dove ci si arriva e dove si va

**In ingresso**: voce `"Preferiti"` nel gruppo "Libreria" della sidebar (icona cuore) — che
azzera il filtro rapido; da mobile passando dalla tab `"Altro"` → elenco Libreria → "Preferiti"
(SP-17).
**In uscita**: click su una tile → lightbox; "Modifica" nella barra di selezione → "Modifica
multipla" (che al ritorno però **atterra sulla timeline Foto**, non su Preferiti — vedi sezione
6); i dialog Album/Condividi/Elimina; qualunque altra voce di navigazione.

### 9. Dati necessari a questa schermata

Gli stessi campi per-foto della timeline, ma la selezione di partenza è "tutte le foto della
libreria dove il flag preferito è vero", senza scoping per cartella. Servono in più il totale dei
preferiti (per il sottotitolo). **Scrive**: il flag preferito (aggiunta/rimozione dal cuoricino o
dalla barra di selezione) e la selezione corrente.

---

## 10. Il tile fotografico — definizione canonica di **SP-1**

### 1. Nome e scopo

`tileHTML(p)`: il mattone di ogni vista a griglia (Foto, Preferiti, Album, dettaglio Persona,
risultati di Cerca). Mostra una foto in miniatura e offre, senza aprirla, le tre azioni rapide:
aprire, selezionare, mettere/togliere dai preferiti.

### 2. Cosa mostra

- **La miniatura**. Nel mockup è un gradiente
  `linear-gradient(135deg, colorA, colorB)` calcolato per foto (`tileStyle`), non un'immagine: nel
  prodotto reale qui va la miniatura. Contenitore `.tile`: raggio 7px, bordo 1px `var(--border)`,
  `overflow:hidden`, `cursor:pointer`. Larghezza e altezza sono impostate **da JS** dal layout
  giustificato, non dal CSS.
- **Badge RAW** in alto a sinistra (SP-15), presente solo se `p.isRaw`: testo `"RAW+JPEG"` se la
  foto ha entrambi i file, `"RAW"` se ha solo il raw (`rawBadgeLabel`). 9.5px peso 700, sfondo
  `rgba(10,10,10,.55)`, testo bianco, padding 2px 6px, raggio 4px, `aria-hidden="true"`.
- **Cerchietto di selezione** in alto a sinistra (stesso angolo del badge): 22×22, cerchio, sfondo
  `rgba(0,0,0,.35)`, bordo 2px `rgba(255,255,255,.9)`. Quando è spuntato contiene un'icona di
  spunta da 12px.
- **Cuoricino** in alto a destra: 22×22 di area, icona da 19px, bianca, con doppia ombra
  (`drop-shadow(0 1px 2px rgba(0,0,0,.65))` + `drop-shadow(0 0 3px rgba(0,0,0,.45))`). Il commento
  spiega la scelta: niente cerchietto di sfondo, ma l'ombra serve perché *"il cuoricino bianco
  spariva sulle foto chiare"*, e il peso visivo deve essere lo stesso del pallino di selezione.
- Nient'altro: **nessun nome file, nessuna data, nessuna valutazione a stelle, nessun indicatore
  pick/scarta, nessun indicatore "in album" o "condivisa" sulla tile**. Quelle informazioni
  esistono solo nell'etichetta accessibile e nel lightbox.

**Etichetta accessibile**: costruita una volta e riusata da entrambi i comandi —
`"<nome file>, <giorno> <mese in minuscolo> 2026"`, con `", preferita"` in coda se la foto è nei
preferiti. Esempio: `Apri DSC08431.ARW, 12 luglio 2026, preferita`.

### 3. Ogni controllo, uno per uno

| Elemento | Attributi | Cosa fa |
|---|---|---|
| `.tile-open` | `role="button"`, `tabindex="0"`, `aria-label="Apri <etichetta>"`, occupa tutta la tile (`inset:0`, `z-index:1`) | fuori dalla selezione multipla apre il lightbox; **dentro** la selezione multipla il click sulla foto **seleziona/deseleziona** invece di aprire (`wireTileOpen` → `if(state.selectionMode) toggleSelect(id)`). |
| `.tile-check` | `role="checkbox"`, `aria-checked`, `aria-label="Seleziona <etichetta>"`, `tabindex="0"`, `z-index:2` | aggiunge/toglie la foto dalla selezione; è il modo canonico per **entrare** in selezione multipla. Ferma la propagazione, così non apre anche il lightbox. |
| `.mini-btn.fav-btn` dentro `.tile-actions` | `role="button"`, `tabindex="0"`, `aria-label` che cambia: `"Aggiungi ai preferiti"` / `"Rimuovi dai preferiti"` | inverte il flag preferito e ridisegna. Ferma la propagazione. |
| `.tile-badge` | `aria-hidden="true"` | puramente informativo, non interattivo. |

**Nota importante**: quando la selezione multipla è attiva, il blocco `.tile-actions` (e quindi il
cuoricino) **non viene proprio generato** — non è nascosto via CSS, è assente dal DOM. Non si può
mettere una foto nei preferiti dalla tile mentre si sta selezionando; si usa il pulsante
"Preferiti" della barra (SP-2).

### 4. Interazioni da mouse

- **Click** sulla tile: apre / seleziona (vedi sopra).
- **Click** sul cerchietto o sul cuoricino: azione dedicata, senza aprire il lightbox.
- **Hover** sulla tile: cerchietto e cuoricino compaiono, badge RAW scompare. Nessun ritardo di
  comparsa.
- **Tap prolungato (solo mobile)**: 500 ms su `.tile-open` → vibrazione di 15 ms
  (`navigator.vibrate(15)`, se supportata) e la foto entra in selezione. Il click sintetico che
  segue il rilascio viene soppresso (`_suppressClick`) così non apre anche il lightbox; il menu
  contestuale del browser viene bloccato (`contextmenu` → `preventDefault`). Il timer viene
  annullato su `pointerup`, `pointerleave`, `pointercancel`.
- **Doppio click, tasto destro, drag & drop**: **non previsti nel mockup**.

### 5. Interazioni da tastiera

- Tre stop di tabulazione per tile, in quest'ordine: apri → cerchietto → cuoricino.
- **Invio e Spazio** attivano ognuno dei tre (SP-8; su `.tile-open` con `preventDefault` per
  evitare lo scroll da Spazio).
- Mettere il **fuoco** su una tile (`:focus-within`) fa comparire i comandi esattamente come
  l'hover: il commento CSS dice che è voluto, *"così chi naviga da tastiera può scoprirlo e
  usarlo"*.
- Nessuna navigazione con le frecce fra tile.

### 6. Animazioni e transizioni

- Cerchietto: `transition: opacity .12s ease, background .12s ease, border-color .12s ease`.
- Cuoricino: `transition: opacity .12s ease`.
- Badge RAW: `transition: opacity .12s ease`.
- Nessuna transizione sul contorno di selezione (`outline`), che appare istantaneo.

### 7. Stati per ogni controllo

- **Tile**: normale · hover · `:focus-within` · `.selected` (`outline:2.5px solid var(--accent);
  outline-offset:-2.5px`, cioè il contorno cade **dentro** la tile e non allarga il layout).
- **Cerchietto**: invisibile e inerte di default (`opacity:0; pointer-events:none` — voluto,
  altrimenti *"l'angolo in alto a sinistra ruberebbe click al resto della foto"*); visibile su
  `.tile:hover`, `:focus-visible`, `.on`, e su **tutte** le tile quando `#app` ha la classe
  `selection-active`; spuntato = sfondo accento, bordo bianco, spunta color `--accent-text`.
- **Cuoricino**: invisibile e inerte di default; visibile su hover, `:focus-within`,
  `:focus-visible` e quando è già preferito (`.on`, che resta visibile sempre); pieno quando `.on`.
- **Badge**: visibile / nascosto (hover, focus-within, tile selezionata). Il commento spiega il
  perché: badge e cerchietto occupano lo stesso angolo e sovrapposti sono illeggibili, quindi la
  label *"cede il posto"*; resta nascosto anche senza hover se la tile è selezionata, perché il
  cerchietto pieno resta comunque visibile.
- Nessuno stato "in caricamento" della miniatura, nessuno stato di errore, nessuno stato
  disabilitato.

### 8. Da dove ci si arriva e dove si va

Il tile non è una schermata: vive dentro Foto, Preferiti, Album, dettaglio Persona e risultati di
Cerca. Da lì porta al **lightbox** (click normale) o alla **selezione multipla** (SP-2).

**Deviazioni note**: nella vista Culling la stessa classe `.tile-badge` è ridefinita più grande
(`.culling-image .tile-badge{top:10px;left:10px;font-size:11px;padding:3px 8px}`) perché lì sta su
un'immagine a piena pagina, non su una miniatura.

### 9. Dati necessari

Per foto: identificativo, colori/miniatura, proporzione, nome file, giorno e mese dello scatto,
se è RAW e di che tipo, se è nei preferiti, e se è nella selezione corrente.
**Scrive**: il flag preferito e l'appartenenza alla selezione.

---

## 11. Filtro rapido a chip — definizione canonica di **SP-3**

### 1. Nome e scopo

Un filtro "sul momento" sulle viste a griglia (Foto, Preferiti, Album, dettaglio Persona): un
pulsante a imbuto nella barra strumenti apre un pannello a sezioni di chip che restringe subito
ciò che si vede. Il commento nel codice lo distingue esplicitamente dalle **condizioni degli album
dinamici** (che sono una regola persistente) e dai filtri della pagina **Cerca** (che sono un'altra
cosa ancora): *"qui non si costruisce una regola persistente, è un filtro sul momento che resta
finché non lo si cancella o non si cambia vista"*.

### 2. Cosa mostra

**Il pulsante** (`.tag-icon-btn.browse-filter-btn`): quadrato 26×26 con raggio 7px, icona imbuto
da 14px, colore terziario. Quando almeno un filtro è attivo prende sfondo `--accent-tint` e colore
accento, e sopra l'angolo in alto a destra compare un **pallino con il numero di filtri attivi**
(`.browse-filter-count`: sfondo accento, testo `--accent-text`, 9.5px peso 700, altezza 14px,
raggio 8px). Il conteggio è la **somma dei valori scelti su tutte le dimensioni**
(`browseFilterActiveCount`), non il numero di dimensioni: scegliere due tag e una fotocamera
mostra `3`.

**Il pannello** (`.browse-filter-panel`): ancorato sotto il pulsante e allineato a destra
(`top:calc(100% + 6px); right:0`), largo 280px (max `calc(100vw - 40px)`), `z-index:8`, sfondo
`--card-bg`, bordo `--border-strong`, raggio 11px, ombra `0 12px 30px rgba(0,0,0,.2)`.

- **Intestazione** (`.browse-filter-head`, 12.5px peso 700, bordo inferiore): l'etichetta
  `"Filtri"` e, **solo se c'è almeno un filtro attivo**, il link `"Cancella tutto"` (11.5px peso
  600, colore accento, sottolineato all'hover).
- **Corpo** (`.browse-filter-body`, `max-height:min(50vh,420px)`, scrollabile): sei sezioni, in
  quest'ordine fisso. Ogni sezione ha un'etichetta 10px maiuscoletto spaziato, colore terziario, e
  una riga di chip.
- **Piede** (`.browse-filter-foot`, 11.5px, sfondo `--chip-bg`, bordo superiore): il **contatore
  di anteprima**, `"N foto con questi filtri"` quando c'è almeno un filtro attivo, `"N foto in
  totale"` quando non ce n'è nessuno. N è calcolato sulla lista **di questa vista**
  (`applyBrowseFilters(scopedList)`), quindi in Preferiti conta solo i preferiti.

**Le sei dimensioni filtrabili** (`browseFilterDimensions`):

| Etichetta della sezione | Valori | Da dove vengono |
|---|---|---|
| `"Tipo"` | `"RAW+JPEG"`, `"RAW"`, `"JPEG"` | costante `BROWSE_FILE_TYPE_OPTIONS`; il tipo di una foto è dedotto dal suo `stackType` (`raw_jpeg` / `raw_only` / altrimenti JPEG). |
| `"Persone"` | una chip per persona | `visiblePeople()` = persone non nascoste con almeno una foto; l'etichetta è il nome visualizzato della persona. **Elenco vuoto se il riconoscimento volti è disattivato** → la sezione non viene disegnata affatto. |
| `"Tag"` | una chip per tag, con **pallino colorato** 8px (`hsl(<colore del tag>,60%,55%)`) prima del nome | l'elenco completo dei tag. |
| `"Categorie"` | una chip per categoria di tag | l'elenco delle categorie. |
| `"Fotocamera"` | una chip per modello di fotocamera distinto | modelli distinti presenti nel catalogo. |
| `"Luogo"` | una chip per cartella | **l'etichetta dice "Luogo" ma i valori sono le cartelle** (`FOLDERS`) e il confronto è su `p.folderId`. Nel mockup le tre cartelle coincidono con tre luoghi ("Urbino", "Lago di Braies", "Chioggia e Venezia"), quindi la finzione regge; nel prodotto reale sono due concetti diversi. |

Quando il riconoscimento volti è spento, sotto le sezioni compare l'avviso
(`.browse-filter-hint`, 11.5px, colore terziario): `"Persone non disponibile: riconoscimento
volti disattivato in Impostazioni."`

### 3. Ogni controllo, uno per uno

| Controllo | Tipo | Cosa fa |
|---|---|---|
| Pulsante imbuto (`aria-label="Filtra"`, `aria-haspopup="true"`, `aria-expanded`) | pulsante icona | apre/chiude il pannello. |
| Pallino del conteggio | badge, non interattivo | numero di valori attivi in totale. |
| `"Cancella tutto"` | link | `resetBrowseFilters()`: svuota tutte e sei le dimensioni in un colpo. Presente solo se c'è qualcosa da cancellare. **Non** azzera i testi digitati nei campi di ricerca delle sezioni. |
| Chip di valore (`.chip`, `role="button"`, `tabindex="0"`) | interruttore | aggiunge/toglie quel valore dalla sua dimensione; ridisegna tutto (`renderAll`) lasciando il pannello aperto, così si vede la griglia cambiare sotto. |
| Campo di ricerca di sezione (`.browse-filter-search`) | campo testo | vedi sotto. |
| Piede del pannello | testo | anteprima del numero di foto risultanti. |
| Scrim (solo mobile) | velo | vedi sezione mobile. |

**Il campo di ricerca per-dimensione**:

- Compare **solo** nelle sezioni con **più di 8 opzioni** (`BROWSE_FILTER_SEARCH_THRESHOLD = 8`).
  Il commento dice perché: *"Solo Tag e Persone possono davvero crescere tanto da servirne una…
  le altre dimensioni restano semplici"*.
- **Placeholder dinamico**: `"Cerca in N…"` dove N è il numero totale di opzioni di quella
  dimensione (es. `Cerca in 24…`).
- Nessuna validazione: è un filtro incrementale, non ha un valore "sbagliato"; lasciato vuoto
  mostra tutte le opzioni. Confronto per **sottostringa, senza distinzione fra maiuscole e
  minuscole**, sull'etichetta, con il testo ripulito dagli spazi ai bordi.
- **Le opzioni già selezionate restano sempre in cima e non vengono mai filtrate via** — il
  commento spiega: *"digitare per trovarne una nuova non fa sparire quelle che hai già
  selezionato"*.
- Nessun risultato → `.browse-filter-noresults` con il testo `Nessun risultato per "<quello che
  hai digitato>"`.
- La sezione con ricerca diventa una **mini-lista scrollabile**: `.browse-filter-section.searchable
  .chip-row{max-height:132px; overflow-y:auto}`, per non allungare il pannello all'infinito.
- Digitare **non ridisegna tutto il pannello**: viene riscritta solo la riga di chip di quella
  sezione e ricablati i suoi handler. Il commento spiega perché: *"altrimenti l'input perderebbe
  il focus a ogni lettera digitata — stesso principio del composer di Cerca"*.
- Bordo `--border-strong` che al fuoco diventa accento (`outline:none; border-color:var(--accent)`)
  — questo è l'unico controllo del pannello che **non** usa l'anello di fuoco standard.

**La logica di filtro** (`photoMatchesBrowseFilters`): dentro una stessa dimensione i valori
scelti sono in **OR**, fra dimensioni diverse è un **AND**. Il commento nel codice dà l'esempio:
`Tipo = RAW E Persone = Marta E Luogo = Urbino`. Nel dettaglio:

- **Tipo**: passa se il tipo della foto è uno di quelli scelti.
- **Luogo/cartella**: passa se la cartella della foto è una di quelle scelte.
- **Fotocamera**: passa se il modello è uno di quelli scelti.
- **Persone**: passa se **almeno una** delle persone scelte è fra i volti **confermati** della
  foto. Se il riconoscimento volti è disattivato e c'è comunque un filtro persona attivo,
  **nessuna foto passa** (`return false` secco, commentato *"dimensione disattivata: nessuna foto
  può matchare"*).
- **Tag e Categorie**: si guardano solo i tag **confermati** della foto. Se sono attive entrambe
  le dimensioni, devono passare **entrambe** (AND fra le due, OR dentro ciascuna): almeno un tag
  scelto **e** almeno una categoria scelta.
- Se nessun filtro è attivo, la lista passa così com'è, senza nemmeno scorrerla.

**Azzeramento** — tre strade:

1. Ri-cliccare la chip attiva (toglie quel singolo valore);
2. `"Cancella tutto"` nell'intestazione del pannello (svuota tutte le dimensioni, non i testi di
   ricerca);
3. Cambiare sezione dalla **sidebar**: il gestore di `[data-nav]` chiude il pannello, azzera i
   filtri **e** azzera i testi di ricerca. Il commento spiega il perché: *"il filtro rapido è
   scoped alla vista: cambiando sezione dalla sidebar riparte pulito, così non resta un filtro
   dimenticato ad assottigliare silenziosamente un'altra vista"*.

### 4. Interazioni da mouse

- **Click sul pulsante imbuto** → apre/chiude. Il click ferma la propagazione, altrimenti il
  gestore globale lo richiuderebbe subito.
- **Click su una chip** → attiva/disattiva quel valore, il pannello resta aperto e la griglia si
  aggiorna dietro.
- **Click dentro il pannello ma non su un controllo** → non chiude (`panel.onclick` ferma la
  propagazione); anche il campo di ricerca ferma la propagazione del proprio click.
- **Click fuori dal pannello, in un punto qualsiasi del documento** → chiude
  (`document.addEventListener('click', …)`).
- **Hover su una chip**: sfondo `--chip-bg-hover`. Nessun tooltip sulle chip.
- **Hover sul pulsante imbuto**: tooltip **non presente** — a differenza di "Seleziona tutto", il
  pulsante del filtro **non ha `data-tip`**, ha solo `aria-label="Filtra"`.
- **Scroll**: il corpo del pannello scorre da solo se supera `min(50vh,420px)`; le sezioni con
  ricerca hanno un secondo scroll interno da 132px.
- **Tasto destro, doppio click, trascinamento**: **non previsti**.

### 5. Interazioni da tastiera

- Il pulsante imbuto è `tabindex="0"`: Invio/Spazio lo aprono e lo chiudono (SP-8).
- Ogni chip è `tabindex="0"` con `role="button"`: Invio/Spazio la attivano.
- **Esc chiude il pannello** ovunque sia il fuoco (gestore globale su `keydown`, valutato prima
  dei gestori di culling e dopo quelli di lightbox/regioni/picklist).
- Tab entra nel campo di ricerca e poi nelle chip, in ordine di DOM.
- **Nessuna navigazione con le frecce** fra le chip, nessun `role="group"`/`listbox` sulle righe
  di chip, nessun fuoco automatico sul primo elemento all'apertura del pannello, e **il fuoco non
  viene riportato sul pulsante imbuto alla chiusura** — a differenza dei dialog modali (SP-5).
  Il pannello **non** intrappola il fuoco.

### 6. Animazioni e transizioni

- **Il pannello non ha animazione di apertura**: appare e scompare di colpo (viene aggiunto o
  tolto dal DOM a ogni render). Nessun fade, nessuno slide, nemmeno nel bottom sheet mobile.
- Le chip ereditano la transizione globale di colore (`background-color/border-color/color .2s
  ease` da `#app *`); non hanno una transizione propria.
- Il link "Cancella tutto" cambia con `text-decoration` all'hover, senza transizione.

### 7. Stati per ogni controllo

- **Pulsante imbuto**: normale (colore terziario) / hover (sfondo `--chip-bg`, colore pieno) /
  focus (anello 2.5px accento) / **attivo** (`.active`: sfondo `--accent-tint`, colore accento) /
  aperto (`aria-expanded="true"`). Non ha stato disabilitato; su lista di partenza vuota non viene
  proprio disegnato (tutto `gridQuickActionsHTML` esce vuoto).
- **Chip**: normale (sfondo `--chip-bg`, colore secondario, bordo trasparente) / hover
  (`--chip-bg-hover`) / focus (anello accento) / **attiva** (`.active`: sfondo `--accent-tint`,
  colore e bordo accento, peso 600). Esiste anche `.chip.disabled` (`opacity:.5; cursor:default`)
  nel CSS, ma **il filtro rapido non la usa mai**: nessuna chip viene disabilitata perché
  porterebbe a zero risultati.
- **Campo di ricerca**: vuoto (mostra tutte le opzioni) / con testo / a fuoco (bordo accento) /
  senza risultati (messaggio dedicato al posto delle chip).
- **Sezione**: presente / **assente** se la sua dimensione non ha opzioni (è il caso di "Persone"
  con riconoscimento volti spento).
- **"Cancella tutto"**: presente solo con almeno un filtro attivo — non esiste una versione
  disabilitata.
- **Piede**: mostra `0 foto con questi filtri` quando la combinazione non produce risultati; è
  l'unico avviso prima di trovare la griglia vuota.

### 8. Da dove ci si arriva e dove si va

Il pannello vive dentro la barra strumenti delle quattro viste a griglia (Foto, Preferiti, Album,
dettaglio Persona) tramite il wrapper condiviso `gridQuickActionsHTML`/`wireGridQuickActions`.
Non porta ad altre schermate: modifica solo ciò che si vede sotto. L'unico rimando esterno è
testuale (l'avviso che punta a Impostazioni per riattivare il riconoscimento volti — **non è un
link cliccabile**).

**Comportamento mobile**: il pannello diventa un **bottom sheet** — `position:fixed`, ancorato in
basso a tutta larghezza, raggio 14px solo sugli angoli superiori — con dietro uno scrim
`rgba(0,0,0,.4)` a `z-index:7`. Il commento spiega che lo scrim è stato aggiunto perché *"il
pannello sembrava un riquadro appoggiato sopra la griglia invece di un vero foglio modale"*; il
tap fuori chiudeva già grazie al listener globale, lo scrim rende ovvio quel comportamento. Su
desktop lo scrim è `display:none`. Sempre su mobile, la barra strumenti che contiene il trigger è
`sticky` in cima alla vista.

### 9. Dati necessari a questa schermata

Per popolare le sei dimensioni servono: i tipi di file possibili; l'elenco delle persone visibili
(non nascoste e con almeno una foto) con il loro nome; l'elenco dei tag con nome e colore;
l'elenco delle categorie di tag; i modelli di fotocamera distinti presenti; l'elenco delle
cartelle. Per applicare il filtro servono, per ogni foto: tipo di file, cartella, fotocamera,
volti **confermati** con la persona associata, tag **confermati** con la loro categoria. Serve
inoltre lo stato dell'interruttore "riconoscimento volti".

**Scrive**: solo stato di sessione — i valori scelti per dimensione, il testo digitato per
dimensione, e se il pannello è aperto. Nulla viene salvato sulle foto e nulla viene persistito fra
un avvio e l'altro.

---

## 12. Selezione multipla e barra azioni — definizione canonica di **SP-2**

### 1. Nome e scopo

Selezionare più foto in una vista a griglia e agire su tutte insieme, tramite una barra che
sostituisce la barra strumenti e riporta quante foto sono selezionate. Il commento nel codice
dichiara l'origine: *"selezione di più foto in Timeline/Preferiti/Cerca/Album, con una barra
azioni (preferiti, aggiungi ad album, elimina) e una pagina dedicata di modifica in blocco, sul
modello bulk edit di WordPress"*.

### 2. Cosa mostra

La barra (`.selection-bar`, `role="toolbar"`, `aria-label="Azioni sulla selezione"`) è un blocco
con sfondo `--chip-bg`, raggio 10px, padding 8px/12px, che va a capo se serve
(`flex-wrap:wrap`). Occupa lo stesso posto della riga strumenti normale, dentro `.grid-toolbar`.

**A sinistra** (gap 12px):

- il pulsante **×** di annullamento (`.mobile-back`, 28×28, raggio 8px, icona `close` 16px,
  `aria-label="Annulla selezione"`);
- il **conteggio in grassetto**: `"N selezionata"` al singolare, `"N selezionate"` al plurale
  (13.5px);
- il link **`"Seleziona tutte"`** (colore accento, peso 600, 12.5px).

**A destra**, cinque pulsanti **solo icona**, quadrati 32×32 senza testo. Il commento CSS spiega
la scelta: *"il significato lo porta il tooltip (desktop) + aria-label (sempre)"* — nel culling lo
stesso commento aggiunge che pulsanti con testo esteso, in questa barra compatta, si
sovrapporrebbero.

| # | Icona | `aria-label` | `data-tip` (tooltip, SP-7) | Variante |
|---|---|---|---|---|
| 1 | cuore 15px | `"Aggiungi o rimuovi dai preferiti"` | `"Preferiti"` | `.btn-ghost` |
| 2 | album 15px | `"Aggiungi ad album"` | `"Album"` | `.btn-ghost` |
| 3 | condividi 15px | `"Condividi selezione"` | `"Condividi"` | `.btn-ghost` |
| 4 | matita 14px | `"Modifica in blocco"` | `"Modifica"` | `.btn` (contornato) |
| 5 | cestino 15px | `"Elimina selezione"` | `"Elimina"` | `.btn-danger` (bordo e testo rossi) |

Fuori dalla barra, la selezione si vede anche sulle tile: contorno accento sulle selezionate e
**tutti** i cerchietti resi visibili (`#app.selection-active`).

**Annuncio per screen reader**: c'è una regione `aria-live="polite" aria-atomic="true"` fuori
schermo (`#selectionLiveRegion`) che a ogni cambio dice `"N foto selezionata/e"` oppure
`"Selezione annullata"`.

### 3. Ogni controllo, uno per uno

| Controllo | Tipo | Cosa fa esattamente |
|---|---|---|
| **×** "Annulla selezione" | pulsante icona | `clearSelection()`: svuota la selezione, esce dalla modalità, ridisegna, annuncia "Selezione annullata". |
| **"Seleziona tutte"** | link/interruttore | **è un toggle**: se *tutte* le foto della lista visibile sono già selezionate, le **deseleziona**; altrimenti le **aggiunge tutte** alla selezione (senza togliere eventuali foto già selezionate altrove). |
| **Preferiti** | pulsante icona | **toggle di gruppo**: se *tutte* le selezionate sono già preferite, le toglie tutte e mostra il toast `"Rimossi dai preferiti."`; altrimenti le mette tutte fra i preferiti con il toast `"Aggiunti ai preferiti."` (SP-6). No-op su selezione vuota. |
| **Album** | pulsante icona | apre il dialog "Aggiungi ad album" (sotto). |
| **Condividi** | pulsante icona | apre il dialog "Condividi N elementi" (sotto). |
| **Modifica** | pulsante icona | azzera la bozza di modifica (`{rating:0, pick:'', fav:'', folder:'', title:''}`) e passa alla vista "Modifica multipla". |
| **Elimina** | pulsante icona | apre il dialog di eliminazione a 3 opzioni (sotto). |

**Come si entra e come si esce.** Il commento è esplicito: *"entrare/uscire dalla modalità
selezione è implicito: si entra selezionando la prima foto (hover+click sul checkbox, tap
prolungato, o Invio/Spazio da tastiera), si esce deselezionando l'ultima o premendo Annulla nella
barra"*. Non c'è un pulsante "Seleziona" da premere prima. Il quarto ingresso è il pulsante
SP-4 "Seleziona tutto quello che vedi" nella barra strumenti, che entra in selezione con tutto il
visibile già spuntato.

**Dialog "Aggiungi ad album"** (`openAlbumPickerDialog`, SP-5):
titolo `"Aggiungi ad album"`; sottotitolo `"N elementi selezionati — attiva/disattiva un album per
aggiungere o rimuovere tutti gli elementi"`; una riga per ogni album **manuale** con copertina a
gradiente 36×36, nome, e un interruttore `.mini-switch` (36×20, pomello 16px, `transition:left
.15s ease`). Ogni riga è `role="switch"` con `aria-checked`. La riga è acceso/spento **di gruppo**:
se tutte le foto selezionate sono già nell'album, il click le toglie tutte; altrimenti le aggiunge
tutte, e il conteggio dell'album viene aggiornato. Gli album **dinamici** non compaiono, con la
nota: `"N album dinamici non mostrati qui: la loro appartenenza è calcolata automaticamente dal
filtro, non modificabile a mano."` Chiusura con `"Fatto"` o Esc; il fuoco va alla prima riga
all'apertura e torna al pulsante che l'ha aperto alla chiusura. La lista scrolla oltre 260px.
**L'effetto è immediato**: non c'è "Annulla", ogni click è già applicato.

**Dialog "Condividi"** (`openShareSelectionDialog`, SP-5):
titolo `"Condividi N elementi"`; sottotitolo `"Concedi accesso a persone già invitate, oppure crea
un link pubblico solo per questa selezione."`; sezione **"Persone"** con una riga per ogni persona
già invitata — avatar con iniziali (SP-16), nome, ruolo sotto in piccolo, e un interruttore.
Attivando/disattivando compare il toast `"Condiviso con <Nome>."` / `"Accesso rimosso a
<Nome>."`; sezione **"Link pubblico"** con la nota `"Sola visualizzazione, senza condividere
l'intera cartella o album di provenienza."` e il pulsante `"Crea link di condivisione"`, che
aggiunge in cima all'elenco delle condivisioni una voce intitolata `"N foto selezionate"` con
sottotitolo `"Selezione manuale · nessuna scadenza · download originale off · N elementi"`, mostra
il toast `"Link creato e copiato negli appunti."` e chiude il dialog. Pulsante `"Fatto"`, Esc,
fuoco alla prima riga (o al pulsante del link se non ci sono persone) e ritorno del fuoco al
trigger.

**Dialog di eliminazione** (`openBulkDeleteDialog` → `openDeleteDialogGeneric`, SP-5):
titolo `"Eliminare N foto?"`; sottotitolo `"Keeppix chiede sempre come procedere — non c'è un
comportamento predefinito implicito."`; tre opzioni:
1. `"Rimuovi solo dall'indice"` — *"Il file resta sul disco, verrà re-indicizzato alla prossima
   scansione della cartella."*
2. `"Sposta nel cestino di Keeppix"` — *"Spostato in .keeppix-trash nella stessa libreria.
   Recuperabile per 30 giorni."*
3. `"Elimina dal disco adesso"` (variante `danger`) — *"Azione irreversibile: il file viene
   cancellato definitivamente."*
Più `"Annulla"`. Alla scelta: ogni foto selezionata riceve `pick='reject'` e la scelta di
smaltimento, toast `"N foto eliminate."`, e la selezione viene azzerata.

### 4. Interazioni da mouse

- **Click sul cerchietto di una tile** → entra in selezione con quella foto / la toglie. Togliendo
  l'ultima si esce automaticamente dalla modalità.
- **Click sul corpo di una tile mentre la selezione è attiva** → seleziona/deseleziona invece di
  aprire il lightbox.
- **Tap prolungato 500 ms (solo mobile)** → entra in selezione con vibrazione.
- **Hover sui cinque pulsanti** → tooltip SP-7 sopra il pulsante.
- **Shift+click per un intervallo**: **non implementato** nelle griglie della libreria. Esiste
  solo nel filmino del culling, che usa un pool di selezione separato e commentato come tale.
- **Ctrl/Cmd+click, selezione a rettangolo, trascinamento della selezione**: **non previsti**.

### 5. Interazioni da tastiera

- Tutti i controlli della barra sono `tabindex="0"` con `role="button"`: **Invio e Spazio**
  attivano (SP-8).
- Si entra in selezione da tastiera mettendo il fuoco sul cerchietto di una tile (che diventa
  visibile proprio al fuoco) e premendo Invio o Spazio.
- **Nessuna scorciatoia globale**: niente Ctrl/Cmd+A per selezionare tutto, niente Canc per
  eliminare, niente Esc per annullare la selezione (Esc è intercettato solo per lightbox, pannello
  filtri, ricerca regioni e picklist).
- **Nessuna gestione del fuoco al passaggio di modalità**: quando la barra sostituisce la riga
  strumenti l'intera vista viene ridisegnata e il fuoco si perde (torna al `<body>`). L'annuncio
  vocale serve proprio a compensare.

### 6. Animazioni e transizioni

- **La barra non ha animazione di entrata/uscita**: sostituisce la riga strumenti al render
  successivo, di colpo.
- Contorno di selezione sulla tile: istantaneo (nessuna transizione su `outline`).
- Cerchietti che compaiono su tutte le tile entrando in modalità: `opacity .12s ease`.
- Interruttori dei dialog: `transition:left .15s ease` sul pomello — *comunica* il passaggio
  acceso/spento.
- Toast (SP-6): `opacity .2s ease, transform .2s ease`, visibile 2400 ms e poi rimosso dopo altri
  250 ms.

### 7. Stati per ogni controllo

- **Barra**: presente solo quando `state.selectionMode` è vero. Non ha uno stato "0 selezionate":
  a zero la modalità si spegne da sola.
- **×, "Seleziona tutte"**: normale / hover (`--chip-bg` sul primo, colore accento fisso sul
  secondo) / focus. Nessuno stato disabilitato.
- **Preferiti / Album / Condividi / Modifica / Elimina**: normale / hover (Preferiti-Album-
  Condividi: sfondo `--chip-bg`; Modifica: sfondo `--chip-bg`; Elimina: sfondo `--danger-tint`) /
  focus. **Nessuno è mai visivamente disabilitato**: la protezione è nel codice (ritorno anticipato
  su selezione vuota), non nell'aspetto — ma la barra non esiste con selezione vuota, quindi il
  caso non si vede.
- **"Seleziona tutte"** non cambia etichetta quando è già tutto selezionato: resta `"Seleziona
  tutte"` anche se in quel momento **deseleziona**.
- **Righe dei dialog**: normale / hover (`--chip-bg`) / focus (anello accento) / acceso
  (`.mini-switch.on`, sfondo accento, pomello a destra).

### 8. Da dove ci si arriva e dove si va

**In ingresso**: da qualunque griglia che chiami `attachSelectionBar` — Foto (timeline), Preferiti,
risultati di **Cerca**, dettaglio **Album**, dettaglio **Persona**. La modalità e l'insieme delle
foto selezionate sono **globali** (`state.selectionMode`, `state.selectedIds`) e condivisi da tutte
queste viste.
**In uscita**: "Modifica" → vista "Modifica multipla"; gli altri tre pulsanti aprono dialog modali
e riportano alla stessa griglia; × o l'ultima deselezione riportano alla barra strumenti normale.

### 9. Dati necessari a questa schermata

**Legge**: l'insieme degli identificativi selezionati; per ogni foto selezionata il flag preferito
(per decidere se il pulsante cuore aggiunge o rimuove) e l'appartenenza agli album (per gli
interruttori del picker); l'elenco degli album **manuali** con nome, copertina e membri, più il
numero di album dinamici; l'elenco delle persone già invitate con nome, ruolo e colore avatar.

**Scrive**: il flag preferito sulle foto selezionate; l'appartenenza agli album (aggiunta/rimozione
di massa) e il conteggio dell'album; una nuova voce nell'elenco dei link di condivisione; su
eliminazione, il marcatore di scarto e la modalità di smaltimento scelta (solo indice / cestino /
disco) su ogni foto selezionata.

---

## 13. Modifica in blocco

### 1. Nome e scopo

Pagina a sé stante (`state.view === 'bulkEdit'`) che applica gli stessi campi a tutte le foto
selezionate in una volta sola, dichiaratamente sul modello del *bulk edit* di WordPress.

### 2. Cosa mostra

- Link di ritorno in alto: chevron sinistro 15px + `"Annulla"` (`.back-link`, 13px, colore
  secondario, hover → colore pieno).
- Titolo `"Modifica multipla"`.
- Sottotitolo: `"N foto selezionate — ogni campo si applica a tutte, lasciane uno "invariato" per
  non toccarlo"`.
- **Striscia di anteprima** (`.bulk-strip`): fino a **30** miniature quadrate 52×52 (raggio 6px,
  gradiente della foto), in una riga a scorrimento orizzontale; se le selezionate sono di più, in
  coda un riquadro `"+N"` (sfondo `--chip-bg`, 11px peso 700). Le miniature **non sono
  interattive**: non si può togliere una foto dalla selezione da qui.
- **Sette sezioni di campi** (`.settings-section`), nell'ordine: Valutazione, Pick / Scarta,
  Preferiti, Album, Tag, Titolo, Rinomina file, Sposta in cartella (otto blocchi in tutto).
- **Piede**: due pulsanti affiancati, `"Applica a N foto"` (primario, icona spunta 14px) e
  `"Annulla"` (ghost).

**Stato vuoto**: se si arriva qui senza selezione, tutto il resto sparisce e resta solo l'empty
state con icona spunta, titolo `"Nessuna foto selezionata"` e sottotitolo `"Torna alla Timeline,
seleziona una o più foto, poi premi "Modifica"."`

### 3. Ogni controllo, uno per uno

| # | Sezione / etichetta | Testo di aiuto | Controllo | Comportamento |
|---|---|---|---|---|
| 1 | `"Valutazione"` | `"Imposta lo stesso rating per tutte — 0 stelle = non modificare"` | 5 stelle da 20px (SP-9) dentro un `role="radiogroup"` | ogni stella è `role="radio"`, `tabindex="0"`, `aria-label="N stelle"`. Cliccare la stella già impostata riporta a 0 = "non modificare". Le stelle 1..N si colorano tutte (riempimento progressivo) ma `aria-checked` è vero **solo** sul valore esatto — il commento spiega che il riempimento progressivo è *"solo visuale"* mentre per la semantica di un radiogroup deve valere il valore scelto. |
| 2 | `"Pick / Scarta"` | `"Segna soltanto — per eliminare davvero le foto usa "Elimina" nella barra di selezione"` | segmentato a 4 opzioni | `"Non modificare"` (attivo di partenza) · `"Pick"` · `"Scarta"` · `"Nessuno"`. `"Nessuno"` **azzera** il marcatore sulle foto (è un'azione, non un "non toccare"). |
| 3 | `"Preferiti"` | — (nessun testo di aiuto) | segmentato a 3 opzioni | `"Non modificare"` (attivo) · `"Aggiungi"` · `"Rimuovi"`. |
| 4 | `"Album"` | `"Aggiungi o rimuovi tutte le foto selezionate da uno o più album"` | pulsante `"Scegli album…"` (icona album 13px) | apre lo **stesso** dialog di SP-2. **Attenzione: agisce subito**, non aspetta "Applica". |
| 5 | `"Tag"` | `"Aggiungi o rimuovi tag da tutte le foto selezionate — un'aggiunta manuale è già una conferma, non passa dalla coda di revisione"` | pulsante `"Aggiungi tag…"` (icona tag 13px) | apre il dialog di scelta tag. **Agisce subito.** Il testo di aiuto è la regola di provenienza SP-12: un tag messo a mano è già confermato. |
| 6 | `"Titolo"` | `"Imposta lo stesso titolo per tutte — lascia vuoto per non modificarlo"` | campo di testo, larghezza max 320px | **placeholder `"Non modificare"`**. Nessuna validazione, nessuna lunghezza massima. Vuoto (o solo spazi) = campo ignorato; altrimenti il titolo viene scritto ripulito dagli spazi ai bordi. |
| 7 | `"Rinomina file"` | `"Rinomina tutte le foto selezionate con una formula — data, fotocamera, titolo, numero progressivo…"` | pulsante `"Rinomina con formula…"` (icona matita 13px) | apre il dialog di rinomina sulla selezione. **Agisce per conto proprio**, non tramite "Applica". |
| 8 | `"Sposta in cartella"` | — | menu a tendina, larghezza max 260px, con etichetta solo per screen reader `"Sposta le foto selezionate in questa cartella"` | voci: `"Non modificare"` (valore vuoto, predefinita) e poi **una voce per ogni cartella della libreria** (nel mockup: "Urbino", "Lago di Braies", "Chioggia e Venezia"). |
| 9 | `"Applica a N foto"` | — | pulsante primario | applica **in un colpo solo** i campi 1, 2, 3, 6 e 8 (vedi sotto), mostra il toast `"Modifiche applicate a N foto."`, azzera la selezione e torna alla timeline. **Nessuna conferma, nessun annulla.** |
| 10 | `"Annulla"` (in fondo) e `"Annulla"` (in alto) | — | pulsante / link | tornano alla timeline **senza** applicare i campi 1-2-3-6-8 e **senza** azzerare la selezione (la barra "N selezionate" si ritrova intatta). Ciò che è stato fatto da Album/Tag/Rinomina resta comunque fatto. |

**Cosa fa esattamente "Applica"**, campo per campo:

- valutazione: scritta solo se maggiore di zero;
- pick/scarta: scritto solo se è stata scelta una delle tre opzioni diverse da "Non modificare"
  (quindi anche `"Nessuno"`, che scrive lo stato "nessuno");
- preferiti: `"Aggiungi"` mette il flag a vero su tutte, `"Rimuovi"` a falso su tutte;
- cartella: scritta solo se diversa da "Non modificare";
- titolo: scritto solo se non vuoto dopo la ripulitura degli spazi.

### 4. Interazioni da mouse

- Click sulle stelle, sulle opzioni segmentate, sui tre pulsanti che aprono dialog, e sui due
  pulsanti finali.
- Il menu a tendina è un `<select>` nativo: si apre come sempre, e la scelta viene registrata al
  `change`.
- La striscia di anteprima si **scorre in orizzontale** con la rotellina/trackpad
  (`overflow-x:auto`), ma le miniature non rispondono al click.
- **Hover**: nessun tooltip in questa pagina (nessun `data-tip`); solo i cambi di sfondo standard
  dei pulsanti.
- **Doppio click, tasto destro, trascinamento**: **non previsti**.

### 5. Interazioni da tastiera

- Tab attraversa in ordine: link "Annulla" in alto → le 5 stelle → le 4 opzioni Pick/Scarta → le
  3 opzioni Preferiti → "Scegli album…" → "Aggiungi tag…" → campo Titolo → "Rinomina con
  formula…" → menu a tendina → "Applica" → "Annulla".
- **Invio e Spazio** attivano stelle, opzioni segmentate e pulsanti (SP-8).
- I due gruppi segmentati usano **roving tabindex**: solo l'opzione attiva è `tabindex="0"`, le
  altre `-1` — quindi da Tab si entra nel gruppo su una sola opzione. **Ma le frecce non sono
  implementate**: dentro un `role="radiogroup"` ci si aspetterebbe di cambiare opzione con ← →,
  e qui non funziona; l'unico modo da tastiera è entrare nell'opzione con Tab… che però passa alla
  successiva anziché muoversi nel gruppo. Le stelle, al contrario, sono **tutte** `tabindex="0"`.
- **Nessun Esc**: Esc non annulla la modifica in blocco.
- Nessuna scorciatoia per "Applica" (niente Ctrl/Cmd+Invio).

### 6. Animazioni e transizioni

- Le stelle cambiano colore direttamente via `style.color`; l'unica transizione che le tocca è
  quella globale `color .2s ease` di `#app *`.
- Le opzioni segmentate: `.seg-option.active` prende sfondo `--card-bg`, testo pieno, peso 600 e
  l'ombra standard; anche qui il cambio passa dalla transizione globale di colore/sfondo .2s ease.
  Non c'è nessun indicatore che "scivola" da un segmento all'altro.
- Il pomello degli interruttori nei dialog: `left .15s ease`.
- Toast finale: SP-6 (`opacity/transform .2s ease`, 2400 ms).
- **Nessuna animazione** di entrata della pagina, della striscia di miniature o del ritorno alla
  timeline.

### 7. Stati per ogni controllo

- **Stelle**: 0 (tutte grigie, `--text-tertiary` = "non modificare") / 1–5 (le prime N in colore
  accento) / focus (anello accento). Nessuno stato disabilitato.
- **Segmentati**: una sola opzione `.active` alla volta; hover non ha una regola dedicata
  (`.seg-option` non definisce `:hover`), focus mostra l'anello. `"Non modificare"` è sempre
  l'opzione iniziale a ogni ingresso nella pagina, perché la bozza viene azzerata dal pulsante
  "Modifica" della barra di selezione.
- **"Scegli album…" / "Aggiungi tag…" / "Rinomina con formula…"**: normale / hover (`--chip-bg`) /
  focus. Mai disabilitati.
- **Campo Titolo**: vuoto con placeholder `"Non modificare"` / con testo / a fuoco (anello
  accento). Nessuno stato di errore possibile.
- **Menu a tendina**: predefinito su `"Non modificare"`; focus con anello accento. Non riflette il
  valore attuale delle foto selezionate (non mostra "cartelle miste").
- **"Applica a N foto"**: primario, hover `filter:brightness(1.05)`. **Mai disabilitato**, nemmeno
  quando nessun campo è stato toccato: in quel caso premerlo comunque azzera la selezione, mostra
  il toast e torna indietro senza aver cambiato nulla.
- **Pagina intera**: stato pieno / stato vuoto (nessuna selezione). Nessuno stato di caricamento o
  di errore; nessuna barra di avanzamento anche con centinaia di foto selezionate.

### 8. Da dove ci si arriva e dove si va

**In ingresso**: **unico** punto d'ingresso, il pulsante "Modifica" (`aria-label="Modifica in
blocco"`) della barra di selezione SP-2 — quindi da Foto, Preferiti, Cerca, Album o dettaglio
Persona. Non c'è nessuna voce di menu che porti qui.
**In uscita**: sia "Applica" sia "Annulla" (entrambi) impostano `state.view='foto'`, cioè
**riportano sempre alla timeline Foto**, anche se si era partiti da Preferiti, da un album o dai
risultati di Cerca. Con "Applica" la selezione viene svuotata; con "Annulla" resta.
Da mobile, la tab bar considera questa vista come parte della tab `"Foto"` (SP-17).

### 9. Dati necessari a questa schermata

**Legge**: l'insieme delle foto selezionate (identificativo e colori/miniatura per la striscia,
più il conteggio); l'elenco delle cartelle con nome e identificativo; l'elenco degli album manuali
e dei tag (per i due dialog che apre).

**Scrive**, su **tutte** le foto selezionate: valutazione 0–5; marcatore pick / scarta / nessuno;
flag preferito; cartella di appartenenza; titolo. Indirettamente, tramite i dialog che apre:
appartenenza agli album, tag confermati (con provenienza "umana", SP-12), e nome dei file
(rinomina con formula).

---

# Parte III — Culling

> **Premessa — che cos'è il culling in Keeppix.** Il codice porta un commento esteso
> (`index.html`, righe 1528–1547) che spiega il modello concettuale; è la chiave di lettura di
> tutta questa sezione e va riportato per intero:
>
> ```
> CULLING A LOTTI (Gruppo C)
> Un lotto (batch) è un import in corso, non ancora "in libreria": vive dentro la
> cartella radice di culling scelta dall'utente (state.cullingRootFolder), una
> sottocartella per lotto. Dentro un lotto, "Scelta"/"Scarta" non sono solo
> un'etichetta: spostano fisicamente la foto in _presi/_scartati dentro quella
> sottocartella — apposta per poterla poi sincronizzare (es. via WebDAV) da un
> altro computer senza passare dal browser. L'interazione resta un click identico
> a un'etichetta (esattamente come le stelle): lo spostamento è un effetto
> invisibile del click, mai un'azione a parte da spiegare o confermare.
> Le foto di un lotto vivono in un pool separato da allPhotos()/FOLDERS: sono
> ancora "in lavorazione", non foto organizzate della libreria — tenerle fuori da
> Preferiti/Album/Tag/Duplicati evita che compaiano lì prima ancora di essere
> scelte. Una volta scelte, postprodotte e ricaricate, l'utente le fa rientrare in
> libreria con l'import normale: non è questo mockup a farcele passare da solo.
> Una foto valutata FUORI da un lotto (nella libreria già organizzata, es. da
> Modifica multipla) continua a funzionare come oggi: solo il flag p.pick, nessuno
> spostamento — stesso concetto di "scelta/scarto", comportamento diverso, lo
> decide solo la posizione della foto, senza alcuno switch di modalità visibile.
> ```
>
> Un secondo commento (righe 3748–3755) ribadisce la differenza rispetto alla versione
> precedente del mockup:
>
> ```
> VIEW: CULLING — a lotti (Gruppo C)
> Pagina d'ingresso: griglia dei lotti (batch), non più una coda generica legata
> alla cartella di libreria aperta al momento. Aprendo un lotto si passa allo
> "stage" (identico nello spirito alla vecchia vista a foto singola: stelle,
> filmstrip, tasti freccia), ma "Scelta"/"Scarta" ora spostano fisicamente la
> foto tra le sotto-aree del lotto (root/_presi/_scartati) invece di limitarsi a
> un flag — vedi il commento esteso più sopra, vicino ai dati CULLING_BATCHES.
> ```
>
> **Conseguenza operativa per chi legge (architetto backend / sviluppatore Vue):**
> "Scarta" dentro un lotto e "Elimina…" nella libreria organizzata sono due azioni **diverse**,
> non due nomi della stessa cosa. Vedi il paragrafo dedicato in **Culling — lotto aperto → §3**.

La vista `culling` ha **due schermate distinte nello stesso `state.view`**, discriminate da
`state.cullingBatchId` (`renderCulling`, righe 3836–3841):

| `state.cullingBatchId` | Schermata renderizzata |
|---|---|
| `null` | griglia dei lotti (`renderCullingPicker`) |
| id valido (`'lotto-dolomiti'`…) | lotto aperto (`renderCullingStage`) |
| id **non** più esistente | fallback: azzera `cullingBatchId` e mostra la griglia |

---

## 14. Culling — scelta del lotto (la griglia dei lotti)

### 1. Nome e scopo

Pagina d'ingresso della sezione **"Culling"**: elenca i lotti (le importazioni ancora da
lavorare) come schede con copertina e conteggi, e dice sotto quale cartella del disco vivono.

### 2. Cosa mostra

**Riga della cartella radice** (`.culling-root-line`, riga 3844), in cima:

- icona cartella (`folder`, 13 px);
- il testo letterale `"Cartella di culling: "`;
- in grassetto il percorso corrente, da `state.cullingRootFolder` — valore iniziale
  `"/volume1/Foto/Culling"`;
- il link `"Cambia in Impostazioni"` (`#cullingRootChangeLink`).

**Intestazione della lista:**

- titolo `"Lotti"` (`.section-title`);
- sottotitolo `"Un lotto per ogni importazione — scegli e scarta, poi sincronizza la
  sottocartella dei presi quando arrivi in studio."` (`.section-sub`).

**Griglia dei lotti** (`.culling-batch-grid`, 3 colonne fisse su desktop, 2 su mobile).
Una scheda per ogni elemento di `CULLING_BATCHES` (righe 1548–1552 — nel mockup sono **3**,
in quest'ordine):

| id | nome mostrato | data mostrata | foto totali |
|---|---|---|---|
| `lotto-dolomiti` | `Dolomiti` | `14 ago 2026` | 184 |
| `lotto-toscana` | `Toscana — Val d'Orcia` | `2 lug 2026` | 96 |
| `lotto-liguria` | `Cinque Terre` | `30 mag 2026` | 142 |

Ogni scheda contiene, dall'alto:

1. **copertina** (`.culling-batch-cover`, alta 110 px): il gradiente della **prima foto** del
   lotto (`cullingPhotosFor(b.id)[0]`, via `tileStyle`). Non è una miniatura scelta
   dall'utente e non c'è modo di cambiarla: **non previsto nel mockup**. Se il lotto è vuoto
   il riquadro resta senza sfondo;
2. **nome del lotto** (`b.name`);
3. **sottotitolo**: `"<data> · <N> foto"` — es. `"14 ago 2026 · 184 foto"`. Il numero è
   `counts.total`, cioè le foto **attualmente** nel lotto (cala se si usa "Svuota scartati");
4. **riga dei tre conteggi** (`.culling-batch-counts`), calcolati da `cullingBatchCounts()`:
   - `"○ <N> da vedere"` — il carattere `○` è testo letterale, non un'icona;
   - icona `check` (11 px) + numero dei **presi**, senza parola accanto;
   - icona `close` (11 px) + numero degli **scartati**, senza parola accanto.

Non mostra: fotografo, dimensione su disco, percorso della sottocartella del singolo lotto,
stato di sincronizzazione, data dell'ultima modifica. **Non previsti nel mockup.**

### 3. Ogni controllo, uno per uno

| # | Elemento | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Cambia in Impostazioni"` | link testuale (`span.culling-root-change`) | `state.view='impostazioni'` + `renderAll()`. Porta alla pagina Impostazioni **intera**, non apre il dialog della cartella radice e non fa scroll fino alla sezione giusta |
| 2 | Scheda lotto (una per lotto, `[data-openbatch]`) | card cliccabile | Apre il lotto: `cullingBatchId=<id>`, `cullingIdx=0`, `cullingFilter='all'`, `cullingClearSelection()`, `renderAll()` |

Sono **tutti** i controlli della schermata: due. Non ci sono qui pulsanti "Nuovo lotto",
"Elimina lotto", "Rinomina lotto", "Importa", né ordinamento o filtro della griglia:
**non previsti nel mockup** (la rinomina del lotto esiste solo dentro il lotto aperto).

### 4. Interazioni da mouse

- **Click sulla scheda** (in qualunque punto: copertina, nome, conteggi): apre il lotto.
- **Click su "Cambia in Impostazioni"**: va in Impostazioni.
- **Hover sulla scheda**: `.culling-batch-card:hover` → il bordo passa da `var(--border)` a
  `var(--border-strong)`. Nessun ingrandimento, nessuna ombra, nessuna anteprima.
- **Hover sul link**: nessuna regola dedicata; il link è già sottolineato e in colore accento
  in stato normale.
- **Doppio click**: nessun comportamento distinto (il primo click ha già aperto il lotto).
- **Tasto destro**: nessun menu contestuale. **Non previsto nel mockup.**
- **Trascinamento** (riordinare i lotti, trascinare foto tra lotti): **non previsto nel mockup.**
- **Rotellina**: solo lo scroll normale della pagina.
- **Tooltip**: nessun `data-tip` su questa schermata.

### 5. Interazioni da tastiera

- Nessuna scorciatoia globale: il ramo `state.view==='culling'` del gestore tastiera (riga
  6315) è condizionato anche a `state.cullingBatchId`, quindi **sulla griglia dei lotti nessun
  tasto fa nulla**. Frecce, `P`, `X`, `1–5`, `Delete` sono inerti qui.
- Le schede chiamano `bindActivatable` (SP-8: Invio e Spazio = click) **ma non hanno né
  `tabindex` né `role="button"`**: di fatto non sono raggiungibili con Tab, quindi
  quell'attivazione da tastiera non si può innescare. Stesso problema per il link
  `"Cambia in Impostazioni"` (solo `onclick`). → vedi Ambiguità.
- `Esc`: nessun effetto in questa schermata.

### 6. Animazioni e transizioni

- Regola globale `#app *{transition:background-color .2s ease,border-color .2s ease,color .2s ease;}`:
  è ciò che fa **sfumare in `.2s ease` il bordo della scheda** al passaggio del mouse — segnala
  che la scheda è un bersaglio cliccabile, senza spostare nulla nel layout.
- Nessuna animazione di comparsa della griglia, nessuno stagger, nessun crossfade sulla
  copertina: la griglia viene riscritta con `innerHTML` a ogni `renderAll()`.

### 7. Stati per ogni controllo

| Controllo | Normale | Hover | Focus | Attivo/premuto | Disabilitato | Caricamento | Errore | Vuoto |
|---|---|---|---|---|---|---|---|---|
| Scheda lotto | bordo `--border`, sfondo `--card-bg`, `cursor:pointer` | bordo `--border-strong` | **nessuno stato di focus raggiungibile** (non focusabile) | nessuno stato "premuto" | mai disabilitata | nessun placeholder/skeleton: i dati sono sincroni | non gestito | copertina senza sfondo se il lotto non ha foto |
| `"Cambia in Impostazioni"` | accento, sottolineato | invariato | non focusabile | — | mai | — | — | — |
| Griglia | 3 schede | — | — | — | — | — | — | `CULLING_BATCHES` è una costante non vuota: **lo stato "nessun lotto" non è mai rappresentato** nel mockup |

### 8. Da dove ci si arriva e dove si va

**In ingresso:**

- desktop: voce di menu laterale `"Culling"` (icona `funnel`) con **badge rosso**
  (`.nav-badge`) che riporta `cullingQueueCount()`, cioè la somma delle foto `cullState==='root'`
  **di tutti i lotti** (commento alla riga 2186: *"conta le foto «da valutare» in tutti i lotti
  di culling — non più solo nella cartella di libreria attualmente aperta"*). Il badge è
  sempre presente, anche quando il conteggio è `0`;
- mobile: icona imbuto nell'header, **visibile solo dalla vista "Foto"** (`#mobileCullingBtn`),
  con lo stesso badge — che lì compare solo se `> 0`;
- ritorno dallo stage: `"Tutti i lotti"`, la briciola del nome lotto sotto la foto, oppure
  cambiando lotto e tornando indietro.

**In uscita:**

- scheda lotto → **Culling — lotto aperto**;
- `"Cambia in Impostazioni"` → vista `impostazioni`;
- mobile: la freccia indietro dell'header porta a `state.view='foto'` (non alla vista "Altro").

La topbar mostra `Culling` in grassetto; il titolo mobile è `"Culling"`.

### 9. Dati necessari a questa schermata

**Legge**, per ogni lotto: id, nome, etichetta di data già formattata, numero totale di foto,
e i tre conteggi derivati (da vedere / presi / scartati); più il gradiente/miniatura della
prima foto del lotto come copertina. Legge inoltre il percorso della cartella radice di culling
(impostazione globale) e — per il badge di navigazione — il numero di foto ancora da valutare
sommato su tutti i lotti.

**Scrive:** nulla sui dati. Cambia solo lo stato di navigazione (quale lotto è aperto, indice
foto a 0, filtro riportato a "Tutte", selezione svuotata).

> Nota implementativa utile all'architetto: le foto dei lotti sono generate **al primo accesso**
> e memorizzate in `CULLING_PHOTO_CACHE` (righe 1586–1593); vengono poi mutate in place. Quindi
> le decisioni prese in un lotto sopravvivono all'uscita e al rientro nella vista, ma non a un
> ricaricamento della pagina — è un mockup senza persistenza.

---

## 15. Culling — lotto aperto (la schermata di valutazione)

### 1. Nome e scopo

La schermata di valutazione vera e propria: una foto grande alla volta, filmino delle foto del
lotto in basso, e i due verdetti **"Scelta"** / **"Scarta"** più la valutazione a stelle, pensata
per essere usata quasi interamente da tastiera.

### 2. Cosa mostra

Dall'alto verso il basso (`renderCullingStage`, righe 3911–4034; il contenitore `#viewRoot`
riceve la classe `no-pad`, riga 3065, perché questa schermata gestisce da sé i propri margini
e occupa tutta l'altezza):

**A. Barra superiore** (`.culling-top`, in colonna, `gap:8px`, `padding-top:14px`)

Prima riga (`cullingStageHeaderHTML`, righe 3869–3882) — elementi in fila con `gap:14px`:

1. `"Tutti i lotti"` preceduto da una freccia `chevronLeft` (14 px) — `.back-link`;
2. il **selettore di lotto**: nome del lotto corrente + `chevronDown` (11 px), su sfondo chip
   (`.culling-selector`);
3. i **tre contatori del lotto** (`.culling-counters`), sempre riferiti al **lotto intero**,
   non al filtro attivo:
   - icona `check` (13 px) + numero in grassetto + `" presi"`;
   - icona `close` (13 px) + numero in grassetto + `" scartati"`;
   - `"○ "` + numero in grassetto + `" da vedere"`.

Seconda riga — **o** la barra dei filtri **o** la barra di selezione, mai entrambe
(riga 3942: `state.cullingSelectedIds.size>0 ? cullingSelectionBarHTML() : filterChipsHTML`).

**Barra dei filtri** (`.culling-filterchips`) — quattro chip, etichette esatte:

| id interno | etichetta **esatta** | coda che produce |
|---|---|---|
| `all` | **`Tutte`** | tutte le foto del lotto |
| `todo` | **`Da vedere`** | `cullState==='root'` |
| `taken` | **`Presi`** | `cullState==='taken'` |
| `skipped` | **`Scartati`** | `cullState==='skipped'` |

> Attenzione: le etichette sono `Tutte / Da vedere / Presi / Scartati` — **non**
> "da valutare / scelte / scartate". La stringa `"Da valutare"` esiste altrove
> (`cullStateLabel`, riga 1603) ma serve alla briciola sotto la foto, non ai chip.

A destra della stessa riga (`.culling-filterchips-right`, spinto a destra da `margin-left:auto`):

- `"Svuota scartati (N)"` con icona `trash` (12 px) — **compare solo** se il filtro attivo è
  `Scartati` **e** ci sono scartati (`counts.skipped>0`);
- pulsante icona **"Seleziona tutto"** (icona `selectAll`, 14 px, `.tag-icon-btn`) — compare
  solo se la coda corrente non è vuota. Tooltip `data-tip="Seleziona tutto"`; `aria-label`
  esteso: `"Seleziona tutte le foto in questo lotto (o in questo filtro, se attivo)"`.

**B. Palco** (`.culling-stage`, occupa l'altezza residua)

- **freccia sinistra** `#cullPrev` — cerchio 34 px, bordo, icona `chevronLeft` 17 px;
- **la foto grande** (`.culling-image`, `max-width:520px`, `max-height:340px`, angoli 10 px):
  nel mockup è un gradiente, non un file immagine. Sopra vi stanno:
  - **badge RAW** in alto a sinistra (SP-15), solo se `p.isRaw`: testo `"RAW"` o `"RAW+JPEG"`
    a seconda di `stackType`;
  - **pulsante info** in alto a destra (`#cullInfoBtn`, cerchio 28 px su fondo nero al 45%),
    `aria-label="Dettagli foto — EXIF, posizione, rinomina"`;
  - **la riga di aiuto** in basso a destra (`.culling-keyhint`, 10.5 px, testo bianco all'85%
    su fondo nero al 35%), **alla lettera**:

    ```
    ← → naviga · shift+← → seleziona intervallo · 1-5 rating · P scegli · X scarta
    ```

    (riga 3949; separatori `·`, "1-5" con trattino semplice, "P scegli" e "X scarta" senza
    maiuscole aggiuntive). È sempre visibile, anche su mobile, anche mentre la barra di
    selezione è attiva; non si può nascondere.
- **freccia destra** `#cullNext`.

**C. Filmino** (`.culling-filmstrip`) — striscia orizzontale scorrevole (`overflow-x:auto`),
bordo sopra e sotto, `gap:6px`, `padding:10px 40px` (14 px su mobile). Contiene **una miniatura
per ogni foto della coda corrente**, non del lotto intero: con un filtro attivo il filmino si
accorcia. Ogni miniatura (`.culling-thumb`, 58×58 px, angoli 6 px, bordo trasparente 2 px):

- gradiente della foto;
- `.mini-tag` in alto a sinistra con `"RAW"`/`"RAW+JPEG"` (7 px) se RAW;
- **checkbox di selezione** in alto a destra (`.culling-thumb-check`, 16×16 px,
  `role="checkbox"`, `aria-label="Seleziona foto N"` con N a base 1).

**D. Riga inferiore** (`.culling-bottom`, `padding:14px 24px`, contenuto ai due estremi)

A sinistra (`.culling-meta`):

1. **nome file** della foto corrente, in grassetto su riga propria (es. `DSC09321.ARW`);
2. **briciola**: `<nome del lotto>` (sottolineato, cliccabile) + `chevronRight` (10 px) +
   **stato della foto**, da `cullStateLabel`: `"Da valutare"` / `"Presi"` / `"Scartati"`;
3. **valutazione a stelle** in linea (SP-9): 5 stelle da 15 px, piene fino a `p.rating`.

A destra (`.culling-actions`, `gap:8px`):

- `"Scelta"` con icona `check` (15 px) — `.btn.btn-pick`, verde `#2E9E5B`;
- `"Scarta"` con icona `close` (15 px) — `.btn.btn-danger`;
- `"Rinomina lotto…"` con icona `edit` (13 px) — `.btn.btn-ghost.btn-sm`.

**E. Stato vuoto** (`queue.length===0`, righe 3923–3935)

Si vedono solo intestazione e chip; al posto di palco/filmino/riga inferiore compare lo stato
vuoto con icona `funnel`: titolo `"Niente da mostrare con questo filtro"`, sottotitolo
`"Cambia filtro qui sopra, oppure torna a \"Tutte\"."`

**Cosa NON mostra:** numero della foto corrente sul totale ("12 di 184") — la classe
`.culling-progress` esiste nel CSS (riga 391) ma **non è più usata da nessuna parte**, è un
residuo della versione precedente della vista; nessun istogramma, nessuna lente/zoom,
nessun confronto a due foto, nessun EXIF in linea (sta nel lightbox, dietro il pulsante info).

### 3. Ogni controllo, uno per uno

| # | Etichetta esatta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `Tutti i lotti` | link con freccia (`.back-link`) | `cullingBatchId=null` + svuota la selezione → torna alla griglia dei lotti |
| 2 | `<nome lotto> ⌄` | trigger di menu (`.culling-selector`) | Apre/chiude il selettore rapido di lotto (SP-14) — vedi sezione dedicata |
| 3 | `Tutte` | chip filtro | `cullingFilter='all'`, `cullingIdx=0` |
| 4 | `Da vedere` | chip filtro | `cullingFilter='todo'`, `cullingIdx=0` |
| 5 | `Presi` | chip filtro | `cullingFilter='taken'`, `cullingIdx=0` |
| 6 | `Scartati` | chip filtro | `cullingFilter='skipped'`, `cullingIdx=0` |
| 7 | `Svuota scartati (N)` | pulsante piccolo, fantasma, rosso | Apre un dialog di conferma (SP-5); confermando **elimina davvero** le foto scartate dal lotto |
| 8 | (icona) tooltip `Seleziona tutto` | pulsante icona | Aggiunge alla selezione **tutte le foto della coda corrente** e azzera l'ancora (`cullingSelectAnchor=null`) |
| 9 | (freccia) `#cullPrev` | pulsante tondo | Foto precedente nella coda (`max(0, idx-1)`) |
| 10 | (freccia) `#cullNext` | pulsante tondo | Foto successiva nella coda (`min(len-1, idx+1)`) |
| 11 | (icona info) | pulsante tondo sulla foto | Apre il **lightbox** su questa foto con pannello info già aperto (`lbInfoOpen=true`) e modalità RAW/JPEG coerente col file |
| 12 | miniatura del filmino | riquadro cliccabile | Click: salta a quella foto. Shift+click: seleziona l'intervallo |
| 13 | checkbox sulla miniatura | `role="checkbox"` | Accende/spegne la selezione di quella foto e **sposta l'ancora** su di essa; con Shift e un'ancora già presente seleziona l'intervallo |
| 14 | briciola `<nome lotto>` | testo sottolineato | Torna alla griglia dei lotti (identico a "Tutti i lotti") |
| 15 | briciola `Da valutare`/`Presi`/`Scartati` | testo **non** cliccabile (`.crumb-current`, `cursor:default`) | Solo informativo |
| 16 | 5 stelle | valutazione in linea (SP-9) | Imposta `rating` 1–5; ricliccare la stessa stella azzera a 0 |
| 17 | `Scelta` | pulsante verde | `decideCulling('taken')` — vedi sotto |
| 18 | `Scarta` | pulsante rosso | `decideCulling('skipped')` — vedi sotto |
| 19 | `Rinomina lotto…` | pulsante fantasma piccolo | Apre "Rinomina con formula" sull'**intero lotto** |

**Barra di selezione** (`cullingSelectionBarHTML`, righe 3788–3805), che rimpiazza i chip
appena c'è almeno una foto selezionata. È SP-2 con quattro deviazioni, dichiarate nel commento
alle righe 3789–3791 (*"stessa struttura/dimensioni della barra di selezione della libreria
(icone in quadrati 32×32, etichetta al passaggio del mouse via data-tip) — non pulsanti con
testo esteso, che in questa barra compatta si sovrapporrebbero"*):

| # | Etichetta / tooltip | Tipo | Cosa fa |
|---|---|---|---|
| 20 | (×) `aria-label="Annulla selezione"` | pulsante icona | Svuota selezione e ancora |
| 21 | `N selezionate` / `1 selezionata` | testo in grassetto | Conteggio, singolare/plurale corretto |
| 22 | `Seleziona tutte` | link accento | **Interruttore**: se la coda corrente è già tutta selezionata deseleziona tutto, altrimenti seleziona tutto (commento riga 3808) |
| 23 | tooltip `Scelta` | pulsante icona 32×32 verde | Porta **tutte** le selezionate a `taken`, toast, svuota la selezione |
| 24 | tooltip `Scarta` | pulsante icona 32×32 rosso | Porta **tutte** le selezionate a `skipped`, toast, svuota la selezione |
| 25 | tooltip `Rinomina…` | pulsante icona 32×32 fantasma | Apre "Rinomina con formula" sulle sole selezionate; a fine applicazione svuota la selezione |

La barra ha `role="toolbar"` e `aria-label="Azioni sulla selezione del lotto"`.
**Deviazioni da SP-2:** qui **non** ci sono "Preferiti", "Aggiungi ad album", "Condividi" né
"Elimina…" — il commento alle righe 3765–3767 lo motiva: *"un pool a parte
(cullingSelectedIds), non lo stesso state.selectedIds usato dalla libreria organizzata: sono due
contesti diversi con azioni diverse (qui Scelta/Scarta/Rinomina, non Preferiti/Album/Condividi)"*.
Le due selezioni sono quindi **indipendenti**: selezionare foto in un lotto non tocca la
selezione della libreria e viceversa.

#### `decideCulling` — "Scelta"/"Scarta" e perché non c'è nessun dialog

Righe 4047–4055; il commento interno è il punto centrale di tutta la sezione:

```
// click semplice, identico a un'etichetta: ricliccare la stessa decisione la annulla
// (torna in 'root'), esattamente come già succede oggi con le stelle — mai un dialog.
cur.cullState = (cur.cullState===decision) ? 'root' : decision;
```

Quindi:

- `Scelta` su una foto "da valutare" → `taken`; `Scelta` su una foto già presa → torna a `root`;
- `Scarta` su una foto "da valutare" → `skipped`; `Scarta` su una già scartata → torna a `root`;
- `Scelta` su una foto **scartata** → passa direttamente a `taken` (e viceversa), senza passare
  da `root`;
- **nessuna conferma, nessun toast, nessuna animazione**: solo il ridisegno.

**Differenza semantica fra "Scarta" (lotto) e "Elimina…" (libreria organizzata).**
Sono due azioni diverse, e il mockup lo rende esplicito in tre punti:

1. `Scarta` in un lotto = **spostamento fisico immediato** della foto in `_scartati` dentro la
   sottocartella del lotto, reversibile ricliccando, **senza alcun dialog** (commento righe
   1532–1537, citato in premessa: *"«Scelta»/«Scarta» non sono solo un'etichetta: spostano
   fisicamente la foto in _presi/_scartati … L'interazione resta un click identico a
   un'etichetta (esattamente come le stelle): lo spostamento è un effetto invisibile del click,
   mai un'azione a parte da spiegare o confermare"*). La foto **resta nel lotto** e resta
   visibile col filtro `Scartati`.
2. `Elimina…` nella libreria organizzata apre invece il **dialog a 3 opzioni**
   (`openDeleteDialogGeneric`, righe 3198 e seguenti), sottotitolo `"Keeppix chiede sempre come
   procedere — non c'è un comportamento predefinito implicito."`, con:
   - `Rimuovi solo dall'indice` — *"Il file resta sul disco, verrà re-indicizzato alla prossima scansione della cartella."*
   - `Sposta nel cestino di Keeppix` — *"Spostato in .keeppix-trash nella stessa libreria. Recuperabile per 30 giorni."*
   - `Elimina dal disco adesso` — *"Azione irreversibile: il file viene cancellato definitivamente."*
   - più `Annulla`.
3. Coerentemente, **nel lightbox aperto su una foto di lotto spariscono "Aggiungi ad album" e
   "Elimina…"**; il commento alle righe 4286–4290 lo dice: *"in culling p può non avere
   folderId/album/tag reali (è una foto «grezza», non ancora organizzata) — il pannello si
   adatta: niente sezione Tag/Album, niente «Aggiungi ad album»/«Elimina…» (in culling si usa
   Scelta/Scarta, non il cestino), breadcrumb verso il lotto invece che verso la cartella"*.

L'unica azione **distruttiva** disponibile dentro un lotto è `"Svuota scartati (N)"`, e quella
sì che conferma (`wireCullEmptySkipped`, righe 4035–4046):

- titolo: `Svuotare gli scartati di "<nome lotto>"?`
- corpo: `"<N> fot{a|e} verranno eliminate definitivamente da questo lotto. Le foto già prese
  non sono toccate — è pensato apposta come passaggio prima di sincronizzare il lotto in
  studio."`
- pulsanti: `Svuota scartati` (rosso) e `Annulla`;
- confermando: le foto `skipped` vengono rimosse dal pool del lotto, `cullingIdx=0`, toast
  `"Foto scartate eliminate dal lotto."`
- **non** propone le 3 opzioni indice/cestino/disco: qui l'eliminazione è una sola.

#### Rinomina

- `"Rinomina lotto…"` chiama `openRenameDialog({kind:'folder', photos: <tutte le foto del
  lotto>, label:<nome lotto>, hasSubfolders:true})`. Nel dialog l'ambito appare come
  `Tutta la cartella "<nome lotto>" (<N> foto)` e, poiché `hasSubfolders` è vero, compare
  l'interruttore `"Includi anche presi e scartati, non solo da valutare"`, **spento** di
  partenza: con l'interruttore spento la rinomina tocca **solo** le foto `cullState==='root'`.
  Nota: l'ambito **ignora il filtro attivo a schermo**.
- Dalla barra di selezione, `Rinomina…` usa `kind:'selection'` (ambito
  `"<N> foto selezionate"`), **senza** quell'interruttore.

### 4. Interazioni da mouse

**Sulla foto grande**

- Click sulla foto: **non fa nulla** — non apre il lightbox, non sceglie, non ingrandisce.
  L'unico modo di aprire il dettaglio è il pulsante info in alto a destra. **Non previsto**
  alcun click sull'immagine nel mockup.
- Hover sul pulsante info: sfondo da `rgba(0,0,0,.45)` a `rgba(0,0,0,.65)`.
- Nessun zoom con rotellina, nessun trascinamento per spostare l'immagine: **non previsti.**
- Tasto destro: nessun menu contestuale. **Non previsto nel mockup.**

**Sulle frecce**

- Click: avanti/indietro di una posizione nella coda.
- Hover: `.culling-nav-btn:hover` → sfondo `var(--chip-bg)`, icona da `--text-secondary` a
  `--text`.

**Sul filmino** (comportamento chiave; commento alle righe 3977–3979: *"click su una miniatura:
naviga a quella foto; shift+click estende la selezione multipla dall'ancora corrente (o dalla
foto aperta, se non c'è ancora un'ancora) — stesso pattern di shift+freccia nelle scorciatoie da
tastiera più sotto"*)

- **Click su una miniatura** → `cullingIdx = i`: la foto grande cambia. **Non** modifica la
  selezione e **non** la azzera.
- **Shift+click su una miniatura** → seleziona l'intervallo `[ancora … i]`; se non c'è ancora,
  l'ancora è la foto attualmente aperta. L'ancora **resta quella di prima**, così si può
  allargare o restringere l'intervallo continuando a fare shift+click. `cullingIdx` **non**
  cambia: la foto grande resta quella di prima.
- **Click sulla checkbox della miniatura** → inverte la selezione di quella sola foto e
  **sposta l'ancora** su di essa; il click non si propaga alla miniatura (`stopPropagation`),
  quindi non cambia la foto visualizzata.
- **Shift+click sulla checkbox** → seleziona l'intervallo `[ancora … i]`, **ma solo se
  un'ancora esiste già**; altrimenti si comporta come un click semplice (inverte e imposta
  l'ancora).
- **Hover su una miniatura** → compare la sua checkbox (`opacity 0 → 1`, `transition:opacity .12s`).
- **Scroll**: il filmino è `overflow-x:auto`, quindi scorre orizzontalmente con la rotellina
  orizzontale/trackpad. **Non c'è alcuno `scrollIntoView`**: navigando con frecce o tastiera
  **il filmino non si sposta da solo** e la miniatura corrente può finire fuori vista (su un
  lotto da 184 foto succede subito). → vedi Ambiguità.
- Trascinamento di miniature (riordino, drag verso un altro lotto): **non previsto nel mockup.**

**Sui chip e sui pulsanti**: click semplice; hover standard dei pattern SP-3 (chip) e dei
`.btn` (sfondo `var(--chip-bg)`; `.btn-danger:hover` sfondo `var(--danger-tint)`;
`.btn-pick:hover` sfondo verde al 10%).

**Tooltip** (SP-7): presenti solo su `Seleziona tutto` e sui tre pulsanti icona della barra di
selezione; compaiono su hover **e** su focus da tastiera, con `opacity/transform .12s ease`, e
sono **disattivati su mobile** (`#app.device-mobile [data-tip]::after{display:none}`).

### 5. Interazioni da tastiera

Gestore globale, righe 6315–6344. Attivo **solo** se `state.view==='culling'` **e**
`state.cullingBatchId` è valorizzato, e **solo** se il lightbox non è aperto (il ramo
`state.lightbox` in cima al gestore intercetta prima e fa `return`).

| Tasto | Effetto esatto |
|---|---|
| `←` (senza Shift) | **Se c'è una selezione multipla, la azzera** (`cullingClearSelection()`, che svuota anche l'ancora); poi `cullingIdx = max(0, cullingIdx-1)` |
| `→` (senza Shift) | **Se c'è una selezione multipla, la azzera**; poi `cullingIdx = min(len-1, cullingIdx+1)` |
| `Shift+←` | Ancora = `cullingSelectAnchor` se esiste, altrimenti la foto corrente; sposta `cullingIdx` di −1; seleziona **tutto l'intervallo** `[ancora … nuovo indice]`; l'ancora resta fissa |
| `Shift+→` | Idem, di +1 |
| `P` / `p` | `decideCulling('taken')` → "Scelta" sulla foto corrente; ripremuto sulla stessa foto già presa, la riporta a "da valutare" |
| `X` / `x` | `decideCulling('skipped')` → "Scarta"; ripremuto, riporta a "da valutare" |
| `Delete` | **Identico a `X`**: scarta. Non elimina nulla, non apre dialog |
| `1` `2` `3` `4` `5` | Imposta `rating` della foto corrente. **Premere di nuovo lo stesso numero azzera la valutazione** (`cur.rating = (cur.rating===n) ? 0 : n`) — stessa regola SP-9 delle stelle cliccate |

Note importanti:

- lo **Shift sulle frecce non è un'estensione additiva**: `cullingSelectRange` svuota sempre e
  ricalcola. Il commento alle righe 3777–3781 spiega il perché: *"shift+click / shift+freccia
  selezionano sempre l'intero intervallo [ancora..indice corrente] — non si sommano a una
  selezione precedente, si sostituiscono ad essa, come il comportamento standard di un file
  manager: muovendo il secondo estremo la selezione si allarga o si restringe di conseguenza
  (serve azzerare e ricalcolare ad ogni mossa, altrimenti restringendo resterebbero
  «appiccicate» le foto già incluse in un giro precedente)"*;
- la freccia semplice **azzera** la selezione: è la via d'uscita rapida da una selezione
  multipla senza usare il mouse (non esiste `Esc` per questo, vedi sotto);
- `P`, `X`, `Delete`, `1–5` agiscono **solo sulla foto corrente**, mai sulla selezione multipla:
  per agire su più foto servono i pulsanti della barra di selezione;
- ai limiti della coda le frecce non "wrappano" e non passano al lotto successivo;
- **`Esc` non fa nulla** in questa schermata: non svuota la selezione, non chiude il lotto, non
  torna alla griglia. Nessun `Escape` è gestito per la vista culling;
- **`Ctrl`/`Cmd`+click e Ctrl/Cmd+A non sono gestiti**: nessuna selezione discontinua da
  tastiera; l'unico "seleziona tutto" è il pulsante icona o il link `Seleziona tutte`;
- **`Tab` / ordine del focus**: nel mockup solo pochi elementi di questa schermata sono
  focusabili — il pulsante info, `Seleziona tutto`, le checkbox delle miniature e i controlli
  della barra di selezione (tutti con `role` + `tabindex="0"`). **Non** sono focusabili:
  `Tutti i lotti`, il selettore di lotto, i quattro chip filtro, `Svuota scartati`, le frecce
  ← →, le miniature, la briciola, `Scelta`, `Scarta`, `Rinomina lotto…`. → vedi Ambiguità;
- `Invio`/`Spazio` (SP-8) attivano gli elementi che passano da `bindActivatable` — quindi
  funzionano su ciò che è effettivamente raggiungibile con Tab;
- lo stile di focus è quello globale: `outline:2.5px solid var(--accent); outline-offset:2px`.

### 6. Animazioni e transizioni

Il mockup è volutamente sobrio: non ci sono keyframes dedicati al culling (l'unico
`@keyframes` del file, `analysisPulse`, appartiene a un'altra vista).

| Cosa | Innesco | Durata/curva | Cosa comunica |
|---|---|---|---|
| Checkbox della miniatura che appare | hover sulla miniatura, oppure `.checked` | `opacity .12s` (lineare, nessuna curva dichiarata) | "questa miniatura si può anche selezionare, non solo aprire" — il comando resta nascosto finché non serve |
| Bordo accento della miniatura **corrente** | cambio di `cullingIdx` | `border-color .2s ease` (regola globale `#app *`) | Sposta lo sguardo sulla nuova posizione nel filmino senza uno scatto secco |
| Anello di selezione della miniatura (`box-shadow:0 0 0 2px var(--accent)`) | selezione | **istantaneo**: `box-shadow` non è nella transizione globale | Asimmetria rispetto al punto sopra — vedi Ambiguità |
| `Scelta` che diventa pieno verde (`.btn-pick.chosen`) | `cullState` della foto corrente = `taken` | `background-color .2s ease` + `color .2s ease` (globale) | **È il vero riscontro visivo della decisione**: nessun toast, nessun dialog, il pulsante stesso diventa lo stato della foto |
| Bordi/colori di chip e pulsanti su hover | hover | `.2s ease` (globale) | affordance di clic |
| Tooltip `[data-tip]` | hover o focus | `opacity .12s ease, transform .12s ease` (sale di 3 px) | nome dell'azione dei pulsanti icona |
| Toast | azioni di massa e "Svuota scartati" | `opacity .2s ease, transform .2s ease`, visibile ~2,4 s poi rimosso | conferma di un'azione che ha toccato più foto |

**La miniatura quando la foto cambia stato.** Va detto chiaramente, perché è un'assenza
significativa: **nel mockup la miniatura non ha alcuna animazione di cambio stato, e non ha
nemmeno un contrassegno di stato.** Nel filmino una foto "presa" e una "scartata" sono
graficamente identiche a una ancora da valutare: gli unici modificatori della miniatura sono
`current` (bordo accento) e `selected` (anello accento). Ciò che si vede accadere è:

- con filtro `Tutte`: **niente**, la striscia resta identica; l'unico segnale è il pulsante
  `Scelta` che si accende e la briciola sotto la foto che passa a `Presi`/`Scartati`;
- con un filtro attivo (`Da vedere`, `Presi`, `Scartati`): la foto **esce dalla coda** e la sua
  miniatura **sparisce di colpo** dalla striscia, senza animazione di uscita; poiché
  `cullingIdx` non viene toccato, la foto successiva scivola in quella posizione e diventa
  automaticamente la foto grande. Questo è il flusso di lavoro naturale con filtro
  `Da vedere`: decidi, e la successiva arriva da sola. Se la foto decisa era l'ultima, il
  clamp `if(state.cullingIdx>=queue.length) state.cullingIdx = queue.length-1` (riga 3936)
  riporta l'indice sull'ultima disponibile; se la coda si svuota del tutto compare lo stato
  vuoto `"Niente da mostrare con questo filtro"`.

### 7. Stati per ogni controllo

| Controllo | Normale | Hover | Focus | Attivo / selezionato | Disabilitato (e perché) | Vuoto / assente |
|---|---|---|---|---|---|---|
| `Tutti i lotti` | testo `--text-secondary` | testo `--text` | non focusabile | — | mai | — |
| Selettore di lotto | chip grigio | nessuna regola dedicata | non focusabile | quando il pannello è aperto **l'aspetto non cambia** (nessuna classe "aperto") | mai | — |
| Chip filtro | sfondo `--chip-bg`, testo secondario | `--chip-bg-hover` | stile focus globale definito ma **irraggiungibile** (nessun tabindex) | `.active`: sfondo `--accent-tint`, testo e bordo accento, `font-weight:600` | la classe `.chip.disabled` (`opacity:.5`) esiste ma **non viene mai applicata qui**: i chip restano cliccabili anche quando la coda risultante sarà vuota | — |
| `Svuota scartati (N)` | rosso, bordo `--danger` | sfondo `--danger-tint` | non focusabile | — | **non è disabilitato: è proprio assente** finché filtro ≠ `Scartati` o `skipped===0` | — |
| `Seleziona tutto` (icona) | 26×26, colore terziario | sfondo `--chip-bg`, colore `--text` | outline accento (è `role=button tabindex=0`) | — | **assente** se la coda è vuota | — |
| Freccia `←` | cerchio con bordo | sfondo chip | non focusabile | — | **`opacity:.35; pointer-events:none` in stile inline quando `cullingIdx===0`** — sei sulla prima foto della coda | — |
| Freccia `→` | idem | idem | non focusabile | — | stesso trattamento quando `cullingIdx === queue.length-1` — ultima foto della coda | — |
| Pulsante info | fondo nero 45% | fondo nero 65% | outline accento | — | mai | — |
| Miniatura | bordo trasparente | mostra la checkbox | non focusabile | `.current` bordo accento; `.selected` anello accento; possono coesistere | mai | — |
| Checkbox miniatura | invisibile (`opacity:0`), fondo nero 40%, bordo bianco | visibile | outline accento + diventa visibile | `.checked`: sempre visibile, sfondo e bordo accento, spunta bianca | mai | — |
| Stelle | grigio `--text-tertiary` | nessun hover-preview (a differenza di molti rating: **niente anteprima al passaggio**) | non focusabile | piene in colore accento fino a `rating` | mai | `rating:0` = 5 stelle grigie |
| `Scelta` | contorno verde `#2E9E5B` su fondo trasparente | fondo verde 10% | non focusabile | `.chosen`: fondo verde pieno, testo bianco — **solo** quando la foto corrente è `taken` | mai | — |
| `Scarta` | contorno rosso | fondo `--danger-tint` | non focusabile | **nessuno stato "attivo"**: su una foto già scartata il pulsante ha lo stesso aspetto di sempre — asimmetria con `Scelta`, vedi Ambiguità | mai | — |
| `Rinomina lotto…` | fantasma | sfondo chip | non focusabile | — | mai; se il lotto non ha foto `root` e l'interruttore è spento, è il **dialog** a mostrare `"Nessuna foto in questo ambito."` e `Applica` resta inerte | — |
| Barra di selezione | compare da 1 foto selezionata | — | tutti i suoi controlli sono focusabili | — | mai disabilitata | quando la selezione torna a 0 la barra sparisce e riappaiono i chip |
| Schermata | — | — | — | — | — | coda vuota → `"Niente da mostrare con questo filtro"` + `"Cambia filtro qui sopra, oppure torna a \"Tutte\"."` |

Nessuno stato di **caricamento** e nessuno stato di **errore** sono rappresentati: i dati sono
sincroni e generati in memoria. In un'implementazione reale servono almeno: filmino in
caricamento, miniatura non decodificabile, spostamento del file fallito.

### 8. Da dove ci si arriva e dove si va

**In ingresso:**

- dalla griglia dei lotti, aprendo una scheda (indice a 0, filtro `Tutte`, selezione vuota);
- dal selettore rapido di lotto, scegliendo un altro lotto (stesse condizioni di partenza);
- dal **lightbox** di una foto di lotto: nel pannello info il nome del lotto è cliccabile e
  riporta qui, impostando `cullingBatchId` su quel lotto e chiudendo il lightbox (riga 4295).
  Attenzione: non ripristina l'indice sulla foto da cui si veniva, riparte dall'indice corrente
  della coda.

**In uscita:**

- `Tutti i lotti` o la briciola col nome del lotto → griglia dei lotti (svuotando la selezione);
- selettore di lotto → un altro lotto, restando in questa schermata;
- pulsante info → **lightbox** sulla stessa foto. Nel lightbox le frecce ← → navigano la
  **coda di culling corrente, filtro compreso** (`lbNeighborList`, righe 4067–4073, con
  commento: *"dentro un lotto di culling il «filmino vicini» del lightbox è la coda di culling
  corrente (rispetta il filtro attivo), non un mese di una cartella di libreria — la foto
  potrebbe non avere nemmeno un folderId/monthOffset"*). Da lì `Esc` (o il pulsante di
  chiusura) riporta esattamente qui;
- `Rinomina lotto…` / `Rinomina…` → dialog modale sopra la schermata (SP-5), che al ritorno
  rimette il focus sul pulsante che l'ha aperto;
- `Svuota scartati` → dialog di conferma (SP-5) e ritorno qui;
- menu laterale / tab bar mobile → qualunque altra vista; **lo stato del lotto aperto,
  dell'indice e del filtro sopravvive** all'uscita e al rientro (non viene resettato), mentre
  la selezione multipla rimane anch'essa in memoria.

Topbar desktop: `Culling / <b>Nome lotto</b>`. Header mobile: il titolo è il **nome del lotto**
e la freccia indietro porta direttamente a `state.view='foto'` — **non** alla griglia dei lotti.
→ vedi Ambiguità.

### 9. Dati necessari a questa schermata

**Legge (per lotto):** nome del lotto; elenco delle sue foto **nell'ordine di importazione**;
i tre conteggi presi/scartati/da vedere sul lotto intero. Più, per **ogni foto** della coda:
miniatura, nome file, se è RAW e se ha un JPEG affiancato (per il badge `RAW`/`RAW+JPEG`),
valutazione 0–5, stato nel lotto (da valutare / preso / scartato), e — per il pulsante info,
cioè per il lightbox — data e ora di scatto, fotocamera, obiettivo, diaframma, ISO, tempo,
larghezza e altezza in pixel, dimensione del RAW e del JPEG, titolo, posizione (che per una
foto di lotto **parte sempre vuota**: vedi il commento alle righe 1645–1649,
*"le foto di un lotto di culling, appena importate, non hanno alcuna posizione finché l'utente
non la imposta a mano"*).

**Scrive:**

- lo **stato della foto nel lotto** (da valutare ↔ preso ↔ scartato) — che nel sistema reale
  significa **spostare il file** nella sotto-area corrispondente del lotto;
- la **valutazione 0–5** della singola foto (stelle o tasti 1–5);
- il **nome file** di una o più foto (rinomina con formula);
- l'**eliminazione definitiva** delle foto scartate di un lotto ("Svuota scartati");
- indirettamente, tramite il lightbox: titolo e posizione della singola foto.

Stato di sola interfaccia (non dati): quale lotto è aperto, indice della foto corrente, filtro
attivo, insieme delle foto selezionate, indice di ancoraggio della selezione, apertura del
selettore di lotto.

---

## 16. Il selettore rapido di lotto

### 1. Nome e scopo

Pannello a tendina agganciato al nome del lotto nell'intestazione, che permette di **saltare a
un altro lotto senza tornare alla griglia**.

### 2. Cosa mostra

`renderCullBatchSwitchPanel`, righe 3883–3898. Il pannello (`.culling-switch-panel`) è
posizionato in assoluto sotto il trigger (`top:calc(100% + 6px); left:0`), largo almeno 220 px,
sfondo `--card-bg`, bordo `--border-strong`, angoli 10 px, ombra `0 12px 30px rgba(0,0,0,.18)`,
`z-index:60`.

Contiene **una riga per ogni lotto di `CULLING_BATCHES`, sempre tutti e tre, incluso quello
attualmente aperto**, in ordine fisso. Ogni riga mostra due sole informazioni:

- a sinistra il **nome del lotto**;
- a destra, in colore terziario, **`"<N> da vedere"`** — cioè le sole foto ancora da valutare
  (`counts.todo`). Non mostra presi, scartati, totale, data né copertina.

Righe separate da una linea (`border-bottom`), tranne l'ultima.

### 3. Ogni controllo, uno per uno

| # | Elemento | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `<nome lotto> ⌄` (nell'intestazione) | trigger | Interruttore aperto/chiuso del pannello (`state.cullingSwitcherOpen`) |
| 2 | riga `<nome lotto>` + `<N> da vedere` | riga cliccabile | Passa a quel lotto: `cullingBatchId=<id>`, `cullingIdx=0`, `cullingFilter='all'`, chiude il pannello, `cullingClearSelection()`, `renderAll()` |

Non c'è campo di ricerca, non c'è voce `"Tutti i lotti"` dentro il pannello, non c'è
`"Nuovo lotto"`. **Non previsti nel mockup.**

### 4. Interazioni da mouse

- **Click sul trigger**: apre/chiude. Il gestore ferma la propagazione (`stopPropagation`) per
  non essere subito richiuso dal listener globale.
- **Click su una riga**: cambia lotto.
- **Click fuori** dal trigger e dal pannello: chiude (listener a livello di documento, righe
  3899–3903, condizionato a `state.view==='culling'`) — è SP-14 nella parte "click fuori
  chiude". Da notare che questo listener chiama solo `renderCullBatchSwitchPanel()`, non
  `renderAll()`: chiude il pannello senza ridisegnare il resto.
- **Hover su una riga**: sfondo `var(--chip-bg)`.
- Tasto destro, doppio click, trascinamento, scroll interno: nessun comportamento dedicato.
  Il pannello non ha `max-height` né scroll: con tre lotti non serve, con molti lotti
  crescerebbe indefinitamente. → vedi Ambiguità.

### 5. Interazioni da tastiera

- Il trigger passa da `bindActivatable` (Invio/Spazio = click), **ma non ha `tabindex` né
  `role`**: non è raggiungibile con Tab.
- Le righe hanno solo `onclick`: **nessuna attivazione da tastiera, nessuna navigazione con
  ↑/↓**, nessun `role="menu"`/`role="menuitem"`, nessun `aria-expanded` sul trigger.
- **`Esc` non chiude questo pannello**: la parte "Esc chiude" di SP-14 **manca qui**. Il
  gestore tastiera globale non prevede `Escape` per `cullingSwitcherOpen`, e con il pannello
  aperto le scorciatoie del culling (frecce, `P`, `X`, `1–5`) restano attive e agiscono sulla
  foto sotto. → vedi Ambiguità.

### 6. Animazioni e transizioni

- Nessuna animazione di apertura/chiusura: il pannello viene inserito e rimosso dal DOM di
  colpo (nessun fade, nessuno slide, nessuna trasformazione di scala).
- La freccia `chevronDown` del trigger **non ruota** quando il pannello è aperto (nessuna regola
  `transform`, a differenza di altri elementi dell'app che ne hanno una — es.
  `.picklist-trigger .ico{transition:transform .12s ease}`).
- Le righe seguono la transizione globale `background-color .2s ease` su hover.

### 7. Stati per ogni controllo

| Controllo | Normale | Hover | Focus | Attivo | Disabilitato | Vuoto |
|---|---|---|---|---|---|---|
| Trigger | chip grigio, testo secondario | nessuna regola | non focusabile | **nessun aspetto "aperto"** | mai | — |
| Riga di un lotto | testo normale 12.5 px | sfondo `--chip-bg` | non focusabile | `.current` (lotto già aperto): testo in colore accento e `font-weight:600` — **resta comunque cliccabile**, e ricliccarlo azzera indice/filtro/selezione del lotto corrente | mai | conteggio `0 da vedere` per un lotto interamente lavorato |
| Pannello | — | — | — | — | — | `CULLING_BATCHES` non è mai vuoto: stato "nessun lotto" non rappresentato |

### 8. Da dove ci si arriva e dove si va

Esiste **solo** dentro un lotto aperto (fa parte di `cullingStageHeaderHTML`); non compare nella
griglia dei lotti. Si apre dal trigger e si chiude scegliendo un lotto o cliccando fuori.
Scegliendo un lotto si resta nella stessa schermata (lotto aperto), con l'altro lotto caricato.

### 9. Dati necessari a questa schermata

**Legge:** per ogni lotto, id, nome e numero di foto ancora da valutare; più l'id del lotto
attualmente aperto (per marcare la riga corrente).
**Scrive:** nulla sui dati — solo lo stato di navigazione (lotto aperto, indice a 0, filtro a
"Tutte", selezione svuotata, pannello chiuso).

---

## 17. Dialog "Scegli la cartella radice di culling"

### 1. Nome e scopo

Dialog modale che permette di scegliere, navigando un albero di cartelle, **la cartella del
disco dentro cui vivono tutti i lotti di culling**.

### 2. Cosa mostra

`openCullingRootPickerDialog`, righe 5675–5718. Scrim modale standard (SP-5) + `.modal-card`
larga **420 px** (86% su mobile), `role="dialog"`, `aria-modal="true"`,
`aria-labelledby="fsPickTitle"`. Contenuto:

1. **titolo**: `"Scegli la cartella radice di culling"`;
2. **sottotitolo**: `"Dentro, ogni sottocartella diventa un lotto — Keeppix crea da sola le
   sottocartelle dei presi/scartati quando servono."`;
3. **briciole di percorso** (`.folder-tree-crumbs`, `#fsCrumbs`): sempre una briciola `"/"`
   iniziale, seguita da `" / <segmento>"` per ogni livello del percorso in corso di modifica —
   es. `/ / volume1 / Foto / Culling`. Ogni segmento è cliccabile;
4. **elenco delle sottocartelle** del livello corrente (`.folder-tree-list`, `#fsList`, bordo,
   angoli 9 px, `max-height:220px` con scroll verticale). Ogni riga: icona `folder` (14 px),
   nome della cartella, `chevronRight` (12 px) a destra. Se non ci sono sottocartelle:
   `"Nessuna sottocartella qui."`;
5. **due pulsanti**: `"Usa questa cartella"` (primario, accento) e `"Annulla"` (fantasma).

L'albero è finto e cablato nel codice (`MOCK_FS_TREE`, righe 1606–1619), con il commento:
*"Albero finto: non c'è un vero filesystem in questo mockup, ma la cartella radice di culling
(impostabile in Impostazioni) deve comunque poter essere «scelta» navigando delle cartelle."*
La struttura è:

```
/
└── volume1
    ├── Foto
    │   ├── Culling
    │   │   ├── 2026
    │   │   └── Archivio
    │   └── Libreria
    └── Backup
```

All'apertura il dialog si posiziona **sul percorso attualmente configurato**
(`state.cullingRootFolder`, di default `/volume1/Foto/Culling`) se quel percorso esiste
nell'albero; altrimenti riparte dalla radice `/`.

Non mostra: spazio libero, permessi di scrittura, numero di elementi per cartella, creazione di
una nuova cartella, campo per digitare un percorso a mano. **Non previsti nel mockup.**

### 3. Ogni controllo, uno per uno

| # | Etichetta esatta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `/` (prima briciola) | briciola cliccabile | Torna alla radice (`path = []`) |
| 2 | `<segmento>` (una per livello) | briciola cliccabile | Tronca il percorso a quel livello incluso |
| 3 | riga cartella (una per sottocartella) | riga cliccabile con freccia | Entra nella cartella (aggiunge il segmento al percorso) e ridisegna elenco e briciole |
| 4 | `Usa questa cartella` | pulsante primario | `state.cullingRootFolder = '/' + path.join('/')`, toast `"Cartella di culling aggiornata."`, chiude, ridisegna |
| 5 | `Annulla` | pulsante fantasma | Chiude senza salvare |

**Validazione:** nessuna. `"Usa questa cartella"` è **sempre attivo**, anche sulla radice — in
quel caso il percorso salvato è `"/"`. Non c'è controllo che la cartella sia scrivibile, né
avviso se la cartella scelta non contiene sottocartelle (quindi zero lotti), né conferma se si
cambia radice avendo lotti già aperti. → vedi Ambiguità.

### 4. Interazioni da mouse

- **Click su una riga cartella** → entra; l'elenco viene riscritto immediatamente. Non c'è
  doppio click: **un solo click entra** (quindi non esiste il gesto "un click seleziona, doppio
  click entra": la cartella corrente è sempre quella in cui si è entrati).
- **Click su una briciola** → risale al livello scelto.
- **Hover su una riga** → sfondo `var(--chip-bg)`.
- **Hover su una briciola** → sottolineata e in colore `--text`.
- **Scroll** dentro l'elenco (oltre le ~220 px di altezza).
- **Click sullo scrim**: **non chiude il dialog** — non è previsto alcun handler sul fondo.
  Deviazione da SP-5, che vale per tutti i dialog dell'app (stesso schema in
  `openConfirmDialog`, `openRenameDialog`, `openDeleteDialogGeneric`).
- Tasto destro / trascinamento: **non previsti.**

### 5. Interazioni da tastiera

- **`Esc` chiude** il dialog senza salvare (listener dedicato registrato all'apertura e rimosso
  alla chiusura).
- Briciole, righe cartella e i due pulsanti passano tutti da `bindActivatable`: **Invio e Spazio
  li attivano** (SP-8). Righe e pulsanti hanno `role="button"` e `tabindex="0"`, quindi sono
  raggiungibili con Tab; **le briciole no** (`bindActivatable` senza `tabindex`).
- **Non c'è trappola del focus**: Tab può uscire dal dialog e raggiungere gli elementi della
  pagina sottostante.
- **Nessun elemento riceve il focus all'apertura** (a differenza di `openConfirmDialog`, che
  mette il focus su `Annulla`, e di `openDeleteDialogGeneric`, che lo mette sulla prima
  opzione). Alla chiusura, invece, **il focus torna correttamente all'elemento che ha aperto il
  dialog** (`trigger.focus()`).
- ↑/↓ per scorrere l'elenco: **non previsti.**

### 6. Animazioni e transizioni

- Comparsa e scomparsa del dialog: **istantanee**, nessun fade dello scrim, nessuna scala della
  card.
- Righe e briciole: solo la transizione globale (`background-color`/`color` `.2s ease`) su hover.
- Toast di conferma: `opacity .2s ease, transform .2s ease` (SP-6).

### 7. Stati per ogni controllo

| Controllo | Normale | Hover | Focus | Attivo | Disabilitato | Vuoto |
|---|---|---|---|---|---|---|
| Riga cartella | 13 px, icona + nome + freccia | sfondo chip | outline accento | — | mai | — |
| Elenco | fino a 220 px, poi scroll | — | — | — | — | `"Nessuna sottocartella qui."` in colore terziario |
| Briciola | 12 px, secondaria | sottolineata, `--text` | non focusabile | nessuna evidenziazione del livello corrente | mai | — |
| `Usa questa cartella` | primario accento | `filter:brightness(1.05)` | outline accento | — | **mai disabilitato**, nemmeno sulla radice `/` | — |
| `Annulla` | fantasma | sfondo chip | outline accento | — | mai | — |

Nessuno stato di caricamento (l'albero è sincrono) e nessuno stato di errore (cartella
irraggiungibile, permessi mancanti): in un backend reale servono entrambi.

### 8. Da dove ci si arriva e dove si va

**Unico punto di ingresso:** Impostazioni → sezione **`"Cartella di culling"`**, pulsante
`"Cambia…"` (riga 6147). La sezione mostra:

- titolo `"Cartella di culling"`;
- sottotitolo `"La cartella radice sul disco dentro cui vivono i lotti — una sottocartella per
  importazione. Serve anche a poterla sincronizzare da un altro computer (es. via WebDAV) dopo
  aver scelto le foto."`;
- il percorso corrente e, sotto, `"<N> lotti attivi"` (il numero di elementi di
  `CULLING_BATCHES`, quindi **3** — non dipende dalla cartella scelta);
- il pulsante `"Cambia…"`.

Il link `"Cambia in Impostazioni"` della griglia dei lotti porta alla pagina Impostazioni ma
**non** apre questo dialog: servono due passaggi.

**In uscita:** conferma o annullamento riportano alla pagina Impostazioni, col focus sul
pulsante `"Cambia…"`. Il nuovo percorso si riflette immediatamente nella riga in cima alla
griglia dei lotti.

### 9. Dati necessari a questa schermata

**Legge:** l'albero delle cartelle del server (nome e figli, livello per livello, esplorabile
in profondità) e il percorso della cartella radice di culling attualmente configurata.
**Scrive:** il nuovo percorso della cartella radice di culling — un'impostazione globale, non
un dato di una foto. Nel mockup **cambiarlo non tocca in alcun modo i lotti esistenti**, che
restano gli stessi tre.

---

# Parte IV — Vista dettaglio

> Blocco documentato: `index.html` righe **4057–4351** (`openLightbox`, `closeLightbox`,
> `lbNeighborList`, `lbLookup`, `lbNav`, `wireLbFaceHover`, `lbDownloadOriginal`,
> `lbRotatePhoto`, `renderLightbox`, `lbTagSectionHTML`, `lbFacesSectionHTML`,
> `lbInfoPanelHTML`), righe **6289–6299** (scorciatoie da tastiera), riga **2289**
> (chiusura del menu ⋯ al click fuori), riga **2116–2118** (stato), CSS **righe 482–567**
> (`.lightbox`, `.lb-*`) e **1002–1016** (`.lb-tag-*`), **1072–1078** (varianti mobile).

Il lightbox non è una "pagina": è un livello che si sovrappone alla vista corrente
(`.lightbox { position:absolute; inset:0; z-index:50 }` dentro `#app`). `state.view` non
cambia quando lo si apre, e `renderAll()` continua a disegnare la vista sottostante prima di
chiamare `renderLightbox()` (riga 3085: `if(state.lightbox) renderLightbox(); else
document.getElementById('lightboxRoot').innerHTML='';`).

---

## 18. Lightbox — struttura e barra superiore

### 1. Nome e scopo

Vista a tutto schermo di una singola foto, con barra superiore di comandi, frecce di
navigazione, filmino delle foto vicine e pannello informazioni laterale; si apre sopra
qualunque griglia della libreria e anche dallo stage di un lotto di culling.

### 2. Cosa mostra

Il contenitore `.lightbox` è una colonna flex, **sempre nera** (`background:#000`,
`color:#f2f2f2`) indipendentemente dal tema chiaro/scuro dell'app. Dall'alto in basso:

**a) Barra superiore `.lb-top`** (`padding:12px 16px`, `flex:0 0 auto`, contenuto agli
estremi):

- a sinistra (`.lb-top-left`, `gap:6px`): il pulsante di chiusura e il **nome del file**
  (`.lb-filename`, 13px, colore `#d8d8d8`) — es. `DSC08123.ARW`. È lo stesso `p.filename`
  ripetuto anche come titolo `<h3>` del pannello informazioni.
- a destra (`.lb-top-right`, `gap:6px`): **quattro** icone quadrate 32×32 (cuore,
  condividi, info, ⋯) — dettagliate al punto 3.

**b) Corpo `.lb-body`** (`flex:1; display:flex; min-height:0`):

- `.lb-stage` (`flex:1`, `padding:10px 60px` — i 60px laterali servono a lasciare spazio
  alle frecce): contiene
  - la freccia sinistra `.lb-arrow.left`, **presente nel DOM solo se `idx>0`**;
  - `.lb-image` (`width:100%;height:100%;border-radius:6px;position:relative`). Nel mockup
    **non c'è una foto vera**: `tileStyle(p)` produce un
    `linear-gradient(135deg, p.colorA, p.colorB)`. Non ci sono quindi stati di caricamento,
    né immagine a bassa risoluzione sostituita da quella piena;
  - sopra l'immagine, **solo in libreria e solo se `state.faceRecognitionEnabled`**, un
    riquadro `.lb-face-box` per ogni volto confermato che abbia un `box` — posizionato in
    percentuale (`left/top/width/height` da `f.box`), con dentro un'etichetta
    `.lb-face-label` col nome della persona (`personDisplayName`, cioè il nome oppure
    `Persona {autoNum}` se senza nome). Il commento CSS (righe 509–512) spiega il perché
    delle percentuali: "coordinate in % così restano corrette qualunque sia la dimensione
    finale renderizzata di `.lb-image`";
  - la freccia destra `.lb-arrow.right`, **presente solo se `idx < list.length-1`**;
- il pannello informazioni `.lb-info` (296px, sfondo `#0c0c0c`, bordo sinistro `#232323`,
  `overflow-y:auto`, `padding:18px 18px 28px`), reso solo se `state.lbInfoOpen`.

**c) Filmino `.lb-filmstrip`** (`flex:0 0 auto`, `padding:10px 16px`, `overflow-x:auto`,
bordo superiore `#1c1c1c`): una miniatura `.fthumb` 52×52 (`border-radius:5px`) per **ogni**
foto del "vicinato" (vedi punto 8 e la sezione 4). Le miniature sono a `opacity:.6`; quella
corrente ha classe `current` → bordo 2px `var(--accent)` e `opacity:1`. Anche qui la
miniatura è il gradiente `tileStyle(x)`, non un'immagine.

Non è mostrato nel lightbox: contatore "n di N", percorso su disco, data di importazione,
zoom/livello di ingrandimento, istogramma, badge RAW sull'immagine (c'è nello stage del
culling ma non qui), spunta di selezione (SP-1/SP-2 non si applicano qui).

### 3. Ogni controllo, uno per uno

Barra superiore, da sinistra a destra:

| # | Elemento | Tipo | Etichetta di accessibilità | Cosa fa |
|---|---|---|---|---|
| 1 | `#lbClose` | `div.lb-icon-btn`, icona `close` 18px | **nessuna** (`aria-label` assente, nessun `role`, nessun `tabindex`) | `closeLightbox()` → `state.lightbox=null` + `renderAll()` |
| 2 | `#lbFav` | `div.lb-icon-btn.fav`, icona `heart` 17px, classe `active` se `p.isFav` | **nessuna** (nessun `aria-label`, nessun `role`, nessun `tabindex`, nessun `aria-pressed`) | interruttore preferito: `p.isFav = !p.isFav` + `renderAll()` |
| 3 | `#lbShareBtn` | `div.lb-icon-btn`, icona `share` 17px | `aria-label="Condividi"` (ma **senza** `role="button"` né `tabindex`) | `stopPropagation()` + `openShareSelectionDialog([p.id])` → dialog modale "Condividi 1 elemento" (persone già invitate + "Crea link di condivisione") |
| 4 | `#lbInfoToggle` | `div.lb-icon-btn`, icona `info` 17px, classe `active` se `state.lbInfoOpen` | **nessuna** (nessun `aria-label`, nessun `role`, nessun `tabindex`, nessun `aria-pressed`) | mostra/nasconde il pannello informazioni |
| 5 | `#lbMoreBtn` | `div.lb-icon-btn`, icona `more` (tre pallini) 17px, `style="position:relative"` | `role="button" tabindex="0" aria-haspopup="true" aria-expanded="{state.lbMoreOpen}" aria-label="Altre azioni"` | apre/chiude il menu ⋯ (sezione 3 di questo documento). È l'**unico** controllo del lightbox raggiungibile con Tab fuori dal pannello informazioni |

Le quattro icone a destra sono, nell'ordine: **cuore (preferito)**, **condividi**,
**informazioni**, **altre azioni (⋯)**. Solo due delle quattro hanno un'etichetta di
accessibilità (`"Condividi"` e `"Altre azioni"`); cuore e info **non ne hanno nessuna** —
per uno screen reader sono elementi muti e non focalizzabili. Vedi ambiguità.

Altri controlli del livello lightbox (esclusi quelli del pannello informazioni, sezione 2):

| Elemento | Tipo | Cosa fa |
|---|---|---|
| `#lbPrev` (`.lb-arrow.left`) | cerchio 38px, icona `chevronLeft` 20px | `lbNav(-1)`. Assente dal DOM sulla prima foto |
| `#lbNext` (`.lb-arrow.right`) | cerchio 38px, icona `chevronRight` 20px | `lbNav(1)`. Assente dal DOM sull'ultima foto |
| `[data-lbthumb]` (`.fthumb`) | miniatura 52×52 nel filmino | salta direttamente a quella foto: `lbLookup(id)` → `state.lightbox = np`, `state.lbRawMode = np.isRaw?'raw':'jpg'` |
| `[data-facebox]` (`.lb-face-box`) | riquadro sul volto, invisibile finché non "acceso" | apre `openFaceBoxMenuDialog(face)` |

Nessuna delle frecce, delle miniature e dei riquadri volto ha `role`/`tabindex`: sono tutti
**solo mouse**.

### 4. Interazioni da mouse

- **Click** su ognuno dei controlli sopra.
- **Click sull'immagine**: non fa niente. Nessuno zoom, nessun pan, nessun "click a destra
  = foto successiva".
- **Doppio click**: non previsto nel mockup (nessun handler `ondblclick` nel lightbox).
- **Tasto destro**: nessun menu contestuale — non previsto nel mockup.
- **Rotellina / scroll**: nessuno zoom né avanzamento foto. Il pannello informazioni scorre
  normalmente (`overflow-y:auto`), il filmino scorre orizzontalmente (`overflow-x:auto`) ma
  **senza** rotazione della rotella verticale in orizzontale (non implementata).
- **Trascinamento**: non previsto nel mockup — né swipe sull'immagine, né riordino del
  filmino, né trascinamento della foto verso un album.
- **Click sullo sfondo nero** attorno all'immagine: **non chiude** il lightbox (a differenza
  dello scrim dei dialog modali, SP-5). L'unico effetto del click "a vuoto" è la chiusura
  del menu ⋯ se aperto (listener globale di riga 2289).
- **Hover**:
  - `.lb-icon-btn:hover{background:rgba(255,255,255,.12)}` — nessuna transizione dichiarata,
    quindi il cambio è istantaneo;
  - `.lb-arrow:hover{background:rgba(255,255,255,.18)}` (base: `rgba(255,255,255,.08)`);
  - `.lb-filmstrip .fthumb:hover{opacity:1}` (base `.6`) — istantaneo;
  - hover sul chip di una persona nel pannello informazioni → accende il riquadro sul volto
    (vedi punto 6);
  - `.lb-face-box:hover{border-color:var(--accent)}` con `transition: border-color .12s ease`.
- **Nessun tooltip** `[data-tip]` (SP-7) è usato nel lightbox: i pulsanti icona non hanno
  etichetta al passaggio del mouse. È una deviazione rispetto alle barre di selezione delle
  griglie, dove le icone senza testo hanno sempre tooltip + `aria-label`.

### 5. Interazioni da tastiera

Gestore globale, righe 6289–6299. Il blocco `if(state.lightbox){ … return; }` è il **primo**
del listener e termina con `return`: finché il lightbox è aperto, **tutte le altre
scorciatoie dell'app sono soppresse** (comprese `1`–`5`, `P`, `X`, shift+frecce del culling).

| Tasto | Effetto |
|---|---|
| `Esc` (1ª pressione, se il menu ⋯ è aperto) | chiude **solo** il menu ⋯ (`state.lbMoreOpen=false`) e ritorna — il lightbox resta aperto. Commento nel codice: "Esc chiude prima il menu ⋯ se aperto, solo un secondo Esc chiude il lightbox intero" |
| `Esc` (menu chiuso) | `closeLightbox()` |
| `←` | `lbNav(-1)` — foto precedente nella lista dei vicini |
| `→` | `lbNav(1)` — foto successiva |
| `i` / `I` | mostra/nasconde il pannello informazioni (equivalente a `#lbInfoToggle`) |
| `f` / `F` | preferito sì/no sulla foto corrente (equivalente a `#lbFav`) |

Non implementati: `Spazio`, `Invio`, `Home`/`End`, `Canc`, `1`–`5` per le stelle, `+`/`-`
per lo zoom, `↑`/`↓`.

**Tab / Shift+Tab e ordine del focus.** Non c'è nessuna trappola del focus e nessun focus
automatico all'apertura (a differenza dei dialog modali SP-5, che portano il focus alla
prima opzione e lo restituiscono al trigger). Gli unici elementi del lightbox raggiungibili
con Tab sono, nell'ordine del DOM:

1. `#lbMoreBtn` (`role="button" tabindex="0"`);
2. nel pannello informazioni: il campo `#lbTitleInput`; i chip persona `[data-facechip]`
   (`role="button" tabindex="0"`); i chip tag applicati dall'IA (`role="button"
   tabindex="0"`); le `×` di rimozione tag (`role="button" tabindex="0"`); le `✓`/`×` dei
   tag in attesa di conferma (`role="button" tabindex="0"`).

Restano **fuori dal Tab**: chiudi, preferito, condividi, info, le due frecce, tutte le
miniature del filmino, le stelle, i chip RAW/JPEG, la mini-mappa, il pulsante
"Modifica/Imposta posizione…", i chip "+ aggiungi", tutti i pulsanti della sezione Azioni e
tutte le voci del menu ⋯. Poiché il lightbox non intrappola il focus, con Tab si finisce
sugli elementi della **vista sottostante**, che è ancora nel DOM e visibile a lettori di
schermo.

`bindActivatable` (SP-8) è applicato a: `#lbMoreBtn`, chip persona, chip tag IA, `×` di
rimozione tag, `✓`/`×` dei suggerimenti — per questi Invio e Spazio equivalgono al click.

### 6. Animazioni e transizioni

- **Apertura e chiusura del lightbox: nessuna animazione.** Non c'è alcuna `transition` né
  `@keyframes` su `.lightbox`; compare e scompare istantaneamente insieme al re-render.
- **Cambio foto: nessuna transizione.** `lbNav` sostituisce `state.lightbox` e ridisegna
  tutto: niente dissolvenza, niente scorrimento laterale.
- **Riquadri volto** — l'unica vera animazione del lightbox:
  `.lb-face-box{opacity:0;pointer-events:none;transition:opacity .12s ease, border-color .12s ease}`,
  `.lb-face-box.face-hint-visible{opacity:1;pointer-events:auto}`,
  `.lb-face-box:hover{border-color:var(--accent)}`.
  *Cosa comunica*: quale nome dell'elenco "Persone" corrisponde a quale faccia
  nell'inquadratura, **senza** coprire stabilmente la foto di rettangoli. Il commento CSS
  (righe 509–512) lo dice esplicitamente: "Restano invisibili per non ingombrare la foto:
  compaiono solo passando sopra il nome corrispondente".
  Il ritardo di chiusura è in JS, non in CSS: `wireLbFaceHover` nasconde il riquadro con un
  `setTimeout` di **200 ms** dopo `mouseleave`/`blur`, e lo annulla se nel frattempo il
  puntatore entra nel riquadro. Il commento (righe 4085–4088) spiega il perché: "un piccolo
  margine di tolleranza per poter spostare il puntatore dal chip al riquadro senza che
  sparisca a metà strada".
- **Toast** (SP-6): `.toast{opacity:0;transform:translateX(-50%) translateY(10px);transition:opacity .2s ease, transform .2s ease}`,
  classe `show` aggiunta dopo 10 ms, tolta dopo 2400 ms, elemento rimosso dopo altri 250 ms.
- **Nessuna transizione** su `.lb-icon-btn`, `.lb-arrow`, `.fthumb`, `.lb-raw-chip`,
  `.lb-action-btn`, `.lb-tag-chip`: tutti gli stati hover cambiano istantaneamente.
- **Il filmino non si anima e non si riposiziona**: non c'è nessuno `scrollIntoView` sulla
  miniatura corrente. Navigando con le frecce, la miniatura evidenziata può uscire dalla
  parte visibile senza che la striscia scorra.

### 7. Stati per ogni controllo

- `.lb-icon-btn` — **normale**: sfondo trasparente, colore `#f2f2f2`; **hover**:
  `rgba(255,255,255,.12)`; **attivo** (`.active`): colore `var(--accent)`; **focus-visible**:
  `outline:2.5px solid var(--accent); outline-offset:2px` — ma solo per `#lbMoreBtn`, l'unico
  con `role="button"`; **disabilitato**: non esiste, nessuno dei quattro pulsanti viene mai
  disabilitato.
- `#lbFav` — stato acceso: `.lb-icon-btn.fav.active svg{fill:currentColor}`. Il commento
  (righe 494–495) spiega la scelta: "stesso principio del cuoricino nella griglia: forma
  invariata, solo il riempimento cambia quando è preferita — niente più scambio
  cuore/stella".
- `#lbInfoToggle` — acceso/spento riflette `state.lbInfoOpen`, che `openLightbox()` rimette
  a `true` **ogni volta** che si apre una foto: la preferenza "pannello chiuso" non viene
  ricordata da un'apertura all'altra (ma resiste alla navigazione con frecce/miniature,
  perché quelle non passano da `openLightbox`).
- `.lb-arrow` — **normale** `rgba(255,255,255,.08)`, **hover** `rgba(255,255,255,.18)`.
  Agli estremi della lista **non c'è uno stato disabilitato**: la freccia viene proprio
  omessa dal markup. (Il culling, per confronto, usa lo stile disabilitato
  `opacity:.35;pointer-events:none` sulle sue frecce; il lightbox no.)
- `.fthumb` — **normale** `opacity:.6`, bordo trasparente; **hover** `opacity:1`;
  **corrente** (`.current`) bordo `var(--accent)` + `opacity:1`; nessuno stato selezionato,
  nessuno stato di scelta/scarto visibile nel filmino del lightbox.
- **Stato vuoto**: non esiste. `openLightbox` esce subito se l'id non corrisponde a nessuna
  foto (`if(!p) return;`), quindi non si arriva mai a un lightbox senza contenuto.
- **Stato di caricamento / errore**: non previsti nel mockup (non ci sono immagini reali da
  caricare, né gestione di un file mancante o corrotto).

### 8. Da dove ci si arriva e dove si va

**In ingresso** (tre punti):

1. **Da qualunque griglia di foto** (Foto, Preferiti, dettaglio Album, dettaglio Persona,
   risultati di Cerca…), tramite `wireTileOpen`: click, oppure Invio/Spazio sul tile
   (`.tile-open`). Se la selezione multipla è attiva, il click **seleziona invece di
   aprire**. Su mobile il tap apre, il tap prolungato entra in selezione.
2. **Dallo stage del culling**: il pulsante rotondo sull'immagine
   `#cullInfoBtn`, `aria-label="Dettagli foto — EXIF, posizione, rinomina"`. Non chiama
   `openLightbox()` ma imposta direttamente lo stato (riga 4026:
   `state.lightbox = cur; state.lbInfoOpen = true; state.lbRawMode = …`) — necessario perché
   `openLightbox()` cerca l'id in `allPhotos()`, che contiene solo le foto di libreria e
   **non** quelle dei lotti di culling.
3. **Dalla pagina "Problemi"**, dal dialog con l'elenco dei file: la riga
   `[data-openprobfile]` chiude il dialog, imposta `state.currentFolder` e
   `state.view='foto'`, poi chiama `openLightbox(id)` — così chiudendo il lightbox si resta
   nella cartella giusta.

**In uscita**:

- `#lbClose` o `Esc` → si torna alla vista sottostante, che non è mai cambiata
  (`state.view` non viene toccato).
- Link nel sottotitolo del pannello informazioni → cartella (`state.view='foto'` +
  `currentFolder`) oppure lotto (`state.view='culling'` + `cullingBatchId`); in entrambi i
  casi il gestore inline fa anche `state.lightbox=null`.
- Click sulla mini-mappa `.lb-map` → `state.view='mappa'` **ma senza azzerare
  `state.lightbox`**: la vista Mappa viene disegnata sotto, il lightbox resta sopra a
  coprirla (vedi ambiguità).
- "Vai alla persona" dal menu del volto → `state.lightbox=null; state.view='persone';
  state.openPerson=…`.
- "Elimina…" con una scelta confermata → `p.pick='reject'; p.trashChoice=choice;
  closeLightbox()`.
- Tutti gli altri dialog aperti dal lightbox (Condividi, Album, Tag, Rinomina, Posizione,
  Persona, Menu volto) tornano al lightbox alla chiusura, restituendo il focus al trigger.

### 9. Dati necessari a questa schermata

**Legge**, per la foto corrente e per ogni foto del vicinato:

- identificativo, nome file, anteprima (nel mockup surrogata da due colori di gradiente),
  proporzioni dell'inquadratura;
- data (giorno + mese, l'anno "2026" è scritto a mano nel template) e ora dello scatto;
- cartella di appartenenza col suo nome (foto di libreria) **oppure** lotto di culling col
  suo nome e lo stato della foto nel lotto (foto di culling);
- preferito sì/no, valutazione 0–5, titolo facoltativo;
- se è RAW, e se ha un JPEG affiancato; dimensione in MB del RAW e del JPEG;
- fotocamera, obiettivo, diaframma, tempo, ISO, dimensioni in pixel;
- posizione: etichetta del luogo e coordinate — impostata sulla foto, oppure ereditata dalla
  cartella, oppure assente;
- volti confermati con nome della persona e riquadro in percentuale (solo libreria);
- tag confermati con categoria, colore e **provenienza** (IA non ancora revisionata / umana),
  e tag suggeriti in attesa (SP-12);
- album di appartenenza, sia manuali (elenco esplicito) sia dinamici (calcolati dalle
  condizioni dell'album sulla foto);
- l'insieme delle foto "vicine" per il filmino e le frecce (vedi sezione 4).

**Scrive**: preferito, valutazione 0–5, titolo (stringa ripulita dagli spazi ai bordi),
posizione della foto, appartenenza agli album, assegnazioni di tag (conferma / rifiuto /
rimozione, tutte con provenienza "umana"), assegnazioni di volti (aggiunta manuale,
correzione di persona, "non è un volto"), e — solo dalla libreria — la marcatura di scarto
con la modalità di eliminazione scelta.

**Nel mockup non scrive davvero** (solo toast, vedi punto 3 della sezione 3): download
dell'originale, rotazione del file.

---

## 19. Pannello informazioni (pannello laterale destro)

### 1. Nome e scopo

Colonna laterale destra larga 296px che raccoglie tutti i metadati della foto e tutte le
azioni su di essa: alcuni campi sono modificabili sul posto, altri sono in sola lettura.

### 2. Cosa mostra

Il pannello è generato da `lbInfoPanelHTML(p)` ed è **sempre scuro** (`background:#0c0c0c`)
anche in tema chiaro. Il commento alle righe 557–561 spiega la conseguenza e il rimedio: il
campo Titolo eredita `.field input` che normalmente segue il tema, quindi è sovrascritto con
`background:#161616; border-color:#262626; color:#f0f0f0` "altrimenti in tema chiaro il campo
Titolo apparirebbe come un riquadro chiaro fuori posto".

**Elenco completo dei campi, nell'ordine esatto in cui appaiono**, con il gruppo di
appartenenza. Le intestazioni di gruppo (`.lb-info-label`) sono scritte nel codice in forma
normale ("Scatto", "Posizione", …) e rese **maiuscole dal CSS** (`text-transform:uppercase`,
10.5px, `letter-spacing:.06em`, colore `#7a7a7d`).

| # | Gruppo | Campo / etichetta esatta | Valore mostrato | Modificabile? |
|---|---|---|---|---|
| 1 | *(intestazione, senza gruppo)* | — (`<h3>`, 14.5px grassetto) | **nome del file**, es. `DSC08123.ARW` | **Sola lettura** (si cambia solo tramite "Rinomina…") |
| 2 | *(intestazione)* | — (`.lb-sub`, 12px, `#8f8f92`) | **data, ora e provenienza**: `"{giorno} {mese minuscolo} 2026, ore {H:MM}"`, poi ` · ` e un **link sottolineato** (`cursor:pointer;text-decoration:underline`) con il nome della **cartella** (libreria) o del **lotto** (culling). In culling segue un altro ` · ` con lo **stato nel lotto**: `"Presi"` / `"Scartati"` / `"Da valutare"` | **Sola lettura** (il link naviga, non modifica) |
| 3 | *(campo isolato, senza gruppo)* | **`Titolo`** + `(opzionale)` in tondo, colore `var(--text-tertiary)` — `<label for="lbTitleInput">` | campo di testo con il titolo attuale | **Modificabile** — vedi punto 3 |
| 4 | *(senza gruppo)* | *(nessuna etichetta)* | **valutazione a stelle**, 5 stelle da 16px (`starRowHTML(p,16)`): piene fino a `p.rating` in `var(--accent)`, le altre in `var(--text-tertiary)` | **Modificabile** (SP-9) |
| 5 | *(senza gruppo)* | *(nessuna etichetta)* | **badge/commutatore RAW–JPEG con le dimensioni** — vedi punto 3 | Commutabile, ma vedi punto 3: nel mockup non cambia nulla |
| 6 | **SCATTO** | `Fotocamera` | `p.camera`, es. `Sony α7R V` | Sola lettura |
| 7 | **SCATTO** | `Obiettivo` | `p.lens` | Sola lettura |
| 8 | **SCATTO** | `Esposizione` | `"{diaframma} · {tempo}s · ISO {iso}"`, es. `f/3.5 · 1/250s · ISO 400` — i tre valori sono in **un unico campo**, non separati | Sola lettura |
| 9 | **SCATTO** | `Dimensioni` | `"{larghezza}×{altezza}"` in pixel, es. `7008×4672` | Sola lettura |
| 10 | **POSIZIONE** | *(nessuna etichetta)* | **mini-mappa** `.lb-map` alta 100px con reticolo `.map-grid-lines` e un pin `.lb-map-pin` — **solo se la foto ha un luogo**. Il pin è in posizione **fissa** (`top:52%;left:46%`), non calcolata dalle coordinate | Cliccabile (porta alla vista Mappa) |
| 11 | **POSIZIONE** | *(nessuna etichetta)* | **luogo** `.lb-place`: `place.label`, e a capo le **coordinate** `.coords` (11px, `#8f8f92`) come `lat, lng` con **4 decimali** | Sola lettura |
| 11b | **POSIZIONE** | *(stato vuoto)* | se non c'è posizione, al posto di mappa+luogo compare `.lb-place-empty` in corsivo: **`"Nessuna posizione impostata."`** | — |
| 12 | **POSIZIONE** | **`Modifica posizione…`** oppure **`Imposta posizione…`** | pulsante `.lb-action-btn` con icona `locate` 14px; l'etichetta dipende dalla presenza di un luogo | **Modificabile** |
| 13 | **PERSONE** *(assente in culling e se il riconoscimento volti è spento)* | *(nessuna etichetta per riga)* | un chip `.lb-tag-chip` per ogni **volto confermato** sulla foto, con il nome della persona (`personDisplayName`: nome, o `Persona {n}` se senza nome), più il chip **`+ aggiungi`** | **Modificabile** |
| 14 | **TAG** *(assente in culling)* | nome della **categoria** (`.lb-tag-cat-name`, 10px, `#6b6b6e`), oppure `Senza categoria` | i tag confermati **raggruppati per categoria**, nello stesso ordine di `TAG_CATEGORIES` | **Modificabile** |
| 15 | **TAG** | chip tag | pallino colorato + nome del tag + (se applicato dall'IA e mai revisionato) marcatore **`IA`** + `×` di rimozione | **Modificabile** |
| 16 | **TAG** | **`+ aggiungi`** | chip che apre il selettore di tag | **Modificabile** |
| 17 | **TAG** | **`In attesa di conferma`** (seconda `.lb-info-label` dentro la stessa sezione, `margin-top:12px`) — presente solo se ci sono suggerimenti | chip tratteggiate `.lb-suggested-tag-chip` con pallino + nome + `✓` e `×` inline | **Modificabile** |
| 18 | **ALBUM** *(assente in culling)* | *(nessuna etichetta per riga)* | un chip `.lb-album-chip` **non cliccabile** per ogni album di cui la foto fa parte — album manuali (elenco `memberIds`) e album dinamici (condizioni valutate sulla foto: tutte se `matchAll`, altrimenti almeno una) — più il chip **`+ aggiungi`** | Solo tramite "+ aggiungi" |
| 19 | **AZIONI** | cinque pulsanti — vedi punto 3 | | |

**Riassunto: cosa è modificabile e cosa no.**
Modificabili dal pannello: **Titolo**, **valutazione**, **posizione**, **persone**, **tag**,
**album**. Modificabile dalla barra superiore: **preferito**.
In sola lettura: **nome file** (cambia solo passando da "Rinomina…"), **data e ora**,
**cartella/lotto di provenienza**, **stato di culling**, **fotocamera**, **obiettivo**,
**esposizione (diaframma/tempo/ISO)**, **dimensioni in pixel**, **dimensioni in MB dei file
RAW e JPEG**, **coordinate**.

### 3. Ogni controllo, uno per uno

**Link nel sottotitolo** — testo sottolineato con `onclick` inline.
Libreria: `state.view='foto'; state.currentFolder='{folderId}'; state.lightbox=null;
renderAll();`. Culling: `state.view='culling'; state.cullingBatchId='{batchId}';
state.lightbox=null; renderAll();`. Non ha `role`, `tabindex` né `href`: **solo mouse**.

**Campo "Titolo"** — `<input type="text" id="lbTitleInput">` dentro un `.field`
(`margin-bottom:14px`).

- Etichetta: `Titolo` seguita da `(opzionale)` in un `<span>` con `font-weight:400` e colore
  `var(--text-tertiary)`; `<label for="lbTitleInput">` correttamente associata.
- **Placeholder esatto: `Senza titolo`** (colore `#7a7a7d`).
- Valore iniziale: `escAttr(p.title)`; nel modello dati il titolo nasce vuoto — il commento
  in `genPhotos` lo dice: "titolo facoltativo, vuoto finché l'utente non lo imposta — vedi
  dettaglio foto / modifica multipla".
- Salvataggio: **`onchange`**, non `oninput`. Il valore viene quindi scritto solo quando il
  campo perde il focus o si preme Invio, non mentre si digita.
- Trasformazione: `p.title = e.target.value.trim()` — gli spazi iniziali e finali vengono
  **sempre** eliminati.
- **Se lasciato vuoto**: `p.title` diventa la stringa vuota; il campo torna a mostrare il
  placeholder `"Senza titolo"`. Non succede nient'altro: nessun errore, nessun titolo
  automatico, nessun ripiego sul nome del file. Il titolo **non** sostituisce mai il nome del
  file nel `<h3>` né nella barra superiore, e non è mostrato in nessun altro punto del
  lightbox.
- Nessuna validazione, nessuna lunghezza massima, nessun toast di conferma, nessun pulsante
  Salva/Annulla, nessuna gestione di Esc per annullare la modifica. Attenzione: `renderAll()`
  ricostruisce l'intero lightbox, quindi qualunque azione che ridisegna mentre si sta
  scrivendo **perde il testo non ancora confermato** (vedi ambiguità).

**Stelle** — `starRowHTML(p,16)` + `attachStarHandlers(root, id=>lbLookup(id))`. SP-9: click
sulla stella *n* imposta `p.rating=n`, riclick sulla stessa stella azzera (`p.rating=0`).
`stopPropagation()` sul click. **Deviazione da SP-9/SP-8**: qui le stelle sono `<span>` con
solo `cursor:pointer`, senza `role`, senza `tabindex` e senza `aria-label` → non usabili da
tastiera e mute per uno screen reader.

**Commutatore RAW / JPEG** (`.lb-raw-toggle`) — dipende da `p.stackType`:

- `stackType === 'raw_jpeg'` (RAW con JPEG affiancato): **due chip cliccabili**
  - `RAW · {sizeRaw} MB` (`data-rawmode="raw"`)
  - `JPEG · {sizeJpg} MB` (`data-rawmode="jpg"`)
  La chip corrispondente a `state.lbRawMode` ha la classe `active`.
- `stackType === 'raw_only'` (RAW senza JPEG): **una sola chip**, sempre `active` e **senza**
  `data-rawmode` (quindi **non cliccabile**), con etichetta
  `RAW · {sizeRaw} MB · nessun JPEG associato`.
- `stackType` nullo (foto JPEG semplice): **nessun blocco**, il commutatore non compare.
- Le dimensioni sono stringhe con la **virgola** decimale italiana (es. `62,0`), generate dal
  mockup, non numeri.

**Cosa cambia davvero il commutatore RAW/JPEG**: `state.lbRawMode` viene impostato
all'apertura del lightbox (`p.isRaw ? 'raw' : 'jpg'`), **reimpostato ad ogni cambio foto**
(freccia o miniatura), e cambiato dal click sulle chip — ma **nessun'altra parte del codice
legge `state.lbRawMode`** (le uniche occorrenze sono nelle due chip stesse, righe 4307–4308).
L'unico effetto osservabile è quindi **quale delle due chip è evidenziata**
(`.lb-raw-chip.active{background:var(--accent-tint);color:var(--accent);border-color:var(--accent)}`).
Non cambia l'immagine mostrata (nel mockup è comunque un gradiente), non cambia il
comportamento di "Scarica originale", non cambia niente di ciò che viene scritto sulla foto.
**È uno dei punti in cui il backend dovrà fare qualcosa di vero**: scegliere quale dei due
file della pila viene decodificato, mostrato e scaricato.

**Mini-mappa `.lb-map`** — `cursor:pointer`, `onclick` → `state.view='mappa'; renderAll()`.
Nessun `role`, nessun `tabindex`, nessuna etichetta di accessibilità: solo mouse.

**`Modifica posizione…` / `Imposta posizione…`** (`#lbEditPlaceBtn`) — apre
`openPhotoPlaceDialog(p)`: dialog modale "Imposta posizione", sottotitolo *"Nessuna mappa
reale in questo mockup — scegli tra i luoghi già noti alla libreria."*, elenco dei luoghi
delle cartelle con le loro coordinate a 2 decimali, riga **"Nessuna posizione"** e pulsante
**"Annulla"**. La regola di risoluzione della posizione è in `photoPlace()` con un commento
esplicito: `p.customPlace` vince sempre, **anche il valore `'none'`**, che serve proprio a
"azzerare esplicitamente" una foto di libreria che altrimenti erediterebbe il luogo della sua
cartella; se non c'è `customPlace`, le foto di libreria ereditano `PLACES[folderId]`; le foto
di un lotto di culling "appena importate, non hanno alcuna posizione finché l'utente non la
imposta a mano".

**Chip persona** (`[data-facechip]`, `.lb-tag-chip` con `role="button" tabindex="0"`) — click
o Invio/Spazio → `openFaceBoxMenuDialog(face)`, il dialog con le voci:

- **`Vai alla persona`** — *"Apre tutte le sue foto"* → chiude il lightbox e va a Persone;
- **`Correggi persona…`** — *"Questo volto appartiene a qualcun altro"* → apre il selettore
  di persona, poi toast **`"Persona corretta."`**;
- **`Non è un volto`** (in `danger`) — *"Falso positivo — non verrà mai più riproposto"* →
  toast **`"Segnato come \"non è un volto\" — non verrà più riproposto."`**;
- **`Annulla`**.

Il commento alle righe 4272–4275 spiega la scelta: la sezione Persone ha la stessa forma
della sezione Tag, "ma ogni chip apre il menu del volto (vai/correggi/non è un volto) invece
di confermare/rimuovere direttamente — è la stessa azione del riquadro sull'immagine, solo un
secondo punto d'ingresso per chi preferisce il pannello alla foto stessa".

**Chip `+ aggiungi` delle persone** (`#lbAddPersonChip`) — apre il selettore di persona (con
possibilità di creare una nuova persona digitando un nome), poi `addManualFaceToPhoto` e
toast **`"Persona aggiunta."`**. Il volto così creato ha `box:null` — commento alle righe
1829–1830: "nessun rilevamento automatico dietro: niente box, la foto non nasce da
un'analisi ma da una scelta umana". Conseguenza concreta: **quel nome non avrà mai un
riquadro sull'immagine**, l'aiuto visivo all'hover non funzionerà per lui.

**Chip tag** — tre stati distinti, descritti nel commento alle righe 4233–4236:

| Stato | Aspetto | Interazione |
|---|---|---|
| Applicato dall'IA, mai revisionato (`origin==='ai'`) | `.lb-tag-chip.ai-applied` → `opacity:.72`, più il marcatore testuale **`IA`** (`.lb-tag-ai-mark`, 9px grassetto, `opacity:.8`) con `title="Assegnato in automatico dall'IA — clicca per confermarlo"`; il chip ha `role="button" tabindex="0"` e `aria-label="Conferma tag {nome}, assegnato in automatico dall'IA"` | click / Invio / Spazio → **conferma** il tag (lo passa a `confirmed`/`human`), toast **`"Tag confermato."`** |
| Confermato da un umano | `.lb-tag-chip` piena (opacità 1), nessun marcatore, nessun `role` | non cliccabile (solo la `×`) |
| In attesa di conferma (suggerito) | `.lb-suggested-tag-chip` — bordo **tratteggiato** `1px dashed #3a3a3a`, testo `#b8b8bc`; sezione separata sotto l'etichetta `In attesa di conferma` | `✓` verde (`#6fd08a`) → conferma, toast **`"Tag confermato."`**; `×` rosso (`#ff8a80`) → rifiuta, toast **`"Suggerimento rifiutato — non verrà riproposto."`** (SP-10) |

La `×` di rimozione (`.lb-tag-x`, `role="button" tabindex="0"`,
`aria-label="Rimuovi tag {nome}"`) è presente su **tutti** i tag confermati; toast
**`"Tag rimosso."`**. Nota importante: rimuovere non cancella l'assegnazione, la porta a
stato `rejected` con origine `human` — commento alle righe 1495–1496: "rimuovere un tag da
una foto è a sua volta una decisione umana: deve restare permanente (altrimenti una rianalisi
potrebbe far ricomparire un tag che l'utente aveva tolto apposta)". Il raggruppamento per
categoria è motivato dal commento alle righe 4240–4242: stesso ordine della pagina "Tag e
categorie", "così nel dettaglio foto si vede anche la categoria di appartenenza, non solo
l'elenco piatto dei tag".

**Chip `+ aggiungi` dei tag** (`#lbAddTagChip`) → `openTagPickerDialog([p.id])`, dialog
"Aggiungi tag" con un interruttore per ogni tag.

**Chip `+ aggiungi` degli album** (`#lbAddAlbumChip`) → `openAlbumPickerDialog([p.id])`,
dialog "Aggiungi ad album" (solo album manuali; gli album dinamici sono esclusi dall'elenco).

**Sezione AZIONI** (`.lb-actions`, flex con `gap:8px` e `flex-wrap`) — cinque pulsanti
`.lb-action-btn` (12px, sfondo `#161616`, bordo `#262626`, `border-radius:8px`,
`padding:7px 10px`; hover `#1f1f1f`; nessun `role`/`tabindex`):

| Etichetta esatta | Icona | Cosa fa | Presente in culling? |
|---|---|---|---|
| **`Scarica originale`** | `download` 14px | `lbDownloadOriginal()` → **solo un toast** | Sì |
| **`Ruota`** | `rotate` 14px | `lbRotatePhoto()` → **solo un toast** | Sì |
| **`Aggiungi ad album`** | `album` 14px | `openAlbumPickerDialog([p.id])` | **No** |
| **`Rinomina…`** | `edit` 14px | `openRenameDialog({kind:'single', photos:[p], hasSubfolders:false})` → dialog "Rinomina con formula", sottotitolo `1 foto — {nome file}` | Sì |
| **`Elimina…`** | `trash` 14px, classe `danger` → colore `var(--danger)` | `openDeleteDialog(p, …)` → dialog a 3 opzioni ("Rimuovi solo dall'indice" / "Sposta nel cestino di Keeppix" / "Elimina dal disco adesso" / "Annulla"); scegliendo un'opzione: `p.pick='reject'; p.trashChoice=choice; closeLightbox()` | **No** |

**Azioni solo dimostrative (toast "Solo demo — …")** — testo **esatto**:

1. `Scarica originale` (sia il pulsante `#lbDownloadBtn` sia la voce `#lbMoreDownload` del
   menu ⋯) →
   **`Solo demo — scaricherebbe il file originale sul tuo dispositivo.`**
2. `Ruota` (sia `#lbRotateBtn` sia `#lbMoreRotate`) →
   **`Solo demo — ruoterebbe il file (l'originale sul disco non viene mai modificato).`**

Il commento alle righe 4102–4104 le identifica esattamente come tali: *"'Scarica'/'Ruota' —
compaiono sia nel pannello Azioni del lightbox sia nel menu ⋯: azioni che in un vero server
toccherebbero il file su disco, qui solo un toast, come per 'Esci' nel menu account."*
**Sono i punti in cui il backend dovrà fare qualcosa di vero.** Vi si aggiunge, come detto
sopra, il commutatore RAW/JPEG, che nel mockup non ha alcun effetto oltre l'evidenziazione
della chip. Il testo della seconda frase contiene già una decisione di prodotto da onorare
lato backend: la rotazione **non deve modificare il file originale sul disco** (quindi va
registrata come metadato/orientamento, non riscrivendo il file).

### 4. Interazioni da mouse

- Click su ognuno dei controlli sopra.
- **Hover sul chip di una persona** → dopo **0 ms** (nessun ritardo di apertura) compare il
  riquadro sul volto corrispondente sull'immagine; uscendo dal chip il riquadro sparisce
  dopo **200 ms** di tolleranza, che si annullano se nel frattempo si entra nel riquadro.
  Lo stesso vale per `focus`/`blur` da tastiera sul chip.
- Hover: `.lb-action-btn:hover{background:#1f1f1f}`, `.lb-tag-chip .lb-tag-x:hover{opacity:1}`
  (base `.6`), `.lb-suggested-tag-actions span:hover{background:#2c2c2c}` (base `#232323`).
- **Scroll**: il pannello scorre verticalmente (`overflow-y:auto`) — con tutte le sezioni
  presenti il contenuto supera facilmente l'altezza disponibile.
- Doppio click, tasto destro, trascinamento: **non previsti nel mockup** in nessun punto del
  pannello (nessun trascinamento di un tag su una foto, nessun menu contestuale sui chip).

### 5. Interazioni da tastiera

- Le scorciatoie globali del lightbox (`Esc`, `←`, `→`, `i`, `f`) restano attive anche
  mentre il focus è **dentro il pannello**, campo Titolo compreso — non c'è nessun controllo
  su `e.target` nel gestore (vedi ambiguità: è un difetto vero).
- Tab raggiunge, nell'ordine del DOM: `#lbTitleInput` → chip persona → chip tag applicati
  dall'IA → `×` di rimozione dei tag → `✓`/`×` dei tag in attesa. Tutto il resto del
  pannello (stelle, chip RAW/JPEG, mappa, pulsante posizione, chip "+ aggiungi", pulsanti
  Azioni) è **fuori dall'ordine di tabulazione**.
- Su tutti gli elementi con `role="button"` valgono Invio e Spazio (SP-8, `bindActivatable`);
  `focus-visible` disegna `outline:2.5px solid var(--accent)` con `outline-offset:2px`.
- Nel campo Titolo: **Invio** conferma (scatta `change` e quindi il salvataggio); **Esc**
  non annulla la modifica, **chiude il lightbox** (o il menu ⋯ se aperto).

### 6. Animazioni e transizioni

Nel pannello l'unica transizione è quella dei riquadri volto già descritta
(`opacity .12s ease`, `border-color .12s ease`, con nascondimento ritardato di 200 ms in JS).
Chip, pulsanti, stelle e commutatore RAW/JPEG **non hanno transizioni**: cambiano stato
istantaneamente. L'apertura e la chiusura del pannello (icona info o tasto `I`) sono
anch'esse istantanee: il pannello viene aggiunto o tolto dal DOM, senza scorrimento né
dissolvenza.

### 7. Stati per ogni controllo

- **Campo Titolo** — normale: sfondo `#161616`, bordo `#262626`, testo `#f0f0f0`; vuoto:
  mostra il placeholder `"Senza titolo"` in `#7a7a7d`; focus: `outline:2.5px solid
  var(--accent)`; **nessuno stato di errore, nessuno stato disabilitato, nessuno stato "sto
  salvando"**.
- **Stelle** — piena `var(--accent)`, vuota `var(--text-tertiary)`; nessun hover di
  anteprima, nessun focus, mai disabilitate.
- **Chip RAW/JPEG** — normale: sfondo `#1a1a1a`, testo `#9a9a9e`, bordo `#232323`; attiva:
  `background:var(--accent-tint); color:var(--accent); border-color:var(--accent)`. La chip
  `raw_only` è **sempre nello stato attivo e non reagisce al click** — non è "disabilitata"
  visivamente, semplicemente non ha un handler: al lettore appare cliccabile
  (`cursor:pointer` è nella regola base) ma non fa niente. Vedi ambiguità.
- **Sezione Posizione** — stato pieno (mappa + luogo + coordinate) e stato vuoto (frase in
  corsivo `"Nessuna posizione impostata."`); il pulsante cambia etichetta di conseguenza.
- **Chip tag** — normale (confermato), attenuato `opacity:.72` + marcatore `IA` (applicato
  dall'IA), tratteggiato (in attesa). `.lb-tag-x` a `opacity:.6`, `1` all'hover.
- **Chip album** — un solo stato: non cliccabili, nessun hover, nessuna distinzione visiva
  fra album manuale e album dinamico.
- **Pulsanti Azioni** — normale `#161616`; hover `#1f1f1f`; variante `danger` in
  `var(--danger)`; **mai disabilitati**. Non esiste uno stato "in corso" per Scarica/Ruota.
- **Sezioni vuote**: la sezione TAG viene comunque disegnata anche senza nessun tag (resta
  l'etichetta "TAG" e il solo chip "+ aggiungi"); idem PERSONE e ALBUM. Non c'è nessun testo
  del tipo "nessun tag" — l'unico stato vuoto esplicito del pannello è quello della
  posizione.

### 8. Da dove ci si arriva e dove si va

Il pannello esiste solo dentro il lightbox. Si apre/chiude con l'icona info o con `I`, ed è
**forzato aperto** a ogni `openLightbox()` (e all'apertura dal culling). Da qui si esce
verso: la vista Foto di una cartella o la vista Culling di un lotto (link nel sottotitolo),
la vista Mappa (click sulla mini-mappa, ma senza chiudere il lightbox — vedi ambiguità), la
vista Persone ("Vai alla persona"), o verso uno dei dialog modali (Posizione, Persona, Menu
volto, Tag, Album, Rinomina, Elimina), tutti conformi a SP-5 e tutti restituiscono il focus
al trigger alla chiusura.

### 9. Dati necessari a questa schermata

Oltre a quanto già elencato al punto 9 della sezione 1, il pannello ha bisogno di:
l'elenco delle categorie di tag col loro ordine e i tag che vi appartengono; per ogni
coppia tag-foto lo **stato** (confermato / suggerito / rifiutato) e la **provenienza** (IA o
umana); per ogni volto sulla foto la persona associata, il suo nome o il suo numero
automatico, e il riquadro se esiste; l'elenco degli album manuali con i loro membri e degli
album dinamici con le loro condizioni; l'elenco dei luoghi noti alla libreria (per il dialog
di posizione) e delle persone note (per il selettore di persona); l'interruttore globale
"riconoscimento volti attivo".

---

## 20. Menu "altre azioni" (⋯)

### 1. Nome e scopo

Menu a comparsa ancorato al pulsante ⋯ della barra superiore, che raccoglie le azioni sul
file — le stesse della sezione AZIONI del pannello informazioni — così da restare
raggiungibili anche a pannello chiuso.

### 2. Cosa mostra

Contenitore `.lb-more-menu` (`id="lbMoreMenu"`), reso **dentro** il pulsante ⋯ (che ha
`position:relative` proprio per fargli da riferimento): `position:absolute; top:calc(100% +
6px); right:0; width:200px; background:var(--card-bg); border:1px solid var(--border-strong);
border-radius:10px; box-shadow:0 8px 24px rgba(0,0,0,.3); padding:6px; z-index:20;
color:var(--text); text-align:left`.

Nota di stile importante: il menu usa i **colori del tema** (`--card-bg`, `--text`), non la
palette nera fissa del resto del lightbox — quindi in tema chiaro è un riquadro chiaro sopra
un lightbox nero. È voluto: il commento alle righe 497–499 dice "stessa struttura del menu
account (`.user-menu`/`-item`/`-sep`), solo ancorato in basso a destra invece che in alto a
sinistra — il bottone ⋯ che lo apre è `position:relative` apposta per fargli da riferimento".

Voci, nell'ordine, tutte `.user-menu-item` (`display:flex; gap:9px; padding:8px 9px;
border-radius:7px; font-size:13px`):

| # | Etichetta esatta | Icona | Azione | In culling |
|---|---|---|---|---|
| 1 | **`Scarica originale`** | `download` 15px | toast **`Solo demo — scaricherebbe il file originale sul tuo dispositivo.`** | presente |
| 2 | **`Ruota`** | `rotate` 15px | toast **`Solo demo — ruoterebbe il file (l'originale sul disco non viene mai modificato).`** | presente |
| 3 | **`Aggiungi ad album`** | `album` 15px | `openAlbumPickerDialog([p.id])` | **omessa** |
| 4 | **`Rinomina…`** | `edit` 15px | `openRenameDialog({kind:'single', photos:[p], hasSubfolders:false})` | presente |
| — | *separatore* `.user-menu-sep` (1px, `var(--border)`, `margin:5px 2px`) | | | **omesso insieme alla voce 5** |
| 5 | **`Elimina…`** | `trash` 15px, classe `danger` → `var(--danger)` | `openDeleteDialog(p, …)`; se si conferma un'opzione: `p.pick='reject'; p.trashChoice=choice; closeLightbox()` | **omessa** |

**Regola: in contesto culling spariscono "Aggiungi ad album" e "Elimina…" (col suo
separatore).** Il ternario è `${isCulling ? '' : …}` in entrambi i casi, con
`isCulling = !!p.batchId`. **Il perché** è nel commento alle righe 4287–4290 di
`lbInfoPanelHTML`: *"in culling `p` può non avere `folderId`/album/tag reali (è una foto
'grezza', non ancora organizzata) — il pannello si adatta: niente sezione Tag/Album, niente
'Aggiungi ad album'/'Elimina…' (in culling si usa Scelta/Scarta, non il cestino), breadcrumb
verso il lotto invece che verso la cartella, posizione via `photoPlace()` (può essere
assente)."* In altre parole: una foto di un lotto non è ancora entrata nella libreria, quindi
non ha senso metterla in un album; e per liberarsene esiste già il meccanismo proprio del
culling — "Scarta", che sposta fisicamente il file in `_scartati` e viene poi svuotato con
"Svuota scartati" — non il cestino della libreria.

### 3. Ogni controllo, uno per uno

- **`#lbMoreBtn`** (già descritto nella sezione 1): interruttore aperto/chiuso del menu.
  `bindActivatable` → click, Invio e Spazio; fa `stopPropagation()` per non essere chiuso
  subito dal listener globale di click.
- **Le 5 voci sopra**: `<div>` senza `role`, senza `tabindex`, con solo `onclick`. Ogni
  handler passa da `closeMoreAnd(fn)`, che chiude il menu e ridisegna **prima** di eseguire
  l'azione, così il dialog che si aprirà non trova il menu ancora sotto di sé.
- `moreMenu.onclick = e => e.stopPropagation()` — un click **dentro** il pannello del menu
  non lo chiude.

### 4. Interazioni da mouse

- Click sul ⋯ → apre/chiude.
- Click su una voce → chiude il menu ed esegue.
- **Click ovunque fuori** → chiude il menu, grazie al listener globale di riga 2289:
  `document.addEventListener('click', ()=>{ if(state.lbMoreOpen){ state.lbMoreOpen=false;
  renderAll(); } })`. Vale anche per un click sullo sfondo nero del lightbox, che di per sé
  non fa nient'altro.
- Hover sulle voci: `.user-menu-item:hover{background:var(--chip-bg)}`, senza transizione.
- Tasto destro, doppio click, trascinamento: non previsti.

### 5. Interazioni da tastiera

- Il menu si **apre** da tastiera (Invio/Spazio sul ⋯, che è `role="button" tabindex="0"`).
- **Le voci non sono raggiungibili da tastiera**: nessun `tabindex`, nessun `role="menu"` /
  `role="menuitem"`, nessuna navigazione con `↑`/`↓`, nessun focus automatico sulla prima
  voce all'apertura. Deviazione da SP-14, che va segnalata al frontend.
- Peggio: mentre il menu è aperto, `←` e `→` **cambiano foto** (il gestore del lightbox non
  sa che c'è un menu aperto), lasciando il menu aperto sulla foto successiva.
- **`Esc` a due livelli**: la prima pressione chiude solo il menu; la seconda chiude il
  lightbox. È l'unico posto dell'app con un Esc a due stadi, ed è commentato esplicitamente
  (riga 6291).
- `aria-expanded` sul pulsante ⋯ riflette correttamente lo stato.

### 6. Animazioni e transizioni

Nessuna: il menu appare e scompare senza dissolvenza, scala o scorrimento (`.lb-more-menu`
non ha `transition` né `animation`). L'unico effetto visivo è l'ombra fissa
`0 8px 24px rgba(0,0,0,.3)`. Anche l'hover sulle voci è istantaneo.

### 7. Stati per ogni controllo

- **Pulsante ⋯**: normale / hover (`rgba(255,255,255,.12)`) / focus-visible (outline accento
  2.5px) / `aria-expanded="true"` a menu aperto. Non riceve la classe `.active`: **non c'è
  nessun segnale di colore che il menu sia aperto**, solo la presenza del pannello.
- **Voci**: normale (colore `var(--text)`) / hover (`var(--chip-bg)`) / `danger`
  (`var(--danger)`, solo "Elimina…"). **Nessuna voce viene mai disabilitata**: le voci non
  applicabili sono rimosse dal markup, non spente.
- **Stato vuoto**: non possibile — in culling restano comunque 3 voci.

### 8. Da dove ci si arriva e dove si va

Si arriva solo dal pulsante ⋯ del lightbox. Si va verso: un toast (Scarica / Ruota, che
lasciano tutto com'è), il dialog "Aggiungi ad album", il dialog "Rinomina con formula", il
dialog di eliminazione a 3 opzioni (che, se confermato, **chiude anche il lightbox**).

### 9. Dati necessari a questa schermata

Solo l'identificativo e il nome del file della foto corrente, e il fatto che appartenga o
meno a un lotto di culling (per decidere quali voci mostrare). I dialog che apre hanno
bisogno, rispettivamente, dell'elenco degli album manuali con i loro membri e dell'elenco dei
segnaposto di rinomina.

---

## 21. Differenze fra lightbox aperto da libreria e lightbox aperto da un lotto di culling

### 1. Nome e scopo

Lo stesso componente serve due contesti diversi: una foto **già organizzata** nella libreria
(cartella, tag, album, volti) e una foto **grezza** appena importata dentro un lotto di
culling; il discriminante è una sola proprietà, `p.batchId`, letta come
`const isCulling = !!p.batchId`.

### 2. Cosa mostra — tabella delle differenze

| Aspetto | Da libreria | Da un lotto di culling |
|---|---|---|
| **Come ci si entra** | tile di una griglia (click / Invio / Spazio / tap), oppure riga file della pagina "Problemi" — passando da `openLightbox(id)` | pulsante rotondo sull'immagine dello stage, `#cullInfoBtn`, `aria-label="Dettagli foto — EXIF, posizione, rinomina"` — che imposta `state.lightbox` **direttamente**, perché `openLightbox()` cerca in `allPhotos()` e non troverebbe mai una foto di lotto |
| **Insieme di navigazione (frecce + filmino)** | `photosFor(p.folderId).filter(x => x.monthOffset === p.monthOffset)` — **tutte le foto della stessa cartella e dello stesso mese** | `cullingQueue()` — le foto del lotto aperto, **filtrate dal filtro attivo** (`all` / `todo` / `taken` / `skipped`) |
| **Ricerca della foto per id** (click su una miniatura) | `allPhotos()` — tutta la libreria | `cullingPhotosFor(p.batchId)` — **tutto il lotto**, anche le foto escluse dal filtro attivo |
| **Sottotitolo (breadcrumb)** | `"{data}, ore {ora} · {nome cartella}"`, il nome della cartella è un link alla vista Foto su quella cartella | `"{data}, ore {ora} · {nome lotto} · {stato}"`, il nome del lotto è un link alla vista Culling su quel lotto; lo stato è `"Presi"` / `"Scartati"` / `"Da valutare"`. Se il lotto non viene trovato, il testo di ripiego è `"Culling"` |
| **Riquadri volto sull'immagine** | presenti, se il riconoscimento volti è attivo | **assenti** (condizione `!p.batchId && state.faceRecognitionEnabled`) |
| **Sezione PERSONE** | presente, se il riconoscimento volti è attivo | **assente** (`isCulling \|\| !state.faceRecognitionEnabled`) |
| **Sezione TAG** | presente | **assente** |
| **Sezione ALBUM** | presente | **assente** |
| **Azione `Aggiungi ad album`** (pannello e menu ⋯) | presente | **assente** |
| **Azione `Elimina…`** (pannello e menu ⋯, col separatore) | presente | **assente** — in culling si usa Scelta/Scarta, non il cestino |
| **Posizione** | ereditata dal luogo della cartella se non impostata sulla foto | nessuna finché non la si imposta a mano → si vede `"Nessuna posizione impostata."` e il pulsante dice `"Imposta posizione…"` |
| **Cosa resta identico** | \- | nome file, data/ora, campo Titolo, stelle, commutatore RAW/JPEG, sezione SCATTO, sezione POSIZIONE, filmino, frecce, barra superiore **completa** (cuore, condividi, info, ⋯), `Scarica originale`, `Ruota`, `Rinomina…` |

Il motivo di tutto questo è dichiarato una volta sola, nel commento in testa a
`lbInfoPanelHTML` (righe 4287–4290) già citato nella sezione 3, e ribadito in
`lbNeighborList` (righe 4067–4069): *"dentro un lotto di culling il 'filmino vicini' del
lightbox è la coda di culling corrente (rispetta il filtro attivo), non un mese di una
cartella di libreria — la foto potrebbe non avere nemmeno un `folderId`/`monthOffset`."*

### 3. Ogni controllo, uno per uno

Vedi le sezioni 1–3: l'inventario dei controlli è lo stesso, meno le due azioni e le tre
sezioni omesse in culling elencate nella tabella. Nessun controllo **aggiuntivo** compare in
culling: in particolare **"Scelta"/"Scarta" non sono disponibili dal lightbox** — per
decidere bisogna chiuderlo e tornare allo stage del lotto.

### 4. Interazioni da mouse

Identiche nei due contesti. Unica differenza sostanziale: cliccando una miniatura del filmino
in culling si può arrivare a una foto **che sarebbe fuori dal filtro corrente** solo se la
lista era già filtrata (il filmino mostra `cullingQueue()`, quindi già filtrata) — mentre
`lbLookup` cerca nell'intero lotto, cosa che conta solo se l'id proviene da altrove.

### 5. Interazioni da tastiera

Identiche (`Esc`, `←`, `→`, `i`, `f`). Importante per il culling: finché il lightbox è
aperto, il `return` a riga 6298 **sopprime tutte le scorciatoie del culling** — `1`–`5` per
le stelle, `P` per "Scelta", `X` per "Scarta", `shift+←/→` per la selezione a intervallo non
funzionano. Il suggerimento a schermo dello stage
(`"← → naviga · shift+← → seleziona intervallo · 1-5 rating · P scegli · X scarta"`) resta
però visibile sotto il lightbox se lo si chiude, quindi non c'è contraddizione visiva; ma
l'utente che apre i dettagli per decidere non può decidere da lì.

### 6. Animazioni e transizioni

Nessuna differenza: l'apertura dallo stage del culling è istantanea esattamente come
l'apertura da una griglia.

### 7. Stati per ogni controllo

Le voci non applicabili in culling non sono **disabilitate**: sono **rimosse dal markup**.
Il lettore non vede quindi mai un "Elimina…" grigio in culling — semplicemente non c'è.
Il cuore (`#lbFav`) è invece un caso limite: nel generatore delle foto di culling
(`genCullingPhotos`) **non esiste il campo `isFav`**, quindi il pulsante parte spento e al
primo click imposta `isFav = true` su un oggetto che nessun'altra vista del culling legge —
uno stato che si scrive e non si vede più. Vedi ambiguità.

### 8. Da dove ci si arriva e dove si va

Chiudendo il lightbox si torna sempre esattamente alla vista da cui si era partiti, perché
`state.view` non viene mai toccato dall'apertura: dalla griglia si torna alla griglia, dallo
stage del culling si torna allo stage con la stessa foto ancora al centro.

### 9. Dati necessari a questa schermata

Rispetto all'elenco della sezione 1, il contesto culling richiede in più: l'appartenenza
della foto a un lotto (con nome del lotto), lo stato della foto nel lotto (da valutare /
presa / scartata) e il filtro attivo sul lotto (perché determina l'insieme su cui navigano le
frecce). Non richiede invece: cartella, mese, tag, album, volti. Il contesto libreria
richiede in più: cartella con il suo nome e il suo luogo, mese di appartenenza (per il
vicinato), tag, album e volti.

---

## 22. Note su mobile (`#app.device-mobile`)

- `.lb-body{flex-direction:column}` — il pannello informazioni passa **sotto** l'immagine.
- `.lb-info{width:100%; flex:0 0 auto; max-height:58%; border-left:none; border-top:1px solid
  #232323; border-radius:16px 16px 0 0}` — diventa un foglio inferiore.
- `.lb-stage{padding:8px 14px}` — spariscono i 60px laterali riservati alle frecce…
- `.lb-arrow{display:none}` — …perché **su mobile le frecce non ci sono**. Restano quindi
  solo il filmino e la tastiera per cambiare foto: **non c'è swipe**, non implementato nel
  mockup. È il vuoto di interazione più evidente della versione mobile del lightbox.
- I tooltip `[data-tip]` sono comunque assenti nel lightbox (SP-7 non usato qui), quindi non
  c'è la solita perdita di etichette su mobile.

---

# Parte V — Ricerca, mappa, condivisione

> Tutto ciò che segue è letto dal mockup `/home/claude/keeppix/index.html`. Dove una cosa
> **non** è implementata è scritto esplicitamente. I pattern condivisi sono richiamati per
> codice (SP-1 … SP-17) e non ridescritti.

---

## 23. Cerca — la barra e i suggerimenti

### 1. Nome e scopo

Un unico campo che accetta insieme testo libero "semantico" e filtri strutturati, e un pannello
di suggerimenti che trasforma il testo digitato in filtri riconosciuti.

Il commento a codice (righe 4353–4363) spiega il perché della scelta, ed è importante per chi
implementa: *«La sfida: un solo campo che gestisca testo libero semantico, filtri strutturati
(anno, fotocamera, ISO, cartella, GPS) e tag — e che li combini — senza sembrare un pannello da
database. Soluzione: si scrive come una frase. Ciò che viene riconosciuto come "strutturato"
(scegliendolo da un suggerimento) diventa una pillola rimovibile a sinistra nel campo; quello che
resta scritto è testo, ed è quello a cui viene applicata la ricerca per descrizione libera […]
Le due cose convivono nello stesso campo e si sommano nei risultati.»*

### 2. Cosa mostra

Il **composer** (`.search-composer`, un riquadro con bordo e sfondo `--chip-bg`, `border-radius:10px`,
padding `9px 10px`), da sinistra a destra:

- icona lente (`.search-composer-icon`, 16px, colore `--text-tertiary`);
- le **pillole** già aggiunte (`#searchPillsHost`, vedi sezione successiva);
- il **campo di testo** `#cercaInput`, `font-size:14px`, `min-width:160px`, `autocomplete="off"`,
  con placeholder **esatto**:
  `"Descrivi cosa cerchi, o scegli un tag, una fotocamera, una cartella…"`;
- il pulsante tondo **✕ "Cancella la ricerca"** (`#searchClearAll`, `aria-label="Cancella la ricerca"`,
  24×24px, sempre presente, anche a campo vuoto);
- il pannello dei suggerimenti `#searchSuggest`, in posizione assoluta sotto il composer.

Il **pannello dei suggerimenti** (`.search-suggest-panel`: sfondo `--card-bg`, bordo, ombra
`0 10px 26px rgba(0,0,0,.18)`, `max-height:320px` con scroll interno, `z-index:15`) mostra i
suggerimenti raggruppati per categoria. Ogni gruppo ha un'etichetta maiuscoletta
(`.search-suggest-group-label`, 10.5px, `text-transform:uppercase`, `letter-spacing:.06em`), e sotto
le righe (`.search-suggest-row`, 13px).

**Le categorie di suggerimento, nell'ordine esatto in cui compaiono** (`buildSearchSuggestions`,
righe 4383–4413):

| Gruppo | Etichetta | Cosa contiene | Limite | Icona / pallino |
|---|---|---|---|---|
| Tag | `"Tag"` | tag il cui nome contiene il testo digitato | max 5 | pallino colorato `hsl(<colore tag>,60%,50%)` |
| Fotocamera | `"Fotocamera"` | modelli di fotocamera distinti che contengono il testo | max 4 | icona `photo` |
| Cartella | `"Cartella"` | cartelle il cui nome contiene il testo | nessuno (3 in totale) | icona `folder` |
| ISO | `"ISO"` | una sola riga `"ISO <n>"` se nel testo c'è un numero di 2–4 cifre che è esattamente uno dei valori 100/200/400/800/1600 | 1 | icona `settings` |
| Anno | `"Anno"` | una riga `"2026"` se il testo (≥2 caratteri) è una sottostringa di "2026" | 1 | icona `search` |
| Posizione | `"Posizione"` | una riga `"Ha coordinate GPS"` se il testo è sottostringa di "gps" | 1 | icona `locate` |
| Paese | `"Paese"` | una riga `"Italia"` se il testo è sottostringa di "italia" | 1 | icona `globe` |

Un suggerimento già usato come pillola **non viene più proposto** (funzione `has(type,value)`).

**A campo vuoto ma con focus** i gruppi sono solo due: `"Tag"` (i primi 5 tag della libreria:
Paesaggi, Fauna selvatica, Tramonti, Architettura, Vicoli e centro storico) e `"Cartella"` (tutte
e tre le cartelle). Il commento a codice, riga 4409, spiega il perché:
*«campo vuoto ma con focus: incoraggia la scoperta con qualche cartella oltre ai tag»*.

**Riga finale "testo libero"** (`.search-suggest-free`, separata da un bordo superiore): se c'è del
testo digitato compare sempre in fondo al pannello la riga
`"Come descrizione libera: «<testo>»"` — con il testo in grassetto. **È solo informativa: non ha
alcun gestore di click**, serve a far capire che quello che si sta scrivendo, se non lo si sceglie
come filtro, viene comunque usato come descrizione.

**Messaggio del pannello vuoto** (`.search-suggest-empty`, centrato, 12.5px): compare solo quando
non c'è testo digitato **e** non c'è nessun gruppo da mostrare (cioè quando tutti i tag e tutte le
cartelle proponibili sono già diventati pillole — caso di frontiera):
`"Scrivi per cercare per tag, fotocamera, cartella, ISO… oppure descrivi la foto («tramonto con casa»)."`
Se invece c'è testo digitato ma nessun gruppo corrisponde, il pannello contiene **solo** la riga
"Come descrizione libera".

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `#cercaInput` | campo di testo | Scrive in `state.cercaQuery` a ogni carattere, apre il pannello suggerimenti e **ricalcola i risultati in tempo reale**. Nessuna validazione, nessun minimo di caratteri, nessun debounce. Lasciato vuoto: nessun errore, semplicemente non c'è filtro testuale. |
| 2 | Riga di suggerimento (`.search-suggest-row`) | riga cliccabile | Aggiunge la pillola corrispondente, **svuota il campo di testo**, chiude il pannello, ricalcola i risultati e rimette il focus nel campo. |
| 3 | ✕ della pillola (`.search-pill-x`) | pulsante (`role="button"`, `tabindex="0"`, `aria-label="Rimuovi filtro <etichetta>"`) | Rimuove quella pillola e ricalcola i risultati. |
| 4 | `#searchClearAll` ✕ | pulsante | Azzera **tutte** le pillole, il testo e chiude il pannello (`renderAll()`). **Non** azzera il chip del tipo file (`state.cercaType`). |
| 5 | Chip `"Tutti i tipi"` | chip filtro | `state.cercaType='all'` |
| 6 | Chip `"RAW"` | chip filtro | Solo foto con `isRaw` |
| 7 | Chip `"JPEG"` | chip filtro | Solo foto senza `isRaw` |
| 8 | Chip `"Preferiti"` | chip filtro | Solo foto con `isFav` |
| 9 | Chip `"Persona"` | chip **disabilitato** | Nessun gestore di click. Ha un `title` HTML nativo: `"Richiede riconoscimento volti — vedi Gruppo B"`. Non è un `[data-tip]` (SP-7): è il tooltip del browser, quindi appare col ritardo di sistema, non con i .12s del tooltip di Keeppix. |

I chip 5–8 sono mutuamente esclusivi (uno solo `.active` alla volta) e **si combinano in AND** con
pillole e testo. Nel mockup **non è possibile deselezionare** un chip riportandolo a "nessuno":
si passa da un chip all'altro, e `"Tutti i tipi"` è il default.

### 4. Interazioni da mouse

- **Click nel campo** (`onfocus`): apre il pannello dei suggerimenti.
- **Click su una riga di suggerimento**: aggiunge la pillola (vedi sopra).
- **Click sul ✕ di una pillola**: la rimuove.
- **Click fuori dal composer**: chiude il pannello. È un listener a livello di `document`
  (riga 4504), attivo solo se `state.view==='cerca'`. Commento a codice: *«chiude il pannello
  suggerimenti cliccando fuori dal composer — stesso schema del picklist "Cartella" nella
  creazione album»* (SP-14).
- **Hover su una riga di suggerimento**: sfondo `--chip-bg`, senza ritardo.
- **Hover sul ✕ di una pillola**: opacità da `.65` a `1` e alone `rgba(0,0,0,.08)`.
- **Hover sul ✕ generale**: sfondo `--chip-bg-hover`, colore da `--text-tertiary` a `--text`.
- **Doppio click, tasto destro, trascinamento, rotellina**: non previsti nel mockup. Non esiste un
  menu contestuale sulla barra di ricerca né sui suggerimenti. Il pannello scorre solo se supera i
  320px di altezza (scroll nativo).

### 5. Interazioni da tastiera

Questo è il punto in cui il mockup è più povero di quanto la UI lasci intendere. Dettaglio onesto:

- **Digitazione**: aggiorna ricerca e suggerimenti a ogni carattere.
- **Esc**: chiude il pannello dei suggerimenti (gestore globale, riga 6312). **Non** cancella il
  testo e **non** rimuove pillole. Nella catena globale dei tasti Esc questa è l'ultima delle
  quattro chiusure (lightbox → ricerca regioni → picklist → pannello filtro → suggerimenti).
- **Invio con testo libero**: **non fa nulla di speciale — non esiste alcun gestore `keydown` su
  `#cercaInput`**. Non c'è form, quindi non c'è nemmeno un submit. La ricerca è già stata applicata
  mentre si digitava, quindi visivamente "non succede niente"; in particolare **Invio non promuove
  il testo a pillola** e non seleziona il primo suggerimento.
- **Backspace su campo vuoto**: **non rimuove l'ultima pillola** — comportamento non implementato.
- **Frecce ↑/↓ nell'elenco dei suggerimenti**: **non implementate**. Le righe `.search-suggest-row`
  non hanno `tabindex` né `role`, quindi **non sono nemmeno raggiungibili con Tab**: nel mockup i
  suggerimenti sono accessibili solo col mouse. È una lacuna di accessibilità da colmare
  nell'implementazione Vue.
- **Tab / Shift+Tab**: l'ordine di focus dentro il composer è: campo di testo → i ✕ delle pillole
  (che sono `role="button" tabindex="0"`, ma nel DOM stanno **prima** del campo, quindi in realtà si
  incontrano prima) → ✕ "Cancella la ricerca" → chip dei tipi (`.chip` è nella lista dei
  `:focus-visible`, ma i chip **non hanno `tabindex`**, quindi non sono focalizzabili da tastiera
  — altra lacuna).
- **Invio / Spazio** sul ✕ di una pillola e sul ✕ generale: equivalgono al click (SP-8).
- **Modificatori (Cmd/Ctrl, Shift, Alt)**: nessun uso specifico nella barra di ricerca.

### 6. Animazioni e transizioni

- Non esiste **nessuna animazione di comparsa** del pannello dei suggerimenti: appare e scompare di
  colpo (`innerHTML` sostituito). Non previsto nel mockup.
- Le pillole compaiono e scompaiono senza transizione.
- L'unica transizione che tocca questi elementi è la regola globale
  `#app *{transition:background-color .2s ease,border-color .2s ease,color .2s ease;}` (riga 86):
  quindi gli hover di righe e pulsanti sfumano in `.2s ease`, ed è anche ciò che rende morbido il
  cambio tema.
- Il tooltip icon-only (SP-7) usa `opacity .12s ease, transform .12s ease` — ma sulla barra di
  ricerca **non è usato**; l'unico tooltip presente è quello nativo del chip "Persona".

Cosa comunicano: la sfumatura dello sfondo su hover segnala "questa riga è cliccabile"; la
sostituzione istantanea del pannello comunica che il contenuto è ricalcolato, non filtrato.

### 7. Stati per ogni controllo

- **Campo di testo**: normale (placeholder in `--text-tertiary`); con focus (`outline:2.5px solid var(--accent)` con `outline-offset:2px`, regola globale di `:focus-visible`); con testo. Nessuno stato disabilitato, di errore o di caricamento — la ricerca è sincrona.
- **Pannello suggerimenti**: chiuso (`innerHTML` vuoto); aperto con gruppi; aperto con solo la riga "Come descrizione libera"; aperto con il messaggio di invito (`.search-suggest-empty`).
- **Riga di suggerimento**: normale / hover. Nessuno stato "selezionato con la tastiera", coerentemente con l'assenza di navigazione a frecce.
- **Pillola**: unico stato, `background:var(--accent-tint)`, testo `var(--accent)`, `font-weight:600`, `border-radius:14px`. Il ✕ ha normale (`opacity:.65`), hover/`:focus-visible`.
- **✕ "Cancella la ricerca"**: sempre attivo, **anche quando non c'è nulla da cancellare** — non ha stato disabilitato. Cliccarlo a vuoto ridisegna la pagina senza effetti visibili.
- **Chip dei tipi**: normale; hover (`--chip-bg-hover`); `.active` (`--accent-tint`, testo accento, bordo accento, `font-weight:600`); `.disabled` (solo "Persona": `opacity:.5`, `cursor:default`).

### 8. Da dove ci si arriva e dove si va

**In ingresso:**
- voce di sidebar **"Cerca"** (icona lente, `NAV_TOP`);
- campo `#topSearch` della topbar desktop, in **sola lettura**, con placeholder
  `"Cerca per data, luogo, persona…"` — cliccandolo si va alla vista Cerca e il focus viene messo
  su `#cercaInput` con un `setTimeout(…,0)` (riga 2340). *Nota: il placeholder della topbar promette
  "data, luogo, persona", tre dimensioni che i suggerimenti reali non offrono — vedi Ambiguità.*
- tab **"Cerca"** della tab bar mobile (SP-17).

**In uscita:** aprire una foto dalla griglia dei risultati (lightbox), cliccare una card di
cartella (→ vista Foto su quella cartella), cliccare una ricerca salvata (resta su Cerca), o
qualsiasi voce di navigazione.

### 9. Dati necessari a questa schermata

**Legge:**
- elenco dei **tag** con nome e colore (per i suggerimenti e il pallino della pillola);
- elenco dei **modelli di fotocamera** distinti presenti in libreria;
- elenco delle **cartelle** con id e nome;
- l'insieme dei **valori ISO** proponibili (nel mockup una costante `[100,200,400,800,1600]`);
- l'**anno** e i **paesi** presenti (nel mockup entrambi cablati: "2026" e "Italia").

**Scrive:** solo stato di interfaccia — il testo digitato, l'elenco delle pillole attive, se il
pannello è aperto. Nulla viene salvato sulle foto.

---

## 24. Cerca — i filtri strutturati (le "pillole" nella barra)

### 1. Nome e scopo

Rappresentazione visiva, dentro la barra stessa, di ogni filtro strutturato che l'utente ha
riconosciuto e "bloccato", così che la barra si legga come una frase invece che come un pannello.

### 2. Cosa mostra

Ogni pillola (`.search-pill`) mostra, in ordine:

1. per le sole pillole di tipo **tag**, un pallino colorato del colore del tag
   (`hsl(<colore>,60%,85%)` — notare: **luminosità 85%**, più chiara del pallino usato nel pannello
   dei suggerimenti che è al 50%);
2. l'**etichetta**, costruita da `pillLabel()` (righe 4367–4376) — questi sono i testi esatti:

| Tipo | Etichetta mostrata |
|---|---|
| `tag` | `"Tag: <nome del tag>"` — es. `"Tag: Tramonti"` |
| `camera` | il modello nudo, es. `"Sony A7 IV"` (senza prefisso) |
| `folder` | il nome della cartella nudo, es. `"Lago di Braies"` |
| `iso` | `"ISO 400"` |
| `year` | `"Anno 2026"` |
| `gps` | `"Con posizione GPS"` — **attenzione: nel pannello dei suggerimenti la stessa cosa si chiama `"Ha coordinate GPS"`**, il testo cambia una volta diventata pillola |
| `country` | `"Italia"` |

3. il **✕ di rimozione** (`.search-pill-x`).

Le pillole sono renderizzate in `#searchPillsHost`, che ha `display:contents`: non è un contenitore
visivo, le pillole diventano figli diretti del composer in `flex-wrap`, così si dispongono in linea
col campo di testo e vanno a capo insieme a lui.

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Corpo della pillola | etichetta statica | **Non cliccabile**: non apre nessun menu, non permette di modificare il valore. Per cambiarlo si rimuove e si riaggiunge. |
| 2 | ✕ della pillola | pulsante accessibile | Rimuove la pillola all'indice indicato e ricalcola i risultati. |

**Come si aggiunge una pillola:** unicamente cliccando una riga del pannello dei suggerimenti.
`addSearchPill(type,value)` (righe 4377–4382): se una pillola con lo stesso tipo **e** lo stesso
valore esiste già, non fa nulla (nessun duplicato, nessun avviso); altrimenti la aggiunge in coda,
**azzera `state.cercaQuery`** e chiude il pannello. Non esiste alcun altro modo di crearla — né
digitando e premendo Invio, né incollando testo, né da altre schermate.

**Come si rimuove:** click (o Invio/Spazio, SP-8) sul suo ✕, oppure ✕ "Cancella la ricerca" che le
toglie tutte in un colpo. **Backspace non rimuove nulla.**

**Come si combinano:** in `computeSearchResults()` (righe 4573–4597) le pillole sono applicate in
**AND** tra loro e in AND con il chip del tipo file e col testo libero. Dentro lo stesso tipo non
c'è OR: due pillole `folder` diverse darebbero zero risultati.

Effetto reale di ciascun tipo sui risultati:

- `tag` → tiene solo le foto per cui la coppia (tag, foto) esiste con `status === 'confirmed'`
  (quindi **esclude i suggerimenti IA non ancora confermati**, SP-10/SP-12);
- `camera` → confronto esatto sul modello;
- `folder` → confronto esatto sull'id cartella;
- `iso` → confronto esatto sul valore numerico;
- `year`, `gps`, `country` → **non filtrano nulla**. Il commento a codice (righe 4583–4585) è
  esplicito ed è un'informazione importante per l'architetto: *«year/gps/country: nel catalogo demo
  valgono per tutte le foto (un anno solo, tutto in Italia, tutto geolocalizzato) — la pillola resta
  comunque applicabile e combinabile, pronta per quando il catalogo reale avrà più varietà.»*

### 4. Interazioni da mouse

Click sul ✕ = rimozione. Hover sul ✕ = opacità piena + alone scuro. Nessun trascinamento per
riordinare, nessun tasto destro, nessun doppio click: non previsti nel mockup.

### 5. Interazioni da tastiera

Il ✕ è `tabindex="0"` con `role="button"` e `aria-label="Rimuovi filtro <etichetta>"`: Invio e
Spazio lo attivano (SP-8), con `preventDefault()` — così lo Spazio non fa scorrere la pagina.
Dopo la rimozione **il focus non viene riposizionato** (l'elemento che lo aveva è stato distrutto
dal `innerHTML`): il focus torna al `<body>`. Da correggere in Vue.

### 6. Animazioni e transizioni

Nessuna animazione di entrata/uscita. Solo le transizioni globali di colore (`.2s ease`) su hover.

### 7. Stati per ogni controllo

Pillola: stato unico (non esiste "disattivata temporaneamente" né "in errore"). ✕: normale
(`opacity:.65`) / hover (`opacity:1` + sfondo) / focus visibile (outline accento 2.5px).
Stato vuoto: nessuna pillola → `#searchPillsHost` non produce nulla e il campo di testo occupa
tutta la barra.

### 8. Da dove ci si arriva e dove si va

Le pillole vivono solo dentro la vista Cerca. Non sopravvivono al passaggio a un'altra vista in
modo esplicito (restano in `state.searchPills`, quindi tornando su Cerca sono ancora lì), ma
vengono **azzerate** cliccando una ricerca salvata (`state.searchPills=[]`, riga 4549) — il che è
incoerente, vedi Ambiguità.

### 9. Dati necessari a questa schermata

Per ogni pillola servono: tipo, valore, etichetta leggibile e (per i tag) il colore. Per applicarle
servono sulle foto: id cartella, modello fotocamera, ISO, stato dei tag con provenienza, e — nel
sistema reale — anno di scatto, presenza di coordinate GPS e paese.

---

## 25. Cerca — l'area dei risultati

### 1. Nome e scopo

La parte sotto la barra: prima di aver cercato è una pagina di scoperta; dopo aver cercato è
l'elenco dei risultati con il riepilogo di cosa è stato chiesto.

### 2. Cosa mostra

**Stato iniziale — nessun testo e nessuna pillola** (`hasSearch === false`):

1. Titolo `"Ricerche salvate"` con sottotitolo
   `"Raccolte \"vive\": si aggiornano da sole quando arrivano nuove foto che corrispondono"`, e sotto
   una riga di chip, una per ricerca salvata (icona lente 11px + etichetta). Nel mockup ne esiste
   una precaricata: `"RAW · Urbino"`. La sezione sparisce se non ce ne sono.
2. Titolo `"Cartelle"` con sottotitolo `"Accesso rapido ai tuoi viaggi"` e tre **card cartella**
   (`.folder-cards`, griglia fissa a 3 colonne, gap 12px): copertina alta 88px (il gradiente della
   prima foto della cartella), nome cartella (13.5px, 700) e `"<N> foto"` (11.5px, terziario) —
   quindi: Urbino / 556 foto, Lago di Braies / 110 foto, Chioggia e Venezia / 246 foto.
3. Titolo `"Aggiunti di recente"` e una griglia di **al massimo 32 foto**, ordinate per
   `monthDistance` crescente (il mese più vicino a luglio, il "mese corrente" della demo, per primo).
   La riga di conteggio è **vuota** in questo stato.

**Stato "ho cercato"** (`hasSearch === true`, cioè c'è testo **oppure** almeno una pillola):

1. Le sezioni "Ricerche salvate" e "Cartelle" **spariscono**.
2. Il titolo diventa `"Risultati"` e a destra compare il pulsante
   `"Salva questa ricerca"` (icona `+`, `btn btn-sm`).
3. Riga di conteggio (`.results-count`, 12.5px terziario) costruita da `queryRecapHTML()`:
   `Ricerca: <b>Tag: Tramonti</b> + descrizione libera <b>«tramonto»</b> — 12 risultati`
   Le parti sono unite da `" + "`, il testo libero è etichettato `"descrizione libera"` e messo tra
   virgolette caporali. Il numero è al singolare o plurale: `"1 risultato"` / `"12 risultati"`.
4. La griglia dei risultati.

**Nota importante:** i chip del tipo file (RAW/JPEG/Preferiti) **non** contano come "ricerca". Con
solo `"RAW"` attivo la pagina resta in stato iniziale — titolo `"Aggiunti di recente"`, nessun
conteggio, card cartelle visibili — ma le 32 foto mostrate sono comunque filtrate solo RAW, senza
che nulla lo dica. Vedi Ambiguità.

**Stato "nessun risultato"**: la griglia viene sostituita dall'empty state standard
(`.empty-state`, icona 34px con `opacity:.5`, testo centrato):
titolo `"Nessun risultato"`, sottotitolo
`"Prova a togliere un filtro, o descrivi la foto in un altro modo."`

**Barra di selezione**: se è attiva la selezione multipla, sopra il conteggio compare la barra
"N selezionate" (SP-2), con le azioni preferiti / album / **condividi** / modifica / elimina.

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Chip di una ricerca salvata | chip | Ripristina `type` e `query` di quella ricerca e **azzera tutte le pillole**. |
| 2 | Card cartella | card cliccabile | Imposta la cartella corrente e passa alla vista Foto. |
| 3 | `"Salva questa ricerca"` | pulsante | Aggiunge in coda a `state.savedSearches` una voce la cui **etichetta** è la concatenazione con `" + "` di tutte le etichette delle pillole e del testo libero (es. `"Tag: Tramonti + Sony A7 IV + tramonto"`). Poi il pulsante cambia testo in `"Salvata ✓"` e si disattiva con `pointer-events:none`. |
| 4 | Tile foto | SP-1 | Apertura nel lightbox, spunta di selezione, cuoricino. |
| 5 | Barra "N selezionate" | SP-2 | Invariata rispetto alle altre viste a griglia. |

Nel mockup **non esiste** in questa pagina: il pannello imbuto del filtro rapido (SP-3), il
"Seleziona tutto quello che vedi" come voce separata (c'è `"Seleziona tutte"` dentro la barra di
selezione, SP-4), l'ordinamento dei risultati, la paginazione, il caricamento progressivo.

### 4. Interazioni da mouse

- Click su card cartella / chip ricerca salvata / tile: come sopra.
- Hover su una card cartella: **nessun effetto visivo** oltre alla transizione globale — la regola
  `.folder-card` non ha `:hover`. Non previsto nel mockup.
- Hover su un tile: SP-1 (compaiono spunta e cuoricino, `opacity .12s ease`).
- Tasto destro, doppio click, trascinamento: non previsti.
- Scroll: scroll nativo della `.view-root`.

### 5. Interazioni da tastiera

- I tile seguono SP-1/SP-8 (`tabindex="0"`, Invio/Spazio aprono o selezionano).
- **Chip ricerca salvata e card cartella non hanno `tabindex`**: non raggiungibili da tastiera.
- `"Salva questa ricerca"` è un `div.btn` **senza `role` né `tabindex`** e con `onclick` diretto:
  anch'esso non attivabile da tastiera (deviazione da SP-8).
- Nessuna navigazione a frecce dentro la griglia dei risultati (SP-13 vale solo nel lightbox e nel
  culling).

### 6. Animazioni e transizioni

- La griglia è ricostruita a ogni carattere digitato: nessuna transizione, nessun fade, il
  contenuto salta al nuovo risultato. È una scelta deliberata di semplicità del mockup.
- Il commento a codice alle righe 4561–4563 spiega il trucco strutturale che rende possibile questa
  reattività: *«ricalcola SOLO l'area risultati (usata a ogni tasto premuto o pillola
  aggiunta/rimossa) — il composer con l'input resta intatto, così il focus e il cursore non saltano
  via a ogni carattere digitato»*.
- E alle righe 4615–4618, il perché di una chiamata apparentemente ridondante: *«renderAll()
  richiama layoutJustifiedGrids() dopo ogni render completo, ma la ricerca ricostruisce questa
  griglia anche FUORI da renderAll() […] senza questa chiamata le tile restano a dimensione 0.»*
  L'implementazione Vue deve garantire lo stesso: ricalcolare il layout giustificato ogni volta che
  l'elenco cambia.
- Transizioni presenti: quelle globali `.2s ease` su colori, e `opacity .12s ease` sulla spunta
  di selezione dei tile.

### 7. Stati per ogni controllo

- **Area risultati**: iniziale (scoperta) / con risultati / vuota (empty state). **Nessuno stato di
  caricamento**: la ricerca è sincrona in memoria. **Nessuno stato di errore.**
- **`"Salva questa ricerca"`**: normale → dopo il click diventa `"Salvata ✓"` e non è più cliccabile
  (`pointer-events:none`). Attenzione: questo stato **si perde** al successivo ricalcolo dell'area
  (basta digitare un carattere) perché il pulsante viene ricreato da zero.
- **Chip ricerca salvata**: normale / hover. Mai `.active`, anche quando la ricerca corrispondente è
  quella attiva.
- **Card cartella**: stato unico.
- **Tile**: SP-1 (normale, hover, selezionato con `outline:2.5px solid var(--accent)` a
  `outline-offset:-2.5px`).

### 8. Da dove ci si arriva e dove si va

Ci si arriva dalla stessa vista Cerca (è la metà inferiore). Si esce: verso il **lightbox** (click
su un tile), verso la **vista Foto** su una cartella (card cartella), verso la **modifica multipla**
o il dialog **"Condividi N elementi"** dalla barra di selezione (SP-2).

### 9. Dati necessari a questa schermata

**Legge:** per ogni foto — miniatura/colore, nome file, cartella di appartenenza, se è RAW, se è
preferita, ISO, fotocamera, mese di scatto, "scena" riconosciuta (nel sistema reale: l'embedding),
stato dei tag. Per le card: nome cartella, totale foto, copertina. Le ricerche salvate: etichetta,
testo, tipo file.

**Scrive:** la lista delle ricerche salvate (aggiunta di una voce). Tramite i tile e la barra di
selezione scrive anche "preferita sì/no" e l'insieme delle foto selezionate.

**Come funziona la ricerca testuale nel mockup** (da riportare all'architetto, perché è un
surrogato): `sceneKeywordMatch()` (righe 1513–1517) spezza il testo su spazi/virgole/punti e virgola
e cerca ogni parola in un dizionario cablato `SCENE_KEYWORDS` (righe 1501–1512) che mappa parole
italiane su una delle dieci "scene" (`tramonto, montagna, mare, citta, ritratto, natura,
architettura, notturna, neve, strada`). Se **almeno una** parola è nel dizionario, si tengono le
foto la cui scena è tra quelle trovate (unione, quindi OR tra le parole riconosciute). Se **nessuna**
parola è riconosciuta, si ripiega su una ricerca per sottostringa su **nome file** e **nome della
cartella**. Il commento a codice è netto: *«Un surrogato scritto a mano per un mockup deterministico
— la ricerca reale confronterebbe embedding, non parole chiave letterali»* e, su `scene` in
`genPhotos`, *«sta al posto del vettore-embedding reale […] mai visibile all'utente»*.

Conseguenza pratica per chi implementa: cercare `"tramonto con casa"` riconosce `tramonto` e `casa`
(→ scene `tramonto` e `architettura`) e ignora `con`; cercare `"DSC08"` non riconosce nulla e cade
sul nome file; cercare `"Urbino"` cade sul nome cartella.

**Cosa si può cercare, in sintesi onesta:**
- ✅ tag (via pillola), fotocamera (via pillola), cartella (via pillola o testo), ISO (via pillola),
  nome file (via testo), scene riconosciute dall'IA (via testo);
- ⚠️ anno, presenza di GPS e paese: la pillola si crea e si vede, ma **non filtra** nel mockup;
- ❌ **date** (nessun filtro per giorno/mese/intervallo, nonostante il placeholder della topbar dica
  "data"), **persone/volti** (il chip "Persona" è disabilitato di proposito), obiettivo, diaframma,
  tempo, valutazione a stelle, stato scelto/scartato.

---

## 26. Mappa

### 1. Nome e scopo

Vista a tutta pagina che mostra dove sono state scattate le foto, raggruppate per luogo, con
accesso rapido alla cartella corrispondente.

### 2. Cosa mostra

- Uno sfondo `--map-bg` con una **griglia di linee** (`.map-grid-lines`: due gradienti lineari da
  1px, passo `64px × 64px`, `opacity:.5`). Non è una mappa: è un fondale astratto.
- **Tre pin** (`.map-pin`), uno per cartella, posizionati con percentuali **cablate** in
  `MAP_PIN_POS` (riga 4625): Urbino `top:46% left:52%`, Lago di Braies `top:22% left:58%`,
  Chioggia `top:40% left:62%`. Il pin è una pillola arancione (`--accent`, testo `--accent-text`),
  alta 26px, `border-radius:13px`, bordo 2px del colore dello sfondo, ombra `0 2px 6px rgba(0,0,0,.25)`.
- **Dentro ogni pin, il numero di foto**: 556, 110, 246 — cioè il campo `total` della cartella.
- In alto a sinistra, tre **pulsanti di controllo** (`.map-ctl-btn`, 30×30px): `+`, `−`, e l'icona
  "individua posizione" (`locate`).
- In basso a sinistra, la **scala**: il testo statico `"300 km"`.
- La vista è a tutta area: `#viewRoot` riceve la classe `no-pad` (riga 3065), quindi niente padding.

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Pin (×3) | pulsante grafico | Apre il popover di quel gruppo; ricliccandolo lo chiude (comportamento a interruttore). |
| 2 | `+` (`#mapZoomIn`) | pulsante | **Nessun gestore: non fa nulla.** È solo grafica. |
| 3 | `−` (`#mapZoomOut`) | pulsante | **Nessun gestore: non fa nulla.** |
| 4 | Icona `locate` | pulsante | **Nessun gestore e nessun id: non fa nulla.** |
| 5 | `"300 km"` | etichetta statica | Non interattiva; il valore non cambia mai. |

### 4. Interazioni da mouse

- **Click su un pin**: apre/chiude il popover. Il gestore fa `stopPropagation()` per non essere
  subito annullato dal listener del contenitore.
- **Click su qualsiasi altro punto della mappa**: chiude il popover.
- **Hover su un pin**: `filter:brightness(1.08)` — nessun ritardo, nessun tooltip, nessuna anteprima.
- **Zoom con la rotellina: non implementato.**
- **Trascinamento / pan della mappa: non implementato.**
- **Doppio click, tasto destro: non implementati.**

**Risposta onesta alla domanda "si può zoomare o trascinare?": no.** Nel mockup la mappa è
un'immagine statica: uno sfondo a griglia con tre pin in posizione fissa espressa in percentuale
del contenitore. I pulsanti `+`/`−` e il mirino esistono solo per comunicare l'intenzione di
progetto. L'unica cosa che "si muove" è il popover. Nel prodotto reale (mappe offline servite dal
server Keeppix — vedi Impostazioni → Mappe offline) il pin dovrebbe essere posizionato dalle
coordinate reali, che il mockup già possiede in `PLACES` ma **non usa** per il posizionamento.

### 5. Interazioni da tastiera

**Nessuna.** I pin sono `<div>` senza `role`, senza `tabindex` e senza `aria-label`: non sono
raggiungibili né attivabili da tastiera. I pulsanti di controllo neanche. **Esc non chiude il
popover** (non c'è un ramo per `state.mapPopover` nel gestore globale dei tasti). Da colmare
interamente nell'implementazione Vue.

### 6. Animazioni e transizioni

- Nessuna animazione di apertura/chiusura del popover: appare e sparisce istantaneamente.
- Nessuna transizione di zoom o pan (non essendoci zoom né pan).
- Sul pin, l'hover applica un `filter` **senza transizione dichiarata** (la regola globale copre
  solo `background-color`, `border-color`, `color`): quindi il cambio di luminosità è istantaneo.

### 7. Stati per ogni controllo

- **Pin**: normale / hover (più chiaro). **Non esiste uno stato "selezionato"**: quando il suo
  popover è aperto il pin ha esattamente lo stesso aspetto di prima — l'utente non ha conferma
  visiva di quale pin ha aperto se non la posizione del popover.
- Esiste in CSS una variante `.map-pin.small` (sfondo card, testo normale, bordo sottile) che **non
  è mai applicata** dal JS: presumibilmente pensata per i cluster piccoli o per i pin secondari.
- **Pulsanti di controllo**: normale / hover (`--chip-bg` via `.btn`? no: `.map-ctl-btn` non ha
  regola `:hover`, quindi **nessun feedback di hover**). Nessuno stato disabilitato, benché di fatto
  siano inerti.
- **Vista**: unico stato. Nessuno stato vuoto (le tre cartelle esistono sempre), nessun caricamento,
  nessun errore.

### 8. Da dove ci si arriva e dove si va

**In ingresso:** voce di sidebar `"Mappa"`; da mobile, tab `"Altro"` → sezione Libreria → `"Mappa"`;
**e dal lightbox**, cliccando la mini-mappa nella sezione "Posizione" del pannello informazioni
(riga 4185) — un ingresso non ovvio, senza affordance visiva, da segnalare.

**In uscita:** `"Apri cartella"` nel popover porta alla vista Foto su quella cartella; altrimenti
solo tramite navigazione.

### 9. Dati necessari a questa schermata

**Legge:** per ogni gruppo geografico — un identificativo, il nome da mostrare, il numero di foto,
una copertina, l'etichetta testuale del luogo, e (nel prodotto reale) le coordinate per il
posizionamento e il livello di zoom a cui il gruppo va aggregato.

**Scrive:** nulla. La mappa è di sola lettura; l'unico stato scritto è "quale popover è aperto".

---

## 27. Popover della mappa (quando si clicca un gruppo di foto)

### 1. Nome e scopo

Scheda di anteprima che compare accanto al pin cliccato e dice cosa c'è in quel luogo.

### 2. Cosa mostra

Una card larga **190px** (`.map-popover`, sfondo `--card-bg`, bordo, `border-radius:10px`, ombra
`0 8px 24px rgba(0,0,0,.18)`, `z-index:6`), posizionata a `top: calc(<top del pin> + 18px)`,
`left: <left del pin>`, `transform:translateX(-50%)` — cioè **sotto** il pin e centrata su di esso.
Contiene, dall'alto:

1. **Copertina** alta 76px: il gradiente della **prima foto** della cartella (`photosFor(f.id)[0]`);
2. **Titolo**: nome della cartella (13px, 700) — es. `"Lago di Braies"`;
3. **Sottotitolo** (11px, terziario): `"<N> foto · <etichetta del luogo>"` — es.
   `"110 foto · Lago di Braies, Trentino-AA"`. Le etichette luogo esatte sono
   `"Urbino, Marche"`, `"Lago di Braies, Trentino-AA"`, `"Chioggia, Veneto"`;
4. **Pulsante `"Apri cartella"`** (`btn btn-sm`, a tutta larghezza, centrato).

**Non mostra:** un'anteprima multipla delle foto, il periodo di scatto, le coordinate numeriche,
il numero di foto realmente geolocalizzate, né alcun modo di filtrare per quel luogo.

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Apri cartella"` | pulsante | Imposta la cartella corrente, passa alla vista Foto, ridisegna. Fa `stopPropagation()` per non far chiudere il popover prima di navigare. |
| 2 | Copertina e testi | statici | Non cliccabili come tali — ma vedi sotto. |

**Non c'è un pulsante di chiusura ✕.** Si chiude ricliccando il pin o cliccando altrove sulla mappa.

### 4. Interazioni da mouse

- Click su `"Apri cartella"` → naviga.
- **Click sul corpo del popover (copertina, titolo, sottotitolo) → chiude il popover.** Il popover è
  figlio di `#mapWrap`, che ha un listener di chiusura su click: qualunque click che non sia sul pin
  o sul pulsante risale fino a lui. È un comportamento controintuitivo (cliccare dentro una scheda
  la fa sparire) — vedi Ambiguità.
- Hover: nessun effetto sul popover; il pulsante segue `.btn:hover` (`background:var(--chip-bg)`).
- Tasto destro, trascinamento, rotellina: non previsti.

### 5. Interazioni da tastiera

**Nessuna.** Il pulsante `"Apri cartella"` è un `div.btn` senza `role="button"` e senza `tabindex`,
con `onclick` diretto: non è focalizzabile né attivabile da tastiera. Il popover non ha
`role="dialog"`, non riceve il focus all'apertura, non lo restituisce alla chiusura e non risponde a
Esc. Deviazione completa da SP-5 e SP-8.

### 6. Animazioni e transizioni

Nessuna: comparsa e scomparsa istantanee. Le uniche transizioni attive sono quelle globali `.2s ease`
sui colori (visibili sull'hover del pulsante).

### 7. Stati per ogni controllo

- **Popover**: chiuso (nessun nodo nel DOM) / aperto. Un solo popover alla volta:
  `state.mapPopover` è un singolo id, quindi aprire un pin chiude automaticamente l'altro.
- **`"Apri cartella"`**: normale / hover. Nessuno stato disabilitato, di caricamento o di errore.
- **Copertina**: se la cartella non avesse foto la copertina sarebbe vuota; nel mockup non accade
  mai.

### 8. Da dove ci si arriva e dove si va

Si apre dal pin. Si esce verso la vista Foto della cartella, oppure si chiude restando sulla mappa.

### 9. Dati necessari a questa schermata

Nome del gruppo/cartella, numero di foto, etichetta leggibile del luogo, una copertina, e l'id da
usare per aprire la cartella. Non scrive nulla.

---

## 28. Dialog "Imposta posizione" / ricerca di regione

Due cose distinte che condividono il tema "luogo", documentate insieme perché è così che si
incontrano nel prodotto.

### A — Dialog "Imposta posizione" (assegnare/modificare il luogo di una foto)

### 1. Nome e scopo

Dialog modale che permette di assegnare, cambiare o togliere la posizione geografica di una singola
foto scegliendo tra i luoghi già noti alla libreria.

### 2. Cosa mostra

Card modale standard (`.modal-card`, larghezza 360px, `border-radius:12px`, padding 18px, ombra
`0 20px 50px rgba(0,0,0,.3)`) sopra uno scrim (`.modal-scrim`, `z-index:80`):

- **Titolo**: `"Imposta posizione"` (sempre questo, anche quando si sta *modificando* una posizione
  già presente — vedi Ambiguità);
- **Sottotitolo**: `"Nessuna mappa reale in questo mockup — scegli tra i luoghi già noti alla libreria."`
  (una dichiarazione di limite del mockup rivolta a chi guarda la demo);
- **Elenco** (`.album-picker-list`, `max-height:260px` con scroll) con una riga per preset. I preset
  sono derivati dalle cartelle: per ciascuna, l'etichetta del luogo e le coordinate arrotondate a
  **due decimali**, allineate a destra in 11px terziario:
  - `"Urbino, Marche"` — `43.73, 12.64`
  - `"Lago di Braies, Trentino-AA"` — `46.70, 12.09`
  - `"Chioggia, Veneto"` — `45.22, 12.28`
- Un'ultima riga `"Nessuna posizione"` (senza coordinate), per rimuovere la posizione;
- Pulsante `"Annulla"` (`btn btn-ghost btn-sm`).

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Riga di un preset | pulsante (`role="button"`, `tabindex="0"`) | Assegna alla foto quel luogo (con lat/lng), chiude il dialog, mostra il toast `"Posizione aggiornata."` e ridisegna. |
| 2 | `"Nessuna posizione"` | pulsante | Imposta il valore speciale che significa "posizione rimossa esplicitamente", chiude, toast `"Posizione rimossa."` |
| 3 | `"Annulla"` | pulsante | Chiude senza modificare nulla. |

**Non c'è** un campo di ricerca, non si possono inserire coordinate a mano, non si può scegliere un
punto su una mappa, non si può applicare a più foto insieme (la modifica multipla ha i suoi campi e
il luogo non è tra questi).

Semantica dei tre valori possibili per il luogo di una foto (`photoPlace`, righe 1650–1655), utile
all'architetto: valore speciale "nessuna" → nessun luogo, **anche se la cartella ne avrebbe uno**;
valore impostato dall'utente → vince; nessun valore → si eredita il luogo della cartella. La
rimozione esplicita è quindi distinta dall'assenza di dato.

### 4. Interazioni da mouse

Click su una riga = scelta immediata (non c'è conferma). Hover su una riga: sfondo `--chip-bg`,
`border-radius:8px`. **Click sullo scrim non chiude il dialog** (nessun gestore sullo scrim) —
deviazione da SP-5.

### 5. Interazioni da tastiera

- **Esc chiude** (gestore `keydown` locale registrato all'apertura e rimosso alla chiusura).
- Le righe sono `role="button" tabindex="0"` e rispondono a **Invio e Spazio** (SP-8).
- **All'apertura il focus non viene messo su nessun elemento** — a differenza del selettore album
  che mette il focus sulla prima riga. Deviazione da SP-5, da correggere.
- Alla chiusura il **focus torna all'elemento che aveva aperto il dialog** (memorizzato in
  `document.activeElement` all'apertura).
- Nessun trap del focus: Tab può uscire dal dialog e raggiungere la pagina sottostante.

### 6. Animazioni e transizioni

Nessuna animazione di apertura/chiusura (il nodo è aggiunto e rimosso dal DOM). Il **toast** che
segue la scelta segue SP-6: `opacity .2s ease, transform .2s ease`, entra da 10px più in basso,
resta 2,4 s, poi svanisce e viene rimosso dopo altri 250 ms.

### 7. Stati per ogni controllo

- **Riga preset**: normale / hover / focus visibile (outline accento 2.5px). **Nessuno stato
  "attualmente selezionato"**: aprendo il dialog su una foto che ha già Urbino, Urbino non è
  evidenziato in alcun modo.
- **`"Annulla"`**: normale / hover.
- Nessuno stato disabilitato, di caricamento o di errore: l'operazione è istantanea e locale.

### 8. Da dove ci si arriva e dove si va

Unico ingresso: **lightbox → pannello informazioni → sezione `"Posizione"` → pulsante
`"Imposta posizione…"`** (se la foto non ha luogo) oppure **`"Modifica posizione…"`** (se ce l'ha).
Nella stessa sezione si vedono, quando il luogo c'è, una mini-mappa con un pin, l'etichetta del
luogo e le coordinate a **quattro** decimali; quando manca, il testo `"Nessuna posizione impostata."`
All'uscita si torna al lightbox, ridisegnato con il nuovo luogo.

### 9. Dati necessari a questa schermata

**Legge:** l'elenco dei luoghi noti alla libreria (etichetta + lat/lng) e il luogo attuale della
foto. **Scrive:** sulla foto, il luogo scelto (etichetta + coordinate) oppure la marcatura
"nessuna posizione".

---

### B — Ricerca di regione (Impostazioni → Mappe offline)

### 1. Nome e scopo

Campo di ricerca in linea che permette di aggiungere all'elenco delle mappe offline un paese o una
regione non ancora presente, per poi scaricarne le tile.

### 2. Cosa mostra

Contesto: la sezione `"Mappe offline"` in Impostazioni, con sottotitolo
`"Le tile sono servite da questo server Keeppix, mai da provider esterni — nessuna richiesta lascia la tua rete"`.
Sopra la ricerca c'è l'elenco delle regioni già in lista, ciascuna con nome, `"<peso> · <stato>"`
(es. `"640 MB · scaricata"`, `"2,1 GB · non scaricata"`) e i suoi pulsanti.

Quando la ricerca è aperta (`.region-search-box`):

- una `<label>` per soli screen reader: `"Cerca un paese o una regione da aggiungere"`;
- il campo `#regionSearchInput` con placeholder `"Cerca un paese o una regione…"`, `autocomplete="off"`;
- un pulsante ✕ con `aria-label="Chiudi ricerca regioni, senza aggiungere nulla"`;
- il riquadro dei risultati (`.region-search-results`, `role="listbox"`,
  `aria-label="Risultati ricerca regioni"`, bordo, `max-height:220px` con scroll), con al massimo
  **8 righe**; ogni riga (`role="option"`, `aria-selected="false"`, `tabindex="0"`) mostra a sinistra
  il **nome** e a destra il **peso** in terziario (es. `"Francia"` — `"480 MB"`).

Il bacino è un elenco cablato di 35 paesi (Francia, Germania, Spagna, Regno Unito, Portogallo,
Paesi Bassi, Belgio, Svizzera, Austria, Grecia, Irlanda, Polonia, Svezia, Norvegia, Danimarca,
Croazia, Stati Uniti, Canada, Messico, Brasile, Argentina, Cile, Giappone, Cina, Corea del Sud,
Thailandia, India, Vietnam, Indonesia, Australia, Nuova Zelanda, Marocco, Egitto, Sudafrica, Kenya).
Il commento a codice (righe 6055–6056) spiega la scelta di interazione: *«elenco ampio apposta: con
la scala mondiale delle mappe offline un elenco di chip diventa ingestibile, meglio un campo di
ricerca (come le città/paesi in Immich o Google Maps)»*.

**Filtro:** sottostringa non sensibile a maiuscole sul nome, **escludendo** le regioni già presenti
in elenco. Poi troncamento a 8.

**Stato vuoto — due messaggi distinti** (`.region-search-empty`):
- senza testo digitato: `"Digita per cercare tra le regioni disponibili — ce ne sono troppe per un elenco unico."`
- con testo che non trova nulla: `"Nessuna regione trovata."`

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Aggiungi regione"` (icona `+`, `btn btn-sm btn-ghost`, `role="button"`, `tabindex="0"`) | pulsante | Apre la ricerca, azzera il testo, ridisegna e mette il focus sul campo (con `setTimeout(…,0)`). Sparisce mentre la ricerca è aperta. |
| 2 | `#regionSearchInput` | campo di testo | Filtra i risultati a ogni carattere. Nessuna validazione. Vuoto = messaggio di invito. |
| 3 | ✕ di chiusura | pulsante | Chiude la ricerca e azzera il testo **senza aggiungere nulla** (lo dice l'`aria-label`). |
| 4 | Riga di risultato | opzione (`role="option"`, `tabindex="0"`) | Aggiunge la regione all'elenco con stato `"non scaricata"` e marcata come non predefinita, chiude la ricerca, azzera il testo e mostra il toast `"<Nome> aggiunta all'elenco."` |
| 5 | `"Scarica"` (su una regione non scaricata) | pulsante | Porta lo stato a `"scaricata"`, toast `"<Nome> scaricata."` |
| 6 | `"Rimuovi"` (su una regione scaricata) | pulsante | Riporta lo stato a `"non scaricata"`, toast `"<Nome> rimossa — spazio liberato."` |
| 7 | ✕ accanto a una regione **non predefinita** (`aria-label="Togli <Nome> dalla lista"`) | pulsante | La toglie del tutto dall'elenco. **Nessun toast, nessuna conferma.** Non compare sulle tre regioni predefinite (Italia, Europa (resto), Resto del mondo), che non si possono togliere. |

### 4. Interazioni da mouse

Click su una riga = aggiunta immediata. Hover su una riga: sfondo `--chip-bg`. Le righe sono
separate da un bordo inferiore da 1px (tranne l'ultima). Nessun tasto destro, doppio click,
trascinamento o rotellina previsti (oltre allo scroll nativo del riquadro dei risultati).

### 5. Interazioni da tastiera

- **Esc chiude la ricerca e azzera il testo, da qualunque punto abbia il focus.** Il commento a
  codice (righe 6300–6302) spiega perché è gestito globalmente e non sull'input:
  *«Esc chiude la ricerca regioni indipendentemente da dove si trova il focus in quel momento (non
  solo quando il focus è nel campo di testo) — prima si poteva restare "intrappolati" lì se il focus
  era finito su un risultato o sul pulsante Annulla.»*
- Le righe di risultato sono `tabindex="0"`: si raggiungono con **Tab** e si attivano con **Invio o
  Spazio** (SP-8).
- **Frecce ↑/↓ non implementate**, benché il contenitore sia un `role="listbox"` e le righe
  `role="option"`: la navigazione avviene con Tab, non con le frecce, e `aria-selected` resta
  **sempre `"false"`** perché nessuno lo aggiorna. Incoerenza ARIA da sistemare in Vue.
- Il campo riceve il focus a ogni ridisegno delle Impostazioni mentre la ricerca è aperta, con il
  cursore forzato in **fondo** al testo (`setSelectionRange` sulla lunghezza del valore).

### 6. Animazioni e transizioni

Nessuna animazione di apertura del riquadro di ricerca né di comparsa dei risultati. Toast: SP-6
(`opacity .2s ease, transform .2s ease`, 2,4 s di permanenza).

### 7. Stati per ogni controllo

- **Ricerca**: chiusa (si vede solo `"Aggiungi regione"`) / aperta con messaggio di invito / aperta
  con risultati / aperta senza risultati.
- **Riga di risultato**: normale / hover / focus visibile. Una regione già in elenco **non compare
  affatto** invece di comparire disattivata.
- **Regione in elenco**: `"scaricata"` (mostra `"Rimuovi"`) / `"non scaricata"` (mostra `"Scarica"`);
  predefinita (senza ✕) / aggiunta dall'utente (con ✕).
- **Nessuno stato di download in corso**: il passaggio a `"scaricata"` è istantaneo. Nel prodotto
  reale servirà una barra di avanzamento e uno stato "in scaricamento" — non previsto nel mockup.

### 8. Da dove ci si arriva e dove si va

Si arriva da **Impostazioni → sezione "Mappe offline"**. Non si esce dalla pagina: l'interazione è
tutta in linea. Il collegamento concettuale con la vista Mappa esiste (le tile che la Mappa
userebbe), ma **non c'è nessun link di navigazione tra le due schermate**.

### 9. Dati necessari a questa schermata

**Legge:** l'elenco delle regioni disponibili al download con nome e peso; l'elenco delle regioni
già in lista con nome, peso, stato di scaricamento e se sono predefinite.
**Scrive:** aggiunta/rimozione di una regione dall'elenco e il suo stato scaricata / non scaricata.

---

## 29. Condivisioni

### 1. Nome e scopo

Pagina di riepilogo di tutto ciò che l'utente ha condiviso (persone, link pubblici, cartelle e
album) e di ciò che altri hanno condiviso con lui.

### 2. Cosa mostra

In cima, due chip che fanno da schede (`.chip-row`): `"Le mie condivisioni"` e `"Condivisi con me"`.
La scheda attiva ha `.active` (sfondo `--accent-tint`, testo accento, bordo accento, 600).

**Scheda `"Le mie condivisioni"`** — tre sezioni (`.share-section`, 26px di distanza l'una dall'altra):

**Sezione 1 — `"Persone"`**
Sottotitolo esatto:
`"Ruoli: Visualizzatore (solo consultazione) o Editor (può modificare, caricare, spostare nel cestino)"`
Poi una riga per persona (`.share-row`, separate da un bordo inferiore, tranne l'ultima), con:
- **avatar** con le iniziali su sfondo colorato (SP-16), colore per persona;
- **nome** (13.5px, 600);
- **email** (11.5px, terziario);
- se la persona ha accesso **ereditato**, una seconda riga sotto l'email in terziario:
  `"Ereditato da: <origine>"` — nel mockup:
  `"Ereditato da: gruppo Famiglia · ereditato in /Chioggia e Venezia"`;
- a destra un **badge del ruolo** (`.role-badge`, pillola grigia 11px): `"Editor"` o `"Visualizzatore"`.

Dati del mockup: **Mich** — mich@keeppix.app — Editor (accesso diretto); **Elena Bianchi** —
elena.bianchi@mail.com — Visualizzatore, ereditato.

In fondo alla sezione, il pulsante `"Invita"` (icona `plusUser`).

**Sezione 2 — `"Link pubblici"`**
Sottotitolo esatto:
`"Chiunque abbia il link può vedere questi contenuti; metadati ed EXIF nascosti di default senza password"`
Poi una riga per link (`.link-row`), con:
- **icona catena** in un quadrato 30×30 con sfondo `--chip-bg`;
- **titolo** del link (13px, 600) — è il nome dell'oggetto condiviso;
- **sottotitolo** (11.5px, terziario);
- a destra, i pulsanti `"Copia"` (ghost, icona `copy`) e `"Revoca"` (rosso).

**Attenzione, punto delicato: il sottotitolo non è un insieme di colonne strutturate, è una singola
stringa già composta**, e i due link del mockup non riportano nemmeno gli stessi campi:

| Titolo | Sottotitolo (letterale) | Campi effettivamente presenti |
|---|---|---|
| `"Chioggia e Venezia"` | `"Cartella · nessuna scadenza · download originale off · 246 elementi"` | tipo, scadenza, download originali, numero elementi |
| `"Migliori scatti 2026"` | `"Album · scade il 30 set 2026 · password attiva · 84 elementi"` | tipo, scadenza, password, numero elementi |

Cioè: **il primo link non dice se ha una password, il secondo non dice se il download degli
originali è attivo.** L'insieme dei campi concettualmente previsti, unendo i due esempi e il link
creato dal dialog di condivisione, è: **tipo di oggetto** (Cartella / Album / `"Selezione manuale"`),
**scadenza** (`"nessuna scadenza"` oppure `"scade il <data>"`), **password** (`"password attiva"`,
mai `"senza password"`), **download degli originali** (`"download originale off"`, mai "on"), e
**numero di elementi**. Il "chi può vedere" non è un campo: lo dice il sottotitolo di sezione —
chiunque abbia il link.

**Sezione 3 — `"Cartelle e album condivisi"`**
Nessun sottotitolo. Una griglia di card (`.album-grid`, colonne auto-riempite da minimo 190px, gap
16px) con due elementi cablati: l'album `"Migliori scatti 2026"` e una finta cartella
`"Chioggia e Venezia"`. Ogni card ha una copertina a gradiente alta 120px con un badge in alto a
destra (`.album-shared-badge`, sfondo `rgba(10,10,10,.55)`, testo bianco 10px, 700):
`"condiviso"` con l'icona di condivisione; sotto, titolo e `"<N> elementi · <periodo o tipo>"` —
es. `"84 elementi · Gen 2026 – Lug 2026"` e `"246 elementi · Cartella"`.

**Scheda `"Condivisi con me"`**
- Se ci sono elementi: titolo `"Condivisi con te"` e una griglia di card. Ogni card: copertina a
  gradiente, badge `"da <proprietario>"` (es. `"da Mich"`), titolo, e sottotitolo
  `"<tipo> · <N> elementi · tu: <ruolo>"` — nel mockup:
  `"Weekend in montagna"`, `"Album · 63 elementi · tu: Editor"`.
- Se non ce ne sono (mai, nel mockup, perché il dato precaricato non è vuoto): empty state con
  titolo `"Niente condiviso con te"` e sottotitolo
  `"Quando qualcuno ti invita a una cartella o un album, comparirà qui — senza mai esporre il percorso reale sul disco."`

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Chip `"Le mie condivisioni"` | scheda | Mostra la prima scheda. |
| 2 | Chip `"Condivisi con me"` | scheda | Mostra la seconda scheda. |
| 3 | Riga persona | statica | **Non cliccabile.** Nessun modo di cambiare il ruolo, revocare l'accesso o vedere cosa esattamente quella persona può vedere. |
| 4 | Badge del ruolo | etichetta | Statico, non è un menu a tendina. |
| 5 | `"Invita"` | pulsante | **Nessun gestore: non fa nulla.** |
| 6 | `"Copia"` (per link) | pulsante | **Nessun gestore: non fa nulla.** Non copia negli appunti e non mostra toast. |
| 7 | `"Revoca"` (per link) | pulsante rosso | **Nessun gestore: non fa nulla.** Nessuna conferma, nessuna rimozione. |
| 8 | `"Crea link di condivisione"` (fondo sezione Link) | pulsante | **Nessun gestore: non fa nulla** (a differenza dell'omonimo pulsante dentro il dialog "Condividi selezione", che invece funziona). |
| 9 | Card cartella/album condiviso | card | **Nessun gestore: non cliccabile.** |
| 10 | Card in "Condivisi con me" | card | **Nessun gestore: non cliccabile.** |

**Azioni disponibili su un link esistente, risposta onesta:** nell'interfaccia sono disegnate due
azioni, **`"Copia"`** e **`"Revoca"`**, e nessuna delle due è collegata a un comportamento nel
mockup. Non sono previsti in alcuna forma: modificare la scadenza, impostare o togliere la password,
attivare il download degli originali, vedere le statistiche di accesso, rinominare il link, vedere
l'URL vero e proprio.

### 4. Interazioni da mouse

- Click sui due chip: cambio scheda (ridisegno completo con `renderAll()`).
- Hover sui chip: `--chip-bg-hover`. Hover sui pulsanti: `.btn:hover` = `--chip-bg`;
  `"Revoca"` (`.btn-danger`) su hover prende sfondo `--danger-tint`.
- Le righe persona e le righe link **non hanno hover**: non c'è regola `:hover` per `.share-row`
  né per `.link-row`, quindi nessun feedback.
- Le card hanno `cursor:pointer` **pur non essendo cliccabili** — falso affordance da correggere.
- Tasto destro, doppio click, trascinamento: non previsti.

### 5. Interazioni da tastiera

**Praticamente nulla.** I chip delle schede non hanno `tabindex` (non focalizzabili). I pulsanti
`"Invita"`, `"Copia"`, `"Revoca"`, `"Crea link di condivisione"` sono `div.btn` senza `role` né
`tabindex`. Le card non sono focalizzabili. Nessuna scorciatoia, nessuna navigazione a frecce,
nessun uso di Esc o Invio in questa vista. È la schermata meno accessibile del blocco: da
ricostruire con elementi nativi nell'implementazione Vue.

### 6. Animazioni e transizioni

Nessuna animazione. Il cambio scheda è un ridisegno secco. Restano solo le transizioni globali
`background-color / border-color / color .2s ease`, che rendono morbido il passaggio del chip da
normale ad attivo, e `.btn-danger:hover` verso `--danger-tint`.

### 7. Stati per ogni controllo

- **Chip scheda**: normale / hover / `.active`.
- **Riga persona**: due varianti di contenuto — con accesso diretto (solo email) e con accesso
  ereditato (email + riga di provenienza). Nessuno stato interattivo.
- **Riga link**: stato unico. Non esiste una resa visiva diversa per un link **scaduto**, **con
  password**, o **revocato**: tutto è testo dentro la stessa stringa.
- **Pulsanti**: normale / hover. Nessuno è disabilitato anche se nessuno funziona: sono
  indistinguibili da pulsanti attivi, e questo è un problema per chi valuta il mockup.
- **Scheda "Condivisi con me"**: con contenuto / vuota (empty state, di fatto irraggiungibile).
- Nessuno stato di caricamento o di errore in nessun punto della pagina.

### 8. Da dove ci si arriva e dove si va

**In ingresso:**
- voce di sidebar `"Condivisioni"` (apre l'ultima scheda usata, di default `"Le mie condivisioni"`);
- da mobile, tab `"Altro"` → sezione **Libreria** → due voci distinte, `"Condivisi con me"` e
  `"Le mie condivisioni"`, che aprono direttamente la scheda giusta (righe 2499–2500);
- indirettamente: creando un link dal dialog "Condividi selezione", il nuovo link compare qui in
  cima all'elenco.

**In uscita:** solo tramite navigazione — nessun elemento della pagina porta altrove.

### 9. Dati necessari a questa schermata

**Legge:**
- per ogni **persona** con accesso: nome, email, ruolo (Visualizzatore / Editor), colore
  dell'avatar, e se l'accesso è ereditato l'origine dell'ereditarietà (gruppo e cartella);
- per ogni **link pubblico**: titolo, tipo di oggetto, scadenza, presenza di password, se il
  download degli originali è permesso, numero di elementi;
- per ogni **cartella/album condiviso**: nome, numero di elementi, periodo o tipo, se è condiviso;
- per ogni **elemento condiviso con me**: nome, tipo, numero di elementi, proprietario, mio ruolo.

**Scrive:** nel mockup, **nulla** — nessuna delle azioni è collegata. Nel prodotto reale dovrebbe
scrivere: creazione/revoca di link, modifica dei loro parametri, inviti e cambi di ruolo.

Rilevante per l'architetto, e coerente con la sezione "Riconoscimento volti" delle Impostazioni:
*«I volti sono dati biometrici […] Non compaiono mai su un link pubblico condiviso: non è
configurabile, vale sempre.»* Quindi qualunque link generato da questa pagina deve escludere i dati
dei volti per costruzione.

---

## 30. Dialog "Condividi selezione"

### 1. Nome e scopo

Dialog modale che condivide **una selezione arbitraria di foto** senza dover condividere l'intera
cartella o album di provenienza — con persone già invitate, o con un nuovo link pubblico.

Il commento a codice (righe 3416–3417) lo dice esattamente così: *«condividi una selezione di foto:
con persone esistenti, o via nuovo link pubblico scoped alla selezione (senza dover condividere
l'intera cartella o album)»*.

### 2. Cosa mostra

Card modale standard 360px su scrim, con `role="dialog"`, `aria-modal="true"` e
`aria-labelledby` che punta al titolo:

- **Titolo**: `"Condividi <N> elementi"`, con singolare corretto: `"Condividi 1 elemento"`;
- **Sottotitolo**:
  `"Concedi accesso a persone già invitate, oppure crea un link pubblico solo per questa selezione."`;
- **Sotto-titolo di sezione `"Persone"`** (12.5px);
- **Elenco delle persone** (`.album-picker-list`, `max-height:260px` con scroll). Ogni riga ha
  `role="switch"`, `aria-checked` (inizialmente `"false"`), `aria-label` col nome, `tabindex="0"`, e
  mostra: **avatar** con iniziali (SP-16, `aria-hidden`), **nome** in 13px 600, sotto il **ruolo**
  in 11.5px terziario (`"Editor"` / `"Visualizzatore"`), e a destra un **interruttore**
  (`.mini-switch`, 36×20px);
- **Sotto-titolo di sezione `"Link pubblico"`** con sottotitolo
  `"Sola visualizzazione, senza condividere l'intera cartella o album di provenienza."`;
- **Pulsante `"Crea link di condivisione"`** (icona catena, `btn btn-sm`, `role="button"`,
  `tabindex="0"`);
- **Pulsante `"Fatto"`** (`btn btn-ghost btn-sm`, `role="button"`, `tabindex="0"`).

**Non mostra:** l'anteprima delle foto selezionate, la scadenza, il campo password, il permesso di
download, e nemmeno l'URL del link creato.

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Riga persona (interruttore) | switch accessibile | Alterna la condivisione con quella persona. Accendendolo: toast `"Condiviso con <Nome>."` Spegnendolo: toast `"Accesso rimosso a <Nome>."` L'interruttore prende/perde la classe `on` e `aria-checked` è aggiornato di conseguenza. |
| 2 | `"Crea link di condivisione"` | pulsante | Inserisce **in cima** all'elenco dei link pubblici una nuova voce con titolo `"<N> foto selezionate"` e sottotitolo `"Selezione manuale · nessuna scadenza · download originale off · <N> elementi"`; mostra il toast `"Link creato e copiato negli appunti."`; chiude il dialog e ridisegna. |
| 3 | `"Fatto"` | pulsante | Chiude il dialog. Non è una conferma: le persone attivate hanno già avuto effetto una per una. |

**Nota sul toast del link:** dice *"e copiato negli appunti"*, ma **nessuna scrittura negli appunti
avviene nel mockup**.

**Nota sulla persistenza:** l'insieme delle persone attivate vive in una variabile locale del dialog
e **non viene salvato da nessuna parte**: riaprendo il dialog sulla stessa selezione, tutti gli
interruttori sono di nuovo spenti. Il link creato invece è persistente per la sessione (compare in
Condivisioni).

### 4. Interazioni da mouse

- Click su una riga persona (in qualunque punto: avatar, nome o interruttore) alterna lo stato.
- Hover sulla riga: sfondo `--chip-bg`, `border-radius:8px`.
- Hover sui pulsanti: `.btn:hover` / `.btn-ghost:hover` → sfondo `--chip-bg`.
- **Click sullo scrim non chiude il dialog** — deviazione da SP-5, identica a quella del dialog
  "Imposta posizione".
- Tasto destro, doppio click, trascinamento: non previsti.

### 5. Interazioni da tastiera

- **Esc chiude** (gestore locale, rimosso alla chiusura).
- Righe persona, `"Crea link di condivisione"` e `"Fatto"` sono tutti `tabindex="0"` con
  `role` appropriato e rispondono a **Invio e Spazio** (SP-8, con `preventDefault()`).
- **All'apertura il focus va sulla prima riga persona**, o — se non ci fossero persone — sul
  pulsante `"Crea link di condivisione"`.
- **Alla chiusura il focus torna al pulsante che ha aperto il dialog** (l'icona "Condividi" della
  barra di selezione, o il pulsante di condivisione del lightbox).
- Nessun trap del focus: Tab può uscire dalla card modale.
- Nessuna navigazione a frecce nell'elenco delle persone.

### 6. Animazioni e transizioni

- Nessuna animazione di apertura/chiusura della card.
- **Il pomello dell'interruttore scorre**: `.mini-switch .knob { transition: left .15s ease }`,
  da `left:2px` a `left:18px`, e lo sfondo passa a `--accent` (con la transizione globale
  `background-color .2s ease`). È l'unica animazione con un vero significato in questo dialog:
  comunica l'atto di "accendere" un permesso.
- **Toast** (SP-6): `opacity .2s ease, transform .2s ease`, ingresso da 10px sotto, permanenza
  2,4 s. Attivando più persone in rapida successione i toast si sovrappongono nello stesso punto —
  non sono impilati.

### 7. Stati per ogni controllo

- **Riga persona**: normale / hover / focus visibile (outline accento 2.5px, `.album-picker-row` è
  nella lista dei `:focus-visible`); interruttore **spento** (sfondo `--border-strong`, pomello a
  sinistra) / **acceso** (`.on`, sfondo `--accent`, pomello a destra).
- **`"Crea link di condivisione"`**: normale / hover / focus visibile. Non si disabilita dopo il
  click perché il dialog si chiude subito.
- **`"Fatto"`**: normale / hover / focus visibile.
- Nessuno stato disabilitato, di caricamento o di errore: tutte le operazioni sono locali e
  istantanee. **Nessuno stato "già condiviso con questa persona"**: il dialog non sa cosa la persona
  può già vedere.
- Il dialog non si apre affatto se la selezione è vuota (il gestore verifica `sel.length` prima di
  chiamarlo), quindi non esiste uno stato vuoto.

### 8. Da dove ci si arriva e dove si va

**In ingresso, due strade:**
1. **Barra di selezione multipla** (SP-2), pulsante icona `"Condividi"` (`aria-label="Condividi
   selezione"`, tooltip `data-tip="Condividi"`, SP-7) — disponibile in Timeline, Preferiti, Album,
   dettaglio Persona **e nei risultati della Ricerca**, cioè ovunque ci sia una griglia con
   selezione;
2. **Lightbox**, pulsante di condivisione: apre il dialog per quella **singola** foto (titolo
   `"Condividi 1 elemento"`).

**In uscita:** si torna esattamente da dove si è venuti, con il focus restituito al pulsante di
partenza. Se è stato creato un link, questo si trova poi in **Condivisioni → Le mie condivisioni →
Link pubblici**, in cima all'elenco.

### 9. Dati necessari a questa schermata

**Legge:** il numero di foto selezionate e i loro identificativi; l'elenco delle persone già
invitate con nome, ruolo e colore dell'avatar.

**Scrive:** un nuovo link pubblico limitato alla selezione (con tipo `"Selezione manuale"`, nessuna
scadenza, download originali disattivato e il conteggio degli elementi) e — concettualmente — la
concessione di accesso alle persone attivate, che nel mockup però non viene persistita.

---

# Parte VI — Persone e volti

> Blocco funzionale "Gruppo B — volti". Tutto ciò che segue è letto da
> `/home/claude/keeppix/index.html` (JS ~1740–1930, 2560–3045, 4085–4285, 5726–5878;
> CSS 468–520, 655–718, 813–822, 898–902, 949–961).
>
> **Nota di lettura trasversale:** l'intero blocco è governato da un interruttore di privacy,
> `state.faceRecognitionEnabled` (default `true`). L'effetto sull'interfaccia è descritto in
> dettaglio nella sezione 1 (§ *L'interruttore del riconoscimento volti*) e ripreso nella
> sezione 9; qui basti sapere che quando è spento **la griglia Persone e la coda Revisione →
> Volti si svuotano**, ma i dati restano.

---

## 31. Persone — la griglia

### 1. Nome e scopo
Pagina d'ingresso del blocco volti (`renderPersone`, riga 2620): mostra tutte le persone
riconosciute nelle foto, raggruppate nei gruppi creati dall'utente, e permette di aprirne una,
selezionarne più d'una per unirle, e gestire i gruppi.

### 2. Cosa mostra

**Intestazione** (`.album-toolbar`):
- Titolo `"Persone"` (`.section-title`, 15px/700).
- Sottotitolo `"Riconosciute automaticamente dalle tue foto — mai su un link pubblico, mai fuori da questo server"` (`.section-sub`, 12.5px, colore terziario).
- Pulsante `"Nuovo gruppo"` in alto a destra (icona `plus` 13px).

**Banner della coda di revisione** — solo se `pendingFaceCount() > 0`:
- Icona `inbox` 15px, testo `"<b>N proposta</b> in attesa nella coda di revisione volti"`
  (singolare `proposta` / plurale `proposte`, scelto su `pendingTotal===1`), e a destra un
  `chevronRight` 14px. Con i dati demo: **23 proposte** (14 per Marta + 9 per Luca).

**Barra di selezione** — solo se `state.personSelectedIds.size > 0`, dentro `.grid-toolbar`
(vedi controllo 7–9 più sotto).

**Un blocco per ogni gruppo** (`personGroupBlockHTML`), nell'ordine di `PERSON_GROUPS`, e in
coda **sempre** un blocco `"Senza gruppo"` anche se vuoto. Ogni blocco mostra:
- nome del gruppo (13.5px/700) e conteggio `"N persona"` / `"N persone"` (11.5px, terziario);
- per i gruppi veri (non per "Senza gruppo") due pulsanti icona: matita (`edit`) e cestino
  (`trash`, variante `danger`);
- la griglia delle persone, oppure — se il gruppo è vuoto — la frase
  `"Nessuno qui ancora — assegna una persona a questo gruppo dal suo dettaglio, o dalla selezione multipla qui sopra."`

**Ogni scheda persona** (`personCardHTML`, riga 2568) mostra esattamente quattro cose:
1. avatar circolare con la **foto di copertina** della persona (`personCoverPhoto`: la foto del
   volto indicato da `coverFaceId`, altrimenti la foto del **primo** volto confermato);
2. la casella di spunta di selezione, in alto a sinistra sull'avatar;
3. il **nome** — `person.name` se c'è, altrimenti `"Persona <autoNum>"` (`personDisplayName`);
4. la riga sotto: `"N foto"` / `"N fota"`→ in realtà `"N fot" + (count===1?'a':'e')`, quindi
   `"1 foto"` è reso `"1 fota"`… no: il codice produce `1 fota`? — testualmente:
   `${count} fot${count===1?'a':'e'}` → con 1 → `"1 fota"`. **Refuso presente nel mockup**, va
   segnalato (vedi Ambiguità in coda al documento). Con più foto: `"52 fote"`. Idem in tutti i
   punti dove ricorre lo stesso costrutto.
   Se la persona **non ha nome**, in coda alla stessa riga: `" · "` + `"da nominare"` reso in
   colore accento e grassetto 600 (`.person-unnamed-hint`).

**Riga finale**, solo se ci sono persone nascoste:
`"N persona/e nascosta/e non mostrata/e qui."` (testo statico con le doppie desinenze, non
declinato).

**Chi entra in griglia:** `visiblePeople()` = persone **non nascoste** *e* con **almeno un volto
confermato**. Una persona a cui vengono tolti tutti i volti sparisce dalla griglia da sola.

**Dati demo** (righe 1731–1786): gruppi `Famiglia` e `Amici`; 12 persone —
Marta (52), Elena (38), Paolo (34) in *Famiglia*; Luca (29), Sara (24), Chiara (31) in *Amici*;
Davide (19) senza gruppo; quattro senza nome (`Persona 12` con 37 volti, `Persona 5` con 11,
`Persona 21` con 9, `Persona 9` con 6) senza gruppo; una nascosta (`Persona 30`, 5 volti) che
non compare. Chiara è costruita apposta con un cluster misto (9 volti su 31 con `subCluster:1`)
per poter dimostrare la Separazione.

### 3. Ogni controllo, uno per uno

| # | Etichetta / elemento | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Nuovo gruppo"` | pulsante (`btn btn-sm btn-ghost`) | Apre il dialog di testo standard: titolo `"Nuovo gruppo"`, sottotitolo `"Un gruppo raccoglie più persone fotografate (es. \"Famiglia\", \"Amici\") — non è un gruppo di utenti dell'app, quello esiste già altrove per i permessi."`, campo con placeholder `"Es. Famiglia, Amici, Colleghi…"`, vuoto all'apertura, conferma `"Crea"` / `"Annulla"`. **Se lasciato vuoto**: il dialog si chiude e non succede nulla (`if(!name) return;`), nessun messaggio d'errore. Nessun controllo di duplicati: due gruppi con lo stesso nome sono ammessi. A conferma: toast `Gruppo "X" creato.` |
| 2 | Banner `"N proposte in attesa nella coda di revisione volti"` | banda cliccabile (`role="button"`, `tabindex="0"`) | Porta a Revisione con la tab Volti già aperta (`state.view='revisione'; state.revisioneTab='volti'`). |
| 3 | Casella di spunta sulla scheda persona | checkbox (`role="checkbox"`, `aria-checked`, `aria-label="Seleziona <nome>"`, `tabindex="0"`) | Aggiunge/toglie la persona da `state.personSelectedIds`. Ferma la propagazione, quindi **non** apre il dettaglio. |
| 4 | La scheda persona (tutta l'area) | area cliccabile, **senza** `role` né `tabindex` | Apre il dettaglio della persona. |
| 5 | Matita nella testata di gruppo | pulsante icona (`aria-label="Rinomina gruppo <nome>"`) | Dialog di testo: titolo `"Rinomina gruppo"`, nessun sottotitolo, placeholder `"Nome gruppo"`, campo **precompilato** col nome attuale e preselezionato, conferma `"Salva"`. Vuoto ⇒ nessuna modifica. Toast `"Gruppo rinominato."` |
| 6 | Cestino nella testata di gruppo | pulsante icona `danger` (`aria-label="Elimina gruppo <nome>"`) | Dialog di conferma: titolo `Eliminare il gruppo "X"?`, sottotitolo `"Le persone al suo interno restano, tornano solo in \"Senza gruppo\"."`, pulsante rosso `"Elimina gruppo"` / `"Annulla"`. Alla conferma azzera `groupId` di tutte le persone del gruppo e rimuove il gruppo. Toast `"Gruppo eliminato."` |
| 7 | `"N selezionata"` / `"N selezionate"` | etichetta nella barra di selezione | Conteggio della selezione persone. |
| 8 | ✕ a sinistra nella barra | pulsante (`aria-label="Annulla selezione"`) | Svuota `personSelectedIds`. |
| 9 | Icona catena (`link`) | pulsante solo-icona, `aria-label="Unisci — sono la stessa persona"`, `data-tip="Unisci"` | **Compare solo con 2 o più selezionate.** Apre il dialog "Unisci persone". |
| 10 | Icona cartella (`folder`) | pulsante solo-icona, `aria-label="Assegna a gruppo"`, `data-tip="Assegna a gruppo"` | Apre "Assegna a gruppo" per tutte le selezionate; al termine svuota la selezione. |

**Il filtro per gruppo non esiste.** Lo stato `state.personGroupFilter` (`'all' | 'senza-gruppo' | id`,
riga 2128) è dichiarato e commentato come "filtro nella griglia Persone" ma **non è letto da
nessuna parte del codice**: non c'è nessuna fila di chip, nessun menu a tendina. Il
raggruppamento avviene solo come blocchi impilati, e sono sempre visibili tutti.
Non c'è nemmeno un ordinamento configurabile (l'ordine è quello dell'array `PEOPLE`), né una
ricerca per nome in questa pagina (esiste solo dentro il selettore di persona, sezione 7).

**Il concetto di "gruppo di persone"** — dal commento a riga 1728 e dal sottotitolo del dialog:
è una raccolta di **persone fotografate** ("Famiglia", "Amici", "Colleghi"), esplicitamente
distinta dai **gruppi di utenti dell'applicazione** usati per i permessi di condivisione, che
"vivono altrove". Serve solo a impilare la griglia in blocchi leggibili: **non** filtra le foto,
**non** dà permessi, **non** è usato in nessun altro punto dell'app. Una persona sta in **al
massimo un gruppo** (`groupId` singolo, non un array). Gruppi preesistenti: **`Famiglia`** e
**`Amici`**; il terzo blocco, **`Senza gruppo`**, non è un gruppo ma il contenitore residuo.

**Come si dà un nome a una persona senza nome.** Nella griglia, una persona senza nome è
mostrata comunque (scelta esplicita, commento a riga 2566: "Ogni card mostra già 'da nominare'
per le persone senza nome, invece di nasconderle altrove"), con:
- come nome, l'etichetta automatica `"Persona <autoNum>"` (es. `"Persona 12"`), dove `autoNum` è
  un contatore progressivo (`_personAutoSeq`, parte da 30) assegnato alla nascita del cluster;
- sotto, il conteggio foto seguito da `· da nominare` in colore accento.

**Non si può nominare dalla griglia.** L'unica via è: aprire la persona → pulsante
`"Rinomina"` nel dettaglio (sezione 2). Le altre due vie per far nascere una persona *già*
nominata sono il selettore di persona (`"Crea persona «…»"`, sezione 7) e il campo `"Nome della
nuova persona"` del dialog di separazione (sezione 6).

### 4. Interazioni da mouse
- **Click sulla scheda** → apre il dettaglio della persona.
- **Click sulla casella di spunta** → seleziona/deseleziona; `stopPropagation` impedisce
  l'apertura del dettaglio.
- **Hover sulla scheda** → la casella di spunta passa da `opacity:0` a `1` in `.12s ease` e
  riprende i `pointer-events` (fuori hover è `pointer-events:none`, così non ruba il click).
  **Nessun ritardo**: è una transizione CSS pura, non un tooltip temporizzato.
- **Click sulla scheda mentre altre sono selezionate** → apre comunque il dettaglio. A
  differenza della griglia foto (SP-2), qui **non esiste una "modalità selezione"** in cui il
  click semplice aggiunge alla selezione.
- **Doppio click**: non implementato (nessun comportamento distinto).
- **Tasto destro**: nessun menu contestuale — non previsto nel mockup.
- **Trascinamento**: non previsto. Non si trascina una persona dentro un gruppo, non si
  riordinano le schede.
- **Rotellina**: solo lo scorrimento normale della pagina.
- **Hover sui pulsanti icona di gruppo**: sfondo `var(--chip-bg)` e colore testo pieno; la
  variante `danger` vira a `var(--danger-tint)` / `var(--danger)`.
- **Tooltip** sui due pulsanti solo-icona della barra di selezione: SP-7 (`data-tip`, assente su
  mobile).

### 5. Interazioni da tastiera
- SP-8 su: `"Nuovo gruppo"`, banner della revisione, casella di spunta di ogni persona, matita e
  cestino di ogni gruppo, i tre controlli della barra di selezione. Invio e Spazio = click
  (`bindActivatable` fa `preventDefault`).
- **Ordine del focus**: quello del DOM — "Nuovo gruppo" → banner → (barra di selezione, se
  presente) → per ogni blocco: matita, cestino, poi le caselle di spunta delle sue persone.
- **La scheda persona non è raggiungibile da tastiera**: non ha `tabindex` né `role`, e il suo
  handler è un `onclick` puro. Da tastiera si può **selezionare** una persona ma **non aprirla**.
  È una lacuna di accessibilità, non una scelta documentata (vedi Ambiguità).
- **Nessuna navigazione con le frecce** dentro la griglia; nessuna scorciatoia di una lettera;
  nessun `Shift+click`/`Shift+Freccia` per selezionare intervalli (esiste solo nel culling).
- **Escape** in questa vista non fa nulla (non annulla la selezione): il gestore globale
  (riga 6289) tratta Esc solo per lightbox, ricerca regioni, picklist, pannello filtri e
  suggerimenti di ricerca.
- Anello di focus: `outline:2.5px solid var(--accent); outline-offset:2px` su tutto ciò che ha
  `[role="button"]` / `[role="checkbox"]`.

### 6. Animazioni e transizioni
- Casella di spunta della scheda: `transition: opacity .12s ease, background .12s ease`.
  Comunica "questa scheda è azionabile ora" senza sporcare la griglia a riposo.
- Selezione della scheda: `.person-card.selected .person-avatar` prende
  `box-shadow: 0 0 0 3px var(--accent)` — **senza transizione dichiarata**, quindi lo stacco è
  istantaneo.
- Toast: SP-6 (`opacity .2s ease, transform .2s ease`, visibile 2400 ms).
- Il resto della vista è ricostruito da `renderAll()` a ogni cambiamento: **nessuna animazione di
  ingresso/uscita** delle schede, nessun riordino animato, nessuno scheletro di caricamento (i
  dati sono sincroni nel mockup).

### 7. Stati per ogni controllo
- **Scheda persona** — normale; hover (compare la spunta); selezionata (anello accento 3px);
  nessuno stato disabilitato, di caricamento o d'errore. Non esiste uno stato "focus" perché non
  è focalizzabile.
- **Casella di spunta** — invisibile a riposo (`opacity:0; pointer-events:none`); visibile su
  hover della scheda, su `:focus-visible` proprio, o quando è `.on`; `.on` = fondo accento, bordo
  bianco, spunta bianca.
- **`"Nuovo gruppo"`, matita, cestino** — normale / hover / focus-visible. Mai disabilitati.
- **Pulsante "Unisci"** — non è *disabilitato* con una sola persona selezionata: **non viene
  proprio disegnato** (`n>=2 ? … : ''`).
- **"Assegna a gruppo"** — sempre presente quando c'è almeno una selezione.
- **Stato vuoto per gruppo** — riga di testo al posto della griglia.
- **Stato vuoto globale** — se non c'è nessuna persona visibile, si vedono comunque le testate
  dei gruppi con "0 persone" e tre righe di testo vuoto: non c'è uno stato vuoto dedicato "non
  hai ancora nessuna persona".
- **Riconoscimento volti spento** — l'intera vista è sostituita (vedi § seguente).
- **Nessuno stato di caricamento/errore** in tutto il blocco.

#### L'interruttore del riconoscimento volti (`state.faceRecognitionEnabled`)
Vive in **Impostazioni → "Riconoscimento volti"** (riga 6179), con questo testo di sezione:
`"I volti sono dati biometrici — un trattamento diverso da un tag \"tramonto\". Non compaiono mai
su un link pubblico condiviso: non è configurabile, vale sempre."`
La riga è `"Riconoscimento facciale attivo"` con un interruttore `role="switch"`,
`aria-checked`, `tabindex=0` (pallina che scorre, `left .15s ease`; fondo accento quando acceso).
Sotto, quando è **spento**, compare la nota:
`"Disattivato: nessun volto nuovo viene rilevato, e \"Persone\" non mostra più nulla. I dati già
raccolti restano salvati finché non li elimini qui sotto."`
Accanto c'è il pulsante rosso `"Elimina tutti i dati dei volti"` → conferma
`"Eliminare tutti i dati dei volti?"` / `"Persone, gruppi e volti riconosciuti verranno cancellati
per sempre — non tocca le foto, solo i dati di riconoscimento facciale. Non è recuperabile."` /
`"Elimina tutto"`, che chiama `wipeAllFaceData()` (svuota **FACES, PEOPLE e PERSON_GROUPS**).

**Cosa sparisce esattamente quando l'interruttore è spento** (i cinque punti in cui è
controllato):
1. **Griglia Persone** (riga 2621): tutto il contenuto è sostituito da uno stato vuoto con icona
   `user`, titolo `"Riconoscimento volti disattivato"`, testo `"Riattivalo da Impostazioni →
   Riconoscimento volti. Le persone già nominate restano salvate finché non elimini i dati dei
   volti."` e un pulsante `"Vai a Impostazioni"` (icona `settings` 13px) che porta direttamente lì.
   Spariscono quindi: gruppi, schede, banner della coda, barra di selezione, "Nuovo gruppo".
2. **Revisione → Volti** (riga 5812): stato vuoto con icona `user`, stesso titolo, testo
   `"Riattivalo da Impostazioni → Riconoscimento volti per vedere le proposte in attesa."` Le
   proposte pendenti **restano in memoria**, semplicemente non si vedono.
3. **Riquadri volto sulla foto nel lightbox** (riga 4144): non vengono disegnati.
4. **Sezione "Persone" del pannello informazioni della foto** (riga 4328): non viene disegnata —
   quindi spariscono anche i chip dei nomi e il chip `"+ aggiungi"`, cioè **l'assegnazione
   manuale di una persona a una foto**.
5. **Dimensione "Persone" del filtro rapido a chip** (SP-3, righe 1968/2014): la lista opzioni
   diventa vuota e al fondo del pannello compare
   `"Persone non disponibile: riconoscimento volti disattivato in Impostazioni."` In più
   `applyBrowseFilters` (riga 1941) fa fallire **ogni** foto se il filtro persone era già
   valorizzato — commento nel codice: *"dimensione disattivata: nessuna foto può 'matchare'"*.
6. **Badge "Revisione" nella barra laterale** (riga 1485): `pendingSuggestionCount()` smette di
   sommare le proposte volti e conta solo i tag.

**Cosa NON cambia** (e va detto all'architetto): la voce **"Persone" resta nella barra laterale**
e nel menu mobile (nessun filtro su `NAV_TOP`), quindi ci si arriva e si trova lo stato vuoto; e
la **linguetta "Volti" della pagina Revisione continua a mostrare il conteggio**
`Volti (23)` perché `revisioneTabsHTML()` (riga 5727) chiama `pendingFaceCount()` senza
controllare l'interruttore — incoerenza rispetto al badge della barra laterale.

### 8. Da dove ci si arriva e dove si va
**In ingresso:** voce `"Persone"` della barra laterale (`NAV_TOP`, icona `user`); da mobile
`"Altro"` → gruppo `"Libreria"` → `"Persone"`; dal menu su un riquadro volto, voce `"Vai alla
persona"` (che però atterra direttamente sul **dettaglio**); da `renderPersonDetail` quando la
persona aperta non esiste più (fallback automatico).
**In uscita:** dettaglio persona (click su una scheda); Revisione → Volti (banner);
Impostazioni (pulsante dello stato vuoto); i cinque dialog elencati nelle sezioni 3–7.
La barra del titolo mostra `Persone` (in grassetto) o `Persone / <b>Nome</b>` quando è aperto un
dettaglio; su mobile il titolo è `"Persone"` o il nome della persona.

### 9. Dati necessari a questa schermata
**Legge:** l'elenco dei gruppi di persone (id, nome, nell'ordine di definizione); l'elenco delle
persone con — id, nome (può essere vuoto), numero automatico di ripiego, gruppo di appartenenza
(uno solo o nessuno), flag "nascosta", volto scelto come copertina; per ogni persona il **numero
di foto distinte** in cui ha un volto **confermato** e la **miniatura** della foto di copertina;
il **numero di proposte volto in attesa** (totale); il **numero di persone nascoste**.
**Scrive:** crea, rinomina ed elimina gruppi; assegna/rimuove il gruppo di una o più persone;
mantiene la selezione multipla corrente (solo stato d'interfaccia, non persistente).

---

## 32. Persone — dettaglio di una persona

### 1. Nome e scopo
`renderPersonDetail` (riga 2691): mostra tutte le foto in cui una persona compare, e offre le
cinque azioni sull'identità della persona (nome, copertina, gruppo, divisione, visibilità).

### 2. Cosa mostra
- Link di ritorno `"Tutte le persone"` con `chevronLeft` 15px (`.back-link`, 13px, colore
  secondario, diventa colore pieno su hover).
- **Testata**: avatar circolare 78×78 con la foto di copertina; a fianco il nome
  (`personDisplayName`) come titolo, e una riga di riepilogo composta da:
  `"N foto"` + (se ha un gruppo) `" · gruppo <b>Nome gruppo</b>"` oppure `" · senza gruppo"` +
  (se nascosta) `" · nascosta"`.
- **Riga di azioni** (`.person-detail-actions`, va a capo su più righe se serve): cinque pulsanti,
  vedi sotto.
- **Barra della griglia**: se è attiva la selezione foto, la barra `"N selezionate"` standard
  (SP-2); altrimenti, allineate a destra, `"Seleziona tutto quello che vedi"` (SP-4) e il
  pannello imbuto del filtro rapido (SP-3) — quest'ultimo calcolato sull'insieme delle foto della
  persona.
- **Griglia foto**: tessere standard SP-1 (badge RAW, spunta, cuoricino) delle foto filtrate.
- **Stati vuoti**: se ci sono foto ma i filtri non ne lasciano passare nessuna → icona `funnel`,
  `"Nessuna foto corrisponde ai filtri"` / `"Prova ad allargare i filtri, o cancellali dal
  pannello qui sopra."`; se la persona non ha proprio foto → icona `user`, `"Nessuna foto qui"` /
  `"Questa persona non ha (più) volti confermati."`

Le foto sono `photosForPerson()`: le foto **distinte** in cui esiste un volto **confermato** di
questa persona (i volti in attesa non contano).

### 3. Ogni controllo, uno per uno

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Tutte le persone"` | link di ritorno | Torna alla griglia (`state.openPerson=null`). Solo `onclick`: **nessun `tabindex`, nessun `role`** → non azionabile da tastiera. |
| 2 | `"Rinomina"` (icona `edit`) | pulsante pieno (`btn btn-sm`) | Dialog di testo: titolo `"Rinomina persona"`, nessun sottotitolo, placeholder `"Nome"`, campo precompilato con `person.name` (**vuoto** se la persona non ha ancora un nome — non viene precompilato con "Persona 12"), conferma `"Salva"`. |
| 3 | `"Scegli copertina"` (icona `photo`) | pulsante fantasma | Apre il dialog della sezione 3. |
| 4 | `"Assegna a gruppo"` **oppure** `"Cambia gruppo"` (icona `folder`) | pulsante fantasma | L'etichetta dipende dal fatto che la persona abbia già un gruppo. Apre il dialog della sezione 4 per questa sola persona. |
| 5 | `"Dividi…"` (icona `copy`) | pulsante fantasma | Apre il dialog della sezione 6. Se la persona ha meno di due volti, **il dialog non si apre** e compare il toast `"Servono almeno due volti per poter dividere questa persona."` — il pulsante resta però pienamente attivo (nessuno stato disabilitato). |
| 6 | `"Nascondi"` (icona `close`) **oppure** `"Mostra di nuovo"` (icona `check`) | pulsante fantasma | Inverte `person.hidden`. Nascondendo: toast `"Persona nascosta."` e **si esce subito dal dettaglio** (`state.openPerson=null`). Riattivando: toast `"Persona di nuovo visibile."` |
| 7 | `"Seleziona tutto quello che vedi"` | SP-4 | Sull'insieme filtrato corrente. |
| 8 | Pannello imbuto (filtro rapido) | SP-3 | Sei dimensioni; qui è particolare che la dimensione "Persone" sia ancora presente e possa quindi restringere ulteriormente alle **co-presenze** ("foto di Marta in cui c'è anche Luca"). |
| 9 | Tessere foto + barra "N selezionate" | SP-1 / SP-2 | Comportamento identico alle altre griglie: cuoricino, album, condividi, modifica in blocco, elimina. |

**Cosa succede se il nome viene lasciato vuoto in "Rinomina"** — il codice assegna comunque:
`person.name = name` (riga 2726) **senza** il controllo `if(!name) return` che invece protegge la
rinomina dei gruppi. Per una persona nata da un cluster automatico l'effetto è pulito (torna a
`"Persona 12"` e ricompare `da nominare`); per le persone del roster iniziale — Marta, Elena,
Paolo, Luca, Sara, Chiara, Davide — che **non hanno `autoNum`**, l'etichetta diventa
`"Persona undefined"`. È un difetto del mockup, non un comportamento voluto.

Non ci sono, in questa schermata: pulsante "Unisci" (l'unione parte solo dalla selezione multipla
nella griglia), pulsante "Elimina persona", elenco dei volti singoli, indicazione di quanti volti
(a differenza delle foto) compongono la persona.

### 4. Interazioni da mouse
- Click sui sei controlli sopra; click sulle tessere = apre il lightbox (SP-1); click sulla spunta
  della tessera = selezione (SP-2); click sul cuoricino = preferito.
- Hover sulle tessere: compaiono spunta e cuoricino (comportamento SP-1).
- Nessun tasto destro, nessun trascinamento (non si trascina una foto per toglierla dalla
  persona), nessun doppio click dedicato.

### 5. Interazioni da tastiera
- I cinque pulsanti di azione: SP-8 (Invio/Spazio), nell'ordine Rinomina → Scegli copertina →
  Assegna/Cambia gruppo → Dividi… → Nascondi.
- Il link `"Tutte le persone"` **non** è nella catena del focus (vedi sopra).
- Griglia foto: comportamento standard SP-1/SP-2; nessuna freccia direzionale in griglia.
- Nessuna scorciatoia specifica del dettaglio persona.

### 6. Animazioni e transizioni
Nessuna animazione propria della schermata: l'avatar grande non ha transizioni, la riga di azioni
neppure. Valgono le transizioni ereditate: spunte delle tessere `opacity .12s ease` (SP-1), toast
SP-6, pannello del filtro rapido (SP-3).

### 7. Stati per ogni controllo
- **Rinomina / Scegli copertina / Assegna gruppo / Dividi… / Nascondi** — normale, hover
  (`background: var(--chip-bg)`), focus-visible (anello accento). **Nessuno è mai disabilitato**,
  nemmeno "Scegli copertina" con zero volti o "Dividi…" con un solo volto: il secondo si limita a
  rispondere con un toast, il primo aprirebbe un dialog con la griglia vuota.
- **"Assegna a gruppo" / "Cambia gruppo"** e **"Nascondi" / "Mostra di nuovo"** — due stati di
  *etichetta e icona*, non due stati visivi.
- **Griglia** — stato pieno, stato "filtri troppo stretti", stato "nessuna foto". Nessun
  caricamento, nessun errore.
- **Nota importante — "Mostra di nuovo" è irraggiungibile.** Nascondendo una persona si esce dal
  dettaglio; le persone nascoste sono escluse da `visiblePeople()`, quindi non compaiono né nella
  griglia Persone, né nel selettore di persona, né nel filtro rapido. Nel mockup **non esiste
  alcun percorso per riaprire il dettaglio di una persona nascosta**: l'unica traccia è la riga
  "N persona/e nascosta/e non mostrata/e qui.", che non è cliccabile. In pratica *nascondere è
  irreversibile dall'interfaccia*, benché il pulsante di ripristino esista nel codice.

### 8. Da dove ci si arriva e dove si va
**In ingresso:** click su una scheda nella griglia Persone; `"Vai alla persona"` dal menu sul
riquadro di un volto (che chiude il lightbox, passa alla vista Persone e apre il dettaglio).
**In uscita:** `"Tutte le persone"`; lightbox di una foto; i quattro dialog (copertina, gruppo,
divisione, più i dialog condivisi di album/condivisione/eliminazione via barra di selezione); il
pulsante "Nascondi" riporta alla griglia. Se la persona sparisce (unita a un'altra, o dati dei
volti eliminati), al render successivo si ricade automaticamente sulla griglia.

### 9. Dati necessari a questa schermata
**Legge:** nome/etichetta della persona, nome del suo gruppo, flag nascosta, foto di copertina;
l'elenco delle foto in cui ha un volto confermato con tutto ciò che serve alla tessera SP-1
(miniatura, tipo RAW/JPEG, preferito, valutazione) e ai sei assi del filtro rapido (tipo,
persone presenti, tag, categorie, fotocamera, cartella).
**Scrive:** il nome della persona; il gruppo di appartenenza; il flag "nascosta"; il volto di
copertina (dal dialog dedicato); indirettamente, tramite la barra di selezione, tutto ciò che
scrive SP-2 sulle foto (preferito, album, cestino, modifiche in blocco).

---

## 33. Dialog "scegli copertina"

### 1. Nome e scopo
`openChooseCoverDialog` (riga 2739): scegliere quale foto rappresenta la persona nell'avatar
della griglia e del dettaglio.

### 2. Cosa mostra
Dialog modale standard (SP-5) largo **460px**, con:
- titolo `"Scegli copertina"`;
- sottotitolo `"<nome persona> — quale foto la rappresenta nella griglia"`;
- una griglia **a 5 colonne** di miniature quadrate (`.cover-pick-grid`, `gap:6px`, altezza
  massima 280px con scorrimento verticale), **una per ogni volto confermato** della persona —
  quindi con i dati demo Marta ne ha 52. La miniatura mostra la **foto intera**, non il ritaglio
  del volto (il mockup non ritaglia: usa `tileStyle(photo)`);
- la miniatura corrispondente alla copertina attuale ha bordo accento (`.current`);
- pulsante `"Chiudi"` (fantasma, piccolo).

### 3. Ogni controllo, uno per uno
| # | Elemento | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Miniatura del volto | riquadro cliccabile (`data-setcover`) | Imposta `person.coverFaceId`, **chiude subito il dialog**, toast `"Copertina aggiornata."` Nessuna anteprima, nessuna conferma. |
| 2 | `"Chiudi"` | pulsante fantasma | Chiude senza modificare. Notare l'etichetta: è `"Chiudi"`, non `"Annulla"` — coerente col fatto che la scelta è immediata. |

Non c'è un'opzione "torna alla copertina automatica" (per rimettere `coverFaceId` a `null`).

### 4. Interazioni da mouse
Click su una miniatura (seleziona e chiude); click su `"Chiudi"`. Hover sulla miniatura: bordo
`var(--border-strong)`. **Il click sullo sfondo scuro non chiude il dialog** (nessun handler sul
`.modal-scrim`) — vale per tutti i dialog di questo blocco. Nessun tasto destro, nessun
trascinamento per riordinare.

### 5. Interazioni da tastiera
- Ogni miniatura è `bindActivatable` (SP-8) — ma attenzione: **le miniature non hanno
  `tabindex` né `role`** nel markup (`splitFaceThumbHTML` a confronto ne ha altrettanto pochi),
  quindi il gestore di tastiera è collegato a elementi non focalizzabili: in pratica **la griglia
  di copertine non è navigabile da tastiera**; lo è solo `"Chiudi"`.
- **Escape** chiude (handler dedicato su `document`, rimosso alla chiusura).
- Alla chiusura il focus torna all'elemento che ha aperto il dialog (`document.activeElement`
  salvato all'apertura).
- **All'apertura nessun elemento riceve il focus** (a differenza dei dialog standard SP-5, che
  mettono il focus sulla prima opzione o sul campo): deviazione da SP-5.
- Nessun *focus trap*: Tab può uscire dal dialog e raggiungere la pagina sottostante.

### 6. Animazioni e transizioni
Nessuna: `.modal-scrim` e `.modal-card` compaiono senza dissolvenza né scala (non c'è `transition`
né `@keyframes` su queste classi). Le miniature cambiano bordo su hover senza transizione
dichiarata. Il solo movimento è il toast (SP-6).

### 7. Stati per ogni controllo
- Miniatura: normale (bordo trasparente 2px) / hover (bordo `--border-strong`) / **corrente**
  (bordo accento). Non esistono stato selezionato-multiplo, disabilitato o di caricamento.
- `"Chiudi"`: normale / hover / focus-visible.
- **Stato vuoto non gestito**: se la persona non ha volti confermati la griglia è vuota e non
  compare alcun messaggio (caso raggiungibile solo forzando, perché la griglia Persone non mostra
  persone con zero foto).

### 8. Da dove ci si arriva e dove si va
Si apre solo dal pulsante `"Scegli copertina"` nel dettaglio persona. Alla chiusura si torna
sempre lì, con la griglia e l'avatar ridisegnati.

### 9. Dati necessari
**Legge:** l'elenco dei volti confermati della persona con la foto a cui appartengono (miniatura)
e l'indicazione di quale è l'attuale copertina.
**Scrive:** l'identificativo del volto scelto come copertina della persona.

---

## 34. Dialog "assegna a gruppo"

### 1. Nome e scopo
`openAssignGroupDialog` (riga 2775): mettere una o più persone dentro un gruppo, o toglierle da
qualunque gruppo.

### 2. Cosa mostra
Dialog modale standard (SP-5, larghezza predefinita 360px):
- titolo `"Assegna a gruppo"`;
- sottotitolo: se le persone sono più d'una, `"N persone selezionate"`; se è una sola, il **nome
  della persona**;
- un elenco (`.album-picker-list`, max 260px con scorrimento) con **come prima riga sempre**
  `"Nessun gruppo"`, poi **una riga per ogni gruppo esistente** nell'ordine di `PERSON_GROUPS`
  (con i dati demo: `Nessun gruppo`, `Famiglia`, `Amici`);
- pulsante `"Annulla"` (fantasma, piccolo).

Le righe mostrano **solo il nome**: nessun conteggio di quante persone contiene il gruppo, nessuna
indicazione di quale sia il gruppo attuale della persona (nessuna riga risulta "selezionata").

### 3. Ogni controllo, uno per uno
| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Nessun gruppo"` | riga cliccabile (`role="button"`, `tabindex="0"`) | Azzera il gruppo di tutte le persone passate; chiude; toast `"Rimosso dal gruppo."` |
| 2 | `"<nome gruppo>"` (una riga per gruppo) | riga cliccabile | Imposta quel gruppo su tutte le persone passate; chiude; toast `"Gruppo assegnato."` |
| 3 | `"Annulla"` | pulsante fantasma | Chiude senza modifiche. |

Non c'è un modo per **creare** un gruppo da qui (a differenza del selettore di persona, che sa
creare al volo): se non esiste ancora nessun gruppo, l'elenco contiene la sola riga
`"Nessun gruppo"` e bisogna uscire e usare `"Nuovo gruppo"` nella griglia.

Chiamato dalla barra di selezione, alla conferma esegue anche il callback che **svuota la
selezione**; chiamato dal dettaglio persona, il callback è `null` e non c'è nulla da svuotare.

### 4. Interazioni da mouse
Click su una riga (applica e chiude); click su `"Annulla"`. Hover riga: `background: var(--chip-bg)`,
angoli arrotondati 8px. Click sullo scrim: **non chiude**. Niente tasto destro, niente
trascinamento.

### 5. Interazioni da tastiera
- Ogni riga è `role="button" tabindex="0"` + SP-8 → Invio/Spazio applicano.
- `.album-picker-row:focus-visible` ha un anello di focus esplicito (regola dedicata a riga 844).
- **Escape** chiude; il focus torna al pulsante che ha aperto il dialog.
- All'apertura **nessun focus iniziale**; nessun focus trap; nessuna navigazione con le frecce.

### 6. Animazioni e transizioni
Nessuna comparsa animata del dialog. Solo l'evidenziazione delle righe su hover (senza durata
dichiarata) e il toast SP-6.

### 7. Stati per ogni controllo
Righe: normale / hover / focus-visible. Nessuno stato "corrente" o "selezionato", nessuno stato
disabilitato, nessuno stato di caricamento. Stato vuoto: elenco con la sola riga
`"Nessun gruppo"` quando non esistono gruppi (nessun messaggio esplicativo).

### 8. Da dove ci si arriva e dove si va
**In ingresso:** pulsante `"Assegna a gruppo"` / `"Cambia gruppo"` del dettaglio persona; icona
cartella della barra di selezione nella griglia Persone.
**In uscita:** si torna alla schermata chiamante, ridisegnata (la persona salta nel blocco del
nuovo gruppo).

### 9. Dati necessari
**Legge:** elenco dei gruppi (id, nome); nome della persona o numero di persone selezionate.
**Scrive:** il gruppo di appartenenza (o nessuno) su ciascuna delle persone indicate.

---

## 35. Dialog "unisci persone"

### 1. Nome e scopo
`openMergePeopleDialog` (riga 2815, preceduto dal commento *"UNIONE — 'queste persone sono in
realtà la stessa'"*): fondere due o più cluster che rappresentano la stessa persona in uno solo,
scegliendo quale nome sopravvive.

### 2. Cosa mostra
Dialog modale standard (SP-5):
- titolo `"Unisci N persone"` (N = quante sono selezionate);
- sottotitolo `"Sono la stessa persona: verranno unite in una sola, <M> foto in tutto. Scegli
  quale nome deve sopravvivere."` — **M è il conteggio delle foto distinte dell'unione**
  (insieme dei `photoId` di tutte le persone, quindi le foto in cui compaiono in due contano una
  volta sola);
- un gruppo di scelta esclusiva (`role="radiogroup"`, `aria-label="Nome che sopravvive"`) con
  **una riga per ogni persona selezionata**: pallino/quadratino di spunta a sinistra
  (`.picklist-check`, 15×15, accento quando attivo), nome della persona
  (`personDisplayName`, quindi anche `"Persona 12"`), e a destra `"N foto"` in 11px terziario;
- due pulsanti: `"Unisci"` (primario, icona `link` 13px) e `"Annulla"` (fantasma).

**Chi sopravvive di default**: la **prima persona selezionata che ha un nome vero**; se nessuna ha
un nome, la prima in assoluto (`people.find(p=>p.name) || people[0]`). "Prima" è nell'ordine in
cui gli id sono stati inseriti nell'insieme di selezione, cioè l'ordine in cui l'utente ha
cliccato — non l'ordine della griglia.

### 3. Ogni controllo, uno per uno
| # | Elemento | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Riga con nome + `"N foto"` | opzione esclusiva (`role="radio"`, `aria-checked`, `tabindex="0"`) | Sceglie il nome che sopravvive; l'elenco viene ridisegnato e i gestori riagganciati. |
| 2 | `"Unisci"` | pulsante primario | Esegue l'unione, chiude, **svuota la selezione**, toast `"N persone unite."` |
| 3 | `"Annulla"` | pulsante fantasma | Chiude senza fare nulla. |

Il dialog **non si apre affatto** se le persone sono meno di due (`if(people.length<2) return;`),
ma questo non è raggiungibile dall'interfaccia perché l'icona catena compare solo con 2+
selezionate.

### 4. Interazioni da mouse
Click su una riga per cambiare il sopravvissuto; click su Unisci/Annulla. Hover riga:
`var(--chip-bg)`. Scrim non cliccabile. Nessun tasto destro, nessun trascinamento.

### 5. Interazioni da tastiera
- Righe: `tabindex="0"` + SP-8 (Invio/Spazio). **Le frecce non funzionano**: pur essendo un
  `radiogroup` ARIA, non è implementata la navigazione con ↑/↓ tipica dei gruppi di radio, e tutte
  le righe restano nella catena del Tab (`tabindex=0` su tutte, non `roving tabindex`).
- Escape chiude; il focus torna al trigger.
- **Attenzione:** cambiando il sopravvissuto l'elenco è ricostruito con `innerHTML`, quindi
  **il focus si perde** e chi naviga da tastiera viene rimandato all'inizio del documento.
- Nessun focus iniziale all'apertura, nessun focus trap.

### 6. Animazioni e transizioni
Nessuna. Il cambio di sopravvissuto è un ridisegno istantaneo dell'elenco (nessuna transizione sul
segno di spunta). Toast SP-6.

### 7. Stati per ogni controllo
- Riga: normale / hover / focus-visible / **scelta** (`.picklist-check.on`: fondo e bordo accento
  con spunta bianca 10px).
- `"Unisci"`: sempre attivo — c'è **sempre** un sopravvissuto scelto, quindi non esiste uno stato
  disabilitato. Nessuno stato di caricamento o di errore.
- Nessun messaggio di avvertimento sull'irreversibilità.

### 8. Da dove ci si arriva e dove si va
**Unico ingresso:** icona catena della barra di selezione nella griglia Persone (visibile solo con
2+ persone selezionate). Non si unisce dal dettaglio persona, né dal lightbox, né dalla coda di
revisione. **In uscita:** si torna alla griglia Persone con la selezione svuotata e una scheda in
meno per ogni persona assorbita.

### 9. Dati necessari
**Legge:** per ogni persona selezionata: id, nome/etichetta, numero di foto; l'insieme unione
delle foto per il conteggio totale.
**Scrive:** riassegna **tutti i volti** delle persone assorbite alla persona sopravvissuta ed
**elimina** le persone assorbite.

### Cosa comporta esattamente unire (e se è reversibile)
`mergePeopleInto(personIds, survivorId)` (riga 1884), per ogni persona diversa dal sopravvissuto:
1. sposta **tutti i suoi volti** (`face.personId = survivorId`), compresi quelli aggiunti a mano;
2. **rimuove la persona** dall'elenco (`PEOPLE.splice`).

Conseguenze da mettere per iscritto per l'architetto:
- **Non è reversibile.** Non c'è annulla, non c'è "unione recente da disfare", il toast non ha
  azione. Le informazioni della persona assorbita — **nome, gruppo, foto di copertina** — vengono
  perse: il sopravvissuto conserva le proprie.
- L'unica "riparazione" possibile è **rifare a mano la separazione** (sezione 6) selezionando i
  volti che erano dell'altra persona e riestraendoli: si riottiene una persona **senza nome, senza
  gruppo, senza copertina**. Quindi: struttura ricostruibile, metadati no.
- **Le proposte in coda restano orfane.** Le proposte volto puntano alla persona *suggerita*
  (`suggestedPersonId`), e l'unione **non le riassegna**: se si unisce Marta dentro un'altra
  persona, le sue 14 proposte restano con `status:'pending'` e un suggerimento verso una persona
  che non esiste più. `pendingFaceGroups()` le scarta (`filter(g=>g.person && …)`), quindi
  **spariscono dalla coda**, ma `pendingFaceCount()` continua a contarle: il badge dice 23 mentre
  la pagina ne mostra 9. È un difetto, non una scelta.
- Nulla viene fatto sulle **foto**: unire persone non tocca in alcun modo file, tag o album.

---

## 36. Dialog "separa persona"

### 1. Nome e scopo
`openSplitPersonDialog` (riga 2874, preceduto dal commento *"SEPARAZIONE — il pezzo difficile: un
cluster contiene due persone diverse"*): estrarre da una persona i volti che in realtà
appartengono a qualcun altro, creando una persona nuova.

### 2. Cosa mostra
Dialog modale largo **640px** (`.split-card`; su mobile 94% della larghezza), con:
- titolo `"Dividi <nome persona>"`;
- sottotitolo `"Questo gruppo contiene due persone diverse? Seleziona i volti che vanno estratti
  in una persona a parte — gli altri restano qui."`;
- **banda di suggerimento dell'IA**, solo se almeno un volto ha `subCluster===1`: icona `info`
  13px e testo `"L'IA pensa che i volti evidenziati potrebbero essere di una persona diversa —
  già preselezionati, controllali prima di confermare."` (sfondo `--accent-tint`, testo accento,
  12px, interlinea 1.4);
- **griglia dei volti a 8 colonne** (`.split-face-grid`, `gap:6px`, altezza massima 280px con
  scorrimento; **4 colonne su mobile**), una miniatura quadrata per **ogni volto confermato**
  della persona — non per ogni foto: se in una foto ci sono due volti della stessa persona,
  compaiono due miniature identiche. Ogni miniatura ha in alto a destra una casella di spunta
  17×17;
- **riga di conteggio**: `"<b>N</b> volti selezionati da estrarre — M restano con \"<nome>\""`
  (singolare/plurale `volto`/`volti`, `selezionato`/`selezionati`);
- **avvertimento rosso**, mostrato solo quando sono selezionati **tutti** i volti:
  `"Non puoi estrarli tutti: non resterebbe nessun volto con questo nome."` (stile
  `.rename-collision-warning`: fondo rosso 10%, bordo rosso 30%, testo `--danger`);
- **campo di testo** con etichetta `"Nome della nuova persona"` seguita, in peso normale e colore
  terziario, da `"(opzionale, puoi nominarla anche dopo)"`; placeholder `"Senza nome"`;
- due pulsanti: `"Dividi in una nuova persona"` (primario, icona `copy` 13px) e `"Annulla"`.

### 3. Ogni controllo, uno per uno
| # | Elemento | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Miniatura di un volto | riquadro a selezione multipla (`data-splitface`) | Aggiunge/toglie il volto dall'insieme da estrarre; l'intero corpo del dialog viene ridisegnato (conteggio, avvertimento, stato del pulsante). |
| 2 | Campo `"Nome della nuova persona"` | campo di testo | Nome della persona che nascerà. **Facoltativo**: se vuoto, la nuova persona nasce senza nome e nella griglia appare come `"Persona <numero>"` con l'indicazione `da nominare`. Nessuna validazione, nessun controllo di omonimia. Il valore viene ripulito degli spazi (`trim()`) prima dell'uso. |
| 3 | `"Dividi in una nuova persona"` | pulsante primario | Esegue la separazione, chiude, toast `"N volti estratti in una nuova persona."` Si **disabilita** (vedi §7) quando la selezione è vuota o totale. |
| 4 | `"Annulla"` | pulsante fantasma | Chiude senza modifiche. |

**Preselezione**: se l'IA ha un sospetto (`subCluster===1` su almeno un volto) quei volti sono
**già selezionati** all'apertura *e* evidenziati con un doppio anello
(`box-shadow: 0 0 0 2px var(--accent-tint), 0 0 0 3px var(--accent)`). Il commento nel codice
spiega il perché: *"l'utente controlla, non subisce"*. Con i dati demo questo accade solo per
**Chiara** (9 volti su 31). Per tutte le altre persone il dialog si apre con **zero** volti
selezionati e senza banda di suggerimento.

Non è previsto: un "seleziona tutto"/"inverti selezione", un ingrandimento della miniatura, una
distinzione visiva fra volti riconosciuti dall'IA e volti aggiunti a mano, un ordinamento.

### 4. Interazioni da mouse
Click su una miniatura per selezionarla/deselezionarla (nessun `Shift+click` per intervalli,
nessun trascinamento a lazo — non previsti nel mockup). Click su Dividi/Annulla. Click nel campo
di testo. Scorrimento della griglia con la rotellina quando supera 280px. Lo scrim non chiude.
Nessun tasto destro.

### 5. Interazioni da tastiera
- Le miniature hanno `bindActivatable` (Invio/Spazio) ma **non hanno `tabindex` né `role`**:
  come nel dialog delle copertine, di fatto **non sono raggiungibili col Tab**.
- Il campo di testo è un `input` normale: Tab lo raggiunge, si scrive liberamente. **Invio non
  conferma** la divisione (a differenza del dialog di testo generico, dove Invio equivale al
  pulsante di conferma).
- `nameInput.onkeydown` fa `stopPropagation()`: serve a impedire che i tasti digitati nel campo
  arrivino ai gestori globali di scorciatoie.
- Escape chiude; il focus torna al trigger. Nessun focus iniziale, nessun focus trap.
- **Problema noto:** ogni click su una miniatura ricostruisce l'intero corpo del dialog via
  `innerHTML`, quindi il campo del nome viene ricreato — il testo digitato è conservato (è tenuto
  in `draft.name`) ma **il focus e la posizione del cursore vanno persi**.

### 6. Animazioni e transizioni
Nessuna transizione dichiarata su `.split-face-thumb`, `.split-face-check`, `.split-hint-banner` o
sulla comparsa del dialog: bordi e spunte cambiano istantaneamente. Toast SP-6.

### 7. Stati per ogni controllo
- **Miniatura**: normale (bordo trasparente 2px) / **suggerita dall'IA** (doppio anello accento,
  permanente finché il dialog è aperto, indipendente dalla selezione) / **selezionata** (bordo
  accento + casella `.on` piena di accento con spunta). Le due condizioni si sommano.
- **Casella di spunta della miniatura**: sempre visibile (non compare al passaggio del mouse come
  nelle tessere foto), fondo nero al 40% quando non selezionata.
- **Pulsante `"Dividi in una nuova persona"`**: disabilitato quando `N===0` **o** `N===tutti`,
  reso con `opacity:.4; pointer-events:none` inline + `aria-disabled="true"` (non è un `<button>`
  con attributo `disabled`). All'apertura senza suggerimento IA è quindi **disabilitato** (zero
  selezionati); il motivo del secondo caso è spiegato dall'avvertimento rosso.
- **Avvertimento rosso**: visibile solo nel caso "tutti selezionati" (`n>0 && invalid`).
- **Campo nome**: normale / focus (anello accento). Mai in errore.
- **Stato vuoto**: non esiste: con meno di due volti il dialog non si apre proprio e compare il
  toast `"Servono almeno due volti per poter dividere questa persona."`

### 8. Da dove ci si arriva e dove si va
**Unico ingresso:** pulsante `"Dividi…"` nel dettaglio persona. **In uscita:** si torna al
dettaglio della persona **originale** (che ora ha meno foto). La persona appena creata **non
viene aperta automaticamente**: va cercata nella griglia, nel blocco "Senza gruppo".

### 9. Dati necessari
**Legge:** l'elenco dei volti confermati della persona, ciascuno con la miniatura della foto e il
sospetto dell'IA di appartenere a un sotto-gruppo diverso; il nome della persona di partenza.
**Scrive:** crea una nuova persona (nome facoltativo, nessun gruppo, nessuna copertina) e le
riassegna i volti scelti.

### Cosa comporta esattamente separare (e se è reversibile)
`splitPersonFaces(personId, faceIds, newName)` (riga 1895):
1. crea una persona nuova, con il nome digitato **oppure vuoto**, `groupId:null`,
   `hidden:false`, `coverFaceId:null` e un nuovo numero automatico progressivo;
2. sposta su di lei **solo i volti selezionati** che appartenevano davvero alla persona di
   partenza;
3. la persona originale **mantiene nome, gruppo, copertina** e i volti restanti.

- **Reversibile solo a mano, e non del tutto**: si può riunire la persona nuova con l'originale
  dalla griglia (selezione multipla → Unisci), scegliendo come sopravvissuta l'originale. Si
  torna così alla situazione di partenza *nei fatti* — ma se la copertina dell'originale era uno
  dei volti estratti, `coverFaceId` continua a puntare a un volto che nel frattempo era di
  un'altra persona; `personCoverPhoto()` in quel caso **ricade sul primo volto disponibile** senza
  errori, ma la scelta esplicita dell'utente è persa.
- Nessuna conferma di secondo livello, nessun preavviso di irreversibilità: la separazione è
  considerata un'operazione a basso rischio proprio perché ricomponibile.
- Il suggerimento dell'IA (`subCluster`) resta scritto sui volti anche dopo la separazione: se si
  riapre "Dividi…" sulla persona nuova, e i suoi volti hanno tutti `subCluster:1`, non compare
  alcuna banda perché la condizione richiede che *almeno uno* l'abbia — di fatto sulla nuova
  persona la banda **ricompare** e li preseleziona tutti, portando subito allo stato "non puoi
  estrarli tutti". Incoerenza minore da segnalare.

---

## 37. Selettore di persona (usato per assegnare un volto)

### 1. Nome e scopo
`openPersonPickerDialog(onPick)` (riga 2945): scegliere una persona esistente — o crearne una
nuova al volo — per assegnarla a un volto o a una foto. Il commento nel codice lo definisce
*"assegnazione manuale/correzione"*.

### 2. Cosa mostra
Dialog modale standard (SP-5, 360px):
- titolo `"Assegna persona"`;
- **campo di ricerca** con etichetta per soli lettori di schermo `"Cerca o crea una persona"` e
  placeholder `"Cerca o crea una persona…"`, `autocomplete="off"`;
- **elenco** (max 260px, scorrevole):
  - come **prima riga**, solo se si è digitato qualcosa che non corrisponde **esattamente** al
    nome di una persona esistente: `"Crea persona «<testo digitato>»"` con icona `plusUser` 16px;
  - poi una riga per ogni persona che corrisponde alla ricerca: nome (o `"Persona N"`) e a destra
    `"N foto"` in 11px terziario;
  - se non c'è alcun risultato (e non si sta proponendo la creazione): `"Nessuna persona trovata."`
    (`.region-search-empty`, 12.5px terziario);
- pulsante `"Annulla"`.

**Chi entra nell'elenco:** `visiblePeople()` — quindi **niente persone nascoste** e niente persone
senza volti confermati. La ricerca è per **sottostringa**, senza distinzione fra maiuscole e
minuscole, sul nome mostrato (quindi trova anche `"persona 12"`).

### 3. Ogni controllo, uno per uno
| # | Elemento | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Campo di ricerca | campo di testo | Filtra l'elenco a ogni battuta (`oninput`, nessun ritardo/antirimbalzo). Vuoto = mostra tutte le persone visibili. Nessuna validazione. |
| 2 | `"Crea persona «X»"` | riga cliccabile | Crea subito una persona con quel nome (senza gruppo, non nascosta, senza copertina, con numero automatico assegnato comunque), chiude il dialog e la restituisce al chiamante. **Non chiede conferma e non impedisce gli omonimi parziali** (il controllo `exactMatch` nasconde la riga solo su corrispondenza esatta). |
| 3 | Riga di una persona | riga cliccabile | Chiude e restituisce quella persona al chiamante. |
| 4 | `"Annulla"` | pulsante fantasma | Chiude senza scegliere: il callback **non** viene invocato. |

**I due chiamanti** (comportamenti diversi a valle):
- dal **menu sul riquadro del volto**, voce `"Correggi persona…"` → riassegna quel volto
  (`face.personId = nuova`), toast `"Persona corretta."`;
- dal chip `"+ aggiungi"` della sezione **"Persone"** del pannello informazioni della foto →
  `addManualFaceToPhoto()` crea un volto **senza riquadro** (`box:null`) già confermato, toast
  `"Persona aggiunta."` Il commento a riga 1829 spiega il perché: *"nessun rilevamento automatico
  dietro: niente box, la foto non nasce da un'analisi ma da una scelta umana"*. Conseguenza
  pratica: quel volto **non ha un riquadro sulla foto** e non è quindi apribile dall'immagine, ma
  compare come chip nel pannello.

### 4. Interazioni da mouse
Click su una riga (sceglie e chiude); click su `"Crea persona «…»"`; click su `"Annulla"`; hover
riga `var(--chip-bg)`. Lo scrim non chiude. Nessun tasto destro, nessun trascinamento.

### 5. Interazioni da tastiera
- **Il campo di ricerca riceve il focus all'apertura** (`input.focus()`) — è l'unico dialog di
  questo blocco a farlo.
- Le righe sono `role="button" tabindex="0"` + SP-8: Invio/Spazio scelgono.
- **Invio nel campo di ricerca non fa nulla**: non seleziona il primo risultato e non crea la
  persona; bisogna raggiungere la riga con il Tab. Nessuna navigazione con ↑/↓ fra i risultati.
- Escape chiude; il focus torna al trigger.
- Ogni battuta ricostruisce l'elenco con `innerHTML`: se il focus era su una riga, si perde (ma di
  norma il focus è nel campo, che non viene ricreato).

### 6. Animazioni e transizioni
Nessuna: comparsa istantanea del dialog, filtro istantaneo dell'elenco, hover senza durata
dichiarata. Toast SP-6 a valle della scelta.

### 7. Stati per ogni controllo
- Campo: normale / focus (anello accento 2.5px). Mai in errore, mai disabilitato.
- Riga persona: normale / hover / focus-visible. Nessuno stato "già assegnata a questa foto":
  è possibile scegliere una persona **già presente** nella foto, creando un secondo volto per lei.
- Riga "Crea persona": compare/scompare in base al testo digitato; non ha stato disabilitato.
- Stato vuoto: `"Nessuna persona trovata."` — mostrato solo quando il testo digitato corrisponde
  esattamente a una persona esistente ma questa non è visibile, oppure quando non esiste alcuna
  persona: negli altri casi al suo posto c'è la riga di creazione.
- Nessuno stato di caricamento (elenco locale).

### 8. Da dove ci si arriva e dove si va
**In ingresso:** voce `"Correggi persona…"` del menu sul riquadro del volto; chip `"+ aggiungi"`
della sezione "Persone" nel pannello informazioni del lightbox. **In uscita:** torna alla
schermata chiamante (lightbox), ridisegnata, con il volto riassegnato o aggiunto.
Non è raggiungibile dalla griglia Persone né dal dettaglio persona.

### 9. Dati necessari
**Legge:** elenco delle persone visibili con nome/etichetta e numero di foto.
**Scrive:** eventualmente **crea** una persona nuova; e — tramite il chiamante — riassegna un
volto esistente a un'altra persona, oppure crea un volto manuale (senza riquadro) su una foto.

---

## 38. Menu sul riquadro del volto

### 1. Nome e scopo
`openFaceBoxMenuDialog(face)` (riga 3008): decidere cosa fare di un singolo volto già confermato
su una foto — andare alla persona, correggere l'attribuzione, o dichiararlo un falso positivo.

### 2. Cosa mostra
Dialog modale standard (SP-5, 360px), **non** un menu a comparsa ancorato (SP-14):
- **titolo = il nome della persona** attualmente attribuita al volto (`personDisplayName`, quindi
  anche `"Persona 12"`);
- tre opzioni in stile `.modal-option` (riquadro con bordo, titolo in grassetto su riga propria e
  descrizione sotto in 12.5px secondario):
  1. `"Vai alla persona"` — `"Apre tutte le sue foto"`;
  2. `"Correggi persona…"` — `"Questo volto appartiene a qualcun altro"`;
  3. `"Non è un volto"` — `"Falso positivo — non verrà mai più riproposto"` (variante `danger`:
     titolo rosso, sfondo rosso tenue su hover);
- pulsante `"Annulla"` (fantasma, piccolo).

Non mostra: la miniatura del volto, la foto, la confidenza dell'IA, la data, né la possibilità di
staccare il volto senza attribuirlo.

### 3. Ogni controllo, uno per uno
| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Vai alla persona"` | opzione | Chiude il menu, **chiude il lightbox** (`state.lightbox=null`), passa alla vista Persone e apre il dettaglio di quella persona. |
| 2 | `"Correggi persona…"` | opzione | Chiude il menu e apre il **selettore di persona** (sezione 7). Alla scelta: `face.personId` cambia, toast `"Persona corretta."` La persona precedente **non viene toccata** (se resta senza volti sparisce dalla griglia da sola). |
| 3 | `"Non è un volto"` | opzione pericolosa | Chiude il menu, toast `"Segnato come \"non è un volto\" — non verrà più riproposto."` **Senza dialog di conferma**, benché il testo dica che è permanente. |
| 4 | `"Annulla"` | pulsante fantasma | Chiude. |

**Le due negazioni sono diverse** ed è il punto concettuale da riportare all'architetto
(commenti alle righe 1866 e 1877):
- **`"Non è un volto"`** (`rejectFaceAsNotAFace`) = falso positivo — *"un disegno, un volto su un
  poster"*: il rilevamento passa a `status:'not_face'`, perde persona e suggerimento, **sparisce e
  basta**; non nasce nessuna persona nuova, non torna in nessuna coda. Immediato e permanente.
- **`"Non è <persona>"`** (`rejectFaceMatch`, disponibile solo nella coda di revisione, sezione 9)
  = il volto è vero, l'attribuzione no: diventa un **cluster nuovo senza nome**, che finisce fra
  le persone senza nome, dove potrà poi essere unito a quella giusta.

### 4. Interazioni da mouse
Il menu si apre con un **click sul riquadro bianco sovrapposto al volto** nella foto
(`data-facebox`, riga 4207) oppure con un click sul **chip col nome** nella sezione "Persone" del
pannello informazioni (`data-facechip`, riga 4210); entrambi fermano la propagazione per non
innescare i gestori del lightbox.
Il riquadro sulla foto **è invisibile a riposo** (`opacity:0; pointer-events:none`): compare solo
passando il mouse (o portando il focus) sul chip col nome corrispondente, o restandoci sopra —
comportamento gestito da `wireLbFaceHover` (riga 4089) con **200 ms di tolleranza** prima di
nasconderlo, così si può spostare il puntatore dal chip al riquadro senza perderlo. Il commento a
riga 509 spiega la scelta: i riquadri "restano invisibili per non ingombrare la foto".
Hover sul riquadro visibile: il bordo passa da bianco all'85% al colore accento.
Nessun tasto destro (nemmeno qui: è un click sinistro che apre il menu), nessun trascinamento del
riquadro per correggerne la posizione, nessun ridimensionamento.

### 5. Interazioni da tastiera
- I **chip dei nomi** nel pannello informazioni sono `role="button" tabindex="0"` + SP-8: Invio o
  Spazio aprono lo stesso menu; portandovi il focus si accende anche il riquadro sulla foto
  (`focus` → `show`). È l'unico modo per raggiungere questo menu da tastiera: i **riquadri sulla
  foto hanno solo un `onclick`**, senza `tabindex`.
- Nel menu: le tre opzioni sono `role="button" tabindex="0"` + SP-8; `"Annulla"` idem.
- Escape chiude il menu; il focus torna al chip/riquadro di partenza. Nessun focus iniziale,
  nessun focus trap, nessuna navigazione con le frecce.
- Nota: quando il lightbox è aperto, il gestore globale (riga 6289) intercetta comunque `Escape`,
  `←`, `→`, `i`, `f`; il menu ha però il proprio gestore di `Escape` registrato dopo.

### 6. Animazioni e transizioni
- Riquadro volto: `transition: opacity .12s ease, border-color .12s ease` — comunica il legame
  fra il nome nel pannello e la porzione di immagine, senza sporcare la foto.
- Ritardo di **200 ms** prima di nascondere il riquadro quando il mouse esce (timer JS, non CSS).
- Il dialog compare senza animazione. Toast SP-6.
- L'etichetta col nome sotto il riquadro (`.lb-face-label`, 10px/700, fondo nero al 65%, larghezza
  massima 160px con ellissi) non ha animazione propria: appare e sparisce col riquadro.

### 7. Stati per ogni controllo
- Riquadro volto: **nascosto** (predefinito) / **visibile** (`.face-hint-visible`, per hover o
  focus del chip) / hover (bordo accento).
- Opzioni del menu: normale / hover (`var(--chip-bg)`; la terza `var(--danger-tint)`) /
  focus-visible. Nessuna è mai disabilitata.
- Il menu **non è mai disponibile** se il riconoscimento volti è spento (né riquadri né chip
  vengono disegnati) o se la foto è di culling (`p.batchId`), perché le foto di culling non hanno
  volti riconosciuti (commento a riga 1725).

### 8. Da dove ci si arriva e dove si va
**In ingresso:** riquadro volto sull'immagine nel lightbox; chip del nome nella sezione "Persone"
del pannello informazioni della foto. **In uscita:** dettaglio persona (prima voce, con chiusura
del lightbox); selettore di persona (seconda voce); ritorno al lightbox aggiornato (terza voce e
Annulla).

### 9. Dati necessari
**Legge:** per il volto scelto, la persona a cui è attribuito (nome o etichetta automatica).
**Scrive:** riassegna il volto a un'altra persona, oppure lo marca definitivamente come "non è un
volto" (rimuovendone persona e suggerimento).

---

## 39. Revisione — volti (la coda di conferma dei volti suggeriti)

### 1. Nome e scopo
`renderRevisioneVolti` (riga 5811): la linguetta **Volti** della pagina Revisione, dove l'utente
conferma o rifiuta i volti che l'IA sospetta appartengano a una persona già nominata, ma con
confidenza troppo bassa per assegnarli da sola.

### 2. Cosa mostra
- In cima, il selettore a due linguette comune alla pagina (`.seg-control`, `role="radiogroup"`,
  `aria-label="Cosa revisionare"`): `"Tag"` e `"Volti"`; la seconda porta il conteggio fra
  parentesi quando ci sono proposte, es. `"Volti (23)"`.
- Titolo `"Revisione volti"` e sottotitolo
  `"N proposte in attesa, raggruppate per persona — nessuna è ancora confermata"`.
- **Un riquadro per ogni persona suggerita** (`.review-group`: bordo 1px, raggio 12, padding 14):
  - testata: `"Questi volti sembrano <b>Nome</b>"` seguito da `"N proposta"` / `"N proposte"` in
    12px terziario;
  - a destra due pulsanti: `"Conferma tutte"` (pieno, icona `check` 13px) e `"Rifiuta tutte"`
    (fantasma);
  - una **striscia di miniature** (`.suggestion-strip`, va a capo, `gap:8px`), una per proposta:
    quadrato **86×86** (variante `.triple`, più larga della versione dei tag perché deve ospitare
    **tre** pulsanti invece di due — commento a riga 711), **bordo tratteggiato color accento**
    (`1.5px dashed`, opacità .92) che segnala "non ancora confermato", e in alto a sinistra il
    **badge `"IA"`** (8.5px/700, fondo `--accent-tint-strong`, testo accento).
- Con i dati demo: due riquadri — **Marta** con 14 proposte e **Luca** con 9.

**Come è indicata la confidenza dell'IA:** **non è indicata in alcun modo numerico.** Non c'è
percentuale, non c'è barra, non c'è ordinamento per confidenza, non c'è soglia regolabile (a
differenza dei tag, che hanno una soglia percentuale configurabile nel loro editor). L'unica
segnalazione è **qualitativa**: badge `"IA"` + bordo tratteggiato + la formula prudente
`"Questi volti sembrano X"` e `"nessuna è ancora confermata"`. Nel modello dati la confidenza non
esiste proprio: un volto in coda ha solo `status:'pending'` e `suggestedPersonId`.

### 3. Ogni controllo, uno per uno
| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Tag"` / `"Volti (N)"` | selettore a due linguette | Cambia `state.revisioneTab`. La linguetta attiva ha `tabindex=0`, l'altra `-1`. |
| 2 | `"Conferma tutte"` | pulsante pieno, **per gruppo** | Conferma in blocco tutte le proposte di quella persona: ogni volto passa a `confirmed` con `personId = suggestedPersonId`. Toast `"N proposte confermate."` |
| 3 | `"Rifiuta tutte"` | pulsante fantasma, **per gruppo** | Rifiuta in blocco: **ogni volto diventa una persona nuova a sé**, senza nome (vedi §7). Toast `"N proposte rifiutate."` |
| 4 | Spunta bianca (`mini-btn confirm`) sulla miniatura | pulsante tondo 24px, `aria-label="Conferma"` | Conferma quella singola proposta. Toast `"Volto confermato."` |
| 5 | ✕ rossa (`mini-btn reject`) | pulsante tondo 24px, `aria-label="Non è <nome persona>"` | Rifiuta l'attribuzione: il volto resta un volto ma diventa una persona nuova senza nome. Toast `"Proposta rifiutata — il volto resta tra le persone senza nome."` |
| 6 | Cestino su fondo rosso (`mini-btn notface`) | pulsante tondo 24px, `aria-label="Non è un volto"` | Falso positivo, permanente. Toast `"Segnato come \"non è un volto\" — non verrà più riproposto."` |

**Sì, c'è l'azione in blocco**, ed è **per persona suggerita**, non globale: non esiste un
"conferma tutto" per l'intera coda, né una selezione multipla di singole miniature, né un annulla
dell'ultima azione. Le azioni in blocco coprono solo due delle tre azioni: **"non è un volto" non
ha una versione in blocco** (è deliberato: è l'azione permanente).

Nessuna conferma di secondo livello per nessuna delle sei azioni. Non c'è paginazione: tutte le
proposte del gruppo sono mostrate insieme.

### 4. Interazioni da mouse
- **Hover su una miniatura** → compare in dissolvenza un velo nero al 50% con i tre pulsanti
  tondi (`opacity 0 → 1`, `.12s ease`). È l'**unico** modo, col mouse, di vedere le azioni per
  singola proposta.
- Click su uno dei tre pulsanti tondi; click sui due pulsanti di gruppo; click sulle linguette.
- **Click sulla miniatura fuori dai tre pulsanti: non fa nulla** — non apre la foto, non apre il
  lightbox, non ingrandisce. Per vedere il contesto della foto bisogna uscire dalla coda.
- Nessun tasto destro, nessun trascinamento, nessun doppio click.

### 5. Interazioni da tastiera
- Linguette, pulsanti di gruppo e i tre pulsanti tondi sono tutti `role="button" tabindex="0"` +
  SP-8 (Invio/Spazio).
- Il velo con i tre pulsanti compare anche col **focus da tastiera**, grazie a
  `.suggestion-thumb:focus-within .suggestion-hover{opacity:1}` — quindi la coda è percorribile a
  Tab: per ogni proposta si incontrano nell'ordine conferma, rifiuta, non-è-un-volto.
- Nessuna scorciatoia a lettera singola, nessuna freccia direzionale, nessun `Escape` dedicato.
- Ogni azione ridisegna l'intera pagina (`renderAll()`): **il focus si perde** e torna all'inizio
  del documento. Confermando una proposta alla volta da tastiera, si deve ricominciare la
  tabulazione ogni volta — motivo pratico per cui esistono le azioni in blocco.

### 6. Animazioni e transizioni
- Velo delle azioni sulla miniatura: `transition: opacity .12s ease`, innescato da hover o da
  focus interno. Comunica "questa proposta è azionabile ora" tenendo pulita la griglia.
- Bordo tratteggiato accento sulla miniatura: statico, non animato — comunica "in attesa".
- Nessuna animazione di uscita: la proposta confermata o rifiutata **sparisce di colpo** al
  ridisegno, senza dissolvenza né scorrimento delle altre.
- Toast SP-6.

### 7. Stati per ogni controllo
- **Miniatura**: unico stato "in attesa" (bordo tratteggiato + badge IA); hover/focus-within
  mostrano il velo. Non esiste uno stato "appena confermata" o "in corso".
- **Pulsanti tondi**: normale (fondo bianco, icona scura), la ✕ ha icona `--danger`, il cestino ha
  fondo `--danger` pieno e icona bianca. Nessuno stato disabilitato.
- **Pulsanti di gruppo**: sempre attivi finché il gruppo esiste.
- **Stato vuoto (coda finita)**: icona `inbox`, titolo `"Nessuna proposta in attesa"`, testo
  `"Quando l'IA troverà volti che sembrano corrispondere a una persona già nominata, appariranno
  qui per la tua conferma — mai applicati da soli."`
- **Stato riconoscimento volti spento**: icona `user`, titolo `"Riconoscimento volti
  disattivato"`, testo `"Riattivalo da Impostazioni → Riconoscimento volti per vedere le proposte
  in attesa."` Le linguette restano visibili — e, come segnalato nella sezione 1, **la linguetta
  "Volti" continua a mostrare il conteggio** anche in questo stato.
- Nessuno stato di caricamento né di errore.

**Cosa fa esattamente ciascuna delle tre azioni** (righe 1862–1883):
- **Conferma** (`confirmFaceMatch`): `personId = suggestedPersonId`, `status='confirmed'`. Il
  volto entra fra le foto di quella persona, il conteggio della persona sale.
  *Il campo `suggestedPersonId` non viene azzerato* — dettaglio irrilevante per l'interfaccia ma
  utile all'architetto.
- **Rifiuta** (`rejectFaceMatch`): nasce una **persona nuova senza nome** (numero automatico
  progressivo, nessun gruppo), il volto le viene assegnato come confermato e il suggerimento è
  cancellato. Il commento è esplicito: *"l'utente potrà poi unirlo a quella giusta con Unione, se
  lo riconosce"*. Conseguenza visibile: **rifiutare in blocco 14 proposte crea 14 persone nuove
  senza nome**, tutte con una sola foto, che compaiono tutte nel blocco "Senza gruppo" della
  griglia Persone. È un effetto collaterale forte, da verificare col committente.
- **Non è un volto** (`rejectFaceAsNotAFace`): `status='not_face'`, persona e suggerimento
  azzerati; sparisce da ogni coda per sempre e non genera nulla.

### 8. Da dove ci si arriva e dove si va
**In ingresso:** barra laterale → gruppo `IA` → `"Revisione"` (con badge che somma proposte tag e
proposte volti), poi linguetta `"Volti"`; oppure direttamente dal **banner** in cima alla griglia
Persone, che imposta vista e linguetta in un colpo solo; da mobile, `"Altro"` → `IA` →
`"Revisione"`.
**In uscita:** linguetta `"Tag"` (stessa pagina); nessun altro collegamento in uscita — in
particolare **non si passa dalla proposta alla persona o alla foto**.

### 9. Dati necessari
**Legge:** l'elenco dei volti in attesa, ciascuno con la persona che l'IA suggerisce e la
miniatura della foto in cui è stato rilevato, raggruppati per persona suggerita; il nome della
persona suggerita; i conteggi per gruppo e totale.
**Scrive:** conferma l'attribuzione di un volto a una persona; oppure crea una nuova persona senza
nome e le assegna il volto; oppure marca il rilevamento come falso positivo permanente. In tutti e
tre i casi la proposta esce dalla coda.

---

## 40. Riferimenti ai pattern condivisi usati in questo blocco

- **SP-1 / SP-2 / SP-3 / SP-4** — solo nel **dettaglio persona**, sulla griglia delle foto della
  persona. La griglia *delle persone* usa una selezione multipla **propria** (`personSelectedIds`,
  barra `personSelectionBarHTML`) con solo due azioni (Unisci, Assegna a gruppo) e **senza**
  "Seleziona tutte", **senza** modalità selezione, **senza** Elimina.
- **SP-5** — tutti i sei dialog del blocco. **Deviazioni comuni a tutti:** nessuno mette il focus
  su un elemento all'apertura (unica eccezione il selettore di persona, che mette il focus nel
  campo di ricerca); nessuno ha un focus trap; **nessuno si chiude cliccando sullo scrim**;
  nessuno ha un'animazione di comparsa.
- **SP-6** — 14 messaggi temporanei distinti, elencati nelle rispettive sezioni.
- **SP-7** — solo sui due pulsanti solo-icona della barra di selezione persone.
- **SP-8** — ovunque ci sia `role="button"` / `role="checkbox"`. **Attenzione alle tre eccezioni**
  dove `bindActivatable` è collegato a elementi *senza* `tabindex` (quindi irraggiungibili col
  Tab): miniature del dialog copertina, miniature del dialog di separazione, e — con solo
  `onclick` — scheda persona, link "Tutte le persone", riquadro volto sulla foto.
- **SP-10** — la coda volti è la seconda istanza del pattern (la prima è quella dei tag); qui con
  **tre** azioni invece di due e conferma/rifiuto in blocco per gruppo.
- **SP-12** — provenienza IA vs utente: sui volti è resa dal badge `"IA"` + bordo tratteggiato
  nelle proposte. **Sui volti già confermati la provenienza non è più visibile**: un volto
  aggiunto a mano e uno riconosciuto dall'IA e confermato appaiono identici (l'unico segno
  indiretto è che quello manuale non ha riquadro sulla foto).
- **SP-14** — il "menu sul riquadro del volto" **non** usa questo pattern: è un dialog modale, non
  un menu a comparsa ancorato.
- **SP-11** — i livelli IA "Pieno"/"Ridotto"/"Spento" governano l'analisi dei tag; il
  riconoscimento volti ha un **interruttore proprio e separato**, che non dipende da `aiTier`.
- **SP-16** — gli avatar con le iniziali su fondo colorato appartengono agli **utenti** della
  condivisione, non alle **persone** fotografate: queste hanno sempre una foto come avatar (o un
  cerchio vuoto col solo anello di bordo se non c'è copertina).
- **SP-13, SP-9, SP-15, SP-17** — non usati direttamente in questo blocco (SP-15 e SP-9 compaiono
  solo dentro le tessere foto del dettaglio persona, tramite SP-1).

---

# Parte VII — Album e manutenzione

Questo blocco copre le raccolte curate (**Album**, con la loro creazione guidata) e le tre pagine
di **manutenzione della libreria** (**Cestino**, **Duplicati**, **Problemi**), più i dialog
riutilizzabili che le attraversano: il **dialog di eliminazione a 3 opzioni** (§9 — definizione
canonica, richiamata da tutte le altre schermate), il dialog **"Aggiungi ad album"**, il dialog
**"file con problemi"** e i due dialog generici **informazione** e **conferma**.

Nota di collocazione, presa dal commento a `NAV_MAINT` (riga 2167): *"Cestino/Duplicati/Problemi
non sono raccolte curate come Album/Preferiti: sono tutte pagine di 'manutenzione' della libreria.
Raggruppate sotto una singola voce a scomparsa invece che tre righe fisse sempre visibili — con la
sidebar già a 680px di altezza fissa, ogni riga fissa in più la fa traboccare in una scrollbar
interna (è esattamente quello che è successo aggiungendo Duplicati come riga a sé)."*

---

## 41. Album — la griglia

### 1. Nome e scopo

Pagina d'ingresso agli album: mostra tutte le raccolte esistenti come schede con copertina
colorata, e offre il punto di partenza per crearne una nuova.

### 2. Cosa mostra

Intestazione (`.album-toolbar`, riga flex con titolo a sinistra e pulsante a destra):

- titolo `"Album"` (`.section-title`);
- sottotitolo `"Selezioni curate, trasversali alle cartelle"` (`.section-sub`);
- pulsante primario `"Crea album"` con icona `plus` a 14px.

Poi una griglia (`.album-grid`, `repeat(auto-fill,minmax(190px,1fr))`, gap 16px) con una scheda
per album. Ogni scheda (`.album-card`) mostra:

- **copertina** (`.album-cover`, altezza fissa 120px): non è una miniatura reale, è un gradiente
  generato. Se l'album ha `mono:true` → `linear-gradient(135deg,#a8a8a8,#3a3a3a)` (grigio, per
  l'album "Bianco e nero"); altrimenti
  `linear-gradient(135deg, hsl(hue,40%,55%), hsl(hue+18,35%,32%))` dove `hue` è una proprietà
  dell'album;
- **badge `"condiviso"`** in alto a **destra** (`.album-shared-badge`, icona `share` 10px, sfondo
  `rgba(10,10,10,.55)`, testo bianco 10px bold) — solo se `a.shared`;
- **badge `"dinamico"`** in alto a **sinistra** (`.album-dynamic-badge`, icona `funnel` 10px,
  stesso stile) — solo se `a.dynamic`. Ha un attributo `title` nativo:
  `"Album dinamico: l'appartenenza si aggiorna da sola in base al filtro"`;
- **nome dell'album** (`.album-title`, 13.5px bold);
- **riga di sottotitolo** (`.album-sub`, 11.5px, testo terziario) nel formato
  `"<N> foto · <intervallo>"`. `N` è il numero di membri calcolato **in tempo reale** da
  `albumMembers(a)`. L'intervallo viene da `albumRangeLabel()`:
  - album **non dinamico** → la stringa statica `a.range` (es. `"Gen 2026 – Lug 2026"`);
  - album **dinamico senza corrispondenze** → `"nessuna foto corrisponde"`;
  - album **dinamico con corrispondenze** → intervallo calcolato dai mesi delle foto, es.
    `"Marzo 2026 – Luglio 2026"`, oppure un solo mese (`"Luglio 2026"`) se tutte le foto cadono
    nello stesso mese.

Album di partenza nel mockup (`ALBUMS`, righe 1330–1335): `"Migliori scatti 2026"` (condiviso),
`"Tramonti"`, `"Ritratti"`, `"Bianco e nero"` (condiviso, mono). Nessuno dei quattro è dinamico:
i dinamici possono nascere solo dalla creazione guidata.

Non esiste uno stato vuoto per questa pagina: `ALBUMS` è pre-popolato e non c'è un modo per
eliminare un album (vedi §3).

### 3. Ogni controllo, uno per uno

| # | Elemento | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Crea album"` | pulsante primario piccolo (`.btn.btn-primary.btn-sm`, `role="button"`, `tabindex="0"`) | Crea una bozza vuota (`freshAlbumDraft()`) e porta alla vista `createAlbum` |
| 2 | Scheda album (una per album) | area cliccabile (`div[data-openalbum]`) | Imposta `state.openAlbum` all'id dell'album e ricarica: si finisce nel dettaglio dell'album |

Sono **solo due** i tipi di controllo. In particolare **non sono previsti nel mockup**: rinomina
album, eliminazione album, duplica album, riordino/drag&drop delle schede, ordinamento della
griglia, menu contestuale sulla scheda, ricerca fra gli album, cambio copertina.

### 4. Interazioni da mouse

- **Click sulla scheda** → apre il dettaglio dell'album. Tutta la scheda è cliccabile
  (`cursor:pointer` su `.album-card`), copertina e corpo compresi.
- **Click su "Crea album"** → vista di creazione.
- **Hover sulla scheda**: nessuna regola CSS `:hover` su `.album-card` — la scheda non cambia
  aspetto al passaggio del mouse. È l'unica affordance mancante rispetto al resto dell'app.
- **Hover sul badge "dinamico"**: compare il tooltip **nativo del browser** (attributo `title`,
  quindi con il ritardo del sistema operativo, ~1s, non configurabile), **non** il tooltip
  `[data-tip]` di SP-7.
- **Hover su "Crea album"**: `filter:brightness(1.05)` (regola `.btn-primary:hover`), senza
  transizione dichiarata → istantaneo.
- **Doppio click, tasto destro, trascinamento, rotellina**: nessun comportamento specifico
  implementato.

### 5. Interazioni da tastiera

- `"Crea album"` è legato con `bindActivatable` → **Invio** e **Spazio** lo attivano (SP-8);
  è raggiungibile con Tab (`tabindex="0"`).
- **Le schede album NON sono raggiungibili da tastiera**: `<div class="album-card"
  data-openalbum="…">` non ha né `role` né `tabindex`, ed è legato con `el.onclick` puro (riga
  4757) invece che con `bindActivatable`. Chi naviga da tastiera non può quindi aprire un album
  da questa pagina. È un difetto di accessibilità, non una scelta.
- Nessuna navigazione con le frecce dentro la griglia.
- Ordine del focus: barra superiore → sidebar → `"Crea album"` → (le schede vengono saltate).
- Nessuna scorciatoia dedicata a questa vista nel gestore globale (riga 6289).

### 6. Animazioni e transizioni

Nessuna. Non c'è transizione né animazione dichiarata su `.album-card`, `.album-cover`,
`.album-grid` o sui badge. La griglia appare istantaneamente al render. L'unica variazione visiva
è il `brightness(1.05)` istantaneo del pulsante primario in hover.

### 7. Stati per ogni controllo

- **"Crea album"** — normale: sfondo accento, testo `--accent-text`. Hover: `brightness(1.05)`.
  Focus: `outline:2.5px solid var(--accent); outline-offset:2px` (regola comune ai
  `[role="button"]`). Premuto: nessuno stile dedicato. **Mai disabilitato**, mai in caricamento.
- **Scheda album** — normale: bordo `--border`, sfondo `--card-bg`, `overflow:hidden`. Nessuno
  stato hover, focus, attivo o selezionato. Un album con 0 foto si presenta identico agli altri,
  solo con `"0 foto"` nel sottotitolo.
- **Stato vuoto della pagina**: non implementato (vedi §2).
- **Stato di caricamento**: non implementato — i dati sono sincroni.

### 8. Da dove ci si arriva e dove si va

**In ingresso:**
- sidebar desktop, voce `"Album"` del gruppo Libreria (`NAV_LIB`); il click azzera anche
  `state.openAlbum`, `state.openPerson`, i filtri rapidi e il menu utente;
- tab bar mobile, tab `"Album"` (che azzera anch'essa `state.openAlbum`);
- ritorno dal dettaglio di un album (link `"Tutti gli album"`);
- ritorno dalla creazione album, sia con `"Annulla"` sia dopo aver creato l'album.

**In uscita:**
- click su una scheda → dettaglio album (§2);
- `"Crea album"` → creazione album (§3);
- qualsiasi altra voce di sidebar / tab bar.

Breadcrumb della topbar: `"<b>Album</b>"` quando `state.openAlbum` è nullo.

### 9. Dati necessari a questa schermata

**Legge**, per ogni album: identificativo, nome, se è condiviso, se è dinamico, se è monocromatico,
la tinta della copertina, l'intervallo di date testuale (per gli album manuali) e l'insieme degli
identificativi delle foto che ne fanno parte (per contarle). Per gli album dinamici legge invece
le condizioni del filtro e l'operatore (tutte / almeno una), e per calcolare i membri ha bisogno
dell'intero catalogo con: cartella, data (giorno e mese), fotocamera, obiettivo, tipo di file
(RAW / RAW+JPEG / JPEG), preferito sì/no, valutazione, stato pick/scarta.

**Scrive**: nulla. L'unica scrittura indiretta è il passaggio alla vista di creazione, che
inizializza la bozza di nuovo album.

---

## 42. Album — dettaglio

### 1. Nome e scopo

Mostra le foto contenute in un album, con gli stessi strumenti di griglia del resto dell'app
(filtro rapido, selezione multipla, azioni di massa).

### 2. Cosa mostra

- **Link di ritorno** `"Tutti gli album"` (`.back-link`, icona `chevronLeft` 15px, 13px, colore
  testo secondario);
- **titolo** = nome dell'album; se l'album è dinamico, subito dopo il nome un badge in linea
  `"dinamico"` (`.album-dynamic-inline-badge`, icona `funnel` 11px, 10.5px bold, colore
  `--accent` su sfondo `--accent-tint`, pillola con raggio 10px);
- **sottotitolo** composto per concatenazione: `"<N> foto · <intervallo>"`, poi `" · condiviso"`
  se l'album è condiviso, poi `" · si aggiorna da solo in base al filtro"` se è dinamico.
  Esempio completo: `"84 foto · Gen 2026 – Lug 2026 · condiviso"`. Il conteggio `N` è quello dei
  membri **totali**, non delle foto effettivamente mostrate dopo i filtri;
- **barra della griglia** (`.grid-toolbar`), che ha due forme alternative:
  - in modalità selezione → la barra `"N selezionate"` (**SP-2**, invariata);
  - altrimenti → una riga con uno spazio vuoto a sinistra e, a destra, il gruppo di azioni
    rapide della griglia: `"Seleziona tutto quello che vedi"` (**SP-4**, presente solo se c'è
    almeno una foto visibile) e il pannello imbuto del filtro rapido (**SP-3**);
- **griglia delle foto** (`.photo-grid`) con le tile standard (**SP-1**: badge RAW, spunta di
  selezione, cuoricino), giustificata su desktop.

**Tre stati vuoti distinti**, scelti in quest'ordine:

1. l'album ha foto ma i **filtri** le nascondono tutte → icona `funnel`,
   `"Nessuna foto corrisponde ai filtri"` / `"Prova ad allargare i filtri, o cancellali dal
   pannello qui sopra."`;
2. l'album è **dinamico** e nessuna foto soddisfa le condizioni → icona `funnel`,
   `"Nessuna foto corrisponde al filtro"` / `"Torna indietro e allarga le condizioni — per
   esempio passando da "Tutte" ad "Almeno una"."`;
3. l'album è **manuale** ed è vuoto → icona `album`, `"Album vuoto"` / `"Seleziona una o più foto
   in Timeline, poi "Aggiungi ad album" dalla barra di selezione."`

### 3. Ogni controllo, uno per uno

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Tutti gli album"` | link di ritorno | Azzera `state.openAlbum` → torna alla griglia degli album |
| 2 | Tile foto | area cliccabile | Apre il visualizzatore, o alterna la selezione se si è già in modalità selezione (SP-1) |
| 3 | Spunta di selezione sulla tile | casella | Entra in / esce dalla modalità selezione (SP-1, SP-2) |
| 4 | Cuoricino sulla tile | pulsante | Alterna il preferito su quella foto (SP-1) |
| 5 | `"Seleziona tutto quello che vedi"` | pulsante | SP-4 — seleziona tutte le foto attualmente visibili |
| 6 | Icona imbuto del filtro rapido | pulsante + pannello | SP-3 — filtro rapido a chip, con pallino del conteggio quando attivo |
| 7 | Barra `"N selezionate"` | barra azioni | SP-2 — Annulla, `"Seleziona tutte"`, Preferiti, Album, Condividi, Modifica, Elimina |

**Deviazioni dai pattern condivisi:** nessuna — l'album detail usa SP-1/2/3/4 così come sono. In
particolare `"Aggiungi ad album"` della barra di selezione (SP-2) apre il dialog di §4 anche
quando ci si trova **dentro** un album, e da lì si può disattivare l'album corrente: è il solo
modo, nel mockup, per **togliere** una foto da un album.

**Non previsto nel mockup in questa vista:** rinominare l'album, eliminarlo, modificarne le
condizioni se dinamico (le condizioni si possono impostare solo alla creazione), cambiare la
copertina, riordinare le foto a mano, impostare/togliere la condivisione.

### 4. Interazioni da mouse

- **Click su `"Tutti gli album"`** → griglia degli album. `.back-link:hover` schiarisce il testo
  da `--text-secondary` a `--text` (senza transizione).
- Click / hover sulle tile, sulla spunta, sul cuoricino: **SP-1**.
- **Tasto destro**: nessun menu contestuale, in nessun punto della vista.
- **Trascinamento**: nessun drag&drop — non si trascinano foto dentro o fuori dall'album.
- **Rotellina**: scroll normale della pagina.

### 5. Interazioni da tastiera

- **`"Tutti gli album"` NON è attivabile da tastiera**: `<div class="back-link" id="albumBack">`
  non ha `role` né `tabindex` ed è legato con `.onclick` puro (riga 4779). Stesso difetto del
  link di ritorno della creazione album.
- Tile, spunte, cuoricini, barra di selezione, filtro rapido: attivabili con **Invio** e
  **Spazio** (SP-8).
- **Escape** con il pannello del filtro rapido aperto lo chiude (gestore globale, riga 6309),
  senza toccare i filtri già applicati.
- Nessuna navigazione con le frecce dentro la griglia; nessuna scorciatoia specifica dell'album.

### 6. Animazioni e transizioni

Nessuna transizione propria della vista. Si ereditano quelle dei pattern: comparsa della spunta
di selezione e del cuoricino sulla tile (SP-1), pannello del filtro rapido (SP-3), toast (SP-6).
La barra della griglia **cambia forma di colpo** quando si entra in modalità selezione (il
markup viene sostituito, non c'è dissolvenza).

### 7. Stati per ogni controllo

- **Link di ritorno** — normale `--text-secondary`; hover `--text`; **nessuno stato focus**
  visibile (non è focalizzabile).
- **`"Seleziona tutto quello che vedi"`** — non viene nemmeno renderizzato quando la griglia
  filtrata è vuota (`shownList.length` a 0); l'intero gruppo di azioni rapide sparisce se
  l'album non ha proprio foto (`scopedList.length` a 0). Quindi in un album vuoto la barra della
  griglia è completamente vuota — e `.grid-toolbar:empty` azzera il margine inferiore per non
  lasciare un buco.
- **Filtro rapido** — normale / attivo con pallino del conteggio (SP-3).
- **Barra di selezione** — visibile solo con almeno una foto selezionata; sparisce da sola
  quando si deseleziona l'ultima.
- **Vuoto** — tre varianti distinte (§2), che è una scelta deliberata: distinguono "non hai
  ancora aggiunto niente" da "il filtro è troppo stretto" da "le condizioni dell'album dinamico
  non pescano nulla".

### 8. Da dove ci si arriva e dove si va

**In ingresso:** click su una scheda nella griglia album; subito dopo aver creato un album (la
creazione imposta `state.openAlbum` sul nuovo id).

**In uscita:** `"Tutti gli album"`; freccia indietro dell'header mobile (che azzera
`state.openAlbum`); apertura di una foto nel visualizzatore; `"Modifica"` della barra di
selezione → pagina di modifica in blocco; `"Aggiungi ad album"` / `"Condividi"` / `"Elimina"`
della barra di selezione → i rispettivi dialog.

Breadcrumb della topbar: `Album / <b>&lt;nome album&gt;</b>`. Titolo dell'header mobile: il nome
dell'album, con freccia indietro.

### 9. Dati necessari a questa schermata

**Legge:** nome dell'album, se è condiviso, se è dinamico, intervallo di date, elenco delle foto
che ne fanno parte; e per ciascuna foto tutto il necessario alla tile (miniatura, proporzioni,
se è RAW o RAW+JPEG, se è preferita, se è selezionata) più i campi su cui lavora il filtro
rapido.

**Scrive:** lo stato di preferito delle foto (dal cuoricino e dall'azione di massa), l'insieme
selezionato (solo in memoria), e — attraverso i dialog richiamati dalla barra di selezione —
l'appartenenza agli album e lo stato di eliminazione delle foto.

---

## 43. Creazione di un album

### 1. Nome e scopo

Pagina a sé (non un dialog) per creare un album, manuale oppure basato su un filtro; il filtro può
essere applicato una sola volta o restare "vivo" e ricalcolare l'appartenenza in continuazione.

### 2. Cosa mostra

**È una pagina unica, non una procedura a più passi.** Non ci sono passi numerati, non c'è
"Avanti/Indietro", non c'è indicatore di avanzamento: tutti i campi sono visibili
contemporaneamente e la parte relativa al filtro compare/scompare in base al tipo scelto.

Dall'alto:

- link di ritorno `"Tutti gli album"` (`.back-link`, `chevronLeft` 15px);
- titolo `"Crea album"`;
- sottotitolo `"Selezione trasversale alle cartelle — manuale, oppure basata su un filtro che si
  aggiorna da solo."`;

**Sezione `"Nome"`** (`.settings-section`, larghezza massima 560px):

- campo di testo con etichetta accessibile nascosta `"Nome album"` e placeholder
  `"Es. Migliori scatti, Tramonti, Ritratti…"`, larghezza massima 360px;
- riga con interruttore `.mini-switch` (`role="switch"`, `aria-checked`) ed etichetta
  `"Condiviso"`; tutta la riga è un `<label>` cliccabile.

**Sezione `"Tipo di raccolta"`**:

- controllo segmentato (`role="radiogroup"`, etichetta `"Tipo di raccolta"`) con due opzioni:
  `"Manuale"` e `"Basato su filtro"`;
- se **Manuale**: un solo testo esplicativo, `"Album vuoto: aggiungerai le foto in un secondo
  momento, dalla selezione multipla → "Aggiungi ad album"."` — e nient'altro;
- se **Basato su filtro**, compaiono in cascata:
  - controllo segmentato (`role="radiogroup"`, etichetta `"Quando applicare il filtro"`) con
    `"Una tantum"` e `"Automatico"`;
  - il testo esplicativo corrispondente:
    - Una tantum → `"Il filtro viene applicato una sola volta, ora: le foto corrispondenti
      entrano nell'album, che da quel momento si comporta come un album manuale qualsiasi
      (potrai aggiungerne o toglierne a mano)."`
    - Automatico → `"Le foto che soddisfano le condizioni entrano ed escono dall'album in
      automatico, sempre, senza doverle aggiungere a mano."`
  - **solo con Automatico**, una nota rivolta al team, in colore terziario, lasciata
    volutamente nel mockup: `"Nota per lo sviluppo: questo si sovrappone concettualmente alle
    "ricerche salvate" già previste in Cerca dalla spec — stessa idea di raccolta "viva",
    presentata qui come album. Da unificare quando passiamo al documento per i dev."`
  - controllo segmentato (`role="radiogroup"`, etichetta `"Operatore tra condizioni"`) con
    `"Tutte le condizioni (AND)"` e `"Almeno una (OR)"`;
  - l'elenco delle condizioni (una riga per condizione);
  - pulsante `"Aggiungi condizione"` (ghost, piccolo, icona `plus` 13px);
  - **anteprima live** (`.filter-preview`, sfondo `--accent-tint`, testo `--accent`, raggio 8px):
    `"<b>N</b> foto corrispondono al filtro attuale"`.

**Barra finale**: pulsante primario `"Crea album"` (icona `plus` 14px) e pulsante ghost
`"Annulla"`, affiancati.

**Anatomia di una riga-condizione** (`.filter-row`, sfondo `--chip-bg`, raggio 9px, padding 10px):

1. un menu a tendina del **campo** (`.filter-field-select`, larghezza minima 150px), con le nove
   voci di `ALBUM_FILTER_FIELDS`;
2. l'**editor del valore**, che cambia forma a seconda del campo;
3. una **x** di rimozione (`.filter-row-remove`, 24×24, icona `close` 13px,
   `aria-label="Rimuovi questa condizione"`).

I nove campi disponibili, con il rispettivo editor:

| Etichetta del campo | Tipo di editor | Valori |
|---|---|---|
| `"Cartella"` | picklist a scelta multipla | `"Urbino"`, `"Lago di Braies"`, `"Chioggia e Venezia"` |
| `"Intervallo di date"` | due campi data | da / a, entrambi limitati a `2026-01-01`–`2026-12-31` |
| `"Paese"` | menu a tendina | `"Italia"` |
| `"Fotocamera"` | menu a tendina | i modelli distinti presenti nel catalogo |
| `"Obiettivo"` | menu a tendina | gli obiettivi distinti presenti nel catalogo |
| `"Tipo file"` | menu a tendina | `"RAW+JPEG"`, `"RAW"`, `"JPEG"` |
| `"Preferiti"` | menu a tendina | `"È un preferito"`, `"Non è un preferito"` |
| `"Valutazione minima"` | menu a tendina | cinque voci disegnate a stelle: `★☆☆☆☆` … `★★★★★` |
| `"Pick / Scarta"` | menu a tendina | `"Pick"`, `"Scarta"`, `"Non ancora deciso"` |

I menu a tendina semplici hanno sempre una prima voce vuota `"Scegli…"`; **finché è selezionata,
la condizione non filtra nulla** (`condMatches` restituisce vero per valore vuoto). Lo stesso
vale per la picklist con zero cartelle spuntate e per l'intervallo di date con entrambi i campi
vuoti. Il campo `"Paese"` restituisce **sempre** vero, con commento esplicito nel codice: *"nel
mockup tutte le cartelle sono in Italia"*.

**La picklist** (`.picklist`, larghezza massima 260px) — il commento al codice ne spiega la
ragione: *"il trigger apre/chiude il pannello, le righe al suo interno spuntano/tolgono un valore
— sostituisce il muro di checkbox in linea, più difficile da scansionare."* Il trigger
(`role="button"`, `aria-haspopup="listbox"`, `aria-expanded`) mostra un riassunto:

- nessun valore spuntato → `"Tutte — cartella"` (etichetta del campo in minuscolo);
- un valore → il nome della cartella;
- più valori → `"<N> selezionate"`.

Il pannello (`role="listbox"`, `aria-multiselectable="true"`) elenca le righe
(`role="option"`, `aria-selected`, `tabindex="0"`) con una casella quadrata 15×15 a sinistra che
si riempie di colore accento quando spuntata.

### 3. Ogni controllo, uno per uno

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Tutti gli album"` | link di ritorno | Scarta la bozza e torna alla griglia album |
| 2 | (campo nome) | campo di testo | Scrive il nome nella bozza a ogni battuta. Placeholder `"Es. Migliori scatti, Tramonti, Ritratti…"`. **Nessuna validazione mentre si scrive**; nessun limite di lunghezza; nessun controllo di nomi duplicati |
| 3 | `"Condiviso"` | interruttore | Alterna `shared` sulla bozza e ridisegna |
| 4 | `"Manuale"` | opzione segmentata | Nasconde tutta la parte del filtro |
| 5 | `"Basato su filtro"` | opzione segmentata | Mostra modalità, operatore, condizioni, anteprima |
| 6 | `"Una tantum"` | opzione segmentata | Il filtro produrrà una fotografia istantanea dell'appartenenza |
| 7 | `"Automatico"` | opzione segmentata | Il filtro resterà vivo (album dinamico) |
| 8 | `"Tutte le condizioni (AND)"` | opzione segmentata | Le condizioni si sommano |
| 9 | `"Almeno una (OR)"` | opzione segmentata | Basta una condizione soddisfatta |
| 10 | (tendina campo condizione) | menu | Cambiando campo, **la condizione viene ricreata da zero** e il valore precedente è perso |
| 11 | (editor del valore) | menu / picklist / due campi data | Aggiorna il valore della condizione |
| 12 | x di rimozione | pulsante | Toglie quella condizione dall'elenco. **Si può rimuovere anche l'ultima**, restando a zero condizioni |
| 13 | `"Aggiungi condizione"` | pulsante ghost | Aggiunge in coda una condizione nuova, sempre sul campo `"Cartella"` con nessuna cartella spuntata (quindi inizialmente innocua) |
| 14 | `"Crea album"` | pulsante primario | Valida e crea (vedi sotto) |
| 15 | `"Annulla"` | pulsante ghost | Identico al link di ritorno: scarta la bozza e torna alla griglia |

**Valori iniziali della bozza** (`freshAlbumDraft()`): nome vuoto, non condiviso, tipo
`"Manuale"`, operatore `"Tutte le condizioni (AND)"`, modalità `"Automatico"`, **e una condizione
già presente** sul campo `"Cartella"` — che però resta invisibile finché non si sceglie
`"Basato su filtro"`.

**Cosa succede alla conferma** (`"Crea album"`):

1. il nome viene ripulito dagli spazi ai bordi. **Se è vuoto**: compare il toast `"Dai un nome
   all'album prima di crearlo."`, il focus torna nel campo nome, **e non si crea nulla**. Il
   pulsante **non** viene disabilitato preventivamente: la validazione è solo al momento
   dell'invio;
2. se il tipo è basato su filtro **e non c'è nessuna condizione**: toast `"Aggiungi almeno una
   condizione, oppure passa a "Manuale"."`, niente creazione;
3. altrimenti si crea l'album, con un id progressivo (`alc1`, `alc2`, …) e una tinta di copertina
   derivata deterministicamente dal nome più l'id;
4. tre esiti diversi a seconda delle scelte:
   - **Manuale** → album con zero membri e intervallo testuale `"nessuna foto ancora"`;
   - **Basato su filtro + Una tantum** → si calcolano subito le foto corrispondenti e i loro id
     diventano l'appartenenza **fissa**; da quel momento l'album è manuale a tutti gli effetti.
     Il commento nel codice lo dice esplicitamente: *"applicato una tantum: le foto che
     corrispondono ORA diventano membership fissa, da quel momento l'album è manuale a tutti gli
     effetti (modificabile a mano)"*. L'intervallo di date viene calcolato dalle foto pescate;
   - **Basato su filtro + Automatico** → album marcato come dinamico, con operatore e condizioni
     salvati (copia profonda della bozza). **Non ha un insieme di membri**: viene ricalcolato a
     ogni render;
5. toast `Album "<nome>" creato.`, la bozza viene azzerata e **si atterra direttamente nel
   dettaglio del nuovo album**.

### 4. Interazioni da mouse

- Click su ognuno dei controlli sopra.
- **Click sul trigger della picklist** → apre/chiude il pannello. Il gestore ferma la propagazione
  dell'evento (`stopPropagation`), perché c'è un **listener globale sul documento** (riga 2293)
  che chiude la picklist a ogni click quando ci si trova in questa vista: cioè **cliccare in un
  punto qualsiasi fuori dalla picklist la chiude** (SP-14).
- **Click su una riga della picklist** → spunta/toglie quel valore; anche qui la propagazione è
  fermata, così il pannello **resta aperto** e si possono spuntare più cartelle di fila.
- **Hover**: `.picklist-row:hover` e `.album-picker-row:hover` prendono `--chip-bg`;
  `.filter-row-remove:hover` prende `--chip-bg-hover` e il colore passa a `--danger` (segnala
  che è un'azione distruttiva); `.btn:hover` prende `--chip-bg`. Tutte senza transizione.
- **Tasto destro, doppio click, drag&drop**: non implementati. Le condizioni **non si riordinano**.

### 5. Interazioni da tastiera

- Campo nome: normale campo di testo; **Invio non invia il modulo** (non c'è un `<form>`, non
  c'è gestore su Invio nel campo).
- Interruttore `"Condiviso"`, opzioni segmentate, x di rimozione, `"Aggiungi condizione"`,
  trigger e righe della picklist, `"Crea album"`: tutti legati con `bindActivatable` → **Invio**
  e **Spazio** (SP-8), tutti con `tabindex="0"`.
- **Escape con una picklist aperta la chiude** (gestore globale, riga 6306) senza perdere i
  valori già spuntati.
- **Ordine del focus dentro i controlli segmentati**: solo l'opzione **attiva** ha
  `tabindex="0"`, le altre hanno `tabindex="-1"` — il gruppo si comporta come un unico stop di
  tabulazione, coerentemente con `role="radiogroup"`. **Ma le frecce ← → non spostano la
  selezione fra le opzioni**: si può cambiare opzione solo raggiungendola col mouse, oppure —
  poiché l'opzione non attiva non è focalizzabile — non la si raggiunge affatto da tastiera.
  È un difetto: un radiogroup senza navigazione a frecce è inutilizzabile via tastiera.
- **`"Annulla"` e `"Tutti gli album"` non sono attivabili da tastiera**: entrambi sono legati con
  `.onclick` puro (righe 4891–4892). `"Annulla"` ha però `role="button"` e `tabindex="0"`, quindi
  è raggiungibile col Tab e sembra attivabile, ma **premendo Invio non succede nulla** — è
  l'incoerenza più insidiosa della schermata.
- Non c'è scorciatoia per aggiungere una condizione né per confermare.

### 6. Animazioni e transizioni

- **Chevron della picklist**: `transform .12s ease` sull'icona; con il pannello aperto ruota di
  180°. Comunica lo stato aperto/chiuso.
- **Interruttore `"Condiviso"`**: il pomello si sposta con `left .15s ease` (da 2px a 18px).
  Il colore di sfondo passa a `--accent` **senza** transizione (solo la posizione è animata).
- **Pannello della picklist**: compare/scompare di colpo (viene aggiunto/tolto dal DOM), senza
  dissolvenza né scorrimento; ha un'ombra `0 10px 26px rgba(0,0,0,.18)` che lo stacca dal fondo.
- **Anteprima del conteggio**: si aggiorna sostituendo il testo, senza animazione. Attenzione:
  cambiare *campo* o rimuovere una condizione ridisegna l'intera pagina; cambiare solo il
  *valore* di un menu o di una data aggiorna **soltanto** il riquadro dell'anteprima
  (`updatePreview()`), evitando di ridisegnare tutto mentre si compila.
- **Toast** di validazione e di conferma: SP-6.
- Le opzioni segmentate cambiano aspetto istantaneamente (`.seg-option.active` prende
  `background:var(--card-bg)`, peso 600 e ombra), senza transizione.

### 7. Stati per ogni controllo

- **Campo nome** — normale: sfondo `--chip-bg`, bordo `--border-strong`, raggio 8px, 13px.
  Focus: `outline:2.5px solid var(--accent); outline-offset:2px`. **Non esiste uno stato di
  errore visivo**: il nome vuoto viene segnalato solo con un toast e con il focus riportato nel
  campo, il bordo non diventa rosso.
- **`"Condiviso"`** — spento: sfondo `--border-strong`. Acceso: sfondo `--accent`, pomello a
  destra. Focus visibile.
- **Opzioni segmentate** — normale (testo secondario, nessuno sfondo) / attiva (sfondo carta,
  testo pieno, peso 600, ombra) / focus (outline accento). **Mai disabilitate.**
- **Trigger picklist** — normale / aperto (chevron ruotato) / focus (outline accento). Non ha
  stato hover dedicato.
- **Riga della picklist** — normale / hover (`--chip-bg`) / spuntata (casella piena di accento
  con la spunta bianca) / focus (outline accento).
- **x di rimozione** — normale (colore terziario) / hover (sfondo `--chip-bg-hover`, colore
  `--danger`) / focus (outline accento). **Non è mai disabilitata**, nemmeno sull'ultima
  condizione rimasta: si può quindi arrivare a zero condizioni, e l'anteprima allora mostra
  `"0 foto corrispondono al filtro attuale"` con l'operatore AND (perché `every` su un array
  vuoto è vero, ma `albumMembers` restituisce comunque un elenco vuoto per un album dinamico
  senza condizioni) — vedi le ambiguità in fondo.
- **`"Crea album"`** — **mai disabilitato**, nemmeno con il nome vuoto o con zero condizioni:
  la validazione è a valle, non a monte. Nessuno stato di caricamento.
- **`"Annulla"`** — normale / hover `--chip-bg` / focus (outline) ma **inerte da tastiera**.
- **Anteprima** — quando nessuna foto corrisponde mostra semplicemente `"0 foto corrispondono al
  filtro attuale"`, senza cambiare colore né aspetto: non c'è uno stato di allarme.

### 8. Da dove ci si arriva e dove si va

**In ingresso:** unicamente dal pulsante `"Crea album"` della griglia album (§1). Non c'è
scorciatoia globale, non c'è voce di menu, non c'è un `+` nella topbar.

**In uscita:**
- `"Annulla"` o `"Tutti gli album"` → griglia album, bozza scartata;
- `"Crea album"` con esito positivo → **dettaglio** del nuovo album;
- qualsiasi voce della sidebar → la vista corrispondente. Attenzione: la navigazione da sidebar
  **non azzera la bozza** e non avvisa di modifiche non salvate; la bozza rimane in memoria ma
  viene comunque sovrascritta la volta successiva che si preme `"Crea album"` dalla griglia.

### 9. Dati necessari a questa schermata

**Legge:** l'elenco delle cartelle (nome e identificativo) per la picklist; i modelli di
fotocamera e obiettivo distinti presenti nella libreria; e — per contare in tempo reale le
corrispondenze dell'anteprima — l'intero catalogo con, per ogni foto: cartella di appartenenza,
data di scatto (giorno e mese), fotocamera, obiettivo, tipo di file (RAW / RAW+JPEG / JPEG),
preferito sì/no, valutazione 0–5, stato pick/scarta.

**Scrive:** un nuovo album, con nome, flag "condiviso", tinta della copertina, e — a seconda
del tipo — o l'**insieme degli identificativi delle foto membri** (manuale e "una tantum"),
oppure le **condizioni del filtro con il loro operatore** (dinamico). Per gli album a
appartenenza fissa scrive anche il conteggio e l'intervallo di date testuale.

---

## 44. Dialog "Aggiungi ad album"

### 1. Nome e scopo

Dialog modale che permette di aggiungere o togliere in blocco un gruppo di foto da uno o più
album manuali, con un interruttore per album.

### 2. Cosa mostra

Scrim scuro a tutto schermo (`--scrim`, `rgba(20,20,20,.72)` in chiaro / `rgba(0,0,0,.85)` in
scuro) e una scheda centrata (`.modal-card`, larghezza **360px** fissa su desktop, **86%** su
mobile, sfondo carta, bordo `--border-strong`, raggio 12px, padding 18px, ombra
`0 20px 50px rgba(0,0,0,.3)`).

Contenuto:

- **titolo** `"Aggiungi ad album"`;
- **sottotitolo** `"<N> elementi selezionati — attiva/disattiva un album per aggiungere o
  rimuovere tutti gli elementi"` (al singolare: `"1 elemento selezionato — …"`);
- **elenco degli album** (`.album-picker-list`, altezza massima 260px, con scorrimento proprio).
  Una riga per **album non dinamico**, ciascuna con:
  - quadratino di copertina 36×36 con lo stesso gradiente della scheda in griglia;
  - nome dell'album;
  - interruttore `.mini-switch` a destra;
- **eventuale nota** in coda, se esistono album dinamici: `"<N> album dinamici non mostrati qui:
  la loro appartenenza è calcolata automaticamente dal filtro, non modificabile a mano."`
  (al singolare `"1 album dinamico non mostrato qui: …"`);
- **pulsante `"Fatto"`** (ghost, piccolo).

Non si vedono: le miniature delle foto selezionate, il conteggio delle foto già presenti in
ciascun album, un campo per creare al volo un album nuovo, né una ricerca fra gli album.

### 3. Ogni controllo, uno per uno

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Riga album (una per album manuale) | interruttore (`role="switch"`, `aria-checked`, `aria-label` = nome album, `tabindex="0"`) | Se **tutte** le foto selezionate sono già in quell'album → le toglie **tutte**; altrimenti → le aggiunge **tutte** |
| 2 | `"Fatto"` | pulsante ghost piccolo | Chiude il dialog e ridisegna l'app |

Due dettagli importanti sul comportamento dell'interruttore:

- **non esiste uno stato intermedio.** L'interruttore è acceso solo se *tutte* le foto
  selezionate sono già membri; con una selezione parzialmente presente l'interruttore appare
  **spento**, e il primo click **aggiunge** le mancanti (portandolo ad acceso). Non c'è un terzo
  stato "alcune".
- **la modifica è immediata e senza conferma.** L'appartenenza viene aggiornata al click; il
  conteggio dell'album viene ricalcolato. Non c'è annullamento, non c'è toast.

### 4. Interazioni da mouse

- Click su una riga → alterna l'appartenenza. Nota tecnica: il dialog **non ridisegna l'intera
  app** a ogni click; aggiorna a mano la classe dell'interruttore e l'attributo `aria-checked`
  della riga, così il dialog non "sfarfalla" e la posizione di scorrimento dell'elenco resta.
  Il ridisegno completo avviene solo alla chiusura.
- Hover sulla riga: sfondo `--chip-bg`, senza transizione.
- **Click sullo scrim (fuori dalla scheda): NON chiude il dialog** — non c'è alcun gestore sul
  contenitore. È una deviazione da SP-5 da segnalare.
- Tasto destro, doppio click, trascinamento: nessun comportamento.

### 5. Interazioni da tastiera

- **All'apertura il focus va sulla prima riga album**; alla chiusura torna sull'elemento che
  aveva aperto il dialog (memorizzato come `document.activeElement`).
- Righe e `"Fatto"` sono attivabili con **Invio** e **Spazio** (SP-8).
- **Escape chiude** (gestore su `document`, quindi funziona ovunque sia il focus).
- **Non c'è trappola del focus:** premendo Tab oltre `"Fatto"` si esce dal dialog e si finisce
  sui controlli della pagina sottostante, che restano raggiungibili. Deviazione da SP-5.

### 6. Animazioni e transizioni

- **Nessuna animazione di entrata o uscita** del dialog: né dissolvenza dello scrim, né
  scalatura della scheda. Compare e sparisce di colpo.
- L'unica transizione è il pomello dell'interruttore: `left .15s ease`.

### 7. Stati per ogni controllo

- **Riga album** — normale / hover (`--chip-bg`) / focus (`outline:2.5px solid var(--accent)`,
  regola dedicata `.album-picker-row:focus-visible`) / acceso (interruttore con sfondo accento e
  pomello a destra). Mai disabilitata.
- **`"Fatto"`** — normale / hover (`--chip-bg`) / focus (outline). Mai disabilitato.
- **Stato vuoto:** se non esistesse alcun album manuale, l'elenco risulterebbe vuoto e la nota
  sui dinamici sarebbe l'unico contenuto. **Non c'è un messaggio dedicato** del tipo "nessun
  album, creane uno" né un pulsante per crearne uno al volo. Con i dati iniziali del mockup il
  caso non si presenta (i quattro album di partenza sono tutti manuali).
- **Nessuno stato di caricamento o di errore.**

### 8. Da dove ci si arriva e dove si va

**In ingresso**, quattro punti:

1. barra `"N selezionate"` (SP-2), pulsante `"Album"` — dalla Timeline, dai Preferiti, da Cerca,
   dal dettaglio di un Album;
2. pagina di **modifica in blocco**, pulsante album;
3. **visualizzatore foto** (lightbox), chip `"Aggiungi ad album"` — su una sola foto;
4. **visualizzatore foto**, menu `⋯`, voce di aggiunta ad album — su mobile.

**In uscita:** `"Fatto"` o **Escape** riportano esattamente da dove si è arrivati; il focus
ritorna al pulsante d'origine. Non porta mai in un'altra schermata.

### 9. Dati necessari a questa schermata

**Legge:** l'elenco degli album con nome, tinta della copertina, se sono dinamici (per
escluderli), e l'appartenenza attuale delle foto selezionate; e il numero di foto selezionate.

**Scrive:** l'appartenenza delle foto selezionate agli album scelti (aggiunta o rimozione in
blocco) e il conteggio dei membri di ciascun album toccato.

---

## 45. Cestino

### 1. Nome e scopo

Raccoglie le foto che l'utente ha eliminato scegliendo `"Sposta nel cestino di Keeppix"`, tenendole
recuperabili per un periodo limitato prima della cancellazione definitiva.

### 2. Cosa mostra

**Cestino vuoto:** un'intestazione con il solo titolo `"Cestino"`, e sotto lo stato vuoto — icona
`trash` 34px al 50% di opacità, titolo `"Il cestino è vuoto"`, testo `"Gli elementi spostati qui
— da Culling o dalla vista dettaglio, scegliendo "Sposta nel cestino di Keeppix" — restano
recuperabili per 30 giorni."`

**Cestino pieno:**

- titolo `"Cestino"`;
- sottotitolo `"<N> elementi · eliminazione definitiva dopo 30 giorni"`;
- pulsante `"Svuota cestino"` (`.btn.btn-danger.btn-sm`, icona `trash` 14px: bordo e testo
  `--danger`, sfondo trasparente);
- una griglia di riquadri quadrati (`.trash-tile`, `aspect-ratio:1/1`, raggio 5px, bordo
  `--border`) — **non** le tile standard SP-1: qui non ci sono badge RAW, spunta di selezione né
  cuoricino. Ogni riquadro mostra:
  - il gradiente della foto come miniatura;
  - un **badge in basso** (`.trash-badge`, 9.5px bold, testo bianco centrato su
    `rgba(10,10,10,.6)`, largo quanto il riquadro meno 5px per lato) con il testo
    `"<N> giorni rimanenti"`.

Il numero di giorni **non deriva da una data di eliminazione reale**: è calcolato come
`20 + (hash(id) % 10)`, quindi cade sempre fra **20 e 29 giorni**. È un valore finto ma
deterministico (lo stesso file mostra sempre lo stesso numero).

Non è mostrato: il nome del file, la cartella d'origine, la data di eliminazione, chi ha
eliminato, quanto spazio occupa il cestino.

### 3. Ogni controllo, uno per uno

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Svuota cestino"` | pulsante danger piccolo | **Elimina definitivamente tutti gli elementi in cestino, all'istante e senza chiedere conferma.** Nessun dialog, nessun toast, nessun annullamento |
| 2 | Icona freccia circolare (`rotate`) sul riquadro | pulsantino, visibile solo in hover, tooltip nativo `title="Ripristina"` | **Ripristina** quella foto: riporta lo stato di scelta a "non ancora deciso" e cancella la marcatura di cestino. La foto torna a comparire normalmente in Timeline |
| 3 | Icona cestino sul riquadro | pulsantino, visibile solo in hover, colore `--danger`, tooltip nativo `title="Elimina definitivamente"` | **Elimina definitivamente** solo quella foto. Senza conferma, senza toast |

**Cosa si può ripristinare:** qualunque elemento presente nel cestino, uno alla volta. Il
ripristino riporta la foto a `pick = "non ancora deciso"` — quindi **non** allo stato che aveva
prima di essere scartata: se era marcata "Scelta" in culling, quel dato è perso.

**Svuotamento:** sì, esiste, ed è il pulsante `"Svuota cestino"`. È l'unica azione di massa
disponibile: non c'è selezione multipla nel cestino, quindi non si può ripristinare o eliminare
un gruppo di elementi.

**Scadenza automatica:** è **annunciata ma non implementata**. Il sottotitolo dichiara
`"eliminazione definitiva dopo 30 giorni"`, il badge di ogni elemento mostra un conto alla
rovescia, e il dialog di eliminazione promette `"Recuperabile per 30 giorni."` — ma nel mockup
nessun elemento sparisce da solo con il tempo, e il conto alla rovescia è generato dall'hash
dell'identificativo. La scadenza è quindi un requisito da implementare lato backend, non un
comportamento osservabile qui.

**Non previsto nel mockup:** selezione multipla, ordinamento, ricerca, filtro rapido, apertura
della foto nel visualizzatore dal cestino, ripristino "nella cartella d'origine" esplicito,
indicazione dello spazio occupato/recuperabile.

### 4. Interazioni da mouse

- **Hover sul riquadro** → compare il velo `.trash-hover` (`inset:0`, sfondo `rgba(0,0,0,.45)`,
  da `opacity:0` a `opacity:1`) con i due pulsantini bianchi 26×26 centrati e distanziati di 6px.
  **Non c'è transizione dichiarata**: il velo compare istantaneamente, a differenza delle azioni
  sulle tile standard.
- **Click su un pulsantino** → ripristina o elimina; l'evento **ferma la propagazione**, per non
  attivare eventuali gestori del riquadro.
- **Click sul riquadro (fuori dai pulsantini)** → **non fa nulla.** Il riquadro porta un attributo
  `data-trash` con l'identificativo della foto, ma **nessun gestore vi è collegato**: sembra un
  residuo di un'intenzione (aprire l'anteprima) mai completata.
- **Hover su `"Svuota cestino"`** → sfondo `--danger-tint`.
- Tasto destro, doppio click, trascinamento, rotellina: nessun comportamento specifico.

### 5. Interazioni da tastiera

Questa è la vista **meno accessibile del blocco**:

- **`"Svuota cestino"` non è raggiungibile né attivabile da tastiera**: `<div class="btn
  btn-danger btn-sm" id="emptyTrash">` non ha `role` né `tabindex`, ed è legato con `.onclick`.
- **I pulsantini di ripristino ed eliminazione non sono raggiungibili né attivabili da
  tastiera**: sono `div.mini-btn` senza `role` né `tabindex`, legati con `.onclick`; inoltre sono
  visibili solo in `:hover` del riquadro, e **non c'è una regola `:focus-within`** che li mostri
  come invece accade per le tile standard.
- Conseguenza pratica: **da sola tastiera il Cestino è di sola lettura.**
- Nessuna scorciatoia dedicata (né Canc, né Ctrl+Z).

### 6. Animazioni e transizioni

- **Velo di hover sul riquadro**: da opacità 0 a 1, **senza transizione** → comparsa
  istantanea. Comunica che ci sono azioni disponibili su quel riquadro, ma lo fa in modo più
  brusco rispetto al resto dell'app (che usa tipicamente `.12s ease`).
- Nessuna animazione di rimozione: un elemento ripristinato o eliminato **sparisce di colpo**
  al ridisegno, senza dissolvenza né scorrimento della griglia.
- Nessun toast di conferma per nessuna delle tre azioni.

### 7. Stati per ogni controllo

- **`"Svuota cestino"`** — normale (bordo e testo `--danger`, sfondo trasparente) / hover
  (`--danger-tint`). **Non ha stato disabilitato**, ma non serve: quando il cestino è vuoto il
  pulsante non viene proprio renderizzato (si passa al ramo dello stato vuoto). Nessuno stato
  focus (non focalizzabile).
- **Pulsantini del riquadro** — invisibili di default (opacità 0 sul velo), visibili in hover.
  Nessuno stato focus, nessuno stato premuto, nessuno stato disabilitato.
- **Riquadro** — un solo stato. Nessuna selezione, nessun focus.
- **Vuoto** — schermata dedicata, con il testo che spiega *da dove* arrivano gli elementi
  (Culling, vista dettaglio) e *per quanto* restano.
- **Errore / caricamento** — non implementati.

### 8. Da dove ci si arriva e dove si va

**In ingresso:**
- sidebar desktop → gruppo a scomparsa `"Manutenzione"` → `"Cestino"`. Il gruppo si apre
  cliccando la voce `"Manutenzione"` (icona `settings`, chevron a destra) e **si apre da solo**
  se la vista corrente è una delle tre di manutenzione;
- mobile: tab `"Altro"` → sezione `"Manutenzione"` → riga `"Cestino"`.

**Come ci finiscono le foto:** unicamente scegliendo `"Sposta nel cestino di Keeppix"` nel dialog
di eliminazione a 3 opzioni (§9), sia dal visualizzatore su una foto singola sia dalla barra
`"N selezionate"` su un gruppo. Le foto scartate in **Culling** *non* finiscono qui: in culling
si usa Scelta/Scarta, che è un'altra cosa (vedi il commento a riga 4289) — il che rende
leggermente impreciso il testo dello stato vuoto, che cita Culling fra le provenienze.

**In uscita:** solo la navigazione generale (sidebar / tab bar). Dal cestino non si apre nulla.

### 9. Dati necessari a questa schermata

**Legge:** l'elenco delle foto marcate come eliminate e finite nel cestino, con miniatura e
identificativo, più i giorni residui prima dell'eliminazione definitiva (nel mockup finti).

**Scrive:** per il ripristino, azzera lo stato di scelta della foto e toglie la marcatura di
cestino; per l'eliminazione definitiva (singola o svuotamento), marca la foto come eliminata in
modo permanente.

**Nota per l'architetto backend:** nel mockup l'eliminazione definitiva **non rimuove davvero
l'elemento dal catalogo** — cambia soltanto il valore della marcatura da "cestino" a "eliminata
definitivamente". La foto resta quindi presente in `allPhotos()` con lo stato "scartata" e
continua a comparire altrove nell'app. È una semplificazione della demo, non un requisito.

---

## 46. Duplicati

### 1. Nome e scopo

Pagina di manutenzione che raggruppa i file con contenuto identico e propone di tenerne una copia
sola, recuperando spazio su disco.

### 2. Cosa mostra

**Criterio di duplicazione — esplicito nel mockup: l'hash del contenuto del file, non il nome.**
Lo dicono tre punti indipendenti del codice:

- il commento di sezione (riga 5186): *"la dedup via content-hash è già supportata dal modello
  dati, ma non era ancora rappresentata in questo mockup"*;
- il titolo di ogni gruppo: `"<N> file identici (stesso hash del contenuto)"`;
- lo stato vuoto: `"Keeppix confronta l'hash del contenuto dei file, non solo il nome — trova
  copie identiche anche se rinominate o in cartelle diverse."`

Il commento a `buildDuplicateGroups` spiega anche perché i duplicati del mockup si somigliano
così tanto: *"copie duplicate vere condividono lo stesso contenuto: stesso colore/miniatura,
stessa dimensione, stessa estensione — solo il nome cambia, di solito con un suffisso
"(1)"/"(copia)" come aggiunge il filesystem quando importi due volte lo stesso file."*

**Stato vuoto:** icona `check`, `"Nessun duplicato trovato"`, con il testo sul criterio citato
sopra.

**Con duplicati:**

- titolo `"Duplicati"`;
- sottotitolo riassuntivo: `"<N> gruppi · <M> file coinvolti · fino a <X,X> MB recuperabili
  tenendo una sola copia per gruppo"` (singolare: `"1 gruppo"`);
- un blocco per gruppo (`.problem-row.dup-group`: riga flex, padding 14px, bordo `--border`,
  raggio 10px, margine inferiore 10px — **lo stesso componente della pagina Problemi**), con:
  - **icona** 34×34 (`.problem-ico.warn`) con l'icona `copy` 16px, colore `--accent` su sfondo
    `--accent-tint` (avviso, non errore);
  - **titolo**: `"<N> file identici (stesso hash del contenuto)"`;
  - **sottotitolo**: `"<motivo> — <X,X> MB recuperabili se tieni solo la copia evidenziata"`;
  - **striscia di miniature** (`.bulk-strip`, scorrimento orizzontale, gap 5px), una per file:
    - miniatura 70×70 con il gradiente della foto;
    - sotto, l'etichetta su due righe: **nome del file** e **dimensione in MB** (una cifra
      decimale);
    - sulla copia scelta, un **pallino con la spunta** in alto a destra della miniatura
      (`.dup-keep-badge`, 16×16, tondo, sfondo `--accent`);
  - **due pulsanti**: `"Risolvi gruppo"` (pulsante normale piccolo) e `"Ignora"` (ghost piccolo).

**I due gruppi del mockup:**

| Gruppo | File | Motivo mostrato |
|---|---|---|
| 1 | 2 file: il nome originale e lo stesso nome con `" (1)"` prima dell'estensione | `"Stesso file importato due volte — import manuale e poi sync automatico dalla stessa scheda SD"` |
| 2 | 3 file: il nome originale, lo stesso con `" (copia)"` e lo stesso con `" (2)"` | `"La stessa foto è stata reimportata più volte per errore nella stessa cartella"` |

I file base sono la terza foto di *Urbino* e la quinta di *Chioggia e Venezia*; tutte le copie di
un gruppo ereditano colore, dimensione ed estensione dal file base, quindi **all'interno di un
gruppo tutte le copie pesano esattamente uguale**.

**Quale copia è proposta come "da tenere" e perché:** la **prima dell'elenco**
(`keepId: group[0].id`), che è sempre quella **senza suffisso di copia** — cioè il file con il
nome originale, non la copia che il filesystem ha rinominato in `"(1)"` / `"(copia)"` / `"(2)"`.
Il criterio è implicito nella costruzione dei dati e **non è mai spiegato all'utente in
interfaccia**: nessun testo dice "teniamo l'originale". Lo si intuisce solo dal fatto che
l'`aria-label` della copia scelta comincia con `"Mantieni …"`.

**Il calcolo dei MB recuperabili** = somma delle dimensioni di tutti i file del gruppo, meno la
dimensione della copia che si tiene. Poiché tutte le copie di un gruppo pesano uguale, il numero
è di fatto `(numero di copie − 1) × dimensione` e **non cambia** cambiando quale copia tenere.

### 3. Ogni controllo, uno per uno

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Miniatura di una copia (una per file) | pulsante a due stati (`role="button"`, `aria-pressed`, `tabindex="0"`) | Elegge **quella** copia come quella da tenere; le altre diventano automaticamente le candidate all'eliminazione. Ridisegna la pagina |
| 2 | `"Risolvi gruppo"` | pulsante piccolo | Apre il **dialog di eliminazione a 3 opzioni** (§9) con titolo `"Eliminare <N> copie duplicate?"` (singolare: `"Eliminare 1 copia duplicata?"`). Scegliendo una delle tre opzioni il gruppo scompare dall'elenco e appare il toast `"<N> copie rimosse, mantenuta <nome del file tenuto>."` (singolare `"1 copia rimossa, mantenuta …"`). Annullando o premendo Escape **il gruppo resta** |
| 3 | `"Ignora"` | pulsante ghost piccolo | Toglie il gruppo dall'elenco **senza toccare i file**, con toast `"Gruppo ignorato — non verrà più segnalato."` Nessuna conferma, nessun annullamento |

**Etichette accessibili delle miniature** (lette dagli screen reader, non visibili):
- sulla copia scelta: `"Mantieni <nome file>"`;
- sulle altre: `"Segna come duplicato da eliminare, mantieni invece <nome file>"`.

**Elenco completo delle azioni disponibili sui duplicati**: eleggere quale copia tenere,
risolvere il gruppo (con le tre modalità di eliminazione), ignorare il gruppo. **Non previsti nel
mockup:** un'azione "risolvi tutti i gruppi", una selezione multipla di gruppi, l'apertura di una
copia nel visualizzatore per confrontarle a schermo intero, il confronto affiancato, il percorso
completo di ciascuna copia (si vede solo il nome del file, non la cartella), la scelta automatica
"tieni la più grande / la più recente / quella in questa cartella", la possibilità di tenere più
di una copia.

### 4. Interazioni da mouse

- **Click su una miniatura** → diventa la copia da tenere.
- **Hover su una miniatura** → l'opacità sale da `.55` a `.8` (`opacity .12s ease`): comunica che
  è cliccabile, e che le copie non scelte sono "in secondo piano".
- **Click su `"Risolvi gruppo"`** → dialog di eliminazione.
- **Click su `"Ignora"`** → il gruppo sparisce.
- **Rotellina / trascinamento orizzontale sulla striscia**: la striscia ha `overflow-x:auto`,
  quindi con più copie di quante ne stanno in larghezza si può scorrere; non c'è però alcun
  gestore di trascinamento personalizzato.
- **Tasto destro, doppio click**: nessun comportamento.

### 5. Interazioni da tastiera

- Le miniature sono `tabindex="0"` e legate con `bindActivatable` → **Invio** e **Spazio**
  eleggono la copia da tenere (SP-8). Sono l'unico elemento con `aria-pressed` del blocco.
- `"Risolvi gruppo"` e `"Ignora"` sono `tabindex="0"` e attivabili con **Invio** e **Spazio**.
- **Nessuna navigazione con le frecce** fra le copie di un gruppo: si passa dall'una all'altra
  solo con Tab.
- **Escape** non ha effetto sulla pagina (agisce solo dentro il dialog aperto da
  `"Risolvi gruppo"`).
- Ordine del focus: per ogni gruppo, prima tutte le miniature da sinistra a destra, poi
  `"Risolvi gruppo"`, poi `"Ignora"`, poi il gruppo successivo.

### 6. Animazioni e transizioni

- **Miniature**: `transition: opacity .12s ease, outline-color .12s ease`. Tre livelli di opacità
  comunicano lo stato: `.55` non scelta, `.8` in hover, `1` scelta. Il contorno
  (`outline:2px solid`) passa da trasparente a `--accent` sulla copia scelta, con la stessa
  transizione — è il segnale principale di "questa è la copia che resta".
- **Etichetta sotto la miniatura**: sulla copia scelta diventa colore `--accent` e peso 600
  (senza transizione).
- **Pallino con la spunta**: viene aggiunto/tolto dal DOM, quindi compare di colpo.
- **Focus da tastiera**: `.dup-thumb-wrap:focus-visible .bulk-thumb` colora il contorno di
  accento — cioè **il focus è visivamente identico allo stato "scelta"**, il che rende
  ambiguo, navigando col Tab, quale sia davvero la copia eletta (la differenza resta solo
  nell'opacità e nel pallino).
- **Sparizione di un gruppo**: istantanea al ridisegno, nessuna animazione di uscita.
- Toast: SP-6.

### 7. Stati per ogni controllo

- **Miniatura** — non scelta (opacità `.55`, contorno trasparente, etichetta terziaria) / hover
  (opacità `.8`) / **scelta** (opacità `1`, contorno accento, pallino con spunta, etichetta
  accento in grassetto, `aria-pressed="true"`) / focus (contorno accento). Mai disabilitata.
  **In ogni gruppo c'è sempre esattamente una copia scelta**: cliccando su un'altra la scelta si
  sposta, non si può azzerare né sceglierne due.
- **`"Risolvi gruppo"`** — normale (`.btn.btn-sm`, bordo `--border-strong`, sfondo carta) /
  hover (`--chip-bg`) / focus (outline accento). Mai disabilitato.
- **`"Ignora"`** — ghost, hover `--chip-bg`, focus outline. Mai disabilitato.
- **Vuoto** — schermata dedicata con icona `check` (positiva) e la spiegazione del criterio.
  **Attenzione:** dopo aver risolto o ignorato tutti i gruppi si atterra proprio in questo stato
  vuoto, che dice `"Nessun duplicato trovato"` — anche se i gruppi non sono stati "non trovati"
  ma solo evasi. Non c'è modo, dall'interfaccia, di far rieseguire la scansione dei duplicati.
- **Caricamento / errore** — non implementati: non c'è nessuna indicazione di "scansione in
  corso", né la data dell'ultima scansione.

### 8. Da dove ci si arriva e dove si va

**In ingresso:** sidebar desktop → gruppo `"Manutenzione"` → `"Duplicati"`; mobile: tab
`"Altro"` → sezione `"Manutenzione"` → `"Duplicati"`. Non ci si arriva da nessun'altra
schermata; nessun collegamento dalla pagina Problemi, nessun badge di notifica sulla voce di
sidebar.

**In uscita:** `"Risolvi gruppo"` apre il dialog di eliminazione **sopra** la pagina (si resta
qui). Nessuna azione porta a un'altra vista.

### 9. Dati necessari a questa schermata

**Legge:** l'elenco dei gruppi di file con contenuto identico; per ogni gruppo, il motivo
probabile della duplicazione in linguaggio naturale e l'elenco dei file coinvolti; per ogni file,
nome, miniatura, dimensione in MB e se è un RAW. Serve inoltre un'indicazione di quale copia
proporre come "da tenere".

**Scrive:** quale copia tenere per ciascun gruppo (scelta dell'utente), l'esito della risoluzione
del gruppo (con la modalità di eliminazione scelta fra le tre di §9 applicata a tutte le altre
copie) e il fatto che un gruppo è stato **ignorato** e non va più segnalato.

**Nota per l'architetto backend:** nel mockup i file di un gruppo duplicati sono record leggeri
**indipendenti dal catalogo** (non sono oggetti-foto), e risolvere un gruppo **non applica
davvero** la modalità di eliminazione scelta: il gruppo viene semplicemente tolto dall'elenco. La
modalità scelta viene raccolta ma non usata. Nel sistema reale la scelta va invece propagata alle
copie da eliminare esattamente come in §9.

---

## 47. Problemi

### 1. Nome e scopo

Pagina unica dove confluiscono tutti gli errori e le eccezioni della libreria, ciascuno con
l'azione che serve a risolverlo.

### 2. Cosa mostra

Il commento di sezione (riga 5021) rimanda alla specifica di prodotto e ne elenca la **portata
prevista**: *"Pagina unica per errori/eccezioni (design doc §"Pagina Problemi"): file corrotti,
librerie offline, job falliti, sidecar non scrivibili."*

**Sono quindi quattro le famiglie di problema previste dal disegno**, ma **solo due sono
rappresentate nel mockup**. Le riporto tutte, distinguendo chiaramente:

| Tipo di problema | Presente nel mockup? | Dicitura esatta |
|---|---|---|
| Sidecar XMP non scrivibile | **sì** | `"3 file con sidecar XMP non scrivibile"` |
| Libreria offline | **sì** | `"Libreria offline: Lago di Braies"` |
| File corrotti | no — citato solo nel commento e nello stato vuoto | — |
| Job falliti | no — citato solo nel commento e nello stato vuoto | — |

**Stato vuoto:** icona `check`, `"Nessun problema rilevato"`, `"File corrotti, librerie offline o
job falliti compariranno qui."` (nota: lo stato vuoto elenca tre famiglie e **omette** i sidecar,
che è invece il problema effettivamente presente nel mockup).

**Con problemi:**

- titolo `"Problemi"`;
- sottotitolo `"<N> elementi richiedono attenzione"`;
- una riga per problema (`.problem-row`), composta da:
  - **icona di gravità** 34×34 (`.problem-ico`, raggio 9px). Due livelli:
    - **avviso** (`.warn`): icona `alert` 17px, colore `--accent` su sfondo `--accent-tint`;
    - **errore**: icona `close` 17px, colore `--danger` su sfondo `--danger-tint`;
  - **titolo del problema** (13.5px bold);
  - **descrizione** (12px, colore terziario);
  - **pulsanti di azione** (`.problem-actions`, gap 8px): il **primo è un pulsante normale**, i
    successivi **ghost** — una gerarchia implicita che indica l'azione consigliata.

**I due problemi del mockup, per esteso:**

**A — gravità "avviso"**
- titolo: `"3 file con sidecar XMP non scrivibile"`
- descrizione: `"Chioggia e Venezia — permessi di scrittura mancanti sulla cartella. Il rating
  resta salvato nel database, ma non sincronizzato sul file."`
- cartella coinvolta: *Chioggia e Venezia*
- azioni: `"Vedi i 3 file"` (primaria) e `"Ignora"` (ghost)

**B — gravità "errore"**
- titolo: `"Libreria offline: Lago di Braies"`
- descrizione: `"Il percorso di rete non risponde da 2 giorni. Le foto restano visibili (dalla
  cache) ma non è possibile importarne di nuove."`
- cartella coinvolta: *Lago di Braies*
- azioni: `"Riprova connessione"` (primaria) e `"Dettagli"` (ghost)

Non è mostrato: quando il problema è stato rilevato, quante volte si è ripetuto, un filtro per
gravità, un raggruppamento, un contatore per tipo.

### 3. Ogni controllo, uno per uno

Le azioni sono definite **per problema**, non globalmente. Le quattro azioni implementate:

| Chiave | Etichetta | Su quale problema | Cosa fa |
|---|---|---|---|
| `viewFiles` | `"Vedi i 3 file"` | sidecar XMP | Apre il **dialog "file con problemi"** (§8) con l'elenco dei file coinvolti. Il problema **resta** nell'elenco |
| `ignore` | `"Ignora"` | sidecar XMP | Rimuove il problema dall'elenco. Toast `"Problema ignorato."` Nessuna conferma, nessun annullamento, il problema non torna |
| `retry` | `"Riprova connessione"` | libreria offline | Toast `"Verifica della connessione in corso…"`, il pulsante viene sbiadito a opacità `.6`; dopo **700 ms** il problema viene rimosso e appare il toast `Connessione ripristinata — Lago di Braies di nuovo online.` **Nel mockup il tentativo riesce sempre**: non esiste il ramo "riprovato e ancora offline" |
| `details` | `"Dettagli"` | libreria offline | Apre il **dialog informativo** (§10) intitolato `"Libreria offline: dettagli"` |

**Contenuto del dialog `"Dettagli"`**, alla lettera:
- riga introduttiva: `"Keeppix indicizza questa cartella da un percorso di rete (NAS/SMB), non dal
  disco locale del server."`
- elenco puntato:
  1. `"Ultimo contatto riuscito: 2 giorni fa"`
  2. `"Percorso: //nas-casa/foto/lago-di-braies"` (il percorso è in carattere monospaziato)
  3. `"Le foto già indicizzate restano visibili dalla cache locale; import e culling di nuovi
     file sono sospesi finché il percorso non torna raggiungibile"`

Il commento a `attachProblemHandlers` chiarisce l'intento del rifacimento: *"azioni della pagina
Problemi: prima erano pulsanti senza alcun comportamento collegato — "Vedi i file" non faceva
nulla. Ognuna ora fa qualcosa di reale nel mockup."*

**Non previsto nel mockup:** azione "risolvi tutto", segnalazione di un problema come risolto
manualmente, riapertura di un problema ignorato, esportazione del log, collegamento diretto alle
Impostazioni della cartella coinvolta, notifica push (esiste però l'interruttore
`"Problemi rilevati"` nelle notifiche in Impostazioni).

### 4. Interazioni da mouse

- Click sui pulsanti di azione, come da tabella.
- Hover: `.btn:hover` → `--chip-bg`; `.btn-ghost:hover` → `--chip-bg`. Nessuna transizione.
- **La riga del problema non è cliccabile nel suo insieme**: solo i pulsanti lo sono.
- Tasto destro, doppio click, trascinamento: nessun comportamento.

### 5. Interazioni da tastiera

- Tutti i pulsanti di azione hanno `role="button"` e `tabindex="0"` e sono legati con
  `bindActivatable` → **Invio** e **Spazio** (SP-8).
- Ordine del focus: per ogni problema, azione primaria poi azioni ghost, quindi il problema
  successivo.
- Escape non ha effetto sulla pagina (solo dentro i dialog).
- Nessuna navigazione con le frecce, nessuna scorciatoia dedicata.

### 6. Animazioni e transizioni

- **`"Riprova connessione"`**: il pulsante viene sbiadito con uno stile in linea
  (`opacity:.6`), **senza transizione dichiarata** — quindi il cambio è istantaneo, non una
  dissolvenza. Comunica "sto lavorando, non ripremere". Non viene mai ripristinato, perché dopo
  700 ms l'intera riga sparisce. **Non è un vero indicatore di caricamento**: non c'è
  rotellina, non c'è testo "in corso" nel pulsante.
- **Ritardo simulato di 700 ms** fra il toast di avvio e quello di esito: serve a rendere
  credibile una verifica di rete.
- **Rimozione della riga**: istantanea, senza animazione di uscita.
- Toast: SP-6.

### 7. Stati per ogni controllo

- **Pulsante primario del problema** — normale / hover (`--chip-bg`) / focus (outline accento) /
  **"in corso"** solo per `"Riprova connessione"`, reso con l'opacità `.6`. **Non viene mai
  disabilitato**: durante i 700 ms si può premere di nuovo, e il codice non lo impedisce (la
  seconda pressione riesegue il tutto; alla scadenza del primo timer il problema è già stato
  rimosso e il secondo tentativo mostrerebbe comunque il toast di successo).
- **Pulsanti ghost** — normale / hover / focus. Mai disabilitati.
- **Icona di gravità** — due varianti (avviso accento / errore rosso), nessuno stato
  interattivo: non è cliccabile e non ha tooltip.
- **Vuoto** — schermata dedicata con icona positiva.
- **Errore della pagina stessa / caricamento** — non implementati.

### 8. Da dove ci si arriva e dove si va

**In ingresso:** sidebar desktop → gruppo `"Manutenzione"` → `"Problemi"`; mobile: tab `"Altro"`
→ sezione `"Manutenzione"` → `"Problemi"`. **Non c'è badge di notifica** sulla voce di sidebar, a
differenza di `"Culling"` e `"Revisione"`: il numero di problemi aperti non è visibile finché non
si entra nella pagina.

**In uscita:**
- `"Vedi i 3 file"` → dialog §8, e da lì, aprendo un file, si esce **davvero** da questa pagina:
  la cartella corrente diventa quella del problema, la vista passa a `"Foto"` e si apre il
  visualizzatore su quel file;
- `"Dettagli"` → dialog informativo sopra la pagina (si resta qui);
- le altre azioni restano sulla pagina.

Esiste inoltre, in Impostazioni → notifiche, un interruttore `"Problemi rilevati"` che governa
(in prospettiva) l'avviso di nuovi problemi.

### 9. Dati necessari a questa schermata

**Legge:** l'elenco dei problemi aperti; per ciascuno un identificativo, il livello di gravità
(avviso / errore), un titolo e una descrizione già in linguaggio naturale, la cartella o libreria
coinvolta, e l'elenco delle azioni proposte con la loro etichetta.

**Scrive:** la rimozione di un problema quando viene ignorato o quando l'azione correttiva
riesce. Nel caso della libreria offline, l'azione di ritentativo richiede al backend una verifica
di raggiungibilità del percorso di rete.

---

## 48. Dialog "file con problemi"

### 1. Nome e scopo

Dialog modale che elenca i file concretamente coinvolti in un problema e permette di aprirne uno
nel visualizzatore per vedere di cosa si tratta.

### 2. Cosa mostra

Scrim e scheda modale standard (larghezza 360px desktop / 86% mobile).

- **titolo** = il titolo del problema, riportato tale e quale (es. `"3 file con sidecar XMP non
  scrivibile"`);
- **sottotitolo** = la descrizione del problema, tale e quale;
- **elenco dei file** (`.album-picker-list`, altezza massima 260px, scorrevole), **fisso a tre
  file**. Ogni riga (`.album-picker-row`) mostra:
  - quadratino 36×36 con il gradiente della foto;
  - **nome del file** in grassetto e, sotto, la **data** nel formato `"<giorno> <mese in
    minuscolo> 2026"` (es. `"14 marzo 2026"`), in 11.5px colore terziario;
  - a destra, la parola `"Apri"` nello stile dei link della barra di selezione;
- **pulsante `"Chiudi"`** (ghost, piccolo).

**Avvertenza importante** — il commento al codice è onesto: *"dialog "Vedi i file": elenco reale
dei file coinvolti (prime N foto della cartella), con apertura diretta nel visualizzatore — non
solo un elenco statico"*. In pratica i file elencati sono le **prime tre foto della cartella
coinvolta**, non i tre file che hanno davvero il sidecar non scrivibile. Nel mockup coincide con
il `"3"` del titolo per costruzione, ma è un numero fisso: se un problema riguardasse 12 file, ne
mostrerebbe comunque 3, senza dire "e altri 9".

### 3. Ogni controllo, uno per uno

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | Riga file (una per file, tre in tutto) | area cliccabile (`role="button"`, `tabindex="0"`, `aria-label="Apri <nome file>"`) | Chiude il dialog, imposta la cartella corrente su quella del problema, porta la vista a `"Foto"` e apre il visualizzatore su quel file |
| 2 | `"Apri"` | testo dentro la riga | **Non è un controllo separato**: è un'etichetta visiva; il click su di essa attiva la riga che la contiene |
| 3 | `"Chiudi"` | pulsante ghost piccolo | Chiude il dialog e riporta il focus al pulsante d'origine |

Non c'è alcuna azione correttiva dentro questo dialog: non si possono correggere i permessi, né
riprovare la scrittura del sidecar, né ignorare da qui. È **solo consultazione + apertura**.

### 4. Interazioni da mouse

- Click su una riga → apre la foto (e abbandona la pagina Problemi).
- Hover sulla riga → sfondo `--chip-bg`, senza transizione.
- **Click sullo scrim: non chiude** (nessun gestore). Deviazione da SP-5.
- Tasto destro, doppio click, trascinamento: nessun comportamento.

### 5. Interazioni da tastiera

- **All'apertura il focus va sulla prima riga file**; se non ci fossero file, andrebbe sul
  pulsante `"Chiudi"` (il codice prevede esplicitamente questo ripiego).
- Righe e `"Chiudi"` attivabili con **Invio** e **Spazio** (SP-8).
- **Escape chiude** (gestore su `document`).
- **Nessuna trappola del focus**: col Tab si esce dal dialog. Deviazione da SP-5.
- Alla chiusura il focus torna all'elemento che aveva aperto il dialog — ma **solo se il dialog
  viene chiuso con `"Chiudi"` o Escape**: aprendo un file, il ritorno del focus avviene comunque,
  ma subito dopo l'app cambia vista e apre il visualizzatore, quindi è irrilevante.

### 6. Animazioni e transizioni

Nessuna: né entrata, né uscita, né transizioni sugli hover delle righe. La transizione visiva più
forte è il passaggio brusco alla vista `"Foto"` con il visualizzatore aperto quando si apre un
file.

### 7. Stati per ogni controllo

- **Riga file** — normale / hover (`--chip-bg`) / focus (`.album-picker-row:focus-visible` →
  outline 2.5px accento). Mai disabilitata.
- **`"Chiudi"`** — normale / hover / focus. Mai disabilitato.
- **Vuoto** — se la cartella non avesse foto, l'elenco resterebbe vuoto **senza alcun
  messaggio**. Caso non raggiungibile con i dati del mockup.
- **Caricamento / errore** — non implementati.

### 8. Da dove ci si arriva e dove si va

**In ingresso:** unicamente dall'azione `"Vedi i 3 file"` della pagina Problemi. È l'unico punto
di richiamo di questo dialog.

**In uscita:** `"Chiudi"` o **Escape** → torna alla pagina Problemi con il problema ancora
presente; click su un file → cartella coinvolta + vista `"Foto"` + **visualizzatore aperto** su
quel file. Chiudendo poi il visualizzatore ci si ritrova nella timeline della cartella, **non**
nella pagina Problemi: la navigazione è a senso unico.

### 9. Dati necessari a questa schermata

**Legge:** titolo e descrizione del problema; la cartella coinvolta; e l'elenco dei file
effettivamente interessati dal problema, con per ciascuno il nome del file, la miniatura e la
data di scatto.

**Scrive:** nulla sui dati. Cambia solo lo stato di navigazione (cartella corrente, vista,
visualizzatore aperto).

---

## 49. Dialog di eliminazione a 3 opzioni — definizione canonica

> Questa è la definizione di riferimento del dialog di eliminazione. Le altre schermate che lo
> aprono (visualizzatore foto, barra `"N selezionate"`, pagina Duplicati) rimandano qui.

### 1. Nome e scopo

Dialog modale che, ogni volta che l'utente elimina qualcosa, gli chiede **in che modo** eliminarlo
fra tre alternative con conseguenze molto diverse su disco e sull'indice — senza mai scegliere al
suo posto.

### 2. Cosa mostra

Scrim (`--scrim`) e scheda modale standard: larghezza **360px** su desktop, **86%** su mobile,
sfondo `--card-bg`, bordo `--border-strong`, raggio 12px, padding 18px, ombra
`0 20px 50px rgba(0,0,0,.3)`, `role="dialog"`, `aria-modal="true"`, collegata al proprio titolo
via `aria-labelledby`.

**Titolo — cambia in base a chi apre il dialog** (è l'unica parte variabile):

| Chi lo apre | Titolo |
|---|---|
| Visualizzatore foto, su una foto singola | `Eliminare "<nome del file>"?` — con il nome file completo fra virgolette, es. `Eliminare "DSC_0421.NEF"?` |
| Barra `"N selezionate"` (eliminazione di massa) | `Eliminare <N> foto?` |
| Pagina Duplicati, `"Risolvi gruppo"` | `Eliminare <N> copie duplicate?` (singolare: `Eliminare 1 copia duplicata?`) |

**Sottotitolo — sempre lo stesso, alla lettera:**

> `"Keeppix chiede sempre come procedere — non c'è un comportamento predefinito implicito."`

È una dichiarazione di principio del prodotto: nessuna preferenza salvata, nessuna casella
"non chiedermelo più", nessuna delle tre opzioni preselezionata.

**Le tre opzioni**, in quest'ordine dall'alto verso il basso — dalla meno alla più distruttiva.
Ogni opzione è un blocco cliccabile (`.modal-option`, bordo `--border-strong`, raggio 9px,
padding 10px 12px, margine inferiore 8px) con **titolo in grassetto** su una riga e
**sottotitolo esplicativo** sulla riga sotto:

---

**Opzione 1 — `"Rimuovi solo dall'indice"`**
Sottotitolo, alla lettera:
> `"Il file resta sul disco, verrà re-indicizzato alla prossima scansione della cartella."`

*Cosa fa al file su disco:* **niente.** Il file rimane esattamente dov'è, con lo stesso nome e
percorso.
*Cosa fa all'indice:* toglie la voce dal catalogo di Keeppix. La foto sparisce da Timeline,
album, ricerche, preferiti.
*Conseguenza pratica da spiegare all'utente:* **è temporanea per costruzione.** Alla prossima
scansione della cartella il file viene ritrovato e re-indicizzato, quindi ricomparirà. Serve a
"nascondere per ora", non a eliminare. Non finisce nel Cestino.

---

**Opzione 2 — `"Sposta nel cestino di Keeppix"`**
Sottotitolo, alla lettera:
> `"Spostato in .keeppix-trash nella stessa libreria. Recuperabile per 30 giorni."`

*Cosa fa al file su disco:* lo **sposta fisicamente** in una cartella nascosta `.keeppix-trash`
**dentro la stessa libreria** — non nel cestino del sistema operativo, e non su un altro volume.
Lo spostamento resta quindi all'interno dello stesso filesystem/condivisione, il che lo rende
un'operazione atomica ed economica anche su NAS.
*Cosa fa all'indice:* la voce resta nel catalogo ma marcata come "in cestino", così da poter
essere ripristinata.
*Conseguenza pratica:* **è l'unica delle tre reversibile dall'interfaccia.** È l'unica che
alimenta la pagina **Cestino** (§5), dove la foto compare con il conto alla rovescia e i pulsanti
di ripristino/eliminazione definitiva. La promessa dei 30 giorni è dichiarata qui, nel
sottotitolo del Cestino e nel suo stato vuoto — ma la scadenza automatica **non è implementata
nel mockup**.

---

**Opzione 3 — `"Elimina dal disco adesso"`** *(variante `.danger`)*
Sottotitolo, alla lettera:
> `"Azione irreversibile: il file viene cancellato definitivamente."`

*Cosa fa al file su disco:* lo **cancella**, subito, definitivamente. Non passa dal
`.keeppix-trash`, non passa dal cestino di sistema.
*Cosa fa all'indice:* toglie la voce dal catalogo, senza possibilità di ripristino.
*Conseguenza pratica:* **non c'è modo di tornare indietro** e **non c'è un secondo passaggio di
conferma**: il click su questa opzione è già la conferma definitiva. L'unica difesa è la
segnalazione visiva (titolo dell'opzione in colore `--danger`, sfondo `--danger-tint` in hover) e
il fatto che sia l'ultima delle tre.

---

**Differenza pratica fra le tre, in una riga ciascuna:**

| Opzione | Il file su disco | La voce nell'indice | Recuperabile? | Ricompare da sola? |
|---|---|---|---|---|
| `"Rimuovi solo dall'indice"` | resta intatto dov'è | rimossa | non serve: il file c'è ancora | **sì**, alla prossima scansione |
| `"Sposta nel cestino di Keeppix"` | spostato in `.keeppix-trash` | marcata "in cestino" | **sì**, dal Cestino, per 30 giorni | no |
| `"Elimina dal disco adesso"` | **cancellato** | rimossa | **no** | no |

**Ultimo controllo:** il pulsante `"Annulla"` (ghost, piccolo), in fondo, staccato dalle opzioni.

### 3. Ogni controllo, uno per uno

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Rimuovi solo dall'indice"` | opzione cliccabile (`role="button"`, `tabindex="0"`) | Chiude il dialog restituendo la scelta "indice" |
| 2 | `"Sposta nel cestino di Keeppix"` | opzione cliccabile | Chiude restituendo la scelta "cestino" |
| 3 | `"Elimina dal disco adesso"` | opzione cliccabile, variante distruttiva | Chiude restituendo la scelta "disco" |
| 4 | `"Annulla"` | pulsante ghost piccolo | Chiude **senza scelta**: il chiamante non fa nulla |

**Cosa fa il chiamante con la scelta:**
- dal visualizzatore su una foto singola: marca la foto come scartata, vi registra la modalità
  scelta, e **chiude il visualizzatore**;
- dalla barra di selezione: marca **tutte** le foto selezionate come scartate con la stessa
  modalità, mostra il toast `"<N> foto eliminate."` e **azzera la selezione** — l'azzeramento
  avviene **anche se si annulla**;
- dalla pagina Duplicati: rimuove il gruppo dall'elenco e mostra il toast
  `"<N> copie rimosse, mantenuta <nome file>."`

**Non previsto nel mockup:** una preferenza "ricorda questa scelta", un'opzione predefinita
preselezionata, un annullamento post-azione ("Annulla" nel toast), l'indicazione di quanto spazio
si libera, un elenco/anteprima delle foto che si stanno per eliminare, un secondo passaggio di
conferma per l'opzione distruttiva.

### 4. Interazioni da mouse

- Click su una delle tre opzioni → chiude e applica.
- Hover sulle opzioni 1 e 2 → sfondo `--chip-bg`; hover sull'opzione 3 → sfondo `--danger-tint`
  (rinforza il segnale di pericolo). Nessuna transizione dichiarata: il cambio è istantaneo.
- Hover su `"Annulla"` → `--chip-bg`.
- **Click sullo scrim: NON chiude il dialog.** Non c'è alcun gestore sul contenitore. È una scelta
  difendibile per un dialog distruttivo (evita chiusure accidentali) ma resta una deviazione da
  SP-5 da documentare.
- Tasto destro, doppio click, trascinamento, rotellina: nessun comportamento.

### 5. Interazioni da tastiera

- **All'apertura il focus va sulla prima opzione**, `"Rimuovi solo dall'indice"` — cioè la **meno
  distruttiva**. È una scelta deliberata: chi preme Invio d'istinto compie l'azione più
  innocua. Il commento al codice conferma l'intento accessibile: *"Dialog accessibile: role=dialog,
  chiusura con Esc, focus alla prima opzione all'apertura e ritorno al trigger alla chiusura."*
- Le tre opzioni e `"Annulla"` sono legate con `bindActivatable` → **Invio** e **Spazio** (SP-8).
- **Escape chiude senza scegliere** (equivale ad `"Annulla"`). Il gestore è su `document`, quindi
  funziona ovunque sia il focus.
- Tab / Shift+Tab scorrono nell'ordine: opzione 1 → opzione 2 → opzione 3 → `"Annulla"`.
- **Non c'è trappola del focus:** oltre `"Annulla"` il Tab esce dal dialog e raggiunge i controlli
  della pagina sottostante. Deviazione da SP-5.
- **Alla chiusura il focus torna all'elemento che aveva aperto il dialog** (memorizzato
  all'apertura come `document.activeElement`), sia che si sia scelto sia che si sia annullato.

### 6. Animazioni e transizioni

**Nessuna.** Il dialog viene aggiunto e rimosso dal DOM senza dissolvenza dello scrim né
scalatura della scheda; le opzioni non hanno transizione sul cambio di sfondo in hover. La
gravità è comunicata solo dal colore e dall'ordine, non dal movimento.

### 7. Stati per ogni controllo

- **Opzioni 1 e 2** — normale: bordo `--border-strong`, titolo 13px colore testo pieno,
  sottotitolo 12.5px colore `--text-secondary`. Hover: sfondo `--chip-bg`. Focus:
  `.modal-option:focus-visible` → outline 2.5px accento, offset 2px. Nessuno stato premuto,
  selezionato o disabilitato: **le opzioni non sono mai disabilitate e nessuna è preselezionata**.
- **Opzione 3** — identica, ma con il **titolo in colore `--danger`** e hover su
  `--danger-tint`. Nessuna conferma aggiuntiva.
- **`"Annulla"`** — normale (ghost, trasparente) / hover `--chip-bg` / focus outline. Mai
  disabilitato.
- **Stato di caricamento** — non implementato: il dialog non attende l'esito dell'operazione, si
  chiude subito.
- **Stato di errore** — non implementato: non c'è modo, in questo dialog, di segnalare che
  l'eliminazione dal disco è fallita (permessi, file in uso, libreria offline). **Per l'architetto
  backend è il buco più rilevante**: nel sistema reale l'opzione 3 può fallire, e il mockup non
  prevede dove mostrarlo.
- **Il dialog non si apre affatto** se la selezione è vuota (controllo a monte nel chiamante).

### 8. Da dove ci si arriva e dove si va

**In ingresso — tre punti, tutti documentati:**

1. **Visualizzatore foto (lightbox)**, pulsante di eliminazione — su desktop;
2. **Visualizzatore foto**, menu `⋯`, voce di eliminazione — su mobile;
3. **Barra `"N selezionate"` (SP-2)**, pulsante `"Elimina"` — da Timeline, Preferiti, Cerca,
   dettaglio Album, con etichetta accessibile `"Elimina selezione"`;
4. **Pagina Duplicati**, `"Risolvi gruppo"`.

**Non ci si arriva da**: Culling (dove si usano Scelta/Scarta, che spostano fisicamente il file
fra le sotto-aree del lotto, non il cestino — vedi il commento a riga 4289), Cestino (dove
l'eliminazione definitiva è immediata e senza dialog), pagina Cartelle, Impostazioni.

**In uscita:** si torna sempre esattamente alla schermata di partenza, con il focus sul pulsante
d'origine. L'unica eccezione è l'eliminazione dal visualizzatore, che dopo la scelta **chiude
anche il visualizzatore** e riporta alla griglia sottostante.

### 9. Dati necessari a questa schermata

**Legge:** il nome del file (per l'eliminazione singola) oppure il numero di elementi coinvolti
(per l'eliminazione multipla e per i duplicati). Nient'altro: il dialog non mostra miniature,
percorsi, dimensioni né spazio recuperabile.

**Scrive:** per ciascun elemento coinvolto, lo stato "scartato" e **la modalità di eliminazione
scelta fra le tre** — è quest'ultimo dato che il backend deve interpretare: rimozione della sola
voce di catalogo, spostamento in `.keeppix-trash` con data di scadenza a 30 giorni, oppure
cancellazione definitiva dal filesystem.

---

## 50. Dialog generici riutilizzati: informazione e conferma

### 1. Nome e scopo

Due dialog modali minimi e riutilizzabili: uno che mostra soltanto un'informazione da leggere e
chiudere, l'altro che chiede una conferma prima di un'azione distruttiva.

### 2. Cosa mostra

**Dialog informativo** — scheda modale standard con:
- **titolo**, passato da chi lo apre (es. `"Libreria offline: dettagli"`);
- **corpo libero**: un frammento di contenuto formattato deciso dal chiamante (paragrafo di
  spiegazione, elenco puntato, testo monospaziato per i percorsi…). Non ha una struttura fissa;
- **pulsante `"Chiudi"`** (ghost, piccolo, con margine superiore di 14px).

Non ha sottotitolo fisso, né icona, né livelli di gravità.

**Dialog di conferma** — scheda modale standard con:
- **titolo**, passato dal chiamante;
- **sottotitolo** (`.modal-sub`, 12px colore terziario), passato dal chiamante: è qui che si
  spiegano le conseguenze dell'azione;
- **due pulsanti affiancati** (gap 8px):
  - a **sinistra**, il pulsante distruttivo (`.btn.btn-danger`, bordo e testo `--danger`, sfondo
    trasparente), la cui **etichetta è decisa dal chiamante** — non è fissa;
  - a **destra**, il pulsante ghost `"Annulla"`, che è invece **sempre lo stesso**.

Il commento al codice ne dichiara la ragione d'essere: *"conferma generica a due pulsanti
(Annulla / azione distruttiva) — usata da eliminazione tag e categorie, invece di
window.confirm"*.

**Da notare per coerenza:** questo dialog di conferma esiste ed è disponibile, ma **non viene
usato** dove ce ne sarebbe più bisogno in questo blocco — `"Svuota cestino"`, l'eliminazione
definitiva di un singolo elemento dal cestino, `"Ignora"` di un problema e `"Ignora"` di un
gruppo duplicati agiscono tutti **senza conferma**.

### 3. Ogni controllo, uno per uno

**Dialog informativo:**

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Chiudi"` | pulsante ghost piccolo | Chiude e riporta il focus al pulsante d'origine |

**Dialog di conferma:**

| # | Etichetta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | (etichetta variabile, decisa dal chiamante) | pulsante danger | Chiude il dialog **e poi** esegue l'azione |
| 2 | `"Annulla"` | pulsante ghost | Chiude senza fare nulla |

Nessuno dei due dialog ha campi, caselle o menu: sono controlli di sola conferma.

### 4. Interazioni da mouse

- Click sui pulsanti come sopra.
- Hover: `--chip-bg` per i ghost, `--danger-tint` per il pulsante distruttivo.
- **Click sullo scrim: non chiude** né l'uno né l'altro. Deviazione da SP-5, coerente con tutti
  gli altri dialog di questo blocco.
- Tasto destro, doppio click, trascinamento: nessun comportamento.

### 5. Interazioni da tastiera

- **Dialog informativo:** all'apertura il focus va su `"Chiudi"`, che è l'unico controllo;
  **Invio** e **Spazio** lo attivano; **Escape** chiude.
- **Dialog di conferma:** all'apertura il focus va su **`"Annulla"`**, non sul pulsante
  distruttivo — stessa filosofia del dialog di eliminazione (§9), l'azione d'istinto è quella
  sicura. Entrambi i pulsanti sono attivabili con **Invio** e **Spazio**; **Escape** chiude
  **senza confermare**.
- Alla chiusura, in entrambi, il focus torna all'elemento che li aveva aperti.
- **Nessuna trappola del focus** in nessuno dei due: col Tab si esce dal dialog.
- Nessuna scorciatoia dedicata (nessun `Y`/`N`, nessun Ctrl+Invio).

### 6. Animazioni e transizioni

Nessuna, in entrambi. Nessuna dissolvenza dello scrim, nessuna scalatura della scheda, nessuna
transizione sugli hover dei pulsanti.

### 7. Stati per ogni controllo

- **`"Chiudi"` / `"Annulla"`** — normale (ghost, trasparente) / hover `--chip-bg` / focus
  (outline 2.5px accento, offset 2px). Mai disabilitati.
- **Pulsante distruttivo del dialog di conferma** — normale (bordo e testo `--danger`, sfondo
  trasparente) / hover `--danger-tint` / focus (outline accento). Mai disabilitato, nessuno stato
  di caricamento: il dialog si chiude **prima** che l'azione venga eseguita, quindi non può
  mostrare né avanzamento né errore.
- **Corpo del dialog informativo** — nessuno stato: è contenuto statico.

### 8. Da dove ci si arriva e dove si va

**Dialog informativo — in ingresso:** dall'azione `"Dettagli"` della pagina Problemi (§7). È
l'unico richiamo che riguarda questo blocco; il componente è però generico e utilizzabile da
qualunque altra schermata.

**Dialog di conferma — in ingresso:** non è richiamato da nessuna schermata di questo blocco.
Viene usato nella gestione dei **tag e delle categorie**, per la loro eliminazione, in
sostituzione della finestra di conferma nativa del browser.

**In uscita:** entrambi tornano sempre alla schermata di partenza, con il focus sul controllo
d'origine. Il dialog di conferma, se confermato, lascia poi che sia il chiamante a eseguire
l'azione — che può a sua volta cambiare la schermata.

### 9. Dati necessari a questa schermata

**Dialog informativo — legge:** un titolo e un contenuto già pronto per la lettura, forniti da chi
lo apre. **Scrive:** nulla.

**Dialog di conferma — legge:** un titolo, una spiegazione delle conseguenze e l'etichetta da dare
al pulsante distruttivo. **Scrive:** nulla di suo — restituisce soltanto il consenso dell'utente
al chiamante, che è chi esegue e registra l'azione.

---

# Parte VIII — Organizzazione automatica

> Contiene 8 sezioni. Le sezioni 1–6 sono schermate/dialog e seguono la struttura a 9
> sottotitoli. Le sezioni 7 e 8 non sono schermate: sono le **definizioni canoniche** dei
> pattern trasversali **SP-11** (livelli IA) e **SP-12** (provenienza IA vs utente), e hanno
> quindi una struttura propria. La definizione canonica di **SP-10** (coda di conferma IA) è
> in coda alla sezione 5.

---

## 51. Premessa al modello dati: tag, categoria e scena sono tre cose diverse

Sono tre concetti distinti, spesso confusi. Vanno tenuti separati anche nel backend.

**1. Il tag** (`TAGS`, riga 1450) è l'unica entità che l'utente crea e che l'IA sa abbinare.
Un tag è un oggetto con: `id`, `name` (il nome per gli umani), `prompt` (la descrizione data
al modello, può essere vuoto), `color` (una **tinta HSL 0–360**, non un colore completo: il
colore finale è sempre `hsl(<color>,60%,50%)`), `threshold` (soglia di confidenza 30–95),
`categoryId` (una sola, o `null`), più due tassi demo `autoRate`/`suggestRate`.
Il commento a riga 1429 lo dice esplicitamente: *«l'utente crea tag e categorie […] l'IA
abbina soltanto i tag esistenti — non ne inventa»*. Non esiste da nessuna parte un flusso in
cui il sistema propone un tag **nuovo**: propone solo l'abbinamento di un tag già esistente a
una foto.

**2. La categoria** (`TAG_CATEGORIES`, riga 1441) è **solo un raggruppamento con un nome**:
`{id, name}`, niente altro. Nessun colore, nessun prompt, nessuna soglia — quelli vivono sul
tag. Un solo livello di annidamento: non esistono sotto-categorie (detto sia nel commento di
riga 5296 sia nella copy del dialog: «un livello solo, niente sotto-categorie»). Un tag
appartiene a **zero o una** categoria. Una categoria non è mai assegnata a una foto: è la
gerarchia dei tag, non un'etichetta delle foto. Serve a: (a) raggruppare le righe nella pagina
"Tag e categorie"; (b) raggruppare le chip nel dettaglio foto; (c) essere una dimensione di
filtro a sé (`state.browseFilters.categoryIds`, riga 1948: una foto matcha una categoria se ha
almeno un tag **confermato** che appartiene a quella categoria). Eliminare una categoria non
elimina i tag: li lascia "senza categoria" (`deleteCategory`, riga 5383).

**3. La scena riconosciuta** (`SCENES`, riga 1268 — `tramonto, montagna, mare, citta,
ritratto, natura, architettura, notturna, neve, strada`) **non è un tag e non è mai visibile
all'utente**. È un campo nascosto `p.scene`, uno solo per foto, assegnato deterministicamente
in `genPhotos` (`SCENES[hash(id) % 10]`). Il commento a riga 1297 spiega il perché: sta al
posto del **vettore-embedding** reale che la ricerca semantica confronterebbe con la
descrizione libera. `SCENE_KEYWORDS` (riga 1501) è un dizionario parola→scene scritto a mano
(`tramonto`→`tramonto`, `alba`→`tramonto`, `lago`/`laghi`/`acqua`→`mare`,
`neve`→`neve`+`montagna`, `centro`→`citta`+`architettura`, ecc.) usato solo da
`sceneKeywordMatch()` per far filtrare qualcosa alla pagina "Cerca" (riga 4589). Il commento
avverte: *«la ricerca reale confronterebbe embedding, non parole chiave letterali»*.
**Conseguenza pratica per l'architetto:** scena ≠ tag. Cercare "tramonto" nella barra di
ricerca **non** filtra per il tag «Tramonti»: filtra per `p.scene==='tramonto'`, che è un asse
completamente indipendente. Nessuna schermata mostra mai la scena, e non esiste modo di
modificarla.

Il modello di stato per la coppia (tag, foto) è descritto nella sezione 8 (SP-12).

---

## 52. Tag e categorie — la pagina

*(`renderTagManagement`, righe 5299–5377; CSS `.tag-*` righe 918–946)*

### 1. Nome e scopo
Pagina in cui l'utente crea, modifica, raggruppa ed elimina i propri tag e le categorie che li
raggruppano — l'unico posto dove il vocabolario dei tag viene definito, dato che l'IA non può
inventarne di nuovi.

### 2. Cosa mostra
Intestazione:
- Titolo `"Tag e categorie"` (`.section-title`, 15px/700).
- Sottotitolo `"Tu crei i tag, l'IA li abbina soltanto alle foto — non può inventarne di nuovi"`
  (`.section-sub`, 12.5px, `--text-tertiary`).

Poi un blocco per **ogni categoria** in ordine di `TAG_CATEGORIES` (nel mockup: `"Natura"`,
`"Luoghi"`, `"Persone e momenti"`), e in fondo un blocco fisso `"Senza categoria"`.

Testata di ogni blocco categoria:
- Nome della categoria (13.5px/700).
- Conteggio `"N tag"` (11.5px, terziario) — numero di tag in quella categoria, **non** di foto.
- Due pulsanti icona (solo sui blocchi delle categorie vere, **non** su "Senza categoria").

Corpo del blocco: una lista bordata (`.tag-list`, bordo 1px `--border`, raggio 12px,
`overflow:hidden`), con una riga per tag. Ogni **riga tag** mostra, da sinistra a destra:
- Pallino colorato 11×11 tondo, `background:hsl(<color>,60%,50%)` — `aria-hidden`.
- Nome del tag (13.5px/600).
- *Solo se utile*: la riga `"Prompt: «...»"` (11.5px, terziario, troncata con ellissi su una
  riga). Compare **solo** se il prompt esiste, non è vuoto, e **è diverso dal nome** (confronto
  case-insensitive, riga 5302): un prompt uguale al nome non aggiunge informazione e viene
  nascosto.
- `"N foto"` (12px) = `tagConfirmedCount(t)`, cioè **solo** le coppie con `status==='confirmed'`,
  indipendentemente dall'origine (IA automatica o umana). Suggerimenti in attesa e rifiuti non
  sono contati, e **non sono mostrati da nessuna parte in questa pagina**.
- Badge soglia `"78%"` (`.tag-threshold-badge`, 10.5px/700, sfondo `--chip-bg`, pillola) con
  tooltip **nativo** `title="Sopra questa confidenza l'IA lo assegna in automatico"` — attenzione:
  è un `title` HTML, **non** il tooltip `[data-tip]` di SP-7, quindi ha il ritardo e l'aspetto
  del browser, non quelli dell'app.
- Icona matita 13px, **decorativa** (`aria-hidden`, nessun handler proprio): serve solo a far
  capire che la riga è modificabile.
- Icona cestino 13px in variante `.danger`, questa sì attivabile.

Blocco finale `"Senza categoria"`: stessa struttura, conteggio dei tag orfani (calcolato come
`!t.categoryId || categoria inesistente`, riga 5300), **nessun pulsante** di modifica/elimina.

Stati vuoti per-blocco:
- Categoria senza tag: `"Nessun tag qui ancora — crealo con \"Nuovo tag\" e assegnalo a questa categoria."`
- Blocco "Senza categoria" vuoto: `"Nessun tag fuori da una categoria."`

Non esiste uno stato vuoto per l'intera pagina: se non ci fossero né tag né categorie, si
vedrebbe comunque la toolbar più il blocco "Senza categoria" con la sua frase.

### 3. Ogni controllo, uno per uno
1. **`"Nuova categoria"`** — pulsante `.btn.btn-sm.btn-ghost` con icona `plus` 13px. Apre il
   dialog categoria in modalità creazione (sezione 3).
2. **`"Nuovo tag"`** — pulsante `.btn.btn-sm.btn-primary` con icona `plus` 13px. Apre l'editor
   tag in modalità creazione (sezione 2).
3. **Riga tag (l'intera riga)** — `role="button"`, `tabindex="0"`,
   `aria-label="Modifica tag <nome>"`. Apre l'editor tag su quel tag.
4. **Cestino sulla riga tag** — `role="button"`, `aria-label="Elimina tag <nome>"`. Ferma la
   propagazione (così non apre anche l'editor) e apre il dialog di conferma distruttiva:
   - titolo `Eliminare il tag "<nome>"?`
   - corpo, se il tag è su almeno una foto: `Verrà rimosso da N fot{a|e}. Le foto restano, perdono solo questo tag. Questa azione non elimina alcun file.`
   - corpo, se non è su nessuna foto: `Non è ancora assegnato a nessuna foto. Questa azione non elimina alcun file.`
   - pulsante di conferma `"Elimina tag"`, annulla `"Annulla"`.
   - Alla conferma: `deleteTag()` + toast `Tag "<nome>" eliminato.` + `renderAll()`.
5. **Matita sulla testata categoria** — `role="button"`, `aria-label="Rinomina categoria <nome>"`.
   Apre il dialog categoria in rinomina.
6. **Cestino sulla testata categoria** — `aria-label="Elimina categoria <nome>"`. Conferma:
   - titolo `Eliminare la categoria "<nome>"?`
   - corpo `I tag al suo interno non vengono eliminati: restano, semplicemente "senza categoria".`
   - conferma `"Elimina categoria"` → `deleteCategory()` + toast `Categoria "<nome>" eliminata.`

Non c'è altro: **niente campo di ricerca tag, niente ordinamento, niente riordino, niente
spostamento di un tag da una categoria all'altra dalla pagina** (si fa solo aprendo l'editor
del tag e cambiando la select "Categoria"). Nessun pulsante "Rianalizza" — `reanalyzeLibrary()`
esiste nel codice ma non è raggiungibile da alcun controllo di questa pagina.

### 4. Interazioni da mouse
- **Click sulla riga tag** → editor del tag. **Click sul cestino della riga** → conferma di
  eliminazione, senza aprire l'editor (`stopPropagation`, riga 5355).
- **Hover riga tag**: sfondo `var(--chip-bg)`; nessuna transizione dichiarata → cambio
  istantaneo.
- **Hover pulsanti icona**: `.tag-icon-btn:hover` sfondo `--chip-bg` e colore `--text`;
  `.tag-icon-btn.danger:hover` sfondo `--danger-tint` e colore `--danger`. Nessuna transizione.
- **Hover badge soglia**: tooltip nativo del browser (ritardo ~1s, gestito dal SO).
- **Doppio click, tasto destro, trascinamento, rotellina**: nessun comportamento specifico
  implementato. In particolare **non esiste drag&drop di un tag dentro una categoria** e
  **non esiste menu contestuale**: non previsti nel mockup.

### 5. Interazioni da tastiera
- Nessuna scorciatoia dedicata a questa pagina. Nessuna navigazione a frecce nella lista dei
  tag: non prevista nel mockup.
- Tutti i controlli sono attivabili con **Invio** e **Spazio** (SP-8, `bindActivatable`,
  riga 3190: `e.preventDefault()` e poi lo stesso handler del click).
- **Ordine del focus con Tab**: segue il DOM, cioè "Nuova categoria" → "Nuovo tag" → per ogni
  categoria: matita → cestino → riga tag 1 → cestino tag 1 → riga tag 2 → … Nota: il cestino di
  una riga è *dentro* la riga, quindi si tabula prima sulla riga e poi sul suo cestino.
- Focus visibile: `outline:2.5px solid var(--accent); outline-offset:2px` (regola condivisa,
  riga 834–847) — vale perché questi elementi hanno `role="button"`.
- **Esc**: non fa nulla in questa pagina (è gestito solo dai dialog).

### 6. Animazioni e transizioni
Nessuna. Nessuna regola `transition` o `animation` è dichiarata su `.tag-row`,
`.tag-icon-btn`, `.tag-list`, `.tag-cat-block`: hover e cambi di stato sono immediati. Le
uniche animazioni percepibili arrivano da fuori: l'apertura dei dialog (SP-5) e il toast
(SP-6, `opacity .2s ease, transform .2s ease`).
Ogni azione che modifica i dati chiama `renderAll()`, che **ricostruisce l'intero
`innerHTML`**: la lista non si aggiorna in modo incrementale e non c'è animazione di uscita
della riga eliminata.

### 7. Stati per ogni controllo
- `"Nuovo tag"` / `"Nuova categoria"`: normale, hover (`filter:brightness(1.05)` per il
  primario, `--chip-bg` per il ghost), focus-visible con outline accent. **Mai disabilitati.**
- Riga tag: normale, hover (chip-bg), focus-visible. Mai disabilitata.
- Cestini: normale (colore terziario), hover in rosso/tinta rossa, focus-visible. Mai
  disabilitati — **anche il cestino di un tag usato su centinaia di foto è sempre attivo**: la
  protezione è il dialog di conferma, non la disabilitazione.
- Blocco categoria vuoto: mostra la frase di stato vuoto al posto delle righe.
- Blocco "Senza categoria": **non ha** i pulsanti matita/cestino — non è una categoria vera,
  è un contenitore calcolato.
- Nessuno stato "in caricamento" e nessuno stato di errore: tutto è sincrono e locale.

### 8. Da dove ci si arriva e dove si va
**In ingresso:**
- Sidebar desktop → gruppo a scomparsa **"IA"** (icona `cpu`) → voce `"Tag e categorie"`
  (icona `tag`). Il gruppo si apre da solo se la vista corrente è una delle tre voci IA
  (`aiActive`, riga 2201).
- Mobile → tab **"Altro"** → sezione `"IA"` → riga `"Tag e categorie"` (riga 2480).
- Breadcrumb desktop: `Tag e categorie`; titolo header mobile: `Tag e categorie`.

**In uscita:**
- Le due altre voci del gruppo IA: `"Revisione"` (sezione 5) e `"Analisi libreria"` (sezione 6).
- I dialog che apre (sezioni 2 e 3) tornano qui alla chiusura, riportando il focus sul trigger.
- Nessun link diretto da un tag alle foto che lo portano: **per vedere le foto di un tag si usa
  il filtro a chip (SP-3) o la pagina Cerca — questa pagina non ci porta**. È un'assenza
  notevole, dato che ogni riga mostra "N foto".
- Cambiare vista dalla sidebar azzera i filtri rapidi della vista precedente (riga 2240).

### 9. Dati necessari a questa schermata
**Legge:** l'elenco delle categorie (id, nome) nel loro ordine; l'elenco dei tag con nome,
prompt, tinta colore, soglia, categoria di appartenenza; per ogni tag il **numero di foto su
cui è confermato**.
**Scrive:** creazione/rinomina/eliminazione di una categoria; creazione/modifica/eliminazione
di un tag (nome, prompt, colore, soglia, categoria); la riassegnazione a `null` della categoria
dei tag orfanati quando una categoria viene eliminata; l'eliminazione di **tutte le decisioni
(tag, foto)** relative a un tag eliminato (vedi la nota in sezione 8, SP-12).

---

## 53. Dialog "modifica tag"

*(`openTagEditorDialog`, righe 5392–5483; `TAG_SWATCH_HUES` riga 5388)*

### 1. Nome e scopo
Unico punto in cui si definisce un tag: come si chiama per l'utente, cosa deve cercare l'IA,
di che colore è, in quale categoria sta e sopra quale confidenza viene assegnato da solo.

### 2. Cosa mostra
Scheda modale larga **400px** (le altre modali dell'app sono 360px),
`role="dialog" aria-modal="true"`, etichettata dal proprio titolo.
- Titolo: `"Nuovo tag"` in creazione, `Modifica tag «<nome>»` in modifica.
- Campo **`"Nome"`**, placeholder `"Es. Regate"`.
- Campo **`"Prompt per il modello"`** seguito da `"(opzionale)"` in peso normale e colore
  terziario; placeholder `"Se vuoto, l'IA cerca il nome stesso"`.
- Riga di spiegazione (`.tag-field-hint`, 11.5px, terziario, interlinea 1.5, margine
  superiore negativo −8px per attaccarla al campo):
  `"Il nome è quello che vedi tu ovunque nell'app. Il prompt è cosa deve riconoscere l'IA nella foto — usalo solo per essere più preciso, es. nome «Regate», prompt «barche a vela in regata»."`
  Il commento di riga 5389 spiega il perché della scelta: *«nome (per gli umani) vs prompt (per
  il modello) sono volutamente due campi distinti e spiegati in una riga, non un solo campo che
  li confonde»*.
- Campo **`"Categoria"`**: `<select>` con `"Nessuna categoria"` come prima opzione (valore
  vuoto) e poi una opzione per categoria.
- Etichetta **`"Colore"`** e una fila di **10 pastiglie** tonde 26×26
  (`TAG_SWATCH_HUES = [24,150,205,340,270,34,195,0,120,290]`, cioè arancio, verde, azzurro,
  rosa, viola, ambra, ciano, rosso, verde acido, magenta), ciascuna colorata
  `hsl(h,60%,50%)`; quella attiva ha `border-color: var(--text)` su bordo 2px.
- **`"Soglia di confidenza — NN%"`**: l'etichetta contiene il valore corrente in accent
  (12.5px/700). Sotto, uno slider `input[type=range]` `min=30 max=95` (step implicito 1) con
  classe `.density-slider` (`accent-color: var(--accent)`, larghezza 100%).
- Nota sotto lo slider, aggiornata in tempo reale:
  `"Sopra il NN% l'IA assegna il tag in automatico. Sotto, lo propone soltanto — resta in attesa della tua conferma nella coda di revisione."`
- Riga pulsanti: primario `"Crea tag"`/`"Salva"`, ghost `"Annulla"`, e — solo in modifica —
  `"Elimina tag"` (ghost + `.btn-danger`, bordo e testo `--danger`) spinto a destra con
  `margin-left:auto`.

### 3. Ogni controllo, uno per uno
1. **`"Nome"`** (testo). Obbligatorio: al salvataggio viene fatto `.trim()`; se resta vuoto
   compare il toast `"Dai un nome al tag prima di salvarlo."`, il dialog **non** si chiude e il
   focus torna nel campo. Nessun'altra validazione: **i nomi duplicati sono permessi**, non
   c'è limite di lunghezza, non c'è normalizzazione.
2. **`"Prompt per il modello"`** (testo, opzionale). Se vuoto, la copy dice che l'IA cerca il
   nome stesso. Viene salvato con `.trim()`. Se il prompt coincide col nome, la pagina
   "Tag e categorie" non lo mostra (vedi §1.2).
3. **`"Categoria"`** (select). Valore vuoto → `categoryId = null` → il tag finisce nel blocco
   "Senza categoria". È l'**unico** modo per spostare un tag fra categorie.
4. **10 pastiglie colore** — `role="radio"`, `aria-checked`, `tabindex="0"`. Click/Invio/Spazio
   seleziona: aggiorna `draft.color` e sposta la classe `.active` e `aria-checked` su tutte.
   Nota: **non sono avvolte in un `role="radiogroup"`** e hanno tutte lo stesso
   `aria-label="Colore"` — a uno screen reader risultano dieci radio indistinguibili.
   Nota 2: **due tag del catalogo demo hanno tinte assenti dalla tavolozza** («Vicoli e centro
   storico» = 28, «Regate» = 210): aprendo il loro editor **nessuna pastiglia risulta
   selezionata**; salvando senza toccare il colore la tinta originale viene però conservata.
   Il colore proposto per un tag nuovo è `TAG_SWATCH_HUES[TAGS.length % 10]` — cambia a ogni
   tag creato, così due tag consecutivi non nascono dello stesso colore.
5. **Slider soglia** — `oninput` (continuo, non solo al rilascio): aggiorna il valore
   nell'etichetta e riscrive la frase esplicativa. Default per un tag nuovo: **75**.
6. **`"Crea tag"` / `"Salva"`** — valida il nome e poi: in creazione genera l'id `tc-<n>`
   (contatore `state.customTagSeq`), aggiunge il tag con `autoRate:0, suggestRate:0`, crea la
   sua mappa di assegnazioni vuota, toast `Tag "<nome>" creato.`; in modifica sovrascrive i
   cinque campi sull'oggetto esistente, toast `Tag "<nome>" salvato.`. Poi chiude e
   `renderAll()`.
   **Conseguenza importante:** un tag creato dall'utente nasce con tassi 0 e
   `reanalyzeLibrary()` (riga 1465) genera assegnazioni solo in base a quei tassi → nel mockup
   **un tag nuovo non riceverà mai suggerimenti o assegnazioni automatiche**; si può solo
   assegnare a mano.
7. **`"Annulla"`** — chiude senza salvare. Nessuna conferma, anche con modifiche in sospeso.
8. **`"Elimina tag"`** (solo in modifica) — chiude *prima* questo dialog, poi apre lo stesso
   dialog di conferma della pagina (testi identici, vedi §1.3 punto 4). Se l'utente annulla la
   conferma, l'editor **non** viene riaperto.

### 4. Interazioni da mouse
- Click sui controlli come sopra. Trascinamento **solo** sullo slider (comportamento nativo del
  browser: trascinare il cursore aggiorna il valore in continuo).
- **Click sullo scrim (fuori dalla scheda) NON chiude il dialog**: nessun handler è agganciato
  a `.modal-scrim` (deviazione da SP-5 e da SP-14 — qui chiude solo Esc o "Annulla").
- Nessun hover speciale sulle pastiglie (nessuna regola `:hover` su `.tag-swatch`): l'unico
  segnale è il bordo della pastiglia attiva.
- Nessun tasto destro, nessun doppio click, nessuna rotellina con effetto (la scheda non
  scrolla: a 400px di larghezza il contenuto ci sta tutto).

### 5. Interazioni da tastiera
- **Esc** chiude il dialog (handler su `document`, rimosso alla chiusura).
- All'apertura il focus va **nel campo "Nome"** (`document.getElementById('tagNameInput').focus()`).
- Alla chiusura, per qualunque via, il focus torna sull'elemento che aveva aperto il dialog
  (`trigger = document.activeElement` catturato all'apertura).
- **Nessun focus trap**: Tab può uscire dalla scheda e raggiungere gli elementi della pagina
  sottostante. Deviazione da SP-5, ricorrente in tutti i dialog di questo blocco.
- Ordine del focus: Nome → Prompt → Categoria → 10 pastiglie → slider → "Crea tag/Salva" →
  "Annulla" → "Elimina tag".
- Pastiglie e pulsanti: Invio/Spazio = click (SP-8). Slider: frecce ←/→ (comportamento nativo
  del range, step 1) — la frase esplicativa si aggiorna anche così, perché è agganciata a
  `oninput`.
- **Invio nel campo "Nome" non salva**: non c'è handler `keydown` sugli input (a differenza,
  per esempio, del dialog di input testuale generico a riga 2554). Bisogna raggiungere il
  pulsante.

### 6. Animazioni e transizioni
- La scheda compare senza animazione propria (`.modal-scrim`/`.modal-card` non hanno né
  `transition` né `animation`): appare istantanea, con l'ombra `0 20px 50px rgba(0,0,0,.3)`.
- La pastiglia attiva cambia bordo senza transizione.
- L'unico movimento è il toast di conferma/errore (SP-6).

### 7. Stati per ogni controllo
- **"Crea tag"/"Salva"**: sempre abilitato, **anche a nome vuoto**. La validazione è
  *reattiva* (toast + focus), non preventiva: non esiste uno stato disabilitato del pulsante.
  È una scelta di disegno da riportare tale e quale in Vue, o da cambiare consapevolmente.
- **Campo Nome in errore**: non ha uno stile di errore. L'errore è comunicato solo dal toast e
  dal focus che ci torna dentro; nessun bordo rosso, nessun messaggio inline.
- **Pastiglie**: normale / attiva (bordo `--text`) / focus-visible — attenzione: `role="radio"`
  **non** è nell'elenco dei selettori `:focus-visible` (riga 834–845, che copre
  `[role="button"]`, `[role="checkbox"]`, `.chip`, `.mini-switch`, `.seg-option`,
  `.album-picker-row`, input/select/textarea): **le pastiglie colore non hanno anello di focus
  visibile**. È un difetto di accessibilità reale.
- **"Elimina tag"**: presente solo in modifica, mai disabilitato.
- Nessuno stato di caricamento: salvataggio sincrono.

### 8. Da dove ci si arriva e dove si va
**In ingresso:** dalla pagina "Tag e categorie" — pulsante `"Nuovo tag"` (modalità creazione) o
click/Invio su una riga tag (modalità modifica). **Non è raggiungibile da nessun'altra parte**:
in particolare dal dettaglio foto o dal selettore di tag non si può creare un tag al volo.
**In uscita:** chiudendo si torna alla pagina, che viene ricostruita; "Elimina tag" porta al
dialog di conferma e da lì alla pagina.

### 9. Dati necessari
**Legge:** il tag da modificare (nome, prompt, colore, soglia, categoria) e l'elenco delle
categorie disponibili; in caso di eliminazione, il numero di foto su cui il tag è confermato.
**Scrive:** i cinque campi del tag, oppure un tag nuovo; l'eliminazione del tag e di tutte le
sue decisioni.
**Nota semantica importante** (commento riga 1446): la soglia è **informativa/prospettica**.
Cambiarla non rivaluta nessuna foto già decisa; nel mockup non innesca proprio nulla, dato che
i suggerimenti derivano da `autoRate`/`suggestRate` e non dalla soglia. Nel sistema reale
dovrà governare le analisi *future*, mai riscrivere decisioni esistenti.

---

## 54. Dialog "modifica categoria"

*(`openCategoryEditorDialog`, righe 5484–5520)*

### 1. Nome e scopo
Crea o rinomina un raggruppamento di tag: l'unica cosa che una categoria possiede è il nome.

### 2. Cosa mostra
Scheda modale standard (360px), `role="dialog" aria-modal="true"`.
- Titolo: `"Nuova categoria"` oppure `"Rinomina categoria"` — nota che in rinomina **il nome
  attuale non compare nel titolo** (a differenza dell'editor tag, che scrive `Modifica tag
  «X»`): piccola incoerenza di copy fra i due dialog.
- Sottotitolo `"Le categorie sono solo un raggruppamento per i tag — un livello solo, niente sotto-categorie."`
- Campo **`"Nome"`**, placeholder `"Es. Natura"`, precompilato col nome attuale in rinomina.
- Pulsanti `"Crea categoria"`/`"Salva"` (primario) e `"Annulla"` (ghost).

Non mostra: quanti tag contiene, quali tag contiene, nessun colore, nessuna icona, nessuna
opzione. Coerente con il modello dati (`{id, name}` e basta).

### 3. Ogni controllo, uno per uno
1. **`"Nome"`** (testo). Obbligatorio: se vuoto dopo `.trim()`, toast
   `"Dai un nome alla categoria prima di salvarla."`, dialog aperto, focus rimesso nel campo.
   Duplicati permessi, nessun limite di lunghezza.
2. **`"Crea categoria"` / `"Salva"`** — in creazione genera l'id `cc-<n>`
   (`state.customCatSeq`) e accoda a `TAG_CATEGORIES`; toast `Categoria "<nome>" creata.`.
   In rinomina scrive il nome sull'oggetto; toast `Categoria rinominata in "<nome>".`
   Poi chiude e `renderAll()`.
3. **`"Annulla"`** — chiude senza salvare, nessuna conferma.
Non c'è un pulsante "Elimina categoria" **dentro** questo dialog (a differenza dell'editor
tag): l'eliminazione si fa solo dal cestino sulla testata del blocco nella pagina.

### 4. Interazioni da mouse
Solo i click sui tre controlli. Scrim non cliccabile (come sopra, deviazione da SP-5). Nessun
tasto destro, doppio click, trascinamento o scroll.

### 5. Interazioni da tastiera
- **Esc** chiude. Focus iniziale sul campo "Nome"; alla chiusura il focus torna al trigger.
- Ordine: Nome → "Crea categoria/Salva" → "Annulla". Nessun focus trap.
- Invio/Spazio sui pulsanti (SP-8). **Invio nel campo non salva** (nessun handler).

### 6. Animazioni e transizioni
Nessuna, come l'editor tag. Solo il toast (SP-6).

### 7. Stati per ogni controllo
- Pulsante di salvataggio sempre abilitato, anche a campo vuoto (validazione reattiva via
  toast).
- Campo senza stile di errore.
- Nessuno stato di caricamento, nessuna disabilitazione, nessuno stato vuoto (il dialog è
  sempre completo).

### 8. Da dove ci si arriva e dove si va
**In ingresso:** solo dalla pagina "Tag e categorie" — pulsante `"Nuova categoria"` oppure
matita sulla testata di un blocco categoria. **In uscita:** ritorno alla pagina, focus sul
trigger.

### 9. Dati necessari
**Legge:** il nome della categoria da rinominare. **Scrive:** il nome della categoria o una
nuova categoria. Non tocca in alcun modo i tag né le foto.

---

## 55. Selettore di tag (assegnare tag a delle foto)

*(`openTagPickerDialog`, righe 5524–5566; CSS `.album-picker-*` righe 898–902)*

### 1. Nome e scopo
Dialog che aggiunge o toglie tag **già esistenti** da una o più foto selezionate, in blocco.

### 2. Cosa mostra
Scheda modale standard (360px), `role="dialog" aria-modal="true"`.
- Titolo `"Aggiungi tag"`.
- Sottotitolo dinamico: `"N element{o|i} selezionat{o|i} — attiva/disattiva un tag per aggiungerlo o toglierlo da tutti"`.
- Lista scorrevole (`.album-picker-list`, `max-height:260px`, `overflow-y:auto`) con **una riga
  per ogni tag esistente**, in ordine di `TAGS`, **senza raggruppamento per categoria e senza
  intestazioni** — è l'unico punto dell'app in cui i tag sono presentati come lista piatta
  (la pagina "Tag e categorie" e le chip del dettaglio foto li raggruppano entrambe).
  Ogni riga mostra: quadratino colorato 22×22 con raggio 7px (`hsl(color,60%,50%)`); il nome
  del tag (13px/600); un interruttore `.mini-switch` a destra.
- Pulsante `"Fatto"` (`.btn.btn-ghost.btn-sm`) in fondo.

**Non mostra:** un campo di ricerca (il selettore di persone ne ha uno), il conteggio delle
foto per tag, le categorie, la possibilità di creare un tag nuovo, i suggerimenti in attesa.

### 3. Ogni controllo, uno per uno
1. **Riga tag** — `role="switch"`, `aria-checked`, `aria-label="<nome del tag>"`,
   `tabindex="0"`. Interruttore a tre effetti:
   - Lo stato **acceso** è calcolato come `allIn`: **tutte** le foto passate hanno quel tag con
     `status==='confirmed'` (l'origine, IA o umana, è indifferente).
   - Click quando è spento → `addManualTag()` su **tutte** le foto → `confirmed` + `human`.
   - Click quando è acceso → `removeTagFromPhoto()` su **tutte** le foto → `rejected` + `human`.
   - **Nessuno stato intermedio/indeterminato**: se solo alcune delle foto selezionate hanno il
     tag, l'interruttore appare **spento**; il primo click lo mette a tutte. Non c'è modo, da
     qui, di sapere che la selezione era mista.
   - L'aggiornamento visivo è **ottimistico e locale** (`sw.classList.toggle('on', !allIn)`),
     senza rerender della lista.
2. **`"Fatto"`** — chiude il dialog e lancia `renderAll()`.
Non esiste un pulsante "Annulla": **le modifiche sono già applicate a ogni click** e non sono
annullabili da qui. Chiudere con Esc equivale in tutto e per tutto a "Fatto".

Il commento di riga 5521 spiega la regola semantica: *«Gli aggiunti così sono sempre
"confermati/umani" — un'aggiunta manuale non passa mai dalla coda di revisione, è già una
decisione dell'utente»*. Conseguenza pratica: accendere l'interruttore su un tag che per quella
foto era `suggested` **lo conferma e lo toglie dalla coda di Revisione**; accenderlo su un tag
`confirmed`+`ai` lo promuove a `confirmed`+`human` (sparisce il marcatore "IA" nel dettaglio
foto).

### 4. Interazioni da mouse
- Click su qualunque punto della riga (non serve colpire l'interruttore: il `role="switch"` è
  la riga intera).
- Hover riga: sfondo `var(--chip-bg)`, senza transizione.
- **Scroll con la rotellina dentro la lista** se i tag superano i 260px di altezza (nel
  catalogo demo, 9 tag ≈ 9×~38px, la lista scrolla).
- Scrim non cliccabile. Nessun tasto destro, doppio click o trascinamento.

### 5. Interazioni da tastiera
- All'apertura il focus va sulla **prima riga della lista** (non sul pulsante "Fatto").
- **Invio/Spazio** su una riga = toggle (SP-8).
- **Esc** chiude (ed è equivalente a "Fatto").
- Alla chiusura il focus torna al trigger.
- Ordine del focus: riga 1 → riga 2 → … → "Fatto". Nessun focus trap. Nessuna navigazione a
  frecce dentro la lista: non prevista nel mockup.
- Focus visibile: `.album-picker-row` è nell'elenco `:focus-visible` → outline accent 2.5px.

### 6. Animazioni e transizioni
- L'unica transizione è il **knob dell'interruttore**: `.mini-switch .knob { transition: left .15s ease }`
  — la pallina scorre da `left:2px` a `left:18px`. Il **colore di sfondo** dell'interruttore
  (`--border-strong` → `--accent`) **non è in transizione**: cambia di scatto. È il segnale
  visivo che il tag è stato applicato.
- Nessuna animazione di apertura della scheda; nessun toast per il toggle (silenzioso di
  proposito: il feedback è l'interruttore stesso).

### 7. Stati per ogni controllo
- Riga tag: spenta / accesa (`.on`) / hover / focus-visible. **Mai disabilitata.**
- Stato "misto" (solo alcune foto hanno il tag): **non rappresentato** — vedi sopra.
- Lista vuota (nessun tag esiste): il dialog mostrerebbe solo titolo, sottotitolo e "Fatto",
  **senza alcuno stato vuoto né invito a creare un tag**. Non previsto nel mockup.
- Nessuno stato di caricamento o errore.

### 8. Da dove ci si arriva e dove si va
**Due soli punti d'ingresso** (verificati su tutto il file):
1. **Dettaglio foto (lightbox)** → sezione "Tag" → chip `"+ aggiungi"` (`#lbAddTagChip`,
   riga 4218) → il dialog opera su **una sola foto**.
2. **"Modifica multipla"** (vista `bulkEdit`) → pulsante `"Aggiungi tag…"` (riga 3524, icona
   `tag` 13px) → opera su tutte le foto selezionate.
**Non** è raggiungibile dalla barra azioni "N selezionate" (SP-2) della griglia, che porta prima
a "Modifica multipla". **In uscita:** si torna sempre da dove si è venuti, con `renderAll()` e
focus sul trigger.

### 9. Dati necessari
**Legge:** l'elenco completo dei tag (id, nome, tinta) e, per ogni coppia (tag, foto
selezionata), se lo stato è "confermato". **Scrive:** per ogni foto selezionata, lo stato della
coppia (tag, foto) a `confermato/umano` oppure a `rifiutato/umano`.

---

## 56. Revisione — tag (coda di conferma; definizione canonica di SP-10)

*(`renderRevisione`, righe 5738–5807; commento d'apertura righe 5720–5725; CSS `.review-*` e
`.suggestion-*`, righe 949–961)*

> La tab **"Volti"** della stessa pagina (`renderRevisioneVolti`, righe 5811–5878) è
> documentata altrove; qui è richiamata solo per il selettore di tab che condividono e per le
> differenze rilevanti al pattern SP-10.

### 1. Nome e scopo
Coda dei tag che l'IA ha **proposto** ma non applicato — quelli a confidenza intermedia — dove
l'utente conferma o rifiuta, uno alla volta o per gruppo.

### 2. Cosa mostra
In cima, sempre, il **selettore di tab** (`revisioneTabsHTML`, riga 5726): `.seg-control` con
`role="radiogroup"` e `aria-label="Cosa revisionare"`, due opzioni:
- `"Tag"` — **senza conteggio nell'etichetta**;
- `"Volti"` seguito da ` (N)` **solo se ci sono proposte volti in attesa**.
Asimmetria da segnalare: la tab Tag non dice mai quante proposte contiene, la tab Volti sì.

Poi, se ci sono proposte:
- Titolo `"Revisione"`.
- Sottotitolo `"N proposte in attesa, raggruppate per tag — nessuna è ancora applicata"`
  (N = somma su tutti i gruppi).
- **Un gruppo per tag** che abbia almeno una proposta (i tag senza proposte non compaiono
  affatto). Ogni gruppo (`.review-group`: bordo 1px, raggio 12px, padding 14px) mostra:
  - pallino del colore del tag;
  - il nome del tag fra virgolette basse: `«Paesaggi»` (13.5px/700);
  - `"N propost{a|e}"` in 12px terziario;
  - due pulsanti di gruppo (vedi §3);
  - una striscia a capo automatico (`.suggestion-strip`, flex-wrap, gap 8px) di **miniature
    74×74**.
- Ogni miniatura (`.suggestion-thumb`) mostra:
  - l'immagine (nel mockup un gradiente) con `border-radius:8px`, **bordo tratteggiato 1.5px
    color accent** e `opacity:.92` — il tratteggio è il segno visivo di "non ancora applicato";
  - un badge `"IA"` in alto a sinistra (8.5px/700, sfondo `--accent-tint-strong`, testo accent,
    raggio 4px);
  - un overlay di azioni nascosto (vedi §6).
Non mostra: nome file, cartella, data, confidenza numerica di quella specifica proposta, né
alcun modo di aprire la foto a schermo intero da qui.

**Stato vuoto** (nessuna proposta tag): il selettore di tab resta, seguito da `emptyStateHTML`
con icona `inbox` 34px (opacità .5), titolo `"Nessun suggerimento in attesa"` e testo
`"Quando l'IA troverà corrispondenze a confidenza intermedia per i tuoi tag, appariranno qui per la tua conferma — mai applicate da sole."`

### 3. Ogni controllo, uno per uno
1. **Tab `"Tag"`** — `role="radio"`, tabindex roving (0 se attiva, −1 altrimenti). Imposta
   `state.revisioneTab='tag'`.
2. **Tab `"Volti"`** — idem, porta all'altra coda.
3. **`"Conferma tutte"`** (per gruppo) — `.btn.btn-sm` con icona `check` 13px. Conferma **tutte
   le proposte di quel tag** in un colpo; toast `N proposte confermate.`
4. **`"Rifiuta tutte"`** (per gruppo) — `.btn.btn-sm.btn-ghost`, nessuna icona. Rifiuta tutte
   le proposte di quel tag; toast `N proposte rifiutate.`
   **Nessuna delle due azioni di massa chiede conferma**, benché "Rifiuta tutte" sia
   irreversibile (vedi §9). È una deviazione consapevole rispetto alle eliminazioni della
   pagina "Tag e categorie", che invece passano sempre da `openConfirmDialog`.
5. **Spunta su una miniatura** — `.mini-btn.confirm`, `role="button"`, `aria-label="Conferma"`,
   icona `check` 13px. Conferma quella singola proposta; toast `"Tag confermato."`
6. **Croce su una miniatura** — `.mini-btn.reject`, `aria-label="Rifiuta"`, icona `close` 13px.
   Rifiuta quella proposta; toast `"Suggerimento rifiutato — non verrà riproposto."`
7. **La miniatura in sé non è un controllo**: non ha `role`, né `tabindex`, né handler. Cliccare
   sull'immagine non apre la foto e non fa nulla.

**Assenti, da segnalare:** non esiste un `"Conferma tutto"` **globale** su tutti i gruppi
(solo per gruppo); non esiste "Annulla"/"Ripristina" dopo un'azione; non esiste un modo di
vedere la confidenza; non esiste paginazione o "mostra altre N" (tutti i suggerimenti di un
tag sono renderizzati insieme); non esiste un filtro o un ordinamento della coda.

### 4. Interazioni da mouse
- **Hover su una miniatura** → compare l'overlay con i due pulsanti (vedi §6). Non c'è ritardo:
  la transizione parte subito.
- **Click** su spunta/croce → azione immediata, toast, e **`renderAll()` che ricostruisce
  tutta la pagina**: la miniatura sparisce dalla striscia e le successive si riflowano.
- Quando l'ultima proposta di un gruppo viene decisa, **il gruppo intero scompare**; quando
  spariscono tutti i gruppi, la pagina si sostituisce con lo stato vuoto e il badge rosso
  accanto a "Revisione" nella sidebar sparisce (è renderizzato solo se il conteggio è > 0,
  riga 2223).
- Nessun tasto destro, nessun doppio click, nessun trascinamento (non si trascinano miniature
  fra gruppi), nessuna rotellina con effetto oltre lo scroll di pagina. Nessuna selezione
  multipla di miniature: **non prevista nel mockup**.

### 5. Interazioni da tastiera
- **Nessuna scorciatoia**: verificato sull'unico handler globale `keydown` (riga 6289), che
  gestisce solo lightbox, culling, pannelli a comparsa ed Esc. Nella pagina Revisione **non
  esiste** un tasto "conferma"/"rifiuta"/"avanti" — nessun Y/N, nessuna freccia.
- L'unica via da tastiera è **Tab** fino alla spunta o alla croce e poi **Invio/Spazio** (SP-8).
  Gli overlay diventano visibili anche via tastiera grazie a
  `.suggestion-thumb:focus-within .suggestion-hover{opacity:1}` — la scelta è corretta e va
  mantenuta in Vue.
- **Problema noto e importante:** ogni azione chiama `renderAll()`, che ricostruisce il DOM;
  il focus finisce sul `body`. Rivedere una coda di 40 proposte da tastiera richiede di
  ri-tabulare dall'inizio dopo ogni conferma. In Vue va previsto lo spostamento del focus alla
  proposta successiva.
- Tab sul selettore: le due tab sono in roving tabindex, quindi il gruppo si attraversa con un
  solo Tab — ma **non è implementata la navigazione a frecce fra le due tab**, che il pattern
  radiogroup richiederebbe.
- **Esc**: nessun effetto in questa pagina.

### 6. Animazioni e transizioni
- **Overlay delle azioni**: `.suggestion-hover` è `position:absolute; inset:0;
  background:rgba(0,0,0,.5); opacity:0; transition: opacity .12s ease`, portato a `opacity:1`
  su `:hover` **o** `:focus-within` del contenitore. Comunica: "questa miniatura è ora
  azionabile", senza occupare spazio permanente in una griglia densa.
- I due pulsanti dentro l'overlay sono cerchi bianchi 26×26; la spunta è nera (`color:#111`,
  ereditato da `.mini-btn`), la croce è `var(--danger)`. **La spunta non ha un colore
  "positivo" proprio** (non esiste una regola `.mini-btn.confirm`): l'asimmetria cromatica è
  solo sul rifiuto. Nella tab Volti c'è un terzo pulsante `.notface` a fondo `--danger` pieno.
- **Nessuna animazione di uscita** della miniatura confermata/rifiutata: sparisce di colpo col
  rerender. Nessuna animazione dello stato vuoto quando la coda si svuota.
- Il toast (SP-6) è l'unico movimento residuo: `opacity .2s ease, transform .2s ease`,
  visibile per 2400 ms.

### 7. Stati per ogni controllo
- **Tab**: attiva (`.seg-option.active`: sfondo `--card-bg`, testo pieno, peso 600, ombra
  `--shadow`) / inattiva (testo secondario, sfondo trasparente sul contenitore `--chip-bg`) /
  focus-visible (outline accent). Mai disabilitate — **la tab "Volti" resta cliccabile anche
  con il riconoscimento volti spento**, e in quel caso mostra uno stato vuoto dedicato che
  rimanda a Impostazioni.
- **"Conferma tutte" / "Rifiuta tutte"**: normale, hover (`--chip-bg`), focus-visible. Mai
  disabilitati: un gruppo esiste solo se ha almeno una proposta, quindi non possono mai essere
  "vuoti".
- **Spunta/croce**: esistono sempre, ma sono **invisibili finché non c'è hover o focus** —
  quindi il loro "stato normale" è di fatto opacità 0. Da tastiera diventano visibili solo
  quando ricevono il focus.
- **Stato vuoto**: descritto in §2. **Nessuno stato di caricamento** e nessuno stato di errore:
  tutto è sincrono.

### 8. Da dove ci si arriva e dove si va
**In ingresso:**
- Sidebar desktop → gruppo **"IA"** → voce `"Revisione"` (icona `inbox`), con **badge rosso**
  (`.nav-badge`, sfondo `--danger`, testo bianco 10.5px/700) che riporta
  `pendingSuggestionCount()`, mostrato solo se > 0.
- Mobile → tab "Altro" → sezione "IA" → riga `"Revisione"` con lo stesso badge.
- Dalla pagina Persone esiste un banner che porta qui **forzando la tab "Volti"**
  (riga 2654: `state.revisioneTab='volti'`).
- Non ci si arriva dal dettaglio foto: lì i suggerimenti si confermano in loco (sezione 8).

**In uscita:** l'altra tab; la sidebar per qualunque altra vista. Nessuna azione di questa
pagina porta altrove, e non c'è alcun collegamento dalla proposta alla foto in dettaglio.

**Il conteggio del badge unisce due code** (riga 1483–1485): tag in attesa + volti in attesa,
questi ultimi **solo se il riconoscimento volti è attivo**. Il commento lo motiva: *«il badge
su "Revisione" segnala tutto ciò che aspetta una conferma umana in quella pagina, non solo i
tag (ora che ha anche la tab Volti)»*. Conseguenza: il badge può dire 12 mentre la tab Tag ne
ha 5.

### 9. Dati necessari a questa schermata
**Legge:** l'elenco delle coppie (tag, foto) in stato "proposto", raggruppate per tag; per ogni
tag il nome e la tinta; per ogni foto la miniatura. Più il conteggio delle proposte volti in
attesa, per l'etichetta della tab e per il badge.
**Scrive:** per ogni proposta decisa, lo stato della coppia (tag, foto) a `confermato/umano`
oppure a `rifiutato/umano`. **Il rifiuto è permanente e definitivo**: `reanalyzeLibrary()` non
riesamina mai una coppia che ha già un valore (riga 1470), quindi una proposta rifiutata non
tornerà mai in coda. Non c'è annullamento: l'unico modo di tornare indietro su un rifiuto è
riaggiungere il tag a mano dal selettore di tag o dal dettaglio foto.

### SP-10 — definizione canonica: coda di conferma IA
Forma condivisa da entrambe le tab di Revisione (e richiamata, in forma ridotta, dal dettaglio
foto):
1. **Raggruppamento per soggetto della proposta** — per tag qui, per persona nella tab Volti.
   Ogni gruppo ha testata con identificativo (pallino + `«nome»` / `Questi volti sembrano
   <b>Nome</b>`) e conteggio `"N propost{a|e}"`.
2. **Nulla è applicato finché un umano non decide** — ribadito tre volte nella copy:
   nel sottotitolo (`"nessuna è ancora applicata"`), nello stato vuoto (`"mai applicate da
   sole"`) e nella nota della soglia nell'editor tag.
3. **Marcatura visiva uniforme della proposta**: miniatura con **bordo tratteggiato accent**,
   `opacity:.92`, e badge `"IA"` in alto a sinistra.
4. **Azioni per singolo elemento in un overlay su hover/focus** (`opacity .12s ease`): sempre
   `Conferma` (spunta) e `Rifiuta` (croce, rossa); la variante Volti aggiunge un terzo pulsante
   `"Non è un volto"` (cestino su fondo rosso pieno) per i falsi positivi, e la miniatura
   diventa `.suggestion-thumb.triple` (86px invece di 74px, pulsanti 24px invece di 26px, gap
   3px invece di 6px) per far stare tre pulsanti.
5. **Azioni per gruppo**: `"Conferma tutte"` (con icona check) e `"Rifiuta tutte"` (ghost),
   senza dialog di conferma, con toast `N proposte confermate.` / `N proposte rifiutate.`
6. **Feedback**: toast breve per ogni azione, poi rerender completo. Il gruppo scompare quando
   si svuota; la pagina passa allo stato vuoto quando spariscono tutti i gruppi; il badge in
   sidebar si aggiorna in automatico.
7. **Il rifiuto è permanente**: nessuna riproposta futura, in nessuna delle due code.
8. **Nessuna scorciatoia da tastiera** in nessuna delle due code, e nessun ripristino del focus
   dopo l'azione. Se il pattern va migliorato, va migliorato in un solo posto.

---

## 57. Analisi libreria

*(`renderAnalisiLibreria` righe 5899–5947, `ensureAnalysisTicker` 5888–5897, `analysisIsPaused`
5887, `etaLabel` 5948; commento d'apertura 5880–5885; CSS `.analysis-*` righe 988–999)*

### 1. Nome e scopo
Pannello di stato del lavoro di analisi in background che calcola, una volta per foto, il
vettore usato sia per abbinare i tag sia per la ricerca per descrizione libera.

### 2. Cosa mostra
- Titolo `"Analisi libreria"`.
- Sottotitolo `"Riconoscimento tag su tutte le foto — gira in background, non blocca l'uso dell'app"`.
- **Scheda di stato** (`.analysis-card`: bordo 1px, raggio 12px, padding 16/18) contenente:
  - **Badge di stato**, uno dei tre:
    - `"Completata"` con icona `check` 12px — variante `.running` (sfondo `--accent-tint`,
      testo accent);
    - `"In pausa"` con icona `info` 12px — variante `.paused` (sfondo `--chip-bg`, testo
      secondario): **neutra di proposito**;
    - `"In corso"` preceduto da un **pallino pulsante** 7px in accent — variante `.running`.
  - **Frase esplicativa** accanto al badge (`.analysis-status-note`, 12px terziario), diversa
    per stato:
    - completata → `"Tutta la libreria è stata analizzata. Le nuove foto importate entrano automaticamente in coda."`
    - in pausa → `"Stai usando l'app — riprende da sola pochi secondi dopo l'ultima azione, senza bisogno di far nulla."`
    - in corso → `"Nessuna attività recente: sta usando la macchina a piena capacità."`
  - **Barra di avanzamento** (`.analysis-progress-bar`, alta 8px, raggio 5, traccia
    `--border-strong`; riempimento in accent).
  - **Riga di misure** sotto la barra, giustificata agli estremi:
    - a sinistra: `<b>128.450</b> di 214.000 foto (60%)` — numeri formattati `it-IT` (punto
      come separatore delle migliaia), percentuale arrotondata all'intero e limitata a 100;
    - a destra: `"stima: NN min rimanenti a questa velocità"` se in corso, la parola
      `"in pausa"` se in pausa, **niente** se completata.
      La stima viene da `etaLabel()`: minuti = `foto rimanenti × ms per foto / 1000 / 60`
      arrotondati; sotto i 60 minuti si stampa `"NN min"`, sopra `"N.N h"` (un decimale).
      I ms per foto dipendono dal livello IA: **42 ms** in "Piena", **260 ms** in "Ridotta"
      (riga 1526).
  - **Nota fissa**: `"Di notte (2:00–7:00) l'analisi lavora a piena velocità su tutti i core disponibili, ignorando le pause di inattività."` — è **solo copy**: nessuno scheduler notturno
    è implementato nel mockup.
  - **Pulsante `"Continua ora"`** con icona `activity` 13px — presente **solo** quando è in
    pausa e non completata.
- **Sezione aggiuntiva solo in modalità "Ridotta"**: titolo `Modalità "Ridotta"`, sottotitolo
  `"Niente analisi automatica di massa in background su questa macchina — puoi comunque analizzare a mano quello che serve."`, pulsante
  `Analizza ora <nome cartella>`.
- **Sezione `"Cosa fa"`** (sempre presente quando l'IA non è spenta):
  `"Per ogni foto calcola una volta un vettore che serve sia per abbinare i tag che crei in \"Tag e categorie\", sia per la ricerca per descrizione libera in \"Cerca\". Non serve rifarlo per ogni ricerca."`

**In modalità "Spenta"** la pagina mostra solo titolo, sottotitolo
`"Riconoscimento tag e ricerca semantica su tutte le foto"` (diverso da quello normale!) e uno
stato vuoto con icona `cpu`, titolo `"Analisi automatica disattivata"` e testo
`"Questo server è in modalità \"Spenta\" (Impostazioni → Intelligenza artificiale) — hardware o database non adatti. Tag manuali e ricerca per nome, cartella o data restano disponibili."`

**Non mostra:** l'elenco delle cartelle in coda, quale foto sta analizzando ora, il numero di
core o thread usati, l'uso di CPU/RAM/GPU, i tempi trascorsi, un log, gli errori. Nessun
indicatore di velocità istantanea: la "velocità misurata" (42/260 ms per foto) vive solo in
Impostazioni.

### 3. Ogni controllo, uno per uno
La pagina ha **al massimo due pulsanti**, ed entrambi possono essere assenti:
1. **`"Continua ora"`** — visibile solo se `in pausa && non completata`. Imposta
   `state.forceRunningOnce = true` e ridisegna: il badge passa subito a "In corso" e la barra
   riprende ad avanzare, **ignorando la regola di pausa fino al prossimo cambio di vista**.
2. **`Analizza ora <cartella>`** — solo in modalità "Ridotta". Fa avanzare l'avanzamento di
   **640 foto** in un colpo (contro le 55/secondo del ticker) e mostra il toast
   `Analisi manuale completata per <cartella>.`
   La cartella è `state.currentFolder`, **con ripiego sulla prima cartella dell'elenco** se
   nessuna è aperta (`FOLDERS.find(...) || FOLDERS[0]`, riga 5932): arrivando qui dalla
   timeline combinata "Foto" (dove `currentFolder` è `null`) il pulsante nomina una cartella
   che l'utente non ha scelto.

**Assenti — da dichiarare esplicitamente:** non c'è un pulsante **Pausa**, non c'è **Riprendi**
(esiste solo "Continua ora", che è un'eccezione una tantum), non c'è **Ferma**, non c'è
**Riavvia/Rianalizza tutto**, non c'è una scelta della priorità o del numero di thread, non c'è
un modo di escludere cartelle, non c'è impostazione della finestra notturna (che è solo
descritta a parole).

### 4. Interazioni da mouse
- Click sui due pulsanti, come sopra. Hover standard `.btn:hover{background:var(--chip-bg)}`.
- **Non c'è hover sulla barra di avanzamento** (nessun tooltip con i numeri esatti: i numeri
  sono già scritti sotto).
- Nessun tasto destro, doppio click, trascinamento, rotellina con effetto.
- **Attenzione:** muovere il mouse o scorrere la pagina **non** rimette in pausa l'analisi. La
  pausa dipende solo dal cambio di vista (vedi sotto).

### 5. Interazioni da tastiera
- Nessuna scorciatoia dedicata. I due pulsanti sono `role="button"` + `tabindex="0"` e si
  attivano con Invio/Spazio (SP-8), con outline accent al focus.
- **Difetto rilevante:** il ticker richiama `renderAnalisiLibreria(root, true)` **ogni secondo**
  e riscrive tutto l'`innerHTML` — anche quando l'analisi è in pausa. Il focus sul pulsante
  `"Continua ora"` viene quindi **distrutto una volta al secondo**: raggiungerlo e premerlo con
  la sola tastiera è, di fatto, una gara contro il timer. In Vue va risolto con un aggiornamento
  reattivo dei soli valori, non con la ricostruzione del nodo.
- Esc: nessun effetto.

### 6. Animazioni e transizioni
- **Pallino pulsante** (`.analysis-pulse`, 7px tondo, accent):
  `animation: analysisPulse 1.4s ease-in-out infinite`, keyframe
  `0%,100% {opacity:1} 50% {opacity:.35}` (riga 995). **Cosa comunica:** che il lavoro è vivo e
  in corso *adesso* — è l'unico elemento animato in continuo dell'app. Sparisce nello stato in
  pausa (dove al suo posto c'è l'icona `info` statica) e in quello completato (icona `check`).
  Nessuna regola `prefers-reduced-motion` esiste nel file: l'animazione non si ferma mai per
  chi la disattiva a livello di sistema — da correggere nell'implementazione reale.
- **Riempimento della barra**: `.analysis-progress-fill { transition: width .3s ease }`. Il
  ticker aggiorna il valore ogni secondo, quindi la barra **scivola** con uno scatto ogni
  secondo invece di saltare — dà l'impressione di un flusso continuo.
- **Cambio di stato del badge**: nessuna transizione, il badge si sostituisce di colpo (colori
  `--accent-tint` ↔ `--chip-bg`).
- Toast per l'analisi manuale (SP-6).

### 7. Stati per ogni controllo
- **Badge**: tre stati mutuamente esclusivi (completata / in pausa / in corso), calcolati in
  quest'ordine di priorità: `done` (≥100%) vince su `paused`, che vince su "in corso".
- **`"Continua ora"`**: **non disabilitato, ma assente** quando non serve (in corso, o
  completata). Questa è la scelta del mockup: nasconde invece di disabilitare.
- **`Analizza ora <cartella>`**: presente solo in "Ridotta", mai disabilitato — è premibile
  anche quando l'avanzamento è già al 100% (l'incremento verrebbe semplicemente saturato da
  `Math.min`, ma il toast direbbe comunque "Analisi manuale completata").
- **Pagina in "Spenta"**: nessun controllo, solo lo stato vuoto; il ticker viene fermato e
  azzerato al primo tick successivo.
- Nessuno stato di errore: un'analisi che fallisce non è rappresentata da nessuna parte.

### 8. Da dove ci si arriva e dove si va
**In ingresso:** sidebar desktop → gruppo "IA" → `"Analisi libreria"` (icona `activity`);
mobile → "Altro" → sezione "IA" → stessa riga. Nessun altro punto d'ingresso: **non c'è
collegamento da Impostazioni → Intelligenza artificiale a questa pagina**, benché lo stato
vuoto rimandi a parole a "Impostazioni → Intelligenza artificiale".
**In uscita:** qualunque voce di navigazione. Uscire dalla vista ferma il ticker al tick
successivo (`clearInterval`, riga 5891) e — cosa più importante — **riavvia il conto della
pausa** (vedi sotto).

### 9. Dati necessari a questa schermata
**Legge:** quante foto sono state analizzate e quante sono in totale; se l'analisi sta girando
o è in pausa; la velocità stimata di analisi per foto su questa macchina; il livello IA
configurato; il nome della cartella corrente (per il pulsante manuale in "Ridotta").
**Scrive:** solo il proprio avanzamento (e la richiesta "riprendi subito"). Non tocca foto, tag
o decisioni: **è una pagina di sola osservazione più due comandi**. Nel sistema reale l'unica
scrittura di sostanza è il vettore per foto, che questa pagina non mostra mai.

### La pausa automatica durante la navigazione — regola completa
È il comportamento meno ovvio dell'intera pagina, e il commento a riga 5882 ne dà il perché:
*«La pausa è automatica (in base alla navigazione reale nell'app, vedi renderAll) e deve
leggersi come comportamento previsto, mai come "bloccato": nessuna icona d'errore, nessun
colore d'allarme, solo un badge neutro e una frase che spiega perché.»*

Meccanica esatta:
1. In `renderAll()` (righe 3053–3058) c'è una variabile `_lastRenderedView`. **Solo quando
   `state.view` cambia davvero** — non a ogni render — vengono eseguiti
   `state.lastNavAt = Date.now()` e `state.forceRunningOnce = false`. Il commento è esplicito:
   *«traccia i cambi di VISTA (non ogni render)»*. Cliccare, filtrare, aprire un dialog o
   scorrere **non** rimettono in pausa nulla; cambiare sezione sì.
2. `analysisIsPaused()` (riga 5887) restituisce vero se
   `!state.forceRunningOnce && (Date.now() - state.lastNavAt) < 4000`.
   **La soglia esatta è 4000 ms — 4 secondi dall'ultimo cambio di vista.**
3. Il ticker gira **ogni 1000 ms** e incrementa l'avanzamento di **55 foto** solo se non è in
   pausa. Il rerender invece avviene a ogni tick, in pausa o no.
4. Conseguenza visibile: **appena si apre "Analisi libreria" il pannello è "In pausa" per circa
   4 secondi**, poi passa da solo a "In corso" con il pallino pulsante — senza che l'utente
   faccia nulla. È esattamente l'effetto voluto ("riprende da sola pochi secondi dopo l'ultima
   azione"), ma va capito che nel mockup l'apertura della pagina stessa conta come navigazione.
5. `"Continua ora"` alza `forceRunningOnce`, che scavalca la finestra dei 4 secondi. La bandiera
   viene abbassata **al primo cambio di vista successivo**: "una tantum", come dice il nome.
6. **Incoerenza da segnalare:** il ticker si ferma solo per `aiTier==='spento'`. In modalità
   **"Ridotta"** — dove la copy afferma che *non* c'è analisi automatica di massa in background —
   la barra continua comunque ad avanzare di 55 foto al secondo da sola. O il ticker deve
   fermarsi anche in "Ridotta", o la copy va cambiata.

---

## 58. I livelli IA "Pieno" / "Ridotto" / "Spento" — definizione canonica di SP-11

*(`state.aiTier` riga 2144; `AI_TIER_COPY` righe 1521–1525; `aiMsPerPhoto()` riga 1526;
`AI_MODEL_NAME` riga 1520; interfaccia in Impostazioni righe 6163–6176 e 6201–6203)*

### Cos'è
Un unico interruttore a tre posizioni che descrive **di cosa è capace questo server**, non cosa
l'utente preferisce. La copy della sezione lo dice a chiare lettere:
`"Misurata automaticamente su questo server — le tre modalità sono tutte configurazioni valide, non errori"`. Non è un errore da risolvere: è una condizione da comunicare bene.

### Dove si cambia
**Impostazioni → sezione `"Intelligenza artificiale"`**. Un `.seg-control` con
`role="radiogroup"` e `aria-label="Modalità intelligenza artificiale"`, tre `role="radio"` in
roving tabindex, etichettati:

| Etichetta a schermo | Valore in `state.aiTier` |
|---|---|
| `"Piena"` | `pieno` |
| `"Ridotta"` | `ridotto` |
| `"Spenta"` | `spento` |

(Le etichette sono al femminile perché concordano con "modalità"; i valori interni sono al
maschile. Va tenuto presente nel passaggio a Vue: non sono la stessa stringa.)

Il cambio è **immediato** (`state.aiTier = ...; renderAll()`): nessuna conferma, nessun toast,
nessun riavvio, nessun avviso che si stanno per nascondere delle funzioni. Sotto il selettore,
in un riquadro `.ai-tier-note` (12.5px, sfondo `--chip-bg`, raggio 9px, padding 10/12,
interlinea 1.5), compare la spiegazione del livello scelto:

- **Piena** — `"Hardware e database (serve pgvector o equivalente) adatti all'analisi automatica completa: tag, ricerca semantica e analisi in background su tutta la libreria."`
- **Ridotta** — `"Hardware sufficiente per tag manuali, ricerca su richiesta e analisi di singole foto o cartelle a mano — ma non per l'analisi automatica di massa in background su tutta la libreria."`
- **Spenta** — `"Questo server non ha hardware o database adatti all'IA. Tag manuali e ricerca per nome file, cartella o data restano disponibili; tag automatici e ricerca per descrizione sono disattivati."`

Sotto, **solo se il livello non è "Spenta"**, due righe informative che spariscono in "Spenta":
- `"Modello in uso"` → `"CLIP ViT-B/32 (locale, via ONNX Runtime)"`. Il commento a riga 1519
  spiega la scelta: *«modello locale, self-hosted come il resto di Keeppix — mai una chiamata a
  un servizio esterno»*. È un'informazione di fiducia, non un dettaglio tecnico: va mantenuta.
- `"Velocità misurata su questa macchina"` → `"42 ms per foto"` (Piena) o `"260 ms per foto"`
  (Ridotta).

### Cosa cambia concretamente nell'interfaccia — stato di fatto del mockup
`state.aiTier` viene **letto in quattro soli punti** in tutto il file (verificato con ricerca
esaustiva): riga 1526 (`aiMsPerPhoto`), riga 5891 (ticker), riga 5901 e 5929
(`renderAnalisiLibreria`), più la sezione di Impostazioni che lo scrive. Quindi:

**Modalità "Piena" (`pieno`) — tutto attivo**
- "Analisi libreria" mostra la scheda completa (badge, barra, misure, stima, "Continua ora").
- Il ticker avanza di 55 foto/s quando non è in pausa.
- Stima calcolata a 42 ms per foto.
- Nessuna sezione aggiuntiva; nessun pulsante di analisi manuale.
- Impostazioni mostra modello e velocità.

**Modalità "Ridotta" (`ridotto`) — l'analisi di massa dovrebbe sparire, la manuale compare**
- In "Analisi libreria" **compare in più** un blocco `Modalità "Ridotta"` con il pulsante
  `Analizza ora <cartella>` (+640 foto per pressione, con toast).
- La stima passa a 260 ms per foto: la stessa coda residua viene dichiarata ~6 volte più lunga
  (e può passare da minuti a ore, cambiando anche il formato dell'etichetta).
- Impostazioni riporta 260 ms.
- **Ma la scheda di avanzamento resta identica e il ticker continua a girare** — vedi
  l'incoerenza segnalata in §6. Nell'implementazione reale, in "Ridotta" dovrebbero sparire:
  il badge "In corso"/"In pausa", il pallino pulsante, la stima, il pulsante "Continua ora" e
  l'avanzamento automatico; dovrebbe restare la barra come storico più l'analisi manuale.

**Modalità "Spenta" (`spento`) — l'unico livello che nasconde davvero qualcosa**
- "Analisi libreria" perde **tutto**: niente scheda, niente barra, niente badge, niente
  pulsanti, niente sezione "Cosa fa". Resta solo lo stato vuoto con icona `cpu`.
- Il ticker si ferma e viene azzerato al primo tick successivo: l'avanzamento si congela.
- Impostazioni nasconde le righe "Modello in uso" e "Velocità misurata su questa macchina".
- **Resta invece attivo tutto il resto**, perché nessun'altra parte del codice controlla il
  livello.

### Cosa la copy promette ma il mockup non fa (elenco operativo per il frontend Vue)
Questa è la lacuna più grossa di tutto il blocco, e va scritta chiaramente perché è lavoro da
fare, non un dettaglio. In "Spenta", nel mockup, **continuano a funzionare come se nulla
fosse**:
- la pagina **"Revisione"**, che resta piena di proposte dell'IA da confermare;
- il **badge rosso** su "Revisione" nella sidebar e in "Altro";
- l'intera voce di navigazione **"Analisi libreria"** (visibile, benché la pagina sia vuota);
- i **marcatori "IA"** e le chip tratteggiate nel dettaglio foto (sezione 8);
- la **ricerca per descrizione libera** in "Cerca" (`sceneKeywordMatch`, riga 4589), che la
  copy dichiara esplicitamente disattivata;
- il **badge soglia** e la nota "Sopra il NN% l'IA assegna il tag in automatico" nell'editor
  tag, che descrivono un comportamento impossibile su quel server;
- la **soglia di confidenza** stessa, che resta un controllo pienamente attivo.

Regola da applicare nell'implementazione reale, coerente con la copy dei tre livelli:

| Funzione | Piena | Ridotta | Spenta |
|---|---|---|---|
| Creare/modificare tag e categorie | sì | sì | sì |
| Assegnare tag a mano (selettore, dettaglio foto, modifica multipla) | sì | sì | sì |
| Filtrare per tag/categoria | sì | sì | sì |
| Ricerca per nome file / cartella / data | sì | sì | sì |
| Ricerca per descrizione libera (semantica) | sì | sì, **su richiesta** | **no** |
| Assegnazione automatica dei tag sopra soglia | sì | solo su analisi manuale | **no** |
| Coda "Revisione" e badge relativo | sì | solo da analisi manuale | **no** (nascondere voce e badge) |
| Analisi in background di tutta la libreria | sì | **no** | **no** |
| Analisi manuale di una cartella | non serve | sì | **no** |
| Campo "Soglia di confidenza" nell'editor tag | sì | sì | da nascondere o spiegare come inattivo |

### Interazioni, stati e animazioni del selettore
- **Mouse:** click su una delle tre opzioni. Hover: nessuna regola dedicata a `.seg-option`
  (cambia solo lo stato attivo). Nessun tasto destro, doppio click o trascinamento.
- **Tastiera:** Invio/Spazio (SP-8); roving tabindex, quindi il gruppo si attraversa con un
  solo Tab. **Le frecce non spostano la selezione**: non implementate, qui come negli altri
  `.seg-control` (Revisione, Modifica multipla).
- **Focus visibile:** `.seg-option:focus-visible` è nell'elenco → outline accent 2.5px.
- **Stati:** attiva (`.seg-option.active`: sfondo `--card-bg`, testo pieno, peso 600, ombra
  `--shadow`) / inattiva (testo secondario) / focus. **Mai disabilitate:** l'utente può sempre
  scegliere un livello superiore a quello che l'hardware regge — nel mockup non c'è alcuna
  verifica delle capacità reali, benché la copy dica "misurata automaticamente".
- **Animazioni:** nessuna. Il cambio di modalità ridisegna la pagina di colpo.

---

## 59. Provenienza IA vs utente — definizione canonica di SP-12

*(modello dati righe 1427–1497; resa visiva `lbTagSectionHTML` righe 4233–4271; CSS
`.lb-tag-chip` e `.lb-suggested-tag-chip` righe 1001–1016)*

### Il modello: due assi, non uno
Per **ogni coppia (tag, foto)** esiste al massimo un record:

```
{ status: 'confirmed' | 'suggested' | 'rejected',
  origin: 'ai' | 'human' }
```

L'archivio è `TAG_ASSIGNMENTS[tagId]`, una mappa `photoId → record` (riga 1463). **L'assenza di
un record significa "mai valutata"**, che è diverso da "rifiutata".

Le combinazioni realmente prodotte sono quattro:

| status + origin | Significato | Dove si vede |
|---|---|---|
| `confirmed` + `ai` | l'IA l'ha assegnato da sola perché sopra la soglia alta, **ma nessun umano l'ha ancora guardato** | chip attenuata con marcatore "IA" nel dettaglio foto |
| `confirmed` + `human` | decisione dell'utente: aggiunto a mano, oppure suggerimento confermato, oppure chip "IA" confermata | chip piena nel dettaglio foto |
| `suggested` + `ai` | confidenza intermedia: proposto, non applicato | pagina Revisione + sezione "In attesa di conferma" nel dettaglio foto |
| `rejected` + `human` | no permanente: suggerimento rifiutato **oppure** tag rimosso a mano | non si vede da nessuna parte — è un'assenza |

`suggested`+`human` e `rejected`+`ai` non vengono mai generate.

Il commento di riga 1427 spiega il *perché* di questa separazione, ed è la frase più importante
di tutto il modello: *«'confirmed'+'ai' = l'IA l'ha assegnato in automatico (sopra soglia alta)
ma nessun umano l'ha ancora guardato — va reso comunque visivamente più "leggero" di
'confirmed'+'human'»*, e *«'rejected' è una decisione umana permanente: reanalyzeLibrary() non
tocca MAI una coppia che ha già un valore, qualunque esso sia — è così che "una decisione umana
non viene mai sovrascritta da rianalisi successive"»*.

Il corollario è a riga 1495: **rimuovere un tag da una foto non cancella il record, lo scrive a
`rejected`** — *«altrimenti una rianalisi potrebbe far ricomparire un tag che l'utente aveva
tolto apposta»*. Un "no" dell'utente è un dato, non un vuoto.

### Come un tag suggerito è visivamente distinto da uno messo dall'utente
Tutto avviene nella sezione "Tag" del dettaglio foto (lightbox, tema scuro con colori
codificati a mano invece che con le variabili di tema). Tre livelli, tre trattamenti:

**1. Confermato da un umano — chip piena.**
`.lb-tag-chip`: sfondo `#161616`, bordo `1px solid #232323`, testo `#d8d8d8`, 11.5px, pillola
raggio 14px, con un pallino 8×8 del colore del tag e una "x" a destra. Nessun marcatore, piena
opacità. Le chip sono **raggruppate per categoria** (`.lb-tag-cat-group`, con il nome della
categoria in 10px `#6b6b6e` sopra ogni gruppo, e "Senza categoria" per i tag orfani), nello
stesso ordine della pagina "Tag e categorie".

**2. Assegnato dall'IA e mai guardato — stessa chip ma attenuata, con marcatore "IA".**
`.lb-tag-chip.ai-applied { opacity:.72 }` — è l'intera chip a schiarirsi. Dentro, un piccolo
marcatore testuale `"IA"` (9px, peso 700, `opacity:.8`) con tooltip nativo
`title="Assegnato in automatico dall'IA — clicca per confermarlo"`. **La chip stessa diventa
cliccabile**: `role="button"`, `tabindex="0"`,
`aria-label="Conferma tag <nome>, assegnato in automatico dall'IA"`.

**3. Proposto e in attesa — chip tratteggiata, in una sezione separata.**
Sotto le chip confermate compare l'etichetta `"In attesa di conferma"` e, sotto, le
`.lb-suggested-tag-chip`: stesso fondo `#161616` ma **bordo tratteggiato** `1px dashed #3a3a3a`
e testo più spento `#b8b8bc`. Ognuna ha due pulsantini tondi da 17px (`background:#232323`,
hover `#2c2c2c`): conferma in verde `#6fd08a` (icona `check` 10px, `aria-label="Conferma <nome>"`)
e rifiuto in rosso chiaro `#ff8a80` (icona `close` 10px, `aria-label="Rifiuta <nome>"`).

Nella pagina Revisione lo stesso concetto è reso sulla miniatura anziché sulla chip: **bordo
tratteggiato accent 1.5px + badge "IA"**. Il tratteggio è quindi il vocabolario condiviso di
"proposto, non applicato"; l'attenuazione (`opacity:.72` / `.92`) è il vocabolario di
"dell'IA".

### Cosa succede quando l'utente conferma un suggerimento
Tre azioni distinte, tutte con lo stesso esito sul dato:
1. **Confermare una proposta** (spunta in Revisione, oppure spunta verde sulla chip
   tratteggiata nel dettaglio foto) → `confirmSuggestion()` → `confirmed` + `human`, toast
   `"Tag confermato."`
   Effetti visibili: la chip esce dalla sezione "In attesa di conferma" e **ricompare fra le
   chip piene, dentro il gruppo della sua categoria**; il conteggio "N foto" del tag nella
   pagina "Tag e categorie" **aumenta di uno** (prima non contava, perché contava solo i
   `confirmed`); il conteggio della pagina Revisione e il badge in sidebar **calano di uno**; la
   foto entra a far parte dei risultati di quel filtro per tag/categoria; il gruppo scompare se
   era l'ultima proposta.
2. **Confermare una chip "IA"** (click sulla chip attenuata) → `confirmAiApplied()` →
   `confirmed` + `human`, toast `"Tag confermato."`
   Effetti visibili: **l'opacità torna piena, il marcatore "IA" sparisce, la chip smette di
   essere un pulsante**. Nessun conteggio cambia (era già `confirmed`): l'unica cosa che cambia
   è che ora c'è una firma umana sopra. È un'affermazione, non un'aggiunta.
3. **Aggiungere a mano** (selettore di tag, sezione 4) → `addManualTag()` → `confirmed` +
   `human`, senza passare per la coda. Il commento di riga 5521 lo giustifica: *«un'aggiunta
   manuale non passa mai dalla coda di revisione, è già una decisione dell'utente»*.

Il **rifiuto** e la **rimozione** convergono a loro volta:
`rejectSuggestion()` (croce in Revisione o nel dettaglio foto) e `removeTagFromPhoto()`
("x" sulla chip, o interruttore spento nel selettore di tag) scrivono entrambi
`rejected` + `human`. I toast differiscono:
`"Suggerimento rifiutato — non verrà riproposto."` per il primo, `"Tag rimosso."` per il
secondo. In entrambi i casi la decisione è **permanente**: nessuna rianalisi la riesaminerà.

### Dove la provenienza NON è mostrata (e sarebbe utile)
- **Pagina "Tag e categorie"**: il conteggio `"N foto"` somma senza distinzione le assegnazioni
  automatiche mai riviste e quelle confermate da un umano. Non c'è modo, da lì, di sapere
  quanto di un tag è "fidato".
- **Selettore di tag**: l'interruttore si accende sia per `confirmed`+`ai` sia per
  `confirmed`+`human`.
- **Filtro rapido (SP-3) e ricerca**: filtrano su `confirmedTagsForPhoto()`, quindi trattano le
  due origini allo stesso modo.
- **Tile della griglia (SP-1)**: non mostrano tag, quindi nemmeno la provenienza.
- **Nessun conteggio "N tag messi dall'IA in attesa di revisione"** esiste al di fuori della
  pagina Revisione e del badge.

### Punto fragile del modello, da segnalare al backend
`deleteTag()` (riga 5378) fa `delete TAG_ASSIGNMENTS[tagId]`: eliminando un tag si perdono
**anche tutti i suoi `rejected`**. Ricreando poi un tag con lo stesso nome (permesso: non ci
sono vincoli di unicità) si ottiene un tag nuovo di zecca, e tutte le decisioni "no" prese
dall'utente su quel concetto sono sparite. Nel sistema reale conviene decidere esplicitamente
se le decisioni umane sopravvivono all'eliminazione del tag, e come.

---

# Parte IX — Preferenze e organizzazione dei file

Blocco 09. Copre quattro schermate/dialog:

| # | Cosa | Funzione nel mockup | Righe |
|---|---|---|---|
| 1 | Impostazioni | `renderImpostazioni()` | 6097–6260 (+ `renderRegionSearchResults()` 6261–6284) |
| 2 | Profilo | `renderProfilo()` | 5981–6037 |
| 3 | Dialog "Rinomina con formula" | `openRenameDialog(scope)` | 5573–5672 |
| 4 | Dialog generico di inserimento testo | `openTextInputDialog(...)` | 2527–2559 |

Appendice: dialog "Scegli la cartella radice di culling" (`openCullingRootPickerDialog()`, righe
5675–5718) — documentato qui **solo** come impostazione di percorso; il significato del culling è
documentato nel blocco Culling.

---

## 60. Impostazioni

### 1. Nome e scopo

Pagina unica (`state.view === 'impostazioni'`, titolo/breadcrumb `"Impostazioni"`) in cui si
regolano le preferenze del server e dell'app: aspetto, densità della griglia, mappe offline,
cartella di culling, notifiche, lingua, livello IA e riconoscimento volti.

### 2. Cosa mostra

Otto sezioni `.settings-section` in quest'ordine esatto. Tutte le modifiche sono **immediate**:
non esiste alcun pulsante "Salva" per l'intera pagina.

**Sezione 1 — "Aspetto"**
- Titolo: `"Aspetto"`
- Sottotitolo: `"Chiaro, scuro, o segui il sistema operativo"`
- Un controllo segmentato (`.seg-control#themeSeg`) con tre opzioni: `"Chiaro"` / `"Scuro"` /
  `"Sistema"` (valori interni `chiaro` / `scuro` / `sistema`).
- **Predefinito: `"Chiaro"`** (`if(!state.themePref) state.themePref = 'chiaro'`; `state.theme`
  parte da `'light'`).
- Commento nel codice (riga 2338): *"Il controllo rapido in alto è stato rimosso: il tema si
  imposta da Impostazioni → Aspetto (Chiaro / Scuro / Sistema), un solo posto invece di due
  controlli ridondanti."* — quindi **non esiste un interruttore tema nella topbar**.

**Sezione 2 — "Densità griglia"**
- Titolo: `"Densità griglia"`
- Sottotitolo dinamico (`#densitySub`):
  `"Colonne nella Timeline su questo dispositivo (Desktop) — salvata separatamente da desktop e mobile (4 colonne)"`
  dove `Desktop`/`Mobile` è l'etichetta del form-factor corrente e il numero fra parentesi è il
  valore corrente.
- Un cursore `<input type=range class="density-slider" id="densitySlider">`.
- **Intervalli** (`densityRangeFor(device)`): **desktop min 2 / max 12**, **mobile min 2 / max 6**.
  Passo 1 (implicito).
- **Predefiniti**: `state.gridDensity = {desktop:4, mobile:3}`.
- Il valore è **salvato separatamente per dispositivo** e sopravvive al passaggio desktop↔mobile.
- **Cosa cambia**: il valore pilota `layoutJustifiedGrids()` (riga 3107). Su **mobile** diventa
  letteralmente `grid-template-columns: repeat(N, 1fr)` con tile quadrate. Su **desktop** è la
  griglia giustificata: `targetH = max(64, (larghezzaContenitore - (N-1)*6) / N / 1.3)` con gap
  fisso 6 px — cioè N è l'*obiettivo* di colonne, non un numero rigido (le righe si giustificano
  in base alle proporzioni delle foto).
- Commento nel codice: *"la densità pilota direttamente layoutJustifiedGrids() — non serve più
  passare da una CSS custom property, il valore è letto da state.gridDensity a ogni render."*

**Sezione 3 — "Mappe offline"**
- Titolo: `"Mappe offline"`
- Sottotitolo: `"Le tile sono servite da questo server Keeppix, mai da provider esterni — nessuna
  richiesta lascia la tua rete"`
- Elenco di righe `.region-row`, una per regione in `state.mapRegions`; ogni riga mostra:
  **nome regione** (`.region-name`) e, sotto, **`"<dimensione> · <stato>"`** (`.region-sub`),
  dove lo stato è la stringa `"scaricata"` o `"non scaricata"`.
- Regioni predefinite (le tre `builtin:true`, sempre presenti):
  | Nome | Dimensione | Stato iniziale |
  |---|---|---|
  | `Italia` | `640 MB` | `scaricata` |
  | `Europa (resto)` | `2,1 GB` | `non scaricata` |
  | `Resto del mondo` | `4,8 GB` | `non scaricata` |
- A destra di ogni riga: `"Rimuovi"` (se scaricata) **oppure** `"Scarica"` (se non scaricata);
  più una **X** (solo per le regioni **non** builtin) con
  `aria-label="Togli <nome> dalla lista"`.
- In fondo: pulsante `"+ Aggiungi regione"` (`#regionAddBtn`), che si trasforma in un riquadro di
  ricerca quando `state.regionPickerOpen === true`.
- Riquadro di ricerca (`.region-search-box`): etichetta per screen reader
  `"Cerca un paese o una regione da aggiungere"`, campo con placeholder
  `"Cerca un paese o una regione…"`, una X di chiusura con
  `aria-label="Chiudi ricerca regioni, senza aggiungere nulla"`, e una lista risultati
  (`role="listbox"`, `aria-label="Risultati ricerca regioni"`).
- Ogni risultato mostra **nome** e **dimensione**; **massimo 8 risultati**; le regioni già in
  elenco sono escluse.
- Vuoto con query digitata: `"Nessuna regione trovata."`
  Vuoto senza query: `"Digita per cercare tra le regioni disponibili — ce ne sono troppe per un
  elenco unico."`
- Il pool di regioni aggiungibili (`EXTRA_REGIONS_POOL`) contiene **35 voci**: Francia 480 MB,
  Germania 510 MB, Spagna 430 MB, Regno Unito 390 MB, Portogallo 220 MB, Paesi Bassi 180 MB,
  Belgio 150 MB, Svizzera 210 MB, Austria 230 MB, Grecia 340 MB, Irlanda 190 MB, Polonia 420 MB,
  Svezia 460 MB, Norvegia 510 MB, Danimarca 170 MB, Croazia 260 MB, Stati Uniti 3,2 GB,
  Canada 2,8 GB, Messico 980 MB, Brasile 1,6 GB, Argentina 890 MB, Cile 420 MB, Giappone 610 MB,
  Cina 2,4 GB, Corea del Sud 310 MB, Thailandia 380 MB, India 1,9 GB, Vietnam 340 MB,
  Indonesia 720 MB, Australia 1,3 GB, Nuova Zelanda 280 MB, Marocco 310 MB, Egitto 350 MB,
  Sudafrica 620 MB, Kenya 290 MB.
- Commento nel codice sul *perché* del campo di ricerca: *"elenco ampio apposta: con la scala
  mondiale delle mappe offline un elenco di chip diventa ingestibile, meglio un campo di ricerca
  (come le città/paesi in Immich o Google Maps)."*

**Sezione 4 — "Cartella di culling"**
- Titolo: `"Cartella di culling"`
- Sottotitolo: `"La cartella radice sul disco dentro cui vivono i lotti — una sottocartella per
  importazione. Serve anche a poterla sincronizzare da un altro computer (es. via WebDAV) dopo
  aver scelto le foto."`
- Una riga `.region-row` che mostra il **percorso corrente** (`state.cullingRootFolder`,
  predefinito `"/volume1/Foto/Culling"`) e sotto `"<N> lotti attivi"` (nel mockup `"3 lotti
  attivi"`, cioè `CULLING_BATCHES.length`).
- A destra: pulsante `"Cambia…"` (`#cullingRootChangeBtn`) → apre il dialog dell'albero cartelle
  (appendice in fondo).

**Sezione 5 — "Notifiche"**
- Titolo: `"Notifiche"` — **nessun sottotitolo**.
- Tre righe `.settings-row`, ciascuna con etichetta e un `mini-switch`:
  | Etichetta | Chiave | Predefinito |
  |---|---|---|
  | `"Riepilogo settimanale"` | `digest` | **acceso** |
  | `"Nuove condivisioni ricevute"` | `condivisioni` | **acceso** |
  | `"Problemi rilevati"` | `problemi` | **acceso** |
- Nessuna delle tre ha un sottotitolo esplicativo.

**Sezione 6 — "Lingua"**
- Titolo: `"Lingua"` — nessun sottotitolo.
- Un `<select>` (largo max 220 px) con due opzioni: `"Italiano"` e `"English"`.
- Predefinito: `"Italiano"` (prima opzione).
- **Il select non ha id né alcun gestore**: cambiarlo non fa nulla nel mockup.

**Sezione 7 — "Intelligenza artificiale"** (pattern **SP-11**)
- Titolo: `"Intelligenza artificiale"`
- Sottotitolo: `"Misurata automaticamente su questo server — le tre modalità sono tutte
  configurazioni valide, non errori"`
- Controllo segmentato `#aiTierSeg` (`role="radiogroup"`,
  `aria-label="Modalità intelligenza artificiale"`) con: `"Piena"` / `"Ridotta"` / `"Spenta"`
  (valori interni `pieno` / `ridotto` / `spento`).
- **Predefinito: `"Piena"`** (`state.aiTier: 'pieno'`).
- Sotto, una nota `.ai-tier-note` che cambia col livello (`AI_TIER_COPY`, testi alla lettera):
  - **Piena**: `"Hardware e database (serve pgvector o equivalente) adatti all'analisi automatica
    completa: tag, ricerca semantica e analisi in background su tutta la libreria."`
  - **Ridotta**: `"Hardware sufficiente per tag manuali, ricerca su richiesta e analisi di singole
    foto o cartelle a mano — ma non per l'analisi automatica di massa in background su tutta la
    libreria."`
  - **Spenta**: `"Questo server non ha hardware o database adatti all'IA. Tag manuali e ricerca per
    nome file, cartella o data restano disponibili; tag automatici e ricerca per descrizione sono
    disattivati."`
- Solo se il livello **non** è `spento`, compaiono due righe di **sola lettura**:
  - `"Modello in uso"` → `"CLIP ViT-B/32 (locale, via ONNX Runtime)"` (costante `AI_MODEL_NAME`;
    commento: *"modello locale, self-hosted come il resto di Keeppix — mai una chiamata a un
    servizio esterno"*).
  - `"Velocità misurata su questa macchina"` → `"<N> ms per foto"`, con **42 ms** in modalità Piena
    e **260 ms** in modalità Ridotta (`aiMsPerPhoto()`). Lo stesso numero alimenta le stime di
    tempo residuo altrove (`etaLabel()`).

**Sezione 8 — "Riconoscimento volti"**
- Titolo: `"Riconoscimento volti"`
- Sottotitolo: `"I volti sono dati biometrici — un trattamento diverso da un tag \"tramonto\". Non
  compaiono mai su un link pubblico condiviso: non è configurabile, vale sempre."`
- Una riga con etichetta `"Riconoscimento facciale attivo"` e un `mini-switch` (`#faceRecToggle`,
  `role="switch"`, `aria-checked`, `tabindex="0"`). **Predefinito: acceso**
  (`state.faceRecognitionEnabled: true`).
- **Solo quando l'interruttore è spento** compare un secondo sottotitolo: `"Disattivato: nessun
  volto nuovo viene rilevato, e \"Persone\" non mostra più nulla. I dati già raccolti restano
  salvati finché non li elimini qui sotto."`
- Pulsante pericoloso `"Elimina tutti i dati dei volti"` (`#faceDataDeleteBtn`, con icona cestino).

**Sezione `"Anteprima stati"` — in fondo alla pagina, e non è una preferenza vera.** È
riconoscibile a colpo d'occhio: bordo tratteggiato, fondo diverso dal resto, icona da laboratorio
accanto al titolo. Contiene tre interruttori — `"Rete lenta"`, `"Errore di caricamento"`,
`"Esito parziale"` — che accendono a comando gli stati che nel prodotto vero dipendono dalla rete.
È **scaffolding del prototipo e nel prodotto finito non esiste**: la sua ragione d'essere, il
comportamento dei tre interruttori e la macchina a stati che ci sta dietro sono documentati nella
**Parte X, "Il pannello Anteprima stati"**. Tutti gli interruttori sono **spenti** di
partenza, e con tutti spenti l'app si comporta come se la sezione non ci fosse.

### 3. Ogni controllo, uno per uno

| # | Etichetta esatta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Chiaro"` | opzione segmentata | `state.themePref='chiaro'`, `state.theme='light'`, `applyTheme()` + `renderAll()` |
| 2 | `"Scuro"` | opzione segmentata | `state.themePref='scuro'`, `state.theme='dark'` |
| 3 | `"Sistema"` | opzione segmentata | `state.themePref='sistema'`; legge `window.matchMedia('(prefers-color-scheme: dark)')` **una volta al click** e imposta di conseguenza `state.theme` |
| 4 | (nessuna etichetta) cursore densità | slider 2–12 (desktop) / 2–6 (mobile) | aggiorna `state.gridDensity[device]` e `state.customDensity[device]=true`; riscrive **solo** il testo di `#densitySub` |
| 5 | `"Scarica"` | pulsante (per regione non scaricata) | `status='scaricata'` + toast `"<nome> scaricata."` |
| 6 | `"Rimuovi"` | pulsante fantasma (per regione scaricata) | `status='non scaricata'` + toast `"<nome> rimossa — spazio liberato."` |
| 7 | X (icona) | pulsante fantasma, solo regioni **non builtin** | toglie la regione dall'elenco. **Nessun toast, nessuna conferma** |
| 8 | `"Aggiungi regione"` (con icona +) | pulsante fantasma | apre il riquadro di ricerca e mette il focus nel campo |
| 9 | `"Cerca un paese o una regione…"` | campo testo (placeholder) | filtro incrementale a ogni carattere; nessuna validazione; se vuoto mostra comunque i primi 8 candidati |
| 10 | X del riquadro ricerca | pulsante | chiude il riquadro **senza aggiungere nulla**, azzera la query |
| 11 | riga di risultato (nome + dimensione) | `role="option"`, attivabile | aggiunge la regione con `status:'non scaricata'`, `builtin:false`; chiude il riquadro; toast `"<nome> aggiunta all'elenco."` |
| 12 | `"Cambia…"` | pulsante | apre `openCullingRootPickerDialog()` |
| 13 | `"Riepilogo settimanale"` | interruttore | inverte `state.notifPrefs.digest` |
| 14 | `"Nuove condivisioni ricevute"` | interruttore | inverte `state.notifPrefs.condivisioni` |
| 15 | `"Problemi rilevati"` | interruttore | inverte `state.notifPrefs.problemi` |
| 16 | `"Italiano"` / `"English"` | menu a discesa | **nessun effetto** (non collegato) |
| 17 | `"Piena"` | opzione segmentata (radio) | `state.aiTier='pieno'` + `renderAll()` |
| 18 | `"Ridotta"` | opzione segmentata (radio) | `state.aiTier='ridotto'` |
| 19 | `"Spenta"` | opzione segmentata (radio) | `state.aiTier='spento'`; nasconde le due righe modello/velocità |
| 20 | `"Riconoscimento facciale attivo"` | interruttore (`role=switch`) | inverte `state.faceRecognitionEnabled` + `renderAll()`. Da spento, la pagina "Persone" mostra lo stato vuoto `"Riconoscimento volti disattivato"` |
| 21 | `"Elimina tutti i dati dei volti"` | pulsante fantasma pericoloso | apre una conferma (vedi sotto) |

Conferma del punto 21 (`openConfirmDialog`, pattern SP-5):
- Titolo: `"Eliminare tutti i dati dei volti?"`
- Testo: `"Persone, gruppi e volti riconosciuti verranno cancellati per sempre — non tocca le foto,
  solo i dati di riconoscimento facciale. Non è recuperabile."`
- Pulsanti: `"Elimina tutto"` (pericoloso) e `"Annulla"`.
- Alla conferma: `wipeAllFaceData()` svuota **FACES, PEOPLE e PERSON_GROUPS**, azzera
  `state.openPerson` e `state.personSelectedIds`, e mostra il toast `"Dati dei volti eliminati."`

### 4. Interazioni da mouse

- Click su tutti i controlli come da tabella sopra.
- **Doppio click**: non previsto nel mockup.
- **Tasto destro / menu contestuale**: non previsto nel mockup.
- **Hover**: `.seg-option` e `.btn` cambiano fondo (`.btn:hover{background:var(--chip-bg)}`);
  `.region-search-row:hover{background:var(--chip-bg)}`. Nessun tooltip `[data-tip]` in questa
  pagina.
- **Trascinamento**: non previsto (nessun riordino delle regioni, nessun drag&drop di cartelle).
- **Rotellina**: solo scorrimento normale della pagina; la lista risultati regioni ha il proprio
  scroll (`max-height:220px; overflow-y:auto`).
- Il cursore della densità si trascina con il mouse (comportamento nativo di `input[type=range]`).

### 5. Interazioni da tastiera

- **Nessuna scorciatoia globale** dedicata a Impostazioni.
- **Esc**: se il riquadro di ricerca regioni è aperto, lo chiude e azzera la query — e funziona
  **ovunque sia il focus**, non solo dentro il campo. Commento nel codice: *"prima si poteva
  restare 'intrappolati' lì se il focus era finito su un risultato o sul pulsante Annulla."*
- **Invio / Spazio** (SP-8, `bindActivatable`) attivano: opzioni IA, `"Cambia…"`,
  `"Riconoscimento facciale attivo"`, `"Elimina tutti i dati dei volti"`, `"Scarica"`,
  `"Rimuovi"`, la X di rimozione regione, `"Aggiungi regione"`, la X del riquadro ricerca, e le
  righe di risultato.
- **Non attivabili da tastiera** (solo `onclick`, senza `tabindex` né ruolo): le tre opzioni del
  **tema** e i tre interruttori delle **notifiche**. È un'incoerenza rispetto al resto della
  pagina (vedi §7 e le ambiguità).
- **Frecce**: il gruppo IA è dichiarato `role="radiogroup"` con roving `tabindex` (0 sull'attiva,
  −1 sulle altre) ma **la navigazione con le frecce non è implementata**: si passa da un'opzione
  all'altra solo con Tab dopo aver cambiato selezione. Nessuna navigazione con le frecce nella
  lista dei risultati regioni, nonostante `role="listbox"`/`role="option"`.
- **Tab**: ordine sorgente. Il campo di ricerca regioni riceve il focus automaticamente
  all'apertura, con il cursore **in fondo al testo** (`setSelectionRange(len, len)`).
- **Invio nel campo di ricerca**: non gestito (non aggiunge il primo risultato).
- Il cursore della densità risponde alle frecce con il comportamento nativo del browser.

### 6. Animazioni e transizioni

- **Interruttori**: il pomello si sposta con `left .15s ease` (`.mini-switch .knob`), da
  `left:2px` a `left:18px`; contemporaneamente il fondo passa da `--border-strong` a `--accent`
  (senza transizione dichiarata sul colore). Comunica il passaggio spento→acceso.
- **Controllo segmentato**: nessuna transizione dichiarata su `.seg-option`; l'opzione attiva
  riceve `background:var(--card-bg)`, `font-weight:600` e `box-shadow:var(--shadow)` — cambio
  istantaneo.
- **Cambio tema**: nessuna transizione di colore globale dichiarata; il tema si applica
  impostando `data-theme` su `#app` e ridisegnando tutto.
- **Toast** (SP-6): `opacity .2s ease, transform .2s ease`, compare dopo 10 ms, resta 2400 ms,
  poi svanisce e viene rimosso dopo altri 250 ms.
- **Focus**: `outline:2.5px solid var(--accent); outline-offset:2px` su `role=button`, `mini-switch`,
  `seg-option`, `input`, `select` (regola `:focus-visible` condivisa).
- Nessuna animazione di entrata/uscita per il riquadro di ricerca regioni: appare e sparisce
  istantaneamente al `renderAll()`.

### 7. Stati per ogni controllo

- **Opzioni tema** — normale / `.active` (fondo carta + grassetto + ombra). Hover: nessuna regola
  dedicata. Focus: **irraggiungibile** (nessun `tabindex`). Mai disabilitate.
- **Cursore densità** — normale; `accent-color:var(--accent)`. Nessuno stato disabilitato. Estremi
  bloccati dal `min`/`max` del dispositivo corrente.
- **Riga regione** — due stati mutuamente esclusivi guidati da `status`: *scaricata* (mostra
  `"Rimuovi"`) / *non scaricata* (mostra `"Scarica"`). Le regioni `builtin` non mostrano mai la X.
  Nessuno stato "in scaricamento": il passaggio è istantaneo, **non c'è barra di avanzamento**.
- **`"Aggiungi regione"`** — visibile solo quando il riquadro di ricerca è chiuso (i due si
  escludono a vicenda).
- **Campo di ricerca regioni** — normale / con focus (outline arancione) / lista vuota con due
  messaggi distinti (con e senza query).
- **Interruttori notifiche** — acceso/spento. Nessun `role`, nessun `aria-checked`, nessun focus.
- **Menu Lingua** — sempre abilitato ma **inerte**.
- **Opzioni IA** — normale / attiva / con focus. `aria-checked` riflette la selezione. Mai
  disabilitate: il commento chiarisce che *tutte e tre sono configurazioni valide, non errori*.
- **Righe "Modello in uso" / "Velocità misurata"** — sola lettura; **assenti** quando il livello è
  `Spenta` (non disabilitate: proprio non renderizzate).
- **`"Riconoscimento facciale attivo"`** — acceso/spento con `aria-checked` corretto; hover, focus.
- **`"Elimina tutti i dati dei volti"`** — **sempre abilitato**, anche quando il riconoscimento è
  già spento e anche dopo che i dati sono già stati cancellati (in quel caso l'operazione è un
  no-op ma mostra comunque il toast di conferma).
- **Stato di caricamento**: nessun controllo di questa pagina ha uno stato "in corso" — tutto è
  sincrono nel mockup.

### 8. Da dove ci si arriva e dove si va

**In ingresso:**
- Desktop: click sull'utente in fondo alla sidebar → menu utente → voce `"Impostazioni"`
  (icona ingranaggio).
- Mobile: avatar in alto a destra → menu account → voce `"Impostazioni"`.
- Dalla pagina **Persone** con riconoscimento disattivato: pulsante `"Vai a Impostazioni"`.
- Dalla schermata **Culling** (elenco lotti): il link `"Cambia in Impostazioni"` nella riga
  `"Cartella di culling: <percorso>"` porta qui.
- La voce `"Impostazioni"` **non** compare nel tab "Altro" della shell mobile
  (`renderLibreriaMenu`), per scelta esplicita: l'account è già sempre in alto a destra.

**In uscita:**
- `"Cambia…"` apre il dialog dell'albero cartelle, che ritorna qui.
- `"Elimina tutti i dati dei volti"` apre la conferma, che ritorna qui.
- Per il resto si esce solo navigando altrove dalla sidebar / tab bar. **Non c'è un pulsante
  "Indietro"** né un "Chiudi" nella pagina.

### 9. Dati necessari a questa schermata

**Legge:**
- preferenza di tema (`chiaro` / `scuro` / `sistema`) e, se `sistema`, la preferenza del sistema
  operativo;
- numero di colonne della griglia, **due valori distinti**: uno per desktop, uno per mobile, più il
  form-factor attualmente in uso;
- elenco delle regioni mappa: id, nome, dimensione leggibile, stato scaricata/non scaricata, se è
  una regione di base non rimovibile;
- catalogo delle regioni scaricabili (nome + dimensione) per la ricerca;
- percorso della cartella radice di culling e numero di lotti attivi al suo interno;
- tre preferenze di notifica (booleani);
- lingua dell'interfaccia;
- livello IA misurato/impostato, nome del modello in uso, millisecondi per foto misurati su questa
  macchina;
- se il riconoscimento facciale è attivo.

**Scrive:**
- la preferenza di tema;
- il numero di colonne per il dispositivo corrente (e un flag "densità personalizzata dall'utente");
- lo stato scaricata/non scaricata di una regione, l'aggiunta e la rimozione di una regione
  dall'elenco;
- il percorso della cartella radice di culling (via dialog);
- le tre preferenze di notifica;
- il livello IA;
- l'interruttore del riconoscimento facciale;
- la **cancellazione totale** di persone, gruppi di persone e volti riconosciuti.

---

## 61. Profilo

### 1. Nome e scopo

Pagina dell'account dell'utente corrente (`state.view === 'profilo'`, titolo `"Profilo"`): dati
anagrafici, colore dell'avatar, sicurezza e sessioni attive.

### 2. Cosa mostra

**Intestazione** (non è una `settings-section`):
- Avatar grande 56×56 px con le iniziali **`"GM"`**, colorato secondo `state.avatarColor`
  (pattern SP-16).
- Nome: `"Giovanni"` (17 px, grassetto).
- Sottotitolo: `"Proprietario · account su questo server Keeppix"`.

**Sezione 1 — "Dati account"**
- Titolo `"Dati account"`, nessun sottotitolo.
- Campo `"Nome visualizzato"`, tipo testo, valore iniziale `"Giovanni"`. **Nessun placeholder,
  nessuna validazione.**
- Campo `"Email"`, tipo testo (**non** `type=email`), valore iniziale
  `"gmastellone94@gmail.com"`. Nessun placeholder, nessuna validazione, nessun controllo di
  formato.
- Pulsante `"Salva modifiche"` — **non è collegato a nulla nel mockup**: nessun `id`, nessun
  gestore. Non salva, non mostra toast, non valida.

**Sezione 2 — "Colore avatar"**
- Titolo `"Colore avatar"`.
- Sottotitolo: `"Solo per te — cambia il colore di sfondo delle tue iniziali, ovunque compaiano
  nell'app."`
- Una fila di **8 pastiglie** circolari 30×30 px (`.avatar-color-swatch`), ciascuna con
  `role="button"`, `tabindex="0"`, `aria-label` e `aria-pressed`, e un tooltip `[data-tip]`
  (SP-7) con la stessa etichetta. La pastiglia scelta mostra una **spunta bianca** al centro.
  Elenco completo, nell'ordine:

  | # | Etichetta esatta (`aria-label` e tooltip) | id | Colore |
  |---|---|---|---|
  | 1 | `"Arancione (predefinito)"` | `accent` | `null` → usa `var(--accent)`, il colore di marca |
  | 2 | `"Blu"` | `blu` | `#3B82C4` |
  | 3 | `"Verde"` | `verde` | `#2E9E5B` |
  | 4 | `"Viola"` | `viola` | `#8B5CF6` |
  | 5 | `"Rosa"` | `rosa` | `#E0578A` |
  | 6 | `"Verde acqua"` | `teal` | `#0E9488` |
  | 7 | `"Grafite"` | `grafite` | `#3A3A3A` |
  | 8 | `"Rosso"` | `rosso` | `#D9503F` |

  **Predefinito: `"Arancione (predefinito)"`** (`state.avatarColor: null`).
  Commento nel codice sul *perché* di questa tavolozza: *"Le altre opzioni sono pensate per restare
  leggibili con testo bianco sopra — vedi .avatar{color:#fff}, non var(--accent-text) — e per
  distinguersi a colpo d'occhio dagli avatar colorati assegnati alle altre persone nella
  condivisione (quelli restano hash-based via hsl(), indipendenti da questa scelta personale)."*
  La scelta si riflette **ovunque** compaia l'avatar dell'utente corrente (sidebar, header mobile,
  Profilo), attraverso l'unica funzione `myAvatarStyle()`.

**Sezione 3 — "Sicurezza"**
- Titolo `"Sicurezza"`, nessun sottotitolo.
- Riga 1: etichetta `"Password"`, sottotitolo `"Ultima modifica: 3 mesi fa"` (stringa fissa), e a
  destra il pulsante fantasma `"Cambia password"` — **non collegato a nulla**.
- Riga 2: etichetta `"Autenticazione a due fattori"`, sottotitolo
  `"Consigliata per l'account proprietario"`, e a destra un `mini-switch` (`#twoFactorSwitch`).
  **Predefinito: spento** (`state.twoFactor = false`). Attivarlo **non apre alcuna procedura di
  configurazione** (nessun QR, nessun codice): inverte solo il flag.

**Sezione 4 — "Sessioni attive"**
- Titolo `"Sessioni attive"`.
- Sottotitolo: `"Dispositivi collegati con il tuo account"`.
- Due righe `.session-row`, ciascuna con **tre campi**: un'**icona** dispositivo (32×32 su fondo
  chip), la **descrizione del dispositivo**, e sotto l'**ultimo accesso**:

  | Descrizione dispositivo | Ultimo accesso | Sessione corrente? | Pulsante |
  |---|---|---|---|
  | `"Questo dispositivo — Chrome su macOS"` | `"Attiva ora"` | sì → alla descrizione si aggiunge `" · questa sessione"` in arancione grassetto | **nessuno** |
  | `"Keeppix iOS (WebApp)"` | `"Ultimo accesso: ieri, 21:40"` | no | `"Esci"` (fantasma) |

- In fondo: pulsante pericoloso `"Esci da tutti gli altri dispositivi"`.
- **Né `"Esci"` né `"Esci da tutti gli altri dispositivi"` sono collegati a un gestore**: nel
  mockup non fanno nulla e non chiedono conferma.

### 3. Ogni controllo, uno per uno

| # | Etichetta esatta | Tipo | Cosa fa |
|---|---|---|---|
| 1 | `"Nome visualizzato"` | campo testo | modificabile; nessun placeholder; nessuna validazione; se lasciato vuoto non succede nulla (non viene letto da nessuno) |
| 2 | `"Email"` | campo testo | idem; nessun controllo di formato indirizzo |
| 3 | `"Salva modifiche"` | pulsante | **inerte** nel mockup |
| 4–11 | le 8 pastiglie colore (vedi tabella sopra) | pulsante circolare | imposta `state.avatarColor` al valore del preset (o `null` per l'arancione) e ridisegna tutta l'app |
| 12 | `"Cambia password"` | pulsante fantasma | **inerte** |
| 13 | `"Autenticazione a due fattori"` | interruttore | inverte `state.twoFactor` + `renderAll()` |
| 14 | `"Esci"` (solo sulla sessione non corrente) | pulsante fantasma | **inerte** |
| 15 | `"Esci da tutti gli altri dispositivi"` | pulsante pericoloso | **inerte** |

### 4. Interazioni da mouse

- Click su una pastiglia colore: la seleziona immediatamente (nessuna conferma).
- **Hover su una pastiglia**: `box-shadow:0 0 0 2px var(--border-strong)`, con transizione
  `box-shadow .12s ease`; compare anche il tooltip col nome del colore (SP-7, `opacity .12s ease,
  transform .12s ease`; assente su mobile).
- **Hover sui pulsanti**: `.btn:hover{background:var(--chip-bg)}`;
  `.btn-danger:hover{background:var(--danger-tint)}`.
- **Doppio click / tasto destro / trascinamento**: non previsti nel mockup.
- **Rotellina**: solo scorrimento normale.

### 5. Interazioni da tastiera

- Le **pastiglie colore** sono attivabili con **Invio e Spazio** (SP-8) e raggiungibili con Tab
  (`tabindex="0"`); il tooltip compare anche con `:focus-visible`.
- Nessuna navigazione con le **frecce** fra le pastiglie: si scorrono una per una con Tab.
- L'interruttore **2FA** ha solo `onclick`: **non è raggiungibile né attivabile da tastiera** (né
  `tabindex`, né `role`, né `aria-checked`).
- `"Salva modifiche"`, `"Cambia password"`, `"Esci"`, `"Esci da tutti gli altri dispositivi"`: sono
  `<div class="btn">` senza `role` né `tabindex`, quindi **non raggiungibili da tastiera**.
- I due campi di testo sono normali `<input>`: Tab, digitazione, Invio non fa nulla (nessun submit).
- **Nessuna scorciatoia** dedicata a questa pagina.

### 6. Animazioni e transizioni

- **Pastiglia colore**: `transition:box-shadow .12s ease`. Da trasparente a
  `0 0 0 2px var(--border-strong)` in hover, e a `0 0 0 2px var(--text)` quando selezionata
  (`.on`). Comunica "questo è il colore attualmente scelto" con un anello scuro, distinto
  dall'anello grigio del solo passaggio del mouse.
- **Interruttore 2FA**: pomello `left .15s ease` (SP condiviso con Impostazioni).
- **Tooltip** dei colori: `opacity .12s ease, transform .12s ease`, sale di 3 px entrando.
- **Cambio colore avatar**: nessuna transizione — l'avatar cambia colore istantaneamente in tutti i
  punti dell'app al `renderAll()`.
- **Focus**: stesso outline arancione 2.5 px condiviso.

### 7. Stati per ogni controllo

- **Campi "Nome visualizzato" / "Email"** — normale, con focus (outline arancione). Mai
  disabilitati, mai in errore (nessuna validazione esiste).
- **`"Salva modifiche"`** — solo normale e hover. **Mai disabilitato**, anche se non è stato
  modificato nulla; e non ha stato "salvataggio in corso" né "salvato".
- **Pastiglie colore** — normale / hover (anello `--border-strong`) / selezionata `.on` (anello
  `--text` + spunta) / focus. Esattamente **una** è sempre selezionata.
- **`"Cambia password"`** — solo normale e hover.
- **Interruttore 2FA** — acceso/spento. Nessuno stato intermedio "in configurazione".
- **Righe sessione** — la sessione corrente si distingue per il suffisso `" · questa sessione"` e
  per **l'assenza** del pulsante `"Esci"` (è l'unico modo per capire quale non si può chiudere:
  non c'è un pulsante disabilitato, il pulsante proprio non c'è).
- **`"Esci da tutti gli altri dispositivi"`** — sempre visibile e sempre abilitato, anche quando
  esiste una sola altra sessione.
- **Stato vuoto**: non previsto — l'elenco sessioni è una costante di due elementi e non può mai
  essere vuoto nel mockup.

### 8. Da dove ci si arriva e dove si va

**In ingresso:**
- Desktop: click sull'utente in fondo alla sidebar → menu utente → voce `"Profilo"` (icona utente).
- Mobile: avatar in alto a destra → menu account → voce `"Profilo"`.
- Il menu contiene anche `"Impostazioni"` e, dopo un separatore, `"Esci"` (in rosso), che mostra
  solo il toast `"Solo demo — il logout reale disconnetterebbe la sessione."`
- Commento nel codice: la riga "profilo" è stata **tolta** dal tab "Altro" mobile perché
  *"duplicava l'avatar/account già sempre presente in alto a destra su ogni schermata mobile"*.

**In uscita:** nessuna navigazione parte da questa pagina. Non ci sono pulsanti che portano
altrove; si esce solo dalla sidebar / tab bar.

### 9. Dati necessari a questa schermata

**Legge:** nome visualizzato, email, iniziali per l'avatar, ruolo dell'account ("Proprietario") e
nome del server; colore avatar scelto; data dell'ultima modifica password (in forma già
leggibile, es. "3 mesi fa"); stato dell'autenticazione a due fattori; elenco delle sessioni
attive, ognuna con *tipo di dispositivo/browser*, *ultimo accesso in forma leggibile* e *se è la
sessione corrente*.

**Scrive:** il colore dell'avatar dell'utente; lo stato dell'autenticazione a due fattori.
(Il salvataggio di nome/email, il cambio password e la chiusura delle sessioni **non sono
implementati**: nel disegno andrebbero scritti anche nome visualizzato, email, e la revoca di una
o di tutte le altre sessioni.)

---

## 62. Dialog "Rinomina con formula"

### 1. Nome e scopo

Un unico pannello modale (`"Rinomina con formula"`) che ricompone il nome di uno o più file
partendo da uno schema testuale con segnaposto, con anteprima obbligatoria e blocco in caso di
nomi duplicati.

Commento di apertura nel codice (righe 5567–5572): *"RINOMINA CON FORMULA — un solo pannello, tre
punti d'ingresso (dettaglio foto singola, selezione multipla, intera cartella/lotto). Lo scope
decide solo l'etichetta e l'elenco di foto coinvolte: la logica di anteprima/collisione/
applicazione è identica ovunque."*
E, alla definizione dei token (righe 1628–1635): *"L'anteprima (prime foto) è obbligatoria: non si
applica mai 'alla cieca'. Se due nomi calcolati coincidono, l'avviso blocca 'Applica' finché lo
schema non li distingue (es. aggiungendo {n:3})."*

### 2. Cosa mostra

Card modale `.rename-card` larga **440 px** (su mobile `width:100%`), dentro uno scrim
(pattern SP-5). Dall'alto:

1. **Titolo**: `"Rinomina con formula"` (sempre uguale nei tre casi).
2. **Sottotitolo di ambito** (`.modal-sub`), **l'unica cosa che cambia fra i tre ingressi**:
   - foto singola → `"1 foto — <nome file attuale>"` (es. `"1 foto — DSC08421.ARW"`);
   - selezione multipla → `"<N> foto selezionate"`;
   - cartella o lotto → `"Tutta la cartella \"<nome>\" (<N> foto)"`
     (es. `"Tutta la cartella \"Dolomiti\" (184 foto)"`).
3. **Riga interruttore sulle sottocartelle** — **presente solo se l'ambito ha sottocartelle**
   (`scope.hasSubfolders`, vero **soltanto** per il rinomina di un lotto di culling):
   etichetta `"Includi anche presi e scartati, non solo da valutare"`, `mini-switch`
   (`#renameIncludeSub`, `role="switch"`, `aria-checked`, `tabindex="0"`), **predefinito: spento**.
4. **Campo** con etichetta `"Schema del nome file"` (`#renameSchemaInput`, `autocomplete="off"`).
   **Valore iniziale sempre `{data}_{luogo}_{n:3}`** — non viene ricordato fra un'apertura e
   l'altra: ogni apertura riparte da questo schema.
5. **Fila di pastiglie-segnaposto** (`.rename-token-row`), sei pulsanti (vedi §3).
6. **Etichetta anteprima**: `"Anteprima"` seguito da `"(prime foto)"` in grigio, peso normale.
7. **Lista di anteprima** (`#renamePreviewList`, alta max 150 px, scorrevole): **al massimo le
   prime 5 foto** dell'ambito attivo. Ogni riga: *nome attuale* (grigio, tagliato con ellissi) →
   icona chevron → *nome nuovo* (in grassetto, colore testo pieno).
   Se l'ambito è vuoto, una sola riga grigia: `"Nessuna foto in questo ambito."`
8. **Avviso collisioni** (`#renameCollisionWarning`), nascosto per default, su fondo rosso
   trasparente con bordo rosso: `"<N> nomi risulterebbero uguali tra loro — aggiungi {n:3} o un
   altro campo che li distingua prima di applicare."`
9. **Pulsanti**: `"Applica"` (primario) e `"Annulla"` (fantasma).

### 3. Ogni controllo, uno per uno

#### 3a. Il campo "Schema del nome file"

Testo libero. Non ha placeholder (parte già valorizzato). Non ha **nessuna validazione bloccante**:
qualunque testo è accettato, viene solo sanificato al calcolo (vedi 3c). Se lo si svuota
completamente, l'anteprima mostra il **nome attuale senza estensione, ricomposto con la stessa
estensione** — cioè il file resta com'è (`"schema vuoto: non rinomina davvero, resta il nome
attuale"`, commento riga 1714).

#### 3b. I sei segnaposto — sintassi esatta

I pulsanti inseriscono il segnaposto **alla posizione del cursore**, sostituendo l'eventuale
testo selezionato; il cursore si riposiziona **subito dopo** il segnaposto inserito e il focus
torna nel campo.

| Etichetta del pulsante | Testo inserito | Da cosa è sostituito |
|---|---|---|
| `"Data"` | `{data}` | Data di scatto in formato **ISO `AAAA-MM-GG`** (es. `2026-08-14`) |
| `"Fotocamera"` | `{fotocamera}` | Nome della fotocamera, "slugificato" |
| `"Obiettivo"` | `{obiettivo}` | Nome dell'obiettivo, "slugificato" |
| `"Luogo"` | `{luogo}` | Etichetta del luogo, "slugificata" |
| `"Titolo"` | `{titolo}` | Titolo facoltativo della foto, "slugificato" |
| `"Numero (001)"` | `{n:3}` | Contatore progressivo con **3 cifre**, a partire da `001` |

**Regole di sostituzione, esatte (`computeRenamedFilename`, righe 1701–1716):**

1. **Estensione**: presa dal nome file attuale (tutto dopo l'ultimo punto) e **messa in
   MAIUSCOLO**. Es. `.arw` → `ARW`. L'estensione **non è mai parte dello schema**: viene sempre
   riattaccata alla fine con un punto. Non è quindi possibile cambiare estensione da qui.
2. **Segnaposto testuali** — regex `\{(data|fotocamera|obiettivo|luogo|titolo)\}`, sostituzione
   globale. Un segnaposto scritto male o inesistente (es. `{iso}`, `{Data}` con la maiuscola)
   **resta nel nome così com'è, letterale**: non è un errore, non è segnalato.
3. **Contatore** — regex `\{n(?::(\d+))?\}`. Quindi sono valide **sia `{n}` (nessun riempimento:
   `1`, `2`, … `10`) sia `{n:<cifre>}` con qualunque numero di cifre** (`{n:2}` → `01`,
   `{n:3}` → `001`, `{n:5}` → `00001`). Il valore è **l'indice nell'elenco attivo + 1**: parte
   sempre da 1 a ogni operazione di rinomina, segue l'ordine dell'array delle foto dell'ambito, e
   **non tiene conto di file già presenti sul disco**. Si può usare più volte nello stesso schema.
4. **Sanificazione finale**: i caratteri `/`, `\` e `:` sono sostituiti con `-`; ogni sequenza di
   spazi bianchi è compressa in **un solo spazio**; il risultato viene **rifilato** ai bordi.
   Nota: **non** sono filtrati altri caratteri problematici (`*`, `?`, `"`, `<`, `>`, `|`).
5. **Fallback**: se dopo tutto questo la stringa è vuota, si usa il nome file attuale **privato
   dell'estensione**.
6. Risultato finale = `<stringa calcolata>` + `.` + `<ESTENSIONE MAIUSCOLA>`.

**"Slugificazione" dei valori testuali** (`renameSlug`, riga 1644), applicata a fotocamera,
obiettivo, luogo e titolo — **ma non alla data**:
- rifila gli spazi ai bordi;
- **elimina** i punti `.` e le virgole `,`;
- sostituisce ogni sequenza di spazi bianchi con un **trattino `-`**.

Esempio: `Sony A7 IV` → `Sony-A7-IV`; `Toscana — Val d'Orcia` → `Toscana-—-Val-d'Orcia`.

**Da dove viene il "Luogo"** (`placeLabelFor`, righe 1695–1699 + `photoPlace`, 1650–1655), in
ordine di priorità:
1. se la foto ha una posizione impostata a mano, quella (e il valore speciale "nessuna posizione"
   la azzera esplicitamente);
2. altrimenti, per una foto di libreria, la posizione **della sua cartella**;
3. altrimenti, per una foto di un **lotto di culling** appena importato (che non ha ancora alcuna
   posizione), si usa **il nome del lotto** — commento nel codice: *"lotto non ancora importato: il
   'luogo' è il nome del viaggio"*;
4. altrimenti stringa vuota.

**Valori mancanti**: se il titolo (o qualunque altro valore) è vuoto, **il segnaposto sparisce
semplicemente e basta** — commento riga 1708: *"se il titolo non è stato impostato, il pezzo
semplicemente non compare — non è un errore."* Attenzione: i **separatori restano**. Con lo schema
predefinito e un luogo vuoto si ottiene `2026-08-14__001.ARW` (due trattini bassi di fila). Il
mockup non ripulisce i separatori orfani.

#### 3c. L'anteprima

- Si ricalcola **a ogni carattere digitato** (`input.oninput`), a ogni pulsante-segnaposto premuto,
  a ogni cambio dell'interruttore sottocartelle, e una volta all'apertura.
- Calcola i nomi per **tutte** le foto dell'ambito attivo (serve per il controllo collisioni), ma
  **mostra solo le prime 5**.
- Confronto vecchio → nuovo su una riga sola, con troncamento a ellissi su entrambi i lati se non
  ci stanno.

#### 3d. Collisione di nomi

- Si contano le occorrenze di ogni nome calcolato **fra le foto dell'ambito attivo**; `dupCount` è
  **il numero di foto il cui nome compare più di una volta** (non il numero di gruppi duplicati:
  3 foto con lo stesso nome danno `dupCount = 3`).
- Se `dupCount > 0`:
  - l'avviso rosso compare con il testo `"<dupCount> nomi risulterebbero uguali tra loro — aggiungi
    {n:3} o un altro campo che li distingua prima di applicare."`;
  - `"Applica"` viene **disattivato**: `opacity:.4`, `pointer-events:none`, `aria-disabled="true"`.
- Se `dupCount === 0` l'avviso si nasconde e `"Applica"` torna normale.
- **Limite importante per il backend**: la collisione è verificata **solo all'interno del gruppo
  che si sta rinominando**. Non c'è alcun controllo contro i file già presenti sul disco, contro
  altre foto della stessa cartella non incluse nell'ambito, né contro le foto delle sottocartelle
  escluse dall'interruttore. Non esiste nessuna strategia di risoluzione automatica (nessun
  suffisso `(1)`, nessun rinvio): l'unico rimedio previsto è che l'utente cambi lo schema.
- Non esiste alcun controllo sulla **lunghezza massima** del nome, né sui caratteri illegali del
  filesystem oltre ai tre sostituiti.

#### 3e. L'interruttore sulle sottocartelle

- Etichetta esatta: `"Includi anche presi e scartati, non solo da valutare"`.
- Compare **solo** quando si rinomina un **lotto di culling** intero (`hasSubfolders: true`);
  in tutti gli altri ingressi (foto singola, selezione multipla nella libreria, selezione multipla
  nel culling, rinomina di una cartella della libreria) la riga **non esiste**.
- **Spento (predefinito)**: l'ambito è ristretto alle foto ancora nella radice del lotto, cioè
  quelle **"Da valutare"**.
- **Acceso**: l'ambito include tutte le foto del lotto, comprese quelle già fisicamente spostate in
  `_presi` e `_scartati`.
- Cambiarlo ricalcola immediatamente anteprima, numerazione (`{n}` riparte da 1 sul nuovo elenco) e
  controllo collisioni.

#### 3f. I pulsanti

| Etichetta | Tipo | Cosa fa |
|---|---|---|
| `"Applica"` | pulsante primario | ricalcola, poi **esce senza fare nulla** se `dupCount > 0` **o** se l'elenco è vuoto; altrimenti riscrive il nome file di ogni foto dell'ambito, mostra il toast, chiude il dialog, chiama l'eventuale callback dell'ambito e ridisegna |
| `"Annulla"` | pulsante fantasma | chiude senza applicare nulla; nessuna conferma, nessun avviso di modifiche non salvate |

**Toast dopo l'applicazione**: `"<N> fote rinominate."` (per N ≠ 1) / `"1 fota rinominata."` —
è la stringa letterale prodotta dal codice `` `${list.length} fot${list.length===1?'a rinominata':'e rinominate'}.` ``.
Vedi le ambiguità in fondo: è un errore di flessione (in italiano "foto" è invariabile).

**Callback di ambito** (`scope.onApplied`): usata solo dalla rinomina della **selezione multipla
nel culling**, dove svuota la selezione dopo l'applicazione. Negli altri casi non è passata.

### 4. Interazioni da mouse

- **Click** su un pulsante-segnaposto: inserisce il segnaposto al cursore (o al posto della
  selezione) e riporta il focus nel campo.
- **Click** su `"Applica"` / `"Annulla"`: come sopra.
- **Click sullo scrim** (fuori dalla card): **non chiude il dialog** — non è implementato.
- **Doppio click / tasto destro**: non previsti.
- **Hover** su una pastiglia-segnaposto: `border-color:var(--border-strong)` e colore testo pieno
  (partendo da bordo trasparente e testo secondario). Nessun tooltip: l'etichetta è già scritta.
- **Trascinamento**: non previsto (non si riordinano i segnaposto).
- **Rotellina**: la lista di anteprima scorre da sola oltre i 150 px di altezza; il resto della
  card no.
- Selezione di testo dentro il campo: comportamento nativo, ed è **rilevante** perché il pulsante
  segnaposto sostituisce la selezione.

### 5. Interazioni da tastiera

- **Esc**: chiude il dialog (listener globale `keydown` registrato all'apertura e rimosso alla
  chiusura). Funziona ovunque sia il focus, campo di testo compreso.
- **Invio nel campo schema**: **non gestito** — non applica. Va premuto `"Applica"`.
- **Invio / Spazio** (SP-8) attivano: i sei pulsanti-segnaposto, l'interruttore sottocartelle,
  `"Applica"`, `"Annulla"`.
- **Tab / Shift+Tab**: ordine sorgente — interruttore sottocartelle (se presente) → campo schema →
  i sei segnaposto in ordine → `"Applica"` → `"Annulla"`. **Non c'è trappola del focus**: Tab può
  uscire dal dialog e raggiungere gli elementi sottostanti.
- **Focus all'apertura**: sul **campo schema**, con il cursore **in fondo al testo**
  (`setSelectionRange(len, len)`), non con il testo selezionato.
- **Focus alla chiusura**: torna all'elemento che ha aperto il dialog (`document.activeElement`
  salvato all'apertura) — vale sia per `"Annulla"`, sia per Esc, sia dopo `"Applica"`.
- **Frecce**: nessuna navigazione fra i segnaposto; solo il movimento nativo del cursore dentro il
  campo.
- **Modificatori** (Cmd/Ctrl, Shift, Alt): nessun comportamento speciale; valgono solo le funzioni
  native del campo di testo.

### 6. Animazioni e transizioni

- **Nessuna animazione di entrata/uscita** del dialog né dello scrim: la card compare e sparisce
  istantaneamente.
- **Pastiglia-segnaposto**: nessuna transizione dichiarata su `.rename-token-btn`; il cambio di
  bordo/colore in hover è immediato.
- **Interruttore sottocartelle**: pomello `left .15s ease`.
- **Avviso collisioni**: appare/scompare con `display:block`/`display:none`, senza dissolvenza.
  Comunica con il rosso (fondo `rgba(214,80,52,.1)`, bordo `rgba(214,80,52,.3)`, testo
  `var(--danger)`) che c'è un problema che va risolto prima di procedere.
- **`"Applica"` disattivato**: passa a `opacity:.4` **senza transizione** — cambio istantaneo,
  legato al ricalcolo dell'anteprima.
- **Toast** finale: SP-6 (`opacity .2s ease, transform .2s ease`, visibile 2400 ms).
- **Focus visibile**: outline arancione 2.5 px con offset 2 px.

### 7. Stati per ogni controllo

- **Campo schema** — normale / con focus. Mai disabilitato, mai in errore visivo (l'errore si
  manifesta solo come avviso collisioni sotto l'anteprima, mai come bordo rosso del campo).
- **Pastiglie-segnaposto** — normale (fondo chip, testo secondario, bordo trasparente) / hover /
  focus. **Mai disabilitate**, nemmeno quando il valore corrispondente è vuoto per tutte le foto
  dell'ambito (es. `"Titolo"` su foto senza titolo): l'utente lo scopre solo dall'anteprima.
- **Interruttore sottocartelle** — spento (predefinito) / acceso / focus. Presente solo
  nell'ingresso "lotto".
- **Lista anteprima** — con contenuto (max 5 righe) / **vuota** (`"Nessuna foto in questo
  ambito."`, testo terziario). Nessuno stato di caricamento: il calcolo è sincrono.
- **Avviso collisioni** — nascosto / visibile.
- **`"Applica"`** — normale / **disattivato** quando ci sono nomi duplicati (opacità .4,
  `pointer-events:none`, `aria-disabled="true"`). **Attenzione**: quando l'ambito è **vuoto** il
  pulsante resta visivamente **abilitato** ma non fa nulla (il guard `list.length===0` è solo nel
  gestore). Inoltre, essendo disattivato solo via `pointer-events`, un utente da tastiera può
  ancora dargli focus e premere Invio: il gestore lo respinge silenziosamente, senza alcun
  riscontro.
- **`"Annulla"`** — sempre abilitato.

### 8. Da dove ci si arriva e dove si va

**I punti d'ingresso sono cinque** (tre "tipi" di ambito):

| Da dove | Etichetta del comando | Ambito passato |
|---|---|---|
| **Lightbox / dettaglio foto**, pulsante nel pannello azioni | `"Rinomina…"` (icona matita) | `single`, 1 foto, senza sottocartelle |
| **Lightbox**, menu `⋯` "altre azioni" | `"Rinomina…"` | idem |
| **Modifica multipla** (`bulkEdit`) | `"Rinomina con formula…"` | `selection`, le foto selezionate, senza sottocartelle |
| **Vista Foto (timeline) con una cartella aperta**, barra strumenti | `"Rinomina cartella…"` | `folder`, **le foto attualmente visibili dopo i filtri rapidi** (`photosList`, non tutte le foto della cartella), etichetta = nome cartella, senza sottocartelle |
| **Culling — lotto aperto**, barra strumenti | `"Rinomina lotto…"` | `folder`, tutte le foto del lotto, etichetta = nome lotto, **`hasSubfolders: true`** |
| **Culling — selezione multipla nel filmino**, pulsante icona | `aria-label` e tooltip `"Rinomina…"` | `selection`, le foto selezionate, senza sottocartelle; alla fine **svuota la selezione** |

Il pulsante `"Rinomina cartella…"` **non compare quando si sta guardando "tutte le foto"** (nessuna
cartella selezionata): in quel caso al suo posto c'è uno spazio vuoto.

**In uscita:** il dialog torna sempre alla schermata da cui è stato aperto, con il focus
ripristinato sul comando che lo ha aperto. Non porta mai altrove.

### 9. Dati necessari a questa schermata

**Legge, per ogni foto dell'ambito:**
- nome file attuale (con estensione — l'estensione viene riusata);
- data di scatto (usata in formato ISO `AAAA-MM-GG`);
- modello di fotocamera;
- modello di obiettivo;
- titolo facoltativo (può essere vuoto);
- etichetta del luogo, risolta con la precedenza: posizione impostata a mano sulla foto → posizione
  della cartella → nome del lotto di culling → niente;
- **solo per i lotti di culling**: se la foto è ancora "da valutare" oppure già "presa"/"scartata"
  (serve al filtro dell'interruttore sottocartelle).

**Legge, sull'ambito:**
- il tipo di ambito (una foto / una selezione / una cartella o lotto) e la sua etichetta;
- il numero totale di foto coinvolte;
- se l'ambito ha sottocartelle presi/scartati.

**Scrive:**
- il **nuovo nome file** di ogni foto dell'ambito attivo, in blocco e in un colpo solo. Nient'altro
  viene toccato: né la posizione fisica, né i tag, né la valutazione.

**Non legge/non scrive** (ma un backend reale dovrà occuparsene): l'elenco dei nomi già occupati
sul disco nella cartella di destinazione, e la gestione dei file affiancati (il RAW e il JPEG di
una stessa coppia RAW+JPEG — nel mockup ogni foto ha un solo `filename` e la rinomina agisce su
quello).

---

## 63. Dialog generico di inserimento testo

### 1. Nome e scopo

Dialog modale riusabile per chiedere **una sola stringa** all'utente (`openTextInputDialog(title,
sub, placeholder, initialValue, confirmLabel, onConfirm)`); commento nel codice: *"stessa struttura
di openConfirmDialog ma con un campo invece di sì/no."*

### 2. Cosa mostra

- Card modale standard `.modal-card` (larga **360 px**; su mobile `86%`), pattern SP-5.
- **Titolo** (parametro `title`), obbligatorio.
- **Sottotitolo** (parametro `sub`) — **la riga intera non viene renderizzata se il testo è
  vuoto**.
- Un campo testo (`#textInputField`) con `autocomplete="off"`, il cui `placeholder` e valore
  iniziale arrivano dai parametri. L'etichetta del campo è **solo per screen reader**
  (`class="sr-only"`) e ripete il titolo del dialog.
- Due pulsanti: quello di conferma con **etichetta variabile** (parametro `confirmLabel`), e
  `"Annulla"` (etichetta fissa).

**I tre usi presenti nel mockup** (tutti nell'area Persone/volti):

| Titolo | Sottotitolo | Placeholder | Valore iniziale | Etichetta conferma |
|---|---|---|---|---|
| `"Nuovo gruppo"` | `"Un gruppo raccoglie più persone fotografate (es. \"Famiglia\", \"Amici\") — non è un gruppo di utenti dell'app, quello esiste già altrove per i permessi."` | `"Es. Famiglia, Amici, Colleghi…"` | vuoto | `"Crea"` |
| `"Rinomina gruppo"` | *(nessuno)* | `"Nome gruppo"` | nome attuale del gruppo | `"Salva"` |
| `"Rinomina persona"` | *(nessuno)* | `"Nome"` | nome attuale della persona | `"Salva"` |

### 3. Ogni controllo, uno per uno

| # | Controllo | Tipo | Cosa fa |
|---|---|---|---|
| 1 | campo testo | input | il valore viene **sempre rifilato** (`trim()`) prima di essere passato al chiamante |
| 2 | pulsante di conferma (`"Crea"` / `"Salva"`) | pulsante primario | **chiude prima il dialog, poi** chiama la callback col valore rifilato |
| 3 | `"Annulla"` | pulsante fantasma | chiude senza chiamare la callback |

**Validazione — sta nei chiamanti, non nel dialog.** Il dialog **non valida nulla** e non
disabilita mai la conferma:
- `"Nuovo gruppo"`: se il nome è vuoto, la callback fa `return` — **niente viene creato e non
  viene mostrato alcun messaggio d'errore**. All'utente sembra semplicemente che non sia successo
  nulla. Se non è vuoto: crea il gruppo e mostra il toast `"Gruppo \"<nome>\" creato."`
- `"Rinomina gruppo"`: stesso guard sul vuoto (silenzioso). Altrimenti rinomina e mostra
  `"Gruppo rinominato."`
- `"Rinomina persona"`: **nessun guard sul vuoto** — confermare con il campo vuoto **azzera il
  nome della persona**, che torna a essere una persona "da nominare". Il toast
  `"Persona rinominata."` viene mostrato comunque. È un'incoerenza rispetto agli altri due usi
  (potrebbe però essere il modo voluto per "togliere il nome").
- Nessun controllo di lunghezza massima, di caratteri ammessi, o di nomi duplicati.

### 4. Interazioni da mouse

- Click sui due pulsanti; click e selezione nel campo.
- **Click sullo scrim**: **non chiude** il dialog (non implementato).
- **Doppio click / tasto destro / trascinamento / rotellina**: non previsti.
- Hover: `.btn-primary:hover{filter:brightness(1.05)}`, `.btn-ghost:hover{background:var(--chip-bg)}`.

### 5. Interazioni da tastiera

- **Invio dentro il campo**: **conferma** (`e.preventDefault()` + stessa azione del pulsante). È
  l'unica differenza rilevante rispetto al dialog di rinomina, dove Invio non fa nulla.
- **Esc**: chiude senza confermare, da qualunque punto abbia il focus.
- **Invio / Spazio** sui due pulsanti (SP-8).
- **Focus all'apertura**: sul campo, con **tutto il testo selezionato** (`input.select()`) — così
  digitare sostituisce subito il nome precedente. (Diverso dal dialog di rinomina, che mette il
  cursore in fondo senza selezionare.)
- **Focus alla chiusura**: torna al comando che ha aperto il dialog.
- **Tab / Shift+Tab**: campo → conferma → `"Annulla"`. **Nessuna trappola del focus.**

### 6. Animazioni e transizioni

- Nessuna animazione di apertura/chiusura del dialog o dello scrim.
- Focus visibile: outline arancione 2.5 px, offset 2 px.
- Toast conseguenti: SP-6.

### 7. Stati per ogni controllo

- **Campo** — normale / con focus / testo preselezionato all'apertura. **Mai in errore**, mai
  disabilitato: un valore non valido (vuoto) non è segnalato in alcun modo.
- **Pulsante di conferma** — normale / hover / focus. **Mai disabilitato**, nemmeno con il campo
  vuoto.
- **`"Annulla"`** — sempre abilitato.
- **Stato vuoto / di caricamento**: non applicabili.

### 8. Da dove ci si arriva e dove si va

Aperto da: pulsante `"Nuovo gruppo"` nella griglia Persone; icona matita sull'intestazione di un
gruppo di persone (`aria-label="Rinomina gruppo <nome>"`); pulsante `"Rinomina…"` nel dettaglio di
una persona. In tutti e tre i casi si torna alla schermata di partenza con il focus ripristinato.

### 9. Dati necessari a questa schermata

**Legge:** il valore corrente da modificare (nome del gruppo o della persona), oppure niente per la
creazione. **Scrive:** la stringa rifilata, che il chiamante usa per creare un gruppo di persone,
rinominare un gruppo, o rinominare una persona.

---

## 64. Appendice — Dialog "Scegli la cartella radice di culling" (dal punto di vista dell'impostazione)

Aperto **solo** dal pulsante `"Cambia…"` della sezione `"Cartella di culling"` in Impostazioni
(righe 5675–5718). Qui è documentato **come impostazione di percorso**; per il significato dei
lotti, dei presi/scartati e della sincronizzazione, vedi il blocco Culling.

- **Titolo**: `"Scegli la cartella radice di culling"`. **Sottotitolo**: `"Dentro, ogni
  sottocartella diventa un lotto — Keeppix crea da sola le sottocartelle dei presi/scartati quando
  servono."`
- Card modale larga **420 px**, pattern SP-5 (scrim, `role=dialog`, Esc chiude, focus di ritorno al
  pulsante `"Cambia…"`). Anche qui **il click sullo scrim non chiude**.
- **Briciole di pane** cliccabili (`/ volume1 / Foto / Culling …`): cliccare un segmento risale a
  quel livello. Hover: sottolineatura + colore testo pieno.
- **Elenco delle sottocartelle** del livello corrente (alto max 220 px, scorrevole): icona cartella,
  nome, chevron. Cliccare (o Invio/Spazio) entra nella cartella. Se non ce ne sono:
  `"Nessuna sottocartella qui."` Hover riga: `background:var(--chip-bg)`.
- **Pulsanti**: `"Usa questa cartella"` (primario) e `"Annulla"`.
- Alla conferma: `state.cullingRootFolder = '/' + <percorso>`, toast `"Cartella di culling
  aggiornata."`, chiusura e ridisegno. **Nessuna validazione** (si può confermare anche la radice
  `/`, ottenendo il percorso vuoto `"/"`); nessuna verifica di permessi di scrittura, nessuna
  possibilità di creare una cartella nuova, nessun campo per digitare un percorso a mano.
- Il dialog si apre già posizionato sul percorso corrente, se esiste nell'albero; altrimenti
  riparte dalla radice.
- **Albero finto**: `MOCK_FS_TREE` contiene `/volume1/{Foto/{Culling/{2026, Archivio}, Libreria},
  Backup}`. Commento nel codice: *"Albero finto: non c'è un vero filesystem in questo mockup, ma la
  cartella radice di culling (impostabile in Impostazioni) deve comunque poter essere 'scelta'
  navigando delle cartelle."*
- **Nessuna navigazione con le frecce** nell'elenco; nessun focus automatico su una riga
  all'apertura; nessuna trappola del focus.
- **Nessuna animazione**.
- Il valore scelto è mostrato in **due posti**: nella sezione Impostazioni e nell'intestazione
  dell'elenco lotti del Culling (`"Cartella di culling: <percorso>"` con il link
  `"Cambia in Impostazioni"`, che riporta qui in Impostazioni ma **non apre** direttamente il
  dialog).

---

# Parte X — Scala, caricamento ed errore

Questa parte copre tre cose che nelle altre sezioni compaiono solo di sfuggita, perché non
appartengono a una schermata sola: **come la libreria regge la scala**, **cosa si vede mentre i
dati arrivano** e **cosa si vede quando non arrivano**.

Sono le tre parti dell'interfaccia che dipendono di più dal backend, ed è per questo che stanno
insieme e in fondo: l'architetto può leggere questa parte da sola.

---

## 65. Perché questa parte esiste

Nelle prime versioni del disegno la timeline mostrava al massimo 24 foto per mese, mentre
l'intestazione dichiarava il conteggio vero ("375 scatti"). Le altre 351 non erano raggiungibili
in nessun modo. Nello stesso periodo non esisteva alcuno stato di caricamento né di errore: i
dati del prototipo erano già in memoria, quindi ogni operazione riusciva sempre e riusciva subito.

Erano le due lacune più gravi dell'intero disegno, e sono state chiuse entrambe. Questa parte
documenta come.

---

## 66. La timeline a scala reale

### 1. Nome e scopo

Il meccanismo che permette alla timeline di contenere l'intera libreria — decine o centinaia di
migliaia di scatti — senza che il numero di foto influisca sulla reattività della pagina.

### 2. Cosa mostra

Dal punto di vista dell'utente non mostra nulla di proprio: mostra la timeline, completa. Il
punto è precisamente che **non c'è niente da notare**. Nessun pulsante "mostra altre", nessun
limite che si raggiunge, nessuna attesa quando si scorre veloce, nessun salto della barra di
scorrimento mentre le foto arrivano.

L'unico segno visibile è indiretto: la barra di scorrimento ha **da subito** la lunghezza
definitiva, e lo scrubber dei mesi salta esattamente sul mese scelto.

### 3. Come funziona, in tre passi

**Primo — la geometria si calcola prima, e senza disegnare niente.**
Dai soli rapporti d'aspetto degli scatti si può sapere in anticipo dove finirà ogni singola
tessera: quali foto stanno su quale riga, quanto è alta ogni riga, quanto è alto ogni mese,
quanto è alta la timeline intera. È lo stesso algoritmo giustificato descritto in **SP-22**, ma
eseguito sui dati invece che sugli elementi a schermo. Non richiede di aver caricato le
miniature: basta sapere, per ogni scatto, quanto è largo rispetto a quanto è alto.

**Secondo — la pagina prende subito la sua altezza definitiva.**
Ogni mese è un contenitore con l'altezza già corretta, anche se al suo interno non c'è ancora
niente. Da qui discendono tre proprietà che l'utente percepisce come "l'app è solida": la barra
di scorrimento è veritiera dal primo istante; lo scrubber dei mesi atterra sul punto giusto anche
su un mese mai visitato; e niente si sposta sotto il dito mentre si scorre.

**Terzo — esistono solo le righe che si stanno guardando.**
Solo le righe che ricadono nella finestra visibile, più un margine di circa un schermo e un
quarto sopra e sotto, sono davvero presenti nel documento. Scorrendo, le righe che escono vengono
smontate e quelle che entrano montate. Il margine serve perché lo scorrimento veloce non arrivi
mai "prima" del contenuto.

**La conseguenza da tenere a mente:** il costo della timeline dipende dall'altezza della finestra,
non da quante foto ci sono in libreria. Una libreria da 200.000 scatti costa quanto una da 200.
È questo che rende il vecchio tetto di 24 non più necessario, invece che semplicemente rimosso.

### 4. Interazioni da mouse

Nessuna di propria: rotellina e trascinamento della barra si comportano come su una pagina
normale. Lo scrubber dei mesi è descritto nella sezione della timeline.

### 5. Interazioni da tastiera

Nessuna di propria. Le tessere montate sono raggiungibili e attivabili esattamente come se fossero
sempre state lì (**SP-8**).

**Punto di attenzione per l'implementazione:** una tessera che esce dalla finestra viene smontata.
Se in quel momento aveva il fuoco, il fuoco va gestito — altrimenti si perde e la navigazione da
tastiera "cade" a inizio pagina. Nel prototipo il caso non è coperto.

### 6. Animazioni e transizioni

Le righe che entrano compaiono con una dissolvenza di **0,18 s**. È volutamente brevissima: serve
a smussare l'apparizione, non a farsi notare. Con la preferenza di sistema "riduci le animazioni"
attiva, la dissolvenza è disattivata.

### 7. Stati

- **Normale** — le righe visibili sono montate, le altre no.
- **Ricalcolo** — al cambio di larghezza della finestra o di densità della griglia, la geometria
  viene ricalcolata da capo e le righe rimontate.
- **In caricamento** — vedi la sezione successiva: al posto del contenuto compare lo scheletro.
- **Errore** — vedi oltre.

### 8. Dove si applica

Alla timeline **Foto** con le sue sezioni per mese, e in forma semplificata — una sezione sola,
senza intestazione — a **Preferiti**, al **dettaglio di un album** e al **dettaglio di una
persona**. Le griglie minori (risultati di ricerca, cestino, duplicati) usano ancora il layout
diretto: sono liste corte per costruzione, ma se in futuro potessero crescere andrebbero portate
sullo stesso meccanismo.

### 9. Dati necessari

Qui c'è la richiesta più importante di tutto il documento per chi lavora sul backend.

**Serve poter conoscere le proporzioni di tutti gli scatti di una vista senza doverne caricare le
miniature.** Per ogni foto della vista: un identificativo, la larghezza e l'altezza (o
direttamente il rapporto), e il mese di appartenenza. Nient'altro. Sono pochi byte per scatto, e
con quelli l'interfaccia costruisce l'intera geometria della libreria in un colpo solo.

Le miniature vere servono solo per le righe che si stanno effettivamente guardando, e vengono
richieste mentre si scorre.

Serve inoltre, per la timeline: **il conteggio reale di foto per ogni mese**, che è un dato
aggregato e non deriva dall'elenco caricato.

Se questa separazione fra "proporzioni di tutto" e "immagini di poco" non fosse possibile, il
disegno andrebbe rivisto: è l'unico punto del documento in cui l'interfaccia impone davvero una
forma al backend.

---

## 67. Caricamento

### 1. Nome e scopo

Cosa vede l'utente nell'intervallo fra "ho chiesto qualcosa" e "è arrivato".

### 2. Il principio

**Il caricamento non è mai uno spinner al centro del vuoto.** È uno scheletro che ha già la forma
del contenuto in arrivo: intestazioni, righe, tessere di larghezze diverse impaginate come lo
sarebbero le foto vere.

Due ragioni, una tecnica e una umana. Tecnica: quando i dati atterrano prendono il posto dello
scheletro senza che nulla si sposti, quindi il layout non "salta". Umana: lo scheletro dice già
*che forma* avrà la risposta, quindi l'occhio sa dove si poserà — un'attesa con una forma è più
breve della stessa attesa davanti al vuoto.

### 3. Ogni forma di caricamento, una per una

| Dove | Cosa si vede |
|---|---|
| **Timeline** | Due mesi scheletro: intestazione in scheletro (titolo e conteggio) più una griglia giustificata di tessere neutre. Due e non uno perché il ritmo "titolo, griglia, titolo, griglia" fa parte di ciò che si sta annunciando. |
| **Griglie piatte** (Preferiti, album, persona) | La sola griglia scheletro, senza intestazioni. |
| **Azione su più foto** | Il pulsante che l'ha avviata passa in stato occupato: perde la reattività al click e affianca all'etichetta un indicatore rotante. |
| **Caricamento sopra contenuto già presente** | Una barra sottile e indeterminata in cima all'area contenuto: dice "sto lavorando" senza sostituire ciò che l'utente sta già guardando. |

### 4. Interazioni da mouse

Lo scheletro è inerte: non è cliccabile, non risponde al passaggio del mouse, non è selezionabile.
È dichiarato come decorativo, quindi gli assistenti vocali lo saltano e annunciano invece
"caricamento in corso".

### 5. Interazioni da tastiera

Durante il caricamento non ci sono elementi raggiungibili con Tab nell'area che sta caricando: il
fuoco passa oltre. Il pulsante occupato **resta** raggiungibile ma non attivabile.

### 6. Animazioni e transizioni

- Lo scheletro ha un riflesso che scorre da sinistra a destra, ciclo di **1,15 s**, lineare. Usa un
  grigio neutro e **non** il colore d'accento: il caricamento non deve sembrare un elemento attivo
  o selezionato.
- L'indicatore rotante compie un giro in **0,7 s**.
- La barra indeterminata percorre l'area in **1,1 s** con accelerazione e decelerazione.
- Con "riduci le animazioni" attiva: il riflesso si ferma, la barra diventa fissa e traslucida,
  la rotazione rallenta a **2,4 s**.

### 7. Stati

Lo stato di un insieme di dati è uno di tre: **in caricamento**, **pronto**, **errore**. Il
prototipo li tiene per *insieme di dati* e non per schermata — così tornando su una cartella già
letta non si rivede lo scheletro, come farebbe una cache vera.

### 8. Da dove si arriva

Ogni cambio di vista o di cartella richiede il proprio insieme di dati. Il primo accesso carica,
i successivi no.

### 9. Dati necessari

Nulla di nuovo rispetto alle schermate: quello che cambia è che ogni richiesta ha una **durata** e
può **non riuscire**. Per il frontend serve poter distinguere i tre stati; per il backend serve
che le richieste siano abbastanza granulari da poter fallire una alla volta senza portarsi dietro
l'intera schermata.

---

## 68. Errore

### 1. Nome e scopo

Cosa vede l'utente quando qualcosa non riesce, e come ne esce.

### 2. Il principio

**Un errore non è mai un vicolo cieco.** Ogni stato d'errore dice tre cose, nell'ordine: cosa non
è riuscito, cosa *non* è successo, e come riprovare.

La seconda è la meno ovvia e la più importante in un'app che custodisce le fotografie di qualcuno.
`"Non è stato possibile caricare la libreria"` da solo lascia aperta la domanda peggiore. La frase
che segue la chiude: *"Le foto sono al sicuro: è la lettura ad essere fallita, non i file."*

Non si usa mai "qualcosa è andato storto".

### 3. Ogni forma di errore, una per una

**Errore a piena vista.** Quando è mancato il contenuto principale della schermata. Impaginato
come lo stato vuoto — così i due si leggono come parenti — ma con l'icona di avviso in colore di
pericolo. Contiene: icona, titolo che nomina cosa è fallito, spiegazione con la rassicurazione, e
un pulsante `"Riprova"`. Facoltativamente una riga di dettaglio tecnico, in carattere monospaziato
e attenuata, per chi amministra il server.

**Errore in riga.** Quando manca solo un pezzo e il resto della pagina è arrivato. Un riquadro
orizzontale con fondo tenue di pericolo, icona, messaggio e un `"Riprova"` compatto. Non prende
tutta la vista — sarebbe sproporzionato — ma non lascia nemmeno passare la cosa sotto silenzio.

**Messaggio temporaneo d'errore.** Quando a fallire è stata un'azione, non un caricamento. Il
messaggio (**SP-6**) prende un filetto colorato a sinistra, resta a schermo **4,2 s** invece di
2,4, ed è annunciato come avviso. Se porta un `"Riprova"` resta **6,5 s**, e il timer si ferma
mentre il puntatore è sopra: non sparisce mentre ci si sta ragionando.

### 4. Interazioni da mouse

`"Riprova"` rimette l'insieme di dati in caricamento e lo richiede da capo. Nessun menu
contestuale, nessun doppio click.

### 5. Interazioni da tastiera

`"Riprova"` è raggiungibile con Tab e si attiva con Invio o Spazio (**SP-8**). L'azione dentro un
messaggio temporaneo è a sua volta raggiungibile e attivabile.

### 6. Animazioni e transizioni

Lo stato d'errore compare senza animazione, deliberatamente: un errore che scivola dentro con
grazia sembra meno serio di quello che è.

### 7. Stati

Il pulsante `"Riprova"` ha gli stati normali di un pulsante. Non ha uno stato disabilitato:
riprovare è sempre permesso.

### 8. Da dove ci si arriva e dove si va

Ci si arriva da qualunque schermata la cui richiesta non sia riuscita. Da lì si esce in due modi:
riprovando, oppure navigando altrove — la barra di navigazione resta sempre operativa, perché un
errore in una schermata non deve intrappolare l'utente in quella schermata.

### 9. Dati necessari

Serve poter distinguere **la natura** del fallimento, perché il messaggio cambia: server
irraggiungibile, cartella non leggibile per permessi, file mancante, tempo scaduto. Il prototipo
ne distingue una sola; il prodotto vero dovrebbe distinguerne almeno queste quattro, perché
"riprova" ha senso in due casi e non negli altri due.

---

## 69. Riuscita parziale

### 1. Nome e scopo

L'esito di un'azione di massa che è riuscita su una parte delle foto e non su tutte.

### 2. Perché ha una sezione sua

Perché su un'operazione che tocca centinaia di file **è l'esito più probabile**, ed è quasi sempre
quello che nessuno disegna. Un file bloccato da un altro programma, un permesso mancante su una
sottocartella, un disco che non risponde per qualche scatto: basta uno di questi e l'operazione
non è né riuscita né fallita.

Le due scorciatoie abituali sono entrambe bugie. Dichiarare successo nasconde all'utente che 183
foto non sono state toccate. Dichiarare errore gli fa credere di dover rifare tutto, quando il
94% è già a posto.

### 3. Come si presenta

Un messaggio temporaneo con un filetto di colore proprio — né quello dell'errore né quello neutro
del successo — che dice esattamente i numeri:

> **731 su 914 completate — 183 non sono riuscite.**  `Riprova le 183 rimaste`

L'azione ritenta **solo quelle rimaste indietro**, non l'intera operazione. È la differenza fra
un secondo tentativo da 183 elementi e uno da 914.

### 4. Interazioni da mouse

Il messaggio non sparisce mentre il puntatore è sopra: il timer si ferma e riparte all'uscita.
Senza questo, un messaggio che offre una scelta sparirebbe proprio mentre la si sta valutando.

### 5. Interazioni da tastiera

L'azione è raggiungibile con Tab e si attiva con Invio o Spazio.

### 6. Animazioni e transizioni

Le stesse del messaggio temporaneo normale (**SP-6**), con la durata di vita più lunga descritta
sopra.

### 7. Stati

Il **secondo tentativo riesce sempre**, per scelta: un errore che si ripete identico all'infinito
non è uno stato, è un vicolo cieco. Nel prodotto vero questa garanzia non esiste, e va deciso cosa
fare al secondo fallimento — verosimilmente passare all'errore pieno con il dettaglio tecnico.

### 8. Dove si applica

A tutte le azioni su più foto: preferiti dalla barra di selezione, modifica in blocco,
e per estensione ogni operazione di massa futura.

### 9. Dati necessari

Questa è la richiesta al backend che vale la pena leggere due volte.

**Un'operazione di massa non può rispondere "fatto" o "non fatto".** Deve rispondere con
**l'elenco di cosa è riuscito e l'elenco di cosa no, foto per foto**, e per ciascun fallimento una
ragione. Senza questo l'interfaccia non può né dire i numeri veri né offrire di ritentare solo le
rimanenti, e ricadrebbe inevitabilmente in una delle due bugie descritte sopra.

È una richiesta che ha conseguenze sulla forma delle risposte e forse sulle transazioni: meglio
saperlo adesso.

---

## 70. Il pannello "Anteprima stati"

### 1. Nome e scopo

Una sezione in fondo a **Impostazioni** che accende a comando gli stati di caricamento, errore e
riuscita parziale, perché altrimenti nel prototipo non si verificherebbero mai.

### 2. Perché esiste, e perché va tolto

Gli stati descritti in questa parte dipendono da una rete e da un server veri. Il prototipo ha i
dati già in memoria: ogni operazione riesce, e riesce istantaneamente. Senza una simulazione,
questi stati sarebbero descritti a parole ma **non rivedibili** — e uno stato d'errore che nessuno
ha mai guardato a schermo è uno stato che non è stato davvero disegnato.

Il pannello è **scaffolding del prototipo, non prodotto**. Si distingue apposta da tutto il resto
delle Impostazioni — bordo tratteggiato, fondo diverso, icona da laboratorio — perché non lo si
scambi mai per una preferenza vera. Nel prodotto finito non esiste.

### 3. I tre interruttori

| Interruttore | Cosa simula |
|---|---|
| `"Rete lenta"` | Ogni schermata impiega **1,1 s** a caricare i suoi dati, mostrando intanto lo scheletro. |
| `"Errore di caricamento"` | Le richieste falliscono invece di completarsi: compare lo stato d'errore a piena vista con il suo `"Riprova"`. |
| `"Esito parziale"` | Le azioni su più foto riescono solo sull'80% circa, e compare il messaggio di riuscita parziale. |

Sono indipendenti e combinabili. Tutti spenti — l'impostazione predefinita — l'app si comporta
esattamente come se il pannello non esistesse: **la simulazione non lascia residui sul percorso
normale**, ed è una proprietà voluta, non un caso.

Cambiando un interruttore, le viste già caricate vengono dimenticate: altrimenti accendere
l'errore non avrebbe effetto su ciò che è già a schermo.

### 4-8. Interazioni, animazioni, stati, navigazione

Sono quelli di una normale sezione di impostazioni (**SP-23**) con tre interruttori (**SP-24**
per la forma, attivabili da tastiera secondo **SP-8**). Non c'è nulla di specifico da documentare:
è deliberatamente un pannello banale.

### 9. Dati necessari

Nessuno. Vive interamente nel prototipo.

**Nota per chi implementa:** la macchina a stati che c'è dietro — un insieme di dati che passa fra
"in caricamento", "pronto" ed "errore" — è invece esattamente quella che serve nel prodotto vero.
Cambia solo chi la fa avanzare: la risposta del server invece di un timer. Il pannello si toglie,
la macchina resta.

---

# Parte XI — Pattern condivisi fra più schermate

Questa sezione raccoglie i comportamenti che si ripetono **identici** in più punti
dell'applicazione. Ognuno ha un codice (**SP-n**) con cui è richiamato nelle sezioni delle
singole schermate.

La regola di lettura è: se una schermata dice "SP-2", vale tutto quanto scritto qui sotto per
SP-2, senza eccezioni; se dice "SP-2, ma …", vale tutto tranne quello che l'eccezione dichiara.
Le deviazioni sono documentate nella schermata, non qui.

Per lo sviluppatore frontend questa sezione è, in pratica, l'elenco dei **componenti condivisi**
da costruire per primi: quasi ogni voce corrisponde a un componente riutilizzabile, e costruirli
prima delle schermate evita che dodici viste divergano ognuna per conto suo. Per l'architetto è
la parte più corta ma più densa: un pattern che cambia si ripercuote su tutte le schermate che lo
usano.

---

## SP-1 · Tile foto in griglia

La miniatura fotografica come compare in tutte le viste a griglia (Foto, Preferiti, dettaglio
Album, dettaglio Persona, risultati di Cerca).

**Definizione canonica:** sezione *Il tile fotografico*.

In sintesi: rettangolo con la miniatura, badge del formato in alto a sinistra (**SP-15**), casella
di selezione in alto a sinistra quando la selezione è attiva, cuoricino dei preferiti in alto a
destra che compare al passaggio del mouse o quando la foto è già preferita. Il tile intero è
apribile; il cuoricino e la casella intercettano il click prima che arrivi al tile.

Usato in: Foto, Preferiti, Album (dettaglio), Persone (dettaglio), Cerca (risultati), Cestino,
Duplicati.

---

## SP-2 · Selezione multipla e barra azioni

Quando almeno una foto è selezionata, la toolbar della vista è **sostituita in posto** da una
barra contestuale che dichiara quante foto sono selezionate e offre le azioni di gruppo.

**Definizione canonica:** sezione *Selezione multipla e barra azioni*.

In sintesi: a sinistra una X per annullare la selezione, il conteggio `"N selezionate"` e il
collegamento `"Seleziona tutte"`; a destra cinque comandi a icona con tooltip — `"Preferiti"`,
`"Album"`, `"Condividi"`, `"Modifica"`, `"Elimina"`.

**Attenzione — esistono due pool di selezione distinti e paralleli**, con la stessa forma
visiva ma insiemi di azioni diversi:

- la selezione **della libreria**, usata da tutte le viste a griglia;
- la selezione **del lotto di culling**, con i propri comandi (`"Scelta"`, `"Scarta"`,
  `"Rinomina…"`) e senza Album/Elimina.

Non si parlano e non si azzerano a vicenda. Vedi le domande aperte: il fatto che la selezione
della libreria sopravviva al cambio di vista è un comportamento probabilmente non voluto.

---

## SP-3 · Filtro rapido a chip

Il pannello a imbuto che assottiglia ciò che la griglia mostra, senza lasciare la vista.

**Definizione canonica:** sezione *Filtro rapido a chip*.

In sintesi: un pulsante a imbuto nella toolbar con il numero di filtri attivi; il pannello elenca
le dimensioni filtrabili come file di chip; la logica è **OR dentro la stessa dimensione, AND fra
dimensioni diverse**; le dimensioni lunghe (Tag, Persone) hanno un campo di ricerca proprio.

Distinguerlo dalla pagina **Cerca**, che è una schermata a sé con un modello diverso (le
"pillole", **SP-19**). Il filtro rapido restringe ciò che stai già guardando; Cerca interroga
l'intera libreria.

---

## SP-4 · "Seleziona tutto quello che vedi"

Un comando a icona nella toolbar delle viste a griglia, con tooltip `"Seleziona tutto"` ed
etichetta di accessibilità `"Seleziona tutto quello che vedi"`.

Seleziona **esattamente ciò che è visibile in quel momento**, non l'intera libreria sottostante:
se un filtro rapido o una ricerca sono attivi, seleziona solo ciò che ci ricade dentro. È la
distinzione fra i due insiemi che l'implementazione deve tenere separati: l'insieme *di partenza*
della vista (che serve al pannello dei filtri per mostrare quante foto corrisponderebbero) e
l'insieme *effettivamente mostrato* (che è quello che questo comando seleziona).

Il comando **scompare** quando non c'è nulla da selezionare, invece di essere disabilitato.

Presente in: Foto, Preferiti, dettaglio Album, dettaglio Persona, e — con logica identica ma pool
di selezione proprio — nel lotto di culling aperto, dove rispetta il filtro del lotto attivo.

---

## SP-5 · Dialog modale standard

Ogni finestra di dialogo dell'applicazione ha la stessa impalcatura.

**Comportamento:**

- Un velo scuro copre l'intera area dell'app; la scheda del dialog è centrata.
- La scheda è annunciata come finestra di dialogo modale con il proprio titolo.
- **Escape chiude** il dialog, annullando (nessuna modifica applicata).
- All'apertura il **focus va al primo elemento interattivo** della scheda.
- Alla chiusura il **focus torna all'elemento che aveva aperto il dialog**. Questo è implementato
  in modo uniforme in tutta l'app ed è un dettaglio da non perdere nella riscrittura.
- La scheda ha quasi sempre: un **titolo**, un **sottotitolo esplicativo** che dice cosa sta per
  succedere, il corpo, e in fondo un `"Annulla"` più l'azione di conferma.

**Due limiti noti e uniformi, da correggere nel frontend reale** (validi per *tutti* i dialog):

- **Il click sul velo non chiude il dialog.** Va deciso se è voluto: per i dialog distruttivi
  probabilmente sì, per i selettori probabilmente no.
- **Il focus non è confinato nella scheda**: premendo Tab abbastanza volte si esce dal dialog e si
  finisce sui controlli della pagina sottostante, che resta operabile.

---

## SP-6 · Toast

Messaggio di conferma temporaneo, centrato in basso sull'area dell'app.

- Compare con una dissolvenza di **0,2 s** accompagnata da una risalita di 10 px.
- Non si accumulano in pila: messaggi successivi si sovrappongono nello stesso punto.

Ha **tre nature**, e la differenza non è solo di colore:

| Natura | Filetto | Durata | Azione |
|---|---|---|---|
| **Successo** | nessuno | 2,4 s | mai |
| **Errore** | colore di pericolo a sinistra | 4,2 s | `"Riprova"`, facoltativa |
| **Riuscita parziale** | colore di avviso a sinistra | 4,2 s | `"Riprova le N rimaste"` |

Il successo resta **neutro di proposito**: è il caso normale e non merita né colore né attenzione.
Errore e riuscita parziale durano di più, perché leggere *cosa non ha funzionato* richiede più
tempo che leggere *fatto*, e sono annunciati agli assistenti vocali come avvisi.

Un messaggio **con azione** resta **6,5 s** e il suo timer **si ferma mentre il puntatore è
sopra**: senza questo, sparirebbe proprio mentre si sta valutando se premerlo. È l'unico caso in
cui un messaggio temporaneo è interattivo, e l'azione è raggiungibile da tastiera.

Vedi **SP-28** e **SP-29** per quando usare le due varianti non neutre.

---

## SP-7 · Tooltip

I comandi ridotti a sola icona portano un'etichetta testuale che compare al passaggio del mouse o
quando ricevono il focus da tastiera.

- Si posiziona **sopra** il comando, centrata.
- Compare in **0,12 s** con una risalita di 3 px. **Non c'è ritardo di apertura**: appare
  immediatamente all'ingresso del puntatore.
- È inerte al puntatore (non intercetta il mouse).
- **Su mobile è disattivata**, perché non esiste il passaggio del mouse: lì l'icona deve bastare
  da sola. Questo è un punto di attenzione — diversi comandi a sola icona su mobile restano
  quindi privi di etichetta visibile.

L'etichetta di accessibilità è sempre presente e spesso **più esplicita** del tooltip: il tooltip
dice `"Seleziona tutto"`, l'etichetta per gli assistenti vocali dice
`"Seleziona tutto quello che vedi"`.

---

## SP-8 · Attivabile da tastiera

I comandi dell'app non sono elementi di modulo nativi ma elementi generici resi interattivi. Il
pattern uniforme è: **Invio e Barra spaziatrice fanno esattamente quello che fa il click**, e la
barra spaziatrice non fa scorrere la pagina.

Questo è applicato in modo **disomogeneo** nel prototipo: molti comandi lo hanno, molti altri no,
e alcuni lo hanno senza però essere raggiungibili con Tab. Vedi le domande aperte — è la lacuna
più diffusa dell'intero disegno.

**Per il frontend reale la raccomandazione è semplice e vale ovunque:** ogni cosa cliccabile deve
essere un pulsante vero, raggiungibile con Tab, attivabile con Invio e Spazio, con un anello di
focus visibile. Il prototipo mostra *cosa* deve fare ogni comando; non è un modello di *come*
esporlo.

---

## SP-9 · Valutazione a stelle

Fila di cinque stelle, usata sia nel pannello informazioni sia nelle liste.

- Cliccare la stella *n* imposta la valutazione a *n*.
- **Cliccare la stella già attiva azzera la valutazione** (riporta a nessuna stella). È la regola
  di reversibilità che si ritrova in tutta l'app — vedi **SP-20**.
- Le stelle piene usano il colore d'accento; quelle vuote il grigio terziario.
- Il click sulla stella non apre la foto: si ferma prima.

Da tastiera, i tasti **1–5** fanno la stessa cosa nel culling, con la stessa regola di
azzeramento se si ripreme il numero già impostato.

---

## SP-10 · Coda di conferma dei suggerimenti automatici

Il modo uniforme in cui Keeppix chiede all'utente di validare ciò che il riconoscimento
automatico ha proposto, invece di applicarlo di nascosto.

**Definizione canonica:** sezione *Revisione — tag*.

Il principio, che vale sia per i tag sia per i volti: i suggerimenti **non entrano nella libreria
finché non sono confermati**, restano in una coda dedicata, e ogni voce si accetta o si rifiuta.
Il numero di elementi in attesa è visibile come badge nella navigazione. La stessa forma è usata
in due punti (Revisione → tag, Revisione → volti) con contenuto diverso.

---

## SP-11 · Livelli di analisi automatica

L'analisi automatica non è un interruttore acceso/spento ma ha tre livelli — `"Pieno"`,
`"Ridotto"`, `"Spento"` — che cambiano *quanto* lavoro il sistema fa e, di conseguenza, quali
parti dell'interfaccia hanno senso.

**Definizione canonica:** sezione *I livelli di analisi*.

Per l'architetto è il pattern con più conseguenze fuori dall'interfaccia: il livello scelto
determina quali elaborazioni girano, e l'interfaccia deve degradare in modo coerente — nascondendo
ciò che non ha più senso invece di mostrarlo vuoto.

---

## SP-12 · Provenienza automatica vs umana

Un'etichetta proposta dal riconoscimento e una messa da una persona **non sono mai indistinguibili**
nell'interfaccia, in nessun punto dell'app.

**Definizione canonica:** sezione *Provenienza automatica vs umana*.

È un principio di prodotto prima che un dettaglio visivo: l'utente deve poter sempre rispondere
alla domanda "questo l'ho deciso io o l'ha indovinato il computer?". Ha una conseguenza diretta
sul modello dati — la provenienza va **conservata** per ogni assegnazione, non dedotta.

---

## SP-13 · Frecce ← → per navigare

Le frecce sinistra e destra spostano l'elemento corrente in avanti e indietro dentro una sequenza.

- Nel **lightbox**: passa alla foto precedente/successiva.
- Nel **lotto di culling**: sposta la foto corrente nella coda.

In entrambi i casi la navigazione **si ferma agli estremi** senza tornare in cerchio.

Nel culling la freccia ha una regola in più: se c'è una selezione multipla attiva, **la freccia
semplice la azzera** prima di spostarsi, mentre **Shift+freccia estende la selezione** (vedi
**SP-21**).

Le frecce **non** navigano dentro le griglie fotografiche né dentro gli elenchi a scelta multipla:
in quei punti non sono implementate, pur essendoci l'annuncio di accessibilità che le
prometterebbe. È una delle lacune segnalate nelle domande aperte.

---

## SP-14 · Menu a comparsa

I menu che si aprono sotto un comando (menu account, menu "altre azioni" del lightbox, selettore
di lotto, pannello dei filtri).

- Si aprono al click sul comando che li ancora.
- **Un click in qualunque altro punto della pagina li chiude**, senza attivare ciò che c'era sotto.
- Un click **dentro** il menu non lo chiude, tranne che sulla voce scelta.
- **Escape li chiude** — ma solo per alcuni: i menu account non lo implementano, e nel culling il
  selettore di lotto nemmeno. Deviazione segnalata nelle rispettive schermate.
- Quando un menu è dentro un contesto che a sua volta risponde a Escape (il menu ⋯ dentro il
  lightbox), **Escape agisce a livelli**: la prima pressione chiude solo il menu, la seconda
  chiude il contesto. È il comportamento corretto e va replicato ovunque ci siano livelli
  annidati.

---

## SP-15 · Badge del formato (RAW / RAW+JPEG)

Etichetta in alto a sinistra sulla miniatura, che dichiara come lo scatto è archiviato:

- `"RAW"` — esiste solo il file negativo;
- `"RAW+JPEG"` — negativo e JPEG affiancati, gestiti come **un unico scatto** e non come due foto;
- nessun badge — solo JPEG.

Il badge è puramente informativo: non è cliccabile e non ha stati. La distinzione fra i due file
di una coppia diventa operativa nel lightbox, dove si può scegliere quale dei due guardare, e
nella rinomina.

Per l'architetto è uno dei punti in cui il modello dati dell'interfaccia è più esigente: **una
foto è una pila di file**, non un file.

---

## SP-16 · Avatar

Cerchio con le iniziali della persona su fondo colorato, usato per l'utente corrente e per i
collaboratori nelle condivisioni.

- Le iniziali sono **sempre bianche**, qualunque sia il colore di fondo. È una scelta deliberata:
  l'avatar è trattato come elemento di marca, non come testo da leggere a lungo.
- Il colore di fondo dell'avatar **dell'utente corrente** è una preferenza personale scelta in
  Profilo (predefinito: il colore d'accento). Cambiandolo si aggiorna ovunque compaia — piede
  della sidebar, header mobile, pagina Profilo.
- Il colore degli **altri** utenti è assegnato per contatto e non è modificabile qui.

Compare in tre punti fissi più le liste di condivisione. Nel frontend reale è un componente
banale; nel prototipo richiede sincronizzazione manuale perché parte del guscio non è
rigenerata dai render — è esattamente il genere di dettaglio che sparisce passando a Vue.

---

## SP-17 · Shell mobile

Sotto una certa larghezza l'app cambia impalcatura invece di comprimere quella desktop.

- La **sidebar sparisce**, sostituita da una **barra di schede in basso**.
- Compare un **header** con il titolo della schermata corrente e, dove ha senso, una freccia
  indietro.
- L'ultima scheda, `"Altro"`, apre una pagina di elenco piatto con tutte le sezioni che non
  entrano nella barra.
- I **tooltip sono disattivati** (**SP-7**) e i pannelli di filtro diventano fogli che salgono
  dal basso, con un velo dietro.
- Le griglie passano dal layout giustificato a **colonne fisse con tessere quadrate**.

**Definizione canonica:** sezioni *Shell mobile* e *Pagina "Altro"*.

---

## SP-18 · Dialog di eliminazione a tre opzioni

Il pattern più importante dell'intera applicazione per chi lavora sul backend, e l'unico che vale
la pena leggere due volte.

**Keeppix non ha mai un comportamento predefinito implicito quando si elimina qualcosa.** Ogni
azione distruttiva apre un dialog che chiede **a cosa** applicarsi, con tre possibilità
esplicite e conseguenze diversissime:

| Opzione | Cosa fa | Reversibile |
|---|---|---|
| `"Rimuovi solo dall'indice"` | Il file resta sul disco; Keeppix se ne dimentica, e lo re-indicizzerà alla prossima scansione della cartella | Sì, di fatto automaticamente |
| `"Sposta nel cestino di Keeppix"` | Spostato in una cartella nascosta dentro la stessa libreria, recuperabile per 30 giorni | Sì, entro 30 giorni |
| `"Elimina dal disco adesso"` | Il file è cancellato definitivamente | **No** |

Il sottotitolo del dialog dichiara il principio a parole:
`"Keeppix chiede sempre come procedere — non c'è un comportamento predefinito implicito."`

**Definizione canonica ed effetti esatti:** sezione *Dialog di eliminazione a tre opzioni*.

Lo stesso dialog è usato per una foto singola, per una selezione multipla e dalle pagine di
manutenzione, cambiando solo il titolo. La terza opzione è marcata visivamente come pericolosa.

Nota importante: dentro un **lotto di culling**, `"Scarta"` **non** apre questo dialog — è uno
spostamento fisico immediato dentro il lotto, che è un'operazione diversa e reversibile. Vedi la
sezione del culling, dove la distinzione è spiegata per esteso.

---

## SP-19 · Pillole di ricerca

Il modello della pagina **Cerca**, distinto dal filtro rapido (**SP-3**): i criteri scelti
diventano "pillole" dentro la barra di ricerca, componibili fra loro, ciascuna rimovibile
singolarmente.

**Definizione canonica:** sezione *Cerca — i filtri strutturati*.

---

## SP-20 · La decisione si annulla ripetendola

Regola di reversibilità che attraversa tutta l'app: **riapplicare un valore già impostato lo
azzera**.

- Cliccare la stella già attiva → nessuna stella (**SP-9**).
- Premere `1`–`5` sul valore già impostato → azzera.
- `"Scelta"` su una foto già scelta → torna da valutare.
- Il cuoricino su una foto già preferita → la toglie dai preferiti.
- `"Seleziona tutte"` quando sono già tutte selezionate → le deseleziona tutte.

È una regola che l'utente impara una volta e riusa ovunque, e va preservata. **Attenzione**: nei
comandi che si comportano da interruttore di gruppo l'etichetta **non cambia** quando il comando
sta per fare l'opposto (`"Seleziona tutte"` continua a dire così anche quando deseleziona) — è un
difetto segnalato nelle domande aperte.

---

## SP-21 · Selezione a intervallo con ancoraggio

Il comportamento di selezione multipla familiare dai gestori di file, implementato nel culling e
da estendere alle griglie.

- Un click semplice sposta l'elemento corrente e **azzera** la selezione.
- **Shift+click** seleziona l'intervallo dall'**ancora** all'elemento cliccato.
- **Shift+freccia** estende l'intervallo di un elemento per volta, mantenendo l'ancora.
- L'ancora è fissata dal primo click semplice e resta finché la selezione non viene azzerata.
- L'intervallo è **ricalcolato**, non accumulato: allargare e poi restringere con Shift riduce
  davvero la selezione invece di lasciare residui.

---

## SP-22 · Griglia giustificata e virtualizzata

Tutte le griglie fotografiche desktop usano righe ad **altezza comune** e larghezza proporzionale
al lato lungo dello scatto, come una pagina di provini: gli scatti orizzontali occupano più
spazio dei verticali, e ogni riga riempie esattamente la larghezza disponibile.

- La scala di ogni riga è limitata per non gonfiare né schiacciare troppo le foto.
- **L'ultima riga non viene stirata**: resta con le tessere alla loro altezza naturale.
- Su mobile il layout è invece a **colonne fisse con tessere quadrate**.

La geometria è calcolata **in anticipo e sui soli rapporti d'aspetto**, senza misurare nulla a
schermo, e questo è ciò che permette di conoscere l'altezza dell'intera libreria prima di averne
disegnato un solo pixel — e quindi di montare solo le righe visibili.

**Definizione completa:** Parte X, *La timeline a scala reale*. È il calcolo più delicato che il
frontend reale dovrà rifare, e va rieseguito a ogni cambio di larghezza, densità o contenuto.

---

## SP-27 · Scheletro di caricamento

Il caricamento non è mai uno spinner al centro del vuoto: è uno **scheletro con la forma del
contenuto in arrivo**, così quando i dati atterrano prendono il suo posto senza che il layout
salti.

**Definizione canonica:** Parte X, *Caricamento*.

---

## SP-28 · Stato d'errore

Ogni errore dice tre cose in quest'ordine: **cosa non è riuscito**, **cosa non è successo** (i
file sono intatti), **come riprovare**. Mai "qualcosa è andato storto", mai un vicolo cieco.
Esiste in tre forme — a piena vista, in riga, come messaggio temporaneo.

**Definizione canonica:** Parte X, *Errore*.

---

## SP-29 · Riuscita parziale

Un'azione di massa riuscita solo in parte non è né un successo né un errore: ha un esito proprio,
che dichiara i numeri veri e offre di **ritentare solo ciò che è rimasto indietro**.

**Definizione canonica:** Parte X, *Riuscita parziale*. Ha conseguenze dirette sulla forma delle
risposte del backend.

---

## SP-30 · Pulsante occupato

Un comando che avvia un'operazione non istantanea passa in stato **occupato**: mantiene la sua
dimensione, smette di rispondere al click e affianca all'etichetta un indicatore rotante. Serve
insieme a dire "sto lavorando" e a **impedire il doppio invio** — che su un'azione di massa è il
modo più facile per duplicare un'operazione.

---

## SP-23 · Sezione di impostazioni

La forma con cui sono composte tutte le pagine di preferenze: un **titolo di sezione**, un
**sottotitolo** che spiega a cosa serve il gruppo, e una serie di righe con etichetta e
sottotitolo a sinistra, controllo a destra.

Vale anche la regola di salvataggio: **non esiste "Salva"** in nessuna pagina di preferenze. Ogni
modifica è applicata immediatamente, senza conferma e senza annullamento. L'unica eccezione è la
sezione "Dati account" del Profilo, che ha un pulsante `"Salva modifiche"`.

---

## SP-24 · Controllo segmentato

Gruppo di due o più opzioni mutuamente esclusive, affiancate in un unico contenitore, con
l'opzione attiva evidenziata. Usato per il tema, per i livelli di analisi, per le schede della
Revisione, per i filtri della modifica in blocco e per il passaggio desktop/mobile.

Nei filtri della modifica in blocco include sempre un'opzione neutra `"Non modificare"` come
valore di partenza, per distinguere "lascia com'è" da "imposta a vuoto" — distinzione che il
backend deve poter ricevere.

**Nel prototipo il livello di accessibilità di questo controllo cambia da caso a caso**: va
uniformato una volta sola nel componente condiviso.

---

## SP-25 · Gruppo di navigazione a scomparsa

Le voci `"Manutenzione"` e `"IA"` della sidebar raggruppano più sezioni sotto un'unica riga
apribile, con una freccia che ruota di 180° in **0,15 s** e sotto-voci rientrate e più piccole.

Il gruppo si apre da solo — e non si può chiudere — quando la sezione corrente sta al suo
interno. È stato introdotto per evitare che la sidebar traboccasse in una scrollbar interna.

---

## SP-26 · Indicatore di posizione

In tutta la navigazione, "sei qui" è comunicato da un **bordino verticale a sinistra** nel colore
d'accento più un fondo tenue. Il colore d'accento pieno è riservato ai **badge numerici**: non è
mai usato come sfondo di una voce selezionata.

Quando la sezione attiva è dentro un gruppo chiuso, la riga del gruppo lo segnala solo con il
**peso del carattere**, senza bordino né sfondo — così il livello attivo resta uno solo.

---

# Parte XII — Assunzioni e domande aperte

Questa è la sezione da leggere per prima, non per ultima.

Contiene tre cose tenute distinte apposta: le **assunzioni** prese per poter scrivere il
documento, i **comportamenti probabilmente non voluti** trovati nel prototipo, e le **decisioni
ancora aperte**. Le prime sono responsabilità di chi scrive, le seconde sono difetti da
correggere, le terze richiedono che qualcuno decida.

Nessuna di queste voci è stata inventata per completezza: sono tutte punti realmente incontrati
nel prototipo.

---

## Come è stato scritto questo documento, e cosa questo implica

Il documento è stato estratto **dal codice del prototipo**, riga per riga, non dalla memoria delle
conversazioni in cui il prototipo è stato disegnato. Ogni etichetta, durata, scorciatoia e stato
disabilitato qui riportato è stato letto nel sorgente.

Questo ha due conseguenze da tenere presenti.

**A favore:** il documento non contiene funzionalità immaginate. Dove il prototipo non fa una
cosa, il documento dice che non la fa, invece di descrivere l'intenzione. Le durate delle
animazioni sono quelle vere.

**A sfavore:** il codice conserva le *decisioni*, non sempre le *ragioni*. Dove una motivazione era
scritta in un commento è stata riportata (e sono molte, in italiano, spesso illuminanti). Ma le
alternative discusse e scartate lungo il percorso, i vincoli che hanno portato a una scelta e le
opzioni valutate e respinte **non sono in questo documento** se non sono finite in un commento.
Se una scelta qui descritta sembra arbitraria, è possibile che avesse una ragione precisa che il
codice non ha conservato: vale la pena chiedere prima di ribaltarla.

---

## Lacune sistematiche — valgono per tutte le schermate

Erano quattro. **Due sono state chiuse** e restano qui, in forma abbreviata, perché il confronto
fra prima e dopo è a sua volta un'informazione utile; le altre due sono ancora aperte.

### 2.1 ~~Non esiste alcuno stato di caricamento~~ — risolto, ma non ovunque

Chiuso. Gli stati di caricamento, errore e riuscita parziale sono ora disegnati, prototipati e
documentati per intero nella **Parte X**, insieme al pannello che permette di rivederli.

**Cosa resta aperto:** lo scheletro di caricamento copre le griglie fotografiche (timeline,
preferiti, album, persona), che sono il caso pesante. **Le altre schermate non hanno ancora uno
scheletro dedicato** — Persone, Cerca, le pagine di manutenzione, la Revisione. Il meccanismo
c'è ed è generico; va esteso, decidendo per ognuna quale forma abbia il suo scheletro.

Restano inoltre senza stato di avanzamento le **operazioni lunghe sul disco**: rinomina di massa,
spostamenti, scansioni. Lì non basta uno scheletro — serve un avanzamento con una percentuale, e
probabilmente la possibilità di annullare a metà. Non è disegnato.

### 2.2 ~~Non esiste alcuno stato di errore~~ — risolto nel principio, da estendere nei dettagli

Chiuso nel principio: l'errore ha tre forme (piena vista, in riga, messaggio temporaneo), dice
sempre cosa non è riuscito e cosa *non* è successo, e non è mai un vicolo cieco. Vedi **SP-28**.

**Cosa resta aperto:** il prototipo distingue **un solo tipo** di fallimento. Il prodotto vero ne
dovrà distinguere almeno quattro — server irraggiungibile, permessi mancanti, file assente, tempo
scaduto — perché `"Riprova"` ha senso nei primi due e non negli altri due, e il messaggio giusto
cambia in ognuno.

Va inoltre deciso **cosa fare al secondo fallimento consecutivo**: nel prototipo il ritentativo
riesce sempre, per non creare un vicolo cieco durante la revisione del disegno.

### 2.3 L'accessibilità da tastiera è incompleta e disomogenea

I comandi dell'app non sono pulsanti nativi ma elementi generici resi interattivi. Il risultato è
che nel prototipo convivono tre livelli diversi, senza un criterio riconoscibile:

- comandi completi — raggiungibili con Tab, attivabili con Invio e Spazio, annunciati
  correttamente;
- comandi attivabili da tastiera ma **non raggiungibili con Tab**, quindi di fatto inutilizzabili;
- comandi solo cliccabili.

Interi blocchi ricadono nell'ultima categoria: quasi tutta la barra di navigazione, quasi tutti i
comandi del culling (schede dei lotti, chip dei filtri, frecce, `"Scelta"`, `"Scarta"`), il
selettore del tema, gli interruttori delle notifiche, la barra dei mesi della timeline.

Ci sono inoltre **promesse non mantenute**: alcuni gruppi di opzioni si annunciano come navigabili
con le frecce, ma le frecce non sono implementate; e diverse regole di stile per l'anello di focus
esistono su elementi che il focus non può raggiungere.

**Raccomandazione:** non replicare questa parte. Costruire i componenti condivisi (**SP-8**) con
elementi nativi e accessibilità corretta *una volta sola* risolve l'intero problema alla radice.
Il prototipo dice cosa deve fare ogni comando; non è un modello di come esporlo.

### 2.4 Non esiste routing: nessuna schermata è indirizzabile

Lo stato della vista è una variabile in memoria. Conseguenze: **il tasto "indietro" del browser non
funziona**, nessuna schermata ha un indirizzo condivisibile, ricaricando la pagina si torna alla
timeline, e la posizione di scorrimento non è mai conservata passando da una vista all'altra e
tornando indietro.

Questa è una **decisione da prendere consapevolmente**, non da ereditare. Vale almeno per: quale
sezione è aperta, quale cartella o album o persona, quale lotto di culling, quale foto aperta nel
lightbox, e verosimilmente i filtri attivi e la ricerca in corso. Il caso "mando a un collega il
link a questa foto" è quello che decide la risposta.

---

## Comportamenti probabilmente non voluti

Difetti trovati leggendo il prototipo. Sono elencati perché **non vanno replicati**, e alcuni
nascondono una domanda di disegno.

### 3.1 Ambito e persistenza dei filtri e delle selezioni

- **Il filtro rapido è globale e sopravvive ai cambi di vista.** Viene azzerato solo cliccando una
  voce della barra di navigazione. Aprendo una cartella dall'elenco Cartelle, o un album dalla
  griglia Album, un filtro impostato prima **resta attivo e assottiglia silenziosamente** la nuova
  vista, senza che sia ovvio perché mancano delle foto. Va deciso l'ambito: per vista, o globale
  ma reso molto più visibile.
- **La selezione multipla sopravvive al cambio di vista.** Si possono selezionare foto in
  Preferiti e ritrovarsi la barra "N selezionate" in Foto, con dentro foto che non si stanno
  guardando.
- **Nel culling, cambiare filtro non azzera la selezione.** Se la nuova coda è vuota la barra
  sparisce ma la selezione resta viva e invisibile — e un'azione successiva la userebbe.
- **Il lotto di culling aperto non viene mai chiuso**: rientrando nella sezione si riapre l'ultimo
  lotto invece della griglia dei lotti. Potrebbe essere voluto (riprendi da dove eri): va deciso.

### 3.2 Il gestore globale della tastiera non guarda dove sei

Con un lotto di culling aperto e un dialog davanti, **digitare in un campo di testo aziona le
scorciatoie del culling**: scrivere `1` in un campo cambia la valutazione della foto sottostante,
`p` e `x` la scelgono o la scartano, le frecce navigano la coda.

Solo il lightbox è schermato correttamente. È un difetto vero e va risolto alla radice nel
frontend reale: le scorciatoie non devono attivarsi quando il focus è in un campo di testo o
quando una finestra modale è aperta.

### 3.3 Etichette e conteggi che non dicono il vero

- ~~La timeline mostra al massimo 24 foto per mese~~ — **risolto**: nessun tetto, timeline
  completa e virtualizzata (**Parte X**). Resta però una richiesta precisa al backend, che è la
  più impegnativa di tutto il documento: **poter conoscere le proporzioni di tutti gli scatti di
  una vista senza caricarne le miniature**. Se quella separazione non fosse possibile, il disegno
  della timeline andrebbe ripensato.
- **L'anno è fisso a "2026"** nelle intestazioni e nei tooltip della barra dei mesi, non derivato
  dai dati.
- `"Rinomina cartella…"` rinomina in realtà **solo le foto passate dai filtri attivi**, mentre il
  sottotitolo dichiara `Tutta la cartella "X" (N foto)`.
- `"3 lotti attivi"` in Impostazioni è un valore costante: non cambia se si cambia la cartella
  radice del culling.
- L'indicatore dello spazio libero è statico e la barra non corrisponde al testo.
- Errore di lingua ricorrente in tutta l'app: la flessione automatica produce `"1 fota rinominata"`
  e `"3 fote"` — "foto" è invariabile in italiano. Va corretto ovunque compaia questa costruzione.

### 3.4 Comandi che dichiarano un contratto e non lo rispettano

- **Nella Modifica in blocco**, le azioni Album, Tag e Rinomina si applicano **subito**, fuori dal
  contratto "Applica / Annulla" che il sottotitolo della schermata dichiara. `"Annulla"` non le
  disfa.
- **L'eliminazione in blocco** contrassegna le foto ma **non le rimuove dalla timeline**, pur
  mostrando il messaggio `"N foto eliminate."`
- `"Applica a N foto"` non è mai disabilitato: senza aver toccato nulla azzera comunque la
  selezione e conferma il successo.
- Nel dialog di condivisione, gli interruttori delle persone **non vengono conservati**: si
  perdono alla chiusura, ma un toast conferma comunque.
- `"Rinomina persona"` non controlla il campo vuoto (a differenza degli altri dialog analoghi):
  confermare vuoto azzera il nome e conferma comunque.

### 3.5 Convalide e controlli mancanti nella rinomina

Il dialog di rinomina è la parte del prototipo con più conseguenze sul disco, ed è anche quella
con le convalide più deboli. Da irrobustire prima di toccare file veri:

- Le **collisioni di nomi sono verificate solo dentro il gruppo selezionato**: nessun controllo
  contro i file già presenti sul disco, contro le foto fuori ambito, né contro le sottocartelle
  escluse. Nessuna strategia di risoluzione automatica.
- Un **segnaposto vuoto lascia i separatori orfani**: un modello come `{data}_{luogo}_{n}` con
  luogo assente produce `2026-08-14__001`.
- La **sanificazione dei caratteri è parziale**: alcuni caratteri vietati dai filesystem restano
  ammessi. Nessun limite di lunghezza del nome.
- Un nome di file **senza estensione** produce un risultato malformato.
- Il pulsante `"Applica"` è reso inattivo solo visivamente: da tastiera resta raggiungibile e
  premerlo non dà alcun riscontro.

### 3.6 Attriti minori ma reali

- **La miniatura corrente del filmino del culling non viene riportata in vista** navigando con le
  frecce: su un lotto di 184 foto esce dallo schermo e non torna.
- **Nel filmino, le foto già scelte o scartate non sono distinguibili** da quelle da valutare
  quando il filtro è su "tutte": non c'è alcun contrassegno di stato sulle miniature.
- **Nessuna animazione di uscita** quando una foto cambia stato con un filtro attivo: sparisce di
  colpo, che è esattamente l'effetto "ho perso qualcosa" da evitare.
- `"Seleziona tutte"` non cambia etichetta quando sta per deselezionare (**SP-20**).
- Nei dialog **il focus non è confinato** e **il click sul velo non chiude** (**SP-5**).
- La freccia "indietro" su mobile torna sempre alla timeline, anche quando si è arrivati da
  altrove; da un lotto aperto non torna alla griglia dei lotti.
- L'opzione di tema `"Sistema"` legge la preferenza del sistema **una volta sola** al momento del
  click: non segue il passaggio chiaro/scuro del sistema operativo a schermo acceso.
- Il cursore della densità aggiorna la descrizione ma non rilancia il calcolo del layout: la
  griglia si adegua solo al render successivo.
- In Profilo, `"Salva modifiche"`, `"Cambia password"`, `"Esci"` e `"Esci da tutti gli altri
  dispositivi"` **non sono collegati a nulla**: sono segnaposto.
- Restano nel foglio di stile alcune regole per controlli rimossi (l'interruttore del tema in alto,
  un accordion nel menu mobile, un indicatore di avanzamento del culling mai usato).

---

## Decisioni di prodotto rimaste aperte

Punti dove il prototipo mostra una soluzione ma la scelta non è stata chiusa, o dove servirebbe
una risposta che l'interfaccia da sola non può dare.

1. **Cosa succede ai file "scartati" di un lotto, e quando.** Il prototipo li sposta dentro il
   lotto e offre `"Svuota scartati"`, ma la politica di lungo periodo (restano lì per sempre?
   scadono? finiscono nel cestino?) non è decisa.
2. **Se le decisioni umane sopravvivano all'eliminazione di un tag**, e come. Se un tag confermato
   dall'utente viene cancellato dalla pagina Tag e categorie, si perde anche l'informazione che
   quella conferma c'era stata.
3. **La scadenza automatica del cestino a 30 giorni è dichiarata a testo ma non implementata**:
   chi la applica, il server con un lavoro periodico o l'interfaccia?
4. **Se una foto in RAW+JPEG possa essere separata** nei suoi due file, e cosa comporti. Oggi la
   coppia è sempre trattata come un'unica entità.
5. **Se l'unione di due persone sia reversibile.** Il prototipo offre sia unione sia separazione,
   ma non è chiaro se separare ripristini davvero lo stato precedente o crei semplicemente una
   nuova persona.
6. **Cosa accade ai dati dei volti già calcolati quando il riconoscimento viene spento.** Esiste
   un comando distinto per eliminarli, quindi spegnere e cancellare sono due cose diverse: il
   comportamento intermedio (spento ma dati conservati) va specificato.
7. **Il criterio con cui due foto sono considerate duplicate** è mostrato ma non definito: stesso
   contenuto? stesso nome? stessa impronta? È una decisione che appartiene al backend e che
   l'interfaccia deve solo saper spiegare all'utente.
8. **La soglia e la logica della pausa automatica dell'analisi** mentre l'utente naviga: il
   principio è giusto e va conservato, i numeri esatti sono da tarare sul sistema vero.
9. **Se il filtro rapido debba essere per vista o globale** (vedi 3.1). Non è un difetto da
   correggere meccanicamente: è una scelta.
10. **Se serva un annullamento generale.** Oggi nessuna azione è annullabile e nessuna preferenza
    ha un "Salva": tutto è immediato. Per le azioni sul disco questo è un rischio; per le
    preferenze è probabilmente giusto così.

---

## Cosa il prototipo non copre affatto

Aree deliberatamente fuori dal disegno di questa fase. Sono elencate perché il loro assenza dal
documento **non va letta come "non serve"**:

- **L'importazione iniziale della libreria**: la scansione delle cartelle, la prima indicizzazione,
  la scelta di quali percorsi sorvegliare.
- **L'amministrazione del server**: utenti multipli e permessi, backup, aggiornamenti, spazio su
  disco reale, stato dei servizi.
- **La vista pubblica di un link condiviso**: il documento descrive come si crea e si gestisce una
  condivisione, non cosa vede chi riceve il link.
- **La modifica delle immagini**: ritaglio, esposizione, correzioni. `"Ruota"` esiste come comando
  ma nel prototipo è dimostrativo.
- **Video**: l'intero disegno assume fotografie.
- **L'installazione e la configurazione iniziale** al primo avvio.
- **Il comportamento offline** e la sincronizzazione.
- **Notifiche vere** (le impostazioni relative esistono, il meccanismo no).

---

## Le azioni volutamente dimostrative

Nel prototipo alcune azioni mostrano un messaggio che comincia con `"Solo demo — …"` invece di
agire. **Non sono dimenticanze: sono esattamente i punti in cui il backend dovrà fare qualcosa di
vero**, e sono elencate qui come lista di lavoro.

- `"Scarica originale"` — scaricherebbe il file originale sul dispositivo.
- `"Ruota"` — ruoterebbe l'immagine, dichiarando esplicitamente che il file originale sul disco
  non viene mai modificato. Quella promessa è una scelta di prodotto da confermare: implica
  modifiche non distruttive conservate a parte.
- `"Esci"` dal menu account.
- Il download delle mappe offline.

Il testo esatto di ciascun messaggio è riportato nella sezione della schermata corrispondente.
