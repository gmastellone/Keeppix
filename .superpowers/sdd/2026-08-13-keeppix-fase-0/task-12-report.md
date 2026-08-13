# Task 12 — Frontend — Report

Stato: **DONE_WITH_CONCERNS**. Tutto il codice richiesto è implementato e
verificato; la preoccupazione principale non è nel codice di questo task ma
in un difetto del backend (`crates/keeppix-api`, fuori dal mio perimetro)
scoperto durante la prova a mano dello step 15 — vedi § "Difetto scoperto,
non corretto" più sotto, con evidenza riprodotta con un vero browser
Chromium.

Questo report copre anche la ripresa dopo un'interruzione per limite di
sessione API a metà lavoro: lo stato su disco al momento della ripresa era
quello descritto dal coordinatore (scaffold pulito, `vite.config.ts` e
`style.css` a posto, i due file di test scritti verbatim, `i18n.spec.ts` già
con l'alias `itMessages`/`enMessages` corretto). Ho proseguito da lì.

## Versioni effettive (nota N2 del preflight)

Il piano è stato scritto contro Tailwind v4 e vue-i18n 11. Con
`npm create vite@latest` oggi si è ottenuto:

| Pacchetto | Atteso dal piano | Risolto |
|---|---|---|
| vite | non specificato, ma "vite" generico | **8.2.1** |
| vue | 3.x | 3.5.41 |
| vue-router | non specificato, tipicamente 4.x per Vue 3 | **5.2.0** |
| pinia | non specificato, tipicamente 2.x | **4.0.3** |
| vue-i18n | 11 (atteso) | 11.4.8 (confermato) |
| tailwindcss | v4 (atteso) | 4.3.3 (confermato) |
| typescript | non specificato | **6.0.3** |
| vue-tsc | non specificato | 3.3.9 |
| eslint | non specificato | 10.8.1 |

`vue-router` 5 e `pinia` 4 sono major più recenti di quanto io mi aspettassi
(il preflight avvisava solo su Tailwind e vue-i18n). Ho verificato che la
superficie API usata dal piano (`createRouter`, `createWebHistory`,
`useRouter`, `useRoute`, `defineStore`, `createPinia`) esiste identica in
queste major, e il codice del piano funziona verbatim con esse — nessun
adattamento richiesto oltre a quanto documentato sotto.

Due impostazioni del tsconfig generato dal template attuale HANNO richiesto
un adattamento del codice verbatim del piano (dettagliato sotto): la
deprecazione di `baseUrl` in TypeScript 6.0 (`TS5101`, errore in build mode)
e il flag `erasableSyntaxOnly` (introdotto in TS 5.8, abilitato di default
dal template attuale) che vieta le proprietà-parametro nei costruttori.

## Scostamenti dal piano (obbligati, non stilistici)

### 1. Collisione di identificatori in `i18n.spec.ts`

Il codice verbatim del piano importa `it` sia come funzione di test da
`vitest` sia come default export di `./it.json` nello stesso scope di
modulo — è un errore di parse (`Identifier 'it' has already been declared`),
non un problema di risoluzione moduli. L'ho verificato nella fase RED (vedi
sotto): `oxc` (il parser di Vite/Vitest) rifiuta il file prima ancora di
provare a risolvere `./it.json`.

Fix: alias `enMessages`/`itMessages` sugli import dei JSON, con un
commento che spiega il perché. Tutte le asserzioni e i dati di test restano
identici — è solo un cambio di nome di binding, non di comportamento.

### 2. `erasableSyntaxOnly` e proprietà-parametro in `ApiProblem`

Il tsconfig generato dallo scaffold attuale (`tsconfig.app.json`) include
`"erasableSyntaxOnly": true`, che vieta sintassi TS che richiede codice
emesso a runtime — incluse le proprietà-parametro nei costruttori
(`constructor(readonly type: string, ...)`), usate verbatim dal piano nella
classe `ApiProblem`. `vue-tsc -b` fallisce con `TS1294` su ognuno dei quattro
parametri.

Fix: dichiarazione esplicita dei campi (`readonly type: string` fuori dal
costruttore) e assegnazione nel corpo del costruttore. Stessa forma
pubblica, stesso comportamento, stessi nomi di campo — cambia solo dove la
riga di dichiarazione vive.

### 3. `baseUrl` deprecato in TS 6.0

Ho aggiunto `"baseUrl": "."` insieme a `"paths": {"@/*": ["./src/*"]}` in
`tsconfig.app.json` per far risolvere a `vue-tsc` lo stesso alias `@/` che
Vite risolve via `vite.config.ts`. TypeScript 6.0 marca `baseUrl` come
deprecato (`TS5101`, errore in build mode `-b`). Rimosso `baseUrl`: da TS 5+
`paths` senza `baseUrl` si risolve relativo alla cartella del tsconfig, e
questo è sufficiente per l'alias usato (`@/*` → `./src/*`).

### 4. `frontend/src/api/auth.ts`: contenuto non specificato dal piano

L'elenco dei file in testa alla sezione del piano include
`frontend/src/api/auth.ts` fra i file da creare, ma **nessuno dei 16 step
ne mostra il contenuto** — lo step 9 (`stores/session.ts`) chiama
`apiFetch` direttamente sugli endpoint di auth, senza passare da un modulo
`auth.ts` intermedio. È un'incongruenza fra l'elenco file (autorevole per
preflight N2) e il codice degli step.

Ho risolto creando `api/auth.ts` come sottile strato di wrapper tipati
attorno ad `apiFetch` per i cinque endpoint consumati
(`getSetupStatus`, `setupAccount`, `login`, `me`, `logout`), con
l'interfaccia `User` e il tipo `SetupPayload` spostati lì (lo store li
re-esporta per compatibilità con l'interfaccia dichiarata dal piano). Ho
rifattorizzato `stores/session.ts` per usare questo strato invece di
chiamare `apiFetch` inline. Non introduce comportamento nuovo, nessun test è
toccato da questa scelta (nessuno step 4/5 la esercita direttamente), ed è
la lettura più naturale del file-list del piano: l'API layer definisce i
DTO, lo store li consuma.

### 5. Commento sull'header `x-keeppix-client` corretto (ruling del controller)

Il commento verbatim del piano su `apiFetch` afferma che "il backend
richiede" l'header sulle mutazioni. Come da ruling ricevuto: il backend non
lo verifica ancora oggi (l'enforcement arriva in una fix wave successiva del
branch). Ho riscritto il commento per descrivere lo stato reale — l'header è
metà di una difesa CSRF la cui altra metà (verifica server-side) non è
ancora stata implementata — mantenendo l'invio dell'header come richiesto.

### 6. `eslint.config.js` non previsto da nessuno step

Il piano installa `eslint`, `eslint-plugin-vue`,
`@vue/eslint-config-typescript` allo step 1 e il preflight richiede
`npm run lint` nella verifica finale, ma nessuno step crea una config: il
template `vue-ts` di `create-vite` non ne genera una di default (a
differenza del template Vue puro, che offre l'opzione ESLint interattiva).
Ho scritto una config flat minima (`vue.configs['flat/recommended']` +
`vueTsConfigs.recommended` da `@vue/eslint-config-typescript` v14, che
supporta nativamente ESLint 10 flat config), con un'eccezione mirata su
`vue/multi-word-component-names` per `components/ui/**` — i nomi
`Button.vue`/`Alert.vue` sono prescritti verbatim dal piano e sono
componenti-primitivo, non pagine, per cui il nome a una parola è la
convenzione comune per questa categoria di file.

Ho anche fatto girare `eslint --fix` una volta sui file generati dagli step
10-11: ha solo riformattato attributi multi-riga e newline dentro i tag
(nessuna modifica di logica); ho poi ri-verificato `vitest` e `vue-tsc`
dopo il fix per confermare che il comportamento non fosse cambiato.

## Ciclo TDD (step 4-5)

**RED** — `cd frontend && npx vitest run`, prima di scrivere `client.ts` e i
JSON di traduzione:

```
FAIL  src/api/client.spec.ts [ src/api/client.spec.ts ]
Error: Failed to resolve import "./client" from "src/api/client.spec.ts".
Does the file exist?

FAIL  src/i18n/i18n.spec.ts [ src/i18n/i18n.spec.ts ]
[PARSE_ERROR] Identifier `it` has already been declared
   1 │ import { describe, expect, it } from 'vitest'
                              ─┬
                               ╰── `it` has already been declared here
   4 │ import it from './it.json'
            ─┬
             ╰── It can not be redeclared here

Test Files  2 failed (2)
     Tests  no tests
```

`client.spec.ts` fallisce esattamente come previsto dal piano ("Cannot find
module"/import irrisolto). `i18n.spec.ts` fallisce per una ragione diversa
da quella che il piano si aspettava (parse error da identificatore duplicato,
non "modulo mancante") — è il difetto del piano descritto sopra, scoperto
proprio in questa fase RED. Ho corretto il test (alias) **prima** di
scrivere `en.json`/`it.json`, poi il fallimento è tornato ad essere quello
atteso ("modulo mancante"), quindi ho implementato.

**GREEN** — dopo `client.ts`, `en.json`, `it.json`, `i18n/index.ts`:

```
 Test Files  2 passed (2)
      Tests  6 passed (6)
```

**Verifica N4 (il test sulle traduzioni deve rompersi davvero)**: ho tolto
`home.logout` solo da `en.json` e rilanciato `npx vitest run
src/i18n/i18n.spec.ts`:

```
 FAIL  src/i18n/i18n.spec.ts > traduzioni > italiano e inglese hanno le stesse chiavi
AssertionError: expected [ 'app.name', 'common.loading', …(19) ] to deeply equal [ …(18) ]
+   "home.logout",
 Tests  1 failed | 1 passed (2)
```

Il test diventa rosso come atteso. Ho ripristinato `en.json` e riverificato
verde (`2 passed`).

## Verifica finale (comandi e output reali)

```
$ npx vue-tsc --noEmit
(nessun output — nessun errore)

$ npx vitest run
 Test Files  2 passed (2)
      Tests  6 passed (6)

$ npm run lint
> eslint . --max-warnings 0
(nessun output — nessun errore né warning)

$ npm run build
> vue-tsc -b && vite build
✓ 60 modules transformed.
dist/index.html                      0.45 kB │ gzip:  0.29 kB
dist/assets/index-*.css              9.51 kB │ gzip:  2.71 kB
dist/assets/Button-*.js              0.52 kB │ gzip:  0.33 kB
dist/assets/HomeView-*.js            0.61 kB │ gzip:  0.42 kB
dist/assets/TextField-*.js           1.19 kB │ gzip:  0.65 kB
dist/assets/LoginView-*.js           1.39 kB │ gzip:  0.77 kB
dist/assets/SetupView-*.js           2.13 kB │ gzip:  0.95 kB
dist/assets/index-*.js             202.19 kB │ gzip: 74.42 kB
✓ built in 534ms

$ find dist/assets -name '*.js' -exec gzip -c {} \; | wc -c
76672
```

**Budget di bundle: 76 672 byte gzip**, ben sotto il limite di 153 600 byte
(150 KB) — circa metà budget. Le viste sono già caricate con import
dinamici (dal router del piano), quindi la maggior parte del JS finisce
nel chunk `index` (Vue + Pinia + Vue Router + Vue I18n runtime): 74.42 KB
gzip. `reka-ui` è installato (richiesto dallo step 1) ma non è importato da
nessun file di questo task — zero impatto sul bundle, tree-shaken per
intero. Nessuna azione necessaria sul budget.

```
$ export KEEPPIX_TEST_DATABASE_URL="postgres://keeppix:keeppix@127.0.0.1:5432/postgres"
$ cargo test --workspace -- --test-threads=1
keeppix-api (unit)        4 passed
keeppix-api tests/auth    13 passed
keeppix-api tests/health   3 passed
keeppix-api tests/openapi  6 passed
keeppix-dav (unit)         0 passed
keeppix-db (unit)          0 passed
keeppix-db tests/migrations 7 passed
keeppix-db tests/sessions  14 passed
keeppix-db tests/settings   6 passed
keeppix-db tests/users     12 passed
keeppix-domain (unit)     22 passed
keeppix-jobs (unit)        0 passed
keeppix-media (unit)       0 passed
keeppix-server (unit)      0 passed
keeppix (bin, unit)        0 passed
keeppix-server tests/config 4 passed
(+ doc-tests di ogni crate: 0 passed, come atteso)
```

91 test totali, **0 falliti**, nessun errore di compilazione. La suite Rust
resta verde. Non ho modificato nulla in `crates/`, `docs/`,
`Cargo.toml`/`Cargo.lock`; `git status --short` sulla radice del repo
mostra solo il file di report `.superpowers/` di cui questo documento fa
parte, nessun'altra modifica fuori da `frontend/`.

Ho notato (non causato da me) **921 database `keeppix_test_*` orfani** sul
cluster Postgres locale, accumulati da esecuzioni precedenti della suite di
altri task. Non li ho ripuliti: è debito infrastrutturale pre-esistente,
fuori dal perimetro di questo task, e cancellare ~900 database non richiesti
mi è sembrato un rischio sproporzionato rispetto al beneficio per un task
che tocca solo `frontend/`. Lo segnalo qui perché la prossima persona che fa
girare la suite localmente lo veda.

## Prova a mano dello step 15 — eseguita, con un difetto scoperto

Ho avviato il backend (`cargo run -p keeppix-server -- serve`, sottocomando
confermato con `--help`) contro un database Postgres 16 locale dedicato
(`keeppix_dev`, creato e migrato apposta per non toccare dati di altri
task), e il frontend con `npm run dev` (proxy Vite verso `127.0.0.1:5673`
per `/api` e `/health`, come da `vite.config.ts`).

Non avendo un ambiente grafico, ho pilotato un vero browser **Chromium
headless** (preinstallato in questo ambiente sotto `/opt/pw-browsers`, non
scaricato) tramite Playwright — installato come tool di verifica
temporaneo in una directory di scratch **fuori da `frontend/`**, non come
dipendenza del progetto (nessuna riga aggiunta a `frontend/package.json`
per questo). Ho scelto questa via invece di curl grezzo perché il primo
tentativo con curl aveva mostrato lo stesso sintomo, ma volevo escludere che
fosse un artefatto del client (curl impone le sue proprie regole sui cookie
`Secure`, distinte da quelle di un browser reale) prima di scriverlo come
difetto genuino.

Flusso osservato (root `http://127.0.0.1:5173/`, istanza vergine):

1. **Redirect a `/setup`**: OK — `url=http://127.0.0.1:5173/setup`.
2. **Creazione admin → arrivo a `/` con saluto**: OK —
   `url=http://127.0.0.1:5173/ heading="Hello, Admin"`.
3. **Ricaricare → restare autenticati**: **FALLISCE**. Dopo il reload,
   redirect a `/login`. Il cookie store del browser risulta **vuoto**
   subito dopo il passo 2 (`context.cookies()` → `[]`), e la console del
   browser mostra `Failed to load resource: 401 (Unauthorized)` sulla
   chiamata a `/api/v1/auth/me` che il bootstrap del router fa al reload.
5. **Rientrare con le credenziali corrette**: OK, testato forzando la
   navigazione a `/login` dato che il passo 3 ha già interrotto la sessione
   — login riuscito, arrivo a `/` con il saluto.
4. **Uscire → tornare a `/login`**: OK, testato sulla sessione fresca
   ottenuta al passo 5 (il pulsante "Sign out" porta correttamente a
   `/login`).

### Causa: il cookie `__Host-kpx_session` non viene mai accettato su HTTP semplice

`crates/keeppix-api/src/cookie.rs` (`should_be_secure`) omette
deliberatamente l'attributo `Secure` quando l'header `Host` è
`localhost`/`127.0.0.1`/`[::1]`, con un commento che spiega l'intenzione:
permettere ai test (che parlano in chiaro su 127.0.0.1) di leggere il
cookie. Ma il prefisso `__Host-` richiede **letteralmente** l'attributo
`Secure` nell'header `Set-Cookie` per essere valido (RFC 6265bis §4.1.3.2),
indipendentemente dal fatto che la connessione sia effettivamente TLS o che
l'host sia loopback: l'eccezione "origine potenzialmente affidabile" che i
browser applicano a `localhost`/`127.0.0.1` rilassa *un'altra* regola (poter
*consegnare* un cookie `Secure` su una connessione non-TLS), non rende
opzionale la presenza letterale dell'attributo per il prefisso `__Host-`.
Omettendo `Secure` del tutto, il cookie fallisce la validazione del
prefisso e viene scartato per intero — da qualunque client conforme,
**browser reali inclusi**, non solo da curl.

Ho verificato empiricamente entrambe le metà di questa lettura con un
piccolo server HTTP di controllo (non incluso nel repo, solo per la
verifica):
- `Set-Cookie: __Host-...; HttpOnly; SameSite=Lax; Path=/` (senza `Secure`,
  esattamente come lo emette oggi il backend su 127.0.0.1) → scartato da
  curl.
- `Set-Cookie: __Host-...; HttpOnly; SameSite=Lax; Path=/; Secure` consegnato
  su HTTP semplice (senza TLS) → scartato anch'esso da curl, per la
  ragione distinta (il flag `Secure` richiede comunque un trasporto
  sicuro, e curl — a differenza dei browser — non ha alcuna eccezione per
  il loopback).

La prova decisiva è però quella col browser reale riportata sopra: il
cookie store di Chromium resta vuoto dopo la `POST /api/v1/setup`, che è
esattamente il sintomo che ci si aspetterebbe se la mia lettura della RFC è
corretta.

**Questo non è un difetto del frontend.** `apiFetch` invia sempre
`credentials: 'same-origin'`, che allega qualunque cookie il browser
possieda per l'origine corrente — si comporta esattamente come dovrebbe.
Il problema è a monte, nell'emissione del cookie lato backend
(`crates/keeppix-api`), fuori dal perimetro di questo task (non ho toccato
`crates/`). Lo segnalo qui con la massima evidenza possibile invece di
dichiarare il passo 15 "fatto" senza riserve, come richiesto dalla nota N6
del preflight.

Effetto pratico sull'accettazione dello step 15: **4 delle 5 affermazioni
del flusso a mano sono verificate vere** (redirect iniziale, creazione
admin, login con credenziali corrette, logout); **la persistenza della
sessione a un reload della pagina non lo è**, e non lo sarà finché
`should_be_secure` non verrà rivista (il fix più probabile: impostare
sempre `Secure`, anche su loopback, contando sull'eccezione di "origine
affidabile" dei browser per accettarlo comunque su HTTP — sapendo che
questo romperebbe i test che oggi leggono il cookie con `reqwest`/curl su
127.0.0.1, che quell'eccezione non ce l'hanno; è una decisione che immagino
richieda una scelta esplicita, non una correzione meccanica). Segnalo la
cosa perché venga vagliata nella prossima review/fix wave sul branch — non
l'ho corretta io perché `crates/` è fuori dal perimetro di questo task.

Ho ripulito il database di scratch (`keeppix_dev`) e fermato entrambi i
server al termine della verifica; non ho lasciato processi in background.

## Verifica di conformità ai confini (N7)

- Nessuna modifica a `crates/`, `docs/`, `Cargo.toml`, `Cargo.lock`.
- Nessuna dipendenza aggiunta oltre a quelle degli step 1 e 3 nel
  `package.json` del frontend (Playwright per la prova a mano è stato
  installato in una directory di scratch separata, mai in
  `frontend/package.json`).
- Nessuna chiamata di rete verso terze parti: font, icone e stili sono
  tutti locali/inline; la CSP del backend (`default-src 'self'` ecc.) non
  viene mai esercitata da una risorsa esterna nel codice che ho scritto.
- `package-lock.json` committato; `frontend/dist` e `node_modules` restano
  ignorati (confermato con `git status --short --ignored`).

## File modificati/creati

Tutti sotto `frontend/`, un solo commit:

- `frontend/package.json`, `package-lock.json` — scaffold + dipendenze +
  script `test`/`lint` aggiunti (non generati dallo scaffold).
- `frontend/vite.config.ts` — verbatim dal piano.
- `frontend/tsconfig.json`, `tsconfig.app.json` (alias `@/*` senza
  `baseUrl`), `tsconfig.node.json` — struttura a project-reference dello
  scaffold attuale, adattata per l'alias e per `TS5101`.
- `frontend/eslint.config.js` — nuovo, non previsto da nessuno step ma
  necessario per `npm run lint`.
- `frontend/index.html` — titolo cambiato in "Keeppix".
- `frontend/src/style.css` — verbatim (Tailwind v4 + tema chiaro/scuro).
- `frontend/src/main.ts`, `App.vue`, `router.ts` — verbatim dal piano.
- `frontend/src/api/client.ts` — verbatim salvo la riscrittura di
  `ApiProblem` (niente proprietà-parametro) e il commento corretto
  sull'header CSRF.
- `frontend/src/api/client.spec.ts` — verbatim.
- `frontend/src/api/auth.ts` — nuovo, non specificato dal piano (vedi
  scostamento 4 sopra).
- `frontend/src/i18n/it.json`, `en.json`, `index.ts` — verbatim.
- `frontend/src/i18n/i18n.spec.ts` — verbatim salvo l'alias
  `itMessages`/`enMessages` (vedi scostamento 1 sopra).
- `frontend/src/stores/session.ts` — stessa interfaccia pubblica del piano
  (`user`, `initialised`, `ready`, `bootstrap`, `login`, `setup`,
  `logout`), rifattorizzato per usare `api/auth.ts`.
- `frontend/src/components/ui/Button.vue`, `TextField.vue`, `Alert.vue` —
  verbatim (più riformattazione automatica di ESLint, nessun cambio di
  logica).
- `frontend/src/views/SetupView.vue`, `LoginView.vue`, `HomeView.vue` —
  verbatim (più la stessa riformattazione automatica).

File dello scaffold rimossi perché non nell'elenco del piano (nota N2):
`src/components/HelloWorld.vue`, `src/assets/` (hero.png, vite.svg,
vue.svg), `public/icons.svg`. Mantenuto `public/favicon.svg` (icona
locale, nessuna chiamata esterna, innocuo da tenere).

## Autoriflessione

- **Completezza**: tutti i 16 step eseguiti; le 3 rotte, lo store e
  `apiFetch` hanno l'interfaccia pubblica richiesta dal brief.
- **Qualità**: ho preferito riscritture minime e mirate (solo dove il
  toolchain corrente lo imponeva) invece di deviazioni di stile personali
  dal codice verbatim del piano.
- **Disciplina**: non ho aggiunto funzionalità non richieste (niente
  gestione multi-lingua oltre a it/en, niente componenti UI oltre ai tre
  richiesti, niente uso di `reka-ui` non necessario a questo task).
- **Test**: seguito TDD per i due file richiesti; verificato con una
  rimozione mirata che il test sulle traduzioni si rompa davvero (N4);
  eseguita la prova a mano con un browser reale invece di limitarmi a
  dichiararla ineseguibile o a curl grezzo, arrivando a una diagnosi
  precisa e verificabile del difetto trovato.
