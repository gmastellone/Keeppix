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

Debiti dichiarati: **diciassette componenti del Task 2 restano da
scrivere** (`PhotoTile`, `SelectionBar`, `QuickFilter`,
`SelectAllVisible`, `Popover`, `ToastHost`, `Tooltip`, `SuggestionQueue`,
`ProvenanceBadge`, `Avatar`, `AppShell`, `DeleteDialog`, `ConfirmDialog`,
`SegmentedControl`, `NavGroup`, `BusyButton`, `LoadingSkeleton` —
`RatingStars` esiste già, riusato non riscritto). Non un buco silenzioso:
la sezione «Tranche» del piano stesso struttura il lavoro in incrementi
verificabili, ed è così che questo task procede.
