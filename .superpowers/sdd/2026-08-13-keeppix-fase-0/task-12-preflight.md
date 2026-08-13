# Task 12 — Frontend

**Gli step verbatim sono nel piano**, non in questo brief: apri
`docs/superpowers/plans/2026-08-13-keeppix-fase-0.md` e leggi la sezione
`## Task 12: Frontend` (riga 3951, fino a riga 4732 esclusa). Sono 16 step con
il codice di ogni file. Questo brief contiene solo ciò che il piano **non** dice
e che è vincolante: le note di pre-volo del controller, i ruling già presi che
toccano il task, e il protocollo di verifica.

Il brief è volutamente sottile perché la sezione del piano è lunga e
autosufficiente. Se qualcosa nel piano contraddice una nota qui sotto, **vince
la nota**, e la contraddizione va segnalata nel report.

## Sintesi

- **Files:** tutto sotto `frontend/`. Vedi l'elenco in testa alla sezione del
  piano. Non toccare nulla in `crates/`.
- **Consuma:** gli endpoint del Task 10 (`/api/v1/setup/status`, `/api/v1/setup`,
  `/api/v1/auth/login|refresh|logout|me`) e il documento OpenAPI del Task 11.
- **Produce:** `apiFetch<T>()` che lancia `ApiProblem { type, title, status, detail? }`;
  `useSessionStore()` (Pinia) con `user`, `initialised`, `bootstrap()`,
  `login()`, `setup()`, `logout()`; le rotte `/setup`, `/login`, `/` (protetta).

## Note di pre-volo (vincolanti)

### N1 — Ambiente: npm funziona, Docker no

`registry.npmjs.org` è nella lista `noProxy` di questo ambiente, quindi
`npm install` e `npm create vite@latest` funzionano normalmente. Node è la 22
(`/opt/node22/bin/node`).

Il pull di immagini Docker invece è **bloccato dalla policy di egress**: non è
un problema transitorio, non riprovare. Non ti riguarda direttamente — questo
task non usa container — ma se un pacchetto tentasse di scaricare un browser
(Playwright e simili) fallirebbe: i test di questo task sono `vitest` + `jsdom`,
che non scaricano nulla.

### N2 — Lo scaffold di Vite non è il contratto

Lo step 1 usa `npm create vite@latest frontend -- --template vue-ts`, che genera
una struttura di partenza decisa dalla versione corrente del template, non dal
piano. **L'elenco di file autorevole è quello in testa alla sezione del piano.**
Se lo scaffold produce file che il piano non prevede (per esempio
`src/components/HelloWorld.vue`, `src/assets/`, `public/vite.svg`), rimuovili
invece di lasciarli in giro; se non produce un file che il piano dà per
esistente, crealo. Verifica anche che il comando non resti in attesa di input
interattivo: usa i flag non interattivi se serve.

Annota nel report la versione effettiva di Vite, Vue e Tailwind che ti sei
trovato, perché il piano è stato scritto contro Tailwind v4 e vue-i18n 11: se
una major è cambiata sotto, la configurazione dello step 3 va adattata e la cosa
va detta esplicitamente, non aggirata in silenzio.

### N3 — `frontend/dist` è ignorato da git, ed è voluto

`.gitignore` contiene già `frontend/dist` e `node_modules`. Il Task 13
incorporerà `frontend/dist` nel binario con `rust-embed`, costruendola al
momento. Non committare la build. **Committa invece `package-lock.json`**: il
Dockerfile del Task 14 lo copia, e senza lockfile la build dell'immagine non è
riproducibile.

### N4 — I test devono provare qualcosa

Lo step 4 prescrive i test di `src/api/client.spec.ts` e `src/i18n/i18n.spec.ts`
e lo step 5 chiede di vederli fallire prima di implementare. Rispetta il ciclo e
riporta nel report l'output reale del fallimento iniziale.

Avvertenza che viene dai dieci task precedenti: in questa fase più di un test
scritto seguendo il piano alla lettera passava senza provare ciò che il suo nome
affermava. Per ogni asserzione chiediti che cosa dovrebbe rompersi perché
diventi rossa. In particolare, per il test sulle traduzioni: un test che
confronta le chiavi di `it.json` e `en.json` deve fallire davvero se una chiave
manca da un lato solo — provalo togliendone una e rimettila.

### N5 — Il budget di bundle è un requisito, non un consiglio

Lo step 14 verifica che il bundle stia sotto **150 KB gzip**. Se lo sfori, non
alzare il limite: riporta il numero reale nel report e di' quali dipendenze lo
gonfiano. È un dato che serve al Task 15, che lo trasformerà in un controllo di
CI.

### N6 — Lo step 15 chiede una prova a mano del flusso completo

Serve il backend acceso. Il database c'è già: PostgreSQL 16 locale, e il server
si avvia con

```bash
export DATABASE_URL="postgres://keeppix:keeppix@127.0.0.1:5432/postgres"
cargo run -p keeppix-server -- serve
```

(controlla il nome esatto del sottocomando con `cargo run -p keeppix-server -- --help`).
Il proxy di Vite inoltra `/api` e `/health` a `127.0.0.1:5673`. Se preferisci
puoi usare un database dedicato invece di `postgres`: crealo con
`psql -h 127.0.0.1 -U keeppix -c 'CREATE DATABASE keeppix_dev'`.

Se il flusso a mano non è eseguibile per qualche ragione, **dillo nel report**
invece di dichiararlo fatto: è la differenza fra una verifica e un'affermazione.

### N7 — Confini

- Non toccare `crates/`, `docs/`, `.superpowers/`, `Cargo.toml`, `Cargo.lock`.
- Non aggiungere dipendenze oltre a quelle degli step 1 e 3 senza motivarlo nel
  report.
- Non introdurre chiamate di rete a terze parti (font, CDN, analytics): la CSP
  del backend è `default-src 'self'` e le bloccherebbe comunque, ma il punto è
  che l'istanza deve funzionare offline.

## Verifica finale attesa

```
cd frontend
npm run test        # o `npx vitest run`, secondo gli script che definisci
npx vue-tsc --noEmit
npm run lint
npm run build
```

più il budget di bundle dello step 14 e la prova a mano dello step 15. Riporta
l'output reale di ciascuno.

Verifica anche che **la suite Rust resti verde**, perché il task tocca
`.gitignore` e la radice del repository:

```bash
export KEEPPIX_TEST_DATABASE_URL="postgres://keeppix:keeppix@127.0.0.1:5432/postgres"
cargo test --workspace -- --test-threads=1
```

`--test-threads=1` serve ai test di `keeppix-server/tests/config.rs`, che
manipolano l'ambiente di processo: è un vincolo pre-esistente.

## Commit

Sul branch `fase-0`, con il messaggio previsto dallo step 16 del piano, più in
coda:

```
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01S1JPfGqKvQQG9FkLX3pnCi
```

Niente `git push`. Albero pulito alla fine.

## Report

In `.superpowers/sdd/2026-08-13-keeppix-fase-0/task-12-report.md`, con: le
scelte non ovvie, gli scostamenti dal piano e perché, l'output reale delle
verifiche, il numero del budget di bundle, e i difetti notati ma deliberatamente
non corretti (finiranno nel ledger come Minor differiti).
