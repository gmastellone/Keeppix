# Task 12 — Frontend — Review

Diff esaminato: `review-ec799d1..248f4c8.diff` (commit unico `248f4c8`, 30 file, 6504
inserzioni, tutto sotto `frontend/`). Confrontato con il piano (16 step
verbatim), il preflight (`task-12-preflight.md`, note N1–N7) e il report
dell'implementer.

## Spec Compliance

✅ **Spec compliant.** Tutti i file elencati nel brief sono stati creati; le
interfacce prodotte (`apiFetch<T>`, `ApiProblem`, `useSessionStore()` con
`user`/`initialised`/`bootstrap`/`login`/`setup`/`logout`, le rotte
`/setup` `/login` `/`) corrispondono a quanto richiesto. Endpoint consumati
verificati contro il backend reale (`crates/keeppix-api/src/routes/{setup,auth}.rs`,
`tests/openapi.rs:60-65`): i path e i quattro codici `type` RFC 9457 usati in
`SetupView.vue:602-604`/`LoginView.vue:546` (`keeppix/invalid-username`,
`keeppix/invalid-password`, `keeppix/already-initialised`,
`keeppix/invalid-credentials`) coincidono esattamente con quelli emessi da
`routes/setup.rs:76-89` e `tests/auth.rs:98,115,151,171`.

Le tre deviazioni "obbligate" dichiarate nel report sono state verificate
in prima persona, non prese per buone:

- **Collisione `it` in `i18n.spec.ts`**: riprodotta. Ho copiato il codice
  verbatim del piano (`import it from './it.json'` insieme a `it` di vitest)
  in un file di test e fatto girare `npx vitest run`: fallisce con
  `[PARSE_ERROR] Identifier 'it' has already been declared`, esattamente
  come riportato. Nessuna via alternativa se non rinominare uno dei due
  binding — il fix con alias è corretto e minimo.
- **`erasableSyntaxOnly` e `ApiProblem`**: riprodotto. Con le
  proprietà-parametro verbatim del piano, `npx vue-tsc -b --force` fallisce
  con `TS1294` su tutti e quattro i parametri. **Ma** ho anche verificato
  l'alternativa che il report non menziona: impostare
  `"erasableSyntaxOnly": false` in `tsconfig.app.json` (opzione introdotta
  dallo scaffold, non richiesta né dal piano né dallo spec) fa compilare il
  codice verbatim del piano senza errori. La deviazione quindi non era
  l'unica via — disattivare il flag sarebbe stato un fix altrettanto valido
  e meno invasivo. Non è un difetto: la scelta di riscrivere la classe è
  difendibile (mantiene lo scaffold più severo, comportamento pubblico
  identico, ben commentata), ma il report la presenta come "obbligata /
  non stilistica" quando in realtà era una scelta fra due strade
  ugualmente legittime. Vedi Minor sotto.
- **`src/api/auth.ts` non specificato**: confermato control-flow — nessuno
  dei 16 step del piano mostra il contenuto di questo file pur essendo
  nell'elenco file. La lettura data dall'implementer (layer DTO/wrapper
  attorno ad `apiFetch`, poi consumato da `stores/session.ts`) è ragionevole
  e non introduce comportamento nuovo (`session.ts:1-19` delega a
  `authApi.*` con la stessa interfaccia pubblica del piano).

⚠️ **Cannot verify from diff alone**: lo step 15 (prova a mano end-to-end)
dipende dal comportamento di `crates/keeppix-api`, fuori da questo diff. Il
difetto Critical riportato (cookie `__Host-` scartato su HTTP semplice
perché omette `Secure`) è coerente con quanto ho letto in
`crates/keeppix-api/src/cookie.rs` nella working tree corrente e con la
mia lettura della RFC 6265bis §4.1.3.2 — ma non l'ho ri-verificato con un
proprio giro di browser, e non è comunque nel perimetro di questo task
(non tocca `crates/`). Confermo *en passant* soltanto che la spiegazione
tecnica nel report è plausibile e che il codice frontend (`apiFetch` con
`credentials: 'same-origin'`, nessuna lettura di cookie lato client) non è
la causa.

## Nota operativa (non un finding sul diff)

Al momento della review, `git status` sulla working tree mostra modifiche
**non committate** a `crates/keeppix-api/{cookie.rs, routes/auth.rs,
routes/setup.rs, tests/auth.rs}` — quasi certamente il lavoro in corso su
un fix separato per il difetto `__Host-`/`Secure` segnalato dal report.
Non le ho toccate, non le ho ispezionate oltre un `git diff --stat`, e non
ho fatto girare `cargo test --workspace` contro questo stato perché
non rappresenterebbe il commit sotto review (`248f4c8`) né lo stato
descritto come "albero pulito" nel dispatch. Il report dell'implementer
documenta 91/91 test verdi con `crates/` non modificato al momento del
commit; non ho trovato motivo per dubitarne (nessuna modifica di questo
diff tocca `crates/` o file di root — confermato con
`git diff ec799d1 248f4c8 -- .gitignore`, nessuna differenza: il file
toccato è solo `frontend/.gitignore`, nuovo). Il controller dovrebbe
verificare la suite Rust quando l'albero tornerà pulito.

## Verifiche eseguite personalmente

```
$ npm ci                     → 351 pacchetti, nessun errore (solo warning glob@10 deprecato, pre-esistente)
$ npx vitest run              → Test Files 2 passed (2), Tests 6 passed (6)
$ npx vue-tsc --noEmit        → nessun errore
$ npm run lint                → nessun errore/warning
$ npm run build                → build ok, vedi output sotto
$ find dist/assets -name '*.js' -exec gzip -c {} \; | wc -c   → 76672
```

Output build:
```
dist/assets/index-*.css       9.51 kB │ gzip:  2.71 kB
dist/assets/Button-*.js       0.52 kB │ gzip:  0.33 kB
dist/assets/HomeView-*.js     0.61 kB │ gzip:  0.42 kB
dist/assets/TextField-*.js    1.19 kB │ gzip:  0.65 kB
dist/assets/LoginView-*.js    1.39 kB │ gzip:  0.77 kB
dist/assets/SetupView-*.js    2.13 kB │ gzip:  0.95 kB
dist/assets/index-*.js      202.19 kB │ gzip: 74.42 kB
```

**Budget di bundle confermato indipendentemente: 76 672 byte gzip**,
identico byte per byte al numero dichiarato nel report, ben sotto i
153 600 (150 KB) del vincolo globale.

**Test sulle traduzioni genuinamente pinnato (N4)**: ho tolto
`home.logout` da `en.json`, rilanciato `npx vitest run
src/i18n/i18n.spec.ts` → `1 failed | 1 passed (2)`, diff dell'assertion
mostra `+ "home.logout"` mancante. Ripristinato il file, ririlanciato →
`2 passed`. Confermo che il test non è un placebo.

**Verifica RFC 9457 in `apiFetch` (`client.ts:20-42`)**:
- 204 → `null` (riga 26-28, coperto da test): OK.
- `content-type: application/problem+json` → `ApiProblem` coi campi del
  corpo (righe 31-34): OK, coperto da test.
- Corpo non `problem+json` (es. testo semplice o JSON generico su errore)
  → `ApiProblem('keeppix/unexpected', response.statusText, response.status)`
  senza tentare `response.json()` sul corpo non conforme, evitando
  un'eccezione di parsing secondaria (riga 35): comportamento corretto e
  coperto da test (`client.spec.ts:35-41`, risposta 502 testo semplice).
- Nessun path lascia una risposta non-ok "silenziosa": ogni ramo
  `!response.ok` termina in un `throw`.

**`useSessionStore` non legge mai il cookie**: verificato che l'unica fonte
di verità per "sono autenticato" è la risposta di `/api/v1/auth/me`
(`stores/session.ts:16-22` via `authApi.me()`), con gestione esplicita del
401 come "nessuna sessione" (non un errore). Nessun accesso a
`document.cookie` in tutto `frontend/src` (verificato per lettura completa
del diff, nessun'occorrenza). Il router (`router.ts:14-16`) usa lo stesso
stato derivato, mai un'assunzione propria.

**CSP**: nessuna chiamata di rete verso host esterni, nessuno script
inline, nessun `eval`, nessuno stile inline oltre alle classi Tailwind
(compilate in `dist/assets/index-*.css`, servite same-origin).
`dist/index.html` generato non contiene `<script>` inline né `<style>`
inline. `public/favicon.svg` è un asset locale con `style="fill:..."` solo
*dentro* il file SVG stesso (non nel documento HTML), irrilevante per lo
`style-src` della pagina. Compatibile con
`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'`.

**`reka-ui`**: installato (richiesto dal piano step 1) ma zero import in
`src/` — verificato con grep, nessuna occorrenza. Tree-shaken, coerente col
report.

## Strengths

- Le tre deviazioni obbligate sono realmente obbligate (verificate a mano,
  non solo dichiarate) e documentate con commenti nel codice, non solo nel
  report — buona pratica.
- TDD rispettato con evidenza reale: fase RED mostrata con l'errore
  effettivo (incluso quello imprevisto sull'identificatore `it`), fase
  GREEN, e un test di "rottura mirata" per N4 con output riprodotto qui
  sopra in modo identico.
- `apiFetch` gestisce tutti e quattro i casi RFC 9457 richiesti (200, 204,
  problem+json, fallback) senza doppio parsing né eccezioni non gestite.
- Nessuna lettura di cookie lato client: lo stato di sessione deriva
  sempre e solo da `/auth/me`, coerente col vincolo `HttpOnly`.
- Bundle a metà budget (76 672 / 153 600 byte), verificato byte-per-byte.
- Confini rispettati: nessun tocco a `crates/`, nessuna dipendenza di rete
  di terze parti, `package-lock.json` committato, `dist/` e `node_modules`
  ignorati.
- La prova a mano dello step 15 è stata eseguita con un vero browser
  (Chromium via Playwright, installato fuori da `frontend/`) invece di
  dichiarata per sentito dire, e ha prodotto una diagnosi precisa di un
  difetto reale fuori perimetro — esattamente il comportamento richiesto
  dalla nota N6 del preflight quando il flusso non è completamente
  verificabile.

## Issues

### Critical (Must Fix)
Nessuno nel codice sotto review.

### Important (Should Fix)

- **`HomeView.vue:11-14` — nessuna gestione d'errore su `signOut()`,
  plan-mandated.** `session.logout()` non è avvolto in `try/catch`: se la
  chiamata fallisce (rete, 500, sessione già scaduta), la funzione async
  genera una promise rejection non gestita, nessun messaggio compare
  all'utente (nemmeno un `console.error`) e il pulsante "Sign out" appare
  semplicemente non fare nulla. Le istruzioni di dispatch chiedevano
  esplicitamente di controllare "gestione degli errori visibile
  all'utente, non solo console.error" per le tre viste: `LoginView` e
  `SetupView` la rispettano, `HomeView` no. Il codice è verbatim dallo
  step 11 del piano, quindi per la regola di calibrazione va segnalato
  come Important nonostante l'origine — l'autorialità del piano non lo
  assolve. Fix minimo: avvolgere in `try/catch`, mostrare un
  `<Alert>`/messaggio anche qui in caso di fallimento (pattern già
  presente nelle altre due viste).

### Minor (Nice to Have)

- **`client.ts:1-18`, riscrittura di `ApiProblem` presentata come
  "obbligata"** (report, scostamento 2) quando in realtà esisteva
  un'alternativa altrettanto valida e più piccola: disattivare
  `erasableSyntaxOnly` in `tsconfig.app.json` (opzione dello scaffold, non
  richiesta da piano o spec) avrebbe permesso il codice verbatim del piano
  senza modifiche. Verificato di persona: con il flag a `false` il
  costruttore a proprietà-parametro compila pulito via `vue-tsc -b
  --force`. Non è un difetto nel codice consegnato — la soluzione scelta è
  difendibile e ben commentata — ma la caratterizzazione nel report andrebbe
  corretta da "obbligato" a "scelto fra due alternative valide", per non
  fuorviare chi lo userà come precedente sui prossimi task.
- **Nessun test unitario per `router.ts` o `stores/session.ts`.** Il piano
  richiede esplicitamente solo `client.spec.ts` e `i18n.spec.ts` allo step
  4, quindi non è una violazione di spec, ma la logica di redirect in
  `router.ts:9-26` (4 rami condizionali sull'istanza/sessione) e il flusso
  di `bootstrap()` in `stores/session.ts` restano scoperti da test
  automatici — un candidato naturale per rafforzare la copertura in un
  task successivo.
- **`index.html:2` `lang="en"` hardcoded** nel sorgente statico; viene
  sovrascritto a runtime da `main.ts:9` (`document.documentElement.lang =
  detectLocale()`), quindi nessun impatto pratico, ma un crawler o uno
  screen reader che leggesse l'HTML pre-idratazione (prima che JS giri)
  vedrebbe sempre "en". Impatto trascurabile per una SPA client-side-only
  come questa.

## Assessment

**Task quality:** Approved.

**Reasoning:** L'implementazione rispetta l'interfaccia richiesta dal
brief, le tre deviazioni dal piano sono realmente necessarie (verificate
in prima persona, non solo lette nel report) e ben documentate nel codice,
il budget di bundle e il test-pinning delle traduzioni sono stati
riconfermati byte-per-byte/riga-per-riga in modo indipendente, e la
gestione di `apiFetch`/sessione/CSP è corretta su tutti i punti sensibili
richiesti dal dispatch. L'unico problema che tocca comportamento reale
(mancata gestione errori in `HomeView.signOut`) è di severità contenuta e
isolato a un solo file; non blocca l'integrazione ma va corretto, insieme
alla piccola imprecisione nel report sulla natura "obbligata" della
riscrittura di `ApiProblem`.
