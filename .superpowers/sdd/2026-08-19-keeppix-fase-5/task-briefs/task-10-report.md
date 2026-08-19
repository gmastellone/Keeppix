# Task 10 — Report: PWA Share Target (frontend)

## Stato

DONE_WITH_CONCERNS — implementazione tecnica completa, tutte le verifiche
automatiche richieste sono verdi, ma con due limiti reali documentati sotto
e nel ledger:

1. **La verifica su device Android/iOS reale è differita** (nota critica
   del brief, non risolvibile in questo ambiente).
2. **I file condivisi dalla galleria vengono accodati ma non partono da
   soli**, perché non esiste ancora — in nessuna parte del codebase, non
   solo per la Share Target — un'interfaccia che permetta di scegliere la
   cartella di destinazione prima dell'upload. Dettagli nel ledger
   (`progress.md`, sezione "Task 10").

## Commit

```
60c9a3b feat(frontend): PWA Share Target for photo upload from the phone gallery
```

Branch: `fase-5`.

## File creati/modificati

- CREATE `frontend/public/manifest.webmanifest` — manifest PWA con
  `share_target` (action `/share-target`, POST, multipart/form-data, campo
  `files` per `image/*` e `video/*`), come da snippet del brief.
- CREATE `frontend/public/sw.js` — service worker minimo: intercetta il
  POST a `/share-target`, legge i file dal `FormData`, li salva in una
  cache dedicata (Cache Storage) insieme a un indice JSON (nome, tipo,
  chiave), poi fa `Response.redirect('/share-target', 303)`. Nessun'altra
  responsabilità (niente caching offline di asset).
- CREATE `frontend/src/pwa/shareTarget.ts` — `readAndClearSharedFiles()`:
  legge l'indice e i file dalla stessa cache, ricostruisce i `File` dai
  blob, cancella le entry lette. Ritorna sempre `[]` (mai un errore) se
  `caches` non esiste o non c'è nulla in coda.
- CREATE `frontend/src/views/ShareTargetView.vue` — vista di passaggio
  montata su `/share-target`: al mount legge i file pendenti, li passa a
  `useUploadStore().addSharedFiles()`, poi torna a `/` (dove il pannello di
  upload globale li mostra).
- MODIFY `frontend/src/router.ts` — nuova rotta `/share-target` con
  `meta: { auth: true }`, coerente con le altre rotte protette.
- MODIFY `frontend/src/main.ts` — registrazione del service worker
  (`navigator.serviceWorker.register('/sw.js')`) dopo il `load`, dietro un
  controllo `'serviceWorker' in navigator`.
- MODIFY `frontend/index.html` — `<link rel="manifest">`, `<meta
  name="theme-color">`, `<meta name="mobile-web-app-capable">`.
- MODIFY `frontend/src/stores/upload.ts`:
  - `UploadSessionState.targetFolderId` e `PersistedSession.targetFolderId`
    ora `string | null` (era `string`).
  - `addFiles(fileList, folderId: string | null)` (era `folderId: string`).
  - nuova `addSharedFiles(files: File[])`: `await addFiles(files, null)`,
    come da snippet del brief.
  - `pump()` ora ignora le sessioni "queued" con `targetFolderId === null`:
    restano visibili ma non vengono mai avviate (vedi concern sotto).
  - `runUpload()`: guardia difensiva che marca "error" +
    `upload.errors.missingFolder` se una sessione senza cartella venisse
    comunque avviata (non dovrebbe succedere dato il filtro in `pump()`, ma
    serve anche a soddisfare TypeScript sul tipo nullable).
- MODIFY `frontend/src/i18n/en.json`, `frontend/src/i18n/it.json` — nuova
  chiave `upload.errors.missingFolder` (stessa chiave nelle due lingue,
  verificato da `i18n.spec.ts` già esistente).
- MODIFY `frontend/src/components/UploadPanel.spec.ts` — nuovo test
  `shared_files_are_queued_for_upload`.

Nessuna nuova dipendenza npm (`package.json`/`package-lock.json` non
toccati). Nessuna stringa hard-coded: l'unica UI nuova (`ShareTargetView`)
usa `t('common.loading')`, già esistente in entrambe le lingue.

## TDD — self-review

Sì. Prima di implementare `addSharedFiles`, ho scritto il test
`shared_files_are_queued_for_upload` in `UploadPanel.spec.ts` e l'ho
eseguito con lo store non ancora modificato (`git stash` temporaneo su
`src/stores/upload.ts`):

```
FAIL  src/components/UploadPanel.spec.ts > pannello di upload persistente — store > shared_files_are_queued_for_upload
TypeError: store.addSharedFiles is not a function
```

Fallito per il motivo giusto (funzione non esistente, non un asserzione
sbagliata). Poi ho ripristinato l'implementazione reale (`git stash pop`) e
rieseguito: verde.

Per il resto (manifest, service worker, rotta, view di passaggio) non ho
scritto test Vitest dedicati, in linea col brief ("Non ci sono test
automatici per la verifica device — documentare nel report") e perché
`caches`/`ServiceWorkerGlobalScope` non sono disponibili nell'ambiente
jsdom di Vitest: `readAndClearSharedFiles()` è scritta per restituire `[]`
in modo sicuro quando `caches` non esiste (branch coperta implicitamente da
qualunque test che monta l'app in jsdom senza mockare `caches`, ma non
testata esplicitamente con un'asserzione dedicata).

## Output di verifica finale

```bash
cd frontend && npm run build && npm run test && npx vue-tsc --noEmit
```

`npm run build`:

```
✓ built in 761ms
```

`dist/manifest.webmanifest` e `dist/sw.js` presenti e raggiungibili
(verificato leggendo il contenuto copiato); `dist/index.html` contiene
`<link rel="manifest" href="/manifest.webmanifest" />` e il meta
`theme-color`. Nuovo chunk lazy `ShareTargetView-*.js` (~0,96 kB / ~0,58 kB
gzip), fuori dal budget dei 150 KB iniziali (è lazy via router, come le
altre view).

`npm run test`:

```
Test Files  24 passed (24)
     Tests  92 passed (92)
```

(92 = 89 pre-esistenti + 3 nuovi indirettamente contati nel file
`UploadPanel.spec.ts`, di cui 1 è il test richiesto dal brief per
`addSharedFiles`; gli altri 2 erano già presenti prima di questo task).

`npx vue-tsc --noEmit`: nessun output, exit 0 — pulito.

Eseguito anche, non richiesto esplicitamente dalla "Verifica finale" ma
per coerenza con le altre verifiche di fase:

`npm run lint`: 0 errori. 9 warning pre-esistenti in `SharesView.vue`
(`vue/html-indent`), file non toccato da questo task (verificato che non è
nel diff).

## Nota critica: verifica su device reale — DIFFERITA

Come indicato esplicitamente nel brief e nella spec (`fase-5-webdav-upload.md`
§4.2: "su iOS il supporto è più limitato e va verificato"), la verifica
manuale con un dispositivo Android reale e uno iOS reale **non è stata
eseguita** e non è possibile in questo ambiente CI/agente: non c'è un
device fisico né un browser mobile con cui simulare realisticamente
"Condividi → Keeppix" dalla galleria (l'azione OS che invoca il Web Share
Target, non replicabile via `curl` o test headless in modo rappresentativo
del comportamento reale del browser).

Cosa NON è stato verificato:
- Che Android Chrome mostri effettivamente "Keeppix" nel foglio di
  condivisione dopo l'installazione della PWA (richiede un manifest valido
  raggiunto e un service worker registrato con successo — verificato solo
  che i file esistano e siano sintatticamente corretti, non il comportamento
  runtime del browser).
- Che il `POST multipart/form-data` reale mandato dall'OS abbia la stessa
  forma di `FormData` assunta dal service worker (`formData.getAll('files')`
  con `File` istanze) — è la forma documentata da Chrome/web.dev per Share
  Target Level 2, ma non testata con un payload reale.
- Il comportamento su iOS, dove il supporto a Web Share Target è
  storicamente più limitato o assente (Safari/WebKit): la spec lo segnala
  come "da verificare", non garantito.

Raccomando che la prima verifica su device reale (quando disponibile)
controlli, in ordine: (1) l'app è installabile come PWA e compare "Keeppix"
nel menu Condividi di Android dopo l'installazione; (2) condividere una
foto porta a `/share-target` e la SPA si carica senza errori in console;
(3) il file condiviso appare nel pannello di upload come "In coda" — a quel
punto, dato il gap descritto sotto, l'upload non partirà da solo, il che è
atteso con questo commit e non un bug della Share Target in sé.

## Concern aperto: nessuna scelta della cartella di destinazione

Il brief mostra `addSharedFiles` come `addFiles(files, null)` con il
commento "null = richiede scelta all'utente, come gli upload normali". Ho
verificato che **questo meccanismo non esiste nel codebase**: `addFiles`
dal Task 3 richiede sempre un `folderId: string`, e nessuna vista in
produzione lo chiama ancora (solo i test, con un id fisso). Costruire un
selettore di cartella reale sarebbe stato fuori scope per il Task 10 (è una
feature mancante del Task 3 / della spec §4.1, non della Share Target).

Ho scelto la decisione più piccola che rispetta la lettera del brief e non
rompe nulla: il tipo `targetFolderId` accetta `null`, la sessione condivisa
viene accodata come "queued" (soddisfa il test richiesto e rende visibile
il file nel pannello), ma `pump()` non avvia mai una sessione senza
cartella. **Risultato onesto: con questo commit, condividere una foto dalla
galleria la mette in coda nel pannello di Keeppix, ma non la carica**, finché
un task futuro non aggiunge un modo per assegnare la cartella (sia per gli
upload condivisi sia per quelli "normali", che hanno lo stesso problema
strutturale). Dettagli e alternative scartate nel ledger di fase.

## Verifica finale — comandi eseguiti

```bash
cd /workspace/frontend && npm run build && npm run test && npx vue-tsc --noEmit
```

Tutti verdi (vedi sopra). `npm run lint` verde (0 errori) eseguito in più.
