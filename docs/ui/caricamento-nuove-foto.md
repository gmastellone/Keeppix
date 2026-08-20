# Keeppix — Caricamento di nuove foto

**Cosa copre:** il caricamento occasionale di nuove foto e video in una libreria già
esistente, dal browser. Non copre l'importazione iniziale di una libreria intera (problema
diverso, Fase 11) e non tocca in alcun modo il Culling.

**Stato:** disegnato, prototipato e verificato in `docs/ui/keeppix-mockup.html`. Il prototipo
è la fonte di verità sul comportamento; questo documento spiega il perché e i dettagli
estetici.

---

## 1. Il principio, in una riga

> **Keeppix non chiede mai «dove?» quando la risposta è già sullo schermo.**

Sei dentro *Urbino* → le foto vanno in Urbino, senza domande. Sei su *Tutte le foto* → lì la
domanda è legittima, e **solo lì** viene fatta.

Non è un vezzo: è la risposta a un vincolo reale. In `frontend/src/stores/upload.ts` un
caricamento in coda **senza cartella di destinazione non parte mai** — resta bloccato su
`queued` per sempre, in silenzio. Invece di aggiungere un passaggio obbligatorio per tutti,
il disegno fa due cose: eredita la destinazione dal contesto ogni volta che può, e quando non
può rende quel blocco **la cosa più visibile del pannello**, a un click dalla soluzione.

Il difetto tecnico diventa la spina dorsale dell'interfaccia invece di un caso da gestire.

---

## 2. Perché non c'è un pulsante flottante

L'ipotesi iniziale era un grande pulsante rotondo arancione, in stile Material. È stata
scartata per tre ragioni, tutte verificabili sul prototipo:

1. **Copre le foto.** Un pulsante flottante sta in basso a destra, cioè sopra la griglia. In
   un'applicazione di fotografie ogni pixel occupato da un comando è una foto che non si vede.
2. **Collide con quello che c'è già.** In basso a destra della timeline passa lo **scrubber
   dei mesi**; in basso al centro escono i **messaggi temporanei**. Sono zone già occupate.
3. **Rompe il linguaggio del marchio.** Le linee guida stabiliscono che il pallino arancione
   è *l'unico elemento a colore di tutto il sistema*. Un disco arancione da 56 px sarebbe
   diventato l'elemento più rumoroso dello schermo, e l'accento avrebbe smesso di significare
   qualcosa.

Le porte d'ingresso sono invece **tre, in ordine di sforzo crescente**, e la prima costa zero
pixel a riposo.

---

## 3. Le tre porte d'ingresso

### 3.1 Trascinare (desktop, porta principale)

Non esiste a riposo. Trascinando file sopra la finestra, l'area contenuto si vela e
**dichiara la destinazione prima che il mouse venga rilasciato**:

- dentro una cartella → *«Rilascia per caricare in Urbino»*
- su Tutte le foto → *«Rilascia le foto qui»*, con il sottotitolo che avvisa che poi si
  sceglierà dove

Entrambi i sottotitoli ricordano che i RAW si caricano dal Culling: l'informazione arriva
*prima* dell'errore, non dopo.

Tecnicamente: `dragenter`/`dragover`/`dragleave`/`drop` sono agganciati a `#app`, filtrati su
`dataTransfer.types` contenente `Files` — trascinare testo o un'immagine da un'altra scheda
non attiva nulla. `dragenter`/`dragleave` scattano anche passando sui figli, quindi si conta
la profondità (`_dragDepth`) invece di fidarsi del singolo evento. `dragover` chiama
`preventDefault()`, senza il quale il browser aprirebbe il file al posto nostro.

**Nel Culling il rilascio è rifiutato** con un messaggio dedicato: è un'area separata con un
suo percorso di importazione.

### 3.2 Il comando `Carica` nella topbar (desktop)

A sinistra del campo di ricerca, come pulsante fantasma con icona. Stessa posizione in ogni
vista, come la ricerca, e non copre mai niente.

L'etichetta **cambia con il contesto**: `Carica` su Tutte le foto, `Carica qui` dentro una
cartella. Il tooltip e l'etichetta per gli assistenti vocali sono ancora più espliciti
(*«Carica foto o video nella cartella Urbino»*), così la destinazione si conosce prima di
aprire il selettore di file.

### 3.3 Il `+` nell'header (mobile)

In alto a destra, accanto all'avatar, dove già vivono i comandi della schermata. Compare solo
dove caricare ha senso: Foto, Preferiti, Album, Libreria — mai nel Culling, mai nelle
impostazioni.

Anche qui niente pulsante flottante: in basso c'è la tab bar, e sopra la griglia la toolbar
dei filtri sticky. Un elemento flottante finirebbe per forza sopra le foto.

---

## 4. Cosa entra e cosa no

| Categoria | Estensioni | Esito |
|---|---|---|
| **Immagini** | jpg, jpeg, jpe, png, tif, tiff, webp, heic, heif | Accettate |
| **Video** | mp4, mov, m4v | Accettati, con stato «in preparazione» dopo il caricamento |
| **RAW** | arw, sr2, srf, cr2, cr3, crw, nef, nrw, raf, orf, rw2, raw, **dng**, pef, srw, x3f, 3fr, iiq, mos, mef, erf, kdc, dcr, mrw, rwl, fff | **Rifiutati sempre**, con rimando al Culling |
| **Tutto il resto** | — | Rifiutato come formato non supportato |

`dng` è trattato come RAW e non come immagine: è un contenitore RAW a tutti gli effetti.

### Il rifiuto dei RAW non è un errore, è una spiegazione

I RAW non entrano da qui **per decisione di prodotto**, non per limite tecnico: entrano dal
Culling, dove si scelgono *prima* di importarli.

Un fotografo che trascina una cartella ci butta dentro RAW e JPEG insieme. Rifiutare
l'**intero** rilascio sarebbe ostile e gli farebbe perdere il lavoro buono insieme a quello
scartato. Quindi il gruppo si divide: **quello che può entrare parte subito**, e il resto
viene elencato a parte con la sua ragione.

Il blocco di rifiuto dice cosa succede e dove andare, e porta un pulsante `Apri Culling`:

> **5 file RAW non caricati**
> I RAW entrano in Keeppix dal **Culling**, dove li scegli prima di importarli. Da qui si
> caricano solo foto già sviluppate e video.
> `DSC20.ARW, DSC21.CR3, DSC22.NEF, DSC23.dng e un altro`

I nomi sono elencati fino a quattro, poi *«e un altro»* / *«e altri N»* — con la concordanza
corretta, che è un difetto già presente altrove nell'app e che non andava replicato qui.

---

## 5. La destinazione

Il chip in cima al pannello, sempre visibile e sempre modificabile: `Destinazione  Urbino ▾`.

**Ordine di precedenza:**

1. Il contesto esplicito del comando premuto (sei dentro una cartella).
2. Se la coda precedente è conclusa, **il contesto attuale**. La destinazione non resta
   appiccicata a quella di un caricamento finito mezz'ora prima: sarebbe il modo più
   silenzioso di mandare delle foto nel posto sbagliato.
3. Se una coda è ancora in corso, la sua destinazione resta: è un lotto solo, e non si
   ridirigono file già partiti.

Il menu elenca tutte le cartelle più `Nuova cartella…`, che riusa il dialog di testo già
esistente e imposta la nuova cartella come destinazione appena creata.

### Lo stato che blocca, reso visibile

Quando la destinazione manca, tre cose cambiano insieme:

- il chip diventa **arancione tenue con anello d'accento** e recita in corsivo *«Scegli una
  cartella»*;
- sotto compare la riga *«Le foto restano in coda finché non scegli dove metterle»*;
- la striscia nella sidebar cambia etichetta in **`Scegli dove`**, prende l'icona di avviso e
  l'anello d'accento;
- il titolo del pannello diventa *«In attesa di una destinazione»*.

La coda **non parte**, esattamente come nel codice vero — ma qui il perché è scritto, e
scegliere la cartella è l'unica azione che la sblocca.

---

## 6. La coda

### 6.1 Dove vive, e perché lì

**Desktop: nel piede della sidebar, sopra «Spazio libero».** Non un pannello che galleggia
sulle foto, ma una striscia compatta dove già abita l'informazione sulla risorsa che un
caricamento consuma — lo spazio su disco. Riusa un contenitore che esiste invece di
inventarne uno.

**Mobile: una fascia sopra la tab bar**, che è il bordo persistente equivalente. Stessa
informazione, stesso comportamento al tocco, solo un altro ancoraggio. Solo una delle due
esiste per volta: cambiando form factor non restano due strisce.

**A coda vuota la striscia non esiste.** A riposo il costo in pixel è zero.

### 6.2 Il pannello

Un click sulla striscia lo espande.

- **Desktop:** ancorato in basso a sinistra (`left:12px; bottom:12px`), largo 344 px, alto al
  massimo 460 px. Non tocca mai lo scrubber dei mesi sul bordo destro.
- **Mobile:** diventa il **bottom sheet** già usato dai filtri — tutta larghezza, angoli
  superiori arrotondati, velo scuro dietro, alto al massimo il 72%.

Struttura in quattro fasce: **testata** (titolo + pausa + chiudi), **fascia destinazione**
(non scorre, così il menu può aprirsi verso il basso senza essere tagliato), **corpo
scorrevole** (rifiuti + righe), **piede** (riepilogo + azioni).

Il caricamento **continua** a pannello chiuso, e la striscia lo ricorda. Chiudere non annulla.

### 6.3 I sei stati

| Stato | Come si presenta |
|---|---|
| **In coda** | `293 KB · in coda`, riga neutra |
| **In caricamento** | `293 KB · 34%` + barra di avanzamento sottile in accento |
| **In pausa** | `293 KB · in pausa` |
| **Completato** | Badge neutro `COMPLETATO` |
| **Saltato** | Badge **ambra** `SALTATO`, riga *«già in libreria»* + link `Vedi quella presente` |
| **Errore** | Badge **rosso** `ERRORE`, riga *«il server non ha risposto»* + link `Riprova` |

Più, sui video completati, il badge `IN PREPARAZIONE` in accento tenue: il file è caricato ma
la resa gira in background o di notte, quindi non è ancora riproducibile. È uno stato che
appartiene alla foto, non alla coda, e resta visibile.

**Il duplicato non è un errore.** Stesso `content_hash` già presente in libreria: è un esito
legittimo, ha un colore proprio, e **non sparisce in silenzio** — far sparire un file senza
dirlo è il modo più rapido per far credere a qualcuno di aver perso delle foto.

### 6.4 I comandi

| Comando | Dove | Cosa fa |
|---|---|---|
| `Pausa` / `Riprendi` | testata, a icona | Ferma la coda; il file in corso passa a «in pausa» |
| `Chiudi` | testata, a icona | Chiude il pannello **senza annullare** |
| `Pulisci completate` | piede | Rimuove concluse, saltate ed errate; a coda vuota il pannello si chiude da solo |
| `Annulla tutto` | piede | Svuota la coda e azzera la destinazione |
| `Riprova` | riga in errore | Rimette in coda quel singolo file |
| `Vedi quella presente` | riga saltata | Porta alla copia già in libreria |
| `Apri Culling` | blocco RAW | Va al Culling |

Il piede riassume: *«11 caricate · 1 saltata · 1 non riuscita»*, con la concordanza corretta
al singolare e al plurale.

---

## 7. Estetica, nel dettaglio

Tutto usa i token esistenti: nessun colore nuovo è stato introdotto per questa funzione.

### 7.1 Colori

| Elemento | Token | Chiaro | Scuro |
|---|---|---|---|
| Barra di avanzamento, anelli di attenzione, badge «in preparazione» | `--accent` | `#F2812E` | `#FF9D52` |
| Fondo del velo di rilascio, chip destinazione mancante | `--accent-tint` | `#FDE7D6` | `#2c1c0e` |
| Badge «saltato», bordo e fondo del blocco rifiuti | `--warn` / `--warn-tint` / `--warn-border` | `#A15C00` / `#FDF0DC` / `#EFD4A8` | `#F0A23C` / `#2E2110` / `#4A361A` |
| Badge «errore» | `--danger` / `--danger-tint` | `#CC4038` / `#FBE4E2` | `#FF6B61` / `#331411` |
| Fondo striscia e miniature | `--chip-bg` | `#f5f5f5` | `#141414` |
| Fondo del pannello | `--card-bg` | `#ffffff` | `#0a0a0a` |
| Bordo del pannello | `--border-strong` | `#dcdcdc` | `#2c2c2c` |

**Tre nature, tre colori, coerenti col resto dell'app:** il completato è **neutro** (è la
norma, non merita colore), il saltato è **ambra** (non è un fallimento), l'errore è **rosso**.
L'accento è riservato al movimento — l'avanzamento — e all'unica cosa che chiede un'azione.

### 7.2 Geometria e tipografia

- **Striscia:** raggio 10 px, padding `10px 12px`, margine `6px 2px 8px`. Etichetta 11,5 px
  peso 600; contatore 11 px in `--text-tertiary`. Barra alta **5 px**, raggio 3 px — la stessa
  geometria di `.storage-bar` sotto, perché le due devono leggersi come la stessa famiglia e
  non come due indicatori diversi.
- **Pannello:** raggio 12 px, bordo 1 px, ombra `0 18px 44px rgba(0,0,0,.28)`. Titolo 13 px
  peso 700.
- **Righe:** altezza libera, padding verticale 7 px, separate da una linea `--border` (l'ultima
  no). Miniatura 30×30 px, raggio 6 px, con l'icona del tipo (fotografia o video). Nome file
  12 px peso 600 con ellissi; metadati 11 px in `--text-tertiary`. Barra della riga alta **3 px**
  — più sottile di quella della striscia, perché è di grado inferiore.
- **Badge:** 10 px, peso 700, maiuscoletto con `letter-spacing .02em`, padding `2px 6px`,
  raggio 5 px.
- **Chip destinazione:** raggio 9 px, padding `8px 10px`, 12 px. L'etichetta «Destinazione» è
  in `--text-tertiary`, il valore in peso 700: si legge il *valore*, non l'etichetta.
- **Velo di rilascio:** bordo **tratteggiato** 2 px in accento, raggio 12 px, titolo 16 px
  peso 700, sottotitolo 12,5 px largo al massimo 340 px.

### 7.3 Movimento

Volutamente pochissimo. Il caricamento è già un'attesa: aggiungere animazioni la allunga.

| Cosa | Durata | Curva |
|---|---|---|
| Comparsa del velo di rilascio | 0,12 s | `ease` |
| Barra della striscia | 0,25 s | `ease` |
| Barra della riga | 0,2 s | `linear` — l'avanzamento non deve accelerare, mentirebbe |
| Indicatore rotante | 0,7 s | `linear` |

Con la preferenza di sistema **«riduci le animazioni»** attiva, il velo compare senza
dissolvenza e le barre saltano al valore senza transizione.

Il ritmo simulato è di circa **3 passi per file**: la barra si vede muovere, ma rivedere la
coda non diventa un'attesa. Con «Rete lenta» acceso nel pannello *Anteprima stati* il ritmo
rallenta, coerentemente col resto dell'app.

### 7.4 Il pannello a riposo non esiste

Vale la pena ripeterlo perché è la scelta estetica principale: **finché non si carica niente,
di tutta questa funzione si vede un solo pulsante di testo nella topbar** (e un `+` su
mobile). Nessuna striscia, nessun pannello, nessun elemento flottante, nessuna area
tratteggiata permanente.

---

## 8. Tastiera e accessibilità

Il prototipo dichiara altrove la propria accessibilità da tastiera come rotta. **Qui no**:
questa funzione è nata accessibile.

- Ogni comando è raggiungibile con **Tab** e si attiva con **Invio** e **Barra spaziatrice**.
- **Esc a livelli:** la prima pressione chiude il menu della destinazione, la seconda chiude
  il pannello. Il caricamento continua comunque — chiudere non annulla.
- La striscia espone `role="button"` e `aria-expanded`; il pannello `role="dialog"` con
  etichetta; il chip `aria-haspopup` e `aria-expanded`; il menu `role="listbox"` con
  `aria-selected` sull'opzione attiva.
- Le etichette per gli assistenti vocali sono più esplicite di quelle visibili: la striscia
  annuncia *«Scegli dove, 0 di 13. Apri il pannello dei caricamenti»*.
- Il blocco dei rifiuti è `role="note"`; i messaggi d'errore sono annunciati come avvisi.
- I tooltip sono disattivati su mobile, come nel resto dell'app: lì l'icona deve bastare, e
  l'etichetta per gli assistenti vocali resta.

**Nessun comando è disabilitato.** Quello che bloccherebbe — la destinazione mancante — è reso
*visibile* invece che disattivato: un pulsante spento non spiega perché è spento.

---

## 9. Cosa serve al backend

In termini di cose, non di endpoint.

**In lettura:** l'elenco delle cartelle della libreria, con nome e identificativo, per il
menu della destinazione.

**In scrittura:**

1. **Creare una cartella** e ricevere il suo identificativo, per `Nuova cartella…`.
2. **Caricare un file in una cartella specifica.** La destinazione fa parte della richiesta:
   non esiste un caricamento senza cartella.
3. **Avanzamento per file** — byte inviati su byte totali — per la barra.
4. **Un esito per file**, non per lotto, con quattro possibilità distinte: riuscito, saltato
   perché duplicato (**con l'identificativo della copia già presente**, che serve al link
   *«Vedi quella presente»*), fallito con una ragione, in preparazione (video caricato ma non
   ancora reso).
5. **Mettere in pausa e riprendere** la coda.

L'esito «saltato per duplicato» è già coperto: la libreria salta i file già presenti in base
al `content_hash`, e l'interfaccia usa esattamente quella nozione.

Il punto 4 è quello che vale la pena verificare per primo: **un caricamento non può rispondere
«fatto» o «non fatto»**. Senza l'esito per singolo file, l'interfaccia non può distinguere un
duplicato da un errore, e la distinzione fra i due è metà di questo disegno.

---

## 10. Fuori perimetro

- **Il Culling non è stato toccato.** Nessuna schermata, nessun flusso, nessuno stato. L'unico
  contatto è un collegamento in uscita dal blocco dei RAW.
- **L'importazione iniziale di una libreria intera** non è disegnata: è un problema diverso,
  già tracciato per la Fase 11.
- **L'interfaccia del video** oltre lo stato «in preparazione» non è disegnata (anch'essa
  Fase 11): qui il video si carica e dichiara di non essere ancora pronto, niente di più.
- **Il caricamento condiviso dal sistema operativo** (share target) non è disegnato, ma
  ricade nello stesso vincolo: anche lì serve una destinazione, ed è il motivo per cui oggi
  resta bloccato.
