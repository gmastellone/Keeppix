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

# Tranche B

## Task 6 — Shell desktop e mobile (in corso)

Task 6 copre due lavori distinti e grandi: la shell (sidebar, topbar,
menu account, "Altro" mobile) e — separatamente, in
`docs/ui/caricamento-nuove-foto.md` — l'intera area di caricamento
nuove foto (tre porte d'ingresso, sei stati per file, rifiuto RAW,
destinazione con precedenza). Trattati come unità di lavoro separate,
non un unico commit: la shell per prima, perché ogni vista futura ci
monta dentro.

Prima di scrivere codice, due agenti Explore hanno verificato lo stato
reale (mai assunto): quali destinazioni esistono davvero (router +
elenco viste), cosa `AppShell`/`NavGroup`/`Avatar` (Task 2) espongono
già, e — soprattutto — quali dati reali il backend può davvero dare
alla sidebar. Tre scoperte che ridisegnano lo scopo di questo passo:

- **Il conteggio per cartella non esiste per scelta, non per lacuna.**
  `FolderView` (`crates/keeppix-api/src/routes/folders.rs:14-22`) ha un
  commento esplicito: *"Task 11 (spec §7bis.2): niente `asset_count` per
  riga — il prototipo mostrava «Urbino 556» in sidebar, ma quel
  aggregato non entra in `/api/v1`."* Non un gap da colmare: una
  decisione già presa in Fase 10.
- **Il badge Culling è un vero campo del backend, `bootstrap.badges.
  culling`, ma vale sempre 0** — commento del backend: *"Zero finché i
  lotti non esistono nel backend"* (Task 17, Tranche D, non ancora
  scritto). Cablarlo comunque è onesto, non un placeholder: il giorno
  che Task 17 lo popola per davvero, questo componente non cambia di
  una riga.
- **Il badge Revisione (tag+volti in attesa) è invece già vero e
  economico** — `bootstrap.badges.revision` somma
  `AssetTagRepo::count_proposed_visible` e
  `FaceRepo::count_proposed_visible`, entrambi sicuri a riconoscimento
  volti spento. **Lo spazio libero/totale è vero anch'esso** —
  `GET /api/v1/libraries/{id}/storage`, già presente anche dentro lo
  stesso `/api/v1/bootstrap` — sostituisce il "1,4 TB su 2 TB" statico
  del mockup con un numero reale, non un'altra costante di demo.

**Ambito dichiarato per la sidebar desktop, solo le voci con una
destinazione vera oggi:** Foto, Cerca, Culling (badge reale),
Mappa, Condivisioni, Album, e dentro "Manutenzione" solo Cestino e
Problemi. **Tolte, debito esplicito verso le Tranche che le
costruiranno:**
- **"Persone"** — Task 16, Tranche D, nessuna vista Persone esiste.
- **"Preferiti"** — nessuna vista dedicata esiste ancora in questa
  sessione.
- **L'intero gruppo "IA"** (Tag e categorie/Revisione/Analisi
  libreria) — Task 15, Tranche C.
- **"Duplicati"** dentro Manutenzione — Task 13, Tranche B, non ancora
  fatto (a differenza di Cestino/Problemi, già reali).
- **Il gruppo "Cartelle"** (l'elenco piatto con salto diretto a una
  cartella filtrata): nessuna timeline filtrata per cartella esiste
  ancora — stesso debito già dichiarato nel Task 3 per le rotte di
  dettaglio. Costruire righe che non portano da nessuna parte sarebbe
  un link morto, non un ambito parziale onesto — a differenza del
  badge Culling (dato reale, destinazione futura), qui **né** il dato
  **né** la destinazione esistono.
- **Menu account: solo "Esci"**, reale (`session.logout()` +
  redirect a `/login`). "Profilo" e "Impostazioni" tolti: nessuna
  pagina hub esiste ancora per nessuno dei due (solo sei pagine
  tecniche sotto `/settings/*`, nessun indice) — Task 14, non ancora
  fatto. Aggiungerli ora avrebbe inventato una convenzione di
  instradamento che Task 14 potrebbe smentire, lo stesso principio già
  seguito per `AppShell` col router.
- **Lo stato "In linea"** (pallino verde) del piede utente: **omesso**,
  non solo rimandato — non esiste alcun concetto di presenza/stato
  online nel backend di un'app self-hosted single-tenant, a differenza
  dello spazio libero che un endpoint reale già dà. Mostrarlo sarebbe
  stato inventare un dato, non solo un ambito ridotto.

### `api/bootstrap.ts` + `stores/shell.ts`

Nuovo client per `GET /api/v1/bootstrap` (un'unica chiamata:
cartelle, spazio per libreria, badge) e uno store Pinia a sé
(`useShellStore`), distinto da `stores/session.ts` nonostante il
backend chiami "bootstrap" anche il proprio giro di
autenticazione/setup — due domande diverse ("chi sono" vs "cosa mostra
il telaio") che condividono solo il nome.

### `components/AppSidebar.vue`

Verificato riga per riga contro §2 del documento funzionale (non il
solo riassunto del piano) e contro il marchio SVG esatto del mockup
(righe 1397-1398: anello `r=62` più pallino `r=24` in `viewBox="0 0 200
200"`, non un'icona approssimata). Riusa `NavGroup` (Task 2, SP-25:
"Manutenzione" si apre da solo quando contiene la vista corrente),
`Avatar` (Task 2, SP-16) e `Popover` (Task 2, SP-14) per il menu
account — nessuno dei tre era ancora importato da una vista reale.

Ogni voce è un vero `<RouterLink>`, quindi raggiungibile da tastiera
per costruzione — il documento (§2.5) dichiara il prototipo non
raggiungibile ("nessuna voce della sidebar è raggiungibile da
tastiera... `:focus-visible` è codice morto").

Terza copia, non unificata, della stessa formattazione byte→testo
leggibile già duplicata (e divergente: base 1024 in
`UploadPanel.vue`, base 1000 in `MapsOfflineView.vue`) — debito reale
annotato nel commento del componente, non silenzioso, ma unificarle
tocca due viste che questo task non sta toccando.

Bug reale trovato scrivendo i test, non nel componente: il menu
account (`Popover`) è teleportato su `document.body`, fuori
dall'albero DOM del wrapper di test — `wrapper.findAll('button')` non
lo vede mai. Stesso schema già noto per `Dialog` (Task 2), applicato
qui per la prima volta a `Popover`: query dirette su
`document.body.querySelectorAll(...)`, non sul wrapper.

Verifica eseguita:
- `shell.spec.ts` (nuovo) → 2/2 verdi: stato iniziale sicuro prima del
  caricamento, `load()` popola cartelle/spazio/badge dalla risposta
  reale.
- `AppSidebar.spec.ts` (nuovo) → 6/6 verdi: caricamento dei dati al
  montaggio con il badge Culling reale; ogni voce è un vero `<a>`
  (raggiungibile da tastiera); solo la rotta corrente è evidenziata;
  "Manutenzione" si apre da sola sulle proprie sotto-voci; spazio
  libero/totale reali (non un segnaposto statico); il menu account
  mostra il nome utente vero e "Esci" disconnette e reindirizza a
  `/login`.
- `npx vitest run` (suite intera) → 58 file, 360/360 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/modificati → pulito (rinominato
  `Sidebar.vue` → `AppSidebar.vue`: `vue/multi-word-component-names` è
  disattivato solo per `components/ui/**`, non per `components/`).
- `npm run build` → bundle iniziale **95.155/153.600** byte gzip
  (script CI). Margine ampio (58.445 byte) — `AppSidebar` non è ancora
  importato da `App.vue`, nessun consumatore reale ancora.

Debito dichiarato, esplicito: `AppSidebar` non è ancora cablato in
`App.vue`, e le ~9 viste con intestazione improvvisata (TimelineView,
MapView, FoldersView, AlbumsView, SharesView, TrashView, GroupsView,
BatchEditView, UsersView) non sono ancora state private del proprio
markup ad-hoc. Prossimo passo di questo task: la topbar (breadcrumb +
scorciatoia di ricerca), poi il cablaggio reale in `App.vue` con la
rimozione dell'intestazione improvvisata da ogni vista, poi la shell
mobile, poi — separatamente — l'area di caricamento nuove foto.

### CI su `e97a83e` (AppSidebar): un rosso, non del componente

Il job `backend` è fallito su `crates/keeppix-db/tests/scale_embeddings.rs
:: vector_search_stays_interactive_with_ivfflat` — misurato 999.4 ms
contro un budget di 1000 ms (`raw vector scan 999.4 ms should be
interactive with IVFFlat`). Il commit non tocca un solo file backend
(solo `frontend/src/{api,stores,components,i18n}`), e il margine
(0.6 ms su 1000) è coerente con rumore del runner, non con una
regressione. Rilanciato `rerun_failed_jobs` sullo stesso run per
confermare prima di considerarlo un flake accertato — nessuna modifica
al codice per un test di soglia temporale che non è nel percorso di
questo task.

### `components/AppTopbar.vue`

Documento funzionale §4 ("Barra superiore / breadcrumb", righe
830-929), verificato riga per riga, più il markup reale del mockup
(righe 1434-1439 per lo scheletro `.topbar`, 3212-3247 per
`renderTopbar()`).

Scoperta scrivendo il componente, non assunta dal riassunto del piano:
il markup del mockup ha **tre** elementi in `.topbar-right`
(`#uploadTopBtn` + `#topSearch`), non i "due soli elementi" che il
testo del documento funzionale afferma alla riga 838 — un disallineamento
reale fra testo e codice del mockup, non un'invenzione. Il mockup HTML
è la fonte di verità qui (stessa regola con cui questa sessione ha già
trattato ogni altro scarto testo/codice). Il pulsante Carica resta
comunque fuori da questo componente: appartiene al sottosistema di
caricamento (`caricamento-nuove-foto.md`), un blocco di lavoro già
dichiarato a parte nel diario — costruirlo ora senza il selettore di
destinazione dietro sarebbe un pulsante finto, stesso principio già
applicato al gruppo "Cartelle" di `AppSidebar`.

Ambito delle briciole di pane: solo il segmento "corrente", per le
sole rotte con una destinazione reale oggi (stesso elenco di
`AppSidebar` più `/batch-edit` → riusa `batchEdit.title`, non un nuovo
testo — il documento funzionale userebbe "Modifica multipla" ma la
vista reale si chiama già "Modifica in blocco": divergenza
preesistente di `BatchEditView`, non introdotta né corretta qui). Il
segmento "genitore" del mockup (`Cartelle / <nome>`, `Album /
<nome>`, `Culling / <nome lotto>`) non è mai raggiungibile: nessuna
rotta oggi porta uno stato "aperto" osservabile dall'esterno della
vista — stesso debito già dichiarato per il gruppo "Cartelle" di
`AppSidebar` (Task 13/15/16). Le rotte reali ma assenti dalla mappa
del documento (`/folders`, `/users`, `/groups`) restano a briciola
vuota: comportamento letterale del prototipo per le viste non mappate
(`crumbs[view] || ''`), verificato con un test dedicato, non
un'omissione silenziosa.

Correzione di accessibilità rispetto al prototipo (stessa politica di
`AppSidebar`): il documento dichiara esplicitamente una deviazione da
SP-8 alla riga 906 ("premere Invio o Spazio... non fa nulla — solo il
click del mouse apre Cerca"). Qui Invio e Spazio attivano la
scorciatoia di ricerca esattamente come il click.

Per mettere a fuoco il campo vero di `SearchView` dopo la
navigazione (comportamento del mockup: `setTimeout(...,0)` più
`getElementById('cercaInput')`), aggiunto `id="search-query-input"`
al campo reale di `SearchView.vue` — unica modifica a quel file.

Bug reale trovato scrivendo il test, non nel componente: il primo
tentativo del test "clic apre /search e mette a fuoco il campo" falliva
sempre (`document.activeElement` restava vuoto) perché il test montava
`AppTopbar` da solo, senza un `<RouterView>` reale — la rotta cambiava
ma nessuna vista di destinazione compariva nel DOM da mettere a fuoco.
Corretto montando un host `{ AppTopbar, RouterView }` insieme, non
`AppTopbar` isolato.

Verifica eseguita:
- `AppTopbar.spec.ts` (nuovo) → 7/7 verdi: briciola in grassetto
  corretta per due rotte diverse; briciola vuota per una rotta reale
  ma non mappata dal documento; il campo è davvero `readonly` col
  placeholder esatto; il click apre `/search` e mette a fuoco il campo
  reale; Invio e Spazio fanno lo stesso (SP-8).
- `npx vitest run` (suite intera) → 59 file, 368/368 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/modificati → pulito. Un solo errore
  preesistente nell'intero repo (`PlayerView.vue:51`, parametro
  `_plan` mai usato) — commit `6fab915` del 20/08, prima di questa
  sessione, non toccato da nessun task Fase 11 finora; il job
  `frontend` della CI non esegue affatto `npm run lint` (solo Tipi/
  Test/Build/Budget — verificato nel workflow), quindi non l'ha mai
  intercettato. Fuori dall'ambito di questo task: non corretto qui.
- `npm run build` → bundle iniziale **95.247/153.600** byte gzip
  (script CI, ricalcolato a mano). Margine ampio (58.353 byte);
  `AppTopbar` non ancora importato da `App.vue`.

Debito invariato rispetto a Task 6 (1/N): sidebar e topbar esistono ma
nessuna delle due è ancora cablata in `App.vue`, e le ~9 viste con
intestazione improvvisata non sono ancora state private del proprio
markup ad-hoc. Prossimo passo: il cablaggio reale in `App.vue`
(entrambi i componenti dentro `AppShell`, rimozione dell'intestazione
improvvisata da ogni vista), poi la shell mobile, poi —
separatamente — l'area di caricamento nuove foto.

### `App.vue` — Task 6 (3/N): cablaggio reale in `AppShell`

`AppShell` (Task 2) sostituisce finalmente il solo `<RouterView>` per
gli utenti con una sessione valida: `#sidebar` → `AppSidebar`,
`#topbar` → `AppTopbar`, slot di default → `<RouterView>` invariato.

**Decisione presa scrivendo questo passo, non nel piano**: le ~9 viste
con intestazione improvvisata **non** vengono ancora spogliate, a
differenza di quanto il diario del passo precedente dava per
scontato. Motivo trovato rileggendo `TimelineView.vue` riga per riga
prima di toccarla: la sua intestazione ad-hoc porta a `/folders`,
`/users` e `/groups` — tre rotte reali che **`AppSidebar` non copre**
(l'ambito dichiarato in Task 6 1/N include Foto/Cerca/Culling/Mappa/
Condivisioni/Album/Cestino/Problemi, non Cartelle né le due viste
admin-only). Spogliare l'intestazione ora renderebbe quelle tre rotte
irraggiungibili dall'interfaccia — non un'omissione dichiarata come le
altre di questo task, ma un vicolo cieco vero. In particolare
`/folders` (`FoldersView`, elenco cartelle reale, non la timeline
filtrata per cartella già esclusa in Task 6 1/N) è una svista di
scoping di quel passo: ho confuso "il gruppo Cartelle del mockup con
salto a una timeline filtrata" (quello sì assente) con "qualunque
modo di raggiungere la pagina Cartelle che esiste già" (quello
avrebbe dovuto restare). Da risolvere prima di spogliare le
intestazioni, non nascondendolo spogliandole comunque: prossimo
sotto-passo, non questo.

Shell mobile: ancora assente (stesso debito già dichiarato in Task 6
1/N e 2/N). Sotto i 768px `AppShell` mostra solo lo slot di default —
nessuna intestazione, nessuna barra a schede. Prima di questo commit
non c'era comunque una vera shell mobile (le intestazioni ad-hoc non
sono mai state responsive per design); dopo questo commit la
differenza pratica è che sparisce anche la manciata di link diretti
che quelle intestazioni offrivano su schermi stretti — regressione
temporanea reale, dichiarata qui, non nascosta, e limitata a un ramo
(`fase-11`) non ancora unito a `main`. Prossimo sotto-passo di questo
task, non rimandato oltre.

Verifica eseguita:
- `App.spec.ts` (nuovo) → 2/2 verdi: con sessione valida, `AppSidebar`
  (un `<a href="/culling">` vero) e `AppTopbar` (`#topSearch`) sono
  montati per davvero e la vista instradata compare nello slot di
  default; con `session.unavailable`, né l'uno né l'altro compaiono —
  solo la schermata di indisponibilità.
- Bug reale trovato scrivendo il test: montare `App` con
  `vi.mock('@/components/UploadPanel.vue', ...)` (un oggetto grezzo
  come sostituto) fa esplodere un errore non gestito
  (`No "__isTeleport" export is defined on the mock`) — il resolver
  di componenti asincroni di Vue Test Utils introspeziona lo spazio
  dei nomi del modulo mockato per simboli interni (`__isTeleport`
  ecc.) che un `vi.mock` con un oggetto letterale non ha. Corretto con
  `global.stubs: { UploadPanel: true }` invece di mockare il modulo:
  stessa cosa (l'overlay reale non viene mai montato), nessun errore.
- `npx vitest run` (suite intera) → 60 file, 370/370 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/modificati → pulito.
- `npm run build` → bundle iniziale **116.469/153.600** byte gzip
  (script CI, ricalcolato a mano). Salto reale e atteso da 95.247 a
  116.469 (+21.222 byte): `AppSidebar`/`AppTopbar` e le loro dipendenze
  (`Avatar`, `Popover`, `NavGroup`, `stores/shell.ts`) erano codice
  morto finché `App.vue` non le importava per davvero — ora sono nel
  chunk d'ingresso. Margine residuo ampio (37.131 byte), numero
  misurato, non stimato.

### `AppSidebar.vue` — Task 6 (4/N): chiuso il vicolo cieco trovato nel passo precedente

Aggiunte due voci non nel documento funzionale (dichiarate come tali,
non spacciate per canoniche):

- **"Cartelle" → `/folders`**: non è il gruppo del mockup (§2, lettera
  d — una riga per cartella, salto diretto a una timeline filtrata),
  che resta assente per lo stesso motivo di sempre (nessuna timeline
  filtrata per cartella esiste). È un solo collegamento all'albero
  cartelle reale dell'app (`FoldersView`, organizzazione/spostamento,
  una funzione diversa da quella del mockup). Aggiunta perché
  altrimenti, tolta l'intestazione improvvisata di `TimelineView`
  (prossimo sotto-passo), `/folders` non sarebbe raggiungibile da
  nessuna parte dell'interfaccia.
- **"Amministrazione" (Utenti/Gruppi) → `/users`, `/groups`**, visibile
  solo per `role==='admin'`: il mockup è a singolo utente, non modella
  amministrazione multiutente — questa non è un'omissione del
  documento funzionale, è una funzione reale del backend che il
  documento non copre affatto. Stesso motivo di "Cartelle": unica
  destinazione oggi è l'intestazione improvvisata che sto per togliere.

Verifica eseguita:
- `AppSidebar.spec.ts` (esteso) → 9/9 verdi (3 nuovi): "Cartelle" è un
  vero `<a href="/folders">`; "Amministrazione" mostra Utenti e Gruppi
  e si apre da sola sulla propria rotta; "Amministrazione" è **assente
  del tutto** (non solo chiusa) per un utente non amministratore.
- `npx vitest run` (suite intera) → 60 file, 373/373 verdi.
- `npx vue-tsc -b` → un errore reale trovato e corretto: il parametro
  opzionale `user` di `mountSidebar()` ereditava per inferenza il tipo
  letterale esatto di `testUser` (`role: "admin"`) dal suo valore di
  default, rifiutando poi un secondo utente con `role: "user"`.
  Corretto tipizzando il parametro con `User` (da `api/auth`), non con
  il tipo inferito dal default.
- `npx eslint` sui file nuovi/modificati → pulito.
- `npm run build` → bundle iniziale **116.568/153.600** byte gzip
  (script CI, ricalcolato a mano). Crescita trascurabile (+99 byte:
  due voci di menu in più, nessuna dipendenza nuova). Margine ampio
  (37.032 byte).

Debito Task 6 ora ridotto a uno solo: le ~9 viste con intestazione
improvvisata non sono ancora state spogliate — ma ora **possono**
esserlo, perché ogni destinazione che offrivano (incluse `/folders`,
`/users`, `/groups`) ha una voce reale in `AppSidebar`. Prossimo
sotto-passo: spogliarle per davvero, una alla volta, verificando per
ciascuna che nessuna destinazione residua venga persa. Poi la shell
mobile, poi — separatamente — l'area di caricamento nuove foto.

### `TimelineView.vue` — Task 6 (5/N): prima vista spogliata

Tolti dall'intestazione improvvisata: il saluto (`home.greeting`, mai
nel documento funzionale — solo un segnaposto di scaffolding, la
chiave è stata rimossa da entrambi i file di traduzione perché
rimasta senza nessun consumatore), il modulo di ricerca digitabile
(sostituito concettualmente dalla scorciatoia sola-lettura di
`AppTopbar`, coerente con §4: non si digita mai lì, solo in
`SearchView`), gli otto `<RouterLink>` (Cartelle/Mappa/Cestino/Album/
Condivisioni/Utenti/Gruppi/Problemi — tutti ora in `AppSidebar`) e il
pulsante "Esci" (ora nel menu account di `AppSidebar`, con il proprio
test già lì).

Restano, spostati in una barra strumenti più snella (non più
`<header>`, per non duplicare il landmark che `AppTopbar` già offre):
l'ingresso al culling (unico pulsante, regola rigida della spec §4.2:
niente scorciatoie sparse) e il controllo di densità. Quest'ultimo
**non è nel documento funzionale**: la densità della griglia vive in
Impostazioni (riga 1745, Task 14, non ancora costruito), non in un
controllo di vista. Lasciato qui come ripiego dichiarato — toglierlo
ora senza sostituto fisserebbe la densità a 6 per chiunque, una
regressione reale — con un commento nel codice che lo segnala per
quando Impostazioni esisterà davvero.

Verifica eseguita:
- `TimelineView.spec.ts` → tolti i 2 test di "Esci" (funzionalità
  spostata, già coperta dal proprio test in `AppSidebar.spec.ts` —
  duplicarli avrebbe testato la funzione `session.logout()`, non più
  il componente), insieme al mock di `@/api/auth` e all'import
  `logout`/`Button`, entrambi orfani dopo la rimozione. 14/14 verdi
  (16 meno i 2 tolti).
- `npx vitest run` (suite intera) → 60 file, 371/371 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file toccati → pulito.
- `npm run build` → bundle iniziale **116.551/153.600** byte gzip
  (invariato: `TimelineView` è un chunk lazy, non nel bundle
  d'ingresso — il suo stesso chunk è sceso da 23,54 a 21,86 kB non
  compresso, coerente con la rimozione di markup e logica).

Debito Task 6 invariato per le altre ~8 viste (MapView, FoldersView,
AlbumsView, SharesView, TrashView, GroupsView, BatchEditView,
UsersView): stessa operazione da ripetere una alla volta, verificando
per ciascuna cosa perde e cosa no (non tutte avranno lo stesso
schema di TimelineView — es. UsersView/GroupsView potrebbero non
avere link di navigazione affatto, solo il proprio contenuto). Poi la
shell mobile, poi — separatamente — l'area di caricamento nuove foto.

### `MapView.vue` + le 7 viste con lo schema "torna a / + h1" — Task 6 (6/N)

**`MapView.vue`**: tolti il link "indietro" e l'`<h1>` (`maps.back`/
`maps.title`, entrambe orfane dopo, rimosse da entrambi i file di
traduzione). Resta solo il pulsante reale "Regioni offline"
(`managingRegions`), senza un'altra sede.

**`FoldersView`, `AlbumsView`, `SharesView`, `TrashView`, `GroupsView`,
`BatchEditView`, `UsersView`**: stesso identico blocco in tutte e
sette — `<p><RouterLink to="/">{{t('folders.back')}}</RouterLink></p>
<h1>{{titolo}}</h1>` — verificato riga per riga per ciascuna prima di
toccarla (nessuna nascondeva un pulsante o un'azione in più dentro
quel blocco). Tolto ovunque; `folders.back` è rimasta orfana ovunque,
rimossa da entrambi i file di traduzione. `albums.title`/`trash.title`/
`shares.title` erano usate **solo** dal proprio `<h1>` ora tolto (il
briciolo di `AppTopbar` per quelle tre riusa `.entry`, valore identico
ma chiave diversa) — orfane, rimosse. `folders.title`/`users.title`/
`groups.title` invece restano: le usa `AppTopbar` (v. sotto).

**Scoperta scrivendo questo passo, non prevista**: `AppTopbar` (Task 6
2/N) lasciava `/folders`, `/users`, `/groups` a briciola vuota,
presumendo che il proprio `<h1>` di ciascuna vista facesse comunque da
titolo. Togliendo qui quell'`<h1>`, quella scelta smetteva di reggere:
quelle tre pagine sarebbero rimaste **senza alcun titolo visibile**, non
fedeli al comportamento del prototipo (che le ignora perché lì non
esistono affatto), solo un buco. Corretto **prima** di procedere:
`AppTopbar`'s `CRUMB_LABEL_KEYS` ora include anche `/folders` →
`folders.title`, `/users` → `users.title`, `/groups` → `groups.title` —
stesso principio già usato per aggiungerle a `AppSidebar` nel Task 6
4/N (destinazioni reali dell'app, non del mockup, meritano un
trattamento reale, non il ripiego del prototipo pensato per tutt'altre
viste).

Verifica eseguita:
- `AppTopbar.spec.ts` (esteso, 8/8 verdi): il test "briciola vuota per
  una rotta non mappata" è stato rifatto su `/settings/maps/offline`
  (non collegata da `AppSidebar`, resta vuota per davvero) invece di
  `/folders` (ora ha una briciola reale, verificato con un test
  proprio: "Cartelle", non un errore né un buco).
- `MapView.spec.ts`, `SharesView.spec.ts`, `FoldersView.spec.ts`,
  `UsersView.spec.ts` → verdi. `AlbumsView`, `TrashView`, `GroupsView`,
  `BatchEditView` **non avevano spec file prima di questo passo** —
  debito preesistente, non introdotto qui, non colmato per non
  allargare l'ambito di questo passo oltre "togliere l'intestazione
  improvvisata".
- `npx vitest run` (suite intera) → 60 file, 372/372 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint .` → un warning reale trovato e corretto: rimuovere il
  blocco in `SharesView.vue` ha spostato l'indentazione attesa del
  markup successivo di 2 spazi (`vue/html-indent`); risolto con
  `--fix` (solo spazi, verificato con `npx vitest run
  SharesView.spec.ts` dopo). Un solo errore nell'intero repo, lo
  stesso preesistente e non correlato di sempre (`PlayerView.vue:51`).
- `npm run build` → bundle iniziale **116.514/153.600** byte gzip
  (variazione trascurabile, tutte le viste toccate sono chunk lazy).

### `MoreView.vue` — Task 6 (7/N): la pagina "Altro" (§6)

Nuova vista a `/more`, elenco piatto SENZA accordion (il documento lo
dice esplicitamente, §6.1/§6.6 — a differenza della sidebar desktop
che usa `NavGroup`, qui non riusato apposta). Tre gruppi, stesse
destinazioni reali di `AppSidebar` (Task 6 1/N e 4/N): Cartelle/Mappa/
Condivisioni; Cestino/Problemi; Utenti/Gruppi solo per un
amministratore.

Scarti dal mockup dichiarati, stesso principio già stabilito per
`AppSidebar`:
- "Condivisi con me"/"Le mie condivisioni" come due righe (§6.3,
  voci 5-6) collassate in una sola "Condivisioni": `SharesView` non ha
  le due schede `state.shareTab` del mockup, è un'unica vista.
- Il valore secondario "N cartelle" della riga "Cartelle": nessun
  conteggio disponibile senza una chiamata dedicata solo per quel
  numero (stesso motivo di `FolderView` senza conteggio foto).
- La sotto-pagina "Cartelle" a card-gradiente (copertina dalla prima
  foto, conteggio foto per cartella): non esiste; "Cartelle" porta
  direttamente a `/folders` (l'albero reale, Task 6 4/N).
- Persone/Preferiti/gruppo IA/Duplicati: stesso debito dichiarato in
  `AppSidebar` (Task 16/15/13).
- Nessuna icona: dichiarato qui esplicitamente (questo frontend non
  ha ancora un sistema di icone — stesso stato di fatto già presente,
  ma mai dichiarato, in `AppSidebar`).

Verifica eseguita:
- `MoreView.spec.ts` (nuovo) → 3/3 verdi: ogni riga è un vero `<a>`
  verso una destinazione reale; "Condivisioni" è una sola riga, non
  due; "Amministrazione" è assente del tutto per un utente non admin.
- `npx vitest run` (suite intera) → 61 file, 376/376 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/modificati → pulito.
- `npm run build` → bundle iniziale **116.561/153.600** byte gzip
  (variazione trascurabile: `MoreView` è un chunk lazy, nessun link
  verso `/more` esiste ancora da nessuna parte della shell — arriverà
  con la tab bar mobile, prossimo sotto-passo).

Prossimo sotto-passo: l'header mobile (freccia indietro + titolo per
vista + pulsante culling + avatar account) e la tab bar (Foto/Cerca/
Album/Altro), cablati in `AppShell`'s `mobile-header`/`mobile-tabbar`
slot — a quel punto `/more` diventa raggiungibile per davvero. Poi —
separatamente — l'area di caricamento nuove foto.

### `AppMobileHeader.vue` + `AppMobileTabbar.vue` — Task 6 (8/N): chiude la shell mobile

Documento funzionale §5 (righe 948-1131), verificato riga per riga.

**Deduplicazione proattiva**: estratta `src/nav/routeTitles.ts`
(`ROUTE_TITLE_KEYS`) da `AppTopbar.vue` — l'header mobile ha bisogno
esattamente della stessa mappa rotta→titolo della briciola desktop
(stesso identico testo per ogni rotta oggi coperta). Fatta prima di
scrivere il secondo consumatore, non dopo: evita la stessa seconda
copia divergente già segnalata più volte in questa sessione
(`formatBytes` triplicata, per esempio) come debito accettato solo
quando unificare tocca codice non correlato — qui i due consumatori
nascono nello stesso passo, unificarli súbito non ha quel costo.

**`AppMobileHeader.vue`**:
- Titolo per rotta: la mappa condivisa, **più** `/more` → "Altro"
  aggiunto qui soltanto. Motivo trovato leggendo il documento (§5.8):
  *"le viste `libreria`/`cartelle` esistono solo nella shell mobile e
  restano montate anche se si torna a Desktop — in quel caso si
  vedono senza briciola"* — quindi `/more` **non** va nella mappa
  condivisa (altrimenti la briciola desktop lo mostrerebbe, contro il
  comportamento dichiarato esplicitamente per quel caso).
- Freccia indietro (§5.3.1): dei tre rami di priorità del mockup, solo
  due sono raggiungibili — "dettaglio album aperto" non esiste (stesso
  debito già dichiarato più volte per la mancanza di uno stato
  "aperto" osservabile dall'esterno della vista). Culling/BatchEdit →
  Foto; ogni altra vista non-radice → Altro.
- Pulsante culling: badge dal dato reale già usato da `AppSidebar`
  (`shell.badges.culling`), visibile solo su `/`; il badge stesso
  sparisce a conteggio zero (non l'intero pulsante).
- Menu account: solo "Esci" (Profilo/Impostazioni non hanno vista,
  Task 14) — stesso ambito già dichiarato per il menu desktop.
- Nessuna icona per la freccia/il pulsante culling (testo/glifo, "←"):
  stesso stato di fatto già dichiarato in `MoreView.vue`.

**`AppMobileTabbar.vue`**: quattro schede reali (Foto/Cerca/Album/
Altro), ordine ed etichette esatte del documento. Regola dell'"attiva"
(§5.7) tradotta alle sole rotte reali: il documento assegna
esplicitamente `culling`/`bulkEdit` alla scheda "Foto" (si entra in
culling solo dal pulsante imbuto della vista Foto) — qui `/culling` e
`/batch-edit` attivano "Foto", non "Altro"; ogni rotta della mappa
di `MoreView.vue` attiva "Altro".

**`App.vue`**: `AppMobileHeader`/`AppMobileTabbar` cablati negli slot
`mobile-header`/`mobile-tabbar` di `AppShell`, accanto a `AppSidebar`/
`AppTopbar` già cablate nel Task 6 (3/N). Chiude la regressione
temporanea dichiarata lì ("sotto i 768px, nessuna intestazione, nessuna
barra a schede") — ora entrambe le larghezze hanno un'impalcatura
reale.

Verifica eseguita:
- `AppMobileHeader.spec.ts` (nuovo) → 9/9 verdi: titolo per rotta
  (incluso "Altro" su `/more`, assente dalla mappa condivisa); freccia
  assente sulle quattro radici; freccia verso `/` da culling/
  batch-edit; freccia verso `/more` da ogni altra vista; scorciatoia
  culling assente fuori da `/`; badge nascosto a zero, mostrato a
  conteggio reale; menu account con nome vero e "Esci" funzionante
  (stesso schema di query su `document.body` già noto per Popover).
- `AppMobileTabbar.spec.ts` (nuovo) → 8/8 verdi: le quattro schede
  esatte come veri `<a>`; una sola scheda attiva per ognuna delle
  quattro radici; "Foto" (non "Altro") attiva su culling/batch-edit;
  "Altro" attiva su una rotta dell'albero di `MoreView`.
- `App.spec.ts` (esteso) → nuovo test: su viewport mobile (matchMedia
  stubbato a `matches:true`), la tab bar (un `<a href="/more">` reale)
  compare al posto di sidebar/topbar (`#topSearch` assente). 3/3 verdi.
- `npx vitest run` (suite intera) → 63 file, 396/396 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/modificati → pulito.
- `npm run build` → bundle iniziale **117.178/153.600** byte gzip.
  Crescita reale e attesa (+617 byte da 116.561): `AppMobileHeader`/
  `AppMobileTabbar` sono ora importati per davvero da `App.vue` (come
  `AppSidebar`/`AppTopbar` nel Task 6 3/N), non più codice morto.
  Margine residuo ampio (36.422 byte).

**Task 6 chiuso** per la parte di impalcatura (sidebar, topbar,
cablaggio in `App.vue`, intestazioni improvvisate tolte da tutte le
viste, shell mobile completa). Resta, dichiarato fin dall'inizio come
blocco a parte: l'area di caricamento nuove foto
(`docs/ui/caricamento-nuove-foto.md`), sopra un `UploadPanel.vue`/
`stores/upload.ts` già corretto e già testato ma oggi inerte (nessun
innesco reale in uso normale, a parte il flusso PWA di condivisione,
esso stesso bloccato — passa sempre `folderId: null` senza modo di
cambiarlo).

# Area di caricamento nuove foto

`docs/ui/caricamento-nuove-foto.md`, letto per intero. "Già corretto e
già testato" riguarda solo il **motore** di `stores/upload.ts`
(upload rispresso a blocchi, ripresa da `localStorage`, pre-check
degli hash) — non l'interfaccia. `UploadPanel.vue` oggi è un pannello
generico fluttuante in basso a destra: esattamente il pattern del
pulsante flottante che il documento dice **esplicitamente scartato**
(§2, tre ragioni verificabili) — va ricostruito, non esteso. Mancano
del tutto: classificazione RAW/video all'ingresso, il chip
destinazione con le sue tre precedenze (§5), il trascinamento su
`#app` (§3.1), il comando "Carica" in topbar (§3.2), il `+` mobile
(§3.3), la striscia in sidebar/sopra la tab bar (§6.1), il pannello
reale a quattro fasce (§6.2), il blocco di rifiuto RAW (§4.1).

Scoperta prima di scrivere codice: `PatchChunkResult` (`api/upload.ts`)
e la risposta di finalizzazione lato backend
(`crates/keeppix-api/src/routes/upload.rs`, `UploadCompleteResponse`)
non portano **nessun** segnale "in preparazione" per i video — il
documento lo elenca fra i requisiti backend (§9.4) ma non esiste,
verificato leggendo la rotta reale, non assunto. A differenza del
badge Culling (Task 6, reale ma sempre zero), qui il campo non esiste
proprio: costruire il badge "IN PREPARAZIONE" ora significherebbe
inventare un segnale, non cablarne uno reale. Rimandato, dichiarato,
non costruito.

Piano di lavoro, un pezzo indipendente e verificabile alla volta
(stesso ritmo del Task 6): (1) classificazione RAW/video/immagine
all'ingresso; (2) estensione del motore per l'assegnazione della
destinazione (oggi `targetFolderId` può restare `null` per sempre,
nessuna interfaccia lo cambia) e i comandi di coda (pausa/riprendi/
annulla tutto); (3) il chip destinazione; (4) il trascinamento su
`#app`; (5) il comando "Carica" in topbar; (6) il `+` mobile; (7) la
striscia sidebar/mobile; (8) il pannello reale a quattro fasce,
sostituendo `UploadPanel.vue` attuale; (9) il blocco di rifiuto RAW.

### `upload/classify.ts` (1/N)

Documento §4, tabella delle estensioni (righe 95-102) trascritta
esatta, non approssimata. `dng` trattato come RAW (nota esplicita del
documento: "è un contenitore RAW a tutti gli effetti"), non come
immagine — un errore facile da fare guardando solo il nome. Divide
sempre il gruppo (accettati / RAW rifiutati / non supportati
rifiutati): non rifiuta mai l'intero rilascio per la presenza di un
RAW o un formato ignoto (§4, "Rifiutare l'intero rilascio sarebbe
ostile").

Verifica eseguita:
- `classify.spec.ts` (nuovo) → 41/41 verdi: ogni estensione della
  tabella (9 immagine, 3 video, 25 RAW) più `.dng` esplicito, un
  formato ignoto, nessuna estensione, e la garanzia che un lotto misto
  non perde il buono per il cattivo.
- `npx vitest run` (suite intera) → 64 file, 437/437 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi → pulito.
- `npm run build` → bundle iniziale **117.178/153.600** byte gzip,
  invariato: `classify.ts` non è ancora importato da nessun
  consumatore reale (arriva nel prossimo sotto-passo).

### `stores/upload.ts` — motore esteso (2/N)

Documento §5 (destinazione) e §6.4 (comandi di coda), verificati riga
per riga contro il motore reale, non riscritto: `addFiles`,
`pump`, `runUpload` (upload a blocchi, ripresa, pre-check) restano
intatti.

**`stickyDestination`** (nuovo, `computed`): la sessione non conclusa
con una cartella già assegnata, se esiste — implementa da sola la
regola 3 del §5 ("non si ridirigono file già partiti"). `addFiles`
ora accetta un `explicitFolderId` opzionale (`null` di default): se
assente, ricade su `stickyDestination`. Copre le regole 1 e 3;
la regola 1 (contesto esplicito di cartella) resta **oggi
irraggiungibile** — nessuna vista porta un "dentro una cartella"
osservabile, stesso debito già dichiarato nel Task 6 per la timeline
filtrata — quindi in pratica ogni caricamento passa sempre da
`stickyDestination` o resta `null` in attesa del chip.

**`setDestination(folderId)`** (nuovo): assegna la cartella a ogni
sessione ancora "queued" senza una, poi avvia la coda — l'azione che
sblocca lo "stato che blocca" del §5, il principio da cui parte
l'intero documento (§1: "il difetto tecnico diventa la spina dorsale
dell'interfaccia").

**`pauseAll`/`resumeAll`/`cancelAll`** (nuovi, §6.4): comandi di coda,
distinti dai `pause(id)`/`resume(id)` per singola sessione già
esistenti. `cancelAll` non può interrompere un `fetch()` già in volo
(nessun `AbortController` in `api/upload.ts`, dichiarato nel commento
del codice, non nascosto) — ferma solo il prossimo blocco tramite un
insieme di id marcati, controllato a ogni giro del ciclo in
`runUpload`. "Azzera la destinazione" (§6.4) non è un'azione a parte:
è una conseguenza di svuotare `sessions`, da cui `stickyDestination`
torna da sola a `null`.

**Bug reale trovato rileggendo il documento**, non nel codice nuovo:
`removeCompleted()` filtrava via solo `done` e `skipped`, non
`error` — il documento dice esplicitamente "Rimuove concluse, saltate
**ed errate**" (§6.4). Con anche un solo caricamento in errore in
coda, "a coda vuota il pannello si chiude da solo" non si sarebbe mai
verificato. Corretto.

**Riorganizzazione dei test**: i cinque test del motore vivevano dentro
`UploadPanel.spec.ts` (nome fuorviante, non testava solo il
componente). Spostati in un nuovo `stores/upload.spec.ts` — non
duplicati — insieme ai test nuovi; `UploadPanel.spec.ts` ora testa
solo il componente.

Verifica eseguita:
- `stores/upload.spec.ts` (nuovo, 5 test spostati + 9 nuovi) → 14/14
  verdi: precedenza 1 assente per costruzione (nessun contesto);
  `setDestination` assegna e avvia; un file aggiunto a coda in corso
  eredita la destinazione (regola 3); un file aggiunto a coda
  **conclusa** non eredita una destinazione stantia (regola 2);
  `pauseAll`/`resumeAll` (incluso il caso "File perso a un refresh" già
  noto per `resume()` singolo); `cancelAll` svuota la coda e la
  destinazione torna `null` da sola; `removeCompleted` rimuove anche
  le sessioni in errore (il bug corretto sopra).
- `UploadPanel.spec.ts` (ridotto al solo test del componente) → 1/1
  verde.
- `npx vitest run` (suite intera) → 65 file, 446/446 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file toccati → pulito.
- `npm run build` → bundle iniziale **117.173/153.600** byte gzip,
  sostanzialmente invariato: `stores/upload.ts` resta dietro
  `UploadPanel.vue`, ancora un `defineAsyncComponent` in `App.vue`.

### Token di colore mancanti — `style.css` (3/N)

Prima di scrivere il chip destinazione: il documento (§7.1) usa
`--accent-tint`, `--warn`/`--warn-tint`/`--warn-border`, `--chip-bg`,
`--card-bg`, `--border-strong` — nessuno di questi esiste ancora nel
`@theme` di questo frontend (solo `surface`/`surface-elevated`/
`content`/`content-muted`/`accent`/`accent-text`/`danger`/`border`).
Aggiunti con gli hex esatti della tabella del documento, chiaro e
scuro, stesso principio già stabilito nel commento in testa al file
("hex esatti... non un'approssimazione"). `--color-warn` è
letteralmente il *"futuro `--warn`"* già anticipato da un commento
esistente su `--color-toast-warn` (Fase 11 Task 1) — non una scelta
nuova, un debito già previsto e ora saldato: il "saltato per
duplicato" non è un errore né un successo, una terza natura
semantica, ambra.

### `DestinationChip.vue` — il chip destinazione (3/N)

Documento §5, verificato riga per riga (righe 126-154) contro il
markup e i colori esatti del prototipo.

**"Nuova cartella…" (riga 139) deliberatamente assente**: il backend
non ha una rotta per creare una cartella — verificato leggendo
`crates/keeppix-api/src/routes/folders.rs` per intero: solo
`tree`/`children`/`relocate`, nessun `create`. Stesso blocco già
dichiarato per il badge "in preparazione" dei video in questo stesso
sottosistema (1/N): costruire quella voce di menu ora significherebbe
un pulsante che non fa nulla di reale.

Riusa `shell.folders` (già caricato da `stores/shell.ts`, Task 6) per
l'elenco — nessuna nuova chiamata API. Stato "manca la destinazione"
(§5): chip `bg-accent-tint`/`border-accent`, valore in corsivo
"Scegli una cartella", riga di rassicurazione sotto. `role="listbox"`
con `aria-selected` sull'opzione attiva (§8), tramite `Popover` (Task
2) già usato per gli altri menu dell'app.

Verifica eseguita:
- `DestinationChip.spec.ts` (nuovo) → 4/4 verdi: stato "manca" con
  testo e classe reali; nome cartella reale una volta risolta;
  l'elenco mostra le cartelle vere e scegliendone una chiama
  `setDestination`; `setDestination` sblocca per davvero una sessione
  in coda senza cartella (non solo un finto stato locale del
  componente).
- `npx vitest run` (suite intera) → 66 file, 451/451 verdi.
- `npx vue-tsc -b` → un errore reale trovato e corretto: import
  `fetchBootstrap` mai letto nel test (mockato per modulo, mai
  referenziato per un'asserzione — a differenza di `AppSidebar.spec.ts`
  che lo controlla).
- `npx eslint` sui file toccati → pulito.
- `npm run build` → bundle iniziale **117.312/153.600** byte gzip
  (+139 byte: le nuove classi Tailwind derivate dai token aggiunti a
  `style.css` sono scansionate dal sorgente, non dal grafo di import —
  compaiono anche se `DestinationChip.vue` non è ancora montato da
  nessuna parte). Margine ampio (36.288 byte).

### `stores/upload.ts` — `addFilesFromPicker` (4/N) e `UploadDropVeil.vue` — il trascinamento (§3.1)

**Store**: nuova `addFilesFromPicker(fileList, explicitFolderId)`,
punto d'ingresso comune a trascinamento/`Carica`/`+` mobile — divide
con `classifyFiles` (1/N) prima di toccare la coda, accumula i nomi
scartati in `rejectedRaw`/`rejectedUnsupported` (nuovi, transitori: mai
persistiti, solo testo per il blocco di rifiuto del pannello — i
`File` scartati non entrano mai in coda, non servono più).
`cancelAll` ora svuota anche queste due liste.

**`UploadDropVeil.vue`**: documento §3.1, verificato riga per riga
**e** contro gli handler reali del prototipo (righe 3085-3103,
7592-7627) — scoperta importante scrivendo il componente: il testo
del documento ("messaggio dedicato" per il rifiuto in Culling)
descrive il comportamento in modo impreciso. Nel mockup reale il velo
mostra **sempre** lo stesso messaggio anche trascinando sopra il
Culling; il rifiuto avviene **al rilascio**, con un toast
(`showToast(..., {kind:'error'})`, testo esatto letto dal codice del
prototipo, non dalla prosa del documento). Costruito secondo il
codice reale, non la sola descrizione testuale — stessa disciplina già
applicata più volte in questa sessione ai casi in cui mockup e
documento non coincidono esattamente.

Variante "dentro una cartella" (`Rilascia per caricare in <nome>`)
omessa: nessuna vista porta oggi un `currentFolder` osservabile — lo
stesso debito già dichiarato più volte, qui applicato una volta di
più invece di scrivere un ramo morto.

`AppShell.vue` (Task 2) riceve `relative` sul contenitore di
topbar+contenuto: l'unica modifica a un primitivo condiviso in questo
sotto-passo, necessaria perché `.drop-overlay` del mockup è
`position:absolute;inset:0` dentro `.main` (sibling della sidebar, non
un suo contenuto) — copre topbar e contenuto, non la sidebar, stessa
area esatta del prototipo (`#dropOverlayHost`, righe 1433-1446 del
mockup).

Verifica eseguita:
- `stores/upload.spec.ts` (esteso, +4 test) → 18/18 verdi:
  `addFilesFromPicker` divide il lotto prima di mettere in coda; un
  lotto di soli scarti non chiama nemmeno il pre-check degli hash; i
  rifiuti si accumulano fra trascinamenti successivi, come la coda;
  `cancelAll` li azzera.
- `UploadDropVeil.spec.ts` (nuovo) → 7/7 verdi: invisibile a riposo
  (nessun elemento nel DOM, non solo nascosto); compare con il testo
  esatto solo se il trascinamento porta file veri; ignora testo/
  immagini trascinati da un'altra scheda; `dragenter`/`dragover`
  chiamano entrambi `preventDefault`; la profondità regge l'attraversare
  un figlio (due `dragenter`, un solo `dragleave` non nasconde); il
  rilascio fuori dal Culling accoda per davvero; il rilascio sul
  Culling mostra il toast esatto e non tocca mai lo store.
- Bug reale trovato scrivendo i test, non nel componente: i primi due
  test su "il rilascio chiama lo store" lasciavano `addFilesFromPicker`
  eseguire per davvero, arrivando fino a una `fetch()` reale non
  mockata (`ERR_INVALID_URL` in Node, un errore non gestito rilevato
  dalla suite intera anche con tutti i singoli test verdi). Corretto
  con `vi.spyOn(...).mockImplementation(async () => {})`: questi test
  isolano il componente, il comportamento reale dello store è già
  coperto altrove.
- `npx vitest run` (suite intera) → 67 file, 463/463 verdi (verificato
  anche senza errori non gestiti, non solo test verdi).
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file toccati → pulito.
- `npm run build` → bundle iniziale **120.005/153.600** byte gzip.
  Salto reale e più grande dei precedenti (+2.693 byte): a differenza
  di `UploadPanel.vue` (dietro un `defineAsyncComponent`),
  `UploadDropVeil` è montato per davvero e subito in `App.vue` — i
  listener devono essere vivi dall'avvio, non dietro un'interazione —
  quindi trascina con sé `stores/upload.ts` e `upload/classify.ts` nel
  chunk d'ingresso per la prima volta. Scelta dichiarata, non un
  effetto collaterale trovato dopo. Margine ancora ampio (33.595 byte).

### `useUploadPicker.ts` — comando "Carica" (§3.2) e "+" mobile (§3.3) (5/N)

Composable condiviso fra `AppTopbar.vue` (bottone "Carica") e
`AppMobileHeader.vue` (bottone "+"), scritto una volta sola per
entrambi i chiamanti invece di due copie dell'input nascosto — stesso
principio già applicato a `nav/routeTitles.ts` (Task 6 8/N). `accept`
esatto dell'input nascosto dal mockup (riga 1449): niente RAW —
solo un suggerimento al sistema operativo, non applicato in modo
uniforme dai browser, quindi `classifyFiles` resta l'unica barriera
vera.

Mobile: visibile solo su `/`, `/albums`, `/more` — le sole tre
destinazioni reali di `MOBILE_UPLOAD_VIEWS` del mockup
(`['foto','preferiti','album','libreria']`, riga 3286; "Preferiti"
resta fuori, nessuna vista esiste).

**Bug reale di `vue-tsc`, non del componente, trovato scrivendo
questo passo**: legare un `ref` restituito da un composable
(`const { inputEl } = useUploadPicker()`) con `ref="inputEl"` nel
template compila e funziona a runtime, ma `vue-tsc` lo segnala come
"mai letto" (`noUnusedLocals`) — la correlazione statica fra un `ref`
di template e la sua dichiarazione sembra riconoscere solo un `ref()`
dichiarato localmente, non uno solo ridestrutturato da una funzione.
Corretto invertendo il controllo: `useUploadPicker(inputEl)` prende il
`ref` come parametro, dichiarato con `ref()` in ciascun componente
chiamante — stesso schema già usato altrove nel codebase (`gridEl` di
`TimelineView.vue`).

Verifica eseguita:
- `useUploadPicker.spec.ts` (nuovo) → 3/3 verdi.
- `AppTopbar.spec.ts` (esteso) → 9/9 verdi: il bottone "Carica" apre
  l'input nascosto e una selezione reale arriva allo store.
- `AppMobileHeader.spec.ts` (esteso) → 16/16 verdi: il "+" compare
  solo sulle tre rotte documentate, sparisce altrove, apre l'input e
  una selezione reale arriva allo store.
- `npx vitest run` (suite intera) → 68 file, 474/474 verdi.
- `npx vue-tsc -b` → pulito (il bug sopra, corretto).
- `npx eslint` sui file toccati → pulito.
- `npm run build` → bundle iniziale **120.694/153.600** byte gzip
  (+689 byte: entrambi i bottoni sono nel chunk d'ingresso, come
  `AppTopbar`/`AppMobileHeader` stessi). Margine ampio (32.906 byte).

### `UploadQueueStrip.vue` — la striscia della coda (§6.1) (6/N)

Documento §6.1, verificato riga per riga **e** contro `renderUploadDock()`/
`uploadCounts()`/`uploadAddFiles()` del prototipo (righe 2733-2937) per
i dettagli che il documento non specifica — etichetta esatta, priorità,
cosa conta come "finito".

**Due scoperte reali, correggono lavoro già spinto in 4/N**, trovate
leggendo `uploadAddFiles()` per intero prima di scrivere questo
componente:
- **I rifiuti sostituiscono, non si accumulano** (`state.upload.rejected
  = (raws.length||others.length) ? {...} : null`, riga 2754): il mio
  `addFilesFromPicker` (4/N) li accumulava fra trascinamenti successivi
  — deciso allora senza aver ancora letto questa riga. Corretto:
  `rejectedRaw.value = raw.map(...)` (assegnazione), non uno spread.
- **Aggiungere file apre sempre il pannello**, anche un lotto di soli
  rifiuti (`state.upload.open = true`, riga 2770) — `addFilesFromPicker`
  ora imposta `panelOpen.value = true`. Conseguenza diretta: la striscia
  **non deve mai** guardare i rifiuti per decidere se mostrarsi
  (`renderUploadDock`, riga 2913, guarda solo `items.length`) — un lotto
  tutto scartato resta visibile lo stesso, tramite il pannello che si è
  aperto da solo, non tramite la striscia.

Store: nuovi `needsDestination` (computed, sessione bloccata su
"queued" senza cartella — più diretto della sola assenza di
`stickyDestination`, che sarebbe vuota anche a coda vuota) e
`panelOpen`/`togglePanel` (stato condiviso fra striscia e pannello,
letto/scritto da entrambi).

Componente unico per le due ancore del documento (piede sidebar
desktop, fascia sopra la tab bar mobile — "solo una delle due esiste
per volta"): stesso principio di `useUploadPicker.ts`, la differenza è
dove il chiamante lo monta. Cablato in `AppSidebar.vue` (sopra "Spazio
libero", posizione esatta) e in `App.vue`, slot `mobile-tabbar` (sopra
`AppMobileTabbar`, non dentro).

Verifica eseguita:
- `stores/upload.spec.ts` (esteso): il test "i rifiuti si accumulano"
  è stato riscritto in "i rifiuti sostituiscono" (verificato contro la
  riga esatta del prototipo); nuovo test per l'apertura automatica del
  pannello anche su un lotto tutto scartato. 23/23 verdi.
- `UploadQueueStrip.spec.ts` (nuovo) → 7/7 verdi: assente a coda vuota
  (nessun elemento nel DOM); assente anche con soli rifiuti (verificato
  contro `renderUploadDock`); le quattro etichette nell'ordine di
  priorità corretto (manca destinazione > in pausa > in corso > finito);
  il clic apre/chiude `panelOpen` condiviso.
- `AppSidebar.spec.ts` (esteso) → 13/13 verdi: la striscia reale compare
  sopra "Spazio libero" con una sessione in coda.
- `App.spec.ts` (esteso) → 5/5 verdi: la striscia compare sia su
  desktop (dentro la sidebar) sia su mobile (sopra la tab bar).
- `npx vitest run` (suite intera) → 69 file, 488/488 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file toccati → pulito.
- `npm run build` → bundle iniziale **121.424/153.600** byte gzip
  (+730 byte: la striscia è nel chunk d'ingresso, come `AppSidebar`/
  `App.vue` stessi). Margine ampio (32.176 byte).

### `UploadPanel.vue` — il pannello reale, quattro fasce (§6.2-§6.4) (7/N)

Sostituisce interamente il pannello generico fluttuante (upload 1/N-6/N
lo annunciavano già come debito): quel pattern era esattamente il
pulsante flottante che il documento §2 dice scartato, non uno stile
diverso dello stesso componente. Verificato riga per riga contro
`renderUploadPanel()`/`uploadRowHTML()`/`uploadCounts()` del prototipo
(righe 2939-3078), non solo il riassunto testuale del documento.

**Deviazione dichiarata dal prototipo, trovata scrivendo questo passo**:
`renderUploadPanel()` (riga 2979, `if(!u.open || !u.items.length){...
return}`) nasconde il pannello anche per un lotto **tutto scartato** —
un rilascio di soli RAW non darebbe nessun riscontro visibile in
quella build, in contraddizione diretta col principio che lo stesso
documento dichiara (§4.1: "il rifiuto dei RAW non è un errore, è una
spiegazione" — una spiegazione che va vista). Il diario di 6/N aveva
già assunto (senza aver letto questa riga fino in fondo) che il
pannello si aprisse comunque per i soli rifiuti — corretto qui: la
condizione di visibilità aggiunge esplicitamente `rejectedRaw`/
`rejectedUnsupported`, non solo `sessions`.

**Bug reale trovato scrivendo i test di questo pannello**: `needsDestination`
(store, aggiunta in 6/N) controllava solo `status==='queued'`, ma
`pause(id)`/`pauseAll()` non controllano la cartella — una sessione
"queued" senza cartella può restare bloccata anche su "paused", non
solo su "queued". Corretto a `stickyDestination.value===null && qualcosa
è ancora in sospeso` — la stessa semantica booleana di `!u.dest &&
c.pending>0` del prototipo (riga 2915/2981), non un'approssimazione.

Struttura a quattro fasce esatta del documento: testata (titolo con
priorità `manca destinazione > in pausa > in corso > completato`,
pausa/riprendi visibile solo quando c'è qualcosa da mettere in pausa e
la destinazione non manca, chiudi che non tocca la coda) — fascia
destinazione (riusa `DestinationChip`, Task 3, primo consumatore reale)
— corpo scorrevole (blocco di rifiuto + righe) — piede (riepilogo +
azioni).

**"Vedi quella presente" è reale, non un rimando**: `useLightboxRoute`
(Task 3, `TimelineView.vue`) risolve `?photo=<id>` anche per un asset
non ancora caricato in pagina — carica da remoto se non lo trova in
locale, lo stesso meccanismo pensato per "mando a un collega il link a
questa foto". Non serviva costruire nulla di nuovo: `router.push({
path:'/', query:{photo: existingAssetId} })` apre per davvero la copia
già presente.

Righe (§6.3): un "done" con `collision==='skipped_duplicate'` (il
server, non il pre-check, trova il duplicato a fine caricamento) si
presenta come "saltato" a tutti gli effetti, stesso trattamento già
nel componente precedente. Il testo di esempio del prototipo per
l'errore ("il server non ha risposto") è solo la sua simulazione demo
— qui la riga mostra la ragione reale già portata dallo store
(`session.error`, sempre una chiave i18n), più corretto della stringa
fissa del mockup.

Token di colore: `--color-danger-tint` mancava dal primo giro (upload
3/N) — serviva solo per il badge "Errore" del pannello, non ancora
scritto allora. Aggiunto ora con l'hex esatto della tabella §7.1.

I18n: `upload.status.*` (della vecchia versione generica) rimossa,
orfana dopo la riscrittura — sostituita da `upload.row.*`/
`upload.panel.*`/`upload.footer.*`/`upload.rejectedRaw.*`/
`upload.rejectedUnsupported.*`, testo esatto del documento/prototipo,
non approssimato. `upload.collision.skipped_duplicate` corretta da
"Già presente in libreria" a "già in libreria" (testo esatto §6.3).

Verifica eseguita:
- `UploadPanel.spec.ts` (riscritto da zero, 27 test) → verdi: visibilità
  (assente a riposo, assente anche con `panelOpen` ma nulla da
  mostrare, presente con una sessione, presente **anche solo con
  rifiuti** — la deviazione dichiarata sopra, scrim mobile che chiude);
  titolo con la priorità esatta delle quattro condizioni; testata
  (pausa/riprendi nascosto se manca la destinazione, i comandi di coda
  reali, chiudi che non tocca `sessions`); le sei righe (coda,
  caricamento con barra, pausa, completato neutro, saltato ambra con
  ragione reale, il caso "done+collision" trattato come saltato,
  "Vedi quella presente" che naviga per davvero, errore rosso con
  "Riprova" reale); il blocco di rifiuto RAW (conteggio e plurale
  esatti, troncamento a quattro nomi con "e un altro"/"e altri N",
  "Apri Culling" reale) e quello dei formati non supportati (nessun
  pulsante lì, corretto); il piede (riepilogo con segmenti condizionali,
  "Pulisci completate"/"Annulla tutto" reali).
- `stores/upload.spec.ts` (esteso) → 22/22 verdi: nuovo test per il bug
  di `needsDestination` sopra.
- Bug reale nei miei stessi test, trovato e corretto prima di finire
  questo passo: `formatBytes(300000)` produce "293 KB" (nessun
  decimale sopra i 10 KB), non "293.0 KB" come avevo prima assunto
  nell'asserzione; il selettore `button:not([aria-label])` per "Vedi
  quella presente" trovava invece il trigger di `DestinationChip`
  (nessun `aria-label` lì), corretto cercando per testo; la lista file
  troncata mancava lo spazio prima di "e un altro" (il prototipo lo
  tiene dentro `uploadAndOthers()` stessa, qui separato in modo
  esplicito nel codice, non nella chiave i18n).
- `npx vitest run` (suite intera) → 69 file, 515/515 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file toccati → pulito; un solo errore nell'intero
  repo, lo stesso preesistente e non correlato di sempre
  (`PlayerView.vue:51`).
- `npm run build` → bundle iniziale **122.116/153.600** byte gzip
  (+692 byte: nuove classi Tailwind scansionate dal sorgente —
  `UploadPanel.vue` resta un chunk lazy separato, verificato in
  `dist/assets/`, non nel bundle d'ingresso). Margine ampio
  (31.484 byte).

**Sottosistema di caricamento sostanzialmente completo**: tutte e tre
le porte d'ingresso (trascinamento, "Carica" desktop, "+" mobile)
classificano e accodano per davvero; la destinazione si assegna e si
sblocca da sola; la striscia e il pannello reale mostrano lo stato
vero della coda, i sei stati, il blocco di rifiuto. Resta, dichiarato
fin dall'inizio come fuori portata di questa sessione: il badge video
"in preparazione" (nessun segnale dal backend, verificato in 1/N) e
"Nuova cartella…" nel menu destinazione (nessuna rotta di creazione
cartella nel backend, verificato in 3/N).

## upload (7b/N) — Esc a livelli sul pannello (§8)

Addendum al passo precedente: `UploadPanel.vue` non chiudeva se stesso
con Esc, solo il popover di destinazione (che già lo fa da solo
tramite il `DismissableLayer` di reka-ui, come da commento esistente
in `Popover.vue`). Aggiunto `tabindex="-1"` e `@keydown.escape="close"`
sulla radice `role="dialog"` del pannello — secondo livello della
sequenza "Esc a livelli" del documento. Debito dichiarato nel commento
del componente: il pannello non si autofocalizza all'apertura, quindi
Esc funziona solo dopo che l'utente ha già spostato il focus tastiera
su un elemento al suo interno con Tab.

Verifica eseguita:
- Nuovo test in `UploadPanel.spec.ts`: Esc sul `role="dialog"` chiude
  il pannello (`upload.panelOpen` torna `false`).
- `npx vitest run src/components/UploadPanel.spec.ts` → 28/28 verdi.
- `npx vitest run` (suite intera) → 69 file, 516/516 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file toccati → pulito; whole-repo → un solo errore,
  lo stesso preesistente di sempre (`PlayerView.vue:51`).
- `npm run build` → bundle iniziale **122.381/153.600** byte gzip
  (variazione trascurabile rispetto al passo precedente). Margine
  ampio (31.219 byte).

# Task 7 — Foto/Timeline (composizione finale), Preferiti, filtro rapido,
# selezione multipla, modifica in blocco

Copre le sezioni 8-13 del documento funzionale (righe 1507-2453):
Foto/Timeline, Preferiti, SP-1 (tile, già costruito nel Task 4), SP-3
(filtro rapido), SP-2 (selezione multipla + barra azioni), Modifica in
blocco. Letto per intero prima di scrivere codice.

Ricognizione preliminare (importante per il piano dell'intero Task): la
maggior parte dei pattern condivisi che questo task consuma **esiste già**
dal Task 2 — `PhotoTile.vue` (SP-1 completo, selezione+cuoricino già
nel componente, solo mai cablato con dati veri), `SelectionBar.vue` (SP-2,
mai montata da nessuna vista), `stores/selection.ts` (due pool paralleli,
libreria+culling, mai consumato), `SelectAllVisible.vue` (SP-4),
`QuickFilter.vue` + `design/quickFilter.ts` (SP-3, generico rispetto alle
dimensioni — dichiaratamente in attesa delle sei dimensioni reali),
`DeleteDialog.vue` (SP-18, le tre opzioni esatte di §12), `Dialog.vue`/
`SegmentedControl.vue`/`Avatar.vue`. Il grosso di questo Task è quindi
**cablaggio** di primitive già verificate, non invenzione di markup nuovo
— cambia il piano dei passi rispetto ai Task precedenti, più veloce dove
il Task 2 ha già coperto il terreno, più lento dove serve dati reali
(le sei dimensioni del filtro) o dialog specifici di schermata (Album,
Condividi) che il Task 2 non copriva.

Gap di backend scoperto leggendo `crates/keeppix-api/src/routes/flags.rs`
per intero: `PUT /assets/{id}/flags` e `POST /flags/batch` sono un
**rimpiazzo completo** del voto (commento russo del campo `favorite`
stesso: «non una patch»), non un PATCH per singolo campo. La sola
`TimelineAsset` letta dalla timeline **non** porta `rating`/`pick`/
`color_label` — scrivere `favorite` da sola su un asset di cui non si
conoscono già gli altri tre campi li azzererebbe in silenzio. Anche
`api/culling.ts`'s `AssetFlags` (frontend) non aveva mai dichiarato
`favorite`, nonostante il backend lo abbia da Fase 10 (`§7bis.1`) — gap
di tipo puro, corretto qui.

## Task 7 (1/N) — Il cuoricino e la selezione multipla in Timeline

**Ambito**: rendere reali i due comandi che `PhotoTile` già sapeva emettere
ma che `TimelineView` ignorava (`:selected="false"`, `:selection-mode=
"false"` erano cablati a fisso), più SP-4 e il montaggio di
`SelectionBar`. I cinque pulsanti d'azione della barra (Preferiti/Album/
Condividi/Modifica/Elimina) e i dialog che aprono sono la prossima unità:
`SelectionBar.vue` è già completa per conteggio/annulla/seleziona-tutte,
ma le azioni restano fuori dal suo perimetro dichiarato (commento del
file: "il chiamante li compone nello slot").

`api/culling.ts`: `AssetFlags` guadagna `favorite: boolean`,
`unvotedFlags.favorite: false` — nessun altro cambiamento, `stores/
culling.ts` non tocca mai questo campo (il culling non ha cuoricini nel
documento).

`stores/favorites.ts` (nuovo): il cuoricino, per singolo tocco (SP-1,
"subito, senza conferma né toast") e di gruppo (SP-2, §12.3, toast
neutro). Ogni scrittura legge prima `fetchFlags` (fallback a
`unvotedFlags` se anche quella fallisce, mai propagare l'errore a un
`GET` che dovrebbe essere innocuo) e fonde `favorite` dentro — mai una
scrittura "nuda". `overlay: Record<string,boolean>` per l'aggiornamento
ottimistico rispetto all'istantanea di `TimelineAsset.favorite`. `setMany`
è sequenziale (stessa motivazione già usata per `removeMany` nel culling
store: una selezione di libreria è decine o centinaia di foto, non
migliaia — l'ordine conta più del parallelismo) e instrada l'esito nel
toast store già esistente (Task 2): successo pieno → il testo esatto del
documento ("Aggiunti ai preferiti."/"Rimossi dai preferiti."); fallimento
parziale → `toast.showPartial` con un ritentativo che copre solo gli id
falliti (nessuna "schermata di riuscita parziale" dedicata esiste ancora
nel frontend — non citata nel piano come già costruita altrove, debito
dichiarato, il toast è la sola forma di riuscita-parziale disponibile
oggi); fallimento totale → `toast.showError`.

`composables/useIsMobile.ts` (nuovo): estratto da `AppShell.vue` al
comparire del secondo consumatore (`PhotoTile`, per `enable-long-press`
— §10.4, tap prolungato solo mobile) — stesso principio già seguito per
`nav/routeTitles.ts` nel Task 6. `AppShell.vue` ora lo consuma invece di
duplicare la logica `matchMedia`; comportamento identico, stesso test.

`TimelineView.vue`: `PhotoTile` riceve ora `:selected`/`:selection-mode`
reali da `selection.library` e `:is-favorite` da `favorites.isFavorite`;
`@toggle-select`/`@toggle-favorite` cablati. `selectionMode` è derivato
dal conteggio (`selection.library.selectedIds.size > 0`), non un flag
separato. **Bug reale trovato e corretto prima del commit**: `store.
library.selectedIds` letto tramite l'istanza reattiva del negozio Pinia è
già **sballato** dal `Ref` (il negozio di setup di Pinia despacchetta i
`ref` annidati a qualunque profondità, non solo al primo livello) —
il primo tentativo scriveva `.value.size`/`.value.has(id)`, che avrebbe
lanciato a runtime (`undefined.size`); confermato leggendo `selection.
spec.ts`, che già accede senza `.value`. La riga strumenti normale
sparisce del tutto in selezione (`v-if="!selectionMode"`) e `<SelectionBar
/>` la sostituisce — ma **mai** dentro un proprio `v-if`/`v-else`: il
commento vincolante nel file del componente dice che la sua regione
d'annuncio deve restare montata anche nell'istante esatto in cui la
selezione si azzera, altrimenti "Selezione annullata" non scatterebbe
mai. Il contenitore attorno a `<SelectionBar>` applica il padding/bordo
solo `:class="selectionMode && '…'"`, non un `v-if` sul componente
stesso — stesso identico bug di montaggio è stato preso e corretto in
questa stessa unità prima del commit (il primo tentativo usava
`v-else`, i test su "conteggio 0 dopo l'annulla" fallivano con
"Cannot call props on an empty VueWrapper").
`SelectAllVisible` seleziona `loadedAssets` ("ciò che è visibile ora",
non l'intera libreria — SP-4 §11.2 — coincide con "già caricato" finché
il filtro rapido non esiste ancora in questa vista, stessa motivazione
già dichiarata per `startCulling()`). "Rinomina cartella…" (§8.3)
resta fuori: questa vista non ha un concetto di "cartella aperta"
(debito preesistente, non nuovo di questa unità).

Verifica eseguita:
- `stores/favorites.spec.ts` (nuovo, 10 test) → verdi: lettura dei flag
  correnti prima di scrivere, ottimismo, toggle avanti/indietro,
  rollback su fallimento (senza toast di successo), fallback a
  `unvotedFlags` se anche `fetchFlags` fallisce, `setMany` no-op su
  selezione vuota, testo esatto dei due toast di gruppo (§12.3),
  rollback selettivo + toast parziale su fallimento parziale, toast
  d'errore secco su fallimento totale.
- `TimelineView.spec.ts` (esteso, +6 test): cuoricino che fonde nei
  flag correnti; check che entra in selezione e sostituisce la riga
  strumenti; click sul corpo della tile in selezione che seleziona
  invece di aprire il lightbox; "Seleziona tutto quello che vedi";
  × che annulla e ripristina la riga normale.
- `npx vitest run` (suite intera) → 70 file, 531/531 verdi.
- `npx vue-tsc -b` → pulito dopo la correzione del binding
  `:aria-label` → `:ariaLabel` su `<SelectionBar>` (vue-tsc non
  camelizza un binding kebab-case quando il nome coincide con un
  attributo ARIA nativo, anche se il componente dichiara un prop
  proprio con quel nome — un solo avviso ESLint nuovo,
  `vue/attribute-hyphenation`, coerente con la baseline di 142 avvisi
  preesistenti già osservata in questa sessione, mai zero avvisi).
- `npx eslint` sui file toccati → pulito salvo l'avviso sopra.
- `npm run build` → bundle iniziale **122.510/153.600** byte gzip
  (+129 byte, trascurabile — `TimelineView.vue` resta un chunk lazy
  separato). Margine ampio (31.090 byte).

## Task 7 (2/N) — I cinque pulsanti della barra di selezione (§12.3)

**Ambito**: rendere reali quattro dei cinque pulsanti che `SelectionBar.vue`
lasciava esplicitamente al chiamante (commento del file: "il chiamante li
compone nello slot") — Preferiti, Album, Modifica, Elimina. Il quinto,
Condividi, **volutamente omesso**, non uno stub: vedi il gap di backend
sotto.

**Gap di backend confermato leggendo per intero `crates/keeppix-db/src/
share_links.rs` e `crates/keeppix-db/src/permissions.rs`**: un link di
condivisione esiste solo per `object_type` `folder`/`album`/`asset`, e
`asset` **conta sempre 1** (`item_counts`, commento del codice stesso: "un
link asset conta sempre 1, senza query") — mai una selezione arbitraria di
N foto. Le concessioni di permesso a persone già invitate (`permissions.rs`
riga 502) coprono solo `object_type IN ('folder','album')`, di nuovo mai
un asset singolo né una selezione. Il dialog "Condividi N elementi" del
documento (§12.3: link pubblico + concessione a persone invitate, per una
selezione ad hoc) **non ha alcun corrispondente possibile nel backend
attuale**, né come link né come permesso — stessa natura di gap già
dichiarata altrove in questa sessione (badge video "in preparazione",
"Nuova cartella…" nel menu destinazione): niente pulsante che non farebbe
nulla di reale, il pulsante semplicemente non esiste finché il backend non
lo supporta.

`components/AlbumPickerDialog.vue` (nuovo): interruttore di gruppo per
album (§12.3) — "attiva/disattiva un album aggiunge o rimuove tutti gli
elementi". Gli album dinamici del documento ("N album dinamici non
mostrati qui") **non esistono in questo backend**: verificato leggendo
`crates/keeppix-api/src/routes/albums.rs` per intero, nessun campo
`kind`/`is_dynamic` da nessuna parte — confermato anche dal piano stesso
("Gli album dinamici non esistono", decisione del 20 agosto, ambito del
Task 12 non ancora costruito). Ogni album da `fetchAlbums()` è quindi
"manuale" per costruzione: niente da filtrare, niente nota da mostrare.
L'appartenenza di gruppo ("sono già tutte dentro?") si deduce da
`fetchAlbum(id).assets`, un `fetchAlbum` per album all'apertura del
dialog (nessun endpoint di sola-appartenenza esiste, e il numero di album
è tipicamente piccolo — non merita un endpoint apposito solo per questo
dialog). Aggiungere è un'unica chiamata bulk (`addAssets`); togliere è
sequenziale, una `removeAsset` per foto (nessuna versione bulk
dell'endpoint di rimozione esiste).

`components/LibrarySelectionActions.vue` (nuovo): i quattro pulsanti reali,
estratto come componente **proprio** (non inline in `TimelineView`) perché
il documento stesso dichiara Preferiti come secondo consumatore già noto
(§9.3, Preferiti: "SP-2 completa, tutti e cinque i pulsanti") — stesso
principio già seguito per `nav/routeTitles.ts` e `useIsMobile.ts`: dedup
proattivo quando il secondo consumatore è già certo, non speculativo.
- **Preferiti**: toggle di gruppo via `favorites.setMany`, verso deciso da
  "tutte le selezionate sono già preferite?" (§12.3).
- **Album**: apre `AlbumPickerDialog`.
- **Modifica**: `router.push('/batch-edit', {query:{ids}})` — la vista
  reale di "Modifica multipla" (§13) è la prossima unità di questo Task;
  la rotta esiste già (usata anche da `CullingView`).
- **Elimina**: riusa `DeleteDialog.vue` (SP-18, già costruito nel Task 2
  e mai consumato — la sua stessa nota di intestazione lo descrive per
  §12, non per il culling, che ha un proprio dialog bespoke separato,
  verificato leggendo `CullingView.vue`: non usa mai `DeleteDialog.vue`).
  **Deviazione dichiarata dal testo letterale del documento**: §12.3 dice
  "ogni foto selezionata riceve `pick='reject'` e la scelta di
  smaltimento" — ma quel voto è pura contabilità del prototipo (uno stato
  client-side che sopravvive nella demo dopo la rimozione dalla lista
  visibile); qui l'asset smette di esistere nell'indice (o va in
  cestino/su disco) con la sola chiamata a `deleteAsset`, e non c'è alcun
  voto da preservare — stesso comportamento già in uso da
  `CullingStore.removeMany`, che chiama `deleteAsset` da solo, senza un
  voto separato prima. Cancellazione sequenziale (stesso principio già
  usato per `removeMany`/`favorites.setMany`), toast partial-aware sullo
  stesso modello del resto della sessione.

`TimelineView.vue`: nuovo `selectedAssets` (computed, filtra
`loadedAssets` sugli id selezionati — servono gli oggetti
`TimelineAsset` completi, non solo gli id, sia per `favorites.setMany`
sia per `AlbumPickerDialog`), passato come prop a
`<LibrarySelectionActions>` dentro lo slot di `<SelectionBar>`.

Verifica eseguita:
- `AlbumPickerDialog.spec.ts` (nuovo, 5 test) → verdi: elenco completo
  degli album; "acceso" solo quando **tutte** le selezionate sono già
  membri (non "almeno una"); aggiunta bulk in una chiamata; rimozione
  sequenziale foto per foto; "Fatto" chiude. **Bug reale trovato e
  corretto prima di finire questo passo**: `watch(open, cb)` senza
  `{immediate:true}` non caricava mai gli album quando il dialog era già
  aperto al montaggio (il caso normale nei test, e nell'uso reale
  quando il chiamante apre il dialog e lo monta nello stesso istante) —
  un `watch` senza `immediate` scatta solo su un **cambiamento**
  successivo, mai sul valore iniziale. Un secondo problema nei miei
  stessi test, non nel componente: `wrapper.find`/`findAll` non vedono
  mai il contenuto di `DialogPortal` (teletrasportato nel vero
  `document.body`, fuori dal sottoalbero DOM del wrapper) — corretto
  interrogando `document.body.querySelectorAll` direttamente, stesso
  identico pattern già stabilito da `DeleteDialog.spec.ts`.
- `LibrarySelectionActions.spec.ts` (nuovo, 6 test) → verdi: esattamente
  quattro pulsanti (**nessun quinto "Condividi"**, verifica esplicita
  dell'omissione); Preferiti aggiunge quando non tutte sono già
  preferite, toglie quando lo sono già tutte, col testo esatto dei due
  toast; Modifica naviga a `/batch-edit` con gli id in query; Elimina
  applica la `DiskAction` scelta a ogni asset e azzera la selezione; il
  toast finale ripete il testo esatto del documento ("1 foto
  eliminata."/plurale).
- `TimelineView.spec.ts` (+1 test): la barra di selezione porta i veri
  oggetti `TimelineAsset` selezionati a `LibrarySelectionActions`, non
  solo gli id.
- `npx vitest run` (suite intera) → 72 file, 545/545 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file toccati → pulito; whole-repo → un solo errore,
  lo stesso preesistente di sempre (`PlayerView.vue:51`); 143 avvisi
  (+1 rispetto al passo precedente, lo stesso `vue/attribute-hyphenation`
  già osservato, nessun avviso nuovo di categoria diversa).
- `npm run build` → bundle iniziale **122.932/153.600** byte gzip
  (+422 byte, trascurabile — `AlbumPickerDialog`/`LibrarySelectionActions`
  vivono dentro il chunk lazy di `TimelineView`, non nel bundle
  d'ingresso). Margine ampio (30.668 byte).

## Task 7 (3/N) — Preferiti (§9)

**Scoperta chiave, prima di scrivere qualunque riga**: `runSearch` (già
costruita nel Task 3/piano backend, `POST /api/v1/search`) supporta già
`SearchNode::Favorite` — verificato leggendo per intero
`crates/keeppix-db/src/search.rs` (riga 77, variante unitaria,
`#[serde(tag="op")]` la serializza come `{op:'favorite'}` da sola). Non
serve **nessun** cambiamento di backend per questa vista: la selezione
delle sole foto preferite è già un filtro di ricerca pronto, mai
consumato dal frontend finora (`api/search/parse.ts` non aveva questa
variante nel proprio union type — aggiunta qui, un solo membro, additiva).
Questo ha reso trattabile un lavoro che sulla carta sembrava toccare il
backend (le sei dimensioni di SP-3, la prossima unità di questo Task,
**quelle sì** lo toccano davvero — vedi la nota di scope sotto).

**Nessun endpoint di sola geometria per una lista piatta**: a differenza
della timeline (Task 4, `fetchGeometry`, un blob binario precalcolato per
l'intera libreria), qui non serve — `runSearch` restituisce gli stessi
`TimelineAsset` con `width`/`height` già inclusi, e `justify()` (Task 4,
pura, mai legata al blob di geometria) basta da sola per il layout
giustificato. Nessun tetto sul caricamento (§9.2, "stessa virtualizzazione
SP-22"): tutte le pagine vengono seguite per `next_cursor` fino
all'esaurimento al montaggio, non una alla volta per finestra visibile
come fa invece la timeline per mese — non ce n'è bisogno, i preferiti
sono per definizione un sottoinsieme limitato della libreria, non l'intera
libreria.

**La griglia visibile è derivata, non uno snapshot**: `visibleAssets =
assets.filter(favorites.isFavorite)`, non l'elenco caricato al montaggio.
Questo dà **gratis** il comportamento esatto di §9.3 ("il cuoricino qui
toglie la foto dalla vista... senza conferma, senza toast, senza
annulla"): lo stesso identico gestore `@toggle-favorite="favorites.
toggleOne(...)"` già usato in Timeline, senza bisogno di un handler
diverso qui — l'unica differenza è che questa vista **guarda** la
sovrascrittura ottimistica dello store condiviso per decidere cosa
disegnare, la timeline no. Stesso principio vale per il pulsante
"Preferiti" della barra di selezione (`LibrarySelectionActions`, Task 7
2/N, riusato tale e quale): su una selezione già tutta preferita
(sempre vero qui) rimuove sempre, mai aggiunge — nessun codice nuovo,
la stessa logica di gruppo del 2/N già lo fa.

`composables/useDensity.ts` (nuovo): estratto da `TimelineView.vue` al
comparire del secondo consumatore (stesso principio di `useIsMobile.ts`/
`nav/routeTitles.ts`) — stessa chiave di `localStorage`, così cambiare
densità in una vista la cambia anche nell'altra alla prossima visita
(coerente con "la densità è un interruttore globale in Impostazioni",
non un controllo per-vista, doc riga 1745).

**I due stati vuoti canonici di §9.2**, entrambi reali: "Nessun preferito
ancora" (icona cuore, **nessuna barra strumenti disegnata affatto** — non
solo nascosta, proprio assente) quando `totalCount===0`; "Nessuna foto
corrisponde ai filtri" quando ci sono preferiti caricati ma
`visibleAssets` è vuoto. Quest'ultimo testo è condiviso in anticipo con
SP-3 (Task 7, prossima unità): aggiunto sotto `ui.filteredEmpty.*`, non
`favorites.*`, perché la Timeline lo userà identico quando il pannello
filtri sarà cablato — stesso principio di estrazione anticipata di
`useDensity`/`useIsMobile`. Oggi, senza filtro rapido ancora cablato su
questa vista, l'unica causa raggiungibile è togliere il cuoricino
all'ultima foto visibile in sessione — stessa identica situazione visiva
("avevi delle foto, ora non ne vedi nessuna") che il pannello filtri
produrrà più avanti, non un riuso forzato.

**Navigazione**: rotta `/favorites` (inglese, come tutte le altre —
`/folders`, `/shares`, `/trash`… **non** `/preferiti`, coerenza con la
convenzione già in uso, non lo spagnolo/italiano dei percorsi del
mockup). Voce "Preferiti" aggiunta in `AppSidebar.vue` (fra "Cartelle" e
"Album", posizione esatta di §2.3, voce 8 prima di voce 9 nell'elenco
canonico) e in `MoreView.vue` (gruppo Libreria, §9.8: "da mobile
passando dalla tab 'Altro' → elenco Libreria → 'Preferiti'"). Aggiunta a
`ROUTE_TITLE_KEYS` (`nav/routeTitles.ts`, Task 6 8/N) — copre sia la
briciola desktop sia il titolo mobile con una sola riga, nessun duplicato.

Verifica eseguita:
- `search/parse.ts`: nuovo membro `{op:'favorite'}` nell'union
  `SearchNode`, puramente additivo (nessuno switch esaustivo altrove lo
  consuma, verificato con una ricerca sul repo).
- `FavoritesView.spec.ts` (nuovo, 10 test) → verdi: caricamento con
  `runSearch({op:'favorite'})` che segue `next_cursor` fino
  all'esaurimento; sottotitolo con il conteggio totale esatto; primo
  stato vuoto **senza alcuna barra strumenti** (nessun pulsante nel DOM,
  non solo nascosto); secondo stato vuoto raggiunto togliendo il
  cuoricino all'ultima foto; il cuoricino che fa sparire la tessera
  subito; "Seleziona tutto quello che vedi" sul solo visibile;
  selezione che entra in modalità e porta il conteggio reale;
  lightbox che apre per `?photo=`; `ErrorState` classificato con
  ritentativo; ricarica su evento live `assets.upserted`.
- `useDensity.spec.ts` (nuovo, 5 test) → verdi: default 6, lettura,
  clamp, scrittura persistita, **due chiamate indipendenti condividono
  la stessa chiave** (verifica esplicita della sincronizzazione
  Timeline/Preferiti).
- `TimelineView.spec.ts` (invariato nel comportamento, verificato dopo
  il refactor di `useDensity`) → ancora 20/20 verdi.
- `AppSidebar.spec.ts`/`MoreView.spec.ts` (+1 test ciascuno): "Preferiti"
  presente, posizionata correttamente prima di "Album"
  (`AppSidebar`)/presente nell'elenco Libreria (`MoreView`).
- `npx vitest run` (suite intera) → 74 file, 562/562 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file toccati → pulito; whole-repo → un solo errore,
  lo stesso preesistente di sempre (`PlayerView.vue:51`); 144 avvisi
  (+1, lo stesso `vue/attribute-hyphenation` di `:ariaLabel` già
  osservato due volte, qui sulla stessa identica riga di `FavoritesView`
  copiata da `TimelineView`).
- `npm run build` → bundle iniziale **123.249/153.600** byte gzip
  (+317 byte, trascurabile — `virtualize.ts`/`justify.ts`/`stream.ts`
  sono ora un chunk condiviso separato fra i due chunk lazy di
  `TimelineView`/`FavoritesView`, mai nel bundle d'ingresso, verificato
  in `dist/assets/`). Margine ampio (30.351 byte).

**Nota di scope per la prossima unità (SP-3, filtro rapido)**: a
differenza di Preferiti, il pannello filtri **tocca davvero il
backend**. Verificato leggendo `crates/keeppix-api/src/routes/
timeline.rs` per intero: `AssetView` non porta fotocamera, tag
confermati (con categoria) né volti confermati (con persona) — solo i
campi già noti a `TimelineAsset`. Le rotte `faces.rs`/`persons.rs`/
`tags.rs` esistono sul backend (Fase 8/Fase 9, già mersate) ma **nessun
modulo API frontend le consuma ancora** (verificato: `frontend/src/api/`
non ha `tags.ts`/`persons.ts`/`faces.ts`). Estendere `AssetView` per
portare questi campi (o un endpoint di join dedicato) è lavoro di
backend reale, non cablaggio — va scoping a parte prima di procedere,
non improvvisato dentro questa stessa unità.

**Corretto dopo verifica diretta sul codice reale** (indicazione
dell'operatore, non presunzione mia): quasi tutto il necessario esiste
già, va solo collegato — verificato riga per riga prima di scrivere
qualunque cosa, non per sentito dire:
- `AssetRepo::move_asset` (Fase 9 Task 1) esiste già, mai esposta da una
  rotta.
- `camera_make`/`camera_model` sono già colonne reali su `asset_exif`
  (migrazione 0005), già indicizzate (`asset_exif_camera_idx`).
- `GET /tags` e `GET /persons` esistono già dalla Fase 7/8, mai
  consumate dal frontend — bastano per i menu a discesa, nessun
  endpoint nuovo serve lì.
- **Scoperta non anticipata**: `POST /assets/batch/rename*` (Fase 9
  Task 10, `RenameRepo`) è **già completo** — anteprima, applicazione
  con `operation_id` tracciato, annullamento — mai esposto al frontend.
  "Rinomina con formula…" (§13.3 campo 7) è quindi **puro cablaggio**
  anch'esso, scoperto solo leggendo `crates/keeppix-api/src/lib.rs` per
  intero per trovare dove aggiungere la rotta di spostamento.

## Task 7 (4/N) — Il terreno di backend per SP-3 e Modifica in blocco

**Ambito**: solo backend + i moduli `api/*.ts` che lo rendono
raggiungibile — nessun cablaggio in una schermata reale ancora (SP-3 e
Modifica in blocco sono le prossime unità). Tre pezzi nuovi, uno
scoperto già pronto:

**1. `POST /assets/batch/move`** (nuovo modulo
`routes/asset_move.rs`) — su `AssetRepo::move_to_folder` (nuovo metodo
in `keeppix-db/src/assets.rs`: wrapper sottile su `move_asset` che
legge il nome corrente e lo passa invariato, per "sposta senza
rinominare" di §13.3 campo 8). Stesso pattern di `flags::batch_set`:
ciclo sequenziale, esito in `BulkOutcome` — non l'involucro con
`operation_id` di `rename.rs`, che serve al progresso `WebSocket` di un
batch tracciato; uno spostamento di cartella non ha né anteprima né
annullamento nel documento (§13.3).

**2. `POST /tags/{id}/assets/batch`** (nuovo handler `assign_batch` in
`routes/tags.rs`) — su `AssetTagRepo::assign` (nuovo metodo in
`keeppix-db/src/asset_tags.rs`): scrive sempre `state='confirmed',
source='user'`, anche sopra un rifiuto precedente — a differenza di
`confirm()` (che transita solo da `'proposed'` ed è in conflitto su un
`'rejected'`), qui la persona sta decidendo *ora*, non risolvendo una
proposta IA passata (SP-12, §13.3 campo 5: "un'aggiunta manuale è già
una conferma, non passa dalla coda di revisione").

**3. `AssetView` esteso** (`camera_model`, `tags`, `faces` — campi
additivi) per SP-3 §11. Tre nuovi metodi bulk, stesso idioma di
`FlagRepo::favorites_among` (una query sola per l'intera pagina, mai
una per riga):
- `AssetRepo::camera_models_among` — `asset_exif`, nessun gate
  pgvector (schema core, migrazione 0005).
- `AssetTagRepo::confirmed_among` — solo `state='confirmed'`;
  `category_id` è il `parent_id` del tag stesso (le "categorie" del
  documento sono tag con `kind='category'`, non una tabella a parte —
  nessun secondo giro per risolverle). Gate pgvector: mappa vuota, non
  errore, se l'estensione manca (`tags`/`asset_tags` non esistono
  affatto in quel caso, migrazione 0043).
- `FaceRepo::confirmed_among` — `person_id IS NOT NULL AND rejected_at
  IS NULL`, sia assegnato a mano sia dal raggruppamento automatico:
  entrambi sono un'identità stabilita, a differenza di
  `proposed_person_id` (un suggerimento non ancora deciso). Stesso gate
  pgvector di sopra (migrazione 0046).

Estratta `enrich_views` (funzione condivisa in `routes/timeline.rs`,
`pub(crate)`) per non duplicare la sequenza "raccogli gli id → quattro
bulk-fetch → fondi" fra `/timeline` e `/search`, che la duplicavano già
solo per `favorite` prima di questa unità — occasione per accorciarla
invece di allungare la duplicazione aggiungendo i tre campi nuovi in
entrambi i posti. Anche `GET /assets/{id}` (dettaglio singolo, usato
dal lightbox per un link diretto) ora porta camera/tag/volti, con gli
stessi tre metodi bulk chiamati su una slice di un solo id — nessun
terzo percorso di codice a parte.

**Frontend**: solo i moduli `api/*.ts` che rendono tutto questo
raggiungibile — `api/assets.ts` (`moveAssetsBatch`), `api/tags.ts`
(`fetchTags`, `assignTagBatch`), `api/persons.ts` (`fetchPersons`),
`api/rename.ts` (`previewRename`/`applyRenameBatch`/`undoRenameBatch`,
il cablaggio "puro" scoperto sopra) — e `TimelineAsset` esteso con
`camera_model`/`tags`/`faces` in `api/timeline.ts`. **Nessuna vista
ancora li consuma**: quello è SP-3 e Modifica in blocco, le prossime
unità.

Verifica eseguita — **con un limite dichiarato**: questo sandbox non ha
Docker (`docker ps` fallisce, nessun demone) né un Postgres locale
(`pg_isready` non risponde), quindi **nessun test che tocca davvero il
database ha potuto girare qui** — gireranno in CI, dove Docker esiste.
Quanto segue è il massimo verificabile senza un database reale:
- `cargo check --workspace` → pulito (richiede `ORT_LIB_LOCATION=/root/
  ort-lib ORT_PREFER_DYNAMIC_LINK=1` per collegare l'ONNX runtime
  locale invece di scaricarlo — il proxy della sandbox blocca il
  download; `LD_LIBRARY_PATH=/root/ort-lib` in più per *eseguire*
  qualunque binario, non solo compilarlo).
- `cargo clippy --workspace --all-targets -- -D warnings` → pulito,
  inclusi tutti i nuovi file di test (un `#[allow(clippy::
  too_many_lines)]` aggiunto a un test con molta seed, stesso motivo
  già presente su `review_queue_lists_confirms_rejects_and_updates_
  bootstrap_revision` in `tags.rs`).
- `cargo check -p X --test Y` mirato su ognuno dei sei file di test
  toccati/nuovi (`keeppix-db`: `asset_tags`, `assets`, `faces`;
  `keeppix-api`: `asset_move`, `tags`, `timeline`) → tutti puliti.
- `cargo test -p keeppix-api --test openapi` → **6/8 verdi** dopo aver
  aggiornato i due elenchi letterali che il test stesso vieta di
  rigenerare automaticamente (`security_requirements_name_a_declared_
  scheme`, `operation_ids_are_explicit_and_unique` — nuove voci
  inserite in ordine alfabetico, non appese) e `UPDATE_OPENAPI=1` una
  volta per rigenerare `docs/api/openapi.json` (il test stesso lo
  autorizza per **sole aggiunte** entro `/api/v1`, con la frase esatta
  da spiegare nel commit: fatto qui). I 2 test ancora rossi
  (`documented_operations_are_all_mounted`,
  `openapi_summaries_do_not_contain_errors_heading`) falliscono solo
  su `SocketNotFoundError("/var/run/docker.sock")` — ambiente, non
  codice: entrambi montano un `TestServer` reale via testcontainers.
- Nuovi test scritti seguendo alla lettera le convenzioni già in uso
  (harness `TestDb`/`TestServer`, stessi helper di seed, stesso stile
  di asserzione) ma **mai eseguiti**, solo compilati: 3 in
  `asset_tags.rs` (assign da zero, assign sopra un rifiuto precedente
  — la deviazione dichiarata da `confirm` —, idempotenza e permessi;
  più 2 su `confirmed_among`), 3 in `faces.rs` (`confirmed_among` con
  assegnazione mano+automatica insieme, esclusione di rifiutati e
  proposte non decise, lista vuota), 2 in `assets.rs`
  (`move_to_folder` non rinomina, `camera_models_among` con/senza riga
  exif), 3 in `asset_move.rs` (spostamento in blocco, una collisione
  che non blocca il resto del lotto, il tetto di batch), 1 in
  `tags.rs` (`assign_batch` end-to-end via HTTP), 2 in `timeline.rs`
  (camera/tags/faces vuoti con grazia senza pgvector; popolati per
  davvero con pgvector, via `TestServer::start_with_vector()`).
  **Debito dichiarato**: verificheranno per la prima volta in CI, non
  qui — coerente con la disciplina già seguita in questa sessione per
  tutto ciò che questo sandbox non può eseguire, mai taciuto.
- Frontend (dopo l'estensione di `TimelineAsset`): `npx vitest run` →
  74 file, 562/562 ancora verdi; `npx vue-tsc -b` → pulito (9 file di
  test con una propria `function photo()` aggiornati con i tre campi
  nuovi, tutti trovati con una ricerca esplicita, non uno alla volta a
  tentoni); `npx eslint` sui file nuovi/toccati → pulito.
- **Incidente ambientale, risolto**: a metà di questa unità il
  filesystem si è riempito (`cargo test --workspace --no-run` stava
  compilando ogni binario di test insieme, ~26 GB in `target/`) — non
  un difetto del codice, la cartella di build di un intero workspace
  Rust con dipendenze pesanti (aws-sdk, tokenizers, ort). Risolto con
  `cargo clean` (26 GB liberati); tutte le verifiche sopra erano già
  state fatte prima dell'incidente, nessuna ripetuta con dati stantii.

## Task 7 (5/N) — `useBrowseFilters`: le sei dimensioni reali di SP-3 (§11)

**Ambito**: solo il composable che dà dati veri alle sei dimensioni
che `QuickFilter.vue` (Task 2) aspettava già, generico, dalla sua
stessa nascita — nessun cablaggio in `TimelineView.vue`/
`FavoritesView.vue` ancora, quella è la prossima unità di questo
stesso Task 7.

`design/quickFilter.ts`'s `matchesFilters` fa già AND-fra-dimensioni/
OR-dentro-una-dimensione (§11.3): trattare "Tag" e "Categorie" come
due voci separate di `dimensions[]`, invece di un caso speciale,
riproduce da sola l'esempio del documento senza logica dedicata; una
"Persone" con lista vuota (nessuna persona visibile con almeno un
volto, §11.2) non compare affatto nel pannello e non serve nemmeno un
controllo esplicito nella combinazione — se una selezione Persone
restasse comunque impostata, `getValues` non restituirebbe mai quel
valore e `matchesFilters` la azzera da sola (il "ritorna falso secco"
del documento, ottenuto gratis dalla genericità, non da un `if` in
più).

Le sei dimensioni e le loro fonti, verificate sul codice reale prima
di scrivere, non presunte:
- **Tipo** — tre opzioni fisse RAW+JPEG/RAW/JPEG, nessuna fonte dati.
- **Persone** — `GET /persons` (`api/persons.ts`, nuovo), filtrato a
  `!hidden && face_count > 0` (stesso criterio di "visibili con
  almeno una foto" già usato altrove in questa sessione per lo stesso
  endpoint).
- **Tag** / **Categorie** — `GET /tags` (`api/tags.ts`, già esistente
  da Task 7 (4/N)), diviso per `kind`.
- **Fotocamera** — `camera_model` distinto fra gli asset già caricati
  in memoria (nessuna chiamata di rete: il campo arriva già dentro
  `TimelineAsset` da Task 7 (4/N)).
- **Luogo** — `shell.folders`, già in store, nessuna chiamata nuova.

Nuovo namespace i18n `browseFilter.*` (`it.json`/`en.json`, sette
chiavi + tre `typeOption.*`), nessun endpoint nuovo lato backend: la
premessa del messaggio dell'utente ("controlla PRIMA se bastano
quelli esistenti") si è confermata su tutti e sei i punti.

Verifica eseguita per intero:
- `npx vitest run src/composables/useBrowseFilters.spec.ts` → 9/9
  verdi (dopo un fix di tempistica: il primo giro usava due
  `$nextTick()` in sequenza per aspettare il `Promise.all` di
  `onMounted`, insufficiente; sostituito con `flushPromises()` da
  `@vue/test-utils`, stesso correttivo già usato con successo in
  questa sessione su `AlbumPickerDialog.spec.ts`).
- `npx vitest run` (suite intera) → 75 file, 571/571 verdi.
- `npx vue-tsc -b` → un errore reale trovato e corretto (`wrapper`
  distrutto ma mai letto in quattro test, residuo del passaggio da
  `$nextTick()` a `flushPromises()`: rimosso dalla destrutturazione),
  poi pulito.
- `npx eslint` su `useBrowseFilters.ts`/`.spec.ts` → pulito.
- `npm run build` + calcolo manuale del bundle iniziale gzip (stesso
  algoritmo esatto di `.github/workflows/ci.yml`: somma di
  `index-*.js` + `client-*.js` + `index-*.css`, i soli asset che
  `dist/index.html` carica subito) → 123.150 byte, sotto il budget di
  153.600 (nessuna vista consuma ancora il composable, quindi questo
  numero non riflette il costo finale del cablaggio — sarà
  ricontrollato quando `TimelineView.vue`/`FavoritesView.vue` lo
  importeranno davvero).

## Task 7 (6/N e 7/N) — SP-3 cablato: `FlatAssetGrid.vue`, Preferiti e Timeline

**Ambito**: il pezzo che mancava dopo Task 7 (5/N) — montare davvero
`QuickFilter.vue`/`useBrowseFilters` in Preferiti e Timeline, non solo
avere i dati pronti. Un solo punto non aveva un primitivo diretto da
riusare, verificato leggendo il codice reale prima di scrivere, non
presunto — riportato qui per intero come da istruzione ("fermati su
quel pezzo preciso e dimmelo"), risolto senza inventare una forma
nuova:

**Il punto**: `planStream`/`TimelineGeometry` (Task 4) presumono
sempre il **mese intero** — `GridCell.offsetInMonth` indicizza dentro
un blob binario calcolato lato server su tutti gli scatti di quel
mese, non su un sottoinsieme. Un filtro rapido attivo non ha un modo
di "ritagliare" celle da righe pensate per un mese intero senza
rompere la geometria che le ha generate — non è "non l'ho ancora
vista", è strutturale: la geometria non ha un concetto di sottoinsieme
filtrato. Il documento stesso lo conferma indirettamente: il
contatore del piede filtro è "calcolato sulla lista **di questa
vista**" (§11, `applyBrowseFilters(scopedList)`), esattamente il
principio già usato per Preferiti e per il debito dichiarato di
culling/lightbox ("quanto già caricato", non l'intera libreria) — un
elenco piatto in memoria, non un blob di geometria.

**La soluzione, senza forma nuova**: quando un filtro è attivo, la
vista Timeline abbandona il blob di geometria e passa alla stessa
identica griglia giustificata e virtualizzata già usata da Preferiti
(`justify()` + `RowVirtualizer`, pure, indipendenti dalla geometria
fin dalla loro scrittura nel Task 4) — non un secondo layout inventato
per l'occasione. Estratta in `components/FlatAssetGrid.vue` nel
momento in cui diventa il secondo consumatore reale (stesso principio
già seguito per `useDensity`/`useIsMobile` in questa sessione:
estrarre al secondo uso, non prima): Preferiti la usa sempre (mai
avuta una geometria propria, §9), Timeline la usa solo quando
`activeFilterCount(selezione) > 0`, altrimenti resta sul blob di
geometria invariato.

**Preferiti** (6/N): `favoriteAssets` (i preferiti, come prima) entra
in `useBrowseFilters`, `filteredAssets` ne esce e sostituisce
`visibleAssets` ovunque — griglia, "Seleziona tutto" (SP-4: "solo ciò
che ricade nel filtro"), secondo stato vuoto (stesso testo
`ui.filteredEmpty.*`, pre-condiviso da Task 7 3/N proprio per questo).
Il sottotitolo (§9.2, "il conteggio è calcolato **prima** dei filtri")
resta sul totale dei preferiti, non tocca `filteredAssets` — nessuna
modifica necessaria, era già così.

**Timeline** (7/N): `loadedAssets` (quanto già caricato, il limite
dichiarato di questa vista) entra in `useBrowseFilters`;
`displayedAssets` (= `filteredAssets` se un filtro è attivo,
altrimenti `loadedAssets` invariato) governa `FlatAssetGrid`, "Seleziona
tutto" e la navigazione prev/next del lightbox — quest'ultima cambiata
apposta: prima o dopo un filtro il visualizzatore deve restare dentro
a ciò che si vede, non risalire nell'intera libreria caricata.
`startCulling()` resta volontariamente su `loadedAssets` intero: il
culling non ha un concetto di "filtrato" nel documento.

**Effetto collaterale sistemato, non taciuto**: `gridEl` (il
contenitore scrollabile del blob di geometria) non è più garantito
montato per tutta la vita del componente — un filtro attivo lo smonta
a favore del `gridEl` interno e indipendente di `FlatAssetGrid`.
L'`onMounted` originale allegava lo scroll-listener/`ResizeObserver`
una tantum: sostituito con un `watch(gridEl, ..., {immediate:true,
flush:'post'})` che segue ogni comparsa/scomparsa del nodo, non solo
la prima — altrimenti riattivare il filtro e poi disattivarlo avrebbe
lasciato la vista senza scroll-tracking sul blob di geometria
"vecchio" nodo mai più aggiornato.

Verifica eseguita per intero:
- Nuovi test di cablaggio (non ripetizione della logica delle sei
  dimensioni, già coperta da `useBrowseFilters.spec.ts`/`design/
  quickFilter.spec.ts`/`QuickFilter.spec.ts`): 3 in
  `FavoritesView.spec.ts` (la griglia si restringe senza toccare il
  sottotitolo, il secondo stato vuoto condiviso, "Seleziona tutto"
  ristretto), 3 in `TimelineView.spec.ts` (passaggio da griglia a
  `FlatAssetGrid`, stato vuoto condiviso, "Seleziona tutto"
  ristretto). Un fix in corsa: `PhotoTile` non ha una prop `asset`
  (passata solo tramite `v-bind` di un oggetto più grande, mai
  dichiarata sul componente) — un primo tentativo di asserzione via
  `.props('asset').id` falliva con `undefined`; corretto leggendo
  `filename` (`"a.jpg"`), l'identificativo che il componente dichiara
  davvero.
- Un secondo fix, più largo: `useBrowseFilters` chiama `GET /tags`/
  `GET /persons` a **ogni** montaggio di queste due viste, non solo
  nei nuovi test — `apiFetch`, mockato a vuoto (`vi.fn()`) in entrambi
  gli spec e resettato da `resetAllMocks()` a ogni test, tornava
  `undefined` invece di una Promise e rompeva `.catch()` nel
  composable per **ogni** test già esistente di entrambi i file (30
  `Unhandled Rejection`, non fallimenti di asserzione — da lì la
  scoperta, non da un test rosso). Risolto con un `beforeEach` che
  installa un default `apiFetch → []` in entrambi gli spec (un solo
  test, il recupero di `?photo=` in `TimelineView.spec.ts`, aveva un
  proprio esito diverso per `/api/v1/assets/a`: reso sensibile all'URL
  invece di sovrascrivere in blocco, così tag/persone restano `[]`
  anche lì).
- `npx vitest run` (suite intera) → 75 file, 578/578 verdi.
- `npx vue-tsc -b` → pulito.
- `npx eslint` sui file nuovi/toccati e sull'intero repo → pulito
  (i due avvisi `:ariaLabel` già accettati da Task 7 1/N, nessun
  avviso nuovo; un solo errore preesistente su `PlayerView.vue`
  confermato indipendente da questa unità con `git stash`).
- `npm run build` + calcolo manuale del bundle iniziale gzip → 123.139
  byte, sotto il budget di 153.600 — `FlatAssetGrid.vue` finisce in un
  chunk condiviso fra le due viste ma resta comunque dietro
  `import()` pigro (nessuna delle due è nel bundle d'ingresso),
  confermato leggendo `dist/index.html` prima di calcolare, non
  presunto.

## Task 7 (8/N) — Modifica in blocco (§13), chiude il Task 7

**Ambito**: la riscrittura completa di `BatchEditView.vue` sul documento
funzionale §13 ("Modifica in blocco") — otto sezioni, "Applica"/"Annulla",
stato vuoto. La vista precedente (selettore di posizione, copia
posizione, importazione GPX) non corrisponde a **nessuna** schermata
documentata in tutto il documento (ricerca testuale sull'intero file,
zero risultati) — sostituita per intero (PROSEGUI, "codice sostituito
si elimina non si commenta), non affiancata. `PlacePicker.vue`/
`copyLocation`/`importGpx` restano intatti: appartengono al dialog
"Imposta posizione" (§28, Lightbox — Task 8, non ancora costruito),
solo scollegati da questa pagina, non eliminati.

**Il pezzo senza primitivo, come da istruzione dell'utente — trovato,
non presunto, e costruito, non taciuto**: il dialog di scelta tag
(§13.3 campo 5) deve poter **sia aggiungere sia togliere** un tag in
blocco — verificato leggendo il prototipo reale
(`openTagPickerDialog`/`removeTagFromPhoto`, `docs/ui/
keeppix-mockup.html`: "attiva/disattiva un tag... per aggiungerlo o
toglierlo da tutti"), non presunto dal solo testo del documento
funzionale. `AssetTagRepo` (Fase 7/Task 7 4/N) aveva `assign` ma
**nessun** modo di togliere un tag assegnato a mano: `reject` esiste,
ma è la decisione **permanente** della coda di revisione IA (`state=
'proposed'→'rejected'`, in conflitto esplicito se già `'confirmed'`
— verificato leggendo `decide()` riga per riga) — semanticamente
sbagliata per "ho ripensato a un tag manuale", e userebbe un `Conflict`
per bloccare esattamente il caso che deve funzionare. Nessun
endpoint, nessun metodo di repository, verificato con una ricerca
esplicita su tutto `asset_tags.rs`/`tags.rs`, non "non l'ho ancora
visto". Costruito, stessa forma esatta di `assign`/`assign_batch`
(niente di nuovo inventato):
- `AssetTagRepo::unassign` (`keeppix-db/src/asset_tags.rs`) — una
  `DELETE` vera della riga, non uno stato `'rejected'`: così
  riassegnare più tardi lo stesso tag passa di nuovo da `assign`
  senza scontrarsi con la permanenza di `reject`. Idempotente.
- `POST /tags/{id}/assets/batch/remove` (`unassign_batch` in
  `routes/tags.rs`) — stesso `BulkOutcome::from_partition` di
  `assign_batch`, stesso ciclo sequenziale.
- `unassignTagBatch` in `api/tags.ts`.
- `TagPickerDialog.vue` (nuovo) — stesso interruttore di gruppo di
  `AlbumPickerDialog.vue` (Task 7 2/N), ma **senza** un fetch per tag:
  l'appartenenza si legge direttamente da `TimelineAsset.tags` (già
  dentro `AssetView` dal Task 7 4/N), non serve un endpoint di
  "appartenenza" come per gli album.

**Il resto — cablaggio puro, tutti primitivi già esistenti**:
- **Valutazione/Pick-Scarta/Preferiti**: nessun endpoint batch
  "parziale" esiste per questi tre insieme — verificato:
  `POST /flags/batch` è anch'esso un rimpiazzo completo
  (`AssetFlagsBody` con `#[serde(default)]` su `pick`/`favorite`:
  ometterli scriverebbe "nessuno"/falso su ogni foto, non "lasciali
  invariati"). Con selezioni che possono avere stati diversi asset per
  asset, non esiste un corpo condiviso valido per tutti: `applyFlags()`
  in `BatchEditView.vue` legge e riscrive **un asset alla volta**
  (`fetchFlags`/`setFlags`, gli stessi due endpoint già usati da
  `stores/favorites.ts`), scrivendo solo i campi toccati sopra il
  valore corrente di ciascuno.
- **Album**: `AlbumPickerDialog.vue`, riusato senza modifiche.
- **Titolo**: `applyMetadataBatch` (già esistente, patch sparsa —
  `Option<Option<T>>` sul backend), una chiamata sola per l'intera
  selezione, solo se non vuoto dopo la ripulitura degli spazi.
- **Sposta in cartella**: `moveAssetsBatch` (Task 7 4/N), una chiamata
  sola, solo se diversa da "Non modificare".
- **Rinomina con formula** (§62): `RenameFormulaDialog.vue` (nuovo,
  solo l'ambito "selezione" — gli altri quattro punti d'ingresso del
  documento restano debito dichiarato per Task 8/culling). Nessuna
  logica di token/slug duplicata in frontend: `keeppix-domain::
  rename::render_base` (Fase 9) è già la fonte di verità, verificata
  leggendo i suoi stessi test unitari prima di scrivere il dialog —
  l'anteprima chiama `previewRename` (già esistente) a ogni cambio di
  schema, calcola su **tutte** le foto dell'ambito ma mostra solo le
  prime 5, "Applica" **davvero disattivato** su collisione (non solo
  `pointer-events`, la doc stessa assegna questo miglioramento
  esplicitamente a Fase 11: "'Applica' davvero disabilitato... è
  comportamento di interfaccia, Fase 11").
- **Stelle di valutazione**: radiogroup/radio veri con `aria-checked`
  solo sul valore esatto e "ri-clic sulla stella attiva → torna a 0"
  (§13.3 campo 1) — scritto qui, non riusando `RatingStars.vue`
  (Task Culling): quel componente è `role="group"`/`aria-pressed`
  cumulativo, un pattern diverso, corretto per il culling ma non
  quello che il documento chiede esplicitamente qui.
- **Pick/Scarta e Preferiti**: `SegmentedControl.vue` (Task 2) — il
  suo stesso commento di intestazione cita già "nei filtri della
  modifica in blocco include sempre 'Non modificare'", scritto in
  anticipo proprio per questo momento.

Verifica eseguita per intero:
- **Rust**: `cargo check --workspace` → pulito. `cargo clippy
  --workspace --all-targets -- -D warnings` → un errore reale trovato
  e corretto (il test di `assign_batch` esteso con la sezione
  `unassign_batch` ha superato le 100 righe di clippy:
  `#[allow(clippy::too_many_lines)]` aggiunto, stesso motivo già
  presente altrove nello stesso file). `cargo check -p keeppix-db
  --test asset_tags` / `cargo check -p keeppix-api --test tags` /
  `--test openapi` → puliti. `cargo test -p keeppix-api --test
  openapi` → **6/8 verdi** dopo aver aggiornato i due elenchi
  letterali (`security_requirements_name_a_declared_scheme`,
  `operation_ids_are_explicit_and_unique` — nuova voce in ordine
  alfabetico) e `UPDATE_OPENAPI=1` una volta per rigenerare
  `docs/api/openapi.json` (aggiunta pura entro `/api/v1`, la stessa
  frase del test da spiegare nel commit). I 2 ancora rossi
  (`documented_operations_are_all_mounted`,
  `openapi_summaries_do_not_contain_errors_heading`) falliscono solo
  su `SocketNotFoundError` — Docker assente in questo sandbox, non
  codice, stesso limite dichiarato in ogni unità precedente di questo
  Task.
- Nuovi test scritti seguendo le convenzioni già in uso, **mai
  eseguiti** (nessun Postgres locale): 3 in `asset_tags.rs`
  (`unassign` cancella davvero, riassegnare dopo non incontra mai il
  `Conflict` permanente di `reject`, idempotenza/permessi), 1 sezione
  aggiunta al test HTTP end-to-end esistente in `tests/tags.rs`
  (`unassign_batch` dopo `assign_batch`, verifica diretta sul
  database che la riga sia sparita).
- **Frontend**: `npx vitest run` (suite intera) → 78 file, 603/603
  verdi (11 nuovi in `BatchEditView.spec.ts`, 5 in `TagPickerDialog.
  spec.ts`, 7 in `RenameFormulaDialog.spec.ts`). Tre bachi trovati e
  corretti **nei test**, non nel codice applicativo, durante la
  scrittura: (1) `PhotoTile` non dichiara una prop `asset` propria
  (solo `v-bind` di un oggetto più grande) — un'asserzione su
  `.props('asset').id` falliva con `undefined`, corretta leggendo
  `filename`; (2) mutare `input.value` via DOM diretto non passa da
  `v-model` — un test sull'inserimento di segnaposto a metà stringa
  falliva perché lo stato reattivo non si era aggiornato, corretto
  posizionando il cursore sul valore di default intatto invece di
  riscriverlo a mano; (3) indici fissi su `[role="radio"]` per
  distinguere Pick/Scarta da Preferiti (12 radio in tutto: 5 stelle +
  4 + 3) erano sbagliati aritmeticamente — sostituiti con una ricerca
  per testo dell'opzione, robusta all'ordine esatto delle sezioni.
  Anche: montare sempre i tre dialog (chiusi) dentro `BatchEditView`
  senza mockare `@/api/albums`/`@/api/tags`/`@/api/rename` nello
  spec produceva `Unhandled Rejection` quando un test successivo li
  apriva (il vero `apiFetch`, mockato a vuoto, risolve `null` invece
  di rifiutare — `.catch(() => [])` non lo intercetta) — risolto
  mockando i tre moduli con risposte innocue, coerente con la
  disciplina già stabilita in questa sessione per `apiFetch`.
- `npx vue-tsc -b` → un errore reale trovato e corretto (`tick()`
  dichiarato ma mai usato in `RenameFormulaDialog.spec.ts`, residuo
  di un tentativo con timer reali sostituito da `vi.useFakeTimers()`),
  poi pulito.
- `npx eslint` sui file nuovi/toccati e sull'intero repo → pulito,
  nessun avviso nuovo (stesso unico errore preesistente e indipendente
  su `PlayerView.vue` di prima).
- `npm run build` + calcolo manuale del bundle iniziale gzip →
  124.848 byte, sotto il budget di 153.600.

**Task 7 chiuso**: Foto/Timeline (composizione finale), Preferiti,
filtro rapido SP-3, selezione multipla, Modifica in blocco — tutte e
cinque le unità del documento funzionale per questo task. Prossimo:
Task 8 (Lightbox, pannello informazioni, menu ⋯), primo consumatore
reale del dialog "Imposta posizione" (§28) già pronto ma scollegato.

# Task 8 — Lightbox, pannello informazioni, menu ⋯ (§18-21)

Letti per intero §18 (struttura/barra superiore), §19 (pannello
informazioni), §20 (menu ⋯), §21 (differenze libreria/culling) prima
di scrivere una riga. `AssetViewer.vue` attuale è un segnaposto da
151 righe (apri/chiudi/frecce, un solo campo di posizione) — la
riscrittura vera è task grande quanto l'intero Task 7, quindi
scomposta nelle stesse unità granulari, cominciando dal terreno di
backend come già fatto per SP-3/Modifica in blocco.

## Task 8 (1/N) — Il terreno di backend per il pannello informazioni

**Ricerca preliminare** (agente dedicato, come per Task 7 4/N):
verificato sul codice reale quali primitivi esistono già prima di
presumere lacune. Risultato: la maggior parte dei campi del pannello
è già raggiungibile (EXIF core scritto da `insert_exif` fin dalla
Fase 5 ma mai letto per intero; `state`/`source` già in `asset_tags`
ma mai esposti; `album_assets` già la tabella giusta per la ricerca
inversa) — **nessuno di questi è un buco reale**, solo query mai
scritte sopra dati già lì. Quattro nuovi metodi di sola lettura/scrittura,
stessa forma di `camera_models_among`/`confirmed_among`/`assign`:

- **`AssetRepo::exif_for`** (nuovo, `keeppix-db/src/assets.rs`) —
  l'intera riga `asset_exif` di un asset (obiettivo, esposizione, ISO,
  focale), non solo `camera_model` come `camera_models_among` (bulk,
  per SP-3). Wired in `AssetView.full_exif` (`timeline.rs`), campo
  additivo popolato **solo** da `GET /assets/{id}` — mai da `/timeline`/
  `/search`, un giro di query in più per riga che nessuna griglia
  legge (`page()` continua a chiamare `enrich_views` invariata).
- **`AssetTagRepo::for_asset`** (nuovo) — tag di un asset, confermati
  e proposti insieme (mai rifiutati, §19.3: "deve restare permanente"),
  con `state`/`source` per le tre rese del chip del documento (piena
  umana / `.ai-applied` IA / tratteggiata in attesa). Nuova rotta
  `GET /assets/{id}/tags`.
- **`AssetTagRepo::remove_confirmed`** (nuovo) — la `×` sui chip
  confermati (§19.3): **non** una `DELETE` come `unassign` (quella
  serve l'aggiunta manuale di Modifica in blocco), ma una transizione
  permanente a `state='rejected'` — verificato che `decide()`
  (confirm/reject) blocca esplicitamente la transizione
  `confirmed→rejected` con un `Conflict` ("una decisione permanente
  non si inverte"), quindi serve un metodo dedicato che la permette
  **solo** da `'confirmed'`, mai da `'proposed'` (quello resta compito
  di `confirm`/`reject`, la coda di revisione). Nuova rotta
  `POST /tags/{id}/assets/{asset_id}/remove`.
- **`AlbumRepo::for_asset`** (nuovo) — la freccia opposta di
  `list_assets`: dato un asset, i suoi album (manuali e dinamici
  insieme — i dinamici sono già materializzati in `album_assets` da
  `refresh`, nessuna `rule` da rivalutare qui). **Controllo di
  visibilità aggiunto in corsa**: la prima stesura filtrava solo per
  proprietà/condivisione dell'*album*, non per visibilità
  dell'*asset* — un chiamante senza permesso sull'asset avrebbe
  comunque scoperto che esiste, se per caso stava in un album suo.
  Corretto con `assert_visible` sull'asset prima di tutto, stesso
  principio già seguito da `AssetTagRepo::for_asset`. Nuova rotta
  `GET /assets/{id}/albums`.

**Debito dichiarato, non backend groundwork di questa unità** (verificato
e scartato per motivi precisi, non "non ci ho pensato"):
- **Aggiungere un volto senza riquadro** (§19.3, "+ aggiungi" persone):
  `Face.bbox` non è opzionale nel dominio, nessuna riga può esistere
  senza — un vero cambiamento di modello, non un giro di query in
  più. Rimandato: il riconoscimento volti reale (Task A, YuNet+SFace)
  non è ancora costruito in questa sessione, la stessa unità futura è
  la sede naturale per rivedere il modello `Face` una volta sola.
  Confermare/correggere/rifiutare un volto **già rilevato** restano
  invece pienamente costruiti (`faces.rs`, verificato).
- **Rotazione reale** (§19.3, "Ruota" — il documento la assegna
  esplicitamente a Fase 11 come azione vera, non più un toast):
  `orientation` è già scrivibile (`MetadataPatchRequest`, patch
  singolo asset) ma **non consumato da nessuna parte** — non dalla
  pipeline dei derivati (`keeppix-media`), non da nessun generatore di
  thumb/preview/full. Renderla reale è lavoro di elaborazione immagini
  vero, non cablaggio: unità propria più avanti in questo stesso
  Task 8, non qui.
- **"Luoghi noti alla libreria"** per il dialog di posizione (§19.3):
  il documento descrive un elenco di "posizioni delle cartelle" — ma
  `Folder` non ha (e non deve avere) un concetto di posizione propria:
  è la stessa finzione mockup-cartella-uguale-luogo già scartata per
  SP-3 §11 nel Task 7. La sostituzione reale, migliore del mockup, è
  già pronta e non richiede nulla di nuovo: `GET /places/suggest`
  (GeoNames vero), non un elenco chiuso di cartelle — unità del
  dialog di posizione, non di questa.
- **"Explicit none" sulla posizione**: `location: Option<Option<...>>`
  già distingue "non toccare" da "azzera" **alla scrittura**, ma
  `effective()` non può distinguere "azzerato apposta" da "mai
  impostato" alla lettura (`COALESCE` non vede la differenza) —
  rilevante solo quando il dialog di posizione stesso viene costruito,
  non per il pannello di sola lettura di questa unità.

**Bug reale trovato e corretto lungo la strada, non nell'ambito
originale ma nella stessa area (album)**: `frontend/src/api/albums.ts`'s
`addAssets(albumId, assetIds)` postava un corpo `{asset_ids}` a
`POST /albums/{id}/assets` — quel percorso ha **solo** `GET`
(`list_assets`, l'elenco dei membri) montato; l'unico endpoint di
scrittura reale è `POST /albums/{id}/assets/{asset_id}`, un asset alla
volta (`AlbumRepo::add_asset`, verificato anche lato `keeppix-db`: mai
stato batch). Non si è visto finora perché `AlbumPickerDialog.spec.ts`
mocka l'intero modulo `@/api/albums` — nessun test in questa sessione
ha mai davvero colpito quella rotta. Corretto con un ciclo sequenziale
sullo stesso endpoint singolo, stesso principio di `stores/
favorites.ts`'s `setMany`.

Verifica eseguita per intero:
- `cargo check --workspace` → pulito.
- `cargo clippy --workspace --all-targets -- -D warnings` → due errori
  reali trovati e corretti: `list_for_asset` in `albums.rs` senza
  sezione `# Errors` nel doc comment (quel file usa commenti
  espliciti, non l'allow di file di `tags.rs`); `.expect(...)` in un
  test di `assets.rs` che non ha l'allow di file per
  `clippy::expect_used` (solo `unwrap_used` per funzione) — sostituito
  con `.unwrap()`.
- `cargo check -p keeppix-db --test asset_tags/assets/albums`,
  `cargo check -p keeppix-api --test tags/albums/timeline` → tutti
  puliti.
- `cargo test -p keeppix-api --test openapi` → **7/8 verdi** dopo tre
  aggiornamenti: i due elenchi letterali (`security_requirements_
  name_a_declared_scheme`, `operation_ids_are_explicit_and_unique` —
  cinque voci nuove in ordine alfabetico) **e**, per la prima volta in
  questa sessione, il conteggio letterale di operazioni
  (`documented_operations_are_all_mounted` e `openapi_summaries_do_
  not_contain_errors_heading` condividono lo stesso numero, 174→180
  — sei nuove operazioni contate, non tre: il numero precedente era
  già leggermente indietro rispetto al codice reale, verificato
  chiedendo al documento vivo invece di ricalcolare a mano). Un
  secondo giro di `UPDATE_OPENAPI=1` necessario dopo aver aggiunto una
  sezione `# Errors` al doc comment di `list_for_asset`: `utoipa`
  porta il commento rustdoc nella `description` OpenAPI anche senza un
  `summary` esplicito che lo sovrascriva del tutto, quindi cambiarlo
  ha invalidato lo snapshot una seconda volta — scoperto dal test
  stesso, non presunto. L'unico ancora rosso
  (`documented_operations_are_all_mounted`) fallisce solo su
  `SocketNotFoundError` — Docker assente qui, non codice.
- Nuovi test scritti seguendo le convenzioni già in uso, **mai
  eseguiti** (nessun Postgres locale in questo sandbox — tentativo di
  esecuzione con timeout esplicito, confermato che si blocca
  cercando un container, non un errore di compilazione): 2 in
  `assets.rs` (`exif_for` con riga piena, `None` senza riga), 6 in
  `asset_tags.rs` (`for_asset` confermati+proposti mai rifiutati con
  provenienza mista IA/umana, forbidden su asset estraneo;
  `remove_confirmed` transizione permanente, idempotenza, conflitto su
  proposta ancora in attesa, not-found su mai assegnato, forbidden), 4
  in `albums.rs` (`for_asset` elenco per nome, vuoto, filtrato per
  permesso condiviso, forbidden su asset invisibile), 1 end-to-end via
  HTTP in `keeppix-api/tests/tags.rs` (lista poi rimuove, verifica che
  sparisca), 1 in `keeppix-api/tests/albums.rs` (lista filtrata per
  appartenenza reale), 1 in `keeppix-api/tests/timeline.rs`
  (`full_exif` presente solo sul dettaglio singolo, mai sulla pagina
  timeline).
- Frontend: `npx vitest run` → 78 file, 603/603 verdi (invariati:
  questa unità è solo terreno di backend + wrapper `api/*.ts`, nessuna
  vista reale li consuma ancora — la prossima unità). `npx vue-tsc -b`
  → pulito. `npx eslint` sui file nuovi/toccati e sull'intero repo →
  pulito (stesso unico errore preesistente su `PlayerView.vue`).
  `npm run build` + calcolo manuale del bundle iniziale gzip →
  124.872 byte, sotto il budget di 153.600.

## Task 8 (2/N) — Il lightbox vero: barra superiore, palco, filmino, menu ⋯

`AssetViewer.vue` riscritto da zero (§18-20 del documento funzionale,
riletti riga per riga) sopra il terreno di backend della 1/N. Contratto
prop/emit ridisegnato: via `prev?`/`next?` + due emit separati, dentro
`neighbors?: TimelineAsset[]` (default `[]`) + un solo emit `step:
[asset]` — `currentIndex`/`prevAsset`/`nextAsset` si calcolano da
`neighbors.findIndex`, eliminando la funzione `viewingNeighbour(delta)`
duplicata in ogni vista chiamante, e il filmino (che ha bisogno
dell'intero elenco vicini, non solo dell'adiacente) arriva gratis dallo
stesso prop. Aggiunta anche `isFavorite: boolean` (prop) + `toggle-
favorite: []` (emit) — stesso schema già stabilito da `PhotoTile.vue`,
il genitore possiede la chiamata `favorites.toggleOne(asset)`.

Contratto propagato ai 4 consumatori reali: `TimelineView.vue`,
`FavoritesView.vue`, `SearchView.vue` (nessun `open-asset`, come già
prima), `MapView.vue` (deliberatamente **senza** `:neighbors`: nel
popover mappa non c'erano frecce né filmino prima, e non ce ne devono
essere ora — un solo asset alla volta).

Barra superiore (§18.3): chiudi, cuoricino (riflette `isFavorite`,
`toggle-favorite` su clic e sul tasto `f`/`F`), info (`i`/`I`), "⋯"
(riuso di `Popover.vue` — il suo stesso commento di intestazione
anticipava questo esatto consumatore). Palco (§18.2): frecce circolari
condizionali (omesse al primo/ultimo vicino), preload nascosto di
`prev`/`next`. Filmino (§18.2, in fondo): una tessera 52×52 per
vicino, quella corrente con anello d'accento, clic = `step`. Due
livelli di Esc (§18.5) gestiti esplicitamente nel listener `onKey` del
lightbox stesso (controllo su `moreOpen` prima di chiudere il
lightbox) — non affidato allo stacking automatico di reka-ui, perché
la radice del lightbox è un `<div role="dialog">` semplice, non un
vero `DialogRoot`; solo il menu "⋯" annidato è un vero popover reka-ui.

Menu "⋯" (§20, cinque voci):
- **Scarica originale**: prima un toast-finto, ora un vero `<a
  :href="originalSrc(id)" :download="filename">` verso `GET /media/
  original/{id}` — quella rotta esisteva già lato backend (stream
  reale con supporto Range) ma non era mai stata consumata da nessun
  punto del frontend finora. `download` forza il salvataggio per un
  link same-origin anche senza `Content-Disposition: attachment`.
- **Ruota**: resta un toast dimostrativo — debito dichiarato, non
  nascosto: `orientation` è scrivibile via `MetadataPatchRequest` ma
  mai letto dalla pipeline di derivati di `keeppix-media`; è lavoro
  vero di elaborazione immagini, sede naturale una futura sotto-unità
  dedicata, non questa (che è cablaggio).
- **Aggiungi ad album**: riuso diretto di `AlbumPickerDialog.vue`.
- **Rinomina…**: riuso di `RenameFormulaDialog.vue`, esteso con un
  prop opzionale `subtitle?: string` che sovrascrive il sottotitolo
  calcolato di default — il doc (§62.8) vuole «1 foto — {nome file}»
  per il punto d'ingresso "singola foto dal lightbox», distinto dalla
  dicitura «N foto selezionate/a» già corretta per il punto d'ingresso
  "selezione".
- **Elimina…**: riuso di `DeleteDialog.vue` a tre vie, stessa mappa
  `DeleteChoice → DiskAction` già in `LibrarySelectionActions.vue`
  (`index→'kept'`, `trash→'moved_to_trash'`, `disk→'purged'`).

Non riprodotto in questa unità (debito dichiarato, non codice morto
nascosto): **Condividi** (nessun endpoint di condivisione singolo-
asset scoperto nella ricerca — stessa area di Task 9/10 non ancora
raggiunta); i **riquadri volto** in overlay (visibili solo su hover di
un chip persona nel pannello informazioni, che non esiste ancora —
sede naturale la riscrittura completa del pannello §19); il **pannello
informazioni completo** (titolo modificabile, valutazione a stelle,
posizione, persone, tag, album — resta il pannello minimo con mini-
mappa già presente prima di questa unità); l'integrazione reale in
`CullingView.vue` (che non monta `<AssetViewer>` per niente, confermato
via grep — solo citato in un commento).

Rimosso `@click.self` sullo sfondo del lightbox (chiudeva cliccando
fuori dall'immagine): il documento (§18.4) non lo prevede, e la stessa
riscrittura del test lo verifica esplicitamente (clic sullo sfondo non
emette `close`).

Verifica eseguita per intero, con tre bug reali trovati e corretti
**nel test**, non nel componente (`AssetViewer.spec.ts` riscritto da
zero seguendo il contratto nuovo):
- Il file di test non impostava `i18n.global.locale.value = 'it'` nel
  `beforeEach` (convenzione già stabilita in `LibrarySelectionActions.
  spec.ts`): senza, `detectLocale()` risolve a `'en'` in jsdom
  (`navigator.language` è `'en-US'` di default) e ogni selettore per
  etichetta italiana falliva silenziosamente.
- Il test del filmino cercava `img[alt="a.jpg|b.jpg|c.jpg"]` su tutto
  il wrapper: l'immagine del palco principale condivide lo stesso
  `alt` dell'asset corrente, quindi il conteggio risultava sempre uno
  in più. Corretto scoping la ricerca al contenitore del filmino
  (`.overflow-x-auto`).
- I tre test del menu "⋯" montano con `attachTo: document.body` (serve
  al popover teletrasportato) ma non smontavano il wrapper fra un test
  e l'altro: il DOM del test precedente restava attaccato al `body`, e
  `menuItemWithText` poteva trovare — e cliccare — il bottone del
  montaggio *sbagliato* (con un `toast`/`pinia` diversi da quello
  osservato dal test). Corretto con `afterEach(() => wrapper?.
  unmount())` sullo stesso pattern di `LibrarySelectionActions.spec.ts`.
- Il test del toast "Ruota" cercava il testo nel `document.body.
  textContent`, ma non esiste nessun host di toast montato in un test
  che monta solo `<AssetViewer>` isolato: corretto asserendo su
  `useToastStore().toasts.at(-1)?.message`, stesso pattern già in uso
  in `LibrarySelectionActions.spec.ts`.

`npx vue-tsc -b` → pulito. `npx vitest run` → 78 file, 612/612 verdi
(78° file di questa unità: `AssetViewer.spec.ts`, ora 12 test contro i
9 precedenti). `npx eslint` sui file nuovi/toccati e sull'intero repo
→ pulito (stesso unico errore preesistente su `PlayerView.vue`, non
toccato in questa sessione — confermato via `git blame`, commit
`6fab915` precedente a questa sessione). `npm run build` + calcolo
manuale del bundle iniziale gzip (stesso algoritmo di `.github/
workflows/ci.yml`) → 124.776 byte, sotto il budget di 153.600.

## Task 8 (3/N) — Pannello informazioni: titolo, valutazione, sezione SCATTO

Prima metà reale di §19 (le altre restano POSIZIONE/PERSONE/TAG/ALBUM/
AZIONI, prossime unità). Prima di scrivere codice, un agente di ricerca ha
verificato lo stato reale di tutte le primitive necessarie (endpoint
metadati, componente stelle, dati RAW/JPEG, place picker, volti, tag,
album) — due gap reali trovati e corretti prima di essere usati, non
dopo:

- `frontend/src/api/metadata.ts`: `AssetMetadata`/`fetchMetadata` avevano
  un tipo (`{exif, overrides}`) che **non corrisponde a nessuna risposta
  reale del backend** ed erano `grep`-confermati mai chiamati da nessun
  punto del frontend — codice morto con un tipo sbagliato, non solo
  inutilizzato. Sostituito con la forma vera di `EffectiveMetadataView`
  (`crates/keeppix-api/src/routes/metadata.rs:48-55`: title, description,
  taken_at, location, place_id, orientation) e aggiunta `patchMetadata`
  per `PATCH /assets/{id}/metadata`, stessa semantica "doppio opzionale"
  del backend (campo assente = non toccare, `null` = azzera).
- `TimelineAsset` (`frontend/src/api/timeline.ts`) non portava
  `location`/`place_id`/`stack_size`, già presenti lato backend
  (`AssetView`, `crates/keeppix-api/src/routes/timeline.rs:48-101`) ma
  mai arrivati al tipo frontend. Aggiunti come campi opzionali (stesso
  motivo di `full_exif`: presenti solo su `GET /assets/{id}`, mai su
  `/timeline`/`/search`) insieme a un nuovo `fetchAsset(id)`.

Il prop `asset` che arriva ad `AssetViewer` dalle griglie (`/timeline`,
`/search`) non porta mai `full_exif` — solo `GET /assets/{id}` lo calcola.
`loadPanelData()` (sostituisce il precedente `loadMetadata()`) ora fa tre
chiamate in parallelo all'apertura del pannello — `fetchMetadata` (titolo,
posizione), `fetchAsset` (`full_exif`, per la sezione SCATTO),
`fetchFlags` (valutazione) — con `Promise.allSettled`: l'esito di ciascuna
è indipendente, così l'assenza di pgvector (che fa fallire `fetchFlags`
altrove nel codice) non porta via anche titolo e posizione. Stessa guardia
di sequenza già in uso per la mini-mappa, estesa a tutt'e tre.

**Titolo** (`#lbTitleInput`, §19.3): salvataggio `@change`, non `@input`
— la digitazione aggiorna solo il modello locale (`v-model`), il giro di
rete parte solo alla perdita del fuoco o Invio. Valore vuoto (dopo
`trim()`) inviato come `title: null` (azzera l'override), non stringa
vuota — il campo torna al placeholder "Senza titolo" senza alcun
ripiego sul nome del file, come da §19.3.

**Stelle** (§19.3): riuso diretto di `RatingStars.vue` (già esistente,
usato da `CullingView.vue`) — il componente emette solo `rate(n)`, il
toggle "riclick sulla stessa stella azzera a 0" è responsabilità del
chiamante e **non** era implementato nemmeno in `CullingView.vue`
(`stores/culling.ts#rate` fa un `set` diretto, mai un toggle): qui sì,
per rispettare §19.3 alla lettera — prima volta che questo comportamento
esiste nel codice. `setFlags` sostituisce l'intero oggetto voti (PUT, non
PATCH): si parte sempre da `flags.value` già caricato, mai da un valore
vuoto, stessa trappola già risolta una volta in `stores/favorites.ts` per
`favorite`.

**Sezione SCATTO** (§19.2 righe 6-9): Fotocamera (`camera_make` +
`camera_model` uniti), Obiettivo, Esposizione (`f/{f_number} ·
{exposure}s · ISO {iso}`, solo le parti effettivamente presenti — un file
senza diaframma noto non mostra "f/undefined"), Dimensioni
(`{width}×{height}`, dall'asset stesso, non dall'exif: la sezione resta
visibile anche senza alcun exif se le dimensioni pixel esistono, verificato
esplicitamente con un test dedicato).

**Debito dichiarato** (aggiunto al commento di testa del file, non
taciuto): il link cartella/lotto nella riga data/ora (nessuna rotta
`GET /folders/{id}` per risolvere un nome da un `folder_id`); il
commutatore RAW/JPEG (serve `GET /assets/{id}/stack`, esiste lato
backend, nessun wrapper frontend ancora — prossima unità).

Verifica completa: `npx vue-tsc -b` pulito. `npx vitest run` → 78 file,
617/617 verdi (17 test in `AssetViewer.spec.ts`, 5 nuovi: titolo
caricato/azzerato-a-null, salvataggio solo su `change` mai su `input`,
toggle delle stelle, sezione SCATTO piena, sezione SCATTO senza exif ma
con dimensioni). `npx eslint` sui file toccati e sull'intero repo →
pulito (stesso unico errore preesistente su `PlayerView.vue`). `npm run
build` + calcolo manuale del bundle iniziale gzip → 125.051 byte, sotto
il budget di 153.600.

## Task 8 (4/N) — Pannello informazioni: sezione POSIZIONE

Chiude §19.2 righe 10-12. `PlacePicker.vue` esisteva già (ricerca reale
GeoNames via `maps.suggestPlaces`/`GET /places/suggest`, applicazione via
`maps.applyPlace`) ma era **orfano**: nessuna vista lo montava — confermato
via grep prima di scrivere codice, e già annotato in un commento di
`BatchEditView.vue` come earmarked proprio per questo dialog. Costruito
`PlacePickerDialog.vue`, un involucro SP-5 (`Dialog.vue`, stesso pattern
di `AlbumPickerDialog`/`TagPickerDialog`) attorno a `PlacePicker` con
un'unica aggiunta: il pulsante **"Nessuna posizione"**, che `PlacePicker.
apply()` non può esprimere (richiede sempre un luogo scelto) — chiama
`patchMetadata(id, {location: null, place_id: null})` direttamente,
sfruttando la stessa semantica "doppio opzionale" già disponibile lato
backend dal Task 8 (3/N). Il sottotitolo del mockup ("Nessuna mappa reale
in questo mockup — scegli tra i luoghi già noti alla libreria") non è
riprodotto: sostituito da un testo che descrive la ricerca reale, che è
strettamente migliore del finto elenco statico del prototipo — stessa
decisione già presa per lo stesso identico dialog nel Task 7.

Nel pannello: stato vuoto (`"Nessuna posizione impostata."`, corsivo) o
luogo+coordinate a 4 decimali+mini-mappa quando una posizione esiste;
pulsante che alterna etichetta **"Imposta posizione…"**/**"Modifica
posizione…"** secondo §19.2 riga 12. Aprire il dialog e applicare/
cancellare una posizione ricarica il pannello (`@applied="loadPanelData"`)
così luogo/coordinate/mini-mappa riflettono subito il nuovo stato, senza
richiudere e riaprire il lightbox.

**Bug scoperto e corretto durante la verifica, non prima**: `maps.regions`
non veniva mai caricato da nessun ingresso globale (solo `MapView.vue` e
`MapsOfflineView.vue` lo fanno) — la mini-mappa del pannello informazioni
usava già `maps.availableRegionIds` fin dal Task 8 (1/N)/(2/N) senza che
nulla la popolasse quando si apre il lightbox da Timeline/Preferiti/Cerca
(mai da Mappa). Risultato pratico: l'avviso "mappa non disponibile" del
nuovo `PlacePickerDialog` sarebbe scattato sempre, anche per regioni già
scaricate. Corretto aggiungendo `void maps.loadRegions()` (fire-and-forget,
non blocca gli altri tre campi del pannello) dentro `loadPanelData()` —
beneficia sia il dialog nuovo sia la mini-mappa preesistente.

Verifica completa: `npx vue-tsc -b` pulito (la CI di questa sandbox
`npx`/`_npx` ha risolto una copia mismatched di `typescript`/`vue-tsc` da
cache in un tentativo — bypassato con `./node_modules/.bin/vue-tsc -b`
diretto, stesso identico controllo). `npx vitest run` → 78 file, 621/621
verdi (20 test in `AssetViewer.spec.ts`, 3 nuovi: stato vuoto col pulsante
giusto, stato pieno con coordinate a 4 decimali e pulsante giusto, apertura
del dialog e cancellazione reale della posizione via `patchMetadata`) — un
bug reale nei mock di due test **preesistenti** scoperto e corretto nello
stesso giro: `maps.loadRegions()` (nuovo in questa unità) colpiva lo stesso
`apiFetch` mockato a un valore fisso non-array per tutte le chiamate,
mandando in eccezione `availableRegionIds` (`regions.value.filter` su un
oggetto) — corretto instradando `/map/regions` a `[]` in entrambi i mock.
`npx eslint` sui file toccati e sull'intero repo → pulito (stesso unico
errore preesistente su `PlayerView.vue`; un errore reale nuovo trovato e
corretto in `PlacePickerDialog.vue`: parametro `_place` mai usato — il
prefisso `_` non è tollerato da questa configurazione ESLint, tolto il
parametro anziché rinominarlo). `npm run build` + calcolo manuale del
bundle iniziale gzip → 125.236 byte, sotto il budget di 153.600.

## Task 8 (5/N) — Pannello informazioni: sezione PERSONE, riquadri volto

Chiude §18.2 (riquadri volto) e §19.2-19.3 righe 13. Prima di scrivere
codice, verificato lo stato reale delle rotte volti/persone
(`crates/keeppix-api/src/routes/faces.rs`, `.../persons.rs`): tutte già
pronte dalla Fase 8 (`GET /assets/{id}/faces`, `POST /faces/{id}/assign`,
`POST /faces/{id}/reject`, `POST /persons`) ma **mai** chiamate da questo
frontend — nessun `api/faces.ts`, nessun selettore di persona, confermato
via grep prima di scrivere. Aggiunti `frontend/src/api/faces.ts`
(`fetchFacesForAsset`, `assignFace`, `rejectFace`) e `createPerson` in
`api/persons.ts` (il commento del backend su `faces::assign` lo dice
esplicito: "il client crea prima la persona, poi assegna il volto a
quella").

**Sezione PERSONE**: un chip per volto confermato, da `asset.faces`
(`AssetFaceBadge[]`, già nel prop — nessun giro di rete per i nomi). Nome
mostrato o, se `person_name` è `null`, un'etichetta generica ("Persona
senza nome") — **non** riprodotto lo schema "Persona {n}" del mockup: è
un numero progressivo che il backend non espone da nessuna parte
(`PersonView` non porta nessun contatore stabile), inventarne uno lato
client sarebbe un dato finto, non un dato mancante da recuperare. Click
sul chip apre un popover (`Popover.vue` — il suo stesso commento di
intestazione elenca già "menu sul riquadro del volto" fra i sei
consumatori previsti) con due voci reali:
- **Correggi persona…**: apre `PersonPickerDialog.vue` (nuovo — elenco
  persone filtrato lato client, `GET /persons` non ha un parametro di
  ricerca, più "crea persona "{nome}"" quando il nome digitato non
  corrisponde a nessuna già in elenco), poi `assignFace` sul volto
  risolto dal chip, toast **"Persona corretta."**.
- **Non è un volto**: `rejectFace`, toast esatto dal documento
  (**`Segnato come "non è un volto" — non verrà più riproposto.`**).

**Omesso, dichiarato non taciuto**: la terza voce del mockup, "Vai alla
persona" — a differenza di "Ruota"/"Scarica" (demo-toast già nel mockup
stesso), qui il mockup naviga davvero verso una schermata Persone che nel
nostro frontend reale non esiste ancora (Task 16, Tranche D): un toast
finto su un bottone senza destinazione sarebbe meno onesto di ometterlo,
stessa logica già usata per "Condividi" nel Task 8 (2/N). Il chip "+
aggiungi" (ultimo della riga, per creare un volto manuale senza
rilevamento) resta debito verso il Task A (Volti: YuNet+SFace): un volto
manuale nasce con `box:null` nel mockup, ma `Face.bbox` nel dominio reale
(`crates/keeppix-domain/src/face.rs`) non è opzionale — un buco di
modello, non di frontend, già segnalato dal Task 8 (1/N) come da
rivedere in quella sede.

**Riquadri volto sull'immagine** (§18.2): visibili solo durante l'hover/
focus del chip corrispondente (0ms all'entrata, 200ms di tolleranza
all'uscita, annullati rientrando nel chip **o** nel riquadro stesso — da
qui gli stessi handler su entrambi). Posizionati in percentuale rispetto
all'immagine **effettivamente disegnata**, non al contenitore: con
`object-contain` le due cose divergono ogni volta che il rapporto
d'aspetto della foto non è quello del contenitore (lettera-/pillar-
boxing) — un dettaglio facile da sbagliare (percentuali dirette sul
contenitore avrebbero prodotto riquadri visibilmente disallineati su
qualunque foto non esattamente quadrata). Misurato con `naturalWidth`/
`naturalHeight` (dopo il `load` dell'`<img>`, che si ripete a ogni cambio
foto) e `ResizeObserver` sulla dimensione dell'elemento.

Verifica completa: `npx vue-tsc -b` (`./node_modules/.bin/vue-tsc -b`,
stesso bypass della 4/N) pulito. `npx vitest run` → 78 file, 626/626
verdi (24 test in `AssetViewer.spec.ts`, 4 nuovi: chip con nome/etichetta
generica, hover-mostra/200ms-nasconde-il-riquadro con timer finti
[`vi.useFakeTimers`], "Non è un volto" con toast esatto, "Correggi
persona…" end-to-end fino al riassegnamento — un bug reale nel test
stesso scoperto e corretto durante la verifica: il popover del chip è
teletrasportato nel `body` come quello del menu ⋯, `wrapper.get(...)`
sul solo albero del componente non lo vedeva, serviva `attachTo:
document.body` + ricerca su `document.body`, stesso correttivo già
maturato nella verifica del Task 8 (2/N)). `npx eslint` sui file toccati
e sull'intero repo → pulito (stesso unico errore preesistente su
`PlayerView.vue`). `npm run build` + calcolo manuale del bundle iniziale
gzip → 125.573 byte, sotto il budget di 153.600.

## Task 8 (6/N) — Pannello informazioni: sezione TAG

Chiude §19.2 righe 14-17. Aggiunte `confirmTagProposal`/
`rejectTagProposal` in `api/tags.ts` (`POST /tags/{id}/assets/{asset_id}/
confirm|reject` — rotte pronte dalla Fase 7, mai chiamate dal frontend).

**Deviazione deliberata dal mockup, verificata contro il vero state
machine del backend prima di scrivere codice, non assunta**: il documento
descrive tre aspetti di chip per un tag confermato — "applicato dall'IA,
mai revisionato" (marcatore "IA", cliccabile per confermare) contro
"confermato da un umano" (pieno, non cliccabile). Letto `AssetTagRepo::
decide` (`crates/keeppix-db/src/asset_tags.rs:336-382`): transita **solo**
righe `state='proposed'`; una riga `state='confirmed'` è per costruzione
già stata decisa (via `confirm()`, che richiede un utente autenticato, o
un'assegnazione manuale) — non importa se il suo `source` originario era
`'ai'` (il campo non viene mai toccato da `decide()`, resta quello messo
da `propose_for_tag` per sempre). Riprodurre "IA, clicca per confermare"
su un tag già confermato sarebbe un bottone che promette un'azione senza
alcun effetto reale (`decide()` è idempotente). Ogni tag confermato ha
quindi **un solo aspetto** nel pannello, indipendente da `source`; la
distinzione a tre vie del mockup collassa correttamente nelle due sezioni
reali del backend — confermato (fatto) e proposto (da decidere) —
documentato nel commento di testa del file, non solo taciuto in un
commit.

**Sezione TAG**: chip confermati raggruppati per categoria
(`category_id` → nome, `GET /tags` filtrato per `kind==='category'`),
ordine alfabetico con "Senza categoria" sempre in fondo — nessun
`TAG_CATEGORIES` lato backend (era una costante del solo prototipo,
verificato: `ORDER BY t.kind ASC, t.name ASC`, nessun ordine custom).
Ogni chip: pallino colorato (`tag.color`) + nome + `×` di rimozione
permanente (`removeConfirmedTag`, già costruito nel Task 8 1/N, toast
**"Tag rimosso."**). Chip **"+ aggiungi"**: riuso diretto di
`TagPickerDialog.vue` (già esistente, usato finora solo da
`BatchEditView.vue`) passando `assets: [asset]` — applica ogni tocco
subito senza un evento di completamento (stesso comportamento di
`AlbumPickerDialog`), quindi il pannello si ricarica alla **chiusura**
del dialog (`watch(tagDialogOpen, ...)`), non ad ogni singolo tocco.
Sezione separata **"In attesa di conferma"** (solo se ci sono proposte):
chip tratteggiati con `✓` (`confirmTagProposal`, toast **"Tag
confermato."**) e `×` (`rejectTagProposal`, toast **"Suggerimento
rifiutato — non verrà riproposto."**, SP-10).

**Bug reale di isolamento test scoperto e corretto, non solo in questa
unità**: i nuovi test della sezione TAG fallivano con `faces.value.filter
is not a function` — non per un difetto nel codice di produzione, ma
perché **nessun `describe` precedente in questo file smontava mai il
proprio wrapper**. Il listener `keydown` globale di `AssetViewer` (apre/
chiude il pannello con `i`) resta registrato su `window` finché il
componente non è smontato: un wrapper della sezione PERSONE (5/N, `asset.
faces` non vuoto) mai smontato continuava a rispondere ai
`dispatchEvent` dei test *successivi* della sezione TAG, richiamando la
propria `loadPanelData()` — che per quel vecchio componente chiedeva
davvero `fetchFacesForAsset` — contro l'`apiFetch` ormai riconfigurato dal
test TAG corrente, il cui ramo di fallback restituiva un oggetto singolo,
non un array. Corretto non solo per la sezione TAG ma **sistematicamente
per tutto il file**: ogni `describe` ora dichiara un `wrapper` condiviso
con `afterEach(() => wrapper?.unmount())` (stesso pattern già in uso nel
blocco "menu ⋯" fin dal Task 8 2/N) — i blocchi precedenti non fallivano
ancora per puro caso (nessuno dei loro asset aveva `faces` non vuoti), ma
erano comunque silenziosamente order-dependent; con altre tre sezioni
ancora da costruire su questo stesso file (ALBUM, AZIONI) il rischio di
rincontrare esattamente questo bug era concreto, non ipotetico.

Verifica completa: `npx vue-tsc -b` pulito. `npx vitest run` → 78 file,
630/630 verdi (28 test in `AssetViewer.spec.ts`, 4 nuovi: raggruppamento
per categoria con "Senza categoria" in fondo, rimozione permanente con
toast esatto, sezione proposte con conferma/rifiuto, apertura di
`TagPickerDialog`). `npx eslint` sui file toccati e sull'intero repo →
pulito (stesso unico errore preesistente su `PlayerView.vue`). `npm run
build` + calcolo manuale del bundle iniziale gzip → 125.830 byte, sotto
il budget di 153.600.

## Task 8 (7/N) — Pannello informazioni: sezioni ALBUM e AZIONI, chiude §19

Ultime due sezioni di §19. **ALBUM** (riga 18): elenco di sola lettura via
`AlbumRepo::for_asset`/`fetchAlbumsForAsset` (costruito nel Task 8 1/N,
mai consumato dal frontend finora), chip non cliccabili + "+ aggiungi"
che riusa lo stesso `AlbumPickerDialog`/`albumDialogOpen` già cablato dal
menu ⋯ nel Task 8 2/N — un solo dialog, due punti d'ingresso, coerente
con "L'effetto è immediato" (§12.3). **AZIONI** (§19.3): le stesse cinque
voci del menu ⋯ (Scarica originale, Ruota, Aggiungi ad album, Rinomina…,
Elimina…), qui come pulsanti visibili invece che dentro un popover — il
documento le dichiara esplicitamente identiche ("le stesse della sezione
AZIONI del pannello informazioni"), stessi handler riusati verbatim, zero
duplicazione di logica.

**Bug reale trovato e corretto, presente fin dal Task 8 (2/N), non solo
in questa unità**: scrivendo il test "Esc chiude il dialog di apertura
album, non il lightbox sotto", `wrapper.emitted('close')` risultava
valorizzato — Esc chiudeva **anche il lightbox**, non solo il dialog
aperto sopra di esso. Causa: la gestione di Esc di reka-ui (`Dialog.vue`,
usato da tutti e sei i dialog del pannello — elimina, album, rinomina,
posizione, persona, tag) gira su `DismissableLayer`, un meccanismo
interno alla libreria che non coordina in alcun modo con
l'`window.addEventListener('keydown', onKey)` scritto a mano in
`AssetViewer.vue`: `onKey` conosceva solo `moreOpen` (il popover del menu
⋯, l'unico caso gestito dal Task 8 2/N in poi) e ricadeva sempre
sull'`emit('close')` del lightbox per qualunque altro Esc — inclusi tutti
i sei dialog aggiunti nelle unità successive (2/N, 4/N, 5/N, 6/N), mai
esercitati da un test con Esc prima d'ora. Corretto con un array
`dialogRefs` controllato prima della ricaduta su `emit('close')`. Non un
debito rimandato: la causa era chiara, la correzione locale e a basso
rischio, coerente con la disciplina di questa sessione di correggere un
bug reale scoperto durante la verifica invece di limitarsi a
documentarlo.

Incidentale: il mock di `@/api/albums` nello spec (`fetchAlbums`/
`fetchAlbum` soli, dal Task 8 2/N) non copriva `fetchAlbumsForAsset`
(nuovo consumo di questa unità) né `addAssets`/`removeAsset` (già
richiesti da `AlbumPickerDialog` ma mai realmente esercitati da nessun
test finora): esteso a tutti e cinque via `vi.hoisted`, con default
azzerabili per test.

Verifica completa: `npx vue-tsc -b` (`./node_modules/.bin/vue-tsc -b`)
pulito. `npx vitest run` → 78 file, 633/633 verdi (31 test in
`AssetViewer.spec.ts`, 3 nuovi: elenco album di sola lettura + apertura
di `AlbumPickerDialog`, Esc-chiude-solo-il-dialog-non-il-lightbox [il
test che ha scoperto il bug], le cinque azioni come pulsanti visibili).
`npx eslint` sui file toccati e sull'intero repo → pulito (stesso unico
errore preesistente su `PlayerView.vue`). `npm run build` + calcolo
manuale del bundle iniziale gzip → 125.841 byte, sotto il budget di
153.600.

**Con questa unità §19 (Pannello informazioni) è costruito per intero**,
salvo il debito dichiarato e verificato lungo le sette unità: link
cartella/lotto nella riga data/ora, commutatore RAW/JPEG, "Vai alla
persona", chip "+ aggiungi" delle persone (Task A). Il Task 8 prosegue
con §21 (integrazione culling, `CullingView.vue` non monta ancora
`<AssetViewer>`) come ultima unità dichiarata, poi il Task 8 è chiuso.

## Task 8 (8/N) — Commutatore RAW/JPEG: chiude l'ultimo debito reale di §19

Nuovo `frontend/src/api/stacks.ts` (`fetchStack`, primo consumo di
`GET /assets/{id}/stack`, Fase 10 — mai chiamata dal frontend finora).

**Deviazione deliberata e migliorativa rispetto al mockup, dichiarata fin
dal Task 8 (1/N)**: il documento descrive il commutatore come puramente
cosmetico — "l'unico effetto osservabile è quale delle due chip è
evidenziata... non cambia l'immagine mostrata... non cambia il
comportamento di 'Scarica originale'" — e lo indica esplicitamente come
uno dei punti in cui "il backend dovrà fare qualcosa di vero: scegliere
quale dei due file della pila viene decodificato, mostrato e scaricato".
Qui la selezione **cambia davvero** cosa mostra lo stage e cosa scarica
"Scarica originale" (sia nel pannello AZIONI sia nel menu ⋯, stesso
`downloadTarget` condiviso). Nessun lavoro nuovo lato `keeppix-media`
serviva per la decodifica: `/media/preview/{hash}` genera già
un'anteprima per i file RAW (le miniature RAW funzionano ovunque
nell'app da prima di questa unità) — mancava solo instradare la scelta
dell'utente al `content_hash`/`id` del membro giusto dello stack, che è
esattamente ciò che questa unità collega.

Tre stati, dalla lettura di `StackMemberView`
(`crates/keeppix-api/src/routes/stacks.rs:16-20`, ogni membro è un
`AssetView` completo via `AssetView::from_asset` — **non**
`from_asset_with_stack` — quindi porta il proprio `raw_kind` per-file,
non quello aggregato dello stack, permettendo di distinguere il membro
RAW da quello JPEG): `raw_kind==='raw+jpeg'` → due chip cliccabili (RAW/
JPEG, dimensioni reali da `size_bytes` via `Intl.NumberFormat` — virgola
italiana verificata dal test, "4,2 MB"); `raw_kind==='raw'` senza membri
nello stack → una chip sola, sempre attiva, non cliccabile, con
"nessun JPEG associato"; `raw_kind` null/`'jpeg'` → nessun blocco, nessuna
richiesta a `/stack` (verificato esplicitamente con un test dedicato —
niente giro a vuoto per l'immensa maggioranza delle foto che non sono
RAW).

Verifica completa: `npx vue-tsc -b` pulito. `npx vitest run` → 78 file,
636/636 verdi (34 test in `AssetViewer.spec.ts`, 3 nuovi: due chip con
switch reale di stage+download, chip singola non cliccabile per RAW
senza JPEG, nessun blocco/nessuna richiesta per un JPEG semplice). `npx
eslint` sui file toccati e sull'intero repo → pulito (stesso unico
errore preesistente su `PlayerView.vue`). `npm run build` + calcolo
manuale del bundle iniziale gzip → 125.977 byte, sotto il budget di
153.600.

**Con questa unità si chiude l'ultimo debito reale (non di destinazione
mancante) dichiarato per §19.** Restano dichiarati e non costruiti:
link cartella/lotto (nessuna rotta di risoluzione nome), "Vai alla
persona" e "+ aggiungi" delle persone (destinazioni che non esistono
ancora altrove nell'app — Task 16 e Task A). Il Task 8 prosegue con §21
(integrazione culling) come ultima unità dichiarata.

## Task 8 (9/N) — Il pannello informazioni parte aperto (§19.8)

Bug reale trovato rileggendo §19.8 mentre ci si preparava a leggere §21
per l'unità successiva (integrazione culling): *"Il pannello... è
forzato aperto a ogni `openLightbox()` (e all'apertura dal culling)."*
Dalla 2/N in poi il pannello partiva **chiuso** (`const info =
ref(false)`) — mai notato perché ogni singolo test di
`AssetViewer.spec.ts`, in tutte le sette unità precedenti, apriva il
pannello esplicitamente con un `dispatchEvent(keydown 'i')` prima di
verificare qualunque contenuto: il test compensava il difetto invece di
scoprirlo, praticamente all'incontrario di quello che dovrebbe fare.

Corretto: `info` parte `true`; `loadPanelData()` (i sette giri paralleli
di titolo/scatto/posizione/persone/tag/album/stack) scatta da
`onMounted`, non solo dal primo `i` o click sull'icona — `i`/l'icona
restano il modo per chiudere e riaprire, comportamento invariato. Nuovo
test dedicato: pannello visibile al mount senza alcuna interazione, `i`
lo chiude, l'icona lo riapre.

**Conseguenza attesa e sistemata nello stesso giro**: `loadPanelData()`
ora scatta ad **ogni** montaggio di `AssetViewer`, non solo quando un
test lo apre esplicitamente — tre file di test che montano il
componente reale dentro una vista (non uno stub) avevano mock di
`apiFetch`/`@/api/timeline`/`@/api/albums` scritti assumendo che quel
giro non sarebbe mai scattato:
- `SearchView.spec.ts`: un test impostava `apiFetch.mockResolvedValue(photo('a'))` **per tutte** le chiamate (non solo quella attesa) — con più chiamate reali in gioco, `fetchTags()`/`fetchTagsForAsset()` ricevevano un oggetto singolo invece di un array e `.filter()` andava in eccezione. Corretto rendendo `mountSearch()` parametrizzabile su un'implementazione di `apiFetch` (default `[]`), invece di un valore fisso globale.
- `TimelineView.spec.ts`: `vi.mock('@/api/timeline', ...)` non includeva `fetchAsset` (aggiunta dal Task 8 3/N, dopo che questo mock era stato scritto) — Vitest lo segnalava come export mancante non silenzioso. Aggiunto, instradato sullo stesso `apiFetch` già mockato nel file.
- `FavoritesView.spec.ts`: `vi.mock('@/api/albums', ...)` non includeva `fetchAlbumsForAsset` (aggiunta dal Task 8 7/N) — stesso errore, stessa correzione.

Nessuno di questi tre era un difetto nuovo introdotto oggi: erano scritti
correttamente per il comportamento di *allora* (pannello chiuso di
default) e sono rimasti silenziosamente indietro ad ogni unità successiva
che ha aggiunto un nuovo giro di rete a `loadPanelData()` — scoperti solo
ora perché solo ora quel giro scatta sempre, non più dietro un `if
(info.value)` che nella pratica dei test non si avverava mai.

Verifica completa: `npx vue-tsc -b` pulito. `npx vitest run` → 78 file,
637/637 verdi, **zero errori non gestiti** (contro i 10 della prima
esecuzione dopo la modifica, prima di correggere i tre file di vista —
un'esecuzione "verde" può nascondere `Unhandled Rejection` reali se non
si guarda oltre il conteggio dei test, esattamente il punto di PROSEGUI.
md §10: un test verde non basta). 35 test in `AssetViewer.spec.ts` (1
nuovo). `npx eslint` sui file toccati e sull'intero repo → pulito
(stesso unico errore preesistente su `PlayerView.vue`). `npm run build`
+ calcolo manuale del bundle iniziale gzip → 125.971 byte, sotto il
budget di 153.600.

## Task 8 (10/N) — Integrazione col culling (§21): ultima unità del Task 8

Letto per intero §21 (`docs/ui/documento-funzionale-ui.md:3995-4092`)
prima di scrivere codice. Discriminante unico dichiarato dal mockup:
`isCulling` (`!!p.batchId` nel mockup). Aggiunto a `AssetViewer.vue` un
prop `isCulling?: boolean` (default `false`, commento header esteso con
il contratto) che governa cosa sparisce rispetto al lightbox normale:
sezione PERSONE (`v-if="!isCulling && asset.faces.length > 0"`), sezione
TAG e sezione ALBUM (entrambe `v-if="!isCulling"`), voce "Aggiungi ad
album" nel menu ⋯ e nel pannello AZIONI, voce "Elimina…" nel menu ⋯ e
nel pannello AZIONI (con relativo separatore). `loadPanelData()` salta i
tre giri di rete corrispondenti quando `isCulling` è vero
(`fetchTagsForAsset`/`fetchTags`/`fetchAlbumsForAsset` sostituiti da
`Promise.resolve([])` inline) invece di limitarsi a nascondere la UI e
lasciare partire comunque chiamate il cui risultato non verrà mai
mostrato — niente giri a vuoto. "Rinomina…" resta ovunque, invariato,
come da elenco esplicito del documento ("cosa resta identico").

`CullingView.vue`: nuovo pulsante tondo "info" sullo stage (`aria-label`
da `t('culling.infoButton')`, visibile solo quando
`store.currentAsset && !store.compareMode`) che imposta
`viewingId = store.currentAsset.id`; un computed `viewingAsset` risolve
l'id nella `TimelineAsset` completa via l'`assetsById` già esistente nello
store (fonte del **lotto intero**, non della sola `order` filtrata — la
lookup "id di qualunque foto del lotto, comprese quelle escluse dal
filtro attivo" richiesta da §21.2 viene quindi gratis, senza codice
nuovo). `<AssetViewer>` montato in coda al template con `is-culling`,
`:neighbors="orderedAssets"` (già il computed esistente che mappa
`store.order` — la navigazione filtrata, esattamente quella richiesta),
e il cuoricino agganciato al vero `useFavoritesStore()` invece che al
campo `isFav` inesistente/write-only del mockup (stessa scelta già presa
per il resto del pannello — vedi Task 8 1/N — di preferire la capacità
reale del backend a una fedeltà pedante a un difetto del mockup che non
ha giustificazione nel sistema vero).

**Bug prevenuto, non solo corretto**: senza guardia, le scorciatoie del
culling (voto, pick/reject, frecce, zoom, confronto, cancella)
avrebbero continuato a rispondere alla tastiera mentre il lightbox è
aperto sopra lo stage, dato che `AssetViewer.vue` registra un proprio
listener `keydown` separato e indipendente da quello di `CullingView.
vue` — esattamente la stessa classe di bug Esc già trovata e corretta al
Task 8 7/N, ma qui prevenuta invece che scoperta a posteriori, perché
questa volta il codice è stato scritto sapendo già che i due listener
non si coordinano. Corretto con una guardia `if (viewingId.value)
return` come **primo** controllo dentro `onKey` di `CullingView.vue`,
prima persino di `isTypingTarget`.

Ciò che il documento dichiara "resta identico" (nome file, data/ora,
titolo, stelle, commutatore RAW/JPEG, SCATTO, POSIZIONE, filmstrip,
frecce, barra superiore, Scarica originale/Ruota/Rinomina) non ha
richiesto nessuna modifica: era già tutto indipendente da `isCulling`
per costruzione. La differenziazione del breadcrumb (nome lotto/stato
vs link cartella) resta debito dichiarato e non esteso, non a metà
costruito: né il mockup né il backend reale hanno un'entità "lotto"
persistita a cui puntare (`culling.start(list)` prende un array
client-side effimero, senza nome né id) né una rotta di risoluzione nome
cartella — la stessa classe di debito già lasciata dichiarata al Task 8
(8/N) per "Vai alla persona"/link cartella.

**Bug di test scoperto e corretto durante questa unità (non un difetto
di produzione)**: le due prime versioni dei nuovi test in
`CullingView.spec.ts` usavano asserzioni in italiano
(`'Dettagli foto — EXIF, posizione, rinomina'`, `'Tag'`, `'Album'`,
ecc.), ma questo file — a differenza di `AssetViewer.spec.ts` — non ha
mai impostato `i18n.global.locale.value = 'it'` in nessun `beforeEach`:
la convenzione consolidata qui è la lingua inglese di default di jsdom
(`navigator.language === 'en-US'`), già usata da test preesistenti nello
stesso file (`'Loading'`, `'Culling'`). Diagnosticato leggendo il DOM
reale stampato dal fallimento (`aria-label="Photo details — EXIF,
location, rename"`) e corretto traducendo le due nuove asserzioni in
inglese, verificate contro le stringhe reali di `en.json`
(`viewer.actions.{rename,download,delete,addToAlbum}`,
`viewer.panel.{tags,albums}`) — non una correzione di codice di
produzione, ma un promemoria dello stesso principio già annotato al Task
8 (3/N) sulla localizzazione nei test.

Verifica completa: `npx vitest run src/views/CullingView.spec.ts` → 9/9
verdi. `npx vitest run` (intero repo) → 78 file, **639/639** verdi (2
nuovi in `CullingView.spec.ts`). `npx vue-tsc -b` pulito. `npx eslint`
sui file toccati (`AssetViewer.vue`, `CullingView.vue`,
`CullingView.spec.ts`, `it.json`, `en.json`) e sull'intero repo →
pulito (stesso unico errore preesistente su `PlayerView.vue`,
confermato invariato). `npm run build` + calcolo manuale del bundle
iniziale gzip (stesso algoritmo di `.github/workflows/ci.yml`) →
125.897 byte, sotto il budget di 153.600.

**Con questa unità si chiude il Task 8** ("Lightbox, pannello
informazioni, menu ⋯", §18–21) — dieci sotto-unità, nessun debito reale
rimasto se non le destinazioni dichiaratamente non ancora esistenti
altrove nell'app (Persone → Task 16, ricerca volti → Task A, link
cartella/lotto → nessuna rotta di risoluzione nome in nessuna delle due
direzioni). Si prosegue con i Task 9–14 della Tranche B (Cerca, Mappa,
Condivisioni, Album, Manutenzione, Impostazioni/Profilo).

## Task 9 (1/N) — Cerca: il composer a pillole (§23-24), sostituisce la sintassi digitata

Letti per intero §23 e §24 (`docs/ui/documento-funzionale-ui.md:4104-4382`)
prima di scrivere codice. `SearchView.vue` esistente non era una versione
parziale del mockup ma un'architettura del tutto diversa: un campo +
pulsante "Cerca" + un mini-linguaggio digitato (`type:.../camera:...`,
`frontend/src/search/parse.ts`, con AND/OR/NOT/parentesi) mai previsto dal
documento — che è esplicito al contrario: *"non esiste alcun altro modo
di creare un filtro strutturato — né digitando e premendo Invio"* (§23.5).
Riscritta la vista: pillole + un nodo `text` per la descrizione libera,
niente sintassi, niente pulsante — la ricerca si ricalcola a ogni
carattere (nessun debounce, come richiesto).

**Il vecchio parser è stato ritirato, non lasciato spento**: `frontend/
src/search/parse.ts` e il suo `parse.spec.ts` eliminati (il tokenizer/
parser non aveva più alcun chiamante reale — l'unico consumatore era
`SearchView.vue`), il tipo `SearchNode` spostato in un nuovo `frontend/
src/search/ast.ts` (stesso enum, senza il tokenizer), esteso con `Tag{id}`
e `Country{value}` per rispecchiare per intero `SearchNode` del backend
(`crates/keeppix-db/src/search.rs:33-156`) nel sottoinsieme che la barra
sa produrre.

**Le sette categorie di suggerimento (§23.2) vengono da due fonti reali
diverse, non da un'unica lista precaricata come nel mockup** — verificato
leggendo `crates/keeppix-db/src/search.rs:396-460` (`SearchRepo::suggest`)
prima di decidere:
- **Tag**: `GET /search/suggest` non produce mai righe di genere `tag` —
  il commento a codice lì lo dice esplicitamente ("la tabella dei tag non
  esiste ancora", scritto in Fase 10 prima che la Fase 7 la creasse, mai
  aggiornato). Costruito qui filtrando `fetchTags()` lato client (stessa
  tecnica del mockup, ma su dati reali).
- **Fotocamera/Cartella/ISO/Anno/Paese**: dal vero endpoint, che a
  differenza del mockup calcola su dati reali di libreria invece che su
  costanti cablate (l'anno "2026" e il paese "Italia" del mockup erano
  letteralmente hardcoded nel prototipo) — usato così com'è, un
  miglioramento reale non un compromesso.
- **Cartella**: serviva l'intero albero appiattito, non solo le radici —
  `fetchTree()` esistente è cablata su `?roots=true` per i due chiamanti
  che già la usano (`FoldersView`/`SharesView`, alberi pigri). Aggiunta
  `fetchAllFolders()` (`frontend/src/api/folders.ts`), stesso endpoint
  senza il parametro, per non filtrare per sbaglio le sottocartelle da
  una ricerca.
- **Posizione (GPS)**: nessuna fonte reale — `SuggestionKind` del backend
  non ha un genere per questo. Riprodotta pari al mockup: una riga
  pseudo-generata quando il testo è sottostringa di "gps".
- **Paese**: il backend restituisce il codice ISO grezzo (`p.country_code`,
  es. "IT"), non un nome leggibile — nessuna tabella codice→nome esiste
  altrove nell'app (verificato: nessun file "places" nel frontend).
  Deviazione dichiarata dal mockup, che mostrava "Italia" cablato.

`frontend/src/api/search.ts` aggiornato dallo stesso giro: il tipo
`fetchSuggestions` era rimasto `{suggestions: string[]}`, ma il backend
restituisce da tempo oggetti tipizzati (`SuggestionView{kind,value,label,
color}`, `crates/keeppix-api/src/routes/search.rs:64-84`) — tipo corretto
per riflettere la realtà, primo vero consumatore.

**Gap di accessibilità che il documento stesso segnala come "da colmare
nell'implementazione Vue"** (§23.5: righe di suggerimento non
raggiungibili da tastiera nel mockup) colmati, non riprodotti: le righe
sono `<button>` veri (tabbable di natura, non `div` con `tabindex`
mancante come nel mockup), e le frecce ↑/↓ spostano il focus tra le righe
del pannello aperto (`focusFirstRow`/`focusNextRow`/`focusPrevRow`), Esc
chiude il pannello senza toccare pillole o testo.

Click-fuori-chiude (§23.4) implementato con un listener `mousedown` a
livello di `document`, montato/smontato col ciclo di vita del componente
— non `Popover.vue` (SP-14): quel componente lega apertura/chiusura a un
`PopoverTrigger` cliccabile e a `PopoverContent` che nella pratica di
reka-ui gestisce anche lo spostamento del focus al contenuto — sposterebbe
il focus fuori da `#cercaInput` mentre si digita, il contrario di quanto
richiesto qui (il cursore deve restare nel campo). Nessun pattern
riutilizzabile esisteva già nel repo per un dropdown ancorato a un campo
di testo sempre attivo (verificato: l'unico `contains(target)` esistente
è un gestore di mappa non correlato).

`?photo=`/`?q=` nell'URL (SP invariata da prima di questo task) resta,
ma `router.replace` ora fonde la query esistente invece di sovrascriverla
(`{...route.query, q}`), altrimenti digitare mentre il lightbox è aperto
avrebbe cancellato `?photo=` a ogni carattere.

**Fuori campo per questa unità, dichiarato**: i chip del tipo file (Tutti/
RAW/JPEG/Preferiti/Persona disabilitato, §23.3 controlli 5-9) e l'intera
area risultati di §25 (ricerche salvate, card cartella, "Aggiunti di
recente", riepilogo "Risultati", stato vuoto) — la griglia resta quella
semplice preesistente, rifatta nella prossima unità.

Verifica completa: `npx vitest run` → 77 file (78-1: `parse.spec.ts`
eliminato), 648/648 verdi (15 nuovi in `SearchView.spec.ts`, contro i 3
preesistenti riscritti). `npx vue-tsc -b` pulito. `npx eslint` sui file
toccati e sull'intero repo → pulito (stesso unico errore preesistente su
`PlayerView.vue`). `npm run build` + calcolo manuale del bundle iniziale
gzip → 126.265 byte, sotto il budget di 153.600.

## Task 9 (2/N) — Cerca: i chip del tipo file (§23.3 controlli 5-9)

Cinque chip sotto il composer: "Tutti i tipi"/"RAW"/"JPEG"/"Preferiti"
(mutuamente esclusivi, `typeFilter` a quattro stati, default `'all'`,
nel mockup non si torna mai a "nessuno" — solo `setTypeFilter` da un
chip all'altro) e "Persona" (`<span>`, non un `<button>`: nessun
gestore di click, `title` HTML nativo `"Richiede riconoscimento volti —
vedi Gruppo B"` — non un tooltip `[data-tip]` di Keeppix, come richiede
esplicitamente il documento).

**RAW/JPEG non riproducono il booleano binario `isRaw` del mockup**: il
sistema reale ha `kind` a quattro valori (`image`/`raw_image`/`video`/
`unknown`, `crates/keeppix-db/src/search.rs:911-914`), non solo RAW/
JPEG. "RAW" filtra `{op:'type', value:'raw_image'}`; "JPEG" filtra
`{op:'type', value:'image'}` **esatto**, non "tutto ciò che non è RAW"
— altrimenti includerebbe anche i video, che nel mockup (solo foto)
semplicemente non esistevano. Stesso principio già seguito al Task 8
(10/N) per il chip "JPEG" del lightbox.

I chip si combinano in AND con pillole e testo (`buildAst()` ora
antepone il nodo del tipo, se presente, prima delle pillole — stesso
ordine, stesso meccanismo `and`). Confermato dal test dedicato: con
"Preferiti" attivo e una pillola `tag` aggiunta, l'AST è `{op:'and',
args:[{op:'favorite'},{op:'tag',id:...}]}`.

`clearAll()` (✕ "Cancella la ricerca") non tocca `typeFilter`, per
costruzione — non serviva codice apposito, solo non includerlo nel
reset, esattamente come richiesto (§23.3, riga 4179: "**Non** azzera il
chip del tipo file").

**Fuori campo, dichiarato**: l'area risultati (§25) resta quella
semplice preesistente — con solo "RAW" attivo e nessuna pillola/testo,
`buildAst()` produce comunque un AST valido (il nodo `type` da solo) e
la ricerca gira, ma la vista non mostra ancora il titolo "Aggiunti di
recente"/le card cartella che il documento richiede in quello stato:
arriva con l'unità successiva, che ricostruisce l'intera area risultati.

Verifica completa: `npx vitest run` → 77 file, 653/653 verdi (5 nuovi
in `SearchView.spec.ts`). `npx vue-tsc -b` pulito. `npx eslint` sui
file toccati e sull'intero repo → pulito (stesso unico errore
preesistente su `PlayerView.vue`). `npm run build` + calcolo manuale
del bundle iniziale gzip → 126.349 byte, sotto il budget di 153.600.

## Task 9 (3/N) — Cerca: l'area risultati (§25), chiude il Task 9

Letto per intero §25 prima di scrivere codice. Due fonti alimentano la
stessa griglia (mai insieme): `assets` (ricerca vera, tutte le pagine,
già esistente) quando `hasSearch`; `recentAssets` (una sola pagina,
nuovo) quando non c'è ricerca — `visibleAssets` computed sceglie fra le
due, usata ovunque (griglia, lightbox, barra di selezione).

**"Aggiunti di recente" non riproduce l'algoritmo `monthDistance` del
mockup**: lì le foto erano ordinate per distanza dal "mese corrente
della demo" (luglio, cablato) — un surrogato che nella demo coincide con
la vera recenza solo perché il catalogo copre un solo anno. Il backend
reale ordina già `/search` per `taken_at_utc DESC`
(`crates/keeppix-db/src/search.rs:246`): usata quella (il titolo della
sezione promette "recente", non "vicino a un mese fisso"), **e** una
sola pagina invece della paginazione esaustiva di una vera ricerca —
più corretto e più economico insieme, non un compromesso.

**Cartelle**: tre card con copertina (prima foto), nome e conteggio
**ricorsivo reale** — `runSearch({op:'folder',id})` per cartella
(`f.path <@ ...`, include le sottocartelle, non un conteggio a un solo
livello), non un numero stimato. Click → `/folders`: verificato che non
esiste, nell'app reale, alcuna "vista Foto scoperta su una cartella" da
raggiungere (né `TimelineView.vue` ha un concetto di cartella corrente,
né `FoldersView.vue` legge parametri di rotta) — `/folders` è la
destinazione reale più vicina, non un salto diretto come nel documento
ma non un link morto.

**Ricerche salvate**: i chip mostrano `fetchSavedSearches()` ma **non
sono cliccabili** — gap dichiarato, non costruito: `SearchRepo::
saved_query` (`crates/keeppix-db/src/search.rs:504-519`, che
interpreta `query_text` in un `SearchNode` eseguibile) esiste ma non è
instradato da **nessuna** rotta API (verificato: zero riferimenti in
`crates/keeppix-api`) — non c'è modo di rieseguire una ricerca salvata
per id nel sistema reale oggi. Riparsare `query_text` a mano nel
frontend duplicherebbe una grammatica che vive solo nel backend, e
sbaglierebbe silenziosamente sulle ricerche con pillole tag/paese o il
filtro Preferiti (la stessa grammatica che segue, vedi sotto, non le sa
rappresentare): mostrare il chip com'è, senza un comportamento fasullo,
è la scelta onesta.

**"Salva questa ricerca" scrive per davvero, ma solo quando può farlo
correttamente**: la grammatica testuale che il backend sa ancora
interpretare (`crates/keeppix-db/src/search.rs:696-798`,
`parse_query_text`/`value_node`, precedente alla Fase 7/9) capisce solo
`type:`/`camera:`/`lens:`/`iso:`/`folder:`/`has:gps`/un anno nudo/testo
libero fra virgolette — non ha mai imparato `tag:`, `country:` né una
parola chiave per "preferiti". `serializedQuery` (computed) restituisce
`null` — pulsante disabilitato con `title` esplicativo — quando una
pillola è `tag`/`country` o il chip attivo è "Preferiti"; altrimenti
costruisce la stringa reale (`camera:"Sony A7 IV" "tramonto con casa"`,
verificato nel test) e chiama `createSavedSearch()` per davvero, con
l'etichetta che concatena le etichette delle pillole e il testo libero
con `" + "` (§25.3 riga 3).

Riusata l'infrastruttura già esistente per le griglie a tessere
(`FlatAssetGrid`/`SelectionBar`/`LibrarySelectionActions`/
`useSelectionStore`, lo stesso schema di `FavoritesView.vue`) invece di
scrivere una nuova griglia: SP-1/SP-2 vengono gratis, correttamente
condivisi fra la griglia di scoperta e quella dei risultati (stessa
`visibleAssets`). Niente `QuickFilter`/`SelectAllVisible`/controlli di
densità in vista: il documento li esclude esplicitamente per questa
pagina (§25.3: "Nel mockup non esiste in questa pagina: il pannello
imbuto del filtro rapido... l'ordinamento... la paginazione").

**Bug reale trovato e corretto durante questa unità**: `hasSearch`
(usato da `visibleAssets`, letto sincronamente dal watcher `immediate:
true` di `useLightboxRoute` durante il montaggio) era dichiarato molto
più in basso nel file rispetto a `visibleAssets`/`lightbox` — un errore
di temporal dead zone che rompeva silenziosamente il primo rendering di
`FlatAssetGrid` (`props.assets` risultava `undefined`) ogni volta che
il lightbox si apriva da `?photo=` al caricamento della pagina.
Diagnosticato isolando il test che falliva sempre e solo in quel
percorso, con una sonda `console.log` che non veniva mai raggiunta —
prova che l'eccezione (TDZ) avveniva prima, dentro la valutazione del
computed stesso. Corretto spostando la dichiarazione di `hasSearch`
(e di `pills`, da cui dipende) prima di `visibleAssets`/`lightbox`, con
un commento che spiega il perché dell'ordine — non solo un fix, un
promemoria contro la stessa classe di errore in futuro.

Verifica completa: `npx vitest run` → 77 file, **658/658** verdi (5
nuovi in `SearchView.spec.ts`, portando il totale del file a 25; corretto
anche `AppTopbar.spec.ts`, che monta `SearchView` per davvero e non
aveva ancora gli stub di `matchMedia`/layout né i mock di `@/api/tags`/
`@/api/folders` diventati necessari con questa unità). `npx vue-tsc -b`
pulito. `npx eslint` sui file toccati e sull'intero repo → pulito
(stesso unico errore preesistente su `PlayerView.vue`). `npm run build`
+ calcolo manuale del bundle iniziale gzip → 126.856 byte, sotto il
budget di 153.600.

**Con questa unità si chiude il Task 9** ("Cerca", §23-25) — tre
sotto-unità: il composer a pillole, i chip del tipo file, l'area
risultati. Debito dichiarato e non costruito, tutto con una causa
reale citata: rieseguire una ricerca salvata (nessuna rotta backend),
salvare ricerche con pillole tag/paese/preferiti (grammatica di
salvataggio non estesa), saltare direttamente alla vista Foto di una
cartella (nessuna rotta/parametro nell'app reale). Si prosegue con i
Task 10-14 della Tranche B (Mappa, Condivisioni, Album, Manutenzione,
Impostazioni/Profilo).

## Task 10 — Mappa (§26-28), le quattro sezioni verificate una per una

Letto per intero §26 ("Mappa"), §27 ("Popover della mappa"), §28-A
(dialog "Imposta posizione") e §28-B ("Ricerca di regione", dentro
Impostazioni → Mappe offline) prima di scrivere codice, e usato un
agente di ricerca dedicato per mappare lo stato reale di ciascuna
sezione contro `MapView.vue`/`MapClusterLayer.vue`/`maps.ts`/
`PlacePickerDialog.vue` prima di decidere cosa costruire. Risultato:
quattro sezioni, quattro stati diversi — un caso di lavoro vero (§27),
due di deviazione già deliberata e documentata altrove (§26, §28-A) e
un debito reale con causa citata (§28-B).

**§26 "Mappa" — già superata, non riprodotta.** Il mockup è
esplicitamente una mappa statica (tre pin a posizione percentuale
cablata, zoom/pan/wheel "non implementati", `"300 km"` fisso). La vista
reale (`MapView.vue`, Fase 11 Task 6, già esistente) è una vera mappa
MapLibre GL con tile pmtiles offline, clustering server-side reale,
tema chiaro/scuro, e uno strumento di selezione area → Timeline
(`showArea`, assente dal documento). Nessun lavoro necessario qui:
verificato che è un miglioramento reale già costruito, non un debito.

**§27 "Popover della mappa" — debito reale, costruito in questa
unità.** Il backend era già pronto: `MapClusterView`/`MapCluster`
(`crates/keeppix-api/src/routes/map.rs:22-35`,
`crates/keeppix-db/src/geo.rs`) portano da tempo `folder_id` e
`place_label` con un commento a codice che cita esplicitamente questa
sezione del documento ("per aprirla dal popover... senza una seconda
richiesta") — il frontend non li leggeva ancora (`MapCluster` in
`frontend/src/stores/maps.ts` non li dichiarava nemmeno). Estesa
l'interfaccia, poi costruito il popover in `MapClusterLayer.vue`:
copertina, nome cartella (risolto da `folder_id` via `fetchAllFolders()`,
cache client-side), `"<N> foto · <luogo>"`, pulsante "Apri cartella".

**Non riproduce il modello "un pin = un popover" del mockup**: il
sistema reale ha clustering gerarchico (un marker aggregato può
rappresentare cartelle/luoghi diversi a zoom bassi), il mockup no (tre
pin terminali, ciascuno già alla granularità minima). Sintesi
deliberata: un marker **aggregato** (`clustered:true`) ora apre il
popover invece di zumare direttamente come prima; un marker **non
aggregato** continua ad aprire la foto (comportamento preesistente
invariato — un punto già alla granularità minima non ha altro da
mostrare che aprirlo). Lo zoom-per-esplorare non sparisce: resta
raggiungibile riaprendo il popover e cliccando di nuovo sullo stesso
marker via il comportamento di zoom preesistente nella mini-mappa
`compact` del lightbox, dove il popover **non appare affatto** — quel
riquadro è alto 176px con `overflow-hidden`, una card da 76px di sola
copertina ci starebbe a stento, e "Apri cartella" non ha senso nel
contesto di un singolo scatto già aperto nel visore.

**Colmati, non riprodotti, gli stessi gap di accessibilità che il
documento segnala per il popover del mockup** (§27.5: "nessun
`role="dialog"`, non riceve focus all'apertura, non lo restituisce alla
chiusura, non risponde a Esc"): il popover reale ha `role="dialog"`,
porta il focus sul pulsante "Apri cartella" all'apertura, lo restituisce
al marker che l'ha aperto alla chiusura, Esc chiude. Chiusura anche su
clic altrove sulla mappa (`map.on('click', ...)`, §26/27) e all'inizio
di un trascinamento (`movestart`) — altrimenti il popover resterebbe
fermo sullo schermo mentre la mappa scorre sotto, un bug visivo che il
documento non poteva prevedere (nel mockup statico non esiste il pan).

**§28-A "Imposta posizione" — già costruito, deviazione già
documentata.** `PlacePickerDialog.vue`/`PlacePicker.vue` (Task 8, 4/N)
sostituiscono le tre righe preimpostate a coordinate fisse del mockup
con una ricerca reale su `GET /places/suggest` (catalogo GeoNames) — un
commento a codice già presente in quei file documenta la scelta
(l'orfano `PlacePicker.vue`, mai agganciato a nessuna vista, riusato
invece di riprodurre l'elenco statico). Nessun lavoro necessario qui.

**§28-B "Ricerca di regione" — debito reale, dichiarato e non
costruito, causa citata.** Il documento vuole una casella di ricerca su
un catalogo cablato di 35 paesi (nome + dimensione stimata). Verificato
che **non esiste alcun endpoint di catalogo regioni** nel backend
(`grep` su `crates/keeppix-api`/`crates/keeppix-db` per "catalog": solo
risultati non correlati) — `MapsOfflineView.vue` espone oggi un modulo
grezzo che l'admin compila a mano con id/etichetta/dimensione/
versione/URL/checksum SHA-256 reali. Costruire una ricerca a scomparsa
sui 35 paesi del mockup richiederebbe **inventare** URL sorgente e
checksum per 35 estratti PMTiles reali — dati che non esistono in
questo repository e che non posso verificare né procurare in modo
affidabile in questa sessione. Un catalogo con URL/checksum fittizi
sarebbe peggio di nessun catalogo: scaricherebbe silenziosamente nulla
o dati sbagliati in un prodotto reale, mentre il modulo grezzo attuale
— per quanto meno comodo — richiede sempre dati verificati da chi lo
compila. Lasciato così, con la causa dichiarata qui e non un "manca
ancora" senza spiegazione.

Verifica completa: `npx vitest run` → 77 file, **663/663** verdi (6
nuovi/riscritti in `MapClusterLayer.spec.ts` — comportamento diverso
per marker aggregato/non aggregato/`compact`, apertura/chiusura del
popover, Esc, clic sulla mappa, inizio trascinamento — 1 nuovo in
`MapView.spec.ts` per "Apri cartella" → `/folders`). `npx vue-tsc -b`
pulito. `npx eslint` sui file toccati e sull'intero repo → pulito
(stesso unico errore preesistente su `PlayerView.vue`). `npm run build`
+ calcolo manuale del bundle iniziale gzip → 126.913 byte, sotto il
budget di 153.600.

Si prosegue con i Task 11-14 della Tranche B (Condivisioni, Album,
Manutenzione, Impostazioni/Profilo).

## Task 11 (1/N) — Condivisioni: dialog "Condividi selezione" (§30)

Letto per intero §29 e §30 e usato un agente di ricerca dedicato per
verificare lo stato reale di `SharesView.vue`, `LibrarySelectionActions
.vue`, `AssetViewer.vue`, `crates/keeppix-api/src/routes/share.rs`/
`permissions.rs` prima di scrivere codice. Confermato quanto già
dichiarato al Task 7 (2/N) e al Task 8: "Condividi" era un'assenza
deliberata sia nella barra di selezione sia nel menu ⋯ del lightbox,
perché **il backend non ha mai avuto un `object_type` per "una
selezione arbitraria di foto"** — verificato di nuovo, riga per riga,
in `crates/keeppix-db/src/share_links.rs` e `permissions.rs`: solo
`folder`/`album`/`asset` esistono ovunque (riga SQL, validazione della
rotta, `item_counts` — "un link asset conta sempre 1, senza query").

**Non serve estendere il backend**: un insieme arbitrario di foto è
esattamente ciò che un **album** già rappresenta, con permessi e link
pubblici già completi (`createAlbum`/`addAssets`, `grantPermission`/
`revokePermission`, `createShareLink` — tutti reali, già usati altrove).
Nuovo `ShareSelectionDialog.vue`: al primo uso reale (non all'apertura
del dialog) crea un album nascosto contenente solo la selezione, poi
condivide *quello* — "Selezione manuale" del mockup diventa un album
auto-generato nel sistema reale, stessa promessa ("non condividi
l'intera cartella/album di provenienza"), meccanismo reale invece di un
tipo che non esiste. Riusa il pattern già collaudato di
`AlbumPickerDialog.vue` (righe switch dentro `Dialog.vue`, `role=
"switch"`/`aria-checked`, nocca che scorre) — lo stesso dialog condiviso
SP-5 di `PlacePickerDialog.vue`, quindi la stessa deviazione già presa
lì viene ereditata gratis: click sul velo chiude (comportamento
migliore di reka-ui, non riprodotta la stranezza del mockup — §30.4 la
segnala esplicitamente come "deviazione da SP-5" nel mockup).

**Gap reale trovato durante la verifica, non ipotizzato**: la sezione
"Persone" del dialog richiede `GET /users` per elencare "persone già
invitate" — quella rotta è riservata agli admin
(`crates/keeppix-api/src/routes/users.rs`, `AdminAuth` su ogni
endpoint), e non esiste alcuna rotta alternativa per un utente normale
per elencare gli altri account dell'istanza (stesso per `GET /groups`).
La sezione "Persone" è quindi visibile solo per un admin
(`session.user?.role === 'admin'`, stesso pattern già in uso in
`PlacePicker.vue` per "Scarica regione"); per chiunque altro il dialog
mostra **solo** "Link pubblico" — pienamente funzionante per tutti,
senza richiedere quell'elenco. Dichiarato nel commento del componente,
non taciuto.

Colore avatar per le "altre persone" in condivisione: il documento
(SP-16, riga 9175-9178) specifica "hash-based via `hsl()`, indipendente
dalla scelta personale" ma non una formula esatta — nuovo
`frontend/src/design/avatarColor.ts`, hash deterministico su stringa →
tonalità 0-359, saturazione/luminosità fisse per restare leggibile con
testo bianco sopra (stesso vincolo del preset "Arancione" di SP-16).

`AssetViewer.vue`: "Condividi…" aggiunto al menu ⋯ e al pannello AZIONI
(stessa `[!isCulling]`, posizione dopo "Ruota" come nell'elenco
canonico del documento "preferiti / album / condividi / modifica /
elimina"), apre lo stesso dialog per l'asset singolo. Aggiunto
`shareDialogOpen` a `dialogRefs` (la lista che previene il bug Esc-
chiude-anche-il-lightbox-sotto già trovato e corretto al Task 8 7/N) —
lo stesso dialog nuovo avrebbe altrimenti reintrodotto esattamente
quella classe di bug.

`ShareLink` (`frontend/src/api/shares.ts`) e la nuova `SharedWithMe`
(`frontend/src/api/permissions.ts`, `GET /shared-with-me` — aggiunta
apposta per §29, mai consumata dal frontend finora) ora hanno
`item_count`/i campi reali già restituiti dal backend ma mancanti dal
tipo TS — preparazione diretta per la prossima unità (§29, la pagina
Condivisioni).

Verifica completa: `npx vitest run` → 78 file (nuovo `avatarColor.
spec.ts`), 670/670 verdi (5 nuovi/estesi in `LibrarySelectionActions.
spec.ts`, 1 nuovo in `AssetViewer.spec.ts`, 3 nuovi in `avatarColor.
spec.ts`). `npx vue-tsc -b` pulito. `npx eslint` sui file toccati e
sull'intero repo → pulito (stesso unico errore preesistente su
`PlayerView.vue`). `npm run build` + calcolo manuale del bundle
iniziale gzip → 127.348 byte, sotto il budget di 153.600.

Il Task 11 prosegue con §29 (la pagina "Condivisioni": due schede,
"Persone"/"Link pubblici"/"Cartelle e album condivisi" per "Le mie
condivisioni", "Condivisi con me" via `shared-with-me`) — riscrive
`SharesView.vue`, oggi uno strumento CRUD di permessi non allineato
alla UI del documento.

## Task 11 (2/N) — Condivisioni: la pagina "Le mie condivisioni"/"Condivisi con me" (§29), chiude il Task 11

`SharesView.vue` riscritta da zero: due schede (`?tab=mine|shared`,
persistito nell'URL come `?photo=`/`?q=` altrove in questa sessione),
tre sezioni per "Le mie condivisioni" (Persone, Link pubblici, Cartelle
e album condivisi), una per "Condivisi con me" — non più uno strumento
CRUD di permessi senza relazione con la UI del documento.

**Sezione "Persone" riservata agli admin, stessa causa già citata al
Task 11 (1/N)**: elencare "con chi ho condiviso" richiede risolvere
nome/e-mail dei soggetti (`GET /users`/`GET /groups`, entrambe
`AdminAuth`) — nessuna rotta alternativa per un utente normale.
Aggregazione reale, non finta: per ogni cartella (`fetchAllFolders()`,
l'intero albero, non solo le radici — la differenza già stabilita al
Task 9 1/N fra questa e `fetchTree()`) e ogni album, una chiamata a
`GET /permissions?object_type&object_id` in parallelo (N+1 delimitato,
stesso principio già accettato per i conteggi delle card cartella del
Task 9 3/N) — appiattite in righe risolte con nome/e-mail/ruolo/origine
di ereditarietà.

**"Invita" (§29.3 riga 5, "no handler" nel mockup) ora fa qualcosa di
reale**: riusa il form di concessione già costruito e testato nella
vecchia `SharesView.vue` (cartella/soggetto/ruolo/eredita, più lo
strumento "Perché può vederla?" di spiegazione della catena) — ricollocato
dentro un pannello a comparsa sotto "Persone", non cancellato: era
codice reale e funzionante, non uno scarto del rifacimento.

**"Copia" esiste solo per un link appena creato in questa sessione,
causa reale non ipotetica**: verificato che `GET /share/links`
(`LinkView`) non include mai il `token` — solo la risposta di
creazione lo restituisce, una volta sola
(`crates/keeppix-api/src/routes/share.rs`). Un link caricato dalla
lista non ha modo di ricostruire l'URL condivisibile: "Copia" compare
solo per un `link.id` presente in una mappa locale popolata da questa
stessa pagina, mai per un link che arriva già esistente da
`fetchShareLinks()` — verificato con un test dedicato. Il pulsante
`"Crea link di condivisione"` della sezione (§29.3 riga 8) non è
costruito qui: nel mockup non ha comunque alcun gestore, e la strada
reale per crearne uno resta `ShareSelectionDialog.vue` (Task 11 1/N),
da una griglia con una selezione.

**Le card di "Cartelle e album condivisi" sono cliccabili**, a
differenza del mockup, che le disegna con `cursor:pointer` ma nessun
gestore — il documento stesso lo segnala come "falsa affordance da
correggere nell'implementazione Vue" (§29.4). Portano rispettivamente a
`/folders` e `/albums`: nessuna vista "Foto scoperta su una cartella"/
"dettaglio album" esiste ancora (stessa lacuna già dichiarata al Task 9
3/N e al Task 10), quindi le destinazioni reali più vicine, non un
salto diretto fasullo. Conteggio elementi reale: `fetchAlbum(id)` per
gli album, lo stesso giro esaustivo `runSearch({op:'folder',id})` già
usato per le card cartella di Ricerca (Task 9 3/N) per le cartelle.

**Sottotitolo dei link pubblici costruito da campi reali**, non da una
stringa cablata: tipo oggetto, `"nessuna scadenza"`/`"scade il
<data>"`, `"password attiva"` solo se `has_password` (mai una dicitura
negativa, come richiede il documento), `"download originale
attivo"`/`"off"` — quest'ultimo riflette lo stato vero, a differenza
del mockup che secondo il documento non mostra mai "on" nei suoi due
soli esempi dimostrativi.

Verifica completa: `npx vitest run` → 78 file, **677/677** verdi (11 in
`SharesView.spec.ts`, riscritti: il form di invito ora richiede una
sessione admin impostata **prima** del montaggio — impostarla dopo non
ricarica i dati, esattamente come nell'app reale, dove il login avviene
prima di navigare qui). `npx vue-tsc -b` pulito. `npx eslint` sui file
toccati e sull'intero repo → pulito (stesso unico errore preesistente
su `PlayerView.vue`). `npm run build` + calcolo manuale del bundle
iniziale gzip → 127.924 byte, sotto il budget di 153.600.

**Con questa unità si chiude il Task 11** ("Condivisioni", §29-30) —
due sotto-unità: il dialog "Condividi selezione" (un album
auto-generato al posto di un `object_type` "selezione" mai esistito nel
backend) e la pagina Condivisioni stessa. Debito dichiarato: la sezione
Persone/Invita per chi non è admin (nessuna rotta di risoluzione nomi
accessibile a un utente normale), "Crea link di condivisione" dentro
Condivisioni (la strada reale resta il dialog di selezione), le
destinazioni delle card cartella/album (nessuna vista scoperta-da-id).
Si prosegue con i Task 12-14 della Tranche B (Album, Manutenzione,
Impostazioni/Profilo).

## Task 12 (1/N) — Album: griglia, dettaglio, creazione minima

Documento funzionale §41 ("Album — la griglia") e §42 ("Album —
dettaglio"), verificati riga per riga (righe 6226-6480). §43
("Creazione di un album", il filtro a 9 condizioni) è rimandato alla
2/N: qui solo un dialog di creazione minimo, nome soltanto — vedi sotto.

**Bug reale trovato e corretto nel livello dati, prima ancora della UI**:
`fetchAlbum(id)` era tipizzato per restituire `assets`, che
`GET /albums/{id}` (`routes::albums::get`) **non ha mai avuto** — l'elenco
membri vive solo su `GET /albums/{id}/assets`, mai chiamato dal frontend
fino a questa unità. `AlbumPickerDialog.vue` copriva il buco con
`detail?.assets ?? []`: l'appartenenza mostrata dal picker "Aggiungi ad
album" era **sempre vuota** in produzione, indipendentemente dallo stato
reale. Corretto aggiungendo `fetchAlbumAssets(id)` (`api/albums.ts`) e
ripuntando i tre consumatori (`AlbumPickerDialog.vue`, `SharesView.vue`
e i rispettivi test) alla rotta vera. `Album` stesso era tipizzato con
un campo `cover_hash` mai esistito sul backend, sostituito con
`cover_asset_id`/`cover_tint`/`monochrome` reali (mai scritti da alcuna
rotta — la copertina resta quindi calcolata lato client, vedi sotto).

**"Automatico" (il documento lo chiama filtro "che si aggiorna da solo"
in continuazione) non esiste sul backend reale**: un album con `rule`
resta un insieme di membri materializzato in `album_assets`, aggiornato
solo su richiesta (`POST /albums/{id}/refresh`, `BulkOutcome` con
`succeeded` = concatenazione di aggiunti+rimossi, senza distinguerli —
verificato in `routes::albums::refresh`). Conseguenza per questa unità:
N e l'intervallo di date di ogni album (§41.2, §42.2) sono sempre
calcolati dai membri **effettivi** restituiti da `fetchAlbumAssets`, mai
da una ricomputazione live sul catalogo — vero sia per un album
"dinamico" (`rule` presente) sia per uno manuale, perché sul backend
reale entrambi hanno la stessa forma di appartenenza materializzata.
`AlbumDetailView.vue` aggiunge un pulsante reale **"Aggiorna album"**
(solo se `album.rule` è presente) che rilancia `POST .../refresh`: è la
contropartita onesta dell'"Automatico" del documento, non prevista lì
(nessuna modifica post-creazione è prevista nel mockup) ma necessaria
qui — senza, un album dinamico non avrebbe mai modo di aggiornare la
propria appartenenza dopo la creazione.

**Copertina a gradiente calcolata lato client** (`design/albumCover.ts`,
`albumCoverGradient(seed)`), stesso principio hash→HSL già usato da
`avatarColorFor` (Task 11 1/N, SP-16: "hash-based, nessuna formula
esatta nel documento oltre al vincolo") — `cover_tint`/`monochrome` non
sono mai scritti da nessuna rotta, quindi nessun vero album "Bianco e
nero" è possibile con dati reali, ma ogni copertina resta deterministica
sull'id.

**`<N> foto · <intervallo>`** (§41.2, §42.2) non può leggere `a.range`
(mai esistito sul backend): `albums/range.ts`, `albumMonthRange()`,
calcola l'intervallo dalle date scatto reali dei membri (`taken_at_utc`)
con `monthFull()` (stesso helper dello scrubber, `Intl.DateTimeFormat`
localizzato — non una tabella di stringhe italiane). `null` quando
l'album non ha membri con una data nota: il chiamante sceglie fra
`"nessuna foto ancora"` (manuale) e `"nessuna foto corrisponde"`
(`rule` presente) — non un'unica dicitura generica.

**I tre stati vuoti distinti di §42.2 restano distinti** in
`AlbumDetailView.vue` (`emptyState`, mutuamente esclusivo): filtro
rapido troppo stretto (riusa `ui.filteredEmpty`, stessa dicitura di
Preferiti/Timeline), album dinamico senza corrispondenze, album manuale
davvero vuoto — tre situazioni diverse anche se il rendering finale si
somiglia, scelta deliberata del documento da non appiattire.

**"Crea album" apre un dialog minimo, solo nome** — non la pagina di
creazione a sé del §43 (nome, condivisione, editor di filtro a 9 campi,
anteprima live), rimandata alla 2/N per non lasciare la griglia/il
dettaglio senza un punto d'ingresso funzionante nel frattempo. Il
dialog copre comunque per intero il caso "Manuale" del §43 (nome pulito
dagli spazi, un toast se vuoto, atterraggio diretto nel dettaglio del
nuovo album).

**Prima rotta con un "aperto" osservabile dall'esterno della vista**:
`/albums/:id` è la prima destinazione dinamica reale di questa app (i
debiti dichiarati per `Cartelle / <nome>`/`Culling / <nome lotto>`
restano — nessuna delle due rotte espone ancora quello stato). Tre
punti di shell aggiornati di conseguenza, con un unico ref di modulo
condiviso (`nav/routeTitles.ts`, `activeAlbumName` — stesso principio
di `useDensity`, non uno store Pinia per un solo campo): la briciola
della topbar diventa `"Album / <nome>"` con solo il nome in grassetto
(§42.8, testuale), il titolo dell'header mobile mostra il nome
dell'album con la freccia indietro che torna a `/albums` invece che ad
"Altro", e sia `AppSidebar` sia `AppMobileTabbar` restano evidenziati
su "Album" anche col dettaglio aperto.

Verifica completa: `npx vitest run` → 82 file, **700/700** verdi (23
nuovi: 3 `albumCover.spec.ts` già dalla sessione precedente, 3
`range.spec.ts`, 7 `AlbumsView.spec.ts`, 9 `AlbumDetailView.spec.ts` +
1 di flakiness da `attachTo: document.body` non smontato fra i test,
corretto con un `afterEach` che smonta il wrapper — stesso principio
già noto da `AlbumPickerDialog.spec.ts`). `npx vue-tsc -b` pulito.
`npx eslint` sui file toccati e sull'intero repo → pulito (stesso unico
errore preesistente su `PlayerView.vue`). `npm run build` + calcolo
manuale del bundle iniziale gzip → 128.797 byte, sotto il budget di
153.600.

Debito dichiarato, esplicito: §43 (pagina di creazione con filtro a 9
campi) resta per la 2/N — questa stessa estenderà `search/ast.ts` con
`Lens`/`Rating`/`Pick`/`DateRange` per rappresentare le condizioni.

## Task 12 (2/N) — Album: creazione con filtro (§43)

Nuova `AlbumCreateView.vue` (`/albums/new`), sostituisce il dialog
minimo solo-nome della 1/N (rimosso, non commentato — era uno stopgap
dichiarato esplicitamente temporaneo). `search/ast.ts` esteso con
`Rating{cmp,value}`/`Pick{value}`/`date_range{from,to}` (rispecchiano
`SearchNode::Rating`/`Pick`/`DateRange` di `crates/keeppix-db/src/
search.rs`, `Lens` c'era già dal Task 9).

**Tre deviazioni reali dal documento, tutte verificate leggendo il
codice del backend, non per scelta stilistica** (documentate anche nei
commenti del file):

1. **Niente "Automatico"**: `PatchAlbumBody` (`routes/albums.rs`) non ha
   un campo `rule` — un album creato con `rule` non può mai tornare
   "puramente manuale", quindi "diventa manuale a tutti gli effetti" del
   documento non è raggiungibile su questo backend. L'unica modalità
   reale è "applica subito" (creazione + `POST .../refresh` immediato,
   la vera equivalenza di "Una tantum"): il controllo segmentato "Quando
   applicare" sparisce, non c'è una seconda opzione onesta da offrirci.
   L'album resta comunque riaggiornabile in seguito da
   `AlbumDetailView` (il pulsante "Aggiorna album" della 1/N) — bonus
   reale non previsto dal mockup, conseguenza naturale di come `rule`
   funziona davvero, non una bugia raccontata all'utente.
2. **Niente switch "Condiviso"**: `is_shared` è una colonna reale letta
   da `AlbumView` ma né `CreateAlbumBody` né `PatchAlbumBody` la
   scrivono mai — resta sempre `false`, stessa storia già documentata
   per `cover_tint`/`monochrome` al Task 12 1/N. La condivisione reale
   resta il percorso già costruito al Task 11 (permessi/link), da dopo
   la creazione.
3. **"Tipo file" offre solo RAW/JPEG**: `SearchNode::Type` filtra
   `assets.kind` (per singolo file), "RAW+JPEG" del mockup è
   l'accoppiamento client-side `raw_kind` (`useBrowseFilters.ts`), mai
   una query SQL — non rappresentabile in una `rule` persistita.
   "Fotocamera"/"Paese" non sono tendine con l'elenco dei valori
   distinti (nessuna rotta li enumera, solo `GET /search/suggest?q=`
   con prefisso non vuoto, limit 6): input di testo con un `<datalist>`
   alimentato dallo stesso endpoint della barra di ricerca (Task 9), non
   una vera tendina. "Obiettivo" non ha nemmeno un `SuggestionKind::
   Lens` sul backend: resta testo libero, senza suggerimenti.

**Gli altri 6 campi mappano un `SearchNode` reale uno a uno**: Cartella
(picklist multi-selezione su `fetchAllFolders()`, già pronta dal Task
9/10, combinata in `{op:'or',args:[...]}` quando più di una è spuntata),
Intervallo di date (`date_range`, estremi mancanti sostituiti con
`0001-01-01`/`9999-12-31` per "aperto da un lato"), Preferiti
(`{op:'favorite'}` o `{op:'not',arg:{op:'favorite'}}` per "Non è un
preferito" — nessun `SearchNode` negativo dedicato, `not` già esisteva),
Valutazione minima (`{op:'rating',cmp:'gte',value:N}`), Pick/Scarta
(`{op:'pick',value}`, tre stelle vere via `Intl`-indipendenti `★`/`☆`,
nessuna traduzione necessaria).

**Anteprima live realmente accurata**, a differenza del conteggio della
griglia (Task 12 1/N, che legge la membership materializzata): qui
`runSearch(rule)` valuta l'AST dal vivo sul catalogo reale, esaustivo a
pagine (stesso giro di `FavoritesView.loadFavorites`), con un debounce
di 300ms sui cambi di condizione/operatore/tipo — non ad ogni carattere
digitato in un campo di testo libero (camera/paese/obiettivo), che
aggiornerebbe la ricerca continuamente durante la digitazione.

Alla conferma: nome obbligatorio (toast + focus se vuoto, validazione
solo all'invio come da documento), "Basato su filtro" con zero
condizioni utili → toast dedicato, altrimenti `createAlbum(nome, rule?)`
seguita da un `refreshAlbum(id)` immediato **solo** se `rule` è
presente, poi navigazione diretta al dettaglio del nuovo album — "si
atterra nel dettaglio" del documento, verificato invariato.

Verifica completa: `npx vitest run` → 83 file, **706/706** verdi (6
nuovi in `AlbumCreateView.spec.ts`; `AlbumsView.spec.ts` aggiornato,
le due prove sul vecchio dialog sostituite da una sola prova di
navigazione a `/albums/new`). `npx vue-tsc -b` pulito. `npx eslint` sui
file toccati e sull'intero repo → pulito (stesso unico errore
preesistente su `PlayerView.vue`, stesso numero di warning
preesistenti). `npm run build` (il nuovo chunk `AlbumCreateView`,
11,86 KB / 3,67 KB gzip, è lazy — non tocca il bundle iniziale) +
calcolo manuale del bundle iniziale gzip → 129.779 byte, sotto il
budget di 153.600.

**Con questa unità si chiude il Task 12** ("Album", §41-43). Si
prosegue con il Task 13 della Tranche B (Manutenzione: Cestino/
Duplicati/Problemi, §45-49).

## Task 13 (1/N) — Manutenzione: Cestino (§45)

Documento funzionale §45 "Cestino", verificato riga per riga (righe
6841-6981). Preceduto da un'analisi di gap via agente in background su
tutto il Task 13 (§45-49): confermato che §49 (dialog di eliminazione a
3 opzioni) è già coperto da `DeleteDialog.vue` (nota della sessione
precedente corretta), che §46 (Duplicati) e §48 (dialog "file con
problemi") non esistono affatto, e che §47 (Problemi) esiste ma ignora
il campo `problems: ProblemView[]` già composto dal backend — tre unità
ancora da fare dopo questa.

**Bug reale trovato e corretto nel livello dati**, stessa classe del
bug Album del Task 12: `TrashedAsset` (`api/trash.ts`) dichiarava
`filename`/`expires_at`, mai esistiti sul vero `TrashItemView`
(`crates/keeppix-api/src/routes/trash.rs:176-185`, che ha invece
`asset_id`/`original_path`/`disk_action`/`days_remaining`) — la vista
leggeva `undefined` per entrambi, e usava l'id della **riga di
cestino** al posto dell'`asset_id` per ripristinare, quindi il
ripristino non avrebbe mai funzionato in produzione. `GET /trash`
restituisce anche una pagina cursor-based (`{items, next_cursor}`),
mai seguita: solo la prima pagina veniva mostrata. Corretto:
`TrashedItem` rispecchia `TrashItemView` esattamente, `fetchTrash`
segue `next_cursor` fino a esaurimento (stesso giro di
`FavoritesView.loadFavorites`).

**Due deviazioni reali dal documento, per capacità reale del backend
diversa dal mockup:**

1. **Miniatura vera, non un gradiente finto**: il mockup mostra "il
   gradiente della foto come miniatura" (stesso trucco delle copertine
   album) perché la sua base dati non porta immagini reali. Sul
   backend vero gli elementi in cestino sono foto vere ancora presenti
   in `assets` (`status='trashed'`, non cancellate finché non si
   sceglie "Elimina definitivamente") — `GET /assets/{id}` le trova
   ancora con `content_hash`/`thumbhash` validi. Un gradiente al posto
   della foto vera sarebbe un passo indietro reale: senza nome file
   (mai mostrato, per documento) sarebbe impossibile riconoscere cosa
   si sta per ripristinare o eliminare per sempre. Pattern N+1 (un
   `fetchAsset` per elemento) già usato ai Task 9/11/12.
2. **"<N> giorni rimanenti" è il vero conto alla rovescia**: il
   documento lo dichiara esplicitamente "annunciato ma non
   implementato" nel mockup (`20 + hash(id)%10`, sempre fra 20 e 29) —
   sul backend reale `days_remaining` è calcolato per davvero da
   `deleted_at` + 30 giorni (`routes/trash.rs:199-202`), già pronto,
   solo mai letto dal frontend prima d'ora.

**Fedele al documento nonostante le azioni siano ora reali e
permanenti**: "Svuota cestino" e "Elimina definitivamente" restano
senza dialog di conferma, senza toast di successo, senza
annullamento — comportamento esplicitamente voluto dal documento
(§45.3, "senza chiedere conferma... senza toast... senza
annullamento"), non un debito di questa unità: il documento è la
specifica di questa interfaccia, non solo un resoconto di una demo, e
non c'è un buco di capacità del backend che lo renda insostenibile —
solo una scelta di design già presa altrove nel documento e qui
rispettata. Un errore di rete resta comunque segnalato con un toast,
perché sul backend reale queste chiamate possono davvero fallire
(403/409/500 — permessi, conflitto sul ripristino, errore filesystem),
cosa che il mockup non prevede solo perché la sua base dati non può
fallire.

**Accessibilità da tastiera corretta rispetto al mockup**: il
documento chiama esplicitamente questa "la vista meno accessibile del
blocco" (§45.5) — un difetto dichiarato, non una scelta di design (a
differenza del "no conferma" sopra). I due pulsantini per riquadro
sono `<button>` reali con `aria-label`, rivelati da `:focus-within`
oltre che da `:hover` (Tailwind `group-focus-within`), coerenti con il
resto dell'app (SP-1) — stessa politica già seguita ovunque in questa
fase per i difetti di accessibilità dichiarati come tali nel
documento.

**"Elimina definitivamente" del singolo riquadro riusa la rotta di
cancellazione a 3 vie** (`DELETE /assets/{id}` con
`disk_action:'purged'`, già wrappata come `deleteAsset` in
`api/culling.ts`) invece di una rotta dedicata — verificato che
`authorize_choose` (`crates/keeppix-db/src/trash.rs:453-488`) non
richiede che l'asset non sia già in stato `trashed`, quindi richiamare
`choose` su un asset già cestinato per purgarlo funziona senza
modifiche al backend.

Verifica completa: `npx vitest run` → 84 file, **715/715** verdi (9
nuovi in `TrashView.spec.ts`, prima assente). `npx vue-tsc -b` pulito.
`npx eslint` sui file toccati e sull'intero repo → pulito (stesso
unico errore preesistente su `PlayerView.vue`, stesso numero di
warning). `npm run build` + calcolo manuale del bundle iniziale gzip →
130.041 byte, sotto il budget di 153.600.

Si prosegue con Task 13 (2/N): Duplicati (§46), da costruire da zero —
il backend (`crates/keeppix-api/src/routes/duplicates.rs`) è già
completo (lista gruppi, membri di un gruppo, risoluzione con
`keep`+`disk_action` applicata per davvero), il frontend non ha ancora
né vista né i wrapper per gli endpoint di membri/risoluzione.

## Task 13 (2/N) — Manutenzione: Duplicati (§46)

Documento funzionale §46 "Duplicati", verificato riga per riga (righe
6984-7155). Vista costruita da zero (`DuplicatesView.vue`, `/duplicates`)
— non esisteva, `AppSidebar.vue`/`MoreView.vue` la dichiaravano
esplicitamente non ancora fatta. Aggiunta anche alla sidebar desktop
(fra Cestino e Problemi), alla pagina "Altro" mobile, alla tab bar
mobile (`ALTRO_ROUTES`) e a `routeTitles.ts` — stessa quaterna di punti
toccata per ogni nuova destinazione di manutenzione (Task 13 1/N).

`fetchDuplicateMembers`/`resolveDuplicateGroup` aggiunte a
`api/library.ts`, accanto alla `fetchDuplicates`/`DuplicateGroup` già
esistenti dal Task 1c (mai consumate da una vista fino a questa unità).

**Due deviazioni reali dal documento, entrambe per capacità reale del
backend diversa dal mockup:**

1. **Niente "motivo probabile" per gruppo**: il mockup mostra un testo
   in linguaggio naturale (`"Stesso file importato due volte — import
   manuale e poi sync automatico dalla stessa scheda SD"`) scritto a
   mano nei due gruppi di prova (`DUPLICATE_GROUPS`) — `DuplicateGroupView`
   (`routes/duplicates.rs:19-24`) non ha un campo del genere: nessuna
   query può indovinare *perché* due file coincidono. Il sottotitolo di
   ogni gruppo qui è solo la parte reale (MB recuperabili), senza
   premettere un motivo inventato.
2. **La copia proposta come "da tenere" non è garantita "quella senza
   suffisso"**: nel mockup lo è per costruzione dei dati di prova. Sul
   backend reale `DuplicateRepo::members` ordina per `a.filename` puro
   (`crates/keeppix-db/src/assets.rs:562`) — uno spazio prima di
   `"(1)"` ordina **prima** di un punto prima dell'estensione, quindi
   l'ordine alfabetico non garantisce affatto che il file senza
   suffisso venga per primo. Si propone comunque `members[0]` come
   default (punto di partenza ragionevole, non una detection reale
   dell'originale che nessuna query supporta) — l'utente sceglie
   comunque liberamente con un click, come da documento.

**Il resto è fedele, incluso il punto più delicato**: la nota per
l'architetto del documento (§46.9) dice esplicitamente che nel mockup
"risolvere un gruppo non applica davvero" la modalità di eliminazione
scelta — qui invece `resolveDuplicateGroup` (`POST /duplicates/
{hash}/resolve`, `{keep, disk_action}`) la applica per davvero a ogni
membro del gruppo tranne quello tenuto, in un'unica chiamata reale;
"Risolvi gruppo"/"Ignora" restano comunque senza conferma propria (il
dialog di eliminazione a 3 opzioni, §9/`DeleteDialog.vue`, riusato
identico, resta l'unica conferma) e "Ignora" resta solo in memoria di
sessione (nessuna rotta per "non segnalare più" un gruppo esiste sul
backend — ma il mockup stesso non persiste oltre la sessione, quindi
non è una perdita di fedeltà).

Verifica completa: `npx vitest run` → 85 file, **722/722** verdi (6
nuovi in `DuplicatesView.spec.ts`, prima assente). `npx vue-tsc -b`
pulito. `npx eslint` sui file toccati e sull'intero repo → pulito
(stesso unico errore preesistente su `PlayerView.vue`, stesso numero
di warning; tre avvisi di indentazione nel file nuovo corretti con
`--fix`, poi riverificato tsc+test). `npm run build` (nuovo chunk
`DuplicatesView` lazy, non nel bundle iniziale) + calcolo manuale del
bundle iniziale gzip → 130.758 byte, sotto il budget di 153.600.

Si prosegue con Task 13 (3/N): Problemi (§47) — la vista esiste già ma
ignora il campo `problems: ProblemView[]` che il backend compone da
tempo (severità/titolo/descrizione/azioni in linguaggio naturale),
mostrando invece elenchi grezzi di nomi file.

## Task 13 (3/N) — Manutenzione: Problemi (§47) + dialog "file con problemi" (§48)

Documento funzionale §47 "Problemi" e §48 'Dialog "file con problemi"',
verificati riga per riga (righe 7157-7386). Costruiti insieme, come già
per griglia+dettaglio album al Task 12: "Vedi i 3 file" non ha senso da
spedire senza il dialog che apre.

**Bug reale corretto nel livello dati**: `Problems` (`api/library.ts`)
non dichiarava affatto il campo `problems: ProblemView[]` che il
backend compone da tempo (`crates/keeppix-api/src/routes/problems.rs`)
— severità/titolo/descrizione/azioni già in linguaggio naturale, pronti
per un bottone. `ProblemsView.vue` leggeva solo i tre secchi grezzi
(`offline_libraries`/`failed_jobs`/`error_assets`), ricostruendo a mano
un'interfaccia molto più povera di quella già pronta, senza azioni
funzionanti collegate — esattamente il difetto che il commento del
mockup a `attachProblemHandlers` descrive come già risolto lì
("prima erano pulsanti senza alcun comportamento collegato... ognuna
ora fa qualcosa di reale"), mai risolto per davvero in questo frontend
fino a questa unità.

**Rimossa la sezione "Duplicati" annidata** (non commentata): da questa
stessa tranche esiste `/duplicates` (Task 13 2/N), pagina reale e
completa — ripeterne un riassunto qui sarebbe ridondante e disallineato
dal documento, che tratta Duplicati come pagina a sé (§46), mai
annidata in Problemi.

**Tre deviazioni reali dal documento, tutte per capacità reale del
backend diversa dal mockup:**

1. **"Riprova connessione" ha un vero ramo di fallimento**: il
   documento dice che nel mockup "il tentativo riesce sempre" (nessun
   ramo "ancora offline"). `POST /libraries/{id}/probe`
   (`api/libraries.ts::probeLibrary`, mai wrappata prima d'ora) verifica
   per davvero se il percorso torna raggiungibile — **non fallisce mai
   con un errore HTTP se la libreria resta irraggiungibile**: risponde
   comunque `200` con `status:'offline'` invariato
   (`LibraryRepo::probe`, `crates/keeppix-db/src/libraries.rs:180-193`).
   Il chiamante deve quindi leggere il campo `status` della risposta,
   non solo l'esito della promise — un vero ramo "ancora offline" con
   un proprio toast, assente nel mockup solo perché la sua base dati
   non può fallire.
2. **Il dialog "Dettagli" mostra dati reali**, non il racconto NAS/SMB
   immaginario del mockup: `root_path` e `last_scan_at` (come "ultimo
   contatto riuscito", via `Intl.RelativeTimeFormat` — nessuna tabella
   di stringhe italiane scritta a mano) della libreria vera, letta da
   `fetchLibraries()`. Niente affermazioni su NAS/SMB: il backend non
   distingue un percorso di rete da un disco locale, `root_path` può
   essere l'uno o l'altro.
3. **`?lang=` sulla `GET /problems`**: le descrizioni composte
   arrivano già nella lingua della richiesta — passare `locale.value`
   dell'interfaccia evita un titolo in inglese dentro un'interfaccia in
   italiano (il mockup non ha questo problema, essendo mono-lingua).

**§48, il dialog "file con problemi"**: fedele fino al dettaglio più
onesto del documento — "elenco reale dei file coinvolti (prime N foto
della cartella)... non i tre file che hanno davvero il problema". Qui
`ProblemView.folder_id` (reale, presente solo quando il problema
riguarda esattamente una cartella) alimenta `fetchChildren(folderId)`,
e si mostrano le prime 3 `assets` — stesso comportamento "primi N, non
i file davvero coinvolti" del mockup, non un miglioramento silenzioso.
Click su una riga → `router.push({path:'/', query:{photo:id}})`, che
apre il visualizzatore su Timeline tramite `useLightboxRoute` già
esistente — "porta la vista a Foto e apre il visualizzatore" del
documento, senza scopare la timeline alla sola cartella del problema
(nessuna vista porta oggi un `currentFolder` osservabile, stesso debito
già dichiarato più volte in questa fase per `AppTopbar`/`caricamento-
nuove-foto.md`).

La sezione "Ricalcolo fusi orari" (`tzPreview`/`tzApply`), reale
strumento funzionante senza alcuna controparte nel documento (§47 non
lo menziona), resta in fondo alla pagina — non toccata nella sostanza,
solo riposizionata sotto ai problemi veri e propri invece che frammista
ai vecchi elenchi grezzi.

Verifica completa: `npx vitest run` → 86 file, **734/734** verdi (9 in
`ProblemsView.spec.ts`, riscritti da zero; 4 nuovi in
`ProblemFilesDialog.spec.ts`). Un bug reale trovato scrivendo i test
del dialog: `watch(open, ...)` senza `{immediate:true}` non caricava i
file se il dialog nasceva già aperto — corretto, stesso principio già
noto da `ShareSelectionDialog.vue`/`AlbumPickerDialog.vue`. `npx
vue-tsc -b` pulito. `npx eslint` sui file toccati e sull'intero repo →
pulito (stesso unico errore preesistente su `PlayerView.vue`, stesso
numero di warning). `npm run build` + calcolo manuale del bundle
iniziale gzip → 131.218 byte, sotto il budget di 153.600.

**Con questa unità si chiude il Task 13** ("Manutenzione", §45-49) — e
con esso la Tranche B della Fase 11. Si prosegue con il Task 14
(Impostazioni/Profilo), ultimo della Tranche B, poi con le Tranche C e
D.
