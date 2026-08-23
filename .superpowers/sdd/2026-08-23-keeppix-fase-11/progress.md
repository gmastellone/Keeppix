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
