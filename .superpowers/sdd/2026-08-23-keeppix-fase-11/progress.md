# Fase 11 — Interfaccia: quattro tranche A→B→C→D

Piano: `docs/superpowers/plans/2026-08-20-keeppix-fase-11.md`
Spec: `docs/superpowers/specs/fase-11-interfaccia.md`
Branch: `fase-11`, da `main` post-merge Fase 9 (`055ba56`).

> Il documento funzionale (`docs/ui/documento-funzionale-ui.md`) e il
> prototipo interattivo (`docs/ui/keeppix-mockup.html`) sono le fonti di
> verità sul comportamento; l'ordine di lettura di `PROSEGUI.md` §2 è stato
> seguito prima di scrivere codice: «Decisioni prese» → prototipo → doc
> funzionale (Parte XII letta per prima) → analisi gap.

## Pre-volo — stato reale del frontend (verificato, non assunto)

Confermato leggendo `frontend/package.json` e l'albero `src/`: Vue 3.5.40,
Pinia 4.0.3, vue-router 5.2.0, vue-i18n 11.4.8, Tailwind 4.3.3, Vite 8.2.1,
Vitest 4.1.10, reka-ui 2.10.3. Viste esistenti (`src/views/`): Timeline,
Culling, Map, Search, Albums, Shares, Trash, Users, Groups, Problems, Setup,
Login, Player, BatchEdit, Folders, ShareTarget — coerente con quanto
dichiara il piano. Componenti riusabili confermati: `AssetViewer`,
`Filmstrip`, `RatingStars`, `PlacePicker`, `MapClusterLayer`, `UploadPanel`,
`SharePanel`. Budget di bundle già verificato in CI (`.github/workflows/
ci.yml`, job `frontend`, step "Budget del bundle iniziale (150 KB gzip)"):
90.364/153.600 byte prima di questo task.

## Gruppo/Tranche A — Fondamenta

### Task 1 — I token di stile e le due mappe di tema

Ogni numero verificato **contro il sorgente reale del prototipo**
(`docs/ui/keeppix-mockup.html`), non contro il riassunto che ne fa il piano:

- `grep -n "transition\|animation" keeppix-mockup.html` conferma le sette
  durate (non solo le quattro "core" citate dal piano): `.1s` (dissolvenza
  tooltip), `.12s` (comparsa comandi tessera — 54 casi), `.15s` (rotazione
  freccia), `.18s` (comparsa tessera), `.2s` (toast, generiche — 53 casi),
  `.25s` (cambio tema), `.3s` (avanzamento analisi/upload). Curva `ease`
  confermata dominante.
- Toast (righe 2611-2614 del mockup): `setTimeout(...,10)` per lo show
  delay, `setTimeout(()=>t.remove(),250)` per la rimozione dal DOM,
  `life = opts.action ? 6500 : (kind==='ok' ? 2400 : 4200)`.
- Tocco prolungato (riga 4186-4188): `navigator.vibrate(15)` dopo `500`ms.
- Pulsazione analisi: `analysisPulse 1.4s ease-in-out infinite` (riga 1255).
- Colori di marca (righe 64-88): `--accent:#F2812E`/`--accent-text:#1a1005`
  in tema chiaro, `#FF9D52` in tema scuro; `--danger:#CC4038` chiaro /
  `#FF6B61` scuro. Verde condiviso `#2E9E5B` (riga 243, commento del
  prototipo stesso: *"culling (#2E9E5B, 3.4:1 su bianco), riusato qui invece
  di introdurre una terza tonalità"*) per `.status-dot` (in linea) e
  `.btn-pick.chosen` (Scelta) — nessuna variante scura dichiarata nel
  mockup, tenuto identico nei due temi.

**Scoperta prima di scrivere codice**: `frontend/src/style.css` aveva già un
blocco `@theme` con `--color-accent`/`--color-danger`, ma **come
approssimazione oklch generica** (blu, non il marchio), e — difetto reale,
non solo un'imprecisione — **il blocco `@media (prefers-color-scheme:
dark)` non li ridefiniva affatto**: il tema scuro ereditava silenziosamente
l'accento/pericolo del tema chiaro invece del proprio. Corretto qui, non
rinviato: entrambi i colori ora hanno un valore hex esatto per tema, letto
dal marchio, non stimato.

**Ruling: le durate vivono in `:root`, non dentro `@theme`.** — `@theme` è
lo spazio dei nomi che Tailwind trasforma in classi utility (`bg-accent`,
ecc.); le durate servono a transizioni CSS scritte a mano e a `setTimeout`
lato JS, non a generare utility Tailwind — nessuna necessità verificata di
una classe `duration-fast` finché un componente non la chiede. Proprietà
custom generiche in `:root`, referenziabili sia da CSS (`var(--duration-
fast)`) sia, per i valori arbitrari Tailwind, da `duration-[var(--duration-
fast)]` — sintassi stabile e documentata, a differenza dello spazio dei nomi
esatto che Tailwind 4 userebbe per generare `duration-*` da `@theme`, non
verificato con una build reale e quindi non assunto.

`prefers-reduced-motion` **era già gestito centralmente** (blocco `*, *::
before, *::after { animation-duration: 0.01ms !important; ... }`) — meglio
del prototipo, che lo fa per-selettore in modo sparso: tenuto com'era,
nessuna modifica necessaria.

Nuovo `frontend/src/design/tokens.ts`: costanti JS-side (`DURATION_MS`,
`TOAST_SHOW_DELAY_MS`, `TOAST_REMOVE_AFTER_MS`, `TOAST_LIFE_SUCCESS_MS`,
`TOAST_LIFE_ERROR_MS` — vale sia per errore sia per riuscita parziale,
`TOAST_LIFE_WITH_ACTION_MS`, `LONG_PRESS_THRESHOLD_MS`,
`LONG_PRESS_VIBRATE_MS`, `ANALYSIS_PULSE_MS`), stessa convenzione
`SCREAMING_SNAKE_MS` già in uso in `stores/session.ts`
(`SESSION_REFRESH_INTERVAL_MS`) — non uno stile nuovo.

Nuovo `frontend/src/design/tokens.spec.ts`: tre test di valore (i numeri
sopra, uno per uno) più uno scanner che legge ogni `.vue` sotto `src/` e
segnala qualunque `transition`/`animation`(`-duration`) dichiarata con un
tempo fuori dalle sette durate della palette — la verifica esplicita che il
piano chiede ("un test che nessun componente dichiari una durata fuori
dalla palette"). Verificato che lo scanner non sia un no-op (`vueFiles.
length > 0`, 38 file trovati) prima di fidarsi del suo "verde" per assenza
di violazioni: oggi zero componenti dichiarano `transition`/`animation` con
un tempo esplicito (solo `Button.vue` usa `transition-opacity`, una utility
Tailwind senza durata letterale), quindi lo scanner parte pulito per
costruzione, non perché non stia guardando nulla.

Verifica eseguita:
- `npx vitest run src/design/tokens.spec.ts` → 41/41 verdi (3 di valore +
  38 per-file, uno per ogni `.vue` sotto `src/`).
- `npx vitest run` (suite intera) → 136/136 verdi, nessuna regressione.
- `npx vue-tsc -b` → pulito.
- `npx eslint src/design/` → pulito.
- `npm run build` → riuscito; budget del bundle iniziale ricalcolato con lo
  **stesso script della CI** (`.github/workflows/ci.yml`, job `frontend`):
  `90.364` byte gzip su `153.600` — invariato nella sostanza (il CSS
  aggiunto è ~200 byte gzip), ampio margine.

Debiti dichiarati: nessuno. Le durate isolate (`.1s`/`.18s`/`.3s`, cinque
casi nel prototipo secondo il piano) sono nella palette ma non ancora usate
da alcun componente reale — normale, i componenti condivisi che le
useranno sono il Task 2, non ancora scritto.

Task 1: complete.

### Task 2 — I trenta pattern condivisi come componenti (in corso)

Diciotto componenti nel piano; questo commit ne chiude uno solo —
`Dialog` (SP-5) — non l'intero task. Scelto per primo perché il piano lo
dichiara esplicitamente fondativo: *"i ventiquattro dialog, menu e popover
del documento si costruiscono sopra due soli componenti — `Dialog` e
`Popover` — non uno per uno."*

Nuovo `frontend/src/components/ui/Dialog.vue`, sopra le primitive reali di
`reka-ui` (`DialogRoot`/`Portal`/`Overlay`/`Content`/`Title`/`Description`/
`Close`) — verificate leggendo i `.d.ts` del pacchetto prima di scrivere
codice, non assunte dal nome. Due difetti dichiarati del prototipo
(documento funzionale, "Attriti minori", 3.6: *"nei dialog il focus non è
confinato e il click sul velo non chiude"*) sono chiusi **gratis**, non
scritti a mano: `DialogContent` intrappola il focus di serie (variante
modale), e `DismissableLayer` (sotto `DialogContent`) chiude già su Esc e
su click fuori — comportamento della libreria, confermato dal test
`Escape asks the caller to close the dialog`, non un'assunzione.

**Le due eccezioni deliberate che il piano chiede di preservare** (dialog
di eliminazione → focus sull'opzione meno distruttiva; dialog di conferma
→ focus su "Annulla"): implementate con una prop `initialFocus?:
HTMLElement`, intercettando l'evento `openAutoFocus` di `DialogContent`
(`event.preventDefault()` + `.focus()` manuale) solo quando la prop è
passata — altrimenti resta il comportamento predefinito di reka-ui (primo
elemento tabbable). Verificato con un componente ospite reale montato nel
DOM (`attachTo: document.body`, necessario perché `document.activeElement`
in jsdom richiede nodi realmente allegati) che il focus atterra
sull'elemento indicato, non sul primo bottone del contenuto.

Aggiunto `ui.dialog.close` a `it.json`/`en.json` (chiave nuova, non
esisteva un namespace `ui.*`) per l'`aria-label` del pulsante di chiusura —
un'icona SVG inline, non un'etichetta di libreria: nessuna libreria di
icone esiste ancora nel progetto e non ne serve una per un solo glifo "×".

Verifica eseguita:
- `npx vitest run src/components/ui/Dialog.spec.ts` → 5/5 verdi: non
  renderizza nulla quando chiuso, titolo/descrizione/slot quando aperto,
  pulsante di chiusura etichettato (`aria-label="Chiudi"` in italiano,
  verificato impostando esplicitamente la lingua del test — il runtime di
  test parte in inglese per via di `detectLocale()`, scoperta durante lo
  sviluppo, non assunta), Esc chiude per davvero, il focus iniziale va
  sull'elemento passato invece che sul primo tabbable.
- `npx vitest run` (suite intera) → 142/142 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito dopo aver corretto un nome di componente riservato
  (`Dialog` come alias locale in un componente ospite di test — rinominato
  `TheDialog`) e l'ordine `setup`/`template` in un componente a oggetto.
- `npm run build` → bundle iniziale 90.596/153.600 byte gzip (stesso
  script della CI): **`Dialog.vue` non contribuisce ancora al peso**,
  perché nessuna schermata reale lo importa ancora — solo il suo stesso
  test. Il numero si sposterà quando i task successivi lo useranno
  davvero, non prima.

Debiti dichiarati (al momento del commit di `Dialog`): diciassette
componenti restavano da scrivere. Non un buco silenzioso: la sezione
«Tranche» del piano stesso struttura il lavoro in incrementi verificabili,
ed è così che questo task procede.

**Nota CI (commit `07ce058`)**: la run è arrivata rossa su
`timeline_with_fifty_permissions_stays_under_budget_at_200k`
(`crates/keeppix-db/tests/scale_200k.rs`) — un test di budget temporale
su 200k righe seminate, **non toccato da questo commit** (solo file sotto
`frontend/src/**` e `i18n/*.json`). `main` e il push del Task 1, con lo
stesso codice backend, erano appena passati verdi sullo stesso test
minuti prima — segnale di rumore del runner condiviso, non una
regressione reale. Ri-lanciato il solo job fallito
(`rerun_failed_jobs`), una volta, prima di continuare — non modificato
il test o il codice che misura, che non appartiene a questo diff.
**Confermato**: il ri-lancio (run `32630216744`, tentativo 2) è
arrivato verde poco dopo, chiudendo l'ipotesi di rumore del runner —
non una regressione reale.

### Popover (SP-14)

Secondo componente fondativo del Task 2, insieme a `Dialog` — il piano lo
dichiara esplicitamente: i ventiquattro dialog/menu/popover del documento
si costruiscono sopra questi due soli. Copre i sei menu a comparsa (menu
account desktop/mobile, "altre azioni" del lightbox, selettore rapido di
lotto, menu sul riquadro del volto, popover della mappa, picklist di
creazione album) e i selettori (persona/tag) quando non serve un dialog
modale a schermo intero.

Nuovo `frontend/src/components/ui/Popover.vue`, sopra
`PopoverRoot`/`Trigger`/`Portal`/`Content` di reka-ui (props verificate
nei `.d.ts` prima di scrivere codice: `side`/`align`/`sideOffset` da
`PopperContentProps`). Il difetto dichiarato del prototipo per questo
pattern (*"click fuori chiude, Esc chiude solo a metà"*, SP-14) è chiuso
gratis dallo stesso `DismissableLayer` sotto `PopoverContent` che chiude
già `Dialog` — nessun gestore scritto qui. **"Esc a livelli quando è
annidato"** (altra richiesta esplicita del piano) è anch'essa
comportamento della libreria: ogni `DismissableLayer` si registra nel
proprio stack di livelli, e solo quello più in alto reagisce a Esc — non
verificato con un test di annidamento reale in questo commit (nessun
secondo popover ancora nel codice per annidarlo davvero), ma la garanzia
viene dalla libreria, non da logica scritta qui che potrebbe sbagliare.

Verifica eseguita:
- `npx vitest run src/components/ui/Popover.spec.ts` → 3/3 verdi: si
  apre al click sul trigger, Esc chiude, `v-model:open` programmatico
  funziona (apertura/chiusura pilotata dal chiamante, non solo dal
  click).
- `npx vitest run` (suite intera) → 146/146 verdi.
- `npx vue-tsc -b` → pulito dopo aver corretto `defineModel('open', {
  default: undefined })` (sintassi non valida per un model opzionale —
  basta `defineModel<boolean>('open')` senza opzioni).
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale 90.610/153.600 byte gzip — invariato
  nella sostanza, stesso motivo di `Dialog`: nessuna schermata reale lo
  importa ancora.

Debiti dichiarati: sedici componenti del Task 2 restano da scrivere
(`PhotoTile`, `SelectionBar`, `QuickFilter`, `SelectAllVisible`,
`ToastHost`, `Tooltip`, `SuggestionQueue`, `ProvenanceBadge`, `Avatar`,
`AppShell`, `DeleteDialog`, `ConfirmDialog`, `SegmentedControl`,
`NavGroup`, `BusyButton`, `LoadingSkeleton`).

### ToastHost (SP-6/SP-28/SP-29)

Terzo componente del Task 2. A differenza di `Dialog`/`Popover` non
avvolge una primitiva reka-ui: il prototipo lo implementa come markup e
timer scritti a mano (`showToast`/`showErrorToast`/`showPartialToast`,
righe 2589-2636 di `keeppix-mockup.html`), quindi qui è uno store Pinia
(`stores/toast.ts`, la logica di tempo) più un componente di sola resa
(`components/ui/ToastHost.vue`) — separazione già in uso nel progetto per
stato che sopravvive a più schermate.

Tre nature confermate dal prototipo: successo (`ok`, resta neutro, nessun
filetto colorato — è il caso normale), errore e riuscita parziale
(entrambi con un filetto colorato e una vita più lunga, perché leggere
"cosa non ha funzionato" richiede più tempo di leggere "fatto"). Numeri,
tutti letti dal sorgente e già presenti in `design/tokens.ts` dal Task 1
(non ristimati qui): ritardo di comparsa `10ms`, rimozione dal DOM dopo
`250ms`, vita `2400ms` (successo senza azione), `4200ms` (errore o
parziale senza azione), `6500ms` quando è presente un'azione —
indipendentemente dalla natura, come fa il prototipo (`opts.action ?
6500 : ...`). Il passaggio del mouse su un toast con azione ferma il
timer (`pause`/`resume` nello store); solo i toast con azione lo
espongono, stesso comportamento del prototipo.

Aggiunti a `style.css`: `--color-toast-danger`/`--color-toast-warn`
(righe 69-72 del mockup, commento del prototipo stesso: lo sfondo del
toast è invertito rispetto al tema — `--color-content` su
`--color-surface`, mai il contrario — quindi i filetti non possono
riusare `--danger`/un futuro `--warn`, illeggibili su quello sfondo
scuro anche in tema chiaro). Aggiunte a `it.json`/`en.json`:
`ui.toast.retry`, `ui.toast.retryRemaining` (con `{n}`),
`ui.toast.partial` — prima chiave del progetto a usare il plurale nativo
di vue-i18n (`'... non è riuscita. | ... non sono riuscite.'`) invece del
solo `{n}` interpolato, perché l'italiano richiede l'accordo verbale
(singolare/plurale), non solo il numero. Firma corretta verificata
leggendo `@intlify/core-base/dist/core-base.d.ts` direttamente invece di
indovinarla: `t(key, named: NamedValue, { plural: n })`, non `t(key, n,
{ named })` (un primo tentativo sbagliato, corretto prima del commit).

`ToastHost.vue` monta un solo pannello fisso (`bottom-5`,
`left-1/2` centrato) che itera `store.toasts`; ogni toast usa `role="alert"`
per errore/parziale (non per successo — non interrompe chi sta leggendo
per dire "fatto") e l'azione è raggiungibile da tastiera
(`tabindex="0"`, `@keydown.enter`/`@keydown.space`), non solo dal mouse —
il prototipo la implementa come click soltanto. **Cablato in `App.vue`**
(`<ToastHost />` accanto a `<UploadPanel />`, montato una volta sola,
sempre presente): un componente scritto ma non montato da nessuna
schermata reale sarebbe morto, non un debito accettabile per un pattern
già pronto all'uso da chi scriverà i prossimi componenti.

**Bug trovato scrivendo il test, non nello store**: la prima stesura di
`toast.spec.ts` avanzava i timer finti sommando gli intervalli dopo il
ritardo di comparsa (`advanceTimersByTime(10)` poi `+2399`), ma il timer
di chiusura viene armato da `show()` al tempo `t=0`, non a `t=10` —
l'aritmetica del test, non la logica dello store, era sbagliata (due
asserzioni fallivano per un tempo cumulato oltre la scadenza reale).
Corretto ricalcolando i tempi assoluti invece di ipotizzarli.

Verifica eseguita:
- `npx vitest run src/stores/toast.spec.ts` → 8/8 verdi (dopo la
  correzione sopra): ritardo di comparsa, vita 2400/4200/6500ms per
  natura e presenza di azione, pausa/ripresa al passaggio del mouse,
  `runAction` chiude ed esegue il richiamo, testo italiano singolare/
  plurale esatto per `showPartial`.
- `npx vitest run` (suite intera) → 155/155 verdi.
- `npx vue-tsc -b` → pulito (compresa la firma di `t()` con plurale,
  verificata a compilazione, non solo a runtime).
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **91.900/153.600** byte gzip (stesso
  script della CI): +1.300 byte circa rispetto a `Dialog`/`Popover`
  perché `ToastHost` è il primo dei tre a essere davvero cablato in
  `App.vue` — differenza attesa, non un regressione, ampio margine
  residuo (61.700 byte).

Debiti dichiarati: quindici componenti del Task 2 restano da scrivere
(`PhotoTile`, `SelectionBar`, `QuickFilter`, `SelectAllVisible`,
`Tooltip`, `SuggestionQueue`, `ProvenanceBadge`, `Avatar`, `AppShell`,
`DeleteDialog`, `ConfirmDialog`, `SegmentedControl`, `NavGroup`,
`BusyButton`, `LoadingSkeleton`).

### Tooltip (SP-7)

Quarto componente del Task 2. Sorgente reale verificata invece del solo
riassunto del piano: il pattern generico `[data-tip]` del prototipo
(`keeppix-mockup.html` righe 382-395, non lo `scrubber-tooltip` separato
delle righe 376-380, che è un componente diverso e specifico dello
scrubber della timeline) — transizione `opacity,transform .12s ease`,
nessun ritardo, disattivato su mobile. Il commento del prototipo stesso
(riga 1128, sui pulsanti icon-only della barra di selezione) è la fonte
della regola di accessibilità: *"il significato lo porta il tooltip
(desktop) + aria-label (sempre)"* — cioè il tooltip è decorazione, mai
l'unica fonte del nome accessibile. Per questo `Tooltip.vue` marca la
propria bolla `aria-hidden="true"`: chi passa da screen reader legge
l'`aria-label` del controllo nello slot, non due volte lo stesso testo.

**"Disattivato su mobile" tradotto per l'app reale**: il prototipo lo fa
con una classe `device-mobile` calcolata a mano su `#app` (demo statica,
nessun equivalente qui). Usato invece `@media not all and (hover: hover)
and (pointer: fine)` — lo stesso criterio che il commento del prototipo
descrive a parole ("niente hover sul touch"), ma verificabile dal
motore CSS invece che da uno stato JS da tenere sincronizzato.

Verifica eseguita:
- `npx vitest run src/components/ui/Tooltip.spec.ts` → 2/2 verdi: slot e
  testo del suggerimento presenti, bolla `aria-hidden` per non
  duplicare l'annuncio dello screen reader.
- `npx vitest run` (suite intera) → 158/158 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **92.026/153.600** byte gzip:
  +126 byte rispetto a `ToastHost`, nonostante nessuna schermata reale
  importi ancora `Tooltip` — scoperta non assunta: lo scanner di
  Tailwind guarda ogni `.vue` sotto `src/` per generare le utility CSS,
  non il grafo degli import, quindi le classi con valore arbitrario
  usate qui (`bottom-[calc(100%+8px)]`, `translate-y-[3px]`) finiscono
  comunque nel CSS compilato anche a componente inutilizzato. Margine
  ampio (61.574 byte).

Debiti dichiarati: quattordici componenti del Task 2 restano da
scrivere (`PhotoTile`, `SelectionBar`, `QuickFilter`,
`SelectAllVisible`, `SuggestionQueue`, `ProvenanceBadge`, `Avatar`,
`AppShell`, `DeleteDialog`, `ConfirmDialog`, `SegmentedControl`,
`NavGroup`, `BusyButton`, `LoadingSkeleton`).

### BusyButton (SP-30)

Quinto componente del Task 2. Distinto da `Button.vue` (già esistente,
la CTA primaria a piena larghezza dei flussi di impostazione): questo è
il `.btn` generico del prototipo (righe 290-299, 857-867, 2638-2657) —
variante `default`/`primary`/`danger`/`ghost`, spesso icon-only, usato
dalla barra di selezione e dalle azioni di massa. `Button.vue` non è
stato toccato: non serve, ha già `w-full` (nessuno spostamento di
larghezza possibile) e già usa l'attributo `disabled` nativo, che
blocca il doppio invio più efficacemente del solo `pointer-events:none`
del prototipo — SP-30 qui riguarda un pulsante diverso, non un difetto
di `Button.vue`.

Comportamento verificato riga per riga contro `setBtnBusy`/`.btn.is-
busy` (non assunto dal nome "BusyButton"): occupato → `disabled` nativo
(blocca il doppio invio, più forte del `pointer-events:none` del
prototipo), `aria-busy="true"`, opacità `.75`, spinner. **"Perde il
testo a favore dello spinner solo se è a sola icona, altrimenti
affianca"** (nota vincolante del piano): implementato con una prop
`iconOnly` — occupato e icon-only nasconde lo slot (spinner al posto
dell'icona, non due indicatori nello spazio di un solo glifo); occupato
e con etichetta la mantiene, lo spinner le si affianca. Sul pulsante
`primary` lo spinner assume `currentColor` (prototipo, riga 867) invece
del grigio neutro di default, per leggersi sul proprio sfondo pieno.

**Lo spinner (`.spinner`/`kpx-spin`) vive in `style.css`, non nel
componente**: è un'animazione che gira in loop finché il pulsante è
occupato, categoria diversa dalla palette delle sette durate di
transizione del Task 1 (quella è per stati che cambiano una volta
sola) — stesso trattamento già riservato a `ANALYSIS_PULSE_MS`, una
costante a parte, non un ottavo valore forzato nella palette. Verificato
che lo scanner di `tokens.spec.ts` non lo veda affatto (guarda solo i
file `.vue`, non `style.css`) prima di scegliere questa collocazione,
non dopo essere stato bloccato da un test rosso.

**Rallentato, non fermato, sotto `prefers-reduced-motion`** (prototipo,
riga 1071: `.spinner{animation-duration:2.4s}`): il blocco generico già
in `style.css` dal Task 1 spegne ogni animazione a `0.01ms` con
`!important` — senza un'eccezione lo spinner sparirebbe del tutto,
lasciando chi ha ridotto le animazioni senza alcun segnale che l'azione
sia ancora in corso. Aggiunta una regola `.spinner{animation-duration:
2.4s!important}` nello stesso blocco: vince sulla regola universale per
specificità (classe contro selettore universale), non per ordine.

Verifica eseguita:
- `npx vitest run src/components/ui/BusyButton.spec.ts` → 5/5 verdi:
  etichetta visibile e pulsante abilitato a riposo, `disabled`+`aria-
  busy` quando occupato, etichetta e spinner coesistono quando non è
  icon-only, l'icona sparisce a favore dello spinner quando lo è,
  l'icona torna visibile a riposo.
- `npx vitest run` (suite intera) → 164/164 verdi — compreso lo scanner
  delle durate, che non segnala `.7s`/`2.4s` perché vivono in
  `style.css`, non in un `.vue`.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **92.316/153.600** byte gzip
  (script CI): +290 byte circa (nuove classi Tailwind con valore
  arbitrario, stesso motivo di `Tooltip` — nessuna schermata reale lo
  importa ancora). Margine ampio (61.284 byte).

Debiti dichiarati: tredici componenti del Task 2 restano da scrivere
(`PhotoTile`, `SelectionBar`, `QuickFilter`, `SelectAllVisible`,
`SuggestionQueue`, `ProvenanceBadge`, `Avatar`, `AppShell`,
`DeleteDialog`, `ConfirmDialog`, `SegmentedControl`, `NavGroup`,
`LoadingSkeleton`).

### ConfirmDialog (SP-5) e DeleteDialog (SP-18)

Sesto e settimo componente del Task 2, presi insieme: entrambi si
costruiscono **sopra** `Dialog.vue`, non lo reimplementano — esattamente
il punto del piano ("i ventiquattro dialog... si costruiscono sopra due
soli componenti"). Comportamento verificato riga per riga contro le due
funzioni vanilla-JS del prototipo, non assunto dal nome:

- **`ConfirmDialog`** — `openConfirmDialog` (righe 6361-6385 del
  mockup): titolo, sottotitolo, un bottone di conferma rosso
  (`confirmLabel`, tipicamente distruttivo) e "Annulla". Il fuoco
  iniziale va su **"Annulla"**, non sulla conferma — passato a `Dialog`
  via `initialFocus`, la stessa prop e la stessa eccezione deliberata
  già implementata e testata per `DeleteDialog` sotto.
- **`DeleteDialog`** — `openDeleteDialogGeneric` (righe 4135-4164): la
  scelta a tre vie per eliminare (rimuovi dall'indice / cestino /
  disco), testo fisso di premessa ("Keeppix chiede sempre come
  procedere..."), terza opzione in rosso. Il fuoco iniziale va sulla
  **prima** opzione (rimuovi dall'indice, la meno distruttiva) — non
  "la prima tabbable per caso", una scelta esplicita via `initialFocus`
  che il piano vincola.

Entrambi emettono un evento (`confirm` / `choose`) invece di accettare
un callback come il prototipo vanilla-JS: idiomatico Vue, non una
libertà presa a caso. Nuova chiave `ui.dialog.cancel` ("Annulla"/
"Cancel", condivisa dai due) e namespace `ui.deleteDialog.*` (premessa
+ tre coppie etichetta/dettaglio, testo preso parola per parola dal
prototipo, non riassunto).

**Bug trovato scrivendo i test, non nei componenti**: la prima stesura
montava ciascun dialog da solo con una prop `open` statica. `open` è
una prop v-model **obbligatoria**: `defineModel` la sincronizza solo se
un genitore reattivo la riscrive davvero dall'esterno in risposta
all'evento emesso — una prop statica (con o senza un ascoltatore
`onUpdate:open` a vuoto) non è quel genitore, quindi cliccare "Annulla"
non chiudeva mai il dialog nel test (falso negativo, non un difetto del
componente). Corretto con un componente ospite che possiede `open`
come proprio `ref` — stesso schema già usato dal test "v-model:open"
di `Popover.spec.ts`, esteso qui al caso in cui è il *figlio* a
richiedere la chiusura, non il chiamante del test.

**Secondo bug, stessa causa radice**: senza smontare esplicitamente
ogni wrapper (`wrapper.unmount()` in un `afterEach`), il markup
teletrasportato di un test restava nel vero `document.body` per il
test successivo — che poteva trovare (e cliccare) il bottone del test
sbagliato, dando esiti incoerenti da un tentativo all'altro. Lo stesso
`DialogPortal` di reka-ui che rende falsi i test scritti senza questa
accortezza in ogni componente-dialog di questa sessione.

Verifica eseguita:
- `npx vitest run src/components/ui/ConfirmDialog.spec.ts src/
  components/ui/DeleteDialog.spec.ts` → 6/6 verdi (dopo le due
  correzioni sopra): fuoco iniziale su "Annulla"/prima opzione, emit +
  chiusura sulla scelta, chiusura senza emit su "Annulla".
- `npx vitest run` (suite intera) → 172/172 verdi — compreso il test di
  parità delle chiavi `it.json`/`en.json`.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **92.573/153.600** byte gzip
  (script CI): nessuna schermata reale li importa ancora. Margine
  ampio (61.027 byte).

Debiti dichiarati: undici componenti del Task 2 restano da scrivere
(`PhotoTile`, `SelectionBar`, `QuickFilter`, `SelectAllVisible`,
`SuggestionQueue`, `ProvenanceBadge`, `Avatar`, `AppShell`,
`SegmentedControl`, `NavGroup`, `LoadingSkeleton`).

### LoadingSkeleton (SP-27)

Ottavo componente del Task 2. Principio 1 del documento funzionale
sugli stati di caricamento (righe 822-833 del mockup): "il caricamento
non è mai uno spinner al centro del vuoto: è uno scheletro che ha già
la FORMA del contenuto che sta arrivando" — non un rettangolo grigio
generico. Due varianti, entrambe verificate riga per riga contro le
funzioni reali del prototipo (righe 3180-3207), non riassunte:

- **`grid`** (`skelGridHTML`): griglia fotografica giustificata — stessi
  ventiquattro rapporti d'aspetto ciclici misurati sul prototipo (riga
  3184: `1.5, 0.67, 1.5, 1.33...`), non un solo quadrato ripetuto, così
  la griglia scheletro assomiglia davvero a una griglia fotografica.
  `aria-hidden`: decorativa, non ha nulla da annunciare da sola.
- **`stream`** (`streamSkeletonPlaceholderHTML`): due mesi scheletro,
  non uno — il commento del prototipo stesso spiega perché: il ritmo
  "titolo, griglia, titolo, griglia" fa parte di ciò che si sta
  annunciando. Un solo `role="status"` avvolge l'intero blocco (il
  caricamento si annuncia una volta, "Caricamento delle foto in
  corso", non tessera per tessera) mentre le due griglie interne
  restano `aria-hidden` — verificato con un test che conta esattamente
  due elementi `aria-hidden`, non uno spot-check.

**Lo shimmer (`.skel`/`kpx-shimmer`) vive in `style.css`, non nel
componente**: stesso trattamento di `.spinner` per `BusyButton` — un
loop, non una transizione di stato, categoria diversa dalla palette
delle sette durate del Task 1. Aggiunto `--color-skel-sheen` (righe
73/89 del mockup: bianco quasi pieno in chiaro, appena percettibile in
scuro — due valori dichiarati dal prototipo, non un'opacità unica
approssimata). **Nessuna eccezione sotto `prefers-reduced-motion`**, a
differenza dello spinner: la forma dello scheletro comunica già "sta
caricando" da sola, il blocco generale che ferma le animazioni (già in
`style.css` dal Task 1) basta — non serve rallentare invece di
fermare, quel bisogno è specifico dello spinner, che senza rotazione
non comunica più nulla.

Verifica eseguita:
- `npx vitest run src/components/ui/LoadingSkeleton.spec.ts` → 3/3
  verdi: griglia con il conteggio richiesto e `aria-hidden`, rapporti
  d'aspetto che variano davvero (non un solo valore ripetuto), due
  regioni interne nascoste sotto un solo `role="status"` in modalità
  `stream`.
- `npx vitest run` (suite intera) → 176/176 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **92.744/153.600** byte gzip
  (script CI). Margine ampio (60.856 byte).

Debiti dichiarati: dieci componenti del Task 2 restano da scrivere
(`PhotoTile`, `SelectionBar`, `QuickFilter`, `SelectAllVisible`,
`SuggestionQueue`, `ProvenanceBadge`, `Avatar`, `AppShell`,
`SegmentedControl`, `NavGroup`).

### SegmentedControl (SP-24)

Nono componente del Task 2. Verificato riga per riga contro
`.seg-control`/`.seg-option` (es. righe 4441-4455 del mockup, il
gruppo "Pick/Scarta" della modifica in blocco) e la loro unica logica
JS, `wireSegGroup` (riga 4519): un radiogroup — `role="radiogroup"` sul
contenitore, `role="radio"`/`aria-checked`/tabindex roving su ogni
opzione — ma **`wireSegGroup` gestisce solo clic/Invio/Spazio**, nessuna
freccia da nessuna parte nel prototipo. Nota vincolante esplicita del
piano: *"roving tabindex e frecce (il prototipo non le ha)"* — qui le
frecce sono un'aggiunta reale, verificata con un test che controlla sia
l'evento emesso sia il fuoco reale nel DOM dopo `ArrowRight`/`ArrowLeft`
(con avvolgimento ai capi), non trascritta dal prototipo che non le ha.

**"Nei filtri della modifica in blocco include sempre 'Non modificare'"**
(altra nota del piano) non è qualcosa che un controllo generico può
imporre da solo: è una regola per chi *chiama* `SegmentedControl` nella
schermata di modifica in blocco (Tranche successiva, non ancora
scritta) — l'opzione va nell'array `options` passato, il componente
qui si limita a rendere qualunque insieme di opzioni gli venga dato.
Non forzata nel componente, dichiarata come debito verso il chiamante
futuro, non dimenticata.

Verifica eseguita:
- `npx vitest run src/components/ui/SegmentedControl.spec.ts` → 4/4
  verdi: solo l'opzione selezionata è raggiungibile da tab (roving
  tabindex -1 sulle altre), il clic seleziona, `ArrowRight`/`ArrowLeft`
  spostano selezione **e fuoco** avvolgendo ai capi dell'array.
- `npx vitest run` (suite intera) → 181/181 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **92.795/153.600** byte gzip
  (script CI). Margine ampio (60.805 byte).

Debiti dichiarati: nove componenti del Task 2 restano da scrivere
(`PhotoTile`, `SelectionBar`, `QuickFilter`, `SelectAllVisible`,
`SuggestionQueue`, `ProvenanceBadge`, `Avatar`, `AppShell`,
`NavGroup`).

### NavGroup (SP-25)

Decimo componente del Task 2. Verificato riga per riga contro il
gruppo "Manutenzione" della barra laterale (mockup, righe 134-146 per
il CSS, 2485-2536 per la logica) — non il solo nome "NavGroup" nel
piano. Comportamento reale, non riassunto: `maintOpen =
state.navMaintOpen || maintActive` — il gruppo **si apre da solo**
quando la vista corrente è una delle sue sotto-voci (`maintActive`), e
il clic sull'interruttore alterna **solo** `navMaintOpen`, mai
`maintActive` — quindi cliccare mentre la vista corrente è dentro il
gruppo non lo chiude mai, l'OR resta vero comunque. Tradotto in Vue con
un `computed` (`open = manuallyOpen || active`) invece di riprodurre lo
stato piatto del prototipo: stessa garanzia, verificata con un test
dedicato che clicca e controlla che il gruppo resti aperto, non solo
che parta aperto.

Freccia che ruota in `.15s` (nota vincolante del piano, già
`--duration-arrow` nella palette del Task 1 — primo consumo reale del
token, non un valore nuovo). `parent-active` del prototipo (il testo si
scurisce/appesantisce quando una sotto-voce è la vista corrente, senza
sfondo né bordino — quel trattamento resta esclusivo di "sei qui
davvero", commento del prototipo stesso riga 141-144) reso con la prop
`active`.

Verifica eseguita:
- `npx vitest run src/components/ui/NavGroup.spec.ts` → 4/4 verdi:
  chiuso di default, si apre al clic con la freccia che ruota, si apre
  da solo quando `active` è vero senza mai essere stato cliccato, non
  si chiude cliccando mentre `active` resta vero.
- `npx vitest run` (suite intera) → 186/186 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **92.876/153.600** byte gzip
  (script CI). Margine ampio (60.724 byte).

Debiti dichiarati: otto componenti del Task 2 restano da scrivere
(`PhotoTile`, `SelectionBar`, `QuickFilter`, `SelectAllVisible`,
`SuggestionQueue`, `ProvenanceBadge`, `Avatar`, `AppShell`).

### ProvenanceBadge (SP-12)

Undicesimo componente del Task 2. Letta la definizione **canonica** del
pattern, non solo la riga di tabella del piano: documento funzionale
§59 ("Provenienza IA vs utente"), non il mockup direttamente — il
documento è la fonte di verità qui, il mockup ne è solo l'attuazione
in una chip specifica (`.lb-tag-chip`, righe 8718-8746). Principio:
*"un'etichetta proposta dal riconoscimento e una messa da una persona
non sono mai indistinguibili nell'interfaccia, in nessun punto"* — un
principio di prodotto, non un dettaglio visivo.

**Scelta di ambito, dichiarata**: il documento descrive un intero
sistema di tre trattamenti su una chip completa (piena/attenuata con
"IA"/tratteggiata con conferma-rifiuto) che non esiste ancora come
componente condiviso — non è nel piano del Task 2 (né `TagChip` né
`FaceBox` sono nella tabella dei diciotto). `ProvenanceBadge` qui è
**solo il marcatore** ("IA", 9px, peso 700, opacità .8, righe
8729-8734) che quei componenti futuri (chip dei tag, riquadro del
volto, miniatura di Revisione) monteranno al proprio interno — non una
reimplementazione anticipata dell'intera chip, che tocca stato/azioni
non ancora progettati qui.

**La decisione "confermato+umano non mostra nulla" vive nel
componente**, non lasciata al chiamante: `origin: 'ai' | 'human'` come
unica prop, `v-if="origin === 'ai'"` sulla radice — così "mai
indistinguibili, in nessun punto" resta vero per costruzione anche nei
componenti futuri che lo monteranno, invece di dipendere da ognuno
ricordarsi il controllo. Descrizione (`aria-label`/`title`) invece del
solo glifo "IA": una sigla di due lettere da sola non spiega cosa
significhi a chi non conosce già la convenzione.

Verifica eseguita:
- `npx vitest run src/components/ui/ProvenanceBadge.spec.ts` → 2/2
  verdi: non renderizza nulla per `origin: 'human'` (asserzione
  sull'HTML letterale, non solo "non contiene IA" — nessun nodo
  fantasma), mostra "IA" con l'`aria-label` esplicativo per `origin:
  'ai'`.
- `npx vitest run` (suite intera) → 189/189 verdi — parità `it`/`en`
  compresa.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **92.953/153.600** byte gzip
  (script CI). Margine ampio (60.647 byte).

Debiti dichiarati: sette componenti del Task 2 restano da scrivere
(`PhotoTile`, `SelectionBar`, `QuickFilter`, `SelectAllVisible`,
`SuggestionQueue`, `Avatar`, `AppShell`).

**Nota CI (commit `03a60b4`, ToastHost)**: la run è arrivata rossa su
**due** test insieme in `crates/keeppix-db/tests/scale_200k.rs` —
`two_hundred_thousand_assets_keep_timeline_and_search_within_budget`
("conteggi mese: 619.847284ms >= 300ms") e
`timeline_with_fifty_permissions_stays_under_budget_at_200k`
("buckets con 50 permessi: 433.72513ms") — la stessa famiglia di test
di budget temporale già vista rossa una volta su `07ce058` (Dialog) e
lì confermata rumore del runner con un ri-lancio pulito. Questo commit
non tocca `keeppix-db` (solo `frontend/src/**`, `App.vue`, `i18n/
*.json`, `style.css`), e le **sette** push successive con lo stesso
codice backend (Tooltip, BusyButton, ConfirmDialog/DeleteDialog,
LoadingSkeleton, SegmentedControl, NavGroup, ProvenanceBadge) sono
tutte passate pulite sugli stessi test — segnale ulteriore di rumore
condiviso, non una regressione. Ri-lanciati i soli job falliti
(`rerun_failed_jobs`), una volta, prima di continuare.

### Avatar (SP-16)

Dodicesimo componente del Task 2. Verificato contro `.avatar` (righe
220-229 del mockup) e `myAvatarStyle()`/`AVATAR_COLOR_PRESETS` (righe
7164-7183). Il commento del prototipo, non un dettaglio da indovinare:
*"testo sempre bianco, non `--accent-text`... confermato da Giovanni:
bianco su arancione va bene qui, trattato come elemento di marca più
che testo da leggere a lungo"* — quindi il testo delle iniziali resta
`#fff` fisso, mai un colore di contrasto calcolato per tema. Iniziali
con lo stesso algoritmo del prototipo (riga 4373, non un'invenzione):
un carattere per parola del nome, massimo due, maiuscolo.

**Il colore non è deciso dal componente**: è una prop (`color?:
string | null`, `null` = `var(--color-accent)` di default). Il
prototipo usa due fonti diverse per lo stesso markup — `state.
avatarColor` (preferenza dell'utente corrente, una delle otto
`AVATAR_COLOR_PRESETS`) per l'utente corrente, `hsl(u.color,55%,45%)`
hash-based per le altre persone in condivisione — un componente di
sola resa non può sapere quale delle due si applica, quindi entrambe
restano responsabilità del chiamante. **"Sincronizzato ovunque"** (nota
vincolante del piano) è garantito dalla condivisione del componente
stesso: stessa coppia (nome, colore) rende sempre identica ovunque sia
montata — esattamente il ruolo che `myAvatarStyle()` già svolge nel
prototipo con una funzione unica invece di markup duplicato per ogni
punto (sidebar, header mobile, Profilo).

Due sole dimensioni reali, non un rapporto inventato: 28px/12px
(`.avatar` di base — footer utente, sidebar) e 56px/20px (riga 7187,
il grande avatar di Profilo). Verificato che il rapporto fra le due non
sia lineare (12/28 ≈ .429, 20/56 ≈ .357) prima di scegliere due
varianti nominate (`sm`/`lg`) invece di una formula che avrebbe
inventato un numero non presente nel prototipo.

Verifica eseguita:
- `npx vitest run src/components/ui/Avatar.spec.ts` → 8/8 verdi:
  iniziali corrette (anche con più di due parole nel nome), colore di
  sfondo predefinito e quello passato esplicitamente, testo sempre
  bianco, le due dimensioni reali con le loro font-size esatte,
  `aria-label` col nome completo.
- `npx vitest run` (suite intera) → 198/198 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **92.967/153.600** byte gzip
  (script CI). Margine ampio (60.633 byte).

Debiti dichiarati: sei componenti del Task 2 restano da scrivere
(`PhotoTile`, `SelectionBar`, `QuickFilter`, `SelectAllVisible`,
`SuggestionQueue`, `AppShell`).

### AppShell (SP-17) — ambito parziale, dichiarato

Tredicesimo componente del Task 2, **non chiuso per intero**. Letta la
definizione canonica (documento funzionale, sezione "Shell mobile:
header, tab bar in basso, menu account mobile", righe 948-1053): una
shell alternativa completa — barra a quattro schede che instrada su
`state.view` (foto/cerca/album/altro), un titolo per ciascuna delle
diciannove viste possibili, un badge culling legato alla coda reale, un
menu account. Tutto questo dipende dal **router**, che è il Task 3 di
questa stessa Tranche — non ancora scritto. Cablare ora quella logica
avrebbe voluto dire inventare convenzioni di instradamento (nomi di
rotta, stato attivo) che il Task 3 potrebbe smentire — lo stesso rischio
già evitato con `SegmentedControl` (l'opzione "Non modificare" lasciata
al chiamante) e `ProvenanceBadge` (solo il marcatore, non l'intera chip).

**Chiuso qui**: solo il meccanismo che il piano vincola esplicitamente
— *"commuta per larghezza, non per interruttore"*. Il prototipo usa
`state.device`, un interruttore manuale per la demo: `#app.device-
mobile` è una classe statica, mai legata a una larghezza reale del
viewport (`.frame-outer.device-mobile` ha perfino una larghezza fissa
`390px`, la scocca del telefono per la demo, non un breakpoint).
`AppShell.vue` usa invece un vero `window.matchMedia('(max-width:
767px)')` con `addEventListener('change', ...)` — commuta da solo
quando la finestra cambia dimensione, non quando qualcuno preme un
interruttore.

**Debito esplicito sulla soglia**: nessuna cifra di breakpoint esiste
nel documento funzionale né nel mockup — verificato (`grep -i
breakpoint`, `grep larghezza.*mobile`): il documento dice solo "sotto
una certa larghezza", mai un numero. `768px` è il breakpoint `md` di
Tailwind, già lo standard del progetto — non un valore misurato sul
prototipo, che non ne ha uno. Se il Task 3 (router/screens) rivelerà un
numero diverso più corretto, va corretto lì, non assunto qui come
definitivo.

Espone quattro slot (`sidebar`/`topbar` per desktop, `mobile-header`/
`mobile-tabbar` per mobile, più il default per il contenuto) senza
sapere nulla di viste o instradamento — la shell reale (Task 3) li
popolerà.

Verifica eseguita:
- `npx vitest run src/components/ui/AppShell.spec.ts` → 3/3 verdi:
  slot desktop sopra la soglia, slot mobile sotto, commutazione reale
  su un evento `change` di `matchMedia` (non un flag settato a mano)
  — `matchMedia` simulato con lo stesso schema già in uso in
  `MapClusterLayer.spec.ts`.
- `npx vitest run` (suite intera) → 202/202 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **92.967/153.600** byte gzip
  (script CI) — invariato: nessuna schermata reale lo importa ancora.

Debiti dichiarati: **il resto della shell mobile** (tab bar/router,
titoli per vista, badge culling, menu account — Task 3) più cinque
componenti del Task 2 (`PhotoTile`, `SelectionBar`, `QuickFilter`,
`SelectAllVisible`, `SuggestionQueue`).

### SelectionBar + store (SP-2)

Quattordicesimo componente del Task 2. Letta la definizione canonica
(documento funzionale §12, "Selezione multipla e barra azioni"), non
solo la riga del piano. Nota vincolante: **due pool di selezione
distinti e paralleli** — libreria (usata da Timeline/Preferiti/Cerca/
Album/Persona) e lotto di culling, con comandi propri e senza Album/
Elimina — *"non si parlano e non si azzerano a vicenda"*.

Nuovo `stores/selection.ts`: due pool costruiti dalla stessa funzione
(`createSelectionPool`), ma due chiusure separate — nessuno stato
condiviso per costruzione, non un solo pool con un flag di contesto.
Verificato con test che toccano un pool e controllano che l'altro resti
intonso, non solo che esistano due proprietà distinte.
`selectAllVisible(visibleIds)` implementa il toggle di gruppo del
documento (§12.3): se tutto il visibile passato è già selezionato lo
toglie, altrimenti lo aggiunge **senza** rimuovere selezioni fatte
altrove — provato con un test che seleziona qualcosa fuori dal
visibile e controlla che sopravviva.

**Bug trovato prima di scrivere il markup, non durante i test**: il
documento (§12.2) descrive la regione d'annuncio (`aria-live="polite"
aria-atomic="true"`) come un nodo **a sé**, fuori schermo — non dentro
`.selection-bar`. La barra stessa **sparisce del tutto** a zero
selezionate ("a zero la modalità si spegne da sola", §12.7). Se la
regione vivesse solo dentro il markup che sparisce, l'annuncio
"Selezione annullata" non potrebbe mai scattare: la regione
sparirebbe nello stesso istante in cui dovrebbe annunciare. Corretto
prima di scrivere il template sbagliato: la radice del componente
resta sempre montata, solo il contenuto visibile della barra si
nasconde internamente sotto `count` — il chiamante non deve mai
`v-if`rlo, esattamente come già fa con `ToastHost`.

**Ambito dichiarato**: i pulsanti di azione restano fuori dal
componente. La libreria ne ha cinque (Preferiti/Album/Condividi/
Modifica/Elimina, righe 2176-2182 del mockup), il culling tre (Scelta/
Scarta/Rinomina…, righe 5002-5004) — icone ed etichette completamente
diverse, e Album/Condividi aprono dialog che non esistono ancora come
componenti condivisi (selettore album, `SharePanel` non è ancora
integrato in questo contesto). Il chiamante li compone nello slot di
default con `Tooltip`+`BusyButton`, già pronti da questo stesso Task —
non pulsanti reinventati una terza volta.

Verifica eseguita:
- `npx vitest run src/stores/selection.spec.ts src/components/ui/
  SelectionBar.spec.ts` → 17/17 verdi: i due pool restano indipendenti
  in entrambe le direzioni, `toggle`/`clear`/`selectAllVisible` con la
  semantica esatta del documento, barra nascosta a zero selezionate ma
  regione d'annuncio sempre montata, conteggio singolare/plurale,
  l'etichetta "Seleziona tutte" non cambia mai, gli eventi `clear`/
  `select-all` emessi, lo slot di default per i pulsanti del
  chiamante, l'annuncio esatto in italiano per selezione e
  annullamento.
- `npx vitest run` (suite intera) → 220/220 verdi — parità `it`/`en`
  compresa.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **93.197/153.600** byte gzip
  (script CI). Margine ampio (60.403 byte).

Debiti dichiarati: il resto della shell mobile (Task 3) più quattro
componenti del Task 2 (`PhotoTile`, `QuickFilter`, `SelectAllVisible`,
`SuggestionQueue`).

### SelectAllVisible (SP-4)

Quindicesimo componente del Task 2. Definizione canonica letta per
intero, non solo la riga del piano: seleziona **esattamente ciò che è
visibile in quel momento**, mai l'intera libreria sottostante — se un
filtro rapido o una ricerca sono attivi, solo ciò che ci ricade dentro.
Il documento distingue esplicitamente due insiemi che l'implementazione
deve tenere separati: quello *di partenza* della vista e quello
*effettivamente mostrato*. Questo componente non conosce nessuno dei
due: riceve solo `visibleCount` (per decidere se mostrarsi) ed emette
`select-all` — il vero insieme da selezionare va allo stesso
`store.selection.*.selectAllVisible(visibleIds)` già costruito per
SP-2, che implementa la semantica di toggle corretta.

**"Scompare quando non c'è nulla, non si disabilita"** (nota
vincolante del piano, ripresa quasi identica nel documento): nessuna
variante disabilitata da disegnare — `v-if="visibleCount > 0"` sulla
radice, stessa disciplina già applicata a `SelectionBar`.

**Due etichette diverse, non la stessa ripetuta**: il tooltip dice
"Seleziona tutto" (SP-7, breve, per chi vede), l'`aria-label` dice
"Seleziona tutto quello che vedi" (documento, riga 10249-10251, **più
esplicito** apposta per chi non vede il contesto visivo) — verificato
con un test che controlla entrambi i testi separatamente, non un solo
valore condiviso. Icona esatta dal prototipo (`selectAll`, riga 1509:
un quadrato con spunta dentro), non un glifo generico.

Verifica eseguita:
- `npx vitest run src/components/ui/SelectAllVisible.spec.ts` → 4/4
  verdi: sparisce a zero visibili, appare da almeno uno, le due
  etichette esatte e distinte, l'evento `select-all` emesso al clic.
- `npx vitest run` (suite intera) → 225/225 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **93.260/153.600** byte gzip
  (script CI). Margine ampio (60.340 byte).

Debiti dichiarati: il resto della shell mobile (Task 3) più tre
componenti del Task 2 (`PhotoTile`, `QuickFilter`, `SuggestionQueue`).

### QuickFilter (SP-3) — ambito parziale, dichiarato

Sedicesimo componente del Task 2. Letta la definizione canonica per
intero (documento funzionale §11): un pannello a sei sezioni di chip
(Tipo/Persone/Tag/Categorie/Fotocamera/Luogo) che restringe la griglia
sul momento, OR dentro una dimensione e AND fra dimensioni diverse
(`photoMatchesBrowseFilters`).

**Sopra `Popover.vue`, non una reimplementazione**: apri/chiudi per
clic, click fuori chiude, Esc chiude — lo stesso contratto già
garantito da reka-ui per SP-14, nessun gestore scritto qui. Il
pulsante a imbuto **non** ha tooltip (§11.4, nota esplicita del
documento: *"a differenza di 'Seleziona tutto', il pulsante del
filtro non ha `data-tip`, ha solo `aria-label`"*) — niente `Tooltip`
qui, a differenza di `SelectAllVisible`.

**Ambito dichiarato**: il componente è generico rispetto alle
dimensioni — riceve un array `{id, label, options}`, non conosce
Persone/Tag/Categorie/Fotocamera/Luogo come concetti hardcoded. Le sei
dimensioni reali dipendono da store che non esistono ancora in questa
sessione (persone, tag, fotocamere, cartelle); la schermata che le
userà davvero le costruirà dalle proprie fonti dati quando quegli
store esisteranno — stesso principio già applicato ad `AppShell`
(Task 3) e a `SelectionBar` (i cinque/tre pulsanti d'azione).

**La logica di combinazione vive fuori dal componente**, pura e
testabile senza un modello di foto reale: nuovo `design/quickFilter.ts`
(`activeFilterCount`, `matchesFilters`) — ogni dimensione espone un
`getValues(item) => string[]` (un array anche per un campo a valore
singolo, come la fotocamera), così l'OR-dentro-la-dimensione si esprime
allo stesso modo per campi singoli e multipli senza casi speciali; una
dimensione disattivata (es. "Persone" a riconoscimento volti spento)
restituisce sempre `[]`, riproducendo lo `return false` secco del
documento senza logica dedicata nel confronto — provato con un test
dedicato, non solo dedotto.

**Comportamento del campo di ricerca** (§11.3, compare solo oltre 8
opzioni — `BROWSE_FILTER_SEARCH_THRESHOLD`): implementato fedelmente,
compreso *"le opzioni già selezionate restano sempre in cima e non
vengono mai filtrate via"* — testato scrivendo un termine che non
corrisponde più e verificando che l'opzione già scelta resti visibile,
non dedotto dalla sola lettura del testo.

**Non ancora nel Task 2** (debito esplicito, non un'omissione): la
regola "digitare non ridisegna tutto il pannello, solo la riga di chip
della sezione" (§11.3) è un'ottimizzazione del DOM manuale del
prototipo — irrilevante nel modello reattivo di Vue, che ridisegna solo
i nodi che cambiano davvero per costruzione; non c'è nulla da
replicare qui.

Verifica eseguita:
- `npx vitest run src/design/quickFilter.spec.ts` → 6/6 verdi:
  conteggio somma su tutte le dimensioni, OR dentro una dimensione, AND
  fra dimensioni, dimensione disattivata come falso secco.
- `npx vitest run src/components/ui/QuickFilter.spec.ts` → 8/8 verdi:
  pallino assente a zero attivi e corretto quando presente, pannello
  che si apre ed elenca le chip, clic che emette la selezione
  aggiornata, "Cancella tutto" solo quando c'è qualcosa da cancellare,
  campo di ricerca solo oltre 8 opzioni con il placeholder dinamico
  esatto, selezione mai filtrata via dalla ricerca, piede che
  distingue "totale" da "con questi filtri".
- `npx vitest run` (suite intera) → 240/240 verdi — parità `it`/`en`
  compresa.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **93.646/153.600** byte gzip
  (script CI). Margine ampio (59.954 byte).

Debiti dichiarati: il resto della shell mobile (Task 3) più due
componenti del Task 2 (`PhotoTile`, `SuggestionQueue`).

### PhotoTile (SP-1 + SP-15)

Diciassettesimo componente del Task 2 — il mattone di ogni vista a
griglia (Foto, Preferiti, Album, dettaglio Persona, Cerca). Definizione
canonica letta per intero (documento funzionale §10), non solo la riga
del piano. *"Nient'altro"* (§10.2): niente nome file, data, stelle,
pick/scarta, "in album" sulla tessera — quell'informazione vive solo
nell'etichetta accessibile e nel lightbox; il componente non aggiunge
markup oltre a quanto il documento elenca (miniatura, badge RAW,
cerchietto, cuoricino).

**Tre stop di tabulazione, in ordine**: apri → cerchietto → cuoricino
— lo stesso ordine del markup, con `<button>` reali invece dei `<div
role="button">` del prototipo (Invio/Spazio funzionano di serie, non
serve ricablarli a mano come `bindActivatable`).

**Bug trovato scrivendo il template, non i test**: la prima stesura
faceva apparire cerchietto/cuoricino/badge solo al passaggio del mouse
**sul singolo bottone**, mentre il documento (§10.7) li vuole visibili
al passaggio su **tutta la tessera** (`.tile:hover .tile-check`, non
`.tile-check:hover`) — corretto con un `group` sulla radice e
`group-hover`/`group-focus-within` sui tre elementi, non l'hover
individuale che avevo scritto per primo. Lo stesso schema copre anche
il badge RAW, che deve **sparire** all'hover/focus (cede il posto ai
comandi, commento del prototipo: badge e cerchietto sullo stesso
angolo si sovrappongono).

**Il cerchietto resta visibile su tutte le tessere quando la selezione
è attiva** (`#app.selection-active` nel prototipo, righe 1142-1145),
non solo su quella sotto il mouse — reso con la prop `selectionMode`
già presente per decidere se mostrare il cuoricino.

**Tocco prolungato** (§10.4, 500ms + vibrazione 15ms): usa
`LONG_PRESS_THRESHOLD_MS`/`LONG_PRESS_VIBRATE_MS`, già in `design/
tokens.ts` dal Task 1 — primo consumo reale di entrambi. **Non
attivo di default**: una prop `enableLongPress` che il chiamante
imposta in base al proprio `AppShell.isMobile` — questo componente non
reimplementa `matchMedia`, lo fa già `AppShell`. Il click sintetico
dopo il rilascio è soppresso (`suppressNextClick`), stesso
`_suppressClick` del prototipo — testato con `vi.useFakeTimers()`:
tocco di 500ms seleziona e sopprime il click che segue; un rilascio
prima dei 500ms annulla il tocco prolungato e il tap normale apre
comunque.

**RAW/RAW+JPEG** (SP-15): stessa logica di `rawBadgeLabel` nel
prototipo (riga 4095) — nessun badge per il solo JPEG. `dateLabel` è
una prop già formattata dal chiamante, non l'anno fisso "2026" del
prototipo (una costante di demo, non un formato da riprodurre su dati
reali).

Verifica eseguita:
- `npx vitest run src/components/ui/PhotoTile.spec.ts` → 10/10 verdi:
  apre fuori selezione, seleziona invece di aprire dentro selezione,
  etichetta accessibile con/senza suffisso preferita, badge RAW/RAW
  +JPEG/nessuno per i tre `stackType`, cerchietto riflette lo stato ed
  emette senza aprire, cuoricino assente durante la selezione ed
  emette senza aprire, tocco prolungato di 500ms seleziona e sopprime
  il click successivo, un rilascio anticipato annulla il tocco e il
  tap normale apre, il tocco prolungato resta inerte se il chiamante
  non lo attiva.
- `npx vitest run` (suite intera) → 251/251 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito.
- `npm run build` → bundle iniziale **94.072/153.600** byte gzip
  (script CI). Margine ampio (59.528 byte).

Debiti dichiarati: il resto della shell mobile (Task 3) più un
componente del Task 2 (`SuggestionQueue`).

### SuggestionQueue (SP-10)

Diciottesimo e ultimo componente del Task 2. Letta la definizione
canonica per intero (documento funzionale §56, "Revisione — tag"), non
solo il rimando `renderRevisione`. Un gruppo di proposte IA per **un**
tag o una persona, in attesa di conferma o rifiuto — mai applicate da
sole. Nota vincolante del piano: **"tag e volti, stessa forma"**.

**La forma condivisa, resa letteralmente condivisa**: un solo
componente per entrambe le code, non due quasi identici. L'unica
differenza reale fra i due domini (§56.6: i volti hanno un terzo
pulsante per miniatura, "Non è un volto", a fondo `--danger` pieno —
non uno dei due normali) è esposta come **slot con ambito**
(`extra-actions`, con l'`id` della miniatura), non una prop specifica
per un dominio che l'altro non condivide — verificato con un test che
passa un pulsante finto nello slot e controlla che riceva l'id giusto.

**Pallino colore solo per i tag** (§56.2: le persone non ce l'hanno) —
prop `color?` opzionale, non renderizzata quando assente, verificato
con due montaggi distinti (con e senza colore) invece di dedurlo dalla
sola lettura del template.

Testo esatto dal documento, non parafrasato: il nome del gruppo fra
virgolette basse (`«Paesaggi»`, non virgolette dritte), "N proposta/e"
con l'accordo singolare/plurale corretto (prima chiave di questo
componente a usare il plurale nativo di vue-i18n dopo `ui.toast.*` e
`ui.selectionBar.*`). Overlay delle azioni nascosto finché non c'è
hover **o** `:focus-within` sulla miniatura (§56.6) — stesso schema
`group`/`group-hover`/`group-focus-within` già usato per `PhotoTile`.

**Approssimazione dichiarata**: il badge "IA" della miniatura usa
`bg-accent/20 text-accent` (l'accento esistente a opacità ridotta), non
un token `--accent-tint-strong` dedicato che il prototipo ha ma che non
esiste ancora nel nostro `@theme` — stesso trattamento già usato altrove
per tinte di sfondo derivate (es. `hover:bg-border/40` in `NavGroup`),
non un nuovo token introdotto per un solo badge.

Verifica eseguita:
- `npx vitest run src/components/ui/SuggestionQueue.spec.ts` → 6/6
  verdi: nome fra virgolette basse e conteggio singolare/plurale
  corretto, pallino colore presente solo per i tag, `confirm-all`/
  `reject-all` emessi dai pulsanti di gruppo, conferma/rifiuto di una
  singola miniatura emette l'id giusto, una miniatura per proposta con
  il badge "IA", lo slot `extra-actions` riceve l'id per il terzo
  pulsante dei volti.
- `npx vitest run` (suite intera) → 258/258 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` → pulito (dopo un `--fix` per l'ordine di un attributo).
- `npm run build` → bundle iniziale **94.270/153.600** byte gzip
  (script CI). Margine ampio (59.330 byte).

## Task 2 — chiuso

Diciotto pattern condivisi su diciotto: `Dialog`, `Popover`,
`ToastHost`, `Tooltip`, `BusyButton`, `ConfirmDialog`, `DeleteDialog`,
`LoadingSkeleton`, `SegmentedControl`, `NavGroup`, `ProvenanceBadge`,
`Avatar`, `AppShell` (ambito parziale, dichiarato), `SelectionBar` +
store, `SelectAllVisible`, `QuickFilter` (ambito parziale, dichiarato),
`PhotoTile`, `SuggestionQueue` — più `RatingStars`, già esistente e
riusato, non riscritto.

Due debiti espliciti verso il Task 3 (router, non ancora scritto),
dichiarati al momento di ogni commit, non scoperti ora: la shell
mobile completa (barra a schede instradata su viste reali, titoli per
vista, badge culling, menu account — `AppShell` chiude solo la
commutazione desktop/mobile) e le sei dimensioni reali del filtro
rapido (Persone/Tag/Categorie/Fotocamera/Luogo/Tipo, che dipendono da
store non ancora costruiti — `QuickFilter` chiude solo il meccanismo
generico e la logica di combinazione). Nessun altro debito silenzioso:
ogni scelta di ambito è stata dichiarata nel commit e in questo ledger
al momento in cui è stata presa.

Ogni componente verificato riga per riga contro il documento funzionale
e/o il mockup (mai il solo riassunto del piano), con un test reale per
ogni comportamento vincolante, e con la stessa disciplina di
verifica locale ripetuta diciotto volte: `vitest` sul file nuovo,
`vitest` sulla suite intera, `vue-tsc -b`, `eslint`, `npm run build` +
il ricalcolo manuale del budget del bundle con lo stesso script della
CI. Nessuna riga di prodotto reale importa ancora questi componenti —
il Task 3 (router) e le Tranche successive sono ciò che li metterà al
lavoro.

## Task 3 — router (ambito parziale, dichiarato)

Il piano nomina il Task 3 come "router: rotte per cartella/album/
persona/lotto di scarto, deep-link della foto nel lightbox, ripristino
scorrimento". Prima di scrivere codice ho verificato con un agente
Explore lo stato reale del frontend: **nessuna** delle quattro rotte
di dettaglio è oggi costruibile.

- Non esiste uno store Pinia per cartelle/album/persone (solo
  `FoldersView.vue`, un albero piatto senza vista di dettaglio per
  singola cartella; `AlbumsView.vue` senza vista di dettaglio album).
- Non esiste alcuna funzionalità volti nel frontend (store, vista, o
  componente) — prerequisito del Task A, non ancora iniziato.
- Culling non ha un concetto di "lotto" instradabile: lo store riceve
  un elenco di risorse ad-hoc dalla vista chiamante, non un id di lotto
  che una rotta potrebbe portare.

Costruire quelle quattro rotte ora significherebbe instradare verso
schermate che non esistono, con uno store fittizio dietro — esattamente
il tipo di debito silenzioso vietato dal mandato. Ho quindi ristretto
il Task 3, a questo giro, a ciò che è genuinamente costruibile con lo
stato attuale dell'app: **deep-link della foto nel lightbox** e
**ripristino della posizione di scorrimento**. Le quattro rotte di
dettaglio restano un debito esplicito verso le Tranche successive
(quando le store corrispondenti esisteranno), non un buco scoperto ora.

### Deep-link della foto (`?photo=`)

Il documento funzionale non descrive alcun comportamento di URL/
cronologia per il prototipo (§7 conferma: zero deep-link, zero
ripristino scorrimento) — è una scelta di design nuova per la
riscrittura, non una riproduzione di un comportamento esistente.

Composable nuovo `useLightboxRoute.ts`: rende `?photo=<id>` l'unica
fonte di verità su "quale risorsa è aperta nel visore", con:
- `open(asset)` → `router.push` (così Indietro chiude il visore);
- `step(asset)` (scorrimento fra risorse a visore già aperto) →
  `router.replace` (nessun affollamento della cronologia per ogni
  freccia premuta);
- `close()` → `router.back()` se il visore è stato aperto in questa
  sessione via `open()` (tracciato con un flag locale), altrimenti
  `router.replace` che toglie solo la chiave `photo` — un ricaricamento
  o link diretto non ha una "nostra" voce di cronologia da far
  scomparire, e `back()` lì uscirebbe dall'app.
- un `watch(() => route.query.photo, ..., { immediate: true })` è
  l'unico punto di sincronizzazione: cerca prima fra le risorse già
  in memoria (`findLocal`, sincrono) poi, se assente, carica da rete
  (`loadRemote`, asincrono) — copre sia il click su una miniatura sia
  il ricaricamento diretto su un URL con `?photo=`.

Cablato in `TimelineView.vue` (sostituendo il precedente
`ref<TimelineAsset|null>` isolato) e in `SearchView.vue` (che non
aveva alcun visore instradato prima), con `maps.loadAsset(id)` —
in realtà `GET /api/v1/assets/:id`, generico nonostante il nome dello
store, già riusato altrove — come `loadRemote` in entrambi i casi.

Bug reale trovato e corretto prima che arrivasse in produzione (non
via test, per ispezione): in `TimelineView.vue` avevo dichiarato
`const lightbox = useLightboxRoute(...)` vicino all'inizio dello
script, prima di `const flatAssets = computed(...)`. Poiché il watcher
interno del composable è `immediate: true`, un `?photo=` già presente
nell'URL al montaggio (il caso del ricaricamento) avrebbe chiamato
`findLocal` in modo sincrono durante il `setup()`, leggendo
`flatAssets.value` prima che fosse inizializzata → `ReferenceError`
da temporal dead zone. Corretto spostando `lightbox` subito dopo la
dichiarazione di `flatAssets`.

Verifica eseguita:
- `useLightboxRoute.spec.ts` (nuovo) → 7/7 verdi: `open` fa `push`,
  `step` fa `replace`, `close` dopo un `open` in sessione fa `back`,
  `close` senza una "nostra" voce di cronologia (avvio diretto su
  `?photo=`) toglie solo `photo` con `replace`, il watcher immediato
  trova la risorsa in locale quando presente e altrimenti la carica da
  remoto, un id non raggiungibile via `findLocal` va comunque a
  `loadRemote`.
  Gotcha di test reale incontrato: `router.isReady()` risolve solo la
  navigazione iniziale del router, non le `push`/`replace` successive —
  le asserzioni vanno fatte attendendo direttamente la promise
  restituita da `open`/`step`/`close` (per questo ora la restituiscono,
  non più `void`); `router.back()` in particolare non si assesta entro
  un singolo `await`+`nextTick`, serve un tick aggiuntivo
  (`setTimeout(0)`).
- `TimelineView.spec.ts` (esteso, +3 test) → 9/9 verdi: click apre
  `?photo=`, ricaricamento su `?photo=` ripristina il visore via
  `loadRemote`, chiusura toglie `photo`. Gotcha jsdom incontrato:
  `HTMLElement.clientWidth` è sempre `0` in jsdom (nessun motore di
  layout reale), il che affama l'algoritmo a griglia giustificata di
  qualunque larghezza utile e produce zero righe renderizzate — nessun
  test in questo file aveva mai cliccato una tessera reale prima.
  Corretto con un helper `stubGridWidth(px)` che sovrascrive
  `HTMLElement.prototype.clientWidth` via `Object.defineProperty`,
  applicato solo dove serve cliccare una tessera vera. Una premessa
  sbagliata scoperta e corretta in corsa: un test voleva verificare che
  il ricaricamento riusasse la pagina già caricata senza un fetch
  aggiuntivo, ma poiché il watcher immediato scatta prima che il
  caricamento asincrono di `onMounted` sia completato, `findLocal` è
  *sempre* vuoto in un ricaricamento reale con questa architettura —
  non un bug, un fatto strutturale. Consolidato in un unico test
  corretto che verifica che `loadRemote`/`apiFetch` **sia** chiamato.
- `SearchView.spec.ts` (nuovo — nessuno spec esisteva prima per questa
  vista) → 3/3 verdi: click apre `?photo=` mantenendo `?q=` coesistente,
  ricaricamento su `?photo=` ripristina via `loadRemote`, chiusura
  toglie `photo` mantenendo `q`.

### Ripristino della posizione di scorrimento

`scrollBehavior` nativo di vue-router aggiunto a `router.ts` per
completezza, ma **da solo è un no-op totale** per questa app: `html,
body, #app { height: 100% }` in `style.css` significa che `window`/
`document` non scorrono mai — ogni vista gestisce una propria regione
interna `overflow-auto`. Verificato per ispezione diretta del CSS
prima di considerare il compito chiuso, non assunto dal solo
aggiungere l'opzione nativa.

Composable nuovo `useScrollRestoration.ts`: una `Map<string, number>`
a livello di modulo, indicizzata per rotta (chiave di default
`route.fullPath`, sovrascrivibile — pensata per un futuro "lotto di
culling" dove più istanze della stessa vista potrebbero convivere sotto
chiavi distinte). Salva su `onBeforeUnmount`, ripristina su
`onMounted`. Non c'è `<KeepAlive>` in `router.ts`, quindi il nodo DOM
scrollabile viene distrutto e ricreato da zero ad ogni navigazione — da
qui la necessità di una cache esplicita per chiave invece di un
semplice "ricordati dov'eri" sull'istanza del componente.

Due bug reali trovati e corretti, il secondo con una diagnosi non
banale:
- Ordine di unmount di Vue: i ref del template vengono azzerati
  **prima** che `onUnmounted` sia invocato (ordine corretto:
  `onBeforeUnmount` → rimozione dal DOM → azzeramento dei ref →
  `onUnmounted`) — la guardia `if (el.value)` in `save()` falliva
  silenziosamente con `onUnmounted`. Corretto usando `onBeforeUnmount`.
- Il test di ripristino continuava a fallire (`expected +0 to be 456`)
  anche dopo quella correzione. Diagnosticato con logging temporaneo
  inline (non uno script isolato fuori dalle root di vitest, tentativo
  precedente fallito perché `/tmp` è fuori dagli `include` del
  progetto): il fallimento non era nel composable ma nel test stesso.
  vue-router applica una patch a `app.unmount()` per azzerare
  `currentRoute.value` a `START_LOCATION` (rotta `/`) quando l'**ultima**
  app che usa quel router viene smontata (pulizia interna contro le
  perdite di memoria, vedi `installedApps` in `vue-router.cjs`). Il
  test smontava l'intera app Vue (`first.unmount()`) e ne montava una
  seconda con lo stesso router — un artefatto di questo stile di test,
  non un comportamento di una SPA reale (dove `app` non viene mai
  smontata, solo i componenti al suo interno cambiano con la
  navigazione). Corretto ripetendo `await router.push(...)` fra i due
  mount, per rispecchiare l'ordine reale (la rotta è già quella di
  destinazione quando il nuovo componente monta).

Cablato in `TimelineView.vue` su `gridEl`, l'unico contenitore con
scorrimento interno sostanziale fra le viste toccate finora.
`SearchView.vue` non ne ha uno scorrimento tanto significativo da
giustificare il cablaggio in questo giro — lasciato per quando quella
vista crescerà.

Verifica eseguita:
- `useScrollRestoration.spec.ts` (nuovo) → 3/3 verdi: ripristina la
  posizione su un nuovo elemento DOM per la stessa rotta, una rotta
  senza posizione salvata riparte dall'alto, una chiave esplicita tiene
  posizioni distinte anche per la stessa rotta.
- `npx vitest run` (suite intera) → 48 file, 274/274 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/modificati → pulito (solo due warning
  attesi `vue/one-component-per-file` nello spec di
  `useScrollRestoration`, per gli host `defineComponent` inline dei
  test — stesso schema già accettato altrove in questa serie).
- `npm run build` → bundle iniziale **94.303/153.600** byte gzip
  (script CI, ricalcolo manuale identico). Margine ampio (59.297 byte).

Debiti espliciti verso le Tranche successive: le quattro rotte di
dettaglio (cartella/album/persona/lotto di scarto) restano non
costruite, in attesa degli store corrispondenti; `useScrollRestoration`
non è ancora cablato in `SearchView.vue` né nelle altre viste con
scorrimento interno che verranno toccate più avanti.

Anche: nota nel ledger sulla CI rossa del commit e5bbdf9 (QuickFilter,
run 255) — `listing_twenty_libraries_stays_within_budget` in
`crates/keeppix-api/tests/budgets.rs` fallito a 112.6ms contro un
budget di 100ms, stessa famiglia di rumore del timing già vista su
07ce058 e 03a60b4, non toccata da un diff che tocca solo
`frontend/src/{components/ui/QuickFilter.*,design/quickFilter.*,
i18n/*.json}`. Ri-lanciato il job fallito una volta.

## Task 4 — la timeline a scala reale (in corso)

"Il calcolo più delicato dell'intera fase" (piano, apertura del Task 4).
Prima di scrivere codice, letto §66 e §8 del documento funzionale
(non solo il riassunto del piano) più il Ruling §3 della spec
fase-11-interfaccia.md. Tre fatti concreti trovati lì che cambiano cosa
va scritto, non solo come:

- **"Nessun raggruppamento per giorno"** (§8, testuale): le foto si
  raggruppano solo per mese. L'implementazione pre-Fase-11 di
  `TimelineView.vue` raggruppa invece per giorno dentro ogni mese
  (`days`/`daysByMonth`, con un `<h3>` per giornata) — un
  comportamento che *non* è nella definizione canonica e va tolto nella
  riscrittura, non portato avanti.
- **L'intestazione di mese non è appiccicata durante lo scroll** (§8:
  "le `.month-head` scorrono via normalmente") — l'implementazione
  attuale ha `sticky top-0` sull'`<h2>` di ogni mese, altro
  comportamento da correggere, non da preservare.
- Lo scrubber dei mesi del prototipo non è raggiungibile da tastiera
  (§8.3, esplicito) — il piano lo marca come correzione voluta:
  "Da rendere raggiungibile da tastiera, cosa che il prototipo non fa."

`GET /timeline/geometry` (già costruito in Fase 10,
`crates/keeppix-api/src/routes/timeline.rs::encode_geometry`) verificato
alla fonte prima di scrivere il decoder, non assunto dal solo commento
del piano: intestazione da 8 byte (versione u32, conteggio u32) + N
record da 6 byte (w:u16, h:u16, month:u16=anno*12+mese), little-endian,
senza id — "le tessere vere arrivano dalle pagine, nello stesso
ordine" (commento del backend). Confermato anche l'ordinamento:
`buckets`/`geometry`/`/timeline?bucket=` usano tutti
`ORDER BY taken_at_utc DESC, id DESC` (o `month DESC` per i bucket) —
lo stream di geometria e la concatenazione delle pagine sono nello
stesso ordine per costruzione, non per convenzione da mantenere a mano.

### Primo passo: il decoder della geometria + il fetch binario

`timeline/geometry.ts` (nuovo): classe `TimelineGeometry` sopra un
`DataView` grezzo — mai 214.000 oggetti `{w,h,month}` (Ruling §3 della
spec: ~50 MB di heap contro 4,7 MB senza spazzatura). ~35 righe,
vicino alle "~30 righe" della spec. Versione del formato verificata e
rifiutata esplicitamente se sconosciuta (`UnsupportedGeometryFormatError`)
invece di leggere byte a caso — lo stesso principio che il backend
dichiara nel proprio commento su `GEOMETRY_FORMAT_VERSION`.

`api/timeline.ts`: `fetchGeometry(bbox?, etag?)`, un fetch binario che
non può passare da `apiFetch` (quella chiama sempre `.json()`).
Refactoring minimo e condiviso: estratta da `apiFetch` la funzione
`throwProblem(response)` che riconosce lo stesso corpo
`application/problem+json` su errore — usata da entrambe, non
duplicata. Supporta `If-None-Match`/`304` (il backend lo implementa
esplicitamente per evitare di riscaricare ~4,7 MB per una vista
invariata) restituendo `{buffer: null, etag}` al chiamante.

Verifica eseguita:
- `geometry.spec.ts` (nuovo) → 4/4 verdi: un buffer sintetico con lo
  stesso layout esatto di `encode_geometry` (non un formato inventato)
  decodifica conteggio e ogni campo per record, incluso un record a
  zero (sizing non ancora arrivato, `saturating_u16`) e il tetto
  `u16::MAX`; una versione di formato sconosciuta viene rifiutata
  invece di essere letta.
- `api/timeline.spec.ts` (nuovo) → 4/4 verdi: 200 restituisce buffer +
  etag, `bbox`/`If-None-Match` passati correttamente, 304 restituisce
  `buffer: null` senza corpo, un errore `problem+json` lancia
  `ApiProblem` — stesso comportamento di `apiFetch` verificato allo
  stesso modo del suo spec esistente.
- `npx vitest run` (suite intera) → 50 file, 282/282 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/modificati → pulito.
- `npm run build` → bundle iniziale **94.318/153.600** byte gzip
  (script CI). Margine ampio (59.282 byte) — nessun consumatore reale
  ancora, solo il proprio test.

Debito dichiarato, non silenzioso: il decoder e il fetch non sono
ancora usati da `TimelineView.vue`. Restano da scrivere, in ordine:
il virtualizzatore (somme prefisse + ricerca binaria, ~120 righe),
la cache LRU delle pagine, e la riscrittura di `TimelineView.vue`
(raggruppamento solo per mese, niente sticky, virtualizzazione,
`ResizeObserver`, scrubber con tastiera e sincronizzazione inversa) —
ciascuno un passo separato con la propria verifica, stesso ritmo dei
diciotto componenti del Task 2. Task 5 (le tre macchine a stati:
scheletro/errore) resta esplicitamente fuori da questo task, per
confine dichiarato dal piano stesso, non per dimenticanza.

### Secondo passo: virtualizzatore + cache LRU delle pagine

`timeline/virtualize.ts` (nuovo): `RowVirtualizer`, ~95 righe, senza
dipendenze — somme prefisse delle altezze di riga in un `Float64Array`
più ricerca binaria su `scrollTop`, esattamente la Ruling §2 della
spec ("~120 righe, nessuna libreria"). Agnostico rispetto a cosa sia
una riga (griglia di foto o intestazione di mese): riceve solo un
array di altezze.

Bug reale trovato scrivendo il test di confronto con una scansione
lineare, non assunto corretto dal solo "sembra ovvio": su un confine
esatto fra due righe (`to` uguale esattamente a un `rowTop`), la prima
versione includeva una riga di troppo. Il motivo: `rowAtOffset(y)`
risponde "quale riga contiene il punto `y`" con la convenzione a
intervallo semiaperto `[top, bottom)` — su un confine esatto la
risposta è la riga che *comincia* lì, non quella che finisce lì — ma
usare quella riga con un `+1` come limite superiore dell'intervallo
visibile la includeva comunque, anche se il suo intervallo non si
sovrappone davvero a `[…, to)`. Corretto con una seconda ricerca
binaria dedicata (`firstRowStartingAtOrAfter`) che risponde alla
domanda giusta per un limite superiore esclusivo: la prima riga che
*comincia* a `to` o oltre.

`timeline/pageCache.ts` (nuovo): `LruPageCache<K,V>`, sopra l'ordine di
inserimento nativo di `Map` (che `get`/`set` di una chiave esistente
sposta in fondo) invece di una lista doppiamente concatenata scritta a
mano — meno codice per lo stesso invariante. Tetto esplicito sul
*numero di pagine*, non sul numero di asset (piano §4.8): la chiave
prevista è il mese del bucket, lo stesso livello di granularità già
usato da `assetsByBucket` in `TimelineView.vue` — non le singole
pagine cursor-based di `/timeline?bucket=`, che quella vista già
concatena per intero prima di considerare un mese "caricato".

Verifica eseguita:
- `virtualize.spec.ts` (nuovo) → 8/8 verdi: altezza totale corretta
  prima di montare qualunque riga, `rowTop` come somma cumulativa,
  intervallo visibile confrontato contro una scansione lineare per
  sette posizioni di scroll diverse (il test che ha trovato il bug di
  cui sopra), overscan che estende senza uscire dai limiti,
  scorrimento oltre la fine che resta sull'ultima riga valida. Test di
  scala del piano: 200.000 righe di altezza *variabile* (non costante,
  per non testare un caso fortunato), il numero di righe montate resta
  sotto una soglia esplicita (40) per cinque posizioni di scroll,
  incluso l'inizio, la fine e tre punti intermedi.
- `pageCache.spec.ts` (nuovo) → 8/8 verdi: memorizzazione/lettura sotto
  il tetto, sfratto della voce meno recente al superamento del tetto,
  `get`/un nuovo `set` sulla stessa chiave aggiornano la recency
  (verificato che questo cambi *quale* voce viene sfrattata dopo, non
  solo che non vada in errore), `delete` esplicito, un tetto sotto 1
  viene rifiutato. Test di scala del piano (§4.8, "verifica che la
  cache delle pagine non supera il tetto dopo uno scroll completo
  simulato"): 4.000 mesi caricati in sequenza con un tetto di 50, la
  dimensione non supera mai 50 durante l'intero scroll, e alla fine
  contiene esattamente e solo gli ultimi 50 mesi visti.
- `npx vitest run` (suite intera) → 52 file, 298/298 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi → pulito.
- `npm run build` → bundle iniziale **94.318/153.600** byte gzip
  (script CI, invariato: nessun consumatore reale ancora).

Debito dichiarato, invariato dal passo precedente: nessuno dei tre
moduli (`geometry.ts`, `virtualize.ts`, `pageCache.ts`) è ancora usato
da `TimelineView.vue`. Prossimo passo: la riscrittura della vista
stessa, l'unico pezzo che rimane.

### Terzo e ultimo passo: la riscrittura di TimelineView.vue

Prima di toccare la vista, trovata in `docs/ui/keeppix-mockup.html` una
implementazione di riferimento del meccanismo intero (`planPhotoRows`/
`planStream`/`syncStreamWindow`/`mountPhotoStream`, righe 4564-4776,
con tanto di commento che descrive esattamente questo task) — verificata
come fonte per le costanti, non stimate: `GRID_GAP=6` (`justify.ts`
usava `4`, **senza alcuna fonte** — bug reale, corretto insieme a
questo task, non un nuovo valore inventato), `MONTH_HEAD_H=29`,
`MONTH_GAP=26`, `STREAM_OVERSCAN=1.25`, e la formula esatta di
`targetRowHeight`: `max(64, (larghezza - gap) / colonne / 1.3)` — la
vista precedente usava `max(48, larghezza/densità)`, un'approssimazione
senza fonte anch'essa corretta ora. Nuovo modulo `timeline/stream.ts`
(`planStream`, guidato dai confini di mese della *geometria* stessa, non
dall'elenco dei bucket — quest'ultimo alimenta solo l'etichetta del
conteggio, con ripiego sulla lunghezza del segmento se un mese manca).

Letto anche lo scrubber del mockup riga per riga (§8.3 del documento
funzionale + mockup righe 1579-1624), non riscritto a memoria dalla
versione precedente: due divergenze reali trovate e corrette.
`monthAtOffset` pesava per `count` — il documento è testuale:
"i mesi sono equidistanti sulla barra anche se uno contiene 5 foto e un
altro 300" — riscritto come indice equidistante
(`round(ratio*(n-1))`, la stessa formula del prototipo). Le etichette
delle tick (`yearLabel`, che mostrava l'ANNO) sono state sostituite da
`monthAbbrev`/`monthFull`: il prototipo usa una tabella `MONTHS`/
`MONTHS_FULL` di stringhe italiane scritte a mano, ma l'app supporta
IT/EN — un nome di mese è testo localizzato come ogni altro, quindi
`Intl.DateTimeFormat(locale, {month:'short'|'long', timeZone:'UTC'})`
al suo posto (verificato con `node -e` contro entrambe le lingue prima
di scrivere i test: "lug"/"luglio 2026" vs "Jul"/"July 2026";
`timeZone:'UTC'` esplicito perché altrimenti un fuso negativo potrebbe
leggere il giorno prima del mese costruito).

**Tolto, non portato avanti** (tre comportamenti dell'implementazione
pre-Task-4 verificati contro il documento funzionale e risultati non
canonici): il raggruppamento per giorno dentro il mese (§8: "Nessun
raggruppamento per giorno", testuale — l'algoritmo giustificato ora
lavora sull'intero mese, non spezzato per giornata); l'intestazione di
mese `sticky` durante lo scroll (§8: "le `.month-head` scorrono via
normalmente"); il filtro "Tutti/Foto/Video" — verificato che non è nel
documento per questa schermata (§8, l'intera sezione "Cosa mostra" non
lo nomina) né una delle sei dimensioni di SP-3 (§11: quella lista usa
"Tipo" per RAW/JPEG, un asse completamente diverso, dedotto da
`stackType` — non foto/video) — non uno scopo ridotto, un
comportamento senza fonte rimosso.

**Messo al lavoro, non solo costruito**: `PhotoTile` (Task 2, SP-1),
mai importato da nessuna vista reale finora, ora è la tessera vera
della timeline al posto del markup `<article>` scritto a mano che
c'era prima — che non era nemmeno raggiungibile da tastiera (nessun
`tabindex`, nessun `<button>`). Esteso con una prop `placeholderUrl`
(nuova, con relativo test): il "primo fotogramma non scarica nessuna
miniatura" (piano Task 4.7) richiede due `<img>` sovrapposti, che
`PhotoTile` non aveva. `stackType`/`favorite` richiedevano due campi
mai portati sul frontend nonostante il backend li avesse già
(`AssetView.raw_kind`/`.favorite`) — aggiunti a `TimelineAsset` come
campi additivi, con tutte le fixture di test esistenti aggiornate (5
file). Il toggle preferito resta **non cablato** qui: verificato nel
piano che "Preferiti, selezione multipla" sono esplicitamente Task 7
(Tranche B), non Task 4 — solo la resa (`isFavorite`) è cablata,
`@toggle-favorite` non è nemmeno ascoltato. Stesso confine per
`selected`/`selectionMode`, sempre `false` qui.

**Priorità di generazione senza `IntersectionObserver`**: il piano
(Task 4.6) lo nomina esplicitamente come meccanismo, ma con la
geometria già nota per ogni riga non serve osservare il DOM per sapere
cosa è davvero visibile — la stessa matematica del virtualizzatore con
`overscan:0` (invece del margine usato per montare le righe) dà la
finestra vera con precisione esatta, non un'approssimazione a soglie.
Scelta dichiarata, non un'omissione: più semplice da testare (nessun
mock di `IntersectionObserver`) ed esatta invece che approssimata.

**Fuoco perso allo smontaggio di una riga** (§66.5, punto di attenzione
esplicito del documento — "non coperto dal prototipo"): un
`watch(mountedRange, ...)` (flush di default `'pre'`, quindi prima che
Vue rimuova la riga dal DOM) sposta il fuoco sul contenitore di
scorrimento (`tabindex="-1"`, mai nell'ordine di tabulazione normale)
se l'elemento attivo appartiene a una riga (`data-row-index`) che sta
per uscire dall'intervallo montato.

**`ResizeObserver`** al posto di `window.resize` (piano Task 4.4):
osserva `gridEl`, ricalcola larghezza (letta da un div interno
`contentEl`, separato dal contenitore di scorrimento apposta — vedi
sotto) e altezza del viewport. Guardia esplicita
`typeof ResizeObserver !== 'undefined'`, stesso trattamento già dato a
`IntersectionObserver` nella versione precedente.

**Riga fade-in** (§66.6, 0.18s, disattivata sotto
`prefers-reduced-motion`): `--duration-tile-in` esisteva già nel Task 1
con un commento che descriveva esattamente questo uso ("comparsa
tessera dopo il caricamento") — evidentemente previsto in anticipo.
Nuova classe globale `.stream-row`/`kpx-fade-in` in `style.css`, stessa
categoria di `.spinner`/`.skel` (un'animazione per evento, non una
`transition` di stato — fuori dalla palette del Task 1 per lo stesso
motivo). Nessuna eccezione sotto `prefers-reduced-motion` necessaria:
la regola generale già la disattiva, ed è pura decorazione qui — non
l'unico segnale di un'azione in corso come lo spinner.

Bug reali trovati e corretti durante la scrittura, non nei moduli puri
già testati ma nell'integrazione:
- **Padding e larghezza di layout**: mettere `px-4` direttamente sul
  contenitore di scorrimento (`gridEl`) avrebbe fatto leggere
  `clientWidth` **comprensivo** del padding orizzontale, facendo
  traboccare le righe posizionate in assoluto oltre il bordo destro.
  Corretto separando il contenitore di scorrimento (nessun padding, solo
  `overflow-auto`) da un `<div>` interno col padding e da un ulteriore
  `contentEl` misurato per la larghezza — nessuna lettura di
  `getComputedStyle` necessaria (che in un test montato senza un vero
  motore CSS avrebbe restituito stringhe vuote per una classe Tailwind).
- **`ref<TimelineGeometry>` smontava l'istanza**: `vue-tsc` rifiutava
  `planStream(geometry.value, …)` con un errore che descriveva
  `TimelineGeometry` come un tipo strutturale senza il campo privato
  `view` — `UnwrapRef` di Vue smonta un'istanza di classe proprietà per
  proprietà quando è dentro un `ref()` semplice, perdendo l'identità
  nominale. Corretto con `shallowRef`, anche semanticamente giusto: un
  blob binario immutabile non ha bisogno di reattività profonda.
- **`v-bind` su un oggetto possibilmente `undefined`**: passare
  `cellProps(...)` (che può restituire `undefined` se l'asset non è
  ancora in cache) direttamente a `v-bind` rendeva OGNI prop di
  `PhotoTile` opzionale agli occhi di TypeScript, comprese quelle
  obbligatorie. Corretto con `resolvedTiles(row)`, che filtra le celle
  già risolte prima del `v-for` — nessuna tessera "vuota" da disegnare,
  la riga resta più sparsa per un istante se il mese non è ancora
  caricato (l'altezza è già riservata dalla geometria, nessuno
  spostamento di layout).
- **`currentIndex` dello scrubber azzerato dal test da tastiera**: la
  prima versione derivava l'indice corrente da un rapporto
  `scrollTop/(totalHeight-viewportHeight)` — quando il contenuto
  caricato è più basso del viewport (una libreria corta; qui, nel
  test), quel rapporto è indefinito e veniva forzato a 0 sempre, anche
  subito dopo un salto a tastiera esplicito a un mese preciso. Non è
  nemmeno l'inverso esatto di `jumpToMonth`, che scrolla a una
  posizione in pixel precisa, non a un rapporto. Corretto: il mese
  "corrente" è l'ultimo la cui intestazione ha già superato la cima del
  viewport (derivato da `monthTop`, la stessa mappa usata da
  `jumpToMonth`) — coerente per costruzione con qualunque salto, non
  solo un'approssimazione che a volte coincide.

Debiti dichiarati verso le Tranche successive, non scoperti ora:
- Task 7 (Tranche B) possiede "Preferiti, selezione multipla" — qui
  solo la resa (`isFavorite`), non l'azione.
- La navigazione precedente/successiva del lightbox resta limitata ai
  mesi attualmente in cache (`loadedAssets`, la concatenazione dei mesi
  residenti in `pageCache`) — stesso limite già dichiarato prima del
  Task 4 per l'avvio del culling, ora esplicitamente esteso anche qui:
  al bordo di un mese caricato, "successiva" può non trovare nulla
  anche se la foto esiste, semplicemente non è ancora in cache.
- Un evento live (`resync`/`assets.upserted`/`assets.deleted`) fa
  ripartire da zero sia `fetchBuckets` sia `fetchGeometry` — su una
  libreria da 214.000 scatti quest'ultima può pesare qualche MB anche
  quando l'`ETag` non coincide più per un singolo asset toccato. Nessun
  numero misurato qui (richiederebbe un backend reale con quella scala),
  quindi nessuna ottimizzazione tentata alla cieca — annotato come nota
  architetturale aperta, non ignorato.

Verifica eseguita:
- `TimelineView.spec.ts` (riscritto per intero) → 12/12 verdi:
  paginazione di un bucket fino a cursore esaurito (ora innescata
  dall'intervallo montato dal virtualizzatore, non più da
  `IntersectionObserver` su `[data-month]`); l'altezza totale scrollabile
  combacia esattamente con `planStream` calcolato a parte con gli stessi
  parametri (la verifica esplicita chiesta dal piano); raggruppamento
  solo per mese (nessun `<h3>` di giorno); bbox passato a tutte e tre le
  chiamate; apertura/chiusura/ricaricamento del lightbox via URL (Task 3,
  ora su `PhotoTile` invece di `article`); eventi live; **virtualizzazione
  reale**: una libreria sintetica da 2.000 scatti (40 mesi × 50) monta un
  numero di `PhotoTile` maggiore di zero ma **minore del totale** — la
  verifica di scala del piano applicata all'integrazione, non solo ai
  moduli puri; scrubber raggiungibile da tastiera (`role="slider"`,
  `tabindex="0"`, End salta all'ultimo mese e aggiorna
  `aria-valuenow`/`aria-valuetext`).
- `npx vitest run` (suite intera) → 53 file, 319/319 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/modificati → pulito (un warning
  `vue/require-default-prop` su `placeholderUrl` risolto aggiungendolo a
  `withDefaults`, non ignorato).
- `npm run build` → bundle iniziale **94.438/153.600** byte gzip
  (script CI). Margine ampio (59.162 byte) nonostante `PhotoTile` sia
  ora davvero importato da una vista reale per la prima volta.

## Task 4 — chiuso

Tre passi, tre commit, stessa disciplina del Task 2: decoder della
geometria + fetch binario; virtualizzatore + cache LRU; riscrittura di
`TimelineView.vue`. Ogni numero del piano verificato riga per riga
contro il mockup o il documento funzionale, mai assunto dal solo
riassunto — due bug reali di vecchia data (`GRID_GAP` senza fonte,
scrubber pesato per conteggio invece che equidistante) corretti insieme
al task che li ha fatti emergere, non ignorati perché "non è compito
di questo task". Debiti verso Task 5 (le tre macchine a stati) e Task 7
(selezione multipla, preferiti, filtro rapido reale) dichiarati sopra,
non silenziosi.

## Task 5 — le tre macchine a stati

Letti §68 (Errore), §69 (Riuscita parziale) e §70 (il pannello "Anteprima
stati") del documento funzionale, più la §7 di
`fase-10-api-interfaccia.md`, prima di scrivere codice. Tre fatti che
cambiano lo scopo del task, non solo l'implementazione:

- **SP-28 è già un terzo costruito.** È definito con tre forme — a piena
  vista, in riga, messaggio temporaneo — e la terza (il toast, con le
  durate 4,2s/6,5s per natura/azione) è già `ToastHost`+`stores/toast.ts`
  del Task 2. Restano solo le prime due, genuinamente nuove.
- **Il pannello "Anteprima stati" (§70) è scaffolding del prototipo,
  esplicitamente "nel prodotto finito non esiste"** — non va costruito.
  La sua nota finale è comunque la vera richiesta del task: "la macchina
  a stati che c'è dietro... è invece esattamente quella che serve nel
  prodotto vero."
- **La tassonomia a quattro nature (`unreachable`/`permission-denied`/
  `file-missing`/`timeout`) esiste già lato backend, ma solo per le
  operazioni di massa** (`FailureReason`/`BulkFailure.reason`,
  `crates/keeppix-api/src/bulk.rs`) — non per una richiesta singola come
  `GET /timeline/buckets`. Per il caricamento di un'intera schermata non
  esiste un campo `reason` da leggere: la natura va dedotta da
  `ApiProblem.type`/rete, non letta da un campo che il backend non manda
  lì. Decisione dichiarata, non assunta: `service-unavailable` (il
  `Problem` che `DbError::Connection` produce, verificato in
  `crates/keeppix-api/src/problem.rs`) → `unreachable`; `forbidden` →
  `permission-denied`; `not-found` → `file-missing`; un `TypeError` di
  `fetch()` stesso (comportamento noto della Fetch API quando la rete
  non risponde affatto) → `unreachable`; un `AbortError` → `timeout`;
  tutto il resto → `unknown`, onesto invece di forzare una delle quattro
  nature note — stesso principio del quinto valore `Unknown` lato
  backend.

### `errors/classify.ts`

`classifyError`/`canRetry`, pure e testate contro istanze reali di
`ApiProblem` con gli `type` slug che il backend produce davvero
(`service-unavailable`, `forbidden`, `not-found`, verificati alla fonte
in `problem.rs`), non un formato inventato.

### `ErrorState.vue` (piena vista) e `InlineError.vue` (in riga)

Trovata in `docs/ui/keeppix-mockup.html` (righe 3150-3173) l'esatta
implementazione di riferimento — `errorStateHTML`/`errorInlineHTML` —
con classi CSS, dimensioni icona (34px piena vista, 17px in riga) e
valori in pixel reali (`.err-title` 14px/700, `.err-sub` 12.5px/max-width
380px/line-height 1.55, `.err-detail` 11.5px monospazio opacità .8),
verificati uno per uno invece di stimati. Icona "alert" (triangolo di
avviso) e "refresh" prese dal path SVG esatto del prototipo (righe 1489
e 1498), non un glifo generico.

Il pulsante "Riprova" **non usa `BusyButton`**, la scelta di default per
ogni altro pulsante di questa sessione — trovato un vincolo esplicito
che lo esclude: §68.7 dice testualmente "non ha uno stato disabilitato:
riprovare è sempre permesso", mentre `BusyButton` esiste apposta per
*disabilitare* durante un'azione (`:disabled="busy"`, pensato contro il
doppio invio su un'azione di massa) — l'opposto di quanto serve qui: un
ritentativo che si blocca non deve mai trasformarsi in un vicolo cieco,
la cosa che l'intero pattern esiste per evitare. Un `<button>` semplice,
senza stato occupato, non un'omissione.

Bug reale trovato scrivendo i test, non nei componenti: un test che
impostava `i18n.global.locale.value = 'en'` dentro un ciclo e lo
ripristinava solo alla fine del proprio `it(...)` lasciava la lingua
sbagliata per i test **di un altro file** eseguito nella stessa
esecuzione di vitest (i moduli singleton come `i18n` non sono isolati
per file quanto ci si aspetterebbe) — non un bug isolato al file dove
si manifestava. Corretto adottando lo stesso schema già in uso in
`PhotoTile.spec.ts` (`beforeEach`/`afterEach` che salva e ripristina la
lingua precedente), non un cerotto locale.

### Cablato in `TimelineView.vue`, l'unico consumatore reale finora

`refreshTimeline()` non aveva **alcuna** gestione d'errore: un
fallimento di `fetchBuckets`/`fetchGeometry` restava una promise
rifiutata non gestita, la vista restava vuota senza spiegazione. Ora un
fallimento sostituisce l'intera griglia con `ErrorState` (è il
contenuto principale della schermata, non un pezzo — coerente con
"nessuna schermata assume che i dati ci siano"), con "Riprova" che
richiama di nuovo `refreshTimeline` (§68.4, testuale: "rimette
l'insieme di dati in caricamento e lo richiede da capo").

Debiti dichiarati, non silenziosi:
- `InlineError.vue` non ha ancora un consumatore reale — nessuna vista
  di questa sessione ha oggi un "pezzo mancante col resto arrivato" da
  mostrare, stesso trattamento di 17 dei 18 componenti del Task 2 al
  momento della loro costruzione.
- Tre schermate pre-esistenti violano già oggi la regola "Riprova solo
  per due nature" — `App.vue` (schermo di bootstrap, `common.unavailable`),
  `ProblemsView.vue` e `MapView.vue` (`common.retry` incondizionato,
  nessuna classificazione di natura) — **non riscritte qui**: sono
  rispettivamente territorio di Task 6 (shell) e Task 10/13 (Tranche B),
  ciascuna con la propria verifica dedicata contro il prototipo. Toccarle
  ora avrebbe anticipato decisioni di quelle schermate senza il confronto
  riga per riga che questa sessione ha sempre richiesto prima di
  scrivere.
- Nessun composable generico "stato di caricamento" (`useAsyncState` o
  simile) costruito: `TimelineView.vue` gestisce già il proprio ciclo di
  caricamento (Task 4) e sarebbe stata un'astrazione senza un secondo
  consumatore reale — prematura, non giustificata dal solo principio
  del piano.
- Nessun meccanismo di timeout lato client aggiunto ad `apiFetch`: la
  natura `timeout` è supportata dal classificatore ma oggi non si
  verifica mai da sola (nessuna richiesta si autointerrompe) — una
  funzionalità a sé, fuori dallo scopo di questo task.
- §69 (Riuscita parziale, SP-29): il toast è già completo dal Task 2.
  Il "ritenta solo le rimaste indietro" richiede l'involucro
  `succeeded`/`failed` che solo le operazioni di massa restituiscono —
  nessuna azione di massa reale è ancora cablata in questa sessione
  (Task 7), quindi non c'è ancora nulla da collegare a quel meccanismo.

Verifica eseguita:
- `classify.spec.ts` (nuovo) → 8/8 verdi: le tre mappature dai veri
  `type` di `Problem`, il ripiego onesto su un `type` sconosciuto, il
  `TypeError` di rete, l'`AbortError`, e `canRetry` vero solo per le due
  nature giuste.
- `ErrorState.spec.ts` + `InlineError.spec.ts` (nuovi) → 17/17 verdi:
  "Riprova" presente/assente per ciascuna delle cinque nature (la
  verifica esplicita chiesta dal piano), l'evento `retry` emesso, il
  testo di ogni natura non contiene mai la frase vietata "qualcosa è
  andato storto", la riga di dettaglio tecnico compare solo se passata,
  parità it/en per tutte e cinque le nature su entrambi i componenti.
- `TimelineView.spec.ts` (esteso, +3 test) → 15/15 verdi: un fallimento
  di rete classificato correttamente sostituisce la griglia con
  `ErrorState` (con la riga di dettaglio tecnico `type · status`),
  "Riprova" richiama `refreshTimeline` e un successivo successo fa
  sparire l'errore mostrando la griglia vera, una natura non
  ritentabile (`file-missing`) non mostra alcun pulsante.
- `npx vitest run` (suite intera) → 56 file, 349/349 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/modificati → pulito.
- `npm run build` → bundle iniziale **94.893/153.600** byte gzip
  (script CI). Margine ampio (58.707 byte).

## Task 5 — chiuso

Le due forme mancanti di SP-28 (piena vista, in riga) più il
classificatore di natura che le due condividono con la terza forma già
esistente (il toast). Un solo consumatore reale cablato (`TimelineView`,
l'unica vista già riscritta con un ciclo di caricamento vero) — stesso
ritmo "costruito, non ancora ovunque messo al lavoro" del Task 2. Debiti
verso Tranche B (le tre schermate pre-esistenti non conformi) e verso
Task 7 (§69, in attesa di una prima azione di massa reale da collegare)
dichiarati sopra, non silenziosi.

## Task 5bis — le ottimizzazioni di client

Non un task da zero: un controllo riga per riga della tabella del piano
(§5bis, "Immagini" + "Scorrimento e layout") contro quello che il Task 4
ha già costruito, prima di aggiungere altro.

**Già vero per costruzione, verificato non assunto:**
- `thumbhash` come primo fotogramma → `PhotoTile.placeholderUrl` (Task 4).
- `IntersectionObserver` → deliberatamente sostituito dalla matematica
  esatta del virtualizzatore (Task 4, dichiarato lì).
- `POST /viewport` mentre si scorre → `trueVisibleHashes` (Task 4).
- `width`/`height` espliciti → la tessera (il genitore assoluto) ha già
  `width`/`height` in pixel dalla geometria; l'`<img>` la riempie con
  `absolute inset-0`, quindi zero spostamento di layout esiste già per
  costruzione, anche senza attributi `width`/`height` sull'`<img>`
  stesso — verificato leggendo `PhotoTile.vue`, non assunto.
- `transform: translateY`, mai `top` → `TimelineView.vue` (Task 4).
- Layout ricalcolato solo su `ResizeObserver`/densità, mai su scroll →
  vero per costruzione: `plan`/`virtualizer` dipendono da `gridWidth`/
  `density`/`geometry`, non da `scrollTop`.
- Ascoltatori passivi → il listener di scroll di `TimelineView.vue` è
  già `{ passive: true }` (Task 4).
- `maplibre-gl`/`hls.js` fuori dal bundle iniziale → verificato di nuovo
  nell'output di build di ogni singolo commit di questa sessione, mai
  nel bundle iniziale.

**Misurato, non assunto — il calcolo del layout giustificato:**
La Ruling della spec (§3) dice "è aritmetica lineare, dell'ordine delle
decine di millisecondi... da misurare in Task 4: se supera i 50 ms,
allora sì [serve un Web Worker]". Misurato con `planStream` reale su
una geometria sintetica da 214.000 record (la dimensione di libreria
citata in tutto il piano) su 240 mesi, 5 esecuzioni:
**51,6 / 54,7 / 60,0 / 96,5 / 99,8 ms, mediana 60 ms.**

Supera la soglia dichiarata dal piano stesso — su *questo* ambiente.
Onestà dovuta: questo container di sviluppo non è l'hardware bersaglio
dichiarato dalla spec (Raspberry Pi 5 / 8 GB), e non ho un Pi 5 su cui
misurare — non posso dire con certezza se il numero vero sarebbe
migliore o peggiore (un core server-grade regge spesso meglio per IPC
di un core ARM Cortex del Pi, ma la virtualizzazione di questo
container aggiunge un costo che un Pi bare-metal non avrebbe).

**Decisione presa qui: non spostare `planStream` in un Web Worker in
questo passo.** Non perché il numero misurato lo escluda — non lo fa,
è ambiguo — ma perché portarlo in un worker è un cambio d'architettura
vero (serializzazione/trasferimento dell'`ArrayBuffer`, il `computed`
reattivo `plan` diventerebbe asincrono, tutta `TimelineView.vue` da
ricablare) che non è giustificato costruire alla cieca su un numero
ambiguo misurato sull'hardware sbagliato. Lasciato esplicitamente
aperto: la decisione giusta richiede o hardware reale (un Pi 5) o
un'indicazione di chi mantiene il progetto, non un'altra stima.

**`fetchpriority="high"` per le tessere della prima schermata**
(unica riga della tabella genuinamente non ancora coperta): nuova prop
`priority?: 'high'|'auto'` su `PhotoTile.vue` (default `'auto'`), che
governa insieme due attributi imparentati — `loading` (`'eager'` invece
di `'lazy'`) e `fetchpriority` (`'high'`, altrimenti assente) — non due
prop separate, perché nella tabella del piano sono la stessa decisione
("solo le tessere della prima schermata") applicata a due attributi
HTML diversi. `TimelineView.vue` decide chi è "prima schermata" da
`rowTop < viewportHeight` — letteralmente cosa è già a schermo al primo
paint, non un'approssimazione dello scroll corrente.

`content-visibility: auto` per le griglie non virtualizzate (l'altra
riga rimasta della tabella): nessuna vista di questa sessione ha oggi
una griglia non virtualizzata reale da toccare (ricerca/cestino/
duplicati usano ancora "layout diretto" per §66.8, ma nessuna è stata
riscritta in questa Tranche) — debito dichiarato verso le schermate che
la useranno davvero.

Verifica eseguita:
- `PhotoTile.spec.ts` (esteso, +1 test) → 14/14 verdi: `priority`
  assente → `loading="lazy"`, nessun `fetchpriority`; `priority="high"`
  → `loading="eager"`, `fetchpriority="high"`.
- `TimelineView.spec.ts` (esteso, +1 test) → 16/16 verdi: su una
  libreria sintetica più grande di uno schermo, esistono sia tessere
  `priority="high"` (prima schermata) sia `priority` non-`"high"` (il
  resto) — non tutte alte, non tutte pigre.
- `npx vitest run` (suite intera) → 56 file, 351/351 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file modificati → pulito.
- `npm run build` → bundle iniziale **94.888/153.600** byte gzip
  (script CI). Margine ampio (58.712 byte).

## Task 5bis — chiuso (con una decisione esplicitamente aperta)

La tabella del piano era già per la maggior parte soddisfatta dal
disegno del Task 4 — verificato voce per voce, non assunto. Un solo
elemento genuinamente mancante costruito (`fetchpriority`/`loading`
della prima schermata). Un numero misurato per la Ruling sul thread
principale (60ms mediana su 214k record, su hardware non bersaglio) che
CONTRADDICE l'assunzione ottimistica del piano ma non è stato seguito
da un'azione — dichiarato apertamente come domanda non risolta, non
insabbiato né deciso alla cieca. `content-visibility:auto` resta debito
verso le griglie non virtualizzate future.

Chiude Tranche A: Task 1-5bis tutti chiusi. Prossimo: Tranche B
(Task 6-14, le singole schermate).
