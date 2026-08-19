# Task 3 — Report: pannello di upload persistente (frontend)

## Stato

DONE_WITH_CONCERNS — implementazione completa, tutte le verifiche richieste
sono verdi, ma segnalo alcune decisioni con costo non nullo se sbagliate
(vedi "Ruling e concern" sotto).

## Commit

Un solo commit sul branch `fase-5`:

```
feat(frontend): persistent resumable upload panel
```

Lo sha esatto è nell'output di `git log -1` dopo il commit (non
autoreferenziabile da questo file, che è incluso nello stesso commit).

## File creati/modificati

- CREATE `frontend/src/api/upload.ts` — client HTTP per le rotte tus
  (`checkHashes`, `createSession`, `headSession`, `patchChunk`, `hashFile`,
  `hashBytes`).
- CREATE `frontend/src/stores/upload.ts` — store Pinia: sessioni in memoria +
  persistenza in `localStorage`, `initFromStorage`, `addFiles`, concorrenza
  massima 3, chunk adattivi 1–8 MiB, `pause`/`resume`/`retry`/
  `removeCompleted`.
- CREATE `frontend/src/components/UploadPanel.vue` — overlay globale
  (barra in basso a destra, minimizzabile, una riga per sessione con icona
  di stato, barra di progresso, azioni riprova/pausa/riprendi).
- CREATE `frontend/src/components/UploadPanel.spec.ts` — 4 test Vitest
  richiesti dal brief.
- MODIFY `frontend/src/i18n/en.json`, `frontend/src/i18n/it.json` — chiave
  `upload.*` (titolo, stati, collisioni, errori, azioni), stesse chiavi
  nelle due lingue (verificato dal test esistente `i18n.spec.ts`).
- MODIFY `frontend/src/App.vue` — montato `UploadPanel` come secondo
  componente dopo `<RouterView>`, via `defineAsyncComponent` (import
  dinamico, non nel bundle iniziale).
- MODIFY `frontend/package.json`, `frontend/package-lock.json` — nuova
  dipendenza `hash-wasm@^4.12.0` (blake3 lato client; motivazione nel
  ledger).

## TDD — self-review

Sì, ho visto fallire i 4 test per il motivo giusto prima di scrivere
l'implementazione reale: ho scritto `api/upload.ts` e uno **stub** di
`stores/upload.ts` (stesse firme esportate, corpo vuoto/no-op), eseguito
`npx vitest run src/components/UploadPanel.spec.ts` e osservato:

- `pre_check_skips_files_already_in_library` → fallito: `queued` restava `[]`
  (lo store non chiamava `addFiles` per davvero).
- `resumes_session_from_localstorage_on_init` → fallito:
  `uploadApi.headSession` non veniva mai chiamato (0 chiamate).
- `marks_session_gone_when_head_returns_410` → fallito: nessuna sessione
  con `status === 'error'` (lo store non leggeva `localStorage`).
- `two_uploads_run_concurrently_up_to_three` → fallito: 0 sessioni
  `uploading` invece di 3.

Poi ho ripristinato l'implementazione reale e rieseguito: 4/4 verdi.

## Output di verifica finale

`npm run test` (intera suite, non solo il file nuovo):

```
Test Files  23 passed (23)
     Tests  88 passed (88)
```

`npx vue-tsc --noEmit`: nessun output, exit 0 — pulito.

Eseguito anche, non richiesto esplicitamente ma utile prima di dichiarare
fatto:
- `npm run lint`: 0 errori. 9 warning pre-esistenti in `SharesView.vue`
  (`vue/html-indent`), non toccato da questo task — verificato con
  `git status --short` che il file non è nel diff.
- `npm run build` (`vue-tsc -b && vite build`): completa con successo.
  `UploadPanel` produce un chunk lazy separato
  (`dist/assets/UploadPanel-*.js`, ~8 KB / ~3 KB gzip) che contiene anche il
  codice blake3 di `hash-wasm`; verificato con `grep` che la stringa
  `blake3` non compare nei chunk iniziali (`index-*.js`, `client-*.js`).

## Ruling e decisioni prese (vedi anche il ledger di fase)

1. **Nuova dipendenza `hash-wasm`.** Non c'era alcuna libreria blake3 nel
   frontend e `crypto.subtle` del browser non implementa blake3, ma il
   protocollo lo richiede sia per il pre-check sia per
   `Upload-Checksum: blake3 <hex>`. Ho scelto `hash-wasm` (MIT, WASM,
   9 KB gzip solo per il modulo BLAKE3, ampiamente usata) e l'ho importata
   dinamicamente per non gravare sul bundle iniziale. **Concern**: è
   comunque una dipendenza nuova non nell'elenco esplicito del brief — la
   segnalo qui invece di deciderla in silenzio.
2. **Avvio dell'upload differito con `setTimeout(0)`** (`schedulePump`),
   non sincrono dentro `addFiles`/`resume`. Necessario per rendere
   osservabile lo stato "queued" subito dopo `addFiles` nel test 1, prima
   che la concorrenza massima 3 faccia scattare "uploading". Comportamento
   coerente e testato dal test 4, ma è una scelta di implementazione non
   dettata letteralmente dal brief.
3. **Sessioni riprese da `localStorage` senza il `File` originale**
   (refresh di pagina): vengono marcate "paused" con l'offset reale letto
   da `HEAD`, ma `resume()` su una di queste imposta l'errore
   `upload.errors.missingFile` invece di riprendere l'invio, perché il
   browser non mantiene l'handle del file tra un refresh e l'altro senza
   File System Access API (fuori budget per questo task). Non violare i 4
   test richiesti, ma è un limite reale dell'esperienza utente: dopo un
   refresh a metà upload, l'utente deve riselezionare il file per
   riprendere. **Non ho costruito l'interfaccia per quella riselezione** —
   non è nello scope dei 4 test né del brief.
4. **`UploadPanel.spec.ts` testa lo store, non monta il componente Vue.**
   I 4 nomi/descrizioni di test nel brief riguardano tutti la logica dello
   store (`addFiles`, `initFromStorage`, concorrenza); ho seguito quella
   descrizione letteralmente invece di forzare un mount del componente
   Vue solo per rispettare il nome del file. Il componente stesso non ha
   test di rendering dedicati in questo task.
5. **Icone di stato come glyph Unicode** (⏳ ↑ ⏸ ✓ ✕) invece di un'icon
   library: nessuna era già presente nelle dipendenze e non ne ho aggiunta
   una per questo. Sono decorative (`aria-hidden="true"`); il testo di
   stato accessibile viene sempre da `t('upload.status.*')`.
