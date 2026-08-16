# Task 9 report — Modalità culling nel frontend

Branch: `fase-2`. Non pushato, come richiesto. Nessun file Rust toccato
(frontend-only, coerente con le istruzioni del task).

## Cosa è stato fatto

### Un unico punto d'ingresso

Pulsante **Culling** nell'header di `TimelineView.vue` (accanto al
link *Problemi*), visibile solo quando ci sono foto caricate. Avvia
`cullingStore.start(flatAssets)` e naviga a `/culling` (rotta lazy).
Nessuna scorciatoia sparsa fuori dalla modalità: il visualizzatore
normale (`AssetViewer.vue`) non è stato toccato — resta com'era,
niente rating/pick/reject/zoom/confronto/cancellazione lì dentro.

**Nota sul "cartella o selezione"**: il frontend non ha ancora una
vista per cartella né una selezione multipla in questa fase (solo
timeline piatta, ricerca, problemi). Il pulsante avvia una sessione su
tutto ciò che è già caricato in timeline — resta un punto d'ingresso
singolo, ma più grezzo del "cartella/selezione" della spec. Vedi
Ruling nel ledger.

### Store (`stores/culling.ts`)

Pinia setup store, stesso pattern di `stores/session.ts`:

| Concetto | Come |
|---|---|
| Navigazione filtrata | `order: string[]` (id nell'ordine corrente, ricalcolato solo in `start()`/`setFilter()` — mai dentro `vote()`, altrimenti la foto corrente "salterebbe" fuori vista nel mezzo di un'azione da tastiera) + `position: number` |
| Voti | `flagsById: Record<string, AssetFlags>`, aggiornati **ottimisticamente** in locale prima che la rete confermi |
| Resilienza di rete | `queue: QueuedVote[]` — un voto in coda per asset (l'ultimo vince se lo stesso asset viene rivotato prima che il precedente sia confermato), un tentativo alla volta in ordine FIFO, retry manuale (`retryQueue()`) o programmato (`setTimeout` 4s) su fallimento, più un listener su `window.addEventListener('online', …)` |
| Filtri | `all` / `pending` / `picks` / `rejects`, applicati su `pick` (`Pick::{none,pick,reject}` lato dominio) |
| Cancellazione | `remove()`/`removeMany()` — **non ottimistica**: il file sparisce dalla sessione solo dopo conferma dal server, riusa `deleteAsset()` (Task 7, `DELETE /api/v1/assets/{id}` con `disk_action`) |

### Componenti

- `RatingStars.vue`: 5 stelle cliccabili, `aria-pressed`/`aria-label`
  tradotti, `readonly` per contesti di sola visualizzazione.
- `Filmstrip.vue` (nome interno `CullingFilmstrip` via `defineOptions`
  per `vue/multi-word-component-names` — il file resta `Filmstrip.vue`
  come da piano): striscia orizzontale di miniature con badge
  scelta/scarto/rating, click per saltare a quella foto.
- `CullingView.vue`: la vista intera — immagine grande (preview,
  originale se zoomato, affiancate se in confronto), barra statistiche
  (scelte/scarti/da vedere), filtri, filmstrip, dialogo di
  cancellazione a tre opzioni (Task 7, riusato pari pari: rimuovi
  dall'indice / cestino / elimina dal disco).

### Scorciatoie (tutte e solo dentro `CullingView`)

`1-5` voto (avanza automaticamente se si vota la foto corrente) ·
`p` pick (toggle) · `x` reject (toggle) · `←`/`→` naviga senza votare ·
`z` zoom 1:1 · `c` confronto (fino a 4 foto in sequenza) · `Canc`/
`Backspace` elimina la foto corrente (apre il dialogo a tre opzioni) ·
`Escape` chiude (il dialogo, se aperto; altrimenti la vista).

**Guardia sui campi di testo**: nessuna scorciatoia scatta se
`event.target` è un `<input>` di tipo testuale, una `<textarea>`, una
`<select>` o un elemento `contenteditable` — verificato che radio/
checkbox (usati nel dialogo di cancellazione) **non** siano trattati
come "sto scrivendo", altrimenti `Escape` non chiuderebbe mai il
dialogo con un radio a fuoco.

### Zoom 1:1 istantaneo

Nessun endpoint di ritaglio lato server in questa fase (fuori dai file
che il brief elenca per Task 9). Realizzato precaricando l'intero file
originale (`/media/original/{id}`) con `new Image()` per le **3 foto
successive** nell'ordine filtrato corrente — non appena `position`
cambia — e mostrandolo a piena risoluzione (`max-width: none`) dentro
un contenitore centrato con `overflow: hidden`: il "ritaglio centrale"
è la finestra di visualizzazione, non un file più piccolo generato
apposta. Vedi Ruling nel ledger sul limite per gli asset RAW puri
(`/media/original/{id}` restituisce il file RAW, che un browser non
decodifica come immagine).

## I quattro test che dovevano fallire per primi

Osservati rossi con `npx vitest run` prima di scrivere
`stores/culling.ts` e `views/CullingView.vue` (import irrisolvibile),
poi verdi dopo l'implementazione:

1. **`l'avanzamento automatico dopo il voto porta alla foto
   successiva`** — `src/stores/culling.spec.ts`,
   `advances to the next photo after voting on the current one` (+
   `does not advance past the last photo`, `does not advance when
   voting on a photo that is not the current one`).
2. **`le scorciatoie non sparano mentre l'utente scrive`** —
   `src/views/CullingView.spec.ts`,
   `does not fire shortcuts while the user is typing in a text field`
   + `ignores shortcuts typed into a textarea or a contenteditable
   element`. Scoperta in corso d'opera: jsdom non implementa
   `HTMLElement.isContentEditable` (verificato isolatamente con `node
   -e` prima di modificare il codice) — la guardia controlla anche
   l'attributo `contenteditable` in DOM, non solo la proprietà.
3. **`il filtro «solo scarti» mostra ciò che dichiara`** —
   `src/stores/culling.spec.ts`, tre test: `rejects` mostra
   esattamente e solo i respinti, `picks` non mostra mai un respinto,
   `all` li mostra entrambi.
4. **`lo store non perde i voti se la rete cade: si accodano e si
   ritentano`** — `src/stores/culling.spec.ts`, due test: un
   fallimento di rete lascia il voto in `store.queue` e `retryQueue()`
   lo reinvia con successo; un secondo voto arrivato mentre il primo è
   ancora "in volo" non viene perso né scavalca il precedente.

## Verifica

```
$ npx vue-tsc --noEmit
(nessun output — pulito)

$ npx vitest run
 Test Files  11 passed (11)
      Tests  35 passed (35)

$ npm run lint
(nessun output — pulito, 0 warning ammessi)

$ npm run build
...
dist/assets/culling-*.js         3.12 kB │ gzip:  1.41 kB
dist/assets/CullingView-*.js     8.30 kB │ gzip:  3.21 kB
...
✓ built in ~0.3s
```

**Budget del bundle iniziale**, stessa misura del job `frontend` della
CI (solo gli asset referenziati da `dist/index.html`):

```
$ ASSETS=$(grep -oE '(src|href)="/assets/[^"]+\.(js|css)"' dist/index.html | ...)
$ # somma dei gzip di ciascun asset referenziato
TOTAL initial gzip: 80296 / 153600 byte (52%)
```

`CullingView-*.js` e `culling-*.js` (store + client API) sono chunk
lazy, mai citati in `index.html` — confermato con lo stesso comando
`grep` che usa `.github/workflows/ci.yml`.

Non eseguito `cargo test`/`cargo clippy`/`cargo fmt`: nessun file Rust
toccato da questo task (confermato con `git status --short` prima del
commit), e le istruzioni del task escludono esplicitamente Postgres/
backend per Task 9.

## File creati

- `frontend/src/api/culling.ts` — client per flags (`GET`/`PUT
  /api/v1/assets/{id}/flags`) e cancellazione (`DELETE
  /api/v1/assets/{id}` con `disk_action`), tutti endpoint già esposti
  da Task 7/8.
- `frontend/src/stores/culling.ts` + `culling.spec.ts` — store e 8
  test (navigazione, filtri, coda resiliente).
- `frontend/src/components/RatingStars.vue` — stelle 1-5.
- `frontend/src/components/Filmstrip.vue` — striscia miniature.
- `frontend/src/views/CullingView.vue` + `CullingView.spec.ts` — vista
  e 3 test (avanzamento da tastiera, guardia sui campi di testo ×2).

## File modificati

- `frontend/src/router.ts` — rotta `/culling` lazy (`meta: { auth:
  true }`, stesso pattern delle altre rotte protette).
- `frontend/src/i18n/{it,en}.json` — namespace `culling` (stesse
  chiavi in entrambe le lingue, verificato dal test esistente
  `i18n.spec.ts`).
- `frontend/src/views/TimelineView.vue` — pulsante *Culling*
  nell'header, funzione `startCulling()`.

## Decisioni degne di nota (vedi ledger per il dettaglio completo)

- Nessuna vista per cartella/selezione ancora nel frontend: il
  pulsante avvia una sessione sull'insieme già caricato in timeline.
- Zoom 1:1 è precaricamento + crop via CSS (`overflow: hidden`), non
  un endpoint di ritaglio lato server — limite noto sui RAW puri
  (`/media/original/{id}` restituisce un file che il browser non
  decodifica).
- `AssetViewer.vue` non toccato: la regola dura è rispettata per
  omissione, non introducendo funzionalità del culling lì.
- Nessun fetch collettivo dei voti pre-esistenti all'avvio (non esiste
  un `GET` batch per `asset_flags`): caricamento pigro per-asset via
  `ensureFlagsLoaded`.
- `p`/`x` sono toggle (premere due volte torna a "nessun voto"), non
  richiesto esplicitamente ma coerente con l'UX standard del settore.

## Commit

Da eseguire con `feat(web): add keyboard-driven culling mode` come da
istruzioni. Non pushato. Non avviata Fase 3.
